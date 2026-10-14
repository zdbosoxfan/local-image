//! Shared numerical edge refinement. Product crates retain their surface/settings adapters.
#![forbid(unsafe_code)]

use std::borrow::Cow;
pub mod cf_matting;
pub mod distance;
pub mod matting;
pub mod refine;
pub use matting::RefineParams;

/// A borrowed RGB guide, or an owned pyramid level. No input-image clone at adapters.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbImage<'a> {
    pub w: usize,
    pub h: usize,
    pub px: Cow<'a, [[f32; 3]]>,
}
impl<'a> RgbImage<'a> {
    pub fn borrowed(w: usize, h: usize, px: &'a [[f32; 3]]) -> Self {
        assert_eq!(px.len(), w * h);
        Self { w, h, px: Cow::Borrowed(px) }
    }
    pub fn from_fn(w: usize, h: usize, mut f: impl FnMut(usize, usize) -> [f32; 3]) -> Self {
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                px.push(f(x, y));
            }
        }
        Self { w, h, px: Cow::Owned(px) }
    }
}
