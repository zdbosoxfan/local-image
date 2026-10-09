//! darktable `src/iop/hazeremoval.c`, `src/common/guided_filter.c` and
//! `src/common/box_filters.cc`, at 733bd69f32cac7ff5e41025115942772add1f088.
//! Copyright (C) 2017–2025 / 2017–2026 / 2009–2026 darktable developers,
//! GPL-3.0-or-later; see licenses/darktable-NOTICE.md.
//!
//! Modern (non-compatibility) quantiles, adaptive w1/w2, RGB guidance with Cramer's rule,
//! Kahan box means normalized by the in-image count, morphological transmission closing,
//! ambient light and depth limit follow upstream. Dehaze/100 maps to strength and its
//! absolute value to distance. Both signs are prepared because masks can combine them.
//! Guided filtering is linear in transmission, so filtering the normalized dark channel
//! once is equivalent to filtering 1-strength*dark for every slider value.

use crate::for_rows;
use lightcraft_raster::blur::min_filter;
use lightcraft_raster::{Plane, Rgb32f};

pub const HAZE_EPS: f32 = 0.025;

#[derive(Clone)]
pub struct Haze {
    pub positive: Plane,
    pub negative: Plane,
    pub air: [f32; 3],
    pub distance: f32,
}
impl Haze {
    pub fn at(&self, i: usize, strength: f32) -> f32 {
        if strength < 0.0 { self.negative.data[i] } else { self.positive.data[i] }
    }
}

pub fn windows(scale: f32) -> (usize, usize) {
    let s = scale.clamp(0.0, 1.0);
    (2 + (4.0 * s).ceil() as usize, 3 + (6.0 * s).ceil() as usize)
}
fn max_filter(p: &Plane, r: usize) -> Plane {
    min_filter(&p.map(|v| -v), r).map(|v| -v)
}
// Term-for-term modern upstream median-of-three quick_select. Keeping its partition
// ordering matters when the dark channel contains repeated window minima.
fn quantile(mut v: Vec<f32>, p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let nth = ((v.len() as f32 * p) as usize).min(v.len() - 1);
    let (mut first, mut last) = (0, v.len());
    while last > first + 1 {
        let pivot = last - 1;
        if v[first] >= v[pivot] {
            v.swap(first, pivot);
        }
        if v[first] >= v[nth] {
            v.swap(first, nth);
        }
        if v[pivot] >= v[nth] {
            v.swap(pivot, nth);
        }
        let val = v[pivot];
        let (mut a, mut b) = (first, last);
        let new_pivot = loop {
            a += 1;
            while a < b && v[a] < val {
                a += 1;
            }
            b -= 1;
            while a < b && v[b] > val {
                b -= 1;
            }
            if a >= b {
                break a;
            }
            v.swap(a, b);
        };
        v.swap(pivot, new_pivot);
        if nth == new_pivot {
            break;
        }
        if nth < new_pivot {
            last = new_pivot;
        } else {
            first = new_pivot + 1;
        }
    }
    v[nth]
}

/// Modern upstream 95% dark-channel / 95% brightness selection, ambient colour and depth.
pub fn ambient_light(img: &Rgb32f, r: usize) -> ([f32; 3], f32) {
    let dark = min_filter(&img.map(|c| c[0].min(c[1]).min(c[2])), r);
    let crit = quantile(dark.data.clone(), 0.95);
    let mid = img.len() / 2;
    let bright: Vec<f32> = (0..mid)
        .rev()
        .chain(mid..img.len())
        .filter(|&i| dark.data[i] >= crit)
        .map(|i| {
            let c = img.data[i];
            c[0] + c[1] + c[2]
        })
        .collect();
    let crit_b = quantile(bright, 0.95);
    let mut sum = [0.0f64; 3];
    let mut n = 0;
    for (d, c) in dark.data.iter().zip(&img.data) {
        if *d >= crit && c[0] + c[1] + c[2] >= crit_b {
            for k in 0..3 {
                sum[k] += c[k] as f64;
            }
            n += 1;
        }
    }
    let air = sum.map(|v| (v / n.max(1) as f64) as f32);
    let distance = if crit > 0.0 { -1.125 * crit.ln() } else { f32::MAX.ln() * 0.5 };
    (air, distance)
}

#[inline]
fn kahan(sum: &mut f32, error: &mut f32, v: f32) {
    let y = v - *error;
    let t = *sum + y;
    *error = (t - *sum) - y;
    *sum = t;
}

/// Separable box mean with Kahan summation, cropped (not replicated) boundary windows.
/// Channel-interleaved layout also supports the guided filter's 4/9-channel moments.
pub fn box_mean(data: &[f32], w: usize, h: usize, nc: usize, r: usize) -> Vec<f32> {
    if w * h == 0 {
        return vec![];
    }
    let mut tmp = vec![0.0; data.len()];
    for_rows(&mut tmp, w * nc, |y, row| {
        for c in 0..nc {
            let mut sum = 0.0;
            let mut err = 0.0;
            for x in 0..=r.min(w - 1) {
                kahan(&mut sum, &mut err, data[(y * w + x) * nc + c]);
            }
            for x in 0..w {
                let lo = x.saturating_sub(r);
                let hi = (x + r).min(w - 1);
                row[x * nc + c] = sum / (hi - lo + 1) as f32;
                if x >= r {
                    kahan(&mut sum, &mut err, -data[(y * w + x - r) * nc + c]);
                }
                if x + r + 1 < w {
                    kahan(&mut sum, &mut err, data[(y * w + x + r + 1) * nc + c]);
                }
            }
        }
    });
    // Columns are independent; transpose to make each worker own a complete column.
    let mut cols = vec![0.0; data.len()];
    for_rows(&mut cols, h * nc, |x, col| {
        for c in 0..nc {
            let mut sum = 0.0;
            let mut err = 0.0;
            for y in 0..=r.min(h - 1) {
                kahan(&mut sum, &mut err, tmp[(y * w + x) * nc + c]);
            }
            for y in 0..h {
                let lo = y.saturating_sub(r);
                let hi = (y + r).min(h - 1);
                col[y * nc + c] = sum / (hi - lo + 1) as f32;
                if y >= r {
                    kahan(&mut sum, &mut err, -tmp[((y - r) * w + x) * nc + c]);
                }
                if y + r + 1 < h {
                    kahan(&mut sum, &mut err, tmp[((y + r + 1) * w + x) * nc + c]);
                }
            }
        }
    });
    let mut out = vec![0.0; data.len()];
    for_rows(&mut out, w * nc, |y, row| {
        for x in 0..w {
            for c in 0..nc {
                row[x * nc + c] = cols[(x * h + y) * nc + c];
            }
        }
    });
    out
}

/// The upstream covariance solve, including the singular-system threshold.
pub fn guided_solve(mean: [f32; 4], var: [f32; 9], eps: f32) -> [f32; 4] {
    let [inp, r, g, b] = mean;
    let rr = var[3] - r * r + eps;
    let rg = var[4] - r * g;
    let rb = var[5] - r * b;
    let gg = var[6] - g * g + eps;
    let gb = var[7] - g * b;
    let bb = var[8] - b * b + eps;
    let det = rr * (gg * bb - gb * gb) - rg * (rg * bb - rb * gb) + rb * (rg * gb - rb * gg);
    if det.abs() <= 4.0 * f32::EPSILON {
        return [0.0, 0.0, 0.0, inp];
    }
    let cr = var[0] - r * inp;
    let cg = var[1] - g * inp;
    let cb = var[2] - b * inp;
    let ar = (cr * (gg * bb - gb * gb) - rg * (cg * bb - cb * gb) + rb * (cg * gb - cb * gg)) / det;
    let ag = (rr * (cg * bb - cb * gb) - cr * (rg * bb - rb * gb) + rb * (rg * cb - rb * cg)) / det;
    let ab = (rr * (gg * cb - gb * cg) - rg * (rg * cb - rb * cg) + cr * (rg * gb - rb * gg)) / det;
    [ar, ag, ab, inp - ar * r - ag * g - ab * b]
}

pub fn guided_rgb(guide: &Rgb32f, p: &Plane, r: usize, eps: f32) -> Plane {
    let (w, h) = (guide.width, guide.height);
    let n = w * h;
    let mut mean = vec![0.0; n * 4];
    let mut var = vec![0.0; n * 9];
    for i in 0..n {
        let [rr, g, b] = guide.data[i];
        let v = p.data[i];
        mean[i * 4..i * 4 + 4].copy_from_slice(&[v, rr, g, b]);
        var[i * 9..i * 9 + 9].copy_from_slice(&[rr * v, g * v, b * v, rr * rr, rr * g, rr * b, g * g, g * b, b * b]);
    }
    let mean = box_mean(&mean, w, h, 4, r);
    let var = box_mean(&var, w, h, 9, r);
    let mut ab = vec![0.0; n * 4];
    for_rows(&mut ab, w * 4, |y, row| {
        for x in 0..w {
            let i = y * w + x;
            let m = std::array::from_fn(|k| mean[i * 4 + k]);
            let v = std::array::from_fn(|k| var[i * 9 + k]);
            row[x * 4..x * 4 + 4].copy_from_slice(&guided_solve(m, v, eps));
        }
    });
    drop(mean);
    drop(var);
    let ab = box_mean(&ab, w, h, 4, r);
    Plane::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let c = guide.data[i];
        ab[i * 4] * c[0] + ab[i * 4 + 1] * c[1] + ab[i * 4 + 2] * c[2] + ab[i * 4 + 3]
    })
}

pub fn haze_plane(img: &Rgb32f, scale: f32) -> Haze {
    let (w1, w2) = windows(scale);
    let (air, distance) = ambient_light(img, w1);
    // Only guard zero airlight. Black/empty scenes otherwise yield infinities upstream.
    let inv = air.map(|v| 1.0 / v.max(1e-6));
    let dark = img.map(|c| (c[0] * inv[0]).min(c[1] * inv[1]).min(c[2] * inv[2]));
    let positive = max_filter(&min_filter(&dark, w1), w1);
    let negative = min_filter(&max_filter(&dark, w1), w1);
    let positive = guided_rgb(img, &positive, w2, HAZE_EPS);
    let negative = guided_rgb(img, &negative, w2, HAZE_EPS);
    Haze { positive, negative, air, distance }
}

/// The upstream scene-linear inversion, with Dehaze's distance tied to |strength|.
pub fn dehaze_px(c: [f32; 3], strength: f32, dark: f32, air: [f32; 3], distance: f32) -> [f32; 3] {
    let s = strength.clamp(-1.0, 1.0);
    let tmin = (-s.abs() * distance).exp().clamp(1.0 / 1024.0, 1.0);
    let t = (1.0 - s * dark).max(tmin);
    if t == 1.0 { c } else { std::array::from_fn(|k| (c[k] - air[k]) / t + air[k]) }
}
