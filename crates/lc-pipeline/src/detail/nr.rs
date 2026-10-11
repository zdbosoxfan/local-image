//! darktable `src/iop/denoiseprofile.c` (wavelets, Y0U0V0, new VST), `src/common/eaw.c`
//! (`eaw_dn_decompose`, `accumulate`) and `src/common/math.h` (`fast_mexp2f`), at
//! 733bd69f32cac7ff5e41025115942772add1f088. Copyright (C) 2012–2026 / 2017–2024 /
//! 2018–2025 darktable developers, GPL-3.0-or-later; see licenses/darktable-NOTICE.md.
//!
//! No camera DB: robust 2x2 high-pass MAD estimates a Poisson slope from image tiles, with
//! darktable's generic a=1e-4 fallback. WB is unity (input is already WB/working RGB).
//! Amount controls strength (0.4+1.6*max amounts); the smaller amount attenuates that channel's
//! band force. Detail lowers fine-band force; Contrast lowers mid/coarse Y0 force; Smoothness
//! increases coarse U0V0 force. U0V0 force is sqrt(multiplier)/2; Y0 force is sqrt(multiplier),
//! calibrated for upstream's amplified Y0 row. Thresholds retain upstream's 4*force².
//! Scale selection, VST exponent/fulcrum, BayesShrink and 5x5 edge weights follow upstream.
//! Host extensions: tiny images use safe clamped taps instead of upstream's copy-through;
//! colour-only NR preserves Rec.2020 Y exactly, luminance-only NR preserves channel ratios;
//! neutral (manual/generic) shadow bias is used because no calibrated camera profile exists.

use crate::for_rows;
use lightcraft_color::luminance_2020;
use lightcraft_develop::DevelopSettings;
use lightcraft_raster::Rgb32f;

pub const NR_MAX_LEVELS: usize = 7;
pub const VARF: f32 = 0.5229125; // sqrt(70)/16, upstream's wavelet noise propagation

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NrParams {
    pub lum: f32,
    pub detail: f32,
    pub contrast: f32,
    pub col: f32,
    pub col_detail: f32,
    pub smoothness: f32,
    pub scale: f32,
}

pub fn nr_params(s: &DevelopSettings, scale: f32) -> Option<NrParams> {
    if !s.section_enabled("detail") {
        return None;
    }
    let f = |v: f64| (v / 100.0).clamp(0.0, 1.0) as f32;
    let d = &s.detail;
    let (lum, col) = (f(d.nr_luminance), f(d.nr_color));
    (lum > 0.0 || col > 0.0).then_some(NrParams {
        lum,
        col,
        detail: f(d.nr_detail),
        contrast: f(d.nr_contrast),
        col_detail: f(d.nr_color_detail),
        smoothness: f(d.nr_color_smoothness),
        scale: scale.clamp(1.0 / 64.0, 1.0),
    })
}

/// Upstream's maximum visible wavelet scale at the original image size and pipe scale.
pub fn max_scale(w: usize, h: usize, scale: f32) -> usize {
    let supp0 = (2 * (2 << (NR_MAX_LEVELS - 1)) + 1) as f32;
    let supp0 = supp0.min(w.max(h) as f32 / scale * 0.2).max(5.0);
    let i0 = ((supp0 - 1.0) * 0.5).log2();
    (0..NR_MAX_LEVELS)
        .take_while(|j| {
            let supp = (2 * (2 << j) + 1) as f32;
            let i_in = ((supp / scale - 1.0) * 0.5).log2() - 1.0;
            1.0 - (i_in + 0.5) / i0 >= 0.0
        })
        .count()
        .max(1)
}

#[derive(Clone, Copy, Debug)]
pub struct NoiseModel {
    pub a: f32,
    pub b: f32,
}

fn quantile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let k = ((v.len() as f32 * p) as usize).min(v.len() - 1);
    v.select_nth_unstable_by(k, f32::total_cmp);
    v[k]
}

/// A robust image model. The 2x2 diagonal high-pass cancels ramps; its squared tap norm is 1.
/// Pooling the three channels handles chroma-only noise too. The lower tile quartile rejects
/// texture; a slope is meaningful in working RGB, without camera-specific WB/profile metadata.
pub fn estimate_noise(img: &Rgb32f) -> NoiseModel {
    let mut slopes = Vec::new();
    let (w, h) = (img.width, img.height);
    for ty in 0..4 {
        for tx in 0..4 {
            let (x0, x1, y0, y1) = (tx * w / 4, ((tx + 1) * w / 4).min(tx * w / 4 + 64), ty * h / 4, ((ty + 1) * h / 4).min(ty * h / 4 + 64));
            let mut samples = Vec::new();
            for y in y0..y1.saturating_sub(1) {
                for x in x0..x1.saturating_sub(1) {
                    let a = img.get(x, y);
                    let b = img.get(x + 1, y);
                    let c = img.get(x, y + 1);
                    let d = img.get(x + 1, y + 1);
                    for k in 0..3 {
                        let mean = ((a[k] + b[k] + c[k] + d[k]) * 0.25).max(1e-5);
                        let hp = (a[k] - b[k] - c[k] + d[k]) * 0.5;
                        if hp.is_finite() && mean.is_finite() {
                            samples.push(hp.abs() / mean.sqrt());
                        }
                    }
                }
            }
            if samples.len() >= 24 {
                let sd = quantile(&mut samples, 0.5) / 0.67448975;
                slopes.push(sd * sd);
            }
        }
    }
    let a = if slopes.is_empty() { 1e-4 } else { quantile(&mut slopes, 0.25).clamp(1e-8, 0.02) };
    NoiseModel { a, b: 0.0 }
}

/// Conversion matrices, including upstream's WB-adaptive Y0 row (retained term for term).
pub fn conversion_matrices(wb: [f32; 3]) -> ([[f32; 3]; 3], [[f32; 3]; 3]) {
    let sum_inv = (1.0 / wb[0] + 1.0 / wb[1] + 1.0 / wb[2]) * 3.0f32.sqrt();
    let su = (0.25 * wb[0] * wb[0] + 0.25 * wb[2] * wb[2]).sqrt();
    let sv = (0.0625 * wb[0] * wb[0] + 0.25 * wb[1] * wb[1] + 0.0625 * wb[2] * wb[2]).sqrt();
    let m = [[sum_inv / wb[0], sum_inv / wb[1], sum_inv / wb[2]], [0.5 / su, 0.0, -0.5 / su], [0.25 / sv, -0.5 / sv, 0.25 / sv]];
    let a = m[1][1] * m[2][2] - m[1][2] * m[2][1];
    let b = -m[1][0] * m[2][2] + m[1][2] * m[2][0];
    let c = m[1][0] * m[2][1] - m[1][1] * m[2][0];
    let d = -m[0][1] * m[2][2] + m[0][2] * m[2][1];
    let e = m[0][0] * m[2][2] - m[0][2] * m[2][0];
    let f = -m[0][0] * m[2][1] + m[0][1] * m[2][0];
    let g = m[0][1] * m[1][2] - m[0][2] * m[1][1];
    let h = -m[0][0] * m[1][2] + m[0][2] * m[1][0];
    let i = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    let det = m[0][0] * a + m[0][1] * b + m[0][2] * c;
    (m, [[a / det, d / det, g / det], [b / det, e / det, h / det], [c / det, f / det, i / det]])
}

#[derive(Clone, Copy, Debug)]
pub struct Vst {
    pub a: f32,
    pub b: f32,
    pub p: f32,
    pub bias: f32,
    pub wb: f32,
    pub to: [[f32; 3]; 3],
    pub from: [[f32; 3]; 3],
}
impl Vst {
    pub fn new(model: NoiseModel, params: &NrParams) -> Self {
        let shadows = (0.1 - 0.1 * model.a.ln()).clamp(0.7, 1.8);
        let p = (shadows + 0.1 * params.scale.ln()).max(0.0);
        // Image-derived profiles have no calibrated shadow bias; upstream's generic/manual
        // neutral bias avoids a spurious brightness shift from auto camera-profile inference.
        let bias = -0.5 * params.scale.ln();
        let wb = (0.4 + 1.6 * params.lum.max(params.col)) * 2.5 * params.scale;
        let (mut to, mut from) = conversion_matrices([1.0; 3]);
        for row in &mut to {
            for v in row {
                *v /= wb;
            }
        }
        for row in &mut from {
            for v in row {
                *v *= wb;
            }
        }
        Self { a: model.a * 0.05 / 0.05f32.powf(shadows), b: model.b, p, bias, wb, to, from }
    }
    pub fn forward(&self, rgb: [f32; 3]) -> [f32; 3] {
        let scale = 2.0 / ((2.0 - self.p) * self.a.sqrt());
        let v = rgb.map(|v| (v + self.b).max(0.0).powf(1.0 - self.p * 0.5) * scale);
        mul(self.to, v)
    }
    pub fn backward(&self, yuv: [f32; 3]) -> [f32; 3] {
        let v = mul(self.from, yuv);
        let scale = self.a.sqrt() * (2.0 - self.p) * 0.25;
        v.map(|v| {
            let x = v.max(0.0);
            let z = (x + (x * x + self.bias * self.wb).max(0.0).sqrt()) * scale;
            z.powf(1.0 / (1.0 - self.p * 0.5)) - self.b
        })
    }
}
fn mul(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}

/// Deliberately uses upstream's historical f32 bit interpolation, including its reduced precision.
pub fn fast_mexp2f(x: f32) -> f32 {
    let i1 = 0x3f800000u32 as f32;
    let i2 = 0x3f000000u32 as f32;
    let k = i1 + x * (i2 - i1);
    f32::from_bits(if k >= 0x800000u32 as f32 { k as u32 } else { 0 })
}

pub fn eaw_decompose(img: &Rgb32f, level: usize) -> (Rgb32f, Rgb32f, [f32; 3]) {
    let (w, h) = (img.width, img.height);
    let step = 1isize << level;
    let inv_sigma2 = 1.0 / VARF.powi(2 * level as i32);
    let mut coarse = Rgb32f::new(w, h);
    const F: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    for_rows(&mut coarse.data, w, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            let center = img.get(x, y);
            let mut sum = [0.0; 3];
            let mut weight = 0.0;
            for j in 0..5 {
                let yy = (y as isize + (j as isize - 2) * step).clamp(0, h as isize - 1) as usize;
                for k in 0..5 {
                    let xx = (x as isize + (k as isize - 2) * step).clamp(0, w as isize - 1) as usize;
                    let v = img.get(xx, yy);
                    let sq = std::array::from_fn::<_, 3, _>(|c| (center[c] - v[c]).powi(2));
                    let dot = (sq[0] + sq[1] + sq[2]) * inv_sigma2;
                    let ww = F[j] * F[k] * fast_mexp2f((dot * 0.02 - 9.0).max(0.0));
                    weight += ww;
                    for c in 0..3 {
                        sum[c] += ww * v[c];
                    }
                }
            }
            *o = sum.map(|v| v / weight);
        }
    });
    let detail = img.zip_map(&coarse, |a, b| std::array::from_fn(|c| a[c] - b[c]));
    // Deterministic wide accumulation avoids image-size-dependent f32 summation loss.
    let mut sum = [0.0f64; 3];
    for d in &detail.data {
        for c in 0..3 {
            sum[c] += (d[c] * d[c]) as f64;
        }
    }
    (coarse, detail, sum.map(|v| v as f32))
}

pub fn band_force(p: &NrParams, level: usize, _levels: usize) -> [f32; 3] {
    let original = level + (1.0 / p.scale).log2().round() as usize;
    let fine = match original {
        0 => 1.2 - 0.8 * p.detail,
        1 => 1.1 - 0.6 * p.detail,
        _ => 1.0 - 0.85 * p.contrast,
    };
    let chroma = match original {
        0 => 1.2 - 0.8 * p.col_detail,
        1 => 1.1 - 0.6 * p.col_detail,
        _ => 0.5 + 1.5 * p.smoothness,
    };
    let max = p.lum.max(p.col).max(1e-6);
    [(p.lum / max * fine).sqrt(), 0.5 * (p.col / max * chroma).sqrt(), 0.5 * (p.col / max * chroma).sqrt()]
}

/// Upstream variance_stabilizing_xform, Y0U0V0 branch (force is one value per channel).
pub fn thresholds(level: usize, npixels: usize, sum: [f32; 3], force: [f32; 3]) -> [f32; 3] {
    let sb2 = VARF.powi(level as i32).powi(2);
    std::array::from_fn(|c| {
        let vy = sum[c] / (npixels as f32 - 1.0).max(1.0);
        let sx = (vy - sb2).max(1e-6).sqrt();
        8.0 * (force[c] * force[c] * 4.0) * sb2 / sx
    })
}

#[inline]
pub fn soft_threshold(d: f32, t: f32) -> f32 {
    (d + t).min(0.0) + (d - t).max(0.0)
}

pub fn join(original: [f32; 3], filtered: [f32; 3], p: &NrParams) -> [f32; 3] {
    let y = luminance_2020(original);
    let fy = luminance_2020(filtered).max(1e-12);
    if p.col == 0.0 {
        if y > 1e-12 { original.map(|v| v * fy / y) } else { original }
    } else if p.lum == 0.0 {
        filtered.map(|v| v * y / fy)
    } else {
        filtered
    }
}

pub fn denoise(img: &mut Rgb32f, p: &NrParams) {
    if img.data.is_empty() {
        return;
    }
    let vst = Vst::new(estimate_noise(img), p);
    let mut coarse = img.map(|c| vst.forward(c));
    let mut acc = Rgb32f::new(img.width, img.height);
    let levels = max_scale(img.width, img.height, p.scale);
    for level in 0..levels {
        let (next, det, sum) = eaw_decompose(&coarse, level);
        let t = thresholds(level, img.data.len(), sum, band_force(p, level, levels));
        for (a, d) in acc.data.iter_mut().zip(det.data) {
            for c in 0..3 {
                a[c] += soft_threshold(d[c], t[c]);
            }
        }
        coarse = next;
    }
    for (i, c) in img.data.iter_mut().enumerate() {
        let v = std::array::from_fn(|k| acc.data[i][k] + coarse.data[i][k]);
        *c = join(*c, vst.backward(v), p);
    }
}
