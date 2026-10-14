//! Perceptual colour models: OkLab / OkLCh (Björn Ottosson, 2020), HSL and HSV.

use std::sync::OnceLock;

use crate::{Mat3, REC2020};

const XYZ_TO_LMS: Mat3 = Mat3([
    [0.818_933_010_1, 0.361_866_742_4, -0.128_859_713_7],
    [0.032_984_543_6, 0.929_311_871_5, 0.036_145_638_7],
    [0.048_200_301_8, 0.264_366_269_1, 0.633_851_707_0],
]);
const LMS_TO_LAB: Mat3 = Mat3([
    [0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_8],
    [1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_9],
    [0.025_904_037_1, 0.782_771_766_2, -0.808_675_766_0],
]);

struct OkMats {
    to_lms: [[f32; 3]; 3],
    from_lms: [[f32; 3]; 3],
    to_lab: [[f32; 3]; 3],
    from_lab: [[f32; 3]; 3],
}

fn mats() -> &'static OkMats {
    static M: OnceLock<OkMats> = OnceLock::new();
    M.get_or_init(|| {
        let to_lms = XYZ_TO_LMS.mul(&REC2020.to_xyz());
        OkMats {
            to_lms: to_lms.to_f32(),
            // constant, invertible matrices (round trips tested below): the identity fallback can't happen
            from_lms: to_lms.inverse().unwrap_or(Mat3::IDENTITY).to_f32(),
            to_lab: LMS_TO_LAB.to_f32(),
            from_lab: LMS_TO_LAB.inverse().unwrap_or(Mat3::IDENTITY).to_f32(),
        }
    })
}

#[inline]
fn mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// The f32 matrices [`oklab_from_2020`] and [`oklab_to_2020`] use (row-major): Rec.2020 → LMS,
/// LMS → Rec.2020, cube-rooted LMS → Lab, Lab → cube-rooted LMS. GPU kernels embed these.
pub fn oklab_matrices() -> [[[f32; 3]; 3]; 4] {
    let m = mats();
    [m.to_lms, m.from_lms, m.to_lab, m.from_lab]
}

/// Linear Rec.2020 → OkLab `[L, a, b]`.
#[inline]
pub fn oklab_from_2020(rgb: [f32; 3]) -> [f32; 3] {
    let m = mats();
    let lms = mul(&m.to_lms, rgb).map(f32::cbrt);
    mul(&m.to_lab, lms)
}

/// OkLab → linear Rec.2020.
#[inline]
pub fn oklab_to_2020(lab: [f32; 3]) -> [f32; 3] {
    let m = mats();
    let lms = mul(&m.from_lab, lab).map(|v| v * v * v);
    mul(&m.from_lms, lms)
}

/// OkLab → OkLCh (hue in radians, −π..π).
#[inline]
pub fn lab_to_lch(lab: [f32; 3]) -> [f32; 3] {
    [lab[0], lab[1].hypot(lab[2]), lab[2].atan2(lab[1])]
}

#[inline]
pub fn lch_to_lab(lch: [f32; 3]) -> [f32; 3] {
    let (s, c) = lch[2].sin_cos();
    [lch[0], lch[1] * c, lch[1] * s]
}

/// RGB (0..1) → HSV (h in degrees 0..360, s, v 0..1).
pub fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    rgb_to_hsv_unit(c, HueUnit::Degrees)
}
/// RGB → HSV with hue in turns, avoiding a degree round trip and its rounding error.
pub fn rgb_to_hsv_turns(c: [f32; 3]) -> [f32; 3] {
    rgb_to_hsv_unit(c, HueUnit::Turns)
}
#[derive(Clone, Copy)]
enum HueUnit {
    Degrees,
    Turns,
}
impl HueUnit {
    fn sectors(self, h: f32) -> f32 {
        match self {
            Self::Degrees => h.rem_euclid(360.0) / 60.0,
            Self::Turns => h.rem_euclid(1.0) * 6.0,
        }
    }
    fn scale_sectors(self, h: f32) -> f32 {
        match self {
            Self::Degrees => h * 60.0,
            Self::Turns => h / 6.0,
        }
    }
}
fn rgb_to_hsv_unit(c: [f32; 3], unit: HueUnit) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    [unit.scale_sectors(hue_sectors(c, max, d)), if max > 0.0 { d / max } else { 0.0 }, max]
}
/// Canonical Develop/display HSV inverse, retaining its calibrated chroma arithmetic.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    hsv_rgb(HueUnit::Degrees.sectors(h), s, v, false)
}
/// Calibration-profile HSV inverse (degrees); product arithmetic preserves DNG reference bits.
pub fn hsv_to_rgb_profile(h: f32, s: f32, v: f32) -> [f32; 3] {
    hsv_rgb(HueUnit::Degrees.sectors(h), s, v, true)
}
/// Brush HSV inverse (turns). Product arithmetic avoids a redundant degree conversion.
pub fn hsv_turns_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    hsv_rgb(HueUnit::Turns.sectors(h), s, v, true)
}
// A single sector permutation. The algebra policy retains calibrated floating-point operation
// order: product form is more accurate for native turn-valued brush inputs; chroma form is the
// existing Develop/display contract. Neither caller needs a private colour formula.
fn hsv_rgb(hp: f32, s: f32, v: f32, product: bool) -> [f32; 3] {
    let (high, low, rising, falling) = if product {
        // rem_euclid can round a tiny negative hue up to the endpoint. Keep that
        // endpoint in the last sector, as calibrated DNG profiles require.
        let f = hp - (hp as u32).min(5) as f32;
        (v, v * (1.0 - s), v * (1.0 - s * (1.0 - f)), v * (1.0 - s * f))
    } else {
        let c = v * s;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let m = v - c;
        (c + m, 0.0 + m, x + m, x + m)
    };
    hue_permute(hp, high, low, rising, falling)
}
// Native precision and remainder policy preserve both Develop's f32 channel
// arithmetic and Affinity's f64 HSL records without separate colour formulas.
trait ChromaFloat:
    Copy + std::ops::Add<Output = Self> + std::ops::Sub<Output = Self> + std::ops::Mul<Output = Self> + std::ops::Div<Output = Self>
{
    fn constant(v: f64) -> Self;
    fn abs(self) -> Self;
    fn rem_euclid(self, divisor: Self) -> Self;
    fn remainder(self, divisor: Self) -> Self;
    fn sector(self) -> u32;
}
impl ChromaFloat for f32 {
    fn constant(v: f64) -> Self {
        v as f32
    }
    fn abs(self) -> Self {
        self.abs()
    }
    fn rem_euclid(self, divisor: Self) -> Self {
        self.rem_euclid(divisor)
    }
    fn remainder(self, divisor: Self) -> Self {
        self % divisor
    }
    fn sector(self) -> u32 {
        self as u32
    }
}
impl ChromaFloat for f64 {
    fn constant(v: f64) -> Self {
        v
    }
    fn abs(self) -> Self {
        self.abs()
    }
    fn rem_euclid(self, divisor: Self) -> Self {
        self.rem_euclid(divisor)
    }
    fn remainder(self, divisor: Self) -> Self {
        self % divisor
    }
    fn sector(self) -> u32 {
        self as u32
    }
}
fn hue_permute<T: ChromaFloat>(hp: T, high: T, low: T, rising: T, falling: T) -> [T; 3] {
    match hp.sector() {
        0 => [high, rising, low],
        1 => [falling, high, low],
        2 => [low, high, rising],
        3 => [low, falling, high],
        4 => [rising, low, high],
        _ => [high, low, falling],
    }
}
fn hsl_chroma<T: ChromaFloat>(s: T, l: T) -> T {
    (T::constant(1.0) - (T::constant(2.0) * l - T::constant(1.0)).abs()) * s
}
/// RGB (0..1) → HSL (h degrees, s, l), retaining Develop's denominator floor.
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    rgb_to_hsl_inner(c, HueUnit::Degrees, 0.0, Some(1e-9), false)
}
/// Adjustment HSL with hue in turns. The near-grey threshold is the existing editor policy.
pub fn rgb_to_hsl_turns(c: [f32; 3]) -> [f32; 3] {
    rgb_to_hsl_inner(c, HueUnit::Turns, 1e-7, None, false)
}
/// HSL channel encoding with its historical bounded saturation policy.
pub fn rgb_to_hsl_channels(c: [f32; 3]) -> [f32; 3] {
    rgb_to_hsl_inner(c, HueUnit::Turns, 0.0, Some(1e-7), true)
}
fn rgb_to_hsl_inner(c: [f32; 3], unit: HueUnit, achromatic: f32, floor: Option<f32>, clamp_s: bool) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let l = (max + min) / 2.0;
    if d <= 0.0 || d.abs() < achromatic {
        return [0.0, 0.0, l];
    }
    let denominator = match floor {
        Some(floor) => (1.0 - (2.0 * l - 1.0).abs()).max(floor),
        None if l > 0.5 => 2.0 - max - min,
        None => max + min,
    };
    let s = d / denominator;
    [unit.scale_sectors(hue_sectors(c, max, d)), if clamp_s { s.clamp(0.0, 1.0) } else { s }, l]
}
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = hsl_chroma(s, l);
    let v = l + c / 2.0;
    hsv_to_rgb(h, if v > 0.0 { c / v } else { 0.0 }, v)
}
/// HSL inverse in native turns, using direct chroma rather than three shifted hue curves.
pub fn hsl_turns_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = hsl_chroma(s, l);
    rgb_from_chroma_turns(h, c, l - c / 2.0)
}
/// Native f64 HSL record conversion, with hue expressed in turns.
pub fn hsl_turns_to_rgb_f64(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = hsl_chroma(s, l);
    rgb_from_chroma_turns_kernel(h, c, l - c / 2.0, true)
}
/// HSV/HSB channel inverse in native turns, using the channel filter's chroma arithmetic.
pub fn hsv_channels_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    rgb_from_chroma_turns(h, c, v - c)
}
fn rgb_from_chroma_turns(h: f32, c: f32, m: f32) -> [f32; 3] {
    rgb_from_chroma_turns_kernel(h, c, m, false)
}
fn rgb_from_chroma_turns_kernel<T: ChromaFloat>(h: T, c: T, m: T, signed_remainder: bool) -> [T; 3] {
    let value = T::constant;
    let hp = h.rem_euclid(value(1.0)) * value(6.0);
    let rem = if signed_remainder { hp.remainder(value(2.0)) } else { hp.rem_euclid(value(2.0)) };
    let x = c * (value(1.0) - (rem - value(1.0)).abs());
    hue_permute(hp, c + m, value(0.0) + m, x + m, x + m)
}
/// Hue in degrees, shared by camera-profile adapters; grey inputs use hue zero.
pub fn hue_degrees(c: [f32; 3], max: f32, d: f32) -> f32 {
    HueUnit::Degrees.scale_sectors(hue_sectors(c, max, d))
}
fn hue_sectors(c: [f32; 3], max: f32, d: f32) -> f32 {
    hue_sectors_with_wrap(c, max, d, true)
}
fn hue_sectors_with_wrap(c: [f32; 3], max: f32, d: f32, wrap_red: bool) -> f32 {
    if d <= 0.0 {
        return 0.0;
    }
    if max == c[0] {
        let h = (c[1] - c[2]) / d;
        if wrap_red { h.rem_euclid(6.0) } else { h }
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    }
}

/// Vectorscope HSV policy: epsilon-grey guard and a final native-turn wrap.
/// Wrapping after division retains the analysis histogram's original bin edges.
pub fn rgb_to_hsv_scope_turns(c: [f32; 3]) -> [f32; 3] {
    let high = c[0].max(c[1]).max(c[2]);
    let low = c[0].min(c[1]).min(c[2]);
    let delta = high - low;
    if delta <= f32::EPSILON || high <= 0.0 {
        return [0.0, 0.0, high];
    }
    [(hue_sectors_with_wrap(c, high, delta, false) / 6.0).rem_euclid(1.0), delta / high, high]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_white_and_grey() {
        let w = oklab_from_2020([1.0, 1.0, 1.0]);
        assert!((w[0] - 1.0).abs() < 2e-3 && w[1].abs() < 2e-3 && w[2].abs() < 2e-3, "{w:?}");
        let g = oklab_from_2020([0.18, 0.18, 0.18]);
        assert!(g[1].abs() < 2e-3 && g[2].abs() < 2e-3);
    }

    #[test]
    fn oklab_roundtrip() {
        for c in [[0.1, 0.5, 0.9], [1.0, 0.0, 0.0], [0.02, 0.03, 0.01], [2.0, 1.5, 0.3]] {
            let back = oklab_to_2020(oklab_from_2020(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-4 * c[i].max(1.0), "{c:?} {back:?}");
            }
        }
        let lch = lab_to_lch([0.5, 0.1, -0.05]);
        let lab = lch_to_lab(lch);
        assert!((lab[1] - 0.1).abs() < 1e-6 && (lab[2] + 0.05).abs() < 1e-6);
    }

    #[test]
    fn hsv_hsl_roundtrip() {
        for c in [[0.2, 0.4, 0.6], [1.0, 0.0, 0.0], [0.5, 0.5, 0.5], [0.9, 0.8, 0.1]] {
            let h = rgb_to_hsv(c);
            let r = hsv_to_rgb(h[0], h[1], h[2]);
            let l = rgb_to_hsl(c);
            let r2 = hsl_to_rgb(l[0], l[1], l[2]);
            for i in 0..3 {
                assert!((r[i] - c[i]).abs() < 1e-5 && (r2[i] - c[i]).abs() < 1e-5, "{c:?}");
            }
        }
        assert_eq!(rgb_to_hsv([0.0, 1.0, 0.0])[0], 120.0);
    }
}

#[cfg(test)]
#[path = "residual_tests.rs"]
mod residual_tests;
