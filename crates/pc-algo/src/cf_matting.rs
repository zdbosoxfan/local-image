//! Compatibility adapters to the shared alpha matting backend.
use crate::segment::RgbImage;
pub use li_matte::cf_matting::{MattingParams, trimap_from_mask};

fn guide(img: &RgbImage) -> li_matte::RgbImage<'_> {
    li_matte::RgbImage::borrowed(img.w, img.h, &img.px)
}

pub fn closed_form_matting(img: &RgbImage, trimap: &[f32], p: &MattingParams) -> Vec<f32> {
    li_matte::cf_matting::closed_form_matting(&guide(img), trimap, p)
}

pub fn closed_form_matting_tiled(img: &RgbImage, trimap: &[f32], p: &MattingParams, tile: usize, margin: usize) -> Vec<f32> {
    li_matte::cf_matting::closed_form_matting_tiled(&guide(img), trimap, p, tile, margin)
}

pub fn estimate_foreground(img: &RgbImage, alpha: &[f32]) -> RgbImage {
    let result = li_matte::cf_matting::estimate_foreground(&guide(img), alpha);
    RgbImage { w: result.w, h: result.h, px: result.px.into_owned() }
}
