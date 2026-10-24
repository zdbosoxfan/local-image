//! darktable `src/iop/denoiseprofile.c` (wavelets, Y0U0V0, new VST), `src/common/eaw.c`
//! (`eaw_dn_decompose`, `accumulate`) and `src/common/math.h` (`fast_mexp2f`), at
//! 733bd69f32cac7ff5e41025115942772add1f088. Copyright (C) 2012–2026 / 2017–2024 /
//! 2018–2025 darktable developers, GPL-3.0-or-later; see licenses/darktable-NOTICE.md.
//!
//! Calibrated camera profiles are supplied by SourceInfo; otherwise robust 2x2 high-pass MAD estimates a Poisson slope from image tiles, with
//! darktable's generic a=1e-4 fallback. WB is unity (input is already WB/working RGB).
//! The analysis rows are a luminance row (Rec.2020 Y, with upstream Y0's gain on neutral) and
//! upstream's U0/V0 rows kept orthogonal to neutral, so luminance NR reaches all luminance noise and colour NR
//! neither changes luminance nor tints neutral edges. Each slider sets only its own rows'
//! strength (0.4+1.6*amount) and band force, fading in over its first 10 units.
//! Detail lowers fine-band force; Contrast lowers mid/coarse Y force; Smoothness increases coarse
//! U/V force. U/V force is sqrt(multiplier)/2; Y force is sqrt(multiplier), calibrated for
//! upstream's amplified Y0 row. Thresholds retain upstream's 4*force².
//! Scale selection, VST exponent/fulcrum, BayesShrink and 5x5 edge weights follow upstream.
//! Host extensions: tiny images use safe clamped taps instead of upstream's copy-through;
//! colour-only NR preserves Rec.2020 Y exactly, luminance-only NR preserves channel ratios;
//! neutral (manual/generic) shadow bias is used because no calibrated camera profile exists.

use crate::for_rows;
use li_color::luminance_2020;
use li_raster::RgbFrame;
use lightcraft_develop::DevelopSettings;

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
pub fn estimate_noise(img: &RgbFrame) -> NoiseModel {
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

/// Upstream's unit U0 and V0 rows at unity WB, orthogonal to (1, 1, 1).
const U0: [f32; 3] = [std::f32::consts::FRAC_1_SQRT_2, 0.0, -std::f32::consts::FRAC_1_SQRT_2];
const V0: [f32; 3] = [0.408_248_3, -0.816_496_6, 0.408_248_3];
/// Upstream's Y0 row at unity WB is 3√3·(1, 1, 1): a neutral change of 1 in every channel gives
/// 9√3. The Y row keeps that gain on neutral, which the Y force is calibrated for.
const Y0_NEUTRAL_GAIN: f32 = 15.588_457;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let n = dot(v, v).sqrt().max(1e-12);
    v.map(|x| x / n)
}
fn without(v: [f32; 3], d: [f32; 3]) -> [f32; 3] {
    let k = dot(v, d) / dot(d, d).max(1e-24);
    std::array::from_fn(|i| v[i] - k * d[i])
}

/// The analysis rows for a variance-stabilized space whose luminance gradient is `luma` and whose
/// neutral axis is `neutral`: a Y row along `luma` with upstream's gain on neutral, and U/V rows
/// orthogonal to `neutral`. `None` when brightening neutral does not raise `luma`.
pub fn luma_basis(luma: [f32; 3], neutral: [f32; 3]) -> Option<[[f32; 3]; 3]> {
    let n = unit(neutral);
    let along = dot(luma, n);
    if along.is_nan() || along <= 1e-6 {
        return None;
    }
    let u = unit(without(U0, n));
    let v = unit(without(without(V0, n), u));
    let gain = Y0_NEUTRAL_GAIN / 3f32.sqrt() / along;
    Some([luma.map(|x| x * gain), u, v])
}

/// Strength per slider: `basis` with its Y row divided by Luminance's strength and its U/V rows by
/// Color's, and the inverse.
pub fn strength_matrices(basis: [[f32; 3]; 3], p: &NrParams) -> Option<([[f32; 3]; 3], [[f32; 3]; 3])> {
    let strength = |amount: f32| (0.4 + 1.6 * amount) * 2.5 * p.scale;
    let (y, c) = (strength(p.lum), strength(p.col));
    let to = [basis[0].map(|v| v / y), basis[1].map(|v| v / c), basis[2].map(|v| v / c)];
    let from = li_color::Mat3(to.map(|r| r.map(f64::from))).inverse()?.to_f32();
    Some((to, from))
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
        let (to, from) = working_matrices(params);
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
/// The strength-scaled analysis matrices of working-space (Rec.2020) NR.
pub fn working_matrices(p: &NrParams) -> ([[f32; 3]; 3], [[f32; 3]; 3]) {
    luma_basis(li_color::LUMA_2020, [1.0; 3]).and_then(|b| strength_matrices(b, p)).unwrap_or_else(|| conversion_matrices([1.0; 3]))
}

fn transpose(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| m[c][r]))
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

pub fn eaw_decompose(img: &RgbFrame, level: usize) -> (RgbFrame, RgbFrame, [f32; 3]) {
    let (w, h) = (img.width, img.height);
    let step = 1isize << level;
    let inv_sigma2 = 1.0 / VARF.powi(2 * level as i32);
    let mut coarse = RgbFrame::new(w, h);
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
    let fade = |amount: f32| (amount / 0.1).min(1.0);
    let uv = 0.5 * chroma.sqrt() * fade(p.col);
    [fine.sqrt() * fade(p.lum), uv, uv]
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

/// The profile is in normalized sensor coordinates; the matrix includes as-shot WB,
/// baseline exposure and the source camera conversion. It never reuses sensor coefficients
/// directly on Rec.2020 channels.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseCalibration {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub camera_to_working: [[f32; 3]; 3],
}

/// Exact host variance transform shared by CPU and GPU wavelet pipelines.
#[derive(Clone, Copy, Debug)]
pub struct ProfiledVst {
    channels: [Vst; 3],
    inverse: [[f32; 3]; 3],
    matrix: [[f32; 3]; 3],
    to: [[f32; 3]; 3],
    from: [[f32; 3]; 3],
}
impl ProfiledVst {
    pub fn new(img: &RgbFrame, p: &NrParams, calibration: &NoiseCalibration) -> Option<Self> {
        use li_color::Mat3;
        let matrix = Mat3(calibration.camera_to_working.map(|r| r.map(f64::from)));
        let inverse = matrix.inverse()?.to_f32();
        if !calibration.a.iter().all(|a| a.is_finite() && *a > 0.0) || !calibration.b.iter().all(|b| b.is_finite()) {
            return None;
        }
        // Demosaic/resampling and frames depart from laboratory fits. Adapt amplitude
        // with the robust camera-space estimator, retaining measured channel ratios.
        let observed = estimate_noise(&img.map(|v| mul(inverse, v))).a;
        let nominal = calibration.a.iter().sum::<f32>() / 3.0;
        let amplitude = (observed / nominal).clamp(1.0 / 64.0, 64.0);
        let channels: [Vst; 3] = std::array::from_fn(|c| Vst::new(NoiseModel { a: calibration.a[c] * amplitude, b: calibration.b[c] }, p));
        // The working neutral axis and luminance gradient, carried into the stabilized camera
        // channels at middle grey (the transforms are per channel power laws).
        let neutral = mul(inverse, [1.0; 3]);
        let luma = mul(transpose(calibration.camera_to_working), li_color::LUMA_2020);
        let (mut along, mut gradient) = ([0.0; 3], [0.0; 3]);
        for c in 0..3 {
            let v = channels[c];
            let e = 1.0 - v.p * 0.5;
            let slope = 2.0 / ((2.0 - v.p) * v.a.sqrt()) * e * (0.18 * neutral[c] + v.b).max(1e-6).powf(e - 1.0);
            along[c] = slope * neutral[c];
            gradient[c] = luma[c] / slope;
        }
        let (to, from) = luma_basis(gradient, along).and_then(|b| strength_matrices(b, p)).unwrap_or_else(|| working_matrices(p));
        Some(Self { channels, inverse, matrix: calibration.camera_to_working, to, from })
    }
    pub fn forward(&self, rgb: [f32; 3]) -> [f32; 3] {
        let camera = mul(self.inverse, rgb);
        let v = std::array::from_fn(|c| {
            let v = self.channels[c];
            (camera[c] + v.b).max(0.0).powf(1.0 - v.p * 0.5) * 2.0 / ((2.0 - v.p) * v.a.sqrt())
        });
        mul(self.to, v)
    }
    pub fn backward(&self, yuv: [f32; 3]) -> [f32; 3] {
        let transformed = mul(self.from, yuv);
        let camera = std::array::from_fn(|c| {
            let v = self.channels[c];
            let x = transformed[c].max(0.0);
            let z = (x + (x * x + v.bias * v.wb).max(0.0).sqrt()) * v.a.sqrt() * (2.0 - v.p) * 0.25;
            z.powf(1.0 / (1.0 - v.p * 0.5)) - v.b
        });
        mul(self.matrix, camera)
    }
}
pub fn denoise_profiled(img: &mut RgbFrame, p: &NrParams, calibration: &NoiseCalibration) {
    let Some(vst) = ProfiledVst::new(img, p, calibration) else {
        denoise(img, p);
        return;
    };
    denoise_transform(img, p, |v| vst.forward(v), |v| vst.backward(v));
}

pub fn denoise(img: &mut RgbFrame, p: &NrParams) {
    let vst = Vst::new(estimate_noise(img), p);
    denoise_transform(img, p, |v| vst.forward(v), |v| vst.backward(v));
}
fn denoise_transform(
    img: &mut RgbFrame,
    p: &NrParams,
    forward: impl Fn([f32; 3]) -> [f32; 3] + Sync + Send,
    backward: impl Fn([f32; 3]) -> [f32; 3],
) {
    if img.data.is_empty() {
        return;
    }
    let mut coarse = img.map(forward);
    let mut acc = RgbFrame::new(img.width, img.height);
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
        *c = join(*c, backward(v), p);
    }
}

#[cfg(test)]
mod camera_tests {
    use super::*;
    #[test]
    fn camdata_profiled_denoise_is_camera_space_and_improves_psnr() {
        let mut seed = 7u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as f64 + 1.0) / (u32::MAX as f64 + 2.0)
        };
        let truth = RgbFrame::from_fn(96, 96, |x, y| {
            let v = 0.04 + 0.25 * x as f32 / 96.0 + 0.02 * y as f32 / 96.0;
            [v, v * 0.8, v * 0.6]
        });
        let profile =
            NoiseCalibration { a: [0.002, 0.001, 0.003], b: [0.0; 3], camera_to_working: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };
        let mut noisy = truth.clone();
        for pixel in &mut noisy.data {
            for c in 0..3 {
                let z = (-2.0 * random().ln()).sqrt() * (std::f64::consts::TAU * random()).cos();
                pixel[c] += z as f32 * (profile.a[c] * pixel[c]).sqrt();
            }
        }
        let mse = |a: &RgbFrame| {
            a.data.iter().zip(&truth.data).flat_map(|(p, q)| (0..3).map(move |c| (p[c] - q[c]).powi(2) as f64)).sum::<f64>()
                / (a.data.len() * 3) as f64
        };
        let before = mse(&noisy);
        let params = NrParams { lum: 0.6, col: 0.6, detail: 0.5, contrast: 0.5, col_detail: 0.5, smoothness: 0.5, scale: 1.0 };
        let mut generic = noisy.clone();
        denoise(&mut generic, &params);
        denoise_profiled(&mut noisy, &params, &profile);
        let after = mse(&noisy);
        eprintln!(
            "camera NR PSNR before {:.3}, calibrated {:.3}, estimated {:.3}",
            -10.0 * before.log10(),
            -10.0 * after.log10(),
            -10.0 * mse(&generic).log10()
        );
        assert!(after < before * 0.6, "MSE {before} -> {after}");
        assert!(noisy.data.iter().flatten().all(|v| v.is_finite()));
        // A nontrivial camera conversion must give the same result as filtering camera data
        // and then converting it, up to the working-luminance join policy.
        let matrix = [[1.5, 0.1, -0.2], [-0.1, 0.9, 0.2], [0.0, -0.2, 1.3]];
        let original = truth.map(|v| mul(matrix, v));
        let mut work = original.clone();
        denoise_profiled(&mut work, &params, &NoiseCalibration { camera_to_working: matrix, ..profile });
        let mean_error = work.data.iter().zip(&original.data).flat_map(|(p, q)| (0..3).map(move |c| (p[c] - q[c]).abs() as f64)).sum::<f64>()
            / (work.data.len() * 3) as f64;
        assert!(mean_error < 0.01, "neutral/ramp drift {mean_error}");
        // Colour NR must not tint a neutral (working-space) step edge seen through the camera.
        let edge = RgbFrame::from_fn(96, 48, |x, _| [if x < 48 { 0.05 } else { 0.4 }; 3]);
        let colour = NrParams { lum: 0.3, col: 1.0, ..params };
        let mut out = edge.clone();
        denoise_profiled(&mut out, &colour, &NoiseCalibration { camera_to_working: matrix, ..profile });
        let tint = out.data.iter().map(|c| (c[0] - c[1]).abs().max((c[2] - c[1]).abs()) / c[1].max(1e-6)).fold(0f32, f32::max);
        assert!(tint < 1e-3, "colour NR tinted a neutral edge by {tint}");
    }
    #[test]
    fn camdata_source_defaults_and_profile_cache_key() {
        let info: crate::SourceInfo = serde_json::from_str("{}").unwrap();
        assert!(info.camera_noise.is_none() && info.camera_wb_presets.is_empty());
        let src = std::sync::Arc::new(RgbFrame::new(24, 24));
        let settings = lightcraft_develop::DevelopSettings::default();
        let request = crate::RenderRequest::fit(24, 24);
        let a = crate::plan(&src, &info, &settings, &request).lin_key;
        let mut profiled = info.clone();
        profiled.camera_noise =
            Some(NoiseCalibration { a: [0.001; 3], b: [0.0; 3], camera_to_working: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] });
        let b = crate::plan(&src, &profiled, &settings, &request).lin_key;
        assert_ne!(a, b);
    }
}
