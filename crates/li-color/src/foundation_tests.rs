//! Byte-level compatibility oracles for absorbed public math and DNG calibration.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[allow(dead_code, clippy::wrong_self_convention)]
mod before {
    /// Row-major 3×3 matrix.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Mat3(pub [[f64; 3]; 3]);

    impl Mat3 {
        pub const IDENTITY: Mat3 = Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

        pub fn diag(a: f64, b: f64, c: f64) -> Mat3 {
            Mat3([[a, 0.0, 0.0], [0.0, b, 0.0], [0.0, 0.0, c]])
        }
        pub fn mul(&self, o: &Mat3) -> Mat3 {
            let mut r = [[0.0; 3]; 3];
            for i in 0..3 {
                for j in 0..3 {
                    r[i][j] = (0..3).map(|k| self.0[i][k] * o.0[k][j]).sum();
                }
            }
            Mat3(r)
        }
        pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
            let m = &self.0;
            [
                m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
                m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
                m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
            ]
        }
        pub fn apply_f32(&self, v: [f32; 3]) -> [f32; 3] {
            let r = self.apply([v[0] as f64, v[1] as f64, v[2] as f64]);
            [r[0] as f32, r[1] as f32, r[2] as f32]
        }
        pub fn determinant(&self) -> f64 {
            let m = &self.0;
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        }
        pub fn inverse(&self) -> Option<Mat3> {
            let d = self.determinant();
            if d.abs() < 1e-300 {
                return None;
            }
            let m = &self.0;
            let i = 1.0 / d;
            Some(Mat3([
                [
                    (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * i,
                    (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * i,
                    (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * i,
                ],
                [
                    (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * i,
                    (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * i,
                    (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * i,
                ],
                [
                    (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * i,
                    (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * i,
                    (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * i,
                ],
            ]))
        }
        pub fn transpose(&self) -> Mat3 {
            let m = &self.0;
            Mat3([[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]])
        }
        /// As f32 (row-major), for kernels.
        pub fn to_f32(&self) -> [[f32; 3]; 3] {
            self.0.map(|r| r.map(|v| v as f32))
        }
    }

    pub fn hue_of(c: [f32; 3], max: f32, d: f32) -> f32 {
        if d <= 0.0 {
            return 0.0;
        }
        let h = if max == c[0] {
            ((c[1] - c[2]) / d).rem_euclid(6.0)
        } else if max == c[1] {
            (c[2] - c[0]) / d + 2.0
        } else {
            (c[0] - c[1]) / d + 4.0
        };
        h * 60.0
    }

    /// HSV → RGB for any saturation ≥ 0 (above 1 yields negative components, the inverse of
    /// [`hue_of`] for out-of-gamut inputs).
    pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
        let hp = h.rem_euclid(360.0) / 60.0;
        let sector = (hp as u32).min(5);
        let f = hp - sector as f32;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        match sector {
            0 => [v, t, p],
            1 => [q, v, p],
            2 => [p, v, t],
            3 => [p, q, v],
            4 => [t, p, v],
            _ => [v, p, q],
        }
    }
}
fn matrix_bits(m: [[f64; 3]; 3]) -> [[u64; 3]; 3] {
    m.map(|r| r.map(f64::to_bits))
}
#[test]
fn absorbed_matrix_operations_keep_original_bits() {
    for seed in 0..128 {
        let a = std::array::from_fn(|i| std::array::from_fn(|j| ((seed * 13 + i * 7 + j * 5) as f64 * 0.17).sin()));
        let b = std::array::from_fn(|i| std::array::from_fn(|j| ((seed * 5 + i * 11 + j * 3) as f64 * 0.13).cos()));
        assert_eq!(matrix_bits(Mat3(a).mul(&Mat3(b)).0), matrix_bits(before::Mat3(a).mul(&before::Mat3(b)).0));
        assert_eq!(Mat3(a).determinant().to_bits(), before::Mat3(a).determinant().to_bits());
        match (Mat3(a).inverse(), before::Mat3(a).inverse()) {
            (Some(new), Some(old)) => assert_eq!(matrix_bits(new.0), matrix_bits(old.0)),
            (None, None) => {}
            _ => panic!("inverse policy changed"),
        }
    }
    for zero in [0.0, -0.0] {
        let m = [[zero; 3]; 3];
        assert_eq!(matrix_bits(Mat3(m).mul(&Mat3(m)).0), matrix_bits(before::Mat3(m).mul(&before::Mat3(m)).0));
        assert_eq!(Mat3(m).inverse(), None);
    }
}
#[test]
fn shared_profile_hsv_keeps_calibrated_headroom_and_wrap_bits() {
    for h in [-1080.0, -360.0, -0.001, -0.00001, -0.00000001, -0.0, 0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 359.99997, 360.0, 720.0] {
        for s in [0.0, 0.001, 0.2, 0.5, 1.0, 1.2, 2.0] {
            for v in [0.0, 0.001, 0.1, 0.5, 1.0, 1.5, 3.0] {
                assert_eq!(
                    perceptual::hsv_to_rgb_profile(h, s, v).map(f32::to_bits),
                    before::hsv_to_rgb(h, s, v).map(f32::to_bits),
                    "h={h} s={s} v={v}"
                );
            }
        }
    }
    for r in 0..=16 {
        for g in 0..=16 {
            for b in 0..=16 {
                let rgb = [r as f32 / 8.0 - 0.5, g as f32 / 8.0 - 0.5, b as f32 / 8.0 - 0.5];
                let max = rgb[0].max(rgb[1]).max(rgb[2]);
                let d = max - rgb[0].min(rgb[1]).min(rgb[2]);
                assert_eq!(perceptual::hue_degrees(rgb, max, d).to_bits(), before::hue_of(rgb, max, d).to_bits());
            }
        }
    }
}
#[test]
fn luminance_precision_and_white_point_contracts_are_explicit() {
    for c in [[0.0, 0.0, 0.0], [0.1, 0.5, 0.9], [1.0, 1.0, 1.0], [-0.25, 0.125, 2.0]] {
        assert_eq!(luminance_709_f64(c).to_bits(), (c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722).to_bits());
        assert_eq!(luminance_2020_f64(c).to_bits(), (c[0] * 0.2627 + c[1] * 0.6780 + c[2] * 0.0593).to_bits());
        let c = c.map(|x| x as f32);
        assert_eq!(luminance_709(c).to_bits(), (c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722).to_bits());
    }
}
