//! local-image: quick **Subject**, **Background**, **Sky** and **Depth** masks from small models
//! that run on the CPU with no AI server (`li-seg`: IS-Net for the subject, PP-MobileSeg for the sky,
//! Depth Anything V2 Small for depth, or a custom model for any of them). The model looks at the photo as developed, uncropped and without masks
//! (as SAM 3 does for Object masks), once, when the mask is added; the result is stored in the
//! mask as a [`SegMask`] (resolution-independent logits), so renders and exports never run the
//! model and the mask stays put when the photo is cropped later. Without a model the mask keeps
//! the classical estimate.

use li_raster::resample::Filter;
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{MaskShape, SegMask};

use crate::Session;

/// Long edge of the photo the quick models see.
const INPUT_EDGE: usize = 1024;
/// Side of the stored logit grid for sky and depth maps (their models resolve no finer).
const GRID: usize = 256;
/// Side of the stored logit grid for subject maps (their 1024 px models resolve twice as fine).
const SUBJECT_GRID: usize = 512;
/// Refine Edges (0..100) of a new mask a model made: its edges follow the photo's instead of the
/// stored grid's (masks already in documents keep theirs).
pub const MODEL_EDGE_REFINE: f64 = 30.0;

/// Whether `shape` holds a model's segmentation (Subject, Background, Sky or Depth Range).
pub fn is_model_mask(shape: &MaskShape) -> bool {
    matches!(
        shape,
        MaskShape::Subject { seg: Some(_) }
            | MaskShape::Background { seg: Some(_) }
            | MaskShape::Sky { seg: Some(_) }
            | MaskShape::DepthRange { seg: Some(_), .. }
    )
}

impl Session {
    /// Recompute only incoming AI selections on this photo. Existing target masks stay intact.
    pub(crate) fn refresh_transferred_masks(&mut self, id: PhotoId, settings: &mut lightcraft_develop::DevelopSettings, first: usize) {
        let input_settings = settings.clone();
        for mask in settings.masks.iter_mut().skip(first) {
            for component in &mut mask.components {
                let shape = &mut component.shape;
                self.quick_segment_with_settings(id, shape, Some(&input_settings));
                match shape {
                    MaskShape::Object { hint, exclude, seg, detail, .. } => {
                        *seg = None;
                        detail.clear();
                        if !hint.is_empty() {
                            *seg = self
                                .segment_clicks(
                                    id,
                                    &hint
                                        .iter()
                                        .map(|p| crate::segment::Click { at: *p, include: true })
                                        .chain(exclude.iter().map(|p| crate::segment::Click { at: *p, include: false }))
                                        .collect::<Vec<_>>(),
                                )
                                .ok();
                        }
                    }
                    MaskShape::Prompt { text, seg, detail, .. } => {
                        *seg = self.segment_text(id, text).ok().flatten();
                        detail.clear();
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn transfer_settings(
        &mut self,
        id: PhotoId,
        current: &lightcraft_develop::DevelopSettings,
        partial: &serde_json::Value,
    ) -> lightcraft_develop::DevelopSettings {
        let mut partial = partial.clone();
        if let Some(object) = partial.as_object_mut() {
            lightcraft_develop::presets::strip_photo_bound(object);
        }
        let mut settings = lightcraft_develop::apply_partial(current, &partial, 1.0);
        if partial.get("masks").is_some() {
            self.refresh_transferred_masks(id, &mut settings, 0);
        }
        settings
    }

    /// Fills a Subject / Background / Sky shape's segmentation from the quick models when one is
    /// installed (other shapes, or no model: unchanged). Returns whether it did.
    pub fn quick_segment(&mut self, id: PhotoId, shape: &mut MaskShape) -> bool {
        let settings = self.segment_key(id).map(|(_, settings)| settings);
        self.quick_segment_with_settings(id, shape, settings.as_ref())
    }
    pub(crate) fn quick_segment_with_settings(
        &mut self,
        id: PhotoId,
        shape: &mut MaskShape,
        settings: Option<&lightcraft_develop::DevelopSettings>,
    ) -> bool {
        #[derive(PartialEq)]
        enum K {
            Subject,
            Sky,
            Depth,
        }
        let kind = match shape {
            MaskShape::Subject { seg: None } | MaskShape::Background { seg: None } => K::Subject,
            MaskShape::Sky { seg: None } => K::Sky,
            MaskShape::DepthRange { seg: None, .. } => K::Depth,
            _ => return false,
        };
        let sky = kind == K::Sky;
        let Some(dir) = self.quick_seg_dir.clone() else { return false };
        let model = match kind {
            K::Subject => li_seg::shared(&dir),
            K::Sky => li_seg::shared_sky(&dir),
            K::Depth => li_seg::shared_depth(&dir),
        };
        let Some(model) = model else { return false };
        let Some(settings) = settings else { return false };
        let mut settings = settings.clone();
        settings.masks.clear();
        let Some(job) = self.preview_job(id, INPUT_EDGE, INPUT_EDGE, false, &settings) else { return false };
        let Ok(r) = job.run().rendered else { return false };
        let img = r.image;
        let (w, h) = (img.width, img.height);
        let rgba: Vec<u8> = img.data.iter().flatten().copied().collect();
        let prob = if kind == K::Depth {
            // stored as distance (0 = nearest), the scale the depth range reads
            model.predict_depth(&rgba, w, h).ok().map(|near| near.iter().map(|n| 1.0 - n).collect())
        } else if sky {
            model.predict_sky(&rgba, w, h).ok()
        } else {
            // No clear subject: an empty subject (a full background).
            model.predict(&rgba, w, h).ok().map(|p| p.unwrap_or_else(|| vec![0.0; w * h]))
        };
        let Some(prob) = prob else { return false };
        let invert = matches!(shape, MaskShape::Background { .. });
        let seg = Some(to_segmask(&prob, w, h, invert, if kind == K::Subject { SUBJECT_GRID } else { GRID }));
        match shape {
            MaskShape::Subject { seg: s } | MaskShape::Background { seg: s } | MaskShape::Sky { seg: s } | MaskShape::DepthRange { seg: s, .. } => {
                *s = seg
            }
            _ => {}
        }
        true
    }
}

/// A `w × h` probability map as a `side`² logit grid over the whole photo (`invert`: 1 − p). The
/// probabilities are area-averaged into the grid's cells before they become logits, so a thin
/// structure keeps its share of a cell instead of being sampled in or out.
pub fn to_segmask(prob: &[f32], w: usize, h: usize, invert: bool, side: usize) -> SegMask {
    let grid = li_raster::GrayFrame { width: w, height: h, data: prob.to_vec() }.resize(side, side, Filter::Box);
    let logits: Vec<f32> = grid
        .data
        .iter()
        .map(|p| {
            let p = if invert { 1.0 - p } else { *p }.clamp(1e-6, 1.0 - 1e-6);
            (p / (1.0 - p)).ln()
        })
        .collect();
    SegMask::from_logits(side, &logits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored grid keeps the probabilities' sides: high where the map is high, low elsewhere,
    /// and Background is its complement.
    #[test]
    fn probabilities_become_a_logit_grid() {
        let (w, h) = (40, 20);
        let prob: Vec<f32> = (0..w * h).map(|i| if i % w < w / 2 { 0.95 } else { 0.05 }).collect();
        for invert in [false, true] {
            let seg = to_segmask(&prob, w, h, invert, GRID);
            assert_eq!(seg.side as usize, GRID);
            let logits = seg.logits().expect("decodes");
            let (left, right) = (logits[GRID * GRID / 2 + 10], logits[GRID * GRID / 2 + GRID - 10]);
            if invert {
                assert!(left < 0.0 && right > 0.0, "{left} {right}");
            } else {
                assert!(left > 0.0 && right < 0.0, "{left} {right}");
            }
        }
    }

    /// A sky over a sloped roofline at 3600 × 2400 (sky at 4.0, wall at 0.05), what a 1024 px model
    /// sees of it (the sky's share of each of its pixels, or with `soft` a logistic edge that many
    /// model pixels wide) and where each column's 50 % sky contour should be.
    fn roofline(soft: Option<f32>) -> (li_raster::RgbFrame, Vec<f32>, (usize, usize), impl Fn(f32) -> f32) {
        let (w, h) = (3600usize, 2400usize);
        let edge = move |x: f32| 0.42 * h as f32 + 0.06 * x;
        let img = li_raster::RgbFrame::from_fn(w, h, |x, y| if (y as f32 + 0.5) < edge(x as f32 + 0.5) { [4.0; 3] } else { [0.05; 3] });
        let (mw, mh) = (1024usize, 683usize);
        let (sx, sy) = (w as f32 / mw as f32, h as f32 / mh as f32);
        let prob = (0..mw * mh)
            .map(|i| {
                let (x, y) = ((i % mw) as f32 * sx, (i / mw) as f32 * sy);
                match soft {
                    Some(soft) => 1.0 / (1.0 + (-(edge(x + sx / 2.0) - y - sy / 2.0) / (soft * sy)).exp()),
                    None => {
                        (0..16).filter(|k| y + (*k / 4) as f32 * sy / 4.0 + sy / 8.0 < edge(x + (*k % 4) as f32 * sx / 4.0 + sx / 8.0)).count() as f32
                            / 16.0
                    }
                }
            })
            .collect();
        (img, prob, (mw, mh), edge)
    }

    /// The mean alpha error within 20 px of the roofline, and the farthest any column's 50 %
    /// contour lies from it.
    fn edge_error(alpha: &li_raster::GrayFrame, edge: &impl Fn(f32) -> f32) -> (f32, f32) {
        let (mut sum, mut n, mut worst) = (0.0f32, 0usize, 0.0f32);
        for x in (40..alpha.width - 40).step_by(7) {
            let e = edge(x as f32 + 0.5);
            let column = |y: usize| alpha.get(x, y);
            for y in (e as usize).saturating_sub(20)..(e as usize + 20).min(alpha.height) {
                let truth = (e - y as f32).clamp(0.0, 1.0);
                sum += (column(y) - truth).abs();
                n += 1;
            }
            let contour = (1..alpha.height).find(|&y| column(y) < 0.5).map_or(alpha.height as f32, |y| {
                let (a, b) = (column(y - 1), column(y));
                y as f32 - 0.5 + (a - 0.5) / (a - b).max(1e-6)
            });
            worst = worst.max((contour - e).abs());
        }
        (sum / n.max(1) as f32, worst)
    }

    #[test]
    fn model_masks_follow_a_roofline_at_full_resolution() {
        use lightcraft_develop::{Mask, MaskComponent, MaskOp};
        let mask = |shape: MaskShape, refine: f64| Mask {
            components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }],
            refine,
            ..Default::default()
        };
        // a sky model's soft edge (3 model pixels) on the sky grid, and a sharp subject model on the
        // subject grid, each against what masks stored before: a 256 grid drawn without refinement
        for (soft, side, max_ratio) in [(Some(3.0), GRID, 0.6), (None, SUBJECT_GRID, 0.7)] {
            let (img, prob, (mw, mh), edge) = roofline(soft);
            let (w, h) = (img.width, img.height);
            let log_l = img.map(lightcraft_pipeline::local::log_lum);
            let frame = lightcraft_pipeline::geometry::Frame::new(w, h, &Default::default(), true);
            let error = |seg: SegMask, refine: f64| {
                let shape = if soft.is_some() { MaskShape::Sky { seg: Some(seg) } } else { MaskShape::Subject { seg: Some(seg) } };
                edge_error(&lightcraft_pipeline::masks::evaluate_one(&mask(shape, refine), &frame, w, h, &img, &log_l, 0.0), &edge)
            };
            let before = error(to_segmask(&prob, mw, mh, false, GRID), 0.0);
            let (mae, worst) = error(to_segmask(&prob, mw, mh, false, side), MODEL_EDGE_REFINE);
            eprintln!(
                "soft {soft:?}, grid {side}: band error {mae:.4} (before {:.4}), contour off by up to {worst:.2} px (before {:.2})",
                before.0, before.1
            );
            assert!(mae < before.0 * max_ratio, "band error {mae} vs {}", before.0);
            assert!(worst < 3.0 && worst < before.1, "contour {worst} px vs {}", before.1);
        }
    }

    #[test]
    fn new_model_masks_refine_their_edges_and_other_masks_do_not() {
        use serde_json::json;
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        let prob: Vec<f32> = (0..64 * 32).map(|i| if i / 64 < 16 { 0.95 } else { 0.05 }).collect();
        let seg = serde_json::to_value(to_segmask(&prob, 64, 32, false, GRID)).unwrap();
        let last = |s: &Session| s.develop_of(id).unwrap().masks.last().cloned().unwrap();
        s.execute("mask.add", &json!({"kind": "sky", "seg": seg})).unwrap();
        assert!(matches!(last(&s).components[0].shape, MaskShape::Sky { seg: Some(_) }));
        assert_eq!(last(&s).refine, MODEL_EDGE_REFINE);
        s.execute("mask.add", &json!({"kind": "radial"})).unwrap();
        assert_eq!(last(&s).refine, 0.0, "only model-made masks");
        s.execute("mask.add", &json!({"kind": "sky"})).unwrap();
        assert_eq!(last(&s).refine, 0.0, "no model, no segmentation: the classical estimate as before");
        // an empty layer's first model-made shape is refined; a shape added to another isn't changed
        s.execute("mask.add", &json!({"kind": "empty"})).unwrap();
        let empty = last(&s).id;
        s.execute("mask.addComponent", &json!({"id": empty, "kind": "subject", "seg": seg})).unwrap();
        assert_eq!(last(&s).refine, MODEL_EDGE_REFINE);
        s.execute("mask.add", &json!({"kind": "linear"})).unwrap();
        let linear = last(&s).id;
        s.execute("mask.addComponent", &json!({"id": linear, "kind": "subject", "seg": seg})).unwrap();
        assert_eq!(last(&s).refine, 0.0);
    }

    #[test]
    fn without_models_masks_keep_the_classical_estimate() {
        let mut s = Session::new();
        let mut shape = MaskShape::Subject { seg: None };
        assert!(!s.quick_segment(PhotoId(1), &mut shape));
        assert_eq!(shape, MaskShape::Subject { seg: None });
    }
}
