//! Small colour-science helpers: 3×3 matrices, CIE XYZ ↔ L*a*b*, chromatic adaptation.
//!
//! All formulas are from public standards: CIE 15 (L*a*b*), ICC.1:2010 Annex A (PCS, D50),
//! and the Bradford cone response matrix published by Lam (1985).

/// Nominal ICC PCS D50: X = 0.9642, Y = 1.0, Z = 0.8249.
/// [`d50_quantized`] applies the s15Fixed16 rounding used by an ICC header.
pub const D50: [f64; 3] = [0.9642, 1.0, 0.8249];

/// CIE D65 white point as XYZ (Y = 1), from the chromaticity (0.3127, 0.3290) used by sRGB,
/// Display P3 and Adobe RGB-compatible spaces.
pub const D65_XY: [f64; 2] = [0.3127, 0.3290];

/// Row-major 3×3 matrix.
pub type Mat3 = [[f64; 3]; 3];

pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    crate::Mat3(*a).mul_direct(&crate::Mat3(*b)).0
}

pub fn apply(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    crate::Mat3(*m).apply(v)
}

pub fn det(m: &Mat3) -> f64 {
    crate::Mat3(*m).determinant()
}

/// Inverse, or `None` when nonfinite or nearly singular under ICC's tolerance.
pub fn invert(m: &Mat3) -> Option<Mat3> {
    crate::Mat3(*m).inverse_with_threshold(1e-12).map(|m| m.0)
}

/// XYZ (Y = 1) of a chromaticity `(x, y)`.
pub fn xy_to_xyz(xy: [f64; 2]) -> [f64; 3] {
    crate::Xy::new(xy[0], xy[1]).to_xyz()
}

/// Bradford cone response matrix (Lam 1985).
const BRADFORD: Mat3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];

/// Bradford chromatic adaptation matrix from white `src` to white `dst` (both XYZ).
/// ICC uses right-associated matrix products to retain the stored-profile reference bits.
pub fn bradford(src: [f64; 3], dst: [f64; 3]) -> Mat3 {
    bradford_with_order(src, dst, false).0
}

/// Develop compatibility order, retaining the original left-associated f64 products.
/// The caller handles the exact same-white identity shortcut.
pub fn bradford_xy_compat(src: [f64; 3], dst: [f64; 3]) -> crate::Mat3 {
    bradford_with_order(src, dst, true)
}

fn bradford_with_order(src: [f64; 3], dst: [f64; 3], develop_order: bool) -> crate::Mat3 {
    let m = crate::Mat3(BRADFORD);
    // The constant matrix has determinant ~1.7; a fallback cannot be reached.
    let inv = m.inverse().unwrap_or(crate::Mat3::IDENTITY);
    let s = m.apply(src);
    let d = m.apply(dst);
    let scale = crate::Mat3::diag(d[0] / s[0], d[1] / s[1], d[2] / s[2]);
    if develop_order { inv.mul(&scale).mul(&m) } else { inv.mul_direct(&scale.mul_direct(&m)) }
}

/// RGB → XYZ matrix (relative to `white`, Y = 1) for primaries given as chromaticities, or
/// `None` when the primaries are not independent.
pub fn rgb_to_xyz_matrix(r: [f64; 2], g: [f64; 2], b: [f64; 2], white: [f64; 3]) -> Option<Mat3> {
    let m = primary_matrix(r, g, b);
    let s = apply(&invert(&m)?, white);
    // ICC preserves the original direct column products.
    Some([
        [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]],
        [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]],
        [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]],
    ])
}

/// Develop's matrix product and degenerate-primary fallback at its existing API.
pub fn rgb_to_xyz_develop_compat(r: [f64; 2], g: [f64; 2], b: [f64; 2], white: [f64; 3]) -> crate::Mat3 {
    let m = crate::Mat3(primary_matrix(r, g, b));
    let s = m.inverse().map_or([1.0; 3], |inv| inv.apply(white));
    m.mul(&crate::Mat3::diag(s[0], s[1], s[2]))
}

fn primary_matrix(r: [f64; 2], g: [f64; 2], b: [f64; 2]) -> Mat3 {
    let (xr, xg, xb) = (xy_to_xyz(r), xy_to_xyz(g), xy_to_xyz(b));
    [[xr[0], xg[0], xb[0]], [xr[1], xg[1], xb[1]], [xr[2], xg[2], xb[2]]]
}

const EPS: f64 = 216.0 / 24389.0; // (6/29)^3
const KAPPA: f64 = 24389.0 / 27.0; // (29/3)^3

trait LabFloat:
    Copy + PartialOrd + std::ops::Add<Output = Self> + std::ops::Sub<Output = Self> + std::ops::Mul<Output = Self> + std::ops::Div<Output = Self>
{
    fn constant(x: f64) -> Self;
    fn cube_root(self) -> Self;
    fn power_i(self, exponent: i32) -> Self;
}
impl LabFloat for f32 {
    fn constant(x: f64) -> Self {
        x as f32
    }
    fn cube_root(self) -> Self {
        self.cbrt()
    }
    fn power_i(self, exponent: i32) -> Self {
        self.powi(exponent)
    }
}
impl LabFloat for f64 {
    fn constant(x: f64) -> Self {
        x
    }
    fn cube_root(self) -> Self {
        self.cbrt()
    }
    fn power_i(self, exponent: i32) -> Self {
        self.powi(exponent)
    }
}

// CIE rational constants are the reference path. Delta arithmetic is retained
// at the old compositor f32 API boundary so unchanged documents retain bits.
#[inline]
fn lab_f<T: LabFloat>(t: T, delta_compat: bool) -> T {
    let c = T::constant;
    if delta_compat {
        let delta = c(6.0) / c(29.0);
        if t > delta.power_i(3) { t.cube_root() } else { t / (c(3.0) * delta.power_i(2)) + c(4.0) / c(29.0) }
    } else if t > c(EPS) {
        t.cube_root()
    } else {
        (c(KAPPA) * t + c(16.0)) / c(116.0)
    }
}

#[inline]
fn lab_finv<T: LabFloat>(f: T, delta_compat: bool) -> T {
    let c = T::constant;
    let f3 = f * f * f;
    if delta_compat {
        let delta = c(6.0) / c(29.0);
        if f > delta { f3 } else { c(3.0) * delta.power_i(2) * (f - c(4.0) / c(29.0)) }
    } else if f3 > c(EPS) {
        f3
    } else {
        (c(116.0) * f - c(16.0)) / c(KAPPA)
    }
}

#[inline]
fn xyz_to_lab_kernel<T: LabFloat>(xyz: [T; 3], white: [T; 3], delta_compat: bool) -> [T; 3] {
    let fx = lab_f(xyz[0] / white[0], delta_compat);
    let fy = lab_f(xyz[1] / white[1], delta_compat);
    let fz = lab_f(xyz[2] / white[2], delta_compat);
    [T::constant(116.0) * fy - T::constant(16.0), T::constant(500.0) * (fx - fy), T::constant(200.0) * (fy - fz)]
}

#[inline]
fn lab_to_xyz_kernel<T: LabFloat>(lab: [T; 3], white: [T; 3], delta_compat: bool) -> [T; 3] {
    let fy = (lab[0] + T::constant(16.0)) / T::constant(116.0);
    let fx = fy + lab[1] / T::constant(500.0);
    let fz = fy - lab[2] / T::constant(200.0);
    [lab_finv(fx, delta_compat) * white[0], lab_finv(fy, delta_compat) * white[1], lab_finv(fz, delta_compat) * white[2]]
}

/// CIE XYZ → L*a*b* relative to `white` (CIE 15), full-precision reference.
pub fn xyz_to_lab(xyz: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    xyz_to_lab_kernel(xyz, white, false)
}

/// CIE L*a*b* → XYZ relative to `white`.
pub fn lab_to_xyz(lab: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    lab_to_xyz_kernel(lab, white, false)
}

#[inline]
pub fn xyz_to_lab_f32(xyz: [f32; 3]) -> [f32; 3] {
    xyz_to_lab_f32_with_white(xyz, [0.9642, 1.0, 0.8249])
}

#[inline]
pub fn xyz_to_lab_f32_with_white(xyz: [f32; 3], white: [f32; 3]) -> [f32; 3] {
    xyz_to_lab_kernel(xyz, white, false)
}

#[inline]
pub fn lab_to_xyz_f32(lab: [f32; 3]) -> [f32; 3] {
    lab_to_xyz_f32_with_white(lab, [0.9642, 1.0, 0.8249])
}

#[inline]
pub fn lab_to_xyz_f32_with_white(lab: [f32; 3], white: [f32; 3]) -> [f32; 3] {
    lab_to_xyz_kernel(lab, white, false)
}

/// Existing compositor f32 arithmetic, with an explicit caller-supplied white.
#[inline]
pub fn xyz_to_lab_f32_compat(xyz: [f32; 3], white: [f32; 3]) -> [f32; 3] {
    xyz_to_lab_kernel(xyz, white, true)
}

#[inline]
pub fn lab_to_xyz_f32_compat(lab: [f32; 3], white: [f32; 3]) -> [f32; 3] {
    lab_to_xyz_kernel(lab, white, true)
}

/// CIE 1976 colour difference.
pub fn delta_e76(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Quantizes to ICC s15Fixed16Number and back (what a profile can actually store).
pub fn s15f16_round(v: f64) -> f64 {
    (v * 65536.0).round() / 65536.0
}

/// The D50 illuminant exactly as an ICC header stores it (s15Fixed16).
pub fn d50_quantized() -> [f64; 3] {
    [s15f16_round(D50[0]), s15f16_round(D50[1]), s15f16_round(D50[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_roundtrip() {
        for xyz in [[0.2, 0.3, 0.1], [0.9642, 1.0, 0.8249], [0.001, 0.002, 0.0005]] {
            let back = lab_to_xyz(xyz_to_lab(xyz, D50), D50);
            for i in 0..3 {
                assert!((back[i] - xyz[i]).abs() < 1e-12);
            }
        }
        let w = xyz_to_lab(D50, D50);
        assert!((w[0] - 100.0).abs() < 1e-12 && w[1].abs() < 1e-12 && w[2].abs() < 1e-12);
    }

    #[test]
    fn bradford_maps_white() {
        let d65 = xy_to_xyz(D65_XY);
        let m = bradford(d65, D50);
        let w = apply(&m, d65);
        for i in 0..3 {
            assert!((w[i] - D50[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn invert_roundtrip() {
        let m = [[0.4, 0.3, 0.2], [0.2, 0.7, 0.1], [0.0, 0.1, 0.8]];
        let p = mul(&m, &invert(&m).unwrap());
        for (i, row) in p.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                assert!((v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
    }
}

#[cfg(test)]
#[path = "compatibility_tests.rs"]
mod compatibility_tests;
