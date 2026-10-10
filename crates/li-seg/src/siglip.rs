// Copyright 2024 The HuggingFace Inc. team. Apache-2.0 reference algorithms;
// see licenses/siglip-NOTICE.md and docs/PORTS.md.
//! SigLIP base-patch16-224 CPU inference. Its image processor stretches RGB to 224 square
//! with Pillow's antialiased bicubic filter, and text is EOS-padded to exactly 64 tokens.
//! Apache-2.0 references and scalar provenance are recorded in docs/PORTS.md.

use crate::{ModelFamily, Runner, Task, companion_path, installed_bytes, model_path, spec, unigram::Unigram};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use tract_onnx::prelude::*;

pub const MODEL_ID: &str = "siglip-b16-224";
pub const DIM: usize = 768;
const SIDE: usize = 224;
/// Pretrained scalar values reported by the upstream SigLIP conversion harness (not initial
/// training values). The split ONNX encoders omit them; do not substitute CLIP's temperature.
pub const LOGIT_SCALE: f32 = 117.3308;
pub const LOGIT_BIAS: f32 = -12.9324;

pub fn probability(cosine: f32) -> f32 {
    let logit = LOGIT_SCALE * cosine + LOGIT_BIAS;
    if logit >= 0.0 {
        1.0 / (1.0 + (-logit).exp())
    } else {
        let e = logit.exp();
        e / (1.0 + e)
    }
}

pub struct Siglip {
    visual: Arc<Runner>,
    textual: Arc<Runner>,
    tokenizer: Unigram,
}
impl Siglip {
    pub fn load(dir: &Path) -> Result<Self> {
        let spec = spec(MODEL_ID).context("missing SigLIP specification")?;
        if spec.task != (Task::ImageText { family: ModelFamily::Siglip }) || installed_bytes(dir, spec).is_none() {
            bail!("SigLIP model bundle is incomplete");
        }
        let mut visual = tract_onnx::onnx()
            .model_for_path(model_path(dir, spec))?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, SIDE, SIDE)))?;
        visual.select_outputs_by_name(["pooler_output"])?;
        let visual = visual.into_optimized()?.into_runnable()?;
        let mut textual = tract_onnx::onnx()
            .model_for_path(companion_path(dir, spec.companions[0].file))?
            .with_input_fact(0, InferenceFact::dt_shape(i64::datum_type(), tvec!(1, 64)))?;
        textual.select_outputs_by_name(["pooler_output"])?;
        let textual = textual.into_optimized()?.into_runnable()?;
        let tokenizer = Unigram::load(&companion_path(dir, spec.companions[1].file))?;
        Ok(Self { visual: Arc::new(move |t| visual.run(tvec!(t.into()))), textual: Arc::new(move |t| textual.run(tvec!(t.into()))), tokenizer })
    }
    pub fn embed_image(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        let values = preprocess(rgba, w, h)?;
        let input = tract_ndarray::Array4::from_shape_vec((1, 3, SIDE, SIDE), values)?;
        embedding((self.visual)(input.into())?)
    }
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let input = tract_ndarray::Array2::from_shape_vec((1, 64), self.tokenizer.encode(text))?;
        embedding((self.textual)(input.into())?)
    }
}
fn embedding(values: TVec<TValue>) -> Result<Vec<f32>> {
    let mut v: Vec<f32> = values.first().context("SigLIP returned no embedding")?.to_plain_array_view::<f32>()?.iter().copied().collect();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if v.len() != DIM || !norm.is_finite() || norm <= 0.0 {
        bail!("invalid SigLIP embedding");
    }
    for x in &mut v {
        *x /= norm;
    }
    Ok(v)
}

/// Pillow's separable 8-bit bicubic resampling: antialias on reduction, normalized 22-bit
/// coefficients, rounding/clipping after each pass. This preserves RGB independent of alpha.
pub fn preprocess(rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
    if w == 0 || h == 0 || w.checked_mul(h).and_then(|v| v.checked_mul(4)) != Some(rgba.len()) {
        bail!("SigLIP image buffer size mismatch");
    }
    let mut rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().copied()).collect();
    if w != SIDE {
        rgb = resample(&rgb, w, h, true);
    }
    if h != SIDE {
        rgb = resample(&rgb, SIDE, h, false);
    }
    let mut out = vec![0.0; 3 * SIDE * SIDE];
    for c in 0..3 {
        for i in 0..SIDE * SIDE {
            out[c * SIDE * SIDE + i] = (f32::from(rgb[3 * i + c]) / 255.0 - 0.5) / 0.5;
        }
    }
    Ok(out)
}
fn cubic(mut x: f64) -> f64 {
    x = x.abs();
    if x < 1.0 {
        ((1.5 * x - 2.5) * x) * x + 1.0
    } else if x < 2.0 {
        ((-0.5 * x + 2.5) * x - 4.0) * x + 2.0
    } else {
        0.0
    }
}
fn resample(src: &[u8], w: usize, h: usize, horizontal: bool) -> Vec<u8> {
    let input = if horizontal { w } else { h };
    let scale = input as f64 / SIDE as f64;
    let filter = scale.max(1.0);
    let support = 2.0 * filter;
    let (ow, oh) = if horizontal { (SIDE, h) } else { (w, SIDE) };
    let mut out = vec![0u8; ow * oh * 3];
    for dest in 0..SIDE {
        let center = (dest as f64 + 0.5) * scale;
        let start = ((center - support + 0.5) as isize).max(0) as usize;
        let end = ((center + support + 0.5) as usize).min(input);
        let mut coefficients: Vec<f64> = (start..end).map(|i| cubic((i as f64 - center + 0.5) / filter)).collect();
        let sum: f64 = coefficients.iter().sum();
        let coefficients: Vec<i64> = coefficients
            .drain(..)
            .map(|c| {
                let v = c / sum * ((1u64 << 22) as f64);
                (v + if v < 0.0 { -0.5 } else { 0.5 }) as i64
            })
            .collect();
        for cross in 0..if horizontal { h } else { w } {
            for c in 0..3 {
                let mut value = 1i64 << 21;
                for (i, k) in (start..end).zip(&coefficients) {
                    let index = if horizontal { (cross * w + i) * 3 + c } else { (i * w + cross) * 3 + c };
                    value += i64::from(src[index]) * k;
                }
                let index = if horizontal { (cross * ow + dest) * 3 + c } else { (dest * ow + cross) * 3 + c };
                out[index] = (value >> 22).clamp(0, 255) as u8;
            }
        }
    }
    out
}

type Slot = Option<(PathBuf, Weak<Siglip>)>;
static SHARED: Mutex<Slot> = Mutex::new(None);
pub fn shared_siglip(dir: &Path) -> Option<Arc<Siglip>> {
    let mut slot = SHARED.lock().ok()?;
    if let Some((path, weak)) = slot.as_ref()
        && path == dir
        && let Some(m) = weak.upgrade()
    {
        return Some(m);
    }
    let model = match Siglip::load(dir) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            log::warn!("Could not load SigLIP: {e:#}");
            return None;
        }
    };
    *slot = Some((dir.to_path_buf(), Arc::downgrade(&model)));
    Some(model)
}
pub(crate) fn forget() {
    *SHARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn siglip_preprocess_golden_pillow_pixels_ignores_alpha() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/siglip-pixels.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let w = case["w"].as_u64().unwrap() as usize;
            let h = case["h"].as_u64().unwrap() as usize;
            let mut rgba = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    rgba.extend_from_slice(&[((x * 31 + y * 17) % 256) as u8, ((x * 7 + y * 61) % 256) as u8, ((x * 43 + y * 13) % 256) as u8, 0]);
                }
            }
            let out = preprocess(&rgba, w, h).unwrap();
            let golden: Vec<(usize, usize, usize, f32)> = serde_json::from_value(case["pixels"].clone()).unwrap();
            for (c, y, x, v) in golden {
                assert!((out[c * SIDE * SIDE + y * SIDE + x] - v).abs() < 1e-6, "{w}x{h} {c},{y},{x}: {} != {v}", out[c * SIDE * SIDE + y * SIDE + x]);
            }
        }
        assert!(preprocess(&[], 0, 1).is_err());
        let v = preprocess(&[128, 128, 128, 255].repeat(3 * 2), 3, 2).unwrap();
        assert!(v.iter().all(|x| (*x - (128.0 / 255.0 - 0.5) / 0.5).abs() < 1e-6));
    }
    #[test]
    fn siglip_sigmoid_golden_and_extremes() {
        assert!((probability(0.1) - 0.23159048).abs() < 1e-5);
        assert_eq!(probability(-1.0), 0.0);
        assert_eq!(probability(1.0), 1.0);
        assert!(probability(0.12) > probability(0.1));
    }
}

#[cfg(test)]
mod real_tests {
    use super::*;
    #[test]
    #[ignore = "set LI_SEG_TEST_MODELS to the verified pinned bundles; never downloads"]
    fn siglip_real_weights_rank_the_public_landscape_fixture() {
        let dir = std::env::var_os("LI_SEG_TEST_MODELS").expect("LI_SEG_TEST_MODELS");
        let model = Siglip::load(Path::new(&dir)).unwrap();
        let image = image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/upstream/lightcraft/images/ba-tetons.jpg")).unwrap().to_rgba8();
        let img = model.embed_image(image.as_raw(), image.width() as usize, image.height() as usize).unwrap();
        let texts = ["a photo of a mountain landscape", "a photo of food", "a photo of a conference speaker", "a photo of a cat"];
        let scores: Vec<f32> = texts
            .iter()
            .map(|s| {
                let t = model.embed_text(s).unwrap();
                img.iter().zip(t).map(|(a, b)| a * b).sum()
            })
            .collect();
        assert!(scores.iter().skip(1).all(|s| scores[0] > *s), "{scores:?}");
    }
}
