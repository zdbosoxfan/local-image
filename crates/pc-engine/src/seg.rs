//! local-image: learned subject segmentation (`li-seg`, U²-Net / IS-Net on the CPU) behind
//! Select › Subject, Remove Background and the Object Selection tool's click mode. It is used when
//! a segmentation model is installed (Local AI window › Selection models); otherwise those commands
//! keep PhotoCraft's classical saliency + GrabCut.

use std::path::PathBuf;

use photocraft_algo::selection::{self as sel, Region};
use photocraft_doc::{Document, LayerId};
use photocraft_geom::Rect;
use serde_json::Value;

/// Where segmentation models live: `<AI model folder>/segmentation/`.
pub fn models_dir() -> PathBuf {
    std::env::var_os("LOCAL_IMAGE_SEG_MODELS").map(PathBuf::from).unwrap_or_else(|| li_ai::AiSettings::load().model_dir())
}

/// The installed model the commands would use, if any.
pub fn installed() -> Option<&'static li_seg::ModelSpec> {
    li_seg::best_installed(&models_dir()).map(|(s, _)| s)
}

/// `"engine"`: `auto` (learned when installed), `classic` or `learned` (an error without a model).
pub fn wanted(p: &Value) -> std::result::Result<bool, String> {
    match p.get("engine").and_then(Value::as_str).unwrap_or("auto") {
        "classic" => Ok(false),
        "learned" if installed().is_none() => Err("no selection model is installed (Help › AI Models & GPU…)".into()),
        "learned" => Ok(true),
        _ => Ok(installed().is_some()),
    }
}

/// 8-bit RGBA of `layer` (or the composite with `all_layers`, or when it has no pixels).
fn rgba8(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> (Rect, Vec<u8>) {
    let area = doc.bounds();
    let px: Vec<[u8; 4]> = match (all_layers, layer.and_then(|id| doc.layer(id)).and_then(|l| l.surface())) {
        (false, Some(surf)) => sel::rgba8_image(surf, area),
        _ => {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            photocraft_compose::render(doc, area).px.iter().map(|p| p.map(q)).collect()
        }
    };
    (area, px.into_iter().flatten().collect())
}

/// Foreground probability over the canvas, or `None` (no model, or no clear subject).
pub fn probability(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<(Rect, Vec<f32>)> {
    let seg = li_seg::shared(&models_dir())?;
    let (area, rgba) = rgba8(doc, layer, all_layers);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let prob = seg.predict(&rgba, w, h).ok()??;
    Some((area, prob))
}

/// The tight region of the non-zero coverage in `prob` (over `area`).
pub fn region(area: Rect, prob: &[f32]) -> Option<Region> {
    let w = area.width() as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (i, v) in prob.iter().enumerate() {
        if *v >= 1.0 / 255.0 {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 == usize::MAX {
        return None;
    }
    let bw = x1 - x0 + 1;
    let mut mask = Vec::with_capacity(bw * (y1 - y0 + 1));
    for y in y0..=y1 {
        mask.extend(prob[y * w + x0..=y * w + x1].iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
    }
    Some(Region { bbox: Rect::new(area.x0 + x0 as i32, area.y0 + y0 as i32, area.x0 + x1 as i32 + 1, area.y0 + y1 as i32 + 1), mask })
}

/// Select Subject with the learned model.
pub fn subject(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<Region> {
    let (area, prob) = probability(doc, layer, all_layers)?;
    region(area, &prob)
}

/// The object under document point `(x, y)` (Object Selection click mode): the connected part of
/// the salient map around the click.
pub fn object_at(doc: &Document, layer: Option<LayerId>, all_layers: bool, x: i32, y: i32) -> Option<Region> {
    let (area, prob) = probability(doc, layer, all_layers)?;
    if !area.contains(x, y) {
        return None;
    }
    let (w, h) = (area.width() as usize, area.height() as usize);
    let obj = li_seg::object_at(&prob, w, h, (x - area.x0) as usize, (y - area.y0) as usize)?;
    region(area, &obj)
}

/// The installed sky model, if any.
pub fn sky_installed() -> Option<&'static li_seg::ModelSpec> {
    li_seg::best_sky(&models_dir()).map(|(s, _)| s)
}

/// Sky probability over the canvas from the sky model, its edges snapped to the image with a
/// colour-guided filter (the model sees a 256–512 px copy). `None` without a sky model.
pub fn sky_probability(doc: &Document, layer: Option<LayerId>, all_layers: bool) -> Option<(Rect, Vec<f32>)> {
    let seg = li_seg::shared_sky(&models_dir())?;
    let (area, rgba) = rgba8(doc, layer, all_layers);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let prob = seg.predict_sky(&rgba, w, h).ok()?;
    let guide = photocraft_algo::segment::RgbImage {
        w,
        h,
        px: rgba.chunks_exact(4).map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect(),
    };
    let r = (w.max(h) / 160).clamp(2, 24);
    let refined = photocraft_algo::matting::guided_filter_color(&guide, &prob, r, 1e-3);
    Some((area, refined.into_iter().map(|v| v.clamp(0.0, 1.0)).collect()))
}

/// Shapes a probability map with Select › Sky's threshold (0–100, higher = stricter) and softness
/// (0–100, the width of the soft edge).
pub fn shape(prob: &mut [f32], threshold: f32, softness: f32) {
    let t = (threshold / 100.0).clamp(0.02, 0.98);
    let s = (softness / 100.0 * 0.5).clamp(0.01, 0.5);
    for v in prob.iter_mut() {
        *v = ((*v - (t - s / 2.0)) / s).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_is_the_tight_box_of_the_coverage() {
        let area = Rect::new(10, 20, 15, 24);
        let mut p = vec![0.0f32; 5 * 4];
        p[5 + 1] = 1.0;
        p[2 * 5 + 3] = 0.5;
        let r = region(area, &p).unwrap();
        assert_eq!(r.bbox, Rect::new(11, 21, 14, 23));
        assert_eq!(r.mask, vec![255, 0, 0, 0, 0, 128]);
        assert!(region(area, &[0.0; 20]).is_none());
    }

    #[test]
    fn classic_engine_needs_no_model() {
        assert_eq!(wanted(&serde_json::json!({"engine": "classic"})), Ok(false));
    }
}
