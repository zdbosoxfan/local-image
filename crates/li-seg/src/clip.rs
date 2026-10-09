//! CPU image/text embeddings for Smart Sort. The paired ONNX plans share safely across workers;
//! only crops and tensors are per-call. Model-family details stay here rather than in the engine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid;

use crate::{ModelSpec, Runner, Task, bpe::Bpe, companion_path, installed_bytes, model_path, spec};

const SIDE: usize = 224;
const DIM: usize = 512;
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Crops {
    Center,
    #[default]
    Three,
}

pub struct Clip {
    visual: Arc<Runner>,
    textual: Arc<Runner>,
    bpe: Bpe,
    text_i32: bool,
}

impl Clip {
    pub fn load(models_dir: &Path, spec: &ModelSpec) -> Result<Self> {
        if spec.task != Task::ImageText || spec.companions.len() != 3 {
            bail!("not a CLIP bundle");
        }
        if installed_bytes(models_dir, spec).is_none() {
            bail!("CLIP model bundle is incomplete");
        }
        let visual = tract_onnx::onnx()
            .model_for_path(model_path(models_dir, spec))?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, SIDE, SIDE)))?
            .into_optimized()?
            .into_runnable()?;
        let textual = tract_onnx::onnx().model_for_path(companion_path(models_dir, spec.companions[0].file))?;
        let dt = textual.input_fact(0)?.datum_type.concretize().context("CLIP text input type")?;
        if dt != i32::datum_type() && dt != i64::datum_type() {
            bail!("unsupported CLIP text input type: {dt:?}");
        }
        let text_i32 = dt == i32::datum_type();
        let textual = textual.with_input_fact(0, InferenceFact::dt_shape(dt, tvec!(1, 77)))?.into_optimized()?.into_runnable()?;
        let bpe = Bpe::load(&companion_path(models_dir, spec.companions[1].file), &companion_path(models_dir, spec.companions[2].file))?;
        Ok(Self { visual: Arc::new(move |t| visual.run(tvec!(t.into()))), textual: Arc::new(move |t| textual.run(tvec!(t.into()))), bpe, text_i32 })
    }

    pub fn dim(&self) -> usize {
        DIM
    }

    pub fn embed_image(&self, rgba: &[u8], w: usize, h: usize, crops: Crops) -> Result<Vec<f32>> {
        let img = resize(rgba, w, h).context("CLIP image buffer size mismatch")?;
        let mut mean = vec![0.0; DIM];
        for (x, y, _) in crop_rects(img.width() as usize, img.height() as usize, crops) {
            let values = nchw(&img, x, y);
            let input = tract_ndarray::Array4::from_shape_vec((1, 3, SIDE, SIDE), values)?;
            let v = embedding((self.visual)(input.into())?)?;
            for (m, v) in mean.iter_mut().zip(v) {
                *m += v;
            }
        }
        normalize(&mut mean)?;
        Ok(mean)
    }

    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let ids = self.bpe.encode(text, 77);
        let input: Tensor = if self.text_i32 {
            tract_ndarray::Array2::from_shape_vec((1, 77), ids.iter().map(|v| *v as i32).collect())?.into()
        } else {
            tract_ndarray::Array2::from_shape_vec((1, 77), ids.iter().map(|v| i64::from(*v)).collect())?.into()
        };
        embedding((self.textual)(input)?)
    }
}

fn embedding(out: TVec<TValue>) -> Result<Vec<f32>> {
    let mut v: Vec<f32> = out.first().context("CLIP returned no embedding")?.to_plain_array_view::<f32>()?.iter().copied().collect();
    if v.len() != DIM {
        bail!("unexpected CLIP embedding dimension: {}", v.len());
    }
    normalize(&mut v)?;
    Ok(v)
}

fn normalize(v: &mut [f32]) -> Result<()> {
    let norm = v.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        bail!("invalid CLIP embedding");
    }
    for v in v {
        *v /= norm;
    }
    Ok(())
}

fn resize(rgba: &[u8], w: usize, h: usize) -> Option<image::RgbaImage> {
    if w == 0 || h == 0 || rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    let input = image::RgbaImage::from_raw(u32::try_from(w).ok()?, u32::try_from(h).ok()?, rgba.to_vec())?;
    let short = w.min(h);
    let rw = w.checked_mul(SIDE)? / short;
    let rh = h.checked_mul(SIDE)? / short;
    // Alpha is deliberately ignored: it must not influence RGB interpolation.
    let input = image::RgbImage::from_fn(input.width(), input.height(), |x, y| {
        let p = input.get_pixel(x, y);
        image::Rgb([p[0], p[1], p[2]])
    });
    let rgb = image::imageops::resize(&input, u32::try_from(rw).ok()?, u32::try_from(rh).ok()?, FilterType::CatmullRom);
    Some(image::RgbaImage::from_fn(rgb.width(), rgb.height(), |x, y| {
        let p = rgb.get_pixel(x, y);
        image::Rgba([p[0], p[1], p[2], 255])
    }))
}

/// Squares in the resized image; a square input is embedded only once.
pub fn crop_rects(w: usize, h: usize, crops: Crops) -> Vec<(usize, usize, usize)> {
    if w < SIDE || h < SIDE {
        return Vec::new();
    }
    let (dx, dy) = (w - SIDE, h - SIDE);
    if crops == Crops::Center || dx == dy {
        return vec![(dx / 2, dy / 2, SIDE)];
    }
    vec![(0, 0, SIDE), (dx / 2, dy / 2, SIDE), (dx, dy, SIDE)]
}

/// Resize by the short edge, then normalise a square (x, y, side) in resized coordinates.
/// An invalid buffer or out-of-bounds crop returns an empty vector.
pub fn preprocess(rgba: &[u8], w: usize, h: usize, crop: (usize, usize, usize)) -> Vec<f32> {
    let Some(img) = resize(rgba, w, h) else {
        return Vec::new();
    };
    let (x, y, side) = crop;
    if side != SIDE || x.checked_add(side).is_none_or(|v| v > img.width() as usize) || y.checked_add(side).is_none_or(|v| v > img.height() as usize) {
        return Vec::new();
    }
    nchw(&img, x, y)
}

fn nchw(img: &image::RgbaImage, x: usize, y: usize) -> Vec<f32> {
    (0..3)
        .flat_map(|c| {
            (0..SIDE)
                .flat_map(move |row| (0..SIDE).map(move |col| (f32::from(img.get_pixel((x + col) as u32, (y + row) as u32)[c]) / 255.0 - MEAN[c]) / STD[c]))
        })
        .collect()
}

// Weak ownership lets closing the last dialog/session release ~1 GB of model memory.
type Slot = Mutex<Option<(PathBuf, Weak<Clip>)>>;
static CLIP: Slot = Mutex::new(None);

pub fn shared_clip(models_dir: &Path) -> Option<Arc<Clip>> {
    let spec = spec("clip-b32-laion")?;
    installed_bytes(models_dir, spec)?;
    let dir = std::fs::canonicalize(models_dir).ok()?;
    let mut slot = CLIP.lock().ok()?;
    if let Some((path, model)) = slot.as_ref()
        && *path == dir
        && let Some(model) = model.upgrade()
    {
        return Some(model);
    }
    let model = match Clip::load(models_dir, spec) {
        Ok(model) => Arc::new(model),
        Err(e) => {
            log::warn!("Could not load Smart Sort model: {e:#}");
            return None;
        }
    };
    *slot = Some((dir, Arc::downgrade(&model)));
    Some(model)
}

pub(crate) fn forget() {
    *CLIP.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_normalization_and_crop_positions() {
        let rgba = [128, 128, 128, 0].repeat(300 * 200);
        let img = resize(&rgba, 300, 200).unwrap();
        assert_eq!((img.width(), img.height()), (336, 224));
        assert_eq!(crop_rects(336, 224, Crops::Three), [(0, 0, 224), (56, 0, 224), (112, 0, 224)]);
        assert_eq!(crop_rects(224, 336, Crops::Three), [(0, 0, 224), (0, 56, 224), (0, 112, 224)]);
        assert_eq!(crop_rects(224, 224, Crops::Three).len(), 1);
        for crop in crop_rects(336, 224, Crops::Three) {
            let v = preprocess(&rgba, 300, 200, crop);
            for c in 0..3 {
                assert!(v[c * SIDE * SIDE..(c + 1) * SIDE * SIDE].iter().all(|v| (*v - (128.0 / 255.0 - MEAN[c]) / STD[c]).abs() < 1e-5));
            }
        }
        assert!(preprocess(&[], 0, 0, (0, 0, 224)).is_empty());
    }

    #[test]
    fn mock_plans_check_text_types_crop_averaging_and_normalization() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for text_i32 in [true, false] {
            let count = Arc::new(AtomicUsize::new(0));
            let runs = count.clone();
            let model = Clip {
                visual: Arc::new(move |input| {
                    assert_eq!(input.shape(), &[1, 3, 224, 224]);
                    let index = runs.fetch_add(1, Ordering::Relaxed);
                    let mut v = vec![0.0f32; DIM];
                    v[index % 2] = 1.0;
                    Ok(tvec!(tract_ndarray::Array2::from_shape_vec((1, DIM), v)?.into_tensor().into()))
                }),
                textual: Arc::new(move |input| {
                    assert_eq!(input.shape(), &[1, 77]);
                    if text_i32 {
                        assert_eq!(input.datum_type(), i32::datum_type());
                        assert_eq!(input.to_plain_array_view::<i32>()?[[0, 0]], 49406);
                        assert_eq!(input.to_plain_array_view::<i32>()?[[0, 2]], 49407);
                    } else {
                        assert_eq!(input.datum_type(), i64::datum_type());
                        assert_eq!(input.to_plain_array_view::<i64>()?[[0, 0]], 49406);
                        assert_eq!(input.to_plain_array_view::<i64>()?[[0, 2]], 49407);
                    }
                    let mut v = vec![0.0f32; DIM];
                    v[0] = 3.0;
                    v[1] = 4.0;
                    Ok(tvec!(tract_ndarray::Array2::from_shape_vec((1, DIM), v)?.into_tensor().into()))
                }),
                bpe: Bpe::new(serde_json::from_str(include_str!("../tests/fixtures/vocab.json")).unwrap(), include_str!("../tests/fixtures/merges.txt")),
                text_i32,
            };
            let v = model.embed_text("cat").unwrap();
            assert!((v[0] - 0.6).abs() < 1e-6);
            assert!((v[1] - 0.8).abs() < 1e-6);
            let v = model.embed_image(&[128, 128, 128, 255].repeat(300 * 200), 300, 200, Crops::Three).unwrap();
            assert_eq!(count.load(Ordering::Relaxed), 3);
            assert!((v[0] - 2.0 / 5.0_f32.sqrt()).abs() < 1e-6);
            assert!((v[1] - 1.0 / 5.0_f32.sqrt()).abs() < 1e-6);
            count.store(0, Ordering::Relaxed);
            model.embed_image(&[128, 128, 128, 255].repeat(224 * 224), 224, 224, Crops::Three).unwrap();
            assert_eq!(count.load(Ordering::Relaxed), 1);
            assert!(model.embed_image(&[], 0, 0, Crops::Center).is_err());
        }
    }

    #[test]
    #[ignore = "coordinator: requires pinned CLIP bundle"]
    fn real_bpe_golden_tokens() {
        let dir = PathBuf::from(std::env::var_os("LOCAL_IMAGE_MODELS_DIR").expect("models dir"));
        let bpe = Bpe::load(&companion_path(&dir, "clip-vocab.json"), &companion_path(&dir, "clip-merges.txt")).unwrap();
        for (text, expected) in [
            ("a photo of a speaker on stage", vec![49406, 320, 1125, 539, 320, 4914, 525, 2170, 49407]),
            ("a photo of an audience", vec![49406, 320, 1125, 539, 550, 6863, 49407]),
            ("People applauding at a conference", vec![49406, 1047, 24713, 796, 536, 320, 2230, 49407]),
            ("sponsor booth with logo banners", vec![49406, 7574, 3971, 593, 5750, 23145, 49407]),
            ("keynote speaker at a podium", vec![49406, 7556, 4914, 536, 320, 14093, 49407]),
        ] {
            let ids = bpe.encode(text, 77);
            assert_eq!(&ids[..expected.len()], expected.as_slice(), "{text}");
            assert!(ids[expected.len()..].iter().all(|v| *v == 0));
        }
    }

    #[test]
    #[ignore = "coordinator: requires pinned CLIP bundle and reference images"]
    fn real_zero_shot_reference_rankings() {
        let dir = PathBuf::from(std::env::var_os("LOCAL_IMAGE_MODELS_DIR").expect("models dir"));
        let model = Clip::load(&dir, spec("clip-b32-laion").unwrap()).unwrap();
        let prompts = [
            "a photo of a mountain landscape",
            "a photo of a mother with her children",
            "a photo of the earth from space",
            "a photo of a speaker on stage",
            "a photo of a crowd of people",
            "a black and white photo of people",
            "a photo of an audience",
            "a portrait of a woman",
        ];
        let texts: Vec<_> = prompts.iter().map(|p| model.embed_text(p).unwrap()).collect();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/upstream/lightcraft/images");
        for (file, expected) in [("ba-tetons.jpg", vec![0]), ("ba-earthrise.jpg", vec![2]), ("ba-migrant-mother.jpg", vec![1, 5])] {
            let img = image::open(root.join(file)).unwrap().to_rgba8();
            for crops in [Crops::Center, Crops::Three] {
                let v = model.embed_image(img.as_raw(), img.width() as usize, img.height() as usize, crops).unwrap();
                let scores: Vec<f32> = texts.iter().map(|t| t.iter().zip(&v).map(|(a, b)| a * b).sum()).collect();
                let best = (0..scores.len()).max_by(|a, b| scores[*a].total_cmp(&scores[*b])).unwrap();
                assert!(expected.contains(&best), "{file} {crops:?}: {scores:?}");
            }
        }
    }
}
