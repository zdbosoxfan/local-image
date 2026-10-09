//! Capture sharpening: Richardson–Lucy deconvolution of the luminance with a Gaussian point
//! spread function (the sensor's and the lens's blur), applied only where the image has
//! structure (a variance mask keeps flat, noisy areas and clipped highlights as they were), the
//! colour following the luminance ratio. The kernel can widen towards the corners (corner boost)
//! where lenses blur more.
//!
//! Ported from darktable's `src/iop/demosaicing/capture.c` (`_capture_sharpen`, `_prepare_blend`,
//! `_modify_blend`, `_blur_mul`, `_blur_div`, `_cs_precalc_gauss_idx`, `_calc_9x9_gauss_coeffs`;
//! GPL-3.0-or-later; the algorithm is Ingo Weyrich's for RawTherapee), see `docs/PORTS.md`.
//! Differences: it runs on the scene-linear Rec.2020 source (darktable: camera RGB inside its
//! demosaic module), luminance uses Rec.2020 weights, the "clipped" test reads the source's own
//! clip level when the caller knows it, and the automatic radius comes from the raw decoder
//! (`lightcraft_raw::capture`).

use lightcraft_raster::{Plane, Rgb32f, par_rows};

/// Kernel σ steps of the per-pixel kernel table (darktable's `CAPTURE_GAUSS_FRACTION`).
const SIGMA_STEP: f32 = 0.01;
/// Below this σ the 5×5 kernel is used (darktable's `CAPTURE_SMALL`).
const SMALL: f32 = 0.66;
/// Luminance floor (darktable's `CAPTURE_YMIN`).
const YMIN: f32 = 0.001;
const NORM_MIN: f32 = 1.52587890625e-05;

/// Capture sharpening parameters, for the image being sharpened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureParams {
    /// Gaussian σ of the blur to undo, in pixels of this image (the sensor radius divided by the
    /// image's downscale).
    pub sigma: f32,
    /// Contrast threshold 0..1: higher protects more of the low-contrast areas (darktable's
    /// "contrast sensitivity", default ≈ 0.4 adjusted for ISO, see [`default_threshold`]).
    pub threshold: f32,
    /// Extra σ towards the corners, 0..1.5.
    pub corner_boost: f32,
    /// Size of the sharp centre 0..1 the corner boost leaves alone.
    pub center: f32,
    /// Deconvolution iterations (darktable's default 8).
    pub iterations: u32,
    /// Pixels with a channel at or above this value are treated as clipped (not sharpened).
    pub clip: Option<f32>,
}

impl Default for CaptureParams {
    fn default() -> Self {
        CaptureParams { sigma: 0.5, threshold: 0.4, corner_boost: 0.0, center: 0.0, iterations: 8, clip: None }
    }
}

/// darktable's contrast threshold for a sensor: 0.4, lower for > 12-bit raw data and low ISO.
pub fn default_threshold(white_level: f32, iso: Option<u32>) -> f32 {
    let mut t = 0.4f32;
    if white_level > 4096.0 {
        t -= 0.07;
    }
    if white_level > 14000.0 {
        t -= 0.01;
    }
    if let Some(iso) = iso {
        t -= 0.012 * (600.0 - iso.clamp(100, 1000) as f32) / 100.0;
    }
    (t * 100.0).trunc() / 100.0
}

/// darktable's iteration count for its iteration slider value.
pub fn iterations_of(slider: u32) -> u32 {
    slider + (0.000065 * (slider as f32).powi(4)) as u32
}

/// The quarter of a 9×9 Gaussian of σ (`k[5·|dy| + |dx|]`), truncated to a disc and normalised
/// (darktable's `_calc_9x9_gauss_coeffs`). σ = 0 is the identity.
fn kernel(sigma: f32) -> [f32; 25] {
    let mut k = [0f32; 25];
    if sigma <= 0.0 {
        k[0] = 1.0;
        return k;
    }
    let range = if sigma < SMALL { 2.5f32 * 2.5 } else { 4.5f32 * 4.5 };
    let temp = -2.0 * sigma * sigma;
    let mut full = [[0f32; 9]; 9];
    let mut sum = 0.0;
    for (yy, row) in full.iter_mut().enumerate() {
        for (xx, v) in row.iter_mut().enumerate() {
            let (dy, dx) = (yy as f32 - 4.0, xx as f32 - 4.0);
            let rad = dy * dy + dx * dx;
            if rad <= range {
                *v = (rad / temp).exp();
                sum += *v;
            }
        }
    }
    for dy in 0..5 {
        for dx in 0..5 {
            k[5 * dy + dx] = full[dy + 4][dx + 4] / sum;
        }
    }
    k
}

/// Per-pixel kernel index (σ / [`SIGMA_STEP`]): the base σ, plus the corner boost, shrinking to
/// 0 within 8 px of the image border (darktable's `_cs_precalc_gauss_idx`).
fn sigma_index(w: usize, h: usize, p: &CaptureParams) -> Vec<u8> {
    let (rw, rh) = (w as f32 / 2.0, h as f32 / 2.0);
    let mdim = rw.min(rh).max(1.0);
    let cboost = 1.0 + 8.0 * p.center * p.center;
    let mut idx = vec![0u8; w * h];
    par_rows(&mut idx, w, |row, out| {
        let fr = row as f32 - rh;
        for (col, o) in out.iter_mut().enumerate() {
            let fc = col as f32 - rw;
            let sc = fr.hypot(fc) / mdim;
            let corr = cboost * p.corner_boost * (sc - 0.5 - p.center).max(0.0).powi(2);
            let border = (h - row - 1).min(w - col - 1).min(col).min(row).min(8);
            let sigma = (p.sigma + corr) * 0.125 * border as f32;
            *o = ((sigma / SIGMA_STEP) as i32).clamp(0, 255) as u8;
        }
    });
    idx
}

/// `out[i] = op(out[i], blur(in)[i])` where `blend[i] > 0`, with each pixel's own kernel.
fn blur_with(
    input: &[f32],
    out: &mut [f32],
    blend: &[f32],
    kernels: &[[f32; 25]],
    idx: &[u8],
    w: usize,
    h: usize,
    op: impl Fn(f32, f32, usize) -> f32 + Sync,
) {
    let small = (SMALL / SIGMA_STEP) as u8;
    par_rows(out, w, |row, out_row| {
        for (col, o) in out_row.iter_mut().enumerate() {
            let i = row * w + col;
            if blend[i] <= 0.0 {
                continue;
            }
            let k = &kernels[idx[i] as usize];
            let bd: isize = if idx[i] < small { 2 } else { 4 };
            let mut val = 0.0f32;
            for dy in -bd..=bd {
                let y = row as isize + dy;
                if y < 0 || y >= h as isize {
                    continue;
                }
                for dx in -bd..=bd {
                    let x = col as isize + dx;
                    if x < 0 || x >= w as isize {
                        continue;
                    }
                    val += k[5 * dy.unsigned_abs() + dx.unsigned_abs()] * input[y as usize * w + x as usize];
                }
            }
            *o = op(*o, val, i);
        }
    });
}

/// Where to sharpen (0..1): structure above the contrast threshold, nothing near clipped or
/// black pixels or at the outermost 2 px (darktable's `_prepare_blend` + `_modify_blend` + the
/// blurred/unblurred mix). Also returns the luminance.
pub fn blend_mask(img: &Rgb32f, p: &CaptureParams) -> (Plane, Plane) {
    let (w, h) = (img.width, img.height);
    let lum: Vec<f32> = img.data.iter().map(|c| lightcraft_color::luminance_2020(*c).max(0.0)).collect();
    // 1. clipped / black pixels exclude their 5×5 (diamond-ish) neighbourhood; borders excluded
    let mut mask = vec![1.0f32; w * h];
    for row in 0..h {
        for col in 0..w {
            let k = row * w + col;
            if !(row > 1 && col > 1 && row + 2 < h && col + 2 < w) {
                mask[k] = 0.0;
                continue;
            }
            let heat = p.clip.is_some_and(|c| img.data[k].iter().any(|v| *v >= c));
            if heat || lum[k] < YMIN {
                for (dy, dxs) in [(-2isize, 1isize), (-1, 2), (0, 2), (1, 2), (2, 1)] {
                    for dx in -dxs..=dxs {
                        mask[(row as isize + dy) as usize * w + (col as isize + dx) as usize] = 0.0;
                    }
                }
            }
        }
    }
    // 2. local coefficient of variation (21-pixel disc) through a sigmoid
    let threshold = 0.6 * p.threshold * p.threshold;
    let tscale = 200.0f32;
    let offset = -2.5 + tscale * threshold / 2.0;
    let mut modified = vec![0f32; w * h];
    par_rows(&mut modified, w, |irow, out| {
        let row = irow.clamp(2, h.saturating_sub(3).max(2));
        for (icol, o) in out.iter_mut().enumerate() {
            let col = icol.clamp(2, w.saturating_sub(3).max(2));
            if row + 2 >= h || col + 2 >= w {
                *o = 0.0;
                continue;
            }
            let (mut sum, mut sq) = (0f32, 0f32);
            let mut add = |y: usize, x: usize| {
                let v = lum[y * w + x];
                sum += v;
                sq += v * v;
            };
            for y in row - 1..row + 2 {
                for x in col - 2..col + 3 {
                    add(y, x);
                }
            }
            for x in col - 1..col + 2 {
                add(row - 2, x);
                add(row + 2, x);
            }
            let ss = (sq - sum * sum / 21.0).max(0.0);
            let sd = (ss / 21.0).sqrt();
            let mean = (sum / 21.0).max(NORM_MIN);
            let t = (1.0 + sd / mean.sqrt()).ln();
            let weight = 1.0 / (1.0 + (offset - tscale * t).exp());
            *o = (mask[irow * w + icol] * 1.01011 * (weight - 0.01)).clamp(0.0, 1.0);
        }
    });
    let unblurred = Plane { width: w, height: h, data: modified };
    let mut blurred = lightcraft_raster::blur::gaussian(&unblurred, 2.0);
    // after the blur, tiny edges keep the unblurred strength
    for (b, u) in blurred.data.iter_mut().zip(&unblurred.data) {
        let wt = 1.0 / (1.0 + (5.0 - 10.0 * (u - *b)).exp());
        *b = (wt * u + (1.0 - wt) * *b).clamp(0.0, 1.0);
    }
    (blurred, Plane { width: w, height: h, data: lum })
}

/// Sharpen `img` in place. Does nothing for σ ≤ 0.2 px, no iterations or images smaller than
/// 9 × 9.
pub fn sharpen(img: &mut Rgb32f, p: &CaptureParams) {
    let (w, h) = (img.width, img.height);
    if p.sigma <= 0.2 || p.iterations == 0 || w < 9 || h < 9 {
        return;
    }
    let (blend, lum) = blend_mask(img, p);
    let idx = sigma_index(w, h, p);
    let kernels: Vec<[f32; 25]> = (0..256).map(|i| kernel(i as f32 * SIGMA_STEP)).collect();
    // Richardson–Lucy: estimate ← estimate · blur(observed / blur(estimate))
    let mut est = lum.data.clone();
    // (pixels left out of the mask keep a neutral ratio; darktable leaves its mask's zeros there)
    let mut ratio = vec![1f32; w * h];
    for _ in 0..p.iterations {
        blur_with(&est, &mut ratio, &blend.data, &kernels, &idx, w, h, |_, v, i| lum.data[i] / v.max(YMIN));
        blur_with(&ratio, &mut est, &blend.data, &kernels, &idx, w, h, |o, v, _| o * v);
    }
    let lum = &lum.data;
    par_rows(&mut img.data, w, |row, out| {
        for (col, px) in out.iter_mut().enumerate() {
            let k = row * w + col;
            let b = blend.data[k];
            if b > 0.0 {
                let new = b.clamp(0.0, 1.0) * (est[k] - lum[k]) + lum[k];
                let f = new / lum[k].max(YMIN);
                *px = px.map(|v| v * f);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grey test chart: discs and bars with sharp edges, lightly textured.
    fn chart(w: usize, h: usize) -> Rgb32f {
        Rgb32f::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32, y as f32);
            let disc = ((fx - 40.0).powi(2) + (fy - 40.0).powi(2)).sqrt() < 22.0;
            let bars = x > 70 && (x / 5) % 2 == 0;
            let v = if disc || bars { 0.6 } else { 0.12 };
            [v * 0.9, v, v * 1.1]
        })
    }

    fn blur(img: &Rgb32f, sigma: f32) -> Rgb32f {
        lightcraft_raster::blur::gaussian(img, sigma)
    }

    /// A vertical step edge (0.12 → 0.6) at x = 40 blurred by an exact Gaussian of σ px.
    fn soft_edge(w: usize, h: usize, sigma: f32) -> Rgb32f {
        let erf = |x: f32| {
            let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
            let y = 1.0 - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_7) * t - 0.284_496_74) * t + 0.254_829_6) * t * (-x * x).exp();
            if x >= 0.0 { y } else { -y }
        };
        Rgb32f::from_fn(w, h, |x, _| {
            let t = 0.5 * (1.0 + erf((x as f32 + 0.5 - 40.0) / (sigma * std::f32::consts::SQRT_2)));
            let v = 0.12 + 0.48 * t;
            [v * 0.9, v, v * 1.1]
        })
    }

    #[test]
    fn deconvolution_restores_edges() {
        let soft = soft_edge(96, 48, 1.0);
        let mut out = soft.clone();
        sharpen(&mut out, &CaptureParams { sigma: 1.0, threshold: 0.2, iterations: 20, ..Default::default() });
        let slope = |img: &Rgb32f| (30..50).map(|x| img.get(x + 1, 24)[1] - img.get(x, 24)[1]).fold(0.0f32, f32::max);
        let (before, after) = (slope(&soft), slope(&out));
        assert!(after > before * 1.3, "steepest step {before} → {after}");
        // bounded overshoot, and far from the edge nothing changes
        let row: Vec<f32> = (0..96).map(|x| out.get(x, 24)[1]).collect();
        assert!(row.iter().all(|v| (0.12 - 0.05..=0.6 + 0.07).contains(v)), "{row:?}");
        assert!((out.get(80, 24)[1] - soft.get(80, 24)[1]).abs() < 1e-4);
        // colour follows the luminance: ratios kept
        let p = out.get(40, 24);
        assert!((p[0] / p[1] - 0.9).abs() < 1e-3 && (p[2] / p[1] - 1.1).abs() < 1e-3, "{p:?}");
        assert!(out.data.iter().all(|p| p.iter().all(|v| v.is_finite() && *v >= 0.0)));
    }

    #[test]
    fn flat_areas_borders_and_clipped_pixels_stay() {
        // flat with faint noise: below the contrast threshold
        let flat = Rgb32f::from_fn(64, 64, |x, y| {
            let n = ((x * 31 + y * 17) % 7) as f32 * 0.0002;
            [0.3 + n, 0.3 + n, 0.3 + n]
        });
        let mut out = flat.clone();
        sharpen(&mut out, &CaptureParams { threshold: 0.4, ..Default::default() });
        let max = out.data.iter().zip(&flat.data).map(|(a, b)| (a[1] - b[1]).abs()).fold(0.0f32, f32::max);
        assert!(max < 2e-4, "{max}");
        // clipped area untouched, as is the border
        let mut img = blur(&chart(64, 64), 1.0);
        for p in img.data.iter_mut().take(64 * 32) {
            *p = p.map(|v| v * 4.0);
        }
        let before = img.clone();
        sharpen(&mut img, &CaptureParams { sigma: 1.0, clip: Some(1.0), ..Default::default() });
        assert_eq!(img.get(40, 20), before.get(40, 20), "clipped disc");
        assert_eq!(img.get(0, 50), before.get(0, 50), "border");
        // σ too small or nothing to do
        let mut same = before.clone();
        sharpen(&mut same, &CaptureParams { sigma: 0.1, ..Default::default() });
        assert_eq!(same.data, before.data);
    }

    #[test]
    fn kernels_and_helpers() {
        for s in [0.0, 0.3, 0.7, 1.5] {
            let k = kernel(s);
            // the quarter kernel expands to a full kernel that sums to 1
            let mut sum = 0.0;
            for dy in -4i32..=4 {
                for dx in -4i32..=4 {
                    sum += k[5 * dy.unsigned_abs() as usize + dx.unsigned_abs() as usize];
                }
            }
            assert!((sum - 1.0).abs() < 1e-5, "σ {s}: {sum}");
        }
        assert_eq!(iterations_of(8), 8);
        assert_eq!(iterations_of(25), 50);
        assert!((default_threshold(16383.0, Some(100)) - 0.26).abs() < 1e-6);
        assert!((default_threshold(4095.0, Some(1000)) - 0.44).abs() < 1e-6);
        // the corner boost raises σ towards the corners only
        let idx = sigma_index(200, 100, &CaptureParams { sigma: 0.5, corner_boost: 1.0, ..Default::default() });
        assert_eq!(idx[50 * 200 + 100], 50);
        assert!(idx[10 * 200 + 10] > 50);
        assert_eq!(idx[0], 0);
    }
}
