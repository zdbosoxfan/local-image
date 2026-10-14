//! Big-LaMa's Apache-2.0 Carve export, unmodified FP32 ONNX through tract.
//! Contract: NCHW RGB / 255, binary mask (1 = remove), output NCHW RGB in 0..255.
//! https://huggingface.co/Carve/LaMa-ONNX/tree/c3c0c9e468934d62e79c329e35d82dd09ff8c444
//! Large connected holes are resized as one context crop, preserving aspect and global
//! structure. Distant holes get separate crops; no hole is cut across a tile boundary.
use anyhow::{Context, Result, bail, ensure};
use image::{GrayImage, Luma, Rgb, RgbImage, imageops};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tract_onnx::prelude::*;

pub const MODEL_ID: &str = "big-lama";
pub const SIDE: u32 = 512;
pub const MODEL: crate::ModelSpec = crate::ModelSpec {
    id: MODEL_ID,
    label: "LaMa · On-device",
    file: "big-lama-fp32.onnx",
    companions: &[],
    bytes: 208044816,
    sha256: "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6",
    url: "https://huggingface.co/Carve/LaMa-ONNX/resolve/c3c0c9e468934d62e79c329e35d82dd09ff8c444/lama_fp32.onnx",
    size: SIDE as usize,
    isnet: false,
    licence: "Apache-2.0 (Samsung Research big-LaMa; Carve ONNX export)",
    task: crate::Task::Inpaint,
    group: Some(crate::Group::Remove),
    about: "Repairs brushed areas locally using the surrounding texture and structure. Runs on the CPU, including Macs; no GPU or AI server needed. Large areas use a resized context crop.",
};

type Cache = Option<(std::path::PathBuf, u64, Option<std::time::SystemTime>, Lama)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

/// Release the cached weights after model removal. An in-flight worker keeps its own Arc.
pub(crate) fn forget() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = None;
    }
}

type Predict = dyn Fn(&RgbImage, &GrayImage, &dyn Fn() -> Result<()>) -> Result<RgbImage> + Send + Sync;
#[derive(Clone)]
pub struct Lama {
    predict: Arc<Predict>,
}

impl Lama {
    /// Compile an ONNX export with the documented 512-pixel contract. Used by reference tools.
    pub fn from_path(path: &Path) -> Result<Self> {
        crate::init_executor();
        let plan = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_path(path)?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, 512, 512)))?
            .with_input_fact(1, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 1, 512, 512)))?
            .into_optimized()?
            .into_runnable()?;
        // One inference at a time per cached plan bounds memory on any host, even with
        // simultaneous Develop/Compositing requests. Waiting remains cancellable.
        let lane = Mutex::new(());
        Ok(Self {
            predict: Arc::new(move |rgb, mask, check| {
                let _lane = loop {
                    check()?;
                    match lane.try_lock() {
                        Ok(guard) => break guard,
                        Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(20)),
                        Err(_) => bail!("LaMa inference lock was poisoned"),
                    }
                };
                let image = tract_ndarray::Array4::from_shape_fn((1, 3, 512, 512), |(_, c, y, x)| rgb.get_pixel(x as u32, y as u32)[c] as f32 / 255.0);
                let mask = tract_ndarray::Array4::from_shape_fn((1, 1, 512, 512), |(_, _, y, x)| f32::from(mask.get_pixel(x as u32, y as u32)[0] > 0));
                let out = plan.spawn()?.run_plan_with_eval(tvec!(image.into_tensor().into(), mask.into_tensor().into()), |context, state, node, inputs| {
                    check()?;
                    tract_onnx::tract_hir::tract_core::plan::eval(context, state, node, inputs)
                })?;
                let a = out[0].to_plain_array_view::<f32>()?;
                ensure!(a.shape() == [1, 3, 512, 512], "Unexpected LaMa output shape: {:?}", a.shape());
                ensure!(a.iter().all(|v| v.is_finite()), "LaMa returned non-finite pixels");
                Ok(RgbImage::from_fn(SIDE, SIDE, |x, y| Rgb([0, 1, 2].map(|c| a[[0, c, y as usize, x as usize]].round().clamp(0.0, 255.0) as u8))))
            }),
        })
    }

    /// Returns a complete repaired image. Original pixels outside the supplied coverage
    /// are byte-identical. Progress counts context crops; cancellation publishes nothing.
    pub fn run(&self, image: &RgbImage, mask: &GrayImage, check: &dyn Fn() -> Result<()>, progress: &dyn Fn(usize, usize)) -> Result<RgbImage> {
        self.run_impl(image, mask, check, progress, true)
    }

    /// Unblended patch pixels for the app's existing layer/patch alpha compositor.
    pub fn run_patch(&self, image: &RgbImage, mask: &GrayImage, check: &dyn Fn() -> Result<()>, progress: &dyn Fn(usize, usize)) -> Result<RgbImage> {
        self.run_impl(image, mask, check, progress, false)
    }

    fn run_impl(&self, image: &RgbImage, mask: &GrayImage, check: &dyn Fn() -> Result<()>, progress: &dyn Fn(usize, usize), blend: bool) -> Result<RgbImage> {
        check()?;
        ensure!(image.dimensions() == mask.dimensions() && image.width() > 0 && image.height() > 0, "Image and mask sizes differ or are empty");
        let regions = regions(mask, check)?;
        ensure!(!regions.is_empty(), "Paint over what you want to remove first.");
        let mut out = image.clone();
        for (index, region) in regions.iter().enumerate() {
            check()?;
            progress(index, regions.len());
            let [x0, y0, x1, y1] = context(*region, image.width(), image.height());
            let (w, h) = (x1 - x0, y1 - y0);
            let rgb = imageops::crop_imm(&out, x0, y0, w, h).to_image();
            // Include only this region's holes, so neighbouring context crops cannot repair
            // another region twice. Earlier repairs provide context for later regions.
            let local = GrayImage::from_fn(w, h, |x, y| {
                let (xx, yy) = (x + x0, y + y0);
                Luma([if xx >= region[0] && xx < region[2] && yy >= region[1] && yy < region[3] { mask.get_pixel(xx, yy)[0] } else { 0 }])
            });
            let (input, hard, size, offset) = prepare(&rgb, &local);
            let predicted = (self.predict)(&input, &hard, check)?;
            check()?;
            let repaired = imageops::crop_imm(&predicted, offset[0], offset[1], size[0], size[1]).to_image();
            let repaired = if repaired.dimensions() == (w, h) { repaired } else { imageops::resize(&repaired, w, h, imageops::FilterType::Lanczos3) };
            for y in region[1]..region[3] {
                check()?;
                for x in region[0]..region[2] {
                    let coverage = mask.get_pixel(x, y)[0];
                    let alpha = if !blend && coverage > 0 { 255 } else { coverage as u32 };
                    if alpha > 0 {
                        let src = image.get_pixel(x, y);
                        let fill = repaired.get_pixel(x - x0, y - y0);
                        out.put_pixel(x, y, Rgb([0, 1, 2].map(|c| ((src[c] as u32 * (255 - alpha) + fill[c] as u32 * alpha + 127) / 255) as u8)));
                    }
                }
            }
            progress(index + 1, regions.len());
        }
        check()?;
        Ok(out)
    }
}

type Region = [u32; 4];
fn bounds(mask: &GrayImage, r: Region) -> Option<Region> {
    let mut b = [r[2], r[3], r[0], r[1]];
    for y in r[1]..r[3] {
        for x in r[0]..r[2] {
            if mask.get_pixel(x, y)[0] > 0 {
                b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
            }
        }
    }
    (b[0] < b[2] && b[1] < b[3]).then_some(b)
}
fn regions(mask: &GrayImage, check: &dyn Fn() -> Result<()>) -> Result<Vec<Region>> {
    let Some(b) = bounds(mask, [0, 0, mask.width(), mask.height()]) else { return Ok(vec![]) };
    let mut pending = vec![b];
    let mut done = vec![];
    while let Some(r) = pending.pop() {
        check()?;
        let mut split = None;
        // Empty bands separate holes while keeping each connected shape coherent. Stop
        // subdividing at 32 regions, retaining every remaining hole in a larger crop.
        if done.len() + pending.len() < 31 {
            for axis in 0..2 {
                let mut start = None;
                for a in r[axis]..r[axis + 2] {
                    let used = (r[1 - axis]..r[3 - axis]).any(|b| if axis == 0 { mask.get_pixel(a, b)[0] > 0 } else { mask.get_pixel(b, a)[0] > 0 });
                    if used {
                        if let Some(s) = start.take()
                            && a - s >= 64
                        {
                            split = Some((axis, s, a));
                            break;
                        }
                    } else {
                        start.get_or_insert(a);
                    }
                }
                if split.is_some() {
                    break;
                }
            }
        }
        if let Some((axis, a, b)) = split {
            let mut left = r;
            left[axis + 2] = a;
            let mut right = r;
            right[axis] = b;
            pending.extend([left, right].into_iter().filter_map(|r| bounds(mask, r)));
        } else {
            done.push(r);
        }
    }
    done.sort_by_key(|r| (r[1], r[0]));
    Ok(done)
}
fn context(r: Region, w: u32, h: u32) -> Region {
    let side = (r[2] - r[0]).max(r[3] - r[1]).saturating_mul(2).saturating_add(128).max(SIDE);
    let (cw, ch) = (side.min(w), side.min(h));
    let x = ((r[0] as i64 + r[2] as i64 - cw as i64) / 2).clamp(0, (w - cw) as i64) as u32;
    let y = ((r[1] as i64 + r[3] as i64 - ch as i64) / 2).clamp(0, (h - ch) as i64) as u32;
    [x, y, x + cw, y + ch]
}
fn reflect(i: i64, n: u32) -> u32 {
    if n <= 1 {
        return 0;
    }
    let period = 2 * (n as i64 - 1);
    let i = i.rem_euclid(period);
    i.min(period - i) as u32
}
/// Preserve aspect, never upscale small crops, symmetrically reflect-pad to the model.
/// Conservative footprint pooling prevents thin masks from disappearing when downsampling.
fn prepare(rgb: &RgbImage, mask: &GrayImage) -> (RgbImage, GrayImage, [u32; 2], [u32; 2]) {
    let (w, h) = rgb.dimensions();
    let k = (SIDE as f64 / w.max(h) as f64).min(1.0);
    let size = [(w as f64 * k).round().max(1.0) as u32, (h as f64 * k).round().max(1.0) as u32];
    let offset = [(SIDE - size[0]) / 2, (SIDE - size[1]) / 2];
    let resized = if size == [w, h] { rgb.clone() } else { imageops::resize(rgb, size[0], size[1], imageops::FilterType::Lanczos3) };
    let small = GrayImage::from_fn(size[0], size[1], |x, y| {
        let xa = x as u64 * w as u64 / size[0] as u64;
        let xb = ((x + 1) as u64 * w as u64).div_ceil(size[0] as u64);
        let ya = y as u64 * h as u64 / size[1] as u64;
        let yb = ((y + 1) as u64 * h as u64).div_ceil(size[1] as u64);
        let hit = (ya as u32..yb as u32).any(|yy| (xa as u32..xb as u32).any(|xx| mask.get_pixel(xx, yy)[0] > 0));
        Luma([if hit { 255 } else { 0 }])
    });
    let input =
        RgbImage::from_fn(SIDE, SIDE, |x, y| *resized.get_pixel(reflect(x as i64 - offset[0] as i64, size[0]), reflect(y as i64 - offset[1] as i64, size[1])));
    let hard =
        GrayImage::from_fn(SIDE, SIDE, |x, y| *small.get_pixel(reflect(x as i64 - offset[0] as i64, size[0]), reflect(y as i64 - offset[1] as i64, size[1])));
    (input, hard, size, offset)
}

/// Metadata-only availability for UI; integrity verification and compilation run on workers.
pub fn installed(dir: &Path) -> bool {
    crate::installed_bytes(dir, &MODEL).is_some()
}
/// Verified cache invalidates when download/removal/replacement changes the file stamp.
pub fn shared(spec: &crate::ModelSpec, dir: &Path, check: &dyn Fn() -> Result<()>) -> Result<Lama> {
    check()?;
    ensure!(spec.task == crate::Task::Inpaint && spec.size == SIDE as usize, "LaMa requires a 512-pixel inpaint descriptor");
    let path = crate::model_path(dir, spec);
    let meta = std::fs::metadata(&path).context("Download AI Remove in Preferences › AI › On-device Tools first.")?;
    ensure!(crate::bundle_bytes(dir, spec, true).is_some(), "LaMa model size or SHA-256 mismatch. Download AI Remove again.");
    let mut slot = CACHE.lock().map_err(|_| anyhow::anyhow!("LaMa cache lock was poisoned"))?;
    check()?;
    if let Some((p, len, modified, model)) = &*slot
        && *p == path
        && *len == meta.len()
        && *modified == meta.modified().ok()
    {
        return Ok(model.clone());
    }
    let model = Lama::from_path(&path)?;
    check()?;
    *slot = Some((path, meta.len(), meta.modified().ok(), model.clone()));
    Ok(model)
}

#[cfg(test)]
mod tests;
