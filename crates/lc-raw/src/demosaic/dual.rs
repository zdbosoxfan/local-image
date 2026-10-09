//! Dual demosaic with upstream VNG-linear and two colour-smoothing passes in flat areas.
//! The legacy bilinear option and its historical mask remain available for saved settings.
//! Ported from darktable `src/iop/demosaicing/dual.c` and `src/develop/masks/detail.c`
//! (GPL-3.0-or-later; Ingo Weyrich for RawTherapee, adapted by Hanno Schwalm).
//! See `docs/PORTS.md` and `licenses/darktable-NOTICE.md`.
//! Masks use unbalanced camera RGB, equivalent to upstream undoing white balance.

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

/// darktable's VNG-linear + two colour-smoothing passes, using the upstream detail mask.
pub(crate) fn dual_vng(m: &Mosaic, high: Rgb32f, threshold: f32) -> Rgb32f {
    if m.w < 16 || m.h < 16 || threshold <= 0.0 {
        return high;
    }
    let mask = upstream_detail_mask(&high, threshold);
    let mut low = super::vng::vng(m, true);
    super::vng::color_smoothing(&mut low, 2);
    let mut out = high;
    par_rows(&mut out.data, m.w, |y, row| {
        for (x, p) in row.iter_mut().enumerate() {
            let i = y * m.w + x;
            *p = std::array::from_fn(|c| mask.data[i] * (p[c] - low.data[i][c]) + low.data[i][c]);
        }
    });
    out
}

/// Full upstream dual mask: Scharr, bit-approximate exponential and disc-truncated 9x9 blur.
/// Data is unbalanced camera RGB, equivalent to darktable undoing WB for the mask.
fn upstream_detail_mask(img: &Rgb32f, threshold: f32) -> Plane {
    let (w, h) = (img.width, img.height);
    let y: Vec<f32> = img.data.iter().map(|p| ((p[0].max(0.0) + p[1].max(0.0) + p[2].max(0.0)) / 3.0).sqrt()).collect();
    let ithreshold = 16.0 / contrast_of(threshold).max(1e-7);
    let mut mask = vec![0.0; w * h];
    par_rows(&mut mask, w, |row, out| {
        for (x, v) in out.iter_mut().enumerate() {
            let g = (scharr(&y, w, x.clamp(1, w - 2), row.clamp(1, h - 2)) / 16.0).clamp(0.0, 1.0);
            *v = (1.0 / (1.0 + crate::numerics::fast_exp(16.0 - ithreshold * g))).clamp(0.0, 1.0);
        }
    });
    Plane { width: w, height: h, data: crate::numerics::gaussian9(&mask, w, h, 2.0, 0.0, 1.0) }
}

#[cfg(test)]
mod refvec_tests {
    use super::*;
    #[test]
    #[ignore = "offline chart measurement; run with --ignored --nocapture"]
    fn full_vng_vs_linear_dual_quality() {
        for (kind, name) in ["zone plate", "Siemens star", "colour edges"].into_iter().enumerate() {
            let truth = crate::quality_support::scene(kind, 256, 256);
            let cfa = crate::Cfa::bayer("RGGB").unwrap();
            let n = super::super::mosaic_from_rgb(&truth, &cfa);
            let m = Mosaic { w: n.width, h: n.height, data: &n.data, cfa: &cfa };
            for method in [crate::Method::Rcd, crate::Method::Amaze] {
                let high = crate::demosaic(&n, method);
                let linear = dual_vng(&m, high.clone(), 0.2);
                let mask = upstream_detail_mask(&high, 0.2);
                let mut low = super::super::vng::vng(&m, false);
                super::super::vng::color_smoothing(&mut low, 2);
                let full = Rgb32f::from_fn(n.width, n.height, |x, y| {
                    let i = y * n.width + x;
                    std::array::from_fn(|c| mask.data[i] * (high.data[i][c] - low.data[i][c]) + low.data[i][c])
                });
                for (low, out) in [("linear+median", linear), ("full+median", full)] {
                    eprintln!(
                        "dual-choice,{name},{method:?},{low},{:.6},{:.9}",
                        super::super::psnr(&truth, &out, 16),
                        crate::quality_support::chroma_error(&truth, &out, 16)
                    );
                }
            }
        }
    }

    #[test]
    fn upstream_mask_and_legacy_difference() {
        let img = Rgb32f::from_fn(64, 64, |x, y| {
            std::array::from_fn(|c| {
                0.2 + 0.003 * x as f32 + 0.001 * y as f32 + 0.015 * crate::test_vectors::noise((y * 64 + x) * 3 + c) + if x > 31 { 0.08 } else { 0.0 }
            })
        });
        crate::test_vectors::compare("dual/mask.f32", &upstream_detail_mask(&img, 0.2).data, 2e-7);
        crate::test_vectors::compare("dual/mask.f32", &detail_mask(&img, 0.2).data, 1.0);
    }
}
