//! Perceptual conversions for sRGB primaries. These differ from the Develop working-space
//! conversions in [`crate::perceptual`], whose input/output is linear Rec.2020.
//!
//! The f32 and f64 entry points instantiate the same published OkLab formula (Björn Ottosson,
//! 2021). Callers choose precision before decoding; a conversion never silently changes primaries.

use std::ops::{Add, Mul, Sub};

trait Float: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> {
    fn coefficient(value: f64) -> Self;
    fn cbrt(self) -> Self;
    fn cube(self) -> Self;
}
impl Float for f32 {
    fn coefficient(value: f64) -> Self {
        value as Self
    }
    fn cbrt(self) -> Self {
        self.cbrt()
    }
    fn cube(self) -> Self {
        self.powi(3)
    }
}
impl Float for f64 {
    fn coefficient(value: f64) -> Self {
        value
    }
    fn cbrt(self) -> Self {
        self.cbrt()
    }
    fn cube(self) -> Self {
        self.powi(3)
    }
}

fn to_oklab<T: Float>([r, g, b]: [T; 3]) -> [T; 3] {
    let c = T::coefficient;
    let l = (c(0.412_221_470_8) * r + c(0.536_332_536_3) * g + c(0.051_445_992_9) * b).cbrt();
    let m = (c(0.211_903_498_2) * r + c(0.680_699_545_1) * g + c(0.107_396_956_6) * b).cbrt();
    let s = (c(0.088_302_461_9) * r + c(0.281_718_837_6) * g + c(0.629_978_700_5) * b).cbrt();
    [
        c(0.210_454_255_3) * l + c(0.793_617_785_0) * m - c(0.004_072_046_8) * s,
        c(1.977_998_495_1) * l - c(2.428_592_205_0) * m + c(0.450_593_709_9) * s,
        c(0.025_904_037_1) * l + c(0.782_771_766_2) * m - c(0.808_675_766_0) * s,
    ]
}

fn from_oklab<T: Float>([l, a, b]: [T; 3]) -> [T; 3] {
    let c = T::coefficient;
    let x = (l + c(0.396_337_777_4) * a + c(0.215_803_757_3) * b).cube();
    let y = (l - c(0.105_561_345_8) * a - c(0.063_854_172_8) * b).cube();
    let z = (l - c(0.089_484_177_5) * a - c(1.291_485_548_0) * b).cube();
    [
        c(4.076_741_662_1) * x - c(3.307_711_591_3) * y + c(0.230_969_929_2) * z,
        c(-1.268_438_004_6) * x + c(2.609_757_401_1) * y - c(0.341_319_396_5) * z,
        c(-0.004_196_086_3) * x - c(0.703_418_614_7) * y + c(1.707_614_701_0) * z,
    ]
}

/// Linear sRGB → OkLab, retaining f32 arithmetic for calibrated gradient imports.
pub fn linear_srgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    to_oklab(rgb)
}
/// OkLab → linear sRGB, retaining f32 arithmetic. Gamut policy belongs to the caller.
pub fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    from_oklab(lab)
}
/// Linear sRGB → OkLab, using f64 throughout (palette and fidelity metrics).
pub fn linear_srgb_to_oklab_f64(rgb: [f64; 3]) -> [f64; 3] {
    to_oklab(rgb)
}
/// OkLab → linear sRGB, using f64 throughout. Gamut policy belongs to the caller.
pub fn oklab_to_linear_srgb_f64(lab: [f64; 3]) -> [f64; 3] {
    from_oklab(lab)
}

/// Encoded sRGB → CIELAB relative to D65, unlike Photoshop/ICC's D50 Lab.
/// The exact CIE curve replaces SLIC's rounded dark-branch constants.
pub fn srgb_to_lab_d65(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|v| crate::transfer::srgb_to_linear_f64(f64::from(v)));
    let xyz = [
        0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b,
        0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b,
        0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b,
    ];
    crate::cms_math::xyz_to_lab(xyz, [0.950_47, 1.0, 1.088_83]).map(|v| v as f32)
}
