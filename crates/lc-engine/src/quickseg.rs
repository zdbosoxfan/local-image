//! local-image: quick **Subject**, **Background** and **Sky** masks from small segmentation models
//! that run on the CPU with no AI server (`li-seg`: U²-Net / IS-Net for the subject, PP-MobileSeg /
//! TinySkyNet for the sky). The model looks at the photo as developed, uncropped and without masks
//! (as SAM 3 does for Object masks), once, when the mask is added; the result is stored in the
//! mask as a [`SegMask`] (resolution-independent logits), so renders and exports never run the
//! model and the mask stays put when the photo is cropped later. Without a model the mask keeps
//! the classical estimate.

use lightcraft_catalog::PhotoId;
use lightcraft_develop::{MaskShape, SegMask};

use crate::Session;

/// Long edge of the photo the quick models see.
const INPUT_EDGE: usize = 1024;
/// Side of the stored logit grid.
const GRID: usize = 256;

impl Session {
    /// Fills a Subject / Background / Sky shape's segmentation from the quick models when one is
    /// installed (other shapes, or no model: unchanged). Returns whether it did.
    pub fn quick_segment(&mut self, id: PhotoId, shape: &mut MaskShape) -> bool {
        let sky = match shape {
            MaskShape::Subject { seg: None } | MaskShape::Background { seg: None } => false,
            MaskShape::Sky { seg: None } => true,
            _ => return false,
        };
        let Some(dir) = self.quick_seg_dir.clone() else { return false };
        let Some(model) = (if sky { li_seg::shared_sky(&dir) } else { li_seg::shared(&dir) }) else { return false };
        let Some((_, settings)) = self.segment_key(id) else { return false };
        let Some(job) = self.preview_job(id, INPUT_EDGE, INPUT_EDGE, false, &settings) else { return false };
        let Ok(r) = job.run().rendered else { return false };
        let img = r.image;
        let (w, h) = (img.width, img.height);
        let rgba: Vec<u8> = img.data.iter().flatten().copied().collect();
        let prob = if sky {
            model.predict_sky(&rgba, w, h).ok()
        } else {
            // No clear subject: an empty subject (a full background).
            model.predict(&rgba, w, h).ok().map(|p| p.unwrap_or_else(|| vec![0.0; w * h]))
        };
        let Some(prob) = prob else { return false };
        let invert = matches!(shape, MaskShape::Background { .. });
        let seg = Some(to_segmask(&prob, w, h, invert));
        match shape {
            MaskShape::Subject { seg: s } | MaskShape::Background { seg: s } | MaskShape::Sky { seg: s } => *s = seg,
            _ => {}
        }
        true
    }
}

/// A `w × h` probability map as a [`GRID`]² logit grid over the whole photo (`invert`: 1 − p).
pub fn to_segmask(prob: &[f32], w: usize, h: usize, invert: bool) -> SegMask {
    let grid = li_seg::upscale(prob, w, h, GRID, GRID);
    let logits: Vec<f32> = grid
        .iter()
        .map(|p| {
            let p = if invert { 1.0 - p } else { *p }.clamp(1e-6, 1.0 - 1e-6);
            (p / (1.0 - p)).ln()
        })
        .collect();
    SegMask::from_logits(GRID, &logits)
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
            let seg = to_segmask(&prob, w, h, invert);
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

    #[test]
    fn without_models_masks_keep_the_classical_estimate() {
        let mut s = Session::new();
        let mut shape = MaskShape::Subject { seg: None };
        assert!(!s.quick_segment(PhotoId(1), &mut shape));
        assert_eq!(shape, MaskShape::Subject { seg: None });
    }
}
