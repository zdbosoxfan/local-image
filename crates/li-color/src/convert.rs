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
mod golden {
    //! Bit-exact snapshots of the old public pc-color conversions these replaced: spot values plus
    //! FNV-1a fingerprints of the output bits over dense input grids.
    use super::*;

    fn bits<const N: usize>(values: [f32; N]) -> [u32; N] {
        values.map(f32::to_bits)
    }
    fn fnv1a(hash: &mut u64, values: &[f32]) {
        for v in values {
            for b in v.to_bits().to_le_bytes() {
                *hash = (*hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        }
    }
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;

    #[test]
    fn transfer_thresholds_and_headroom_are_bit_identical() {
        // (input, srgb_to_linear, linear_to_srgb)
        for (value, linear, encoded) in [
            (-1.0, -0.07739938, -12.92),
            (-0.0, -0.0, -0.0),
            (0.0, 0.0, 0.0),
            (0.0031308, 0.00024232198, 0.040449936),
            (0.04045, 0.003130805, 0.22220546),
            (0.5, 0.21404114, 0.7353569),
            (1.0, 1.0, 0.99999994),
            (2.0, 4.9538474, 1.353256),
            (f32::INFINITY, f32::INFINITY, f32::INFINITY),
            (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        ] {
            assert_eq!(srgb_to_linear(value).to_bits(), f32::to_bits(linear), "srgb_to_linear({value})");
            assert_eq!(linear_to_srgb(value).to_bits(), f32::to_bits(encoded), "linear_to_srgb({value})");
        }
        let (mut linear, mut encoded) = (FNV_OFFSET, FNV_OFFSET);
        for code in 0..=65535 {
            let value = code as f32 / 65535.0;
            fnv1a(&mut linear, &[srgb_to_linear(value)]);
            fnv1a(&mut encoded, &[linear_to_srgb(value)]);
        }
        assert_eq!(linear, 0xdee057b1996b0875, "srgb_to_linear over every 16-bit code");
        assert_eq!(encoded, 0xdc3f09acdc4538bb, "linear_to_srgb over every 16-bit code");
    }

    #[test]
    fn document_lab_gray_and_cmyk_naive_are_bit_identical() {
        // (rgb, srgb_to_lab, lab_to_srgb of that Lab, rgb_to_gray, rgb_to_cmyk_naive, cmyk_to_rgb_naive of that CMYK)
        for (rgb, lab, back, gray, cmyk, rgb_again) in [
            (
                [0.25, 0.5, 0.75],
                [51.43181, -5.2289066, -40.143383],
                [0.24999455, 0.5000015, 0.7499999],
                0.45375,
                [0.6666667, 0.33333334, 0.0, 0.25],
                [0.24999999, 0.49999997, 0.75],
            ),
            (
                [-0.01, 0.0, 1.5],
                [46.12676, 93.13863, -152.83571],
                [0.0, 8.2977414e-5, 0.99999994],
                0.16801,
                [1.0066667, 1.0, 0.0, -0.5],
                [-0.00999999, 0.0, 1.5],
            ),
            (
                [0.001, 0.004, 0.009],
                [0.254179, -0.07545948, -0.5594373],
                [0.0009999698, 0.0040000104, 0.0090000015],
                0.0036730003,
                [0.8888904, 0.5555548, 0.0, 0.991],
                [0.0009999869, 0.004000008, 0.009000003],
            ),
            ([1.0, 0.0, 0.0], [54.29168, 80.812485, 69.885185], [0.99999934, 4.215201e-7, 0.0], 0.299, [0.0, 1.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ] {
            assert_eq!(bits(srgb_to_lab(rgb)), bits(lab), "srgb_to_lab({rgb:?})");
            assert_eq!(bits(lab_to_srgb(lab)), bits(back), "lab_to_srgb({lab:?})");
            assert_eq!(rgb_to_gray(rgb).to_bits(), f32::to_bits(gray), "rgb_to_gray({rgb:?})");
            assert_eq!(bits(rgb_to_cmyk_naive(rgb)), bits(cmyk), "rgb_to_cmyk_naive({rgb:?})");
            assert_eq!(bits(cmyk_to_rgb_naive(cmyk)), bits(rgb_again), "cmyk_to_rgb_naive({cmyk:?})");
        }
        let mut grid = Vec::new();
        for r in 0..=32 {
            for g in 0..=32 {
                for b in 0..=32 {
                    grid.push([r as f32 / 32.0, g as f32 / 32.0, b as f32 / 32.0]);
                }
            }
        }
        grid.extend([[-0.01, 0.0, 1.5], [0.001, 0.004, 0.009], [0.0; 3], [1.0; 3]]);
        let mut hashes = [FNV_OFFSET; 5];
        for rgb in grid {
            let lab = srgb_to_lab(rgb);
            fnv1a(&mut hashes[0], &lab);
            fnv1a(&mut hashes[1], &lab_to_srgb(lab));
            fnv1a(&mut hashes[2], &[rgb_to_gray(rgb)]);
            let cmyk = rgb_to_cmyk_naive(rgb);
            fnv1a(&mut hashes[3], &cmyk);
            fnv1a(&mut hashes[4], &cmyk_to_rgb_naive(cmyk));
        }
        assert_eq!(
            hashes,
            [0xd92a304f2bd89fe9, 0xb17eb1ed3f9a7e96, 0x4f05e10cd9985367, 0x3a1df9bbda81bda6, 0x3f16280868452e19],
            "srgb_to_lab, lab_to_srgb, rgb_to_gray, rgb_to_cmyk_naive, cmyk_to_rgb_naive over the 33³ grid"
        );
    }
}
