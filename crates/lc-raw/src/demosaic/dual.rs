//! Dual demosaic: a detail-preserving demosaic (RCD) where the image has structure, a smooth one
//! (bilinear) in flat areas, blended by a local-contrast mask — flat areas then show less
//! maze-like noise. Ported from darktable's `src/iop/demosaicing/dual.c` and the detail mask of
//! `src/develop/masks/detail.c` (GPL-3.0-or-later; dual demosaicing by Ingo Weyrich for
//! RawTherapee, adapted by Hanno Schwalm), see `docs/PORTS.md`.
//!
//! Differences from darktable: the flat-area method is bilinear instead of VNG4 with colour
//! smoothing, and the mask is computed without white balance (the mosaic is not white balanced
//! here).

use super::Mosaic;
use crate::Rgb32f;
use lightcraft_raster::{Plane, par_rows};

/// darktable's `_slider2contrast`: threshold slider (0..1) → contrast threshold of the mask.
fn contrast_of(threshold: f32) -> f32 {
    0.005 * threshold.powf(1.1)
}

/// Scharr gradient magnitude at `(x, y)` of `p` (darktable's `scharr_gradient`).
#[inline]
fn scharr(p: &[f32], w: usize, x: usize, y: usize) -> f32 {
    let at = |dx: isize, dy: isize| p[(y as isize + dy) as usize * w + (x as isize + dx) as usize];
    let gx = 47.0 / 255.0 * (at(-1, -1) - at(1, -1) + at(-1, 1) - at(1, 1)) + 162.0 / 255.0 * (at(-1, 0) - at(1, 0));
    let gy = 47.0 / 255.0 * (at(-1, -1) - at(-1, 1) + at(1, -1) - at(1, 1)) + 162.0 / 255.0 * (at(0, -1) - at(0, 1));
    gx.hypot(gy)
}

/// Where `img` has detail: 1 on edges and texture, 0 in flat areas, blurred (σ = 2 px).
pub(crate) fn detail_mask(img: &Rgb32f, threshold: f32) -> Plane {
    let (w, h) = (img.width, img.height);
    // a gamma (sqrt) evens out the noise variance across the tonal range
    let y: Vec<f32> = img.data.iter().map(|p| ((p[0].max(0.0) + p[1].max(0.0) + p[2].max(0.0)) / 3.0).sqrt()).collect();
    let ithreshold = 16.0 / contrast_of(threshold).max(1e-7);
    let mut mask = Plane::new(w, h);
    par_rows(&mut mask.data, w, |row, out| {
        let iy = row.clamp(1, h - 2);
        for (col, o) in out.iter_mut().enumerate() {
            let ix = col.clamp(1, w - 2);
            let g = (scharr(&y, w, ix, iy) / 16.0).clamp(0.0, 1.0);
            *o = (1.0 / (1.0 + (16.0 - ithreshold * g).exp())).clamp(0.0, 1.0);
        }
    });
    lightcraft_raster::blur::gaussian(&mask, 2.0)
}

/// Blend `high` (the detail demosaic) with a bilinear demosaic by [`detail_mask`].
/// `threshold` 0..1 (0 = `high` everywhere).
pub(crate) fn dual(m: &Mosaic, high: Rgb32f, threshold: f32) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    if w < 16 || h < 16 || threshold <= 0.0 {
        return high;
    }
    let mask = detail_mask(&high, threshold);
    let low = super::bilinear::bilinear(m);
    let mut out = high;
    par_rows(&mut out.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let i = y * w + x;
            let k = mask.data[i];
            let l = low.data[i];
            *px = std::array::from_fn(|c| k * (px[c] - l[c]) + l[c]);
        }
    });
    out
}

/// darktable's VNG-linear + two colour-smoothing passes, using the existing detail mask.
pub(crate) fn dual_vng(m: &Mosaic, high: Rgb32f, threshold: f32) -> Rgb32f {
    if m.w < 16 || m.h < 16 || threshold <= 0.0 {return high;}
    let mask = detail_mask(&high, threshold);
    let mut low = super::vng::vng(m, true);
    super::vng::color_smoothing(&mut low, 2);
    let mut out = high;
    par_rows(&mut out.data, m.w, |y,row| {
        for (x,p) in row.iter_mut().enumerate() {
            let i=y*m.w+x;
            *p=std::array::from_fn(|c|mask.data[i]*(p[c]-low.data[i][c])+low.data[i][c]);
        }
    });
    out
}
