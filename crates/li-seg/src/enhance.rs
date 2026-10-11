//! Fixed-shape, bounded-memory super-resolution. NCHW sRGB floats, no normalisation.
//! Context is discarded before feathering overlapping cores, following darktable's neural
//! restore and ComfyUI's tiled upscale conventions. The ONNX exports remain unmodified.
use std::{io::Cursor, path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage};
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::internal::DimLike;

type Predict = dyn Fn(&RgbImage, &dyn Fn() -> Result<()>) -> Result<Vec<[f32; 3]>> + Send + Sync;

#[derive(Clone)]
pub struct Upscaler {
    pub tile: usize,
    pub scale: usize,
    halo: usize,
    predict: Arc<Predict>,
}

impl Upscaler {
    pub fn from_custom_onnx(bytes: &[u8]) -> Result<Self> {
        use tract_onnx::tract_hir::infer::Factoid;
        let model = tract_onnx::onnx().model_for_read(&mut Cursor::new(bytes))?;
        let shape = model.input_fact(0)?.shape.concretize().context("Custom model needs a fixed NCHW input shape")?;
        let shape: Vec<usize> = shape.iter().map(|d| d.to_usize()).collect::<Result<_, _>>()?;
        if shape.len() != 4 || shape[0] != 1 || shape[1] != 3 || shape[2] != shape[3] || !(16..=1024).contains(&shape[2]) {
            bail!("Custom restoration input must be 1 × 3 × tile × tile (16–1024 pixels)");
        }
        let typed = model.into_typed()?;
        let output = typed.output_fact(0)?.shape.as_concrete().context("Custom model needs a fixed output shape")?;
        if output.len() != 4 || output[..2] != [1, 3] || output[2] != output[3] || !output[2].is_multiple_of(shape[2]) {
            bail!("Custom restoration output must be NCHW RGB at 2× or 4×");
        }
        let scale = output[2] / shape[2];
        if !matches!(scale, 2 | 4) {
            bail!("Custom restoration output must be 2× or 4×");
        }
        Self::from_onnx(bytes, shape[2], scale)
    }

    pub fn load(spec: &crate::ModelSpec, models_dir: &Path) -> Result<Self> {
        if spec.task != crate::Task::Upscale || crate::installed_bytes(models_dir, spec).is_none() {
            bail!("Download {} in Settings › Local AI first.", spec.label);
        }
        let scale = if spec.id.ends_with("2x") { 2 } else { 4 };
        let bytes = std::fs::read(crate::model_path(models_dir, spec))?;
        let bytes = if spec.file.ends_with(".dtmodel") { archive_model(&bytes, &format!("model_x{scale}.onnx"))? } else { bytes };
        let mut model = Self::from_onnx(&bytes, spec.size, scale)?;
        // RRDB 4× has a much longer receptive field. The ordinary 64-pixel context
        // produced 28-level edge differences; 112 pixels measured at most one level.
        if spec.id == "bsrgan-4x" {
            model.halo = spec.size * 7 / 16;
        }
        Ok(model)
    }

    pub fn from_onnx(bytes: &[u8], tile: usize, scale: usize) -> Result<Self> {
        let plan = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_read(&mut Cursor::new(bytes))?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, tile, tile)))?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self {
            tile,
            scale,
            halo: tile / 4,
            predict: Arc::new(move |input, check| {
                let tensor = tract_ndarray::Array4::from_shape_fn((1, 3, tile, tile), |(_, c, y, x)| input.get_pixel(x as u32, y as u32)[c] as f32 / 255.0);
                let output = plan.spawn()?.run_plan_with_eval(tvec!(Tensor::from(tensor).into()), |context, state, node, inputs| {
                    check()?;
                    tract_onnx::tract_hir::tract_core::plan::eval(context, state, node, inputs)
                })?;
                let values = output[0].to_plain_array_view::<f32>()?;
                let side = tile * scale;
                if values.shape() != [1, 3, side, side] {
                    bail!("Unexpected upscale output shape: {:?}", values.shape());
                }
                let a = values.as_slice().context("Non-contiguous upscale output")?;
                Ok((0..side * side).map(|i| [a[i], a[side * side + i], a[2 * side * side + i]]).collect())
            }),
        })
    }

    /// Progress reports completed / total tiles. `check` is called before allocation, before
    /// and after each inference, and while blending, so cancellation never publishes a partial.
    pub fn run(&self, input: &RgbImage, check: &dyn Fn() -> Result<()>, progress: &dyn Fn(usize, usize)) -> Result<RgbImage> {
        tiled(input, self.tile, self.scale, self.halo, &*self.predict, check, progress)
    }

    /// A single inference, with the same context/padding as tiling. Intended for small-image
    /// reference comparisons; rejects images that cannot fit rather than silently resizing.
    pub fn whole(&self, input: &RgbImage, check: &dyn Fn() -> Result<()>) -> Result<RgbImage> {
        let halo = self.halo;
        if input.width() == 0 || input.height() == 0 || input.width() as usize + halo > self.tile || input.height() as usize + halo > self.tile {
            bail!("Image is too large for whole-image reference inference");
        }
        check()?;
        let patch = RgbImage::from_fn(self.tile as u32, self.tile as u32, |x, y| {
            let sx = (x as i32 - halo as i32).clamp(0, input.width() as i32 - 1);
            let sy = (y as i32 - halo as i32).clamp(0, input.height() as i32 - 1);
            *input.get_pixel(sx as u32, sy as u32)
        });
        let values = (self.predict)(&patch, check)?;
        check()?;
        let side = self.tile * self.scale;
        Ok(RgbImage::from_fn(input.width() * self.scale as u32, input.height() * self.scale as u32, |x, y| {
            let p = values[(y as usize + halo * self.scale) * side + x as usize + halo * self.scale];
            Rgb(p.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8))
        }))
    }
}

fn origins(length: usize, core: usize, step: usize) -> Vec<usize> {
    if length <= core {
        return vec![0];
    }
    let mut out: Vec<_> = (0..=length - core).step_by(step).collect();
    if out.last() != Some(&(length - core)) {
        out.push(length - core);
    }
    out
}

fn feather(p: usize, length: usize, before: usize, after: usize) -> f32 {
    let left = if before > 0 { ((p + 1) as f32 / (before + 1) as f32).min(1.0) } else { 1.0 };
    let right = if after > 0 { ((length - p) as f32 / (after + 1) as f32).min(1.0) } else { 1.0 };
    left * right
}

fn overlaps(origins: &[usize], index: usize, core: usize, scale: usize) -> [usize; 2] {
    let origin = origins[index];
    [
        if index > 0 { (origins[index - 1] + core - origin) * scale } else { 0 },
        if index + 1 < origins.len() { (origin + core - origins[index + 1]) * scale } else { 0 },
    ]
}

fn tiled(
    input: &RgbImage,
    tile: usize,
    scale: usize,
    halo: usize,
    predict: &Predict,
    check: &dyn Fn() -> Result<()>,
    progress: &dyn Fn(usize, usize),
) -> Result<RgbImage> {
    check()?;
    if tile < 16 || halo == 0 || halo * 2 >= tile || !matches!(scale, 2 | 4) || input.width() == 0 || input.height() == 0 {
        bail!("Invalid upscale dimensions");
    }
    let (w, h) = (input.width() as usize, input.height() as usize);
    let (ow, oh) = (w.checked_mul(scale).context("Upscale width overflow")?, h.checked_mul(scale).context("Upscale height overflow")?);
    let pixels = ow.checked_mul(oh).filter(|n| *n <= 400_000_000).context("Upscale exceeds 400 megapixels")?;
    let core = tile - 2 * halo;
    let overlap = core / 4;
    let xs = origins(w, core, core - overlap);
    let ys = origins(h, core, core - overlap);
    let total = xs.len() * ys.len();
    // Only tile rows not yet completed need floating-point accumulators.
    let band_rows = core.min(h) * scale;
    let mut sums = Vec::new();
    sums.try_reserve_exact(ow * band_rows)?;
    sums.resize(ow * band_rows, [0.0f32; 4]);
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(pixels * 3)?;
    bytes.resize(pixels * 3, 0);
    let mut output = RgbImage::from_raw(ow as u32, oh as u32, bytes).context("Invalid upscale output dimensions")?;
    let mut flushed = 0;
    let mut done = 0;
    progress(done, total);
    for (yi, &y) in ys.iter().enumerate() {
        let [top, bottom] = overlaps(&ys, yi, core, scale);
        for (xi, &x) in xs.iter().enumerate() {
            let [left, right] = overlaps(&xs, xi, core, scale);
            check()?;
            let patch = RgbImage::from_fn(tile as u32, tile as u32, |px, py| {
                let ix = (x as isize + px as isize - halo as isize).clamp(0, w as isize - 1);
                let iy = (y as isize + py as isize - halo as isize).clamp(0, h as isize - 1);
                *input.get_pixel(ix as u32, iy as u32)
            });
            let values = predict(&patch, check)?;
            check()?;
            let side = tile * scale;
            if values.len() != side * side || values.iter().flatten().any(|v| !v.is_finite()) {
                bail!("Invalid upscale model output");
            }
            let (cw, ch) = (core.min(w - x) * scale, core.min(h - y) * scale);
            for py in 0..ch {
                if py.is_multiple_of(32) {
                    check()?;
                }
                let wy = feather(py, ch, top, bottom);
                for px in 0..cw {
                    let weight = wy * feather(px, cw, left, right);
                    let value = values[(py + halo * scale) * side + px + halo * scale];
                    let sum = &mut sums[((y * scale + py) % band_rows) * ow + x * scale + px];
                    for c in 0..3 {
                        sum[c] += value[c] * weight;
                    }
                    sum[3] += weight;
                }
            }
            done += 1;
            progress(done, total);
        }
        let end = ys.get(yi + 1).map_or(oh, |y| y * scale);
        for row in flushed..end {
            if row.is_multiple_of(32) {
                check()?;
            }
            for col in 0..ow {
                let sum = &mut sums[(row % band_rows) * ow + col];
                output.put_pixel(col as u32, row as u32, Rgb(std::array::from_fn(|c| (sum[c] / sum[3] * 255.0).round().clamp(0.0, 255.0) as u8)));
                *sum = [0.0; 4];
            }
        }
        flushed = end;
    }
    check()?;
    Ok(output)
}

/// The publisher's .dtmodel format is a ZIP container. Only the named ONNX member is read;
/// filenames are never used as filesystem paths. Stored and raw-DEFLATE entries are supported.
pub fn archive_model(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let u16_at = |p: usize| -> Result<u16> { Ok(u16::from_le_bytes(bytes.get(p..p + 2).context("Truncated model archive")?.try_into()?)) };
    let u32_at = |p: usize| -> Result<u32> { Ok(u32::from_le_bytes(bytes.get(p..p + 4).context("Truncated model archive")?.try_into()?)) };
    let mut p = 0;
    while bytes.get(p..p + 4) == Some(b"PK\x03\x04") {
        let flags = u16_at(p + 6)?;
        if flags & 9 != 0 {
            bail!("Unsupported model archive flags");
        }
        let method = u16_at(p + 8)?;
        let compressed = u32_at(p + 18)? as usize;
        let size = u32_at(p + 22)? as usize;
        let n = u16_at(p + 26)? as usize;
        let extra = u16_at(p + 28)? as usize;
        let file = std::str::from_utf8(bytes.get(p + 30..p + 30 + n).context("Truncated archive name")?)?;
        let start = p + 30 + n + extra;
        let data = bytes.get(start..start + compressed).context("Truncated archive member")?;
        if file.rsplit('/').next() == Some(name) {
            if size > 200_000_000 {
                bail!("Model archive member too large");
            }
            let out = match method {
                0 => data.to_vec(),
                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size).map_err(|e| anyhow::anyhow!("Model archive inflate: {e:?}"))?,
                _ => bail!("Unsupported model archive compression"),
            };
            if out.len() != size {
                bail!("Model archive size mismatch");
            }
            return Ok(out);
        }
        p = start + compressed;
    }
    bail!("Model archive does not contain {name}")
}

#[cfg(test)]
mod tests {

    #[test]
    fn ds2_final_tile_uses_actual_overlap_and_flushed_bands_match_whole() {
        let starts = origins(29, 16, 12);
        assert_eq!(starts, [0, 12, 13]);
        assert_eq!(overlaps(&starts, 2, 16, 2), [30, 0]);
        assert!((feather(10, 32, 30, 0) - 11.0 / 31.0).abs() < 1e-6);
        let image = RgbImage::from_fn(29, 41, |x, y| Rgb([(x * 5) as u8, (y * 5) as u8, 100]));
        for scale in [2, 4] {
            let output = tiled(&image, 32, scale, 8, &*nearest(scale), &|| Ok(()), &|_, _| {}).unwrap();
            assert_eq!(output, image::imageops::resize(&image, output.width(), output.height(), image::imageops::FilterType::Nearest));
        }
    }

    use super::*;
    fn nearest(scale: usize) -> Arc<Predict> {
        Arc::new(move |img, _| {
            Ok((0..img.height() as usize * scale)
                .flat_map(|y| (0..img.width() as usize * scale).map(move |x| img.get_pixel((x / scale) as u32, (y / scale) as u32).0.map(|v| v as f32 / 255.0)))
                .collect())
        })
    }
    #[test]
    fn enhance_tiled_matches_whole_without_seams_and_sizes() {
        let input = RgbImage::from_fn(73, 49, |x, y| Rgb([(x * 3) as u8, (y * 5) as u8, (x + y) as u8]));
        for scale in [2, 4] {
            let output = tiled(&input, 32, scale, 8, &*nearest(scale), &|| Ok(()), &|_, _| {}).unwrap();
            assert_eq!(output.dimensions(), (73 * scale as u32, 49 * scale as u32));
            assert_eq!(output, image::imageops::resize(&input, output.width(), output.height(), image::imageops::FilterType::Nearest));
        }
    }
    #[test]
    fn enhance_overlap_with_context_matches_whole_convolution() {
        // A spatial model, unlike nearest-neighbour, exposes both missing context and seams.
        let smooth = |image: &RgbImage| -> Vec<[f32; 3]> {
            (0..image.height() * 2)
                .flat_map(|y| {
                    (0..image.width() * 2).map(move |x| {
                        let mut sum = [0.0; 3];
                        for dy in -2..=2 {
                            for dx in -2..=2 {
                                let sx = (x as i32 / 2 + dx).clamp(0, image.width() as i32 - 1) as u32;
                                let sy = (y as i32 / 2 + dy).clamp(0, image.height() as i32 - 1) as u32;
                                for c in 0..3 {
                                    sum[c] += image.get_pixel(sx, sy)[c] as f32 / (25.0 * 255.0);
                                }
                            }
                        }
                        sum
                    })
                })
                .collect()
        };
        let input = RgbImage::from_fn(73, 49, |x, y| Rgb([(x * y % 255) as u8, (x * 7 % 255) as u8, (y * 9 % 255) as u8]));
        let whole = smooth(&input);
        let output = tiled(&input, 32, 2, 8, &move |image, _| Ok(smooth(image)), &|| Ok(()), &|_, _| {}).unwrap();
        for (p, expected) in output.pixels().zip(whole) {
            for c in 0..3 {
                assert!((p[c] as f32 - expected[c] * 255.0).abs() <= 0.501);
            }
        }
    }
    #[test]
    fn enhance_cancellation_during_tiles_discards_output() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let check = || {
            if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 8 {
                bail!("Cancelled");
            }
            Ok(())
        };
        assert!(tiled(&RgbImage::new(80, 50), 32, 2, 8, &*nearest(2), &check, &|_, _| {}).unwrap_err().to_string().contains("Cancelled"));
    }
    #[test]
    fn enhance_invalid_outputs_and_archives_are_rejected() {
        assert!(tiled(&RgbImage::new(1, 1), 32, 2, 8, &|_, _| Ok(vec![]), &|| Ok(()), &|_, _| {}).is_err());
        assert!(archive_model(b"PK\x03\x04", "model_x2.onnx").is_err());
    }

    #[test]
    #[ignore = "Opt-in: verified real ONNX models under LOCAL_IMAGE_TEST_MODELS"]
    fn enhance_real_tiled_vs_whole() {
        let root = std::path::PathBuf::from(std::env::var("LOCAL_IMAGE_TEST_MODELS").unwrap());
        for id in ["realplksr-2x", "realplksr-4x", "bsrgan-2x", "bsrgan-4x"] {
            if std::env::var("ENHANCE_CANDIDATE").is_ok_and(|candidate| candidate != id) {
                continue;
            }
            let spec = crate::spec(id).unwrap();
            let model = Upscaler::load(spec, &root).unwrap();
            let core = model.tile - 2 * model.halo;
            let width = (core + 12) as u32; // crosses a tile core
            let height = (model.tile / 3).min(core / 2) as u32;
            let input = RgbImage::from_fn(width, height, |x, y| {
                Rgb([(80.0 + (x as f32 * 0.1).sin() * 40.0) as u8, (90.0 + (y as f32 * 0.09).cos() * 30.0) as u8, 120])
            });
            let whole = model.whole(&input, &|| Ok(())).unwrap();
            let tiled = model.run(&input, &|| Ok(()), &|_, _| {}).unwrap();
            if let Ok(dir) = std::env::var("ENHANCE_OUTPUT") {
                std::fs::create_dir_all(&dir).unwrap();
                whole.save(std::path::Path::new(&dir).join(format!("{id}-whole.png"))).unwrap();
                tiled.save(std::path::Path::new(&dir).join(format!("{id}-tiled.png"))).unwrap();
            }
            let error: Vec<_> = whole.pixels().zip(tiled.pixels()).flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c]) as f64)).collect();
            let mse = error.iter().map(|v| v * v).sum::<f64>() / error.len() as f64;
            let max = error.into_iter().fold(0.0, f64::max);
            eprintln!("{id} tiled/whole RMSE {:.4} levels, max {max}", mse.sqrt());
            assert!(mse.sqrt() < 1.0 && max <= 6.0, "{id}: visible seam/reference discrepancy");
        }
    }
}
