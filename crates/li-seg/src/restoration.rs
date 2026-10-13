//! On-device Restore, FP32 ONNX through tract. No Python or external runtime in the app.
//!
//! DDColor preprocessing follows piddnad/DDColor `ddcolor/pipeline.py` at
//! 2adb63f2656ac41cbdf7b894cddd94121a3faf13 (Apache-2.0): resize RGB, Lab L to neutral
//! RGB, predict ab, nearest-resize ab and combine with the original L (D65 sRGB).
//! Scratch detection follows Microsoft's `Global/detection.py` at
//! 33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c (MIT): one-channel [-1,1], shortest
//! side 256, bilinear resize, sigmoid >= 0.4, nearest mask at the source size.
//! Export provenance and installation instructions: docs/RESTORE-MODELS.md.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, ensure};
use image::{GrayImage, RgbaImage};
use sha2::{Digest, Sha256};
use tract_onnx::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Colourise,
    Scratches,
}

/// A locally produced export of the approved official checkpoint. Imports must match both
/// length and digest. There is deliberately no unverified third-party download URL.
#[derive(Clone, Copy, Debug)]
pub struct Pin {
    pub kind: Kind,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub licence: &'static str,
    pub source: &'static str,
}
pub const DD_COLOR: Pin = Pin {
    kind: Kind::Colourise,
    file: "ddcolor-tiny.onnx",
    bytes: 220552826,
    sha256: "56e33562efde1d866a7a804b11f07c024037904dbaba85611c86a1c6fd5b5cc5",
    licence: "Apache-2.0",
    source: "https://huggingface.co/piddnad/DDColor-models/blob/e9e7b527709c8aeb2f5e1bf701e72fd468a13baa/README.md",
};
pub const SCRATCHES: Pin = Pin {
    kind: Kind::Scratches,
    file: "microsoft-scratches.onnx",
    bytes: 150600682,
    sha256: "14036ea1fa86b748973b992ab734d86c7acd3b524cab8d3a198dd63d9fa06f9b",
    licence: "MIT",
    source: "https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c/LICENSE",
};
impl Pin {
    pub fn path(self, dir: &Path) -> PathBuf {
        dir.join("restoration").join(self.file)
    }
    /// Cheap readiness check for UI paint; workers verify SHA before every inference.
    pub fn installed(self, dir: &Path) -> bool {
        std::fs::symlink_metadata(self.path(dir)).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() == self.bytes)
    }
    pub fn verify(self, path: &Path) -> Result<()> {
        let meta = std::fs::symlink_metadata(path).context("Install the exported Restore model first")?;
        ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len() == self.bytes, "Restore model is not the pinned regular file (size mismatch)");
        let mut hash = Sha256::new();
        std::io::copy(&mut std::fs::File::open(path)?, &mut hash)?;
        ensure!(format!("{:x}", hash.finalize()) == self.sha256, "Restore model SHA-256 mismatch");
        Ok(())
    }
    /// Verified, atomic local import. The selected file stays untouched. Publication happens
    /// only after the actual copied bytes pass the pin, preventing a changed source from racing.
    pub fn install(self, source: &Path, dir: &Path) -> Result<()> {
        self.install_checked(source, dir, &|| true)
    }
    pub fn install_checked(self, source: &Path, dir: &Path, keep_going: &dyn Fn() -> bool) -> Result<()> {
        check(keep_going)?;
        self.verify(source)?;
        check(keep_going)?;
        let dest = self.path(dir);
        std::fs::create_dir_all(dest.parent().context("missing model directory")?)?;
        if source == dest || self.verify(&dest).is_ok() {
            return Ok(());
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = dest.with_extension(format!("{}.{}.tmp", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let result = (|| {
            use std::io::{Read, Write};
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            let mut src = std::fs::File::open(source)?;
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                check(keep_going)?;
                let n = src.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                f.write_all(&buffer[..n])?;
            }
            f.flush()?;
            self.verify(&tmp)?;
            check(keep_going)?;
            std::fs::rename(&tmp, &dest)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

type Run = dyn Fn(Vec<f32>) -> Result<Vec<f32>> + Send + Sync;
struct Model {
    run: Box<Run>,
}
type Cache = Mutex<Option<(PathBuf, usize, usize, Arc<Model>)>>;
static COLOUR: Cache = Mutex::new(None);
static SCRATCH: Cache = Mutex::new(None);

fn model(pin: Pin, dir: &Path, w: usize, h: usize) -> Result<Arc<Model>> {
    let path = pin.path(dir);
    pin.verify(&path)?;
    let slot = if pin.kind == Kind::Colourise { &COLOUR } else { &SCRATCH };
    let mut cache = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((p, cw, ch, m)) = &*cache
        && p == &path
        && *cw == w
        && *ch == h
    {
        return Ok(m.clone());
    }
    // Free a previous shape before loading another full network.
    *cache = None;
    let channels = if pin.kind == Kind::Colourise { 3 } else { 1 };
    let out_channels = if pin.kind == Kind::Colourise { 2 } else { 1 };
    crate::init_executor();
    let plan = tract_onnx::onnx()
        .model_for_path(&path)?
        .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, channels, h, w)))?
        .into_optimized()?
        .into_runnable()?;
    let m = Arc::new(Model {
        run: Box::new(move |input| {
            ensure!(input.len() == channels * w * h && input.iter().all(|v| v.is_finite()), "invalid Restore input");
            let t = tract_ndarray::Array4::from_shape_vec((1, channels, h, w), input)?.into_tensor();
            let output = plan.run(tvec!(t.into()))?;
            let a = output.first().context("Restore model returned nothing")?.to_plain_array_view::<f32>()?;
            ensure!(a.shape() == [1, out_channels, h, w] && a.iter().all(|v| v.is_finite()), "invalid Restore output");
            Ok(a.iter().copied().collect())
        }),
    });
    *cache = Some((path, w, h, m.clone()));
    Ok(m)
}

fn check(keep_going: &dyn Fn() -> bool) -> Result<()> {
    ensure!(keep_going(), "cancelled");
    Ok(())
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
fn srgb(v: f32) -> f32 {
    if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}
fn lab_f(v: f32) -> f32 {
    if v > 216.0 / 24389.0 { v.cbrt() } else { v * (841.0 / 108.0) + 4.0 / 29.0 }
}
fn lab_inv(v: f32) -> f32 {
    if v > 6.0 / 29.0 { v * v * v } else { (v - 4.0 / 29.0) * (108.0 / 841.0) }
}
fn luminance(rgb: [f32; 3]) -> f32 {
    116.0 * lab_f(0.212671 * linear(rgb[0]) + 0.715160 * linear(rgb[1]) + 0.072169 * linear(rgb[2])) - 16.0
}
fn lab_rgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let y = (l + 16.0) / 116.0;
    let x = 0.950456 * lab_inv(y + a / 500.0);
    let z = 1.088754 * lab_inv(y - b / 200.0);
    let y = lab_inv(y);
    [3.240479 * x - 1.53715 * y - 0.498535 * z, -0.969256 * x + 1.875991 * y + 0.041556 * z, 0.055648 * x - 0.204043 * y + 1.057311 * z]
        .map(|v| srgb(v).clamp(0.0, 1.0))
}
/// Half-pixel bilinear resize, matching OpenCV INTER_LINEAR and PyTorch align_corners=false.
fn bilinear(values: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    (0..w * h)
        .map(|i| {
            let x = ((i % w) as f32 + 0.5) * sw as f32 / w as f32 - 0.5;
            let y = ((i / w) as f32 + 0.5) * sh as f32 / h as f32 - 0.5;
            let px = x.floor() as isize;
            let py = y.floor() as isize;
            let at = |x: isize, y: isize| values[y.clamp(0, sh as isize - 1) as usize * sw + x.clamp(0, sw as isize - 1) as usize];
            let tx = x - px as f32;
            let ty = y - py as f32;
            (at(px, py) * (1.0 - tx) + at(px + 1, py) * tx) * (1.0 - ty) + (at(px, py + 1) * (1.0 - tx) + at(px + 1, py + 1) * tx) * ty
        })
        .collect()
}
fn colour_input(source: &RgbaImage) -> Vec<f32> {
    let (w, h) = source.dimensions();
    let channels: Vec<Vec<f32>> =
        (0..3).map(|c| bilinear(&source.pixels().map(|p| f32::from(p[c]) / 255.0).collect::<Vec<_>>(), w as usize, h as usize, 512, 512)).collect();
    let grey: Vec<f32> =
        (0..512 * 512).map(|i| srgb(lab_inv((luminance([channels[0][i], channels[1][i], channels[2][i]]) + 16.0) / 116.0)).clamp(0.0, 1.0)).collect();
    [grey.as_slice(), grey.as_slice(), grey.as_slice()].concat()
}
fn colour_output(source: &RgbaImage, ab: &[f32]) -> RgbaImage {
    let mut result = source.clone();
    let (w, h) = source.dimensions();
    for (x, y, p) in result.enumerate_pixels_mut() {
        let i = (y as usize * 512 / h as usize) * 512 + x as usize * 512 / w as usize;
        let l = luminance([f32::from(p[0]) / 255.0, f32::from(p[1]) / 255.0, f32::from(p[2]) / 255.0]);
        let rgb = lab_rgb(l, ab[i], ab[512 * 512 + i]);
        for c in 0..3 {
            p[c] = (rgb[c] * 255.0).round() as u8;
        }
    }
    result
}
pub fn colourise(dir: &Path, source: &RgbaImage, keep_going: &dyn Fn() -> bool) -> Result<RgbaImage> {
    ensure!(source.width() > 0 && source.height() > 0, "Restore needs a non-empty image");
    check(keep_going)?;
    let m = model(DD_COLOR, dir, 512, 512)?;
    check(keep_going)?;
    let input = colour_input(source);
    check(keep_going)?;
    let ab = (m.run)(input)?;
    check(keep_going)?;
    Ok(colour_output(source, &ab))
}
/// Aspect-preserving Microsoft detector scale, rounded to a multiple of 16.
fn scratch_size(w: u32, h: u32) -> Result<(usize, usize)> {
    ensure!(w > 0 && h > 0, "Restore needs a non-empty image");
    let scale = 256.0 / f64::from(w.min(h));
    let sw = ((f64::from(w) * scale / 16.0).round_ties_even() as usize) * 16;
    let sh = ((f64::from(h) * scale / 16.0).round_ties_even() as usize) * 16;
    ensure!(sw <= 2048 && sh <= 2048, "Scratch detection supports aspect ratios up to 8:1");
    Ok((sw, sh))
}
fn scratch_input(source: &RgbaImage, w: usize, h: usize) -> Vec<f32> {
    // PIL RGB -> L fixed-point coefficients, rounded once to 8 bits.
    let grey: Vec<f32> = source
        .pixels()
        .map(|p| {
            let l = (19595 * u32::from(p[0]) + 38470 * u32::from(p[1]) + 7471 * u32::from(p[2]) + 32768) >> 16;
            l as f32 / 127.5 - 1.0
        })
        .collect();
    bilinear(&grey, source.width() as usize, source.height() as usize, w, h)
}
fn scratch_output(source: &RgbaImage, logits: &[f32], w: usize, h: usize) -> GrayImage {
    GrayImage::from_fn(source.width(), source.height(), |x, y| {
        let i = (y as usize * h / source.height() as usize) * w + x as usize * w / source.width() as usize;
        // sigmoid(x)>=0.4, using the equivalent logit threshold to avoid overflow.
        image::Luma([if logits[i] >= (0.4f32 / 0.6).ln() && source.get_pixel(x, y)[3] > 0 { 255 } else { 0 }])
    })
}
pub fn scratches(dir: &Path, source: &RgbaImage, keep_going: &dyn Fn() -> bool) -> Result<GrayImage> {
    let (w, h) = scratch_size(source.width(), source.height())?;
    check(keep_going)?;
    let m = model(SCRATCHES, dir, w, h)?;
    check(keep_going)?;
    let logits = (m.run)(scratch_input(source, w, h))?;
    check(keep_going)?;
    Ok(scratch_output(source, &logits, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restoration_lab_neutral_roundtrip_preserves_luminance_and_alpha() {
        let source = RgbaImage::from_fn(19, 13, |x, y| image::Rgba([(x * 13) as u8, (y * 19) as u8, 73, (x * y) as u8]));
        let out = colour_output(&source, &vec![0.0; 2 * 512 * 512]);
        for (a, b) in source.pixels().zip(out.pixels()) {
            assert_eq!(a[3], b[3]);
            assert!(b[0].abs_diff(b[1]) <= 1 && b[1].abs_diff(b[2]) <= 1);
            assert!((luminance([a[0], a[1], a[2]].map(|v| f32::from(v) / 255.0)) - luminance([b[0], b[1], b[2]].map(|v| f32::from(v) / 255.0))).abs() < 0.3);
        }
    }
    #[test]
    fn restoration_scratch_scale_threshold_nearest_and_transparent_mask() {
        assert_eq!(scratch_size(65, 49).unwrap(), (336, 256));
        assert!(scratch_size(0, 1).is_err());
        assert!(scratch_size(9999, 1).is_err());
        let mut source = RgbaImage::from_pixel(4, 2, image::Rgba([0, 0, 0, 255]));
        source.get_pixel_mut(3, 0)[3] = 0;
        let mask = scratch_output(&source, &[-1.0, 0.0], 2, 1);
        assert_eq!(mask.into_raw(), [0, 0, 255, 0, 0, 0, 255, 255]);
        assert_eq!(scratch_input(&source, 4, 2), vec![-1.0; 8]);
        assert_eq!(bilinear(&[0., 1., 2., 3.], 2, 2, 1, 1), vec![1.5]);
    }
    // Generated with OpenCV 5.0.0.93 float RGB/Lab conversions, independent reference.
    #[test]
    fn restoration_lab_matches_opencv_reference_vectors() {
        let source = RgbaImage::from_fn(2, 2, |x, y| {
            image::Rgba(match (x, y) {
                (0, 0) => [255, 0, 0, 255],
                (1, 0) => [0, 255, 0, 255],
                (0, 1) => [0, 0, 255, 255],
                _ => [200, 200, 200, 255],
            })
        });
        let grey = colour_input(&source);
        for (i, expected) in [
            (0, 0.49852088),
            (255, 0.4819209),
            (256, 0.48398274),
            (511, 0.86249274),
            (130815, 0.4438267),
            (131328, 0.4469348),
            (261632, 0.29781187),
            (262143, 0.7832421),
        ] {
            assert!((grey[i] - expected).abs() < 0.004, "{i}: {} vs {expected}", grey[i]);
        }
        for (lab, expected) in [
            ([10., 0., 0.], [0.10770349; 3]),
            ([50., 0., 0.], [0.4663266; 3]),
            ([53.24, 80.09, 67.2], [0.9999721, 0.000140603, 0.00000917497]),
            ([80., -40., 40.], [0.56701815, 0.84865975, 0.47426614]),
            ([60., 20., -40.], [0.5496944, 0.5335272, 0.8435656]),
        ] {
            for (a, b) in lab_rgb(lab[0], lab[1], lab[2]).into_iter().zip(expected) {
                assert!((a - b).abs() < 0.0001);
            }
        }
    }
    #[test]
    fn restoration_import_keeps_previous_model_when_same_size_bytes_are_tampered() {
        let dir = std::env::temp_dir().join(format!("li-restore-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("fixture.onnx");
        std::fs::write(&source, b"verified fixture").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"verified fixture"));
        let pin = Pin { bytes: 16, sha256: Box::leak(hash.into_boxed_str()), file: "fixture.onnx", ..DD_COLOR };
        assert!(pin.install_checked(&source, &dir, &|| false).is_err());
        assert!(!pin.path(&dir).exists());
        pin.install(&source, &dir).unwrap();
        std::fs::write(&source, b"tampered fixture").unwrap();
        assert!(pin.install(&source, &dir).is_err());
        assert_eq!(std::fs::read(pin.path(&dir)).unwrap(), b"verified fixture");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Actual app inference contracts and pin importer, on the real owner's CPU. Models and
    /// public-domain NASA test image remain in target; no owner photo is used.
    #[test]
    #[ignore = "requires approved exports and NASA astronaut fixture; see docs/RESTORE-MODELS.md"]
    fn restoration_real_cpu_colourise_and_scratch_mask() {
        let export = PathBuf::from(std::env::var("LOCAL_IMAGE_RESTORE_EXPORTS").expect("export folder"));
        let dir = export.join("installed");
        for pin in [DD_COLOR, SCRATCHES] {
            pin.install(&export.join(pin.file), &dir).unwrap();
        }
        let rgb = image::open(export.join("astronaut.png")).unwrap().to_rgba8();
        let mut source = image::imageops::resize(&rgb, 257, 193, image::imageops::FilterType::CatmullRom);
        for (x, y, p) in source.enumerate_pixels_mut() {
            let gray = ((u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3) as u8;
            *p = image::Rgba([gray, gray, gray, if x == 0 && y == 0 { 0 } else { 255 }]);
        }
        source.save(export.join("gray-input.png")).unwrap();
        for n in 0..3 {
            source.get_pixel_mut(1, 1)[0] = source.get_pixel(1, 1)[0].wrapping_add(1);
            let start = std::time::Instant::now();
            let out = colourise(&dir, &source, &|| true).unwrap();
            eprintln!("RESTORE_CPU colourise sample={n} ms={:.2}", start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(out.dimensions(), source.dimensions());
            assert!(out.pixels().any(|p| p[0].abs_diff(p[1]) > 20));
            for (a, b) in source.pixels().zip(out.pixels()) {
                assert_eq!(a[3], b[3]);
            }
            if n == 0 {
                out.save(export.join("colourised.png")).unwrap();
            }
        }
        for y in 15..180 {
            for x in 80..84 {
                *source.get_pixel_mut(x, y) = image::Rgba([245, 245, 245, 255]);
            }
        }
        source.save(export.join("damaged.png")).unwrap();
        for n in 0..3 {
            let start = std::time::Instant::now();
            let mask = scratches(&dir, &source, &|| true).unwrap();
            eprintln!(
                "RESTORE_CPU scratches sample={n} ms={:.2} pixels={}",
                start.elapsed().as_secs_f64() * 1000.0,
                mask.pixels().filter(|p| p[0] > 0).count()
            );
            assert_eq!(mask.dimensions(), source.dimensions());
            assert_eq!(mask.get_pixel(0, 0)[0], 0);
            assert!(mask.pixels().any(|p| p[0] > 0));
            if n == 0 {
                mask.save(export.join("scratches-mask.png")).unwrap();
            }
        }
    }
    #[test]
    fn restoration_pin_rejects_tampering_and_cancel_before_loading() {
        let dir = std::env::temp_dir().join(format!("li-restoration-pin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("bad.onnx");
        std::fs::write(&source, b"not a model").unwrap();
        assert!(DD_COLOR.install(&source, &dir).is_err());
        assert!(!DD_COLOR.path(&dir).exists());
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        assert!(colourise(&dir, &image, &|| false).unwrap_err().to_string().contains("cancelled"));
        assert!(scratches(&dir, &image, &|| false).unwrap_err().to_string().contains("cancelled"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
