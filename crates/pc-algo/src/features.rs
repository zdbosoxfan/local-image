//! Source-compatible editor registration facade over the shared merge backend.
use crate::transform::Homography;
pub use lightcraft_merge::editor::features::{Corner, Descriptor, Model, PATCH_RADIUS, describe, hamming, harris, match_descriptors};
pub fn fit(model: Model, src: &[[f64; 2]], dst: &[[f64; 2]]) -> Option<Homography> {
    lightcraft_merge::editor::features::fit(model, src, dst).map(|h| Homography(h.0))
}
pub fn ransac(model: Model, src: &[[f64; 2]], dst: &[[f64; 2]], iters: usize, threshold: f64, seed: u64) -> Option<(Homography, Vec<usize>)> {
    lightcraft_merge::editor::features::ransac(model, src, dst, iters, threshold, seed).map(|(h, i)| (Homography(h.0), i))
}
pub fn register(
    w: usize,
    h: usize,
    reference: &[f32],
    moving: &[f32],
    valid_ref: Option<&[bool]>,
    valid_mov: Option<&[bool]>,
    model: Model,
) -> Option<(Homography, usize)> {
    lightcraft_merge::editor::features::register(w, h, reference, moving, valid_ref, valid_mov, model).map(|(h, n)| (Homography(h.0), n))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Deterministic textured test image: blobs of varying brightness.
    fn texture(w: usize, h: usize) -> Vec<f32> {
        let mut rng = Rng(42);
        let mut img = vec![0.3f32; w * h];
        for _ in 0..140 {
            let (cx, cy) = (rng.unit() * w as f64, rng.unit() * h as f64);
            let r = 3.0 + rng.unit() * 9.0;
            let v = rng.unit() as f32;
            let sq = rng.unit() > 0.5;
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                    let inside = if sq { dx.abs() < r && dy.abs() < r * 0.6 } else { dx.hypot(dy) < r };
                    if inside {
                        img[y * w + x] = v;
                    }
                }
            }
        }
        img
    }

    /// Resample `img` by `h` (destination → source lookup with `inv`), bilinear, edge-clamped.
    fn warp(w: usize, hgt: usize, img: &[f32], inv: &Homography) -> Vec<f32> {
        let mut out = vec![0.3f32; w * hgt];
        for y in 0..hgt {
            for x in 0..w {
                let (sx, sy) = inv.apply(x as f64, y as f64);
                if sx < 0.0 || sy < 0.0 || sx >= (w - 1) as f64 || sy >= (hgt - 1) as f64 {
                    continue;
                }
                let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
                let (tx, ty) = ((sx - x0 as f64) as f32, (sy - y0 as f64) as f32);
                let p = |xx: usize, yy: usize| img[yy * w + xx];
                let top = p(x0, y0) + (p(x0 + 1, y0) - p(x0, y0)) * tx;
                let bot = p(x0, y0 + 1) + (p(x0 + 1, y0 + 1) - p(x0, y0 + 1)) * tx;
                out[y * w + x] = top + (bot - top) * ty;
            }
        }
        out
    }

    #[test]
    fn harris_finds_square_corners() {
        let (w, h) = (80, 80);
        let mut img = vec![0.0f32; w * h];
        for y in 30..50 {
            for x in 30..50 {
                img[y * w + x] = 1.0;
            }
        }
        let c = harris(w, h, &img, 10, 5.0, None);
        assert!(c.len() >= 4, "{c:?}");
        for (x, y) in [(30.0, 30.0), (49.0, 49.0)] {
            assert!(c.iter().any(|k| (k.x - x).abs() <= 2.0 && (k.y - y).abs() <= 2.0), "corner near ({x},{y}): {c:?}");
        }
    }

    #[test]
    fn fits_are_exact_on_clean_data() {
        let src = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [5.0, 3.0]];
        let truth = Homography([1.1, 0.05, 3.0, -0.02, 0.95, -2.0, 0.001, 0.0005, 1.0]);
        let dst: Vec<[f64; 2]> = src
            .iter()
            .map(|p| {
                let (x, y) = truth.apply(p[0], p[1]);
                [x, y]
            })
            .collect();
        let h = fit(Model::Homography, &src, &dst).unwrap();
        for p in &src {
            let (a, b) = (h.apply(p[0], p[1]), truth.apply(p[0], p[1]));
            assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
        }
        let sim = Homography([0.9, -0.2, 4.0, 0.2, 0.9, 1.0, 0.0, 0.0, 1.0]);
        let dst: Vec<[f64; 2]> = src
            .iter()
            .map(|p| {
                let (x, y) = sim.apply(p[0], p[1]);
                [x, y]
            })
            .collect();
        let h = fit(Model::Similarity, &src, &dst).unwrap();
        assert!(h.0.iter().zip(sim.0).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn ransac_rejects_outliers() {
        let mut src = Vec::new();
        let mut dst = Vec::new();
        for i in 0..40 {
            let p = [(i * 7 % 50) as f64, (i * 13 % 50) as f64];
            src.push(p);
            dst.push(if i % 4 == 0 { [p[0] * 3.0 + 40.0, -p[1]] } else { [p[0] + 12.0, p[1] - 5.0] });
        }
        let (h, inl) = ransac(Model::Translation, &src, &dst, 100, 1.0, 1).unwrap();
        assert_eq!(inl.len(), 30);
        assert!((h.0[2] - 12.0).abs() < 1e-9 && (h.0[5] + 5.0).abs() < 1e-9);
    }

    #[test]
    fn registers_translation_and_similarity() {
        let (w, h) = (192, 160);
        let reference = texture(w, h);
        // The moving image is the reference shifted by (+9, −6): registration maps it back.
        let shift = Homography([1.0, 0.0, 9.0, 0.0, 1.0, -6.0, 0.0, 0.0, 1.0]);
        let moving = warp(w, h, &reference, &shift.inverse().unwrap());
        let (m, n) = register(w, h, &reference, &moving, None, None, Model::Translation).unwrap();
        assert!(n >= 6);
        assert!((m.0[2] + 9.0).abs() < 1.0 && (m.0[5] - 6.0).abs() < 1.0, "{m:?}");
        // Rotated 4° and scaled 1.03 about the centre.
        let (c, s) = (4f64.to_radians().cos() * 1.03, 4f64.to_radians().sin() * 1.03);
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let sim = Homography([c, -s, cx - c * cx + s * cy, s, c, cy - s * cx - c * cy, 0.0, 0.0, 1.0]);
        let moving = warp(w, h, &reference, &sim.inverse().unwrap());
        let (m, _) = register(w, h, &reference, &moving, None, None, Model::Similarity).unwrap();
        // m should undo sim: m · sim ≈ identity at a few points.
        for p in [[40.0, 40.0], [150.0, 120.0], [96.0, 80.0]] {
            let (x, y) = sim.apply(p[0], p[1]);
            let (bx, by) = m.apply(x, y);
            assert!((bx - p[0]).abs() < 1.5 && (by - p[1]).abs() < 1.5, "{p:?} → {bx},{by}");
        }
    }

    #[test]
    fn descriptors_are_deterministic() {
        let (w, h) = (96, 96);
        let img = texture(w, h);
        let c = harris(w, h, &img, 50, 6.0, None);
        assert_eq!(describe(w, h, &img, &c, false), describe(w, h, &img, &c, false));
        assert_eq!(hamming(&[0, 0, 0, 0], &[1, 3, 0, u64::MAX]), 67);
    }
}

#[cfg(test)]
mod j19_parity {
    use super::*;
    #[test]
    fn j19_editor_fitter_parity_with_shared_native() {
        let src = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [5.0, 3.0]];
        let truth = Homography([1.02, -0.03, 7.25, 0.03, 1.02, -4.5, 0.001, -0.0003, 1.0]);
        let dst: Vec<_> = src
            .iter()
            .map(|p| {
                let (x, y) = truth.apply(p[0], p[1]);
                [x, y]
            })
            .collect();
        let actual = fit(Model::Homography, &src, &dst).unwrap();
        let native = lightcraft_merge::editor::features::fit(Model::Homography, &src, &dst).unwrap();
        assert_eq!(actual.0, native.0);
        for p in src {
            let (a, b) = (actual.apply(p[0], p[1]), truth.apply(p[0], p[1]));
            assert!((a.0 - b.0).hypot(a.1 - b.1) < 1e-7);
        }
        assert!(fit(Model::Homography, &src, &dst[..4]).is_none());
    }
    #[test]
    fn j19_editor_mask_excludes_full_descriptor_patch() {
        let (w, h) = (128, 96);
        let pixels: Vec<_> = (0..w * h).map(|i| ((i * 73 + i / w * 31) % 211) as f32 / 211.0).collect();
        let valid = vec![false; w * h];
        assert!(harris(w, h, &pixels, 100, 3.0, Some(&valid)).is_empty());
        assert!(register(w, h, &pixels, &pixels, Some(&valid), None, Model::Translation).is_none());
    }
}
