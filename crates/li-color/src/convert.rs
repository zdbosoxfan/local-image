//! Shared document conversions, preserving native f32 precision and legacy D50 colorimetry.
//! Profile-dependent CMYK state belongs to the calling CMS adapter, not this math crate.

pub use crate::transfer::{linear_to_srgb, srgb_to_linear};

/// Naive CMYK → RGB (no profile): `(1 − c)(1 − k)`. Kept for reference and tests.
pub fn cmyk_to_rgb_naive(c: [f32; 4]) -> [f32; 3] {
    let k = 1.0 - c[3];
    [(1.0 - c[0]) * k, (1.0 - c[1]) * k, (1.0 - c[2]) * k]
}

/// Naive RGB → CMYK with full GCR.
pub fn rgb_to_cmyk_naive(rgb: [f32; 3]) -> [f32; 4] {
    rgb_to_cmyk_naive_with_black_threshold(rgb, 1.0 - 1e-6)
}

/// The same full-GCR formula with the caller's black endpoint policy. Flat
/// codec conversion historically uses exact black; document channels reserve
/// a small near-black interval to avoid dividing by a vanishing denominator.
pub fn rgb_to_cmyk_naive_with_black_threshold(rgb: [f32; 3], black_threshold: f32) -> [f32; 4] {
    let k = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    if k >= black_threshold {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let d = 1.0 - k;
    [(1.0 - rgb[0] - k) / d, (1.0 - rgb[1] - k) / d, (1.0 - rgb[2] - k) / d, k]
}

/// Historical document-Lab D50 white. This differs from nominal ICC PCS D50.
pub const D50: [f32; 3] = [0.964_22, 1.0, 0.825_21];

/// Rounded legacy linear sRGB → D50 XYZ matrix, retained for document compatibility.
/// Row sums approximate [`D50`]; explicit-white conversions use derived f64 matrices.
pub const SRGB_TO_XYZ_D50: [[f32; 3]; 3] = [[0.436_074, 0.385_064, 0.143_080], [0.222_504, 0.716_878, 0.060_618], [0.013_932, 0.097_104, 0.714_173]];

/// D50 XYZ → linear sRGB (the inverse of [`SRGB_TO_XYZ_D50`]).
pub const XYZ_D50_TO_SRGB: [[f32; 3]; 3] =
    [[3.133_856, -1.616_867, -0.490_615], [-0.978_768, 1.916_142, 0.033_454], [0.071_945, -0.228_991, 1.405_243]];

/// `m · v` for a row-major 3 × 3 matrix.
#[inline]
pub fn mat3_mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}

/// CIE L*a*b* (D50) → encoded sRGB (Bradford-adapted to D65).
pub fn lab_to_srgb(lab: [f32; 3]) -> [f32; 3] {
    let xyz = crate::cms_math::lab_to_xyz_f32_compat(lab, D50);
    mat3_mul(&XYZ_D50_TO_SRGB, xyz).map(|lin| linear_to_srgb(lin.clamp(0.0, 1.0)))
}
/// CIE L*a*b* relative to an explicit XYZ white → encoded sRGB, at f64 precision.
///
/// Use this for file formats whose nominal white differs from the historical document-Lab
/// contract above. The caller chooses the format's white; shared CIE and Bradford kernels
/// derive the conversion from sRGB's published primaries. Gamut is clipped before encoding.
pub fn lab_to_srgb_with_white(lab: [f32; 3], white: [f64; 3]) -> [f32; 3] {
    let xyz = crate::cms_math::lab_to_xyz(lab.map(f64::from), white);
    let adaptation = crate::cms_math::bradford(white, crate::SRGB.white.to_xyz());
    let d65_xyz = crate::cms_math::apply(&adaptation, xyz);
    crate::SRGB.from_xyz().apply(d65_xyz).map(|v| crate::transfer::linear_to_srgb_f64(v.clamp(0.0, 1.0)) as f32)
}

/// Encoded sRGB → CIE L*a*b* (D50).
pub fn srgb_to_lab(rgb: [f32; 3]) -> [f32; 3] {
    let xyz = mat3_mul(&SRGB_TO_XYZ_D50, rgb.map(srgb_to_linear));
    crate::cms_math::xyz_to_lab_f32_compat(xyz, D50)
}
/// Rec.601 luma used by Photoshop's Grayscale conversion default (encoded domain).
pub const LUMA_601: [f32; 3] = [0.299, 0.587, 0.114];

#[inline]
pub fn rgb_to_gray(rgb: [f32; 3]) -> f32 {
    LUMA_601[0] * rgb[0] + LUMA_601[1] * rgb[1] + LUMA_601[2] * rgb[2]
}

/// Recover the green channel while retaining Rec.601 luma, preserving the
/// denoiser's native arithmetic and unbounded sample range.
pub fn green_from_luma_601(luma: f32, red: f32, blue: f32) -> f32 {
    (luma - LUMA_601[0] * red - LUMA_601[2] * blue) / LUMA_601[1]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod parity {
    use super::*;
    // Golden precision oracle extracted from the old public pc-color conversion functions.
    #[allow(dead_code)]
    mod previous {
        /// sRGB transfer function: encoded → linear.
        #[inline]
        pub fn srgb_to_linear(v: f32) -> f32 {
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        }

        /// sRGB transfer function: linear → encoded.
        #[inline]
        pub fn linear_to_srgb(v: f32) -> f32 {
            if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
        }

        /// Naive CMYK → RGB (no profile): `(1 − c)(1 − k)`. Kept for reference and tests.
        pub fn cmyk_to_rgb_naive(c: [f32; 4]) -> [f32; 3] {
            let k = 1.0 - c[3];
            [(1.0 - c[0]) * k, (1.0 - c[1]) * k, (1.0 - c[2]) * k]
        }

        /// Naive RGB → CMYK with full GCR.
        pub fn rgb_to_cmyk_naive(rgb: [f32; 3]) -> [f32; 4] {
            let k = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
            if k >= 1.0 - 1e-6 {
                return [0.0, 0.0, 0.0, 1.0];
            }
            let d = 1.0 - k;
            [(1.0 - rgb[0] - k) / d, (1.0 - rgb[1] - k) / d, (1.0 - rgb[2] - k) / d, k]
        }

        /// Historical document-Lab D50 white. This differs from nominal ICC PCS D50.
        pub const D50: [f32; 3] = [0.964_22, 1.0, 0.825_21];

        /// Rounded legacy linear sRGB → D50 XYZ matrix, retained for document compatibility.
        /// Row sums approximate [`D50`]; explicit-white conversions use derived f64 matrices.
        pub const SRGB_TO_XYZ_D50: [[f32; 3]; 3] =
            [[0.436_074, 0.385_064, 0.143_080], [0.222_504, 0.716_878, 0.060_618], [0.013_932, 0.097_104, 0.714_173]];

        /// D50 XYZ → linear sRGB (the inverse of [`SRGB_TO_XYZ_D50`]).
        pub const XYZ_D50_TO_SRGB: [[f32; 3]; 3] =
            [[3.133_856, -1.616_867, -0.490_615], [-0.978_768, 1.916_142, 0.033_454], [0.071_945, -0.228_991, 1.405_243]];

        /// `m · v` for a row-major 3 × 3 matrix.
        #[inline]
        pub fn mat3_mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
            m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
        }

        /// CIE L*a*b* (D50) → encoded sRGB (Bradford-adapted to D65).
        pub fn lab_to_srgb(lab: [f32; 3]) -> [f32; 3] {
            let [l, a, b] = lab;
            let fy = (l + 16.0) / 116.0;
            let fx = fy + a / 500.0;
            let fz = fy - b / 200.0;
            let finv = |t: f32| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
            let xyz = [finv(fx) * D50[0], finv(fy) * D50[1], finv(fz) * D50[2]];
            mat3_mul(&XYZ_D50_TO_SRGB, xyz).map(|lin| linear_to_srgb(lin.clamp(0.0, 1.0)))
        }

        /// Encoded sRGB → CIE L*a*b* (D50).
        pub fn srgb_to_lab(rgb: [f32; 3]) -> [f32; 3] {
            let lin = rgb.map(srgb_to_linear);
            let xyz = mat3_mul(&SRGB_TO_XYZ_D50, lin);
            let f = |t: f32| if t > (6.0f32 / 29.0).powi(3) { t.cbrt() } else { t / (3.0 * (6.0f32 / 29.0).powi(2)) + 4.0 / 29.0 };
            let (fx, fy, fz) = (f(xyz[0] / D50[0]), f(xyz[1] / D50[1]), f(xyz[2] / D50[2]));
            [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
        }

        /// Rec.601 luma used by Photoshop's Grayscale conversion default.
        pub fn rgb_to_gray(rgb: [f32; 3]) -> f32 {
            0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]
        }
    }
    fn bits<const N: usize>(values: [f32; N]) -> [u32; N] {
        values.map(f32::to_bits)
    }
    #[test]
    fn transfer_thresholds_and_headroom_are_bit_identical() {
        for value in [-1.0, -0.0, 0.0, 0.0031308, 0.04045, 1.0, 2.0, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(srgb_to_linear(value).to_bits(), previous::srgb_to_linear(value).to_bits());
            assert_eq!(linear_to_srgb(value).to_bits(), previous::linear_to_srgb(value).to_bits());
        }
        for code in 0..=65535 {
            let value = code as f32 / 65535.0;
            assert_eq!(srgb_to_linear(value).to_bits(), previous::srgb_to_linear(value).to_bits());
            assert_eq!(linear_to_srgb(value).to_bits(), previous::linear_to_srgb(value).to_bits());
        }
    }
    #[test]
    fn document_lab_gray_and_cmyk_naive_are_bit_identical() {
        for r in 0..=32 {
            for g in 0..=32 {
                for b in 0..=32 {
                    let rgb = [r as f32 / 32.0, g as f32 / 32.0, b as f32 / 32.0];
                    let lab = previous::srgb_to_lab(rgb);
                    assert_eq!(bits(srgb_to_lab(rgb)), bits(lab));
                    assert_eq!(bits(lab_to_srgb(lab)), bits(previous::lab_to_srgb(lab)));
                    assert_eq!(rgb_to_gray(rgb).to_bits(), previous::rgb_to_gray(rgb).to_bits());
                    let cmyk = rgb_to_cmyk_naive(rgb);
                    assert_eq!(bits(cmyk), bits(previous::rgb_to_cmyk_naive(rgb)));
                    assert_eq!(bits(cmyk_to_rgb_naive(cmyk)), bits(previous::cmyk_to_rgb_naive(cmyk)));
                }
            }
        }
        for rgb in [[-0.01, 0.0, 1.5], [0.001, 0.004, 0.009], [0.0; 3], [1.0; 3]] {
            let lab = previous::srgb_to_lab(rgb);
            assert_eq!(bits(srgb_to_lab(rgb)), bits(lab));
            assert_eq!(bits(lab_to_srgb(lab)), bits(previous::lab_to_srgb(lab)));
        }
    }
}
