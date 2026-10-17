//! Shared raster kernels and the dense [`Frame`] buffer (Develop's float images included).
//! Tiled editor documents keep their own surface type.
#![forbid(unsafe_code)]
pub mod blur;
pub mod frame;
pub mod histogram;
pub mod resample;
pub mod resize_u8;
pub use frame::{Edge, Frame, GrayFrame, RgbFrame, Rgba8, RgbaFrame, quantize_u8};
pub use histogram::Histogram;
pub use resample::{Filter, Pixel};
pub use resize_u8::{resize_gray8, resize_rgb8, resize_rgba8, resize_u8};
/// Independent rows; sequential on wasm or without the parallel feature.
pub fn par_rows<T: Send>(data: &mut [T], row_len: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    if row_len == 0 {
        return;
    }
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    {
        use rayon::prelude::*;
        data.par_chunks_mut(row_len).enumerate().for_each(|(y, row)| f(y, row));
    }
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    {
        data.chunks_mut(row_len).enumerate().for_each(|(y, row)| f(y, row));
    }
}
/// Run `a` and `b` potentially in parallel (sequentially on wasm or without the parallel
/// feature). For independent passes that each scale poorly on their own (small images, short rows).
pub fn par_join<A: Send, B: Send>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    {
        rayon::join(a, b)
    }
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    {
        (a(), b())
    }
}
/// Maps `f` over `0..n`, keeping order; sequential on wasm or without the parallel feature.
pub fn par_map<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    {
        use rayon::prelude::*;
        (0..n).into_par_iter().map(f).collect()
    }
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    {
        (0..n).map(f).collect()
    }
}
/// Half-open raster region in document pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}
impl Region {
    pub const EMPTY: Self = Self::new(0, 0, 0, 0);
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }
    pub fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
    pub fn width(self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }
    pub fn height(self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }
}
