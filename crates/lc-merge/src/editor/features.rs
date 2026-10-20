//! Editor registration onto the shared MOPS/ANMS and native RANSAC backend, in the editor's
//! integer-centre coordinates. Binary BRIEF descriptors back the editor panorama's `detect`.
use super::geom::Homography;
use super::support::par_map;
pub use crate::ransac::Model;
use li_raster::GrayFrame;
use lightcraft_geom::Point;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub x: f32,
    pub y: f32,
    pub score: f32,
}
pub type Descriptor = [u64; 4];
pub const PATCH_RADIUS: usize = 15;
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

fn blur(w: usize, h: usize, v: &[f32], r: usize) -> Vec<f32> {
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut dst = vec![0.0f32; src.len()];
        let (n, m) = if horizontal { (h, w) } else { (w, h) };
        for line in 0..n {
            let at = |k: i64| {
                let k = k.clamp(0, m as i64 - 1) as usize;
                if horizontal { src[line * w + k] } else { src[k * w + line] }
            };
            let mut acc: f32 = (-(r as i64)..=r as i64).map(at).sum();
            for k in 0..m {
                let i = if horizontal { line * w + k } else { k * w + line };
                dst[i] = acc / (2 * r + 1) as f32;
                acc += at(k as i64 + r as i64 + 1) - at(k as i64 - r as i64);
            }
        }
        dst
    };
    pass(&pass(v, true), false)
}

/// Native MOPS features, converted to the editor's integer-centre coordinates.
pub fn mops(w: usize, h: usize, img: &[f32], valid: Option<&[bool]>, max: usize) -> crate::features::Features {
    if w == 0 || h == 0 || w.checked_mul(h) != Some(img.len()) || valid.is_some_and(|v| v.len() != img.len()) || max == 0 {
        return crate::features::Features::default();
    }
    let plane = GrayFrame { width: w, height: h, data: img.to_vec() };
    let mut f = crate::features::detect(&plane, max);
    let mut points = Vec::new();
    let mut desc = Vec::new();
    for (mut k, d) in f.points.into_iter().zip(f.desc) {
        k.x -= 0.5;
        k.y -= 0.5;
        // Rotated 40px descriptor footprint plus Gaussian smoothing support.
        let radius = (34usize).checked_shl(k.level as u32).unwrap_or(usize::MAX);
        let x = k.x.round() as usize;
        let y = k.y.round() as usize;
        if let Some(mask) = valid {
            let (x0, y0) = (x.saturating_sub(radius), y.saturating_sub(radius));
            let (x1, y1) = (x.saturating_add(radius).min(w - 1), y.saturating_add(radius).min(h - 1));
            if !(y0..=y1).all(|yy| (x0..=x1).all(|xx| mask[yy * w + xx])) {
                continue;
            }
        }
        points.push(k);
        desc.push(d);
    }
    f.points = points;
    f.desc = desc;
    f
}
/// Corner selection from the native MOPS detector.
pub fn harris(w: usize, h: usize, img: &[f32], max: usize, min_dist: f32, valid: Option<&[bool]>) -> Vec<Corner> {
    if max == 0 {
        return Vec::new();
    }
    let f = mops(w, h, img, valid, max.saturating_mul(4));
    let mut out: Vec<Corner> = Vec::new();
    for k in f.points {
        if out.iter().all(|c| (c.x - k.x).hypot(c.y - k.y) >= min_dist) {
            out.push(Corner { x: k.x, y: k.y, score: k.strength });
            if out.len() >= max {
                break;
            }
        }
    }
    out
}
/// The 256 BRIEF test pairs (fixed, Gaussian around the centre, σ = 31/5, clipped to the patch).
fn pairs() -> &'static [[f32; 4]; 256] {
    static P: std::sync::OnceLock<[[f32; 4]; 256]> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        let mut rng = Rng(0x0B21_EF00);
        let mut gauss = || {
            // Box–Muller.
            let (u, v) = (rng.unit().max(1e-12), rng.unit());
            ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos() * 31.0 / 5.0).clamp(-13.0, 13.0) as f32
        };
        let mut p = [[0.0f32; 4]; 256];
        for q in &mut p {
            *q = [gauss(), gauss(), gauss(), gauss()];
        }
        p
    })
}

/// Steered-BRIEF descriptors for `corners` (orientation from the intensity centroid unless
/// `upright`). Returns one descriptor per corner, in order.
pub fn describe(w: usize, h: usize, img: &[f32], corners: &[Corner], upright: bool) -> Vec<Descriptor> {
    if w == 0 || h == 0 || w.checked_mul(h) != Some(img.len()) {
        return Vec::new();
    }
    let smooth = blur(w, h, img, 2);
    let at = |x: f32, y: f32| -> f32 {
        let (xi, yi) = ((x.round() as i64).clamp(0, w as i64 - 1) as usize, (y.round() as i64).clamp(0, h as i64 - 1) as usize);
        smooth[yi * w + xi]
    };
    corners
        .iter()
        .map(|c| {
            let (s, co) = if upright {
                (0.0, 1.0)
            } else {
                let (mut m10, mut m01) = (0.0f32, 0.0f32);
                let r = PATCH_RADIUS as i64;
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy <= r * r {
                            let v = at(c.x + dx as f32, c.y + dy as f32);
                            m10 += dx as f32 * v;
                            m01 += dy as f32 * v;
                        }
                    }
                }
                m01.atan2(m10).sin_cos()
            };
            let mut d = [0u64; 4];
            for (i, p) in pairs().iter().enumerate() {
                let rot = |x: f32, y: f32| (c.x + x * co - y * s, c.y + x * s + y * co);
                let (ax, ay) = rot(p[0], p[1]);
                let (bx, by) = rot(p[2], p[3]);
                if at(ax, ay) < at(bx, by) {
                    d[i / 64] |= 1 << (i % 64);
                }
            }
            d
        })
        .collect()
}

pub fn hamming(a: &Descriptor, b: &Descriptor) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// Matches `(index in a, index in b)` passing the ratio test and the mutual-nearest check.
pub fn match_descriptors(a: &[Descriptor], b: &[Descriptor], ratio: f32) -> Vec<(usize, usize)> {
    let best = |d: &Descriptor, set: &[Descriptor]| -> Option<(usize, u32, u32)> {
        let mut first = (usize::MAX, u32::MAX);
        let mut second = u32::MAX;
        for (j, e) in set.iter().enumerate() {
            let h = hamming(d, e);
            if h < first.1 {
                second = first.1;
                first = (j, h);
            } else if h < second {
                second = h;
            }
        }
        (first.0 != usize::MAX).then_some((first.0, first.1, second))
    };
    // Each query descriptor searches b independently (and cross-checks against a): parallel,
    // order preserved for determinism.
    let matched = par_map(a.len(), |i| {
        let (j, d1, d2) = best(&a[i], b)?;
        if d1 > 80 || (d2 != u32::MAX && d1 as f32 > ratio * d2 as f32) {
            return None;
        }
        best(&b[j], a).is_some_and(|(k, _, _)| k == i).then_some((i, j))
    });
    matched.into_iter().flatten().collect()
}

fn points(p: &[[f64; 2]]) -> Vec<Point> {
    p.iter().map(|p| Point::new(p[0], p[1])).collect()
}
/// Model fitting uses the selected shared native fitter in editor coordinates.
pub fn fit(model: Model, src: &[[f64; 2]], dst: &[[f64; 2]]) -> Option<Homography> {
    if src.len() != dst.len() || src.iter().chain(dst).flatten().any(|v| !v.is_finite()) {
        return None;
    }
    crate::ransac::fit_model(model, &points(src), &points(dst)).map(|h| Homography(h.0))
}
/// Native adaptive RANSAC (the iteration count adapts to the inlier ratio).
pub fn ransac(model: Model, src: &[[f64; 2]], dst: &[[f64; 2]], threshold: f64, seed: u64) -> Option<(Homography, Vec<usize>)> {
    if src.len() != dst.len() || !threshold.is_finite() || threshold <= 0.0 || src.iter().chain(dst).flatten().any(|v| !v.is_finite()) {
        return None;
    }
    crate::ransac::ransac(model, &points(src), &points(dst), threshold, 1, seed).map(|f| (Homography(f.h.0), f.inliers))
}
/// Registers using full precision MOPS descriptors, preserving validity and motion model.
pub fn register(
    w: usize,
    h: usize,
    reference: &[f32],
    moving: &[f32],
    valid_ref: Option<&[bool]>,
    valid_mov: Option<&[bool]>,
    model: Model,
) -> Option<(Homography, usize)> {
    let a = mops(w, h, reference, valid_ref, 800);
    let b = mops(w, h, moving, valid_mov, 800);
    let matched = crate::features::match_features(&b, &a, 0.8);
    let src: Vec<_> = matched.iter().map(|&(i, _)| Point::new(b.points[i].x as f64, b.points[i].y as f64)).collect();
    let dst: Vec<_> = matched.iter().map(|&(_, j)| Point::new(a.points[j].x as f64, a.points[j].y as f64)).collect();
    let threshold = (w.max(h) as f64 / 400.0).clamp(1.5, 4.0);
    let f = crate::ransac::ransac(model, &src, &dst, threshold, 6, 7)?;
    Some((Homography(f.h.0), f.inliers.len()))
}

#[cfg(test)]
mod native_adapter_tests {
    use super::*;
    #[test]
    fn native_mops_adapter_keeps_float_descriptors_and_centres() {
        let (w, h) = (128, 96);
        let img: Vec<_> =
            (0..w * h).map(|i| (((i % w / 7) * 37 + (i / w / 8) * 23 + (i % w / 13) * (i / w / 11) * 17) % 127) as f32 / 127.0).collect();
        let native = crate::features::detect(&GrayFrame { width: w, height: h, data: img.clone() }, 100);
        let editor = mops(w, h, &img, None, 100);
        assert!(!editor.points.is_empty());
        assert_eq!(native.desc, editor.desc);
        for (native, editor) in native.points.iter().zip(editor.points) {
            assert_eq!(native.x - 0.5, editor.x);
            assert_eq!(native.y - 0.5, editor.y)
        }
    }
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
        let (h, inl) = ransac(Model::Translation, &src, &dst, 1.0, 1).unwrap();
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
mod parity {
    use super::*;
    #[test]
    fn editor_fitter_parity_with_shared_native() {
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
        let native = fit(Model::Homography, &src, &dst).unwrap();
        assert_eq!(actual.0, native.0);
        for p in src {
            let (a, b) = (actual.apply(p[0], p[1]), truth.apply(p[0], p[1]));
            assert!((a.0 - b.0).hypot(a.1 - b.1) < 1e-7);
        }
        assert!(fit(Model::Homography, &src, &dst[..4]).is_none());
    }
    #[test]
    fn editor_mask_excludes_full_descriptor_patch() {
        let (w, h) = (128, 96);
        let pixels: Vec<_> = (0..w * h).map(|i| ((i * 73 + i / w * 31) % 211) as f32 / 211.0).collect();
        let valid = vec![false; w * h];
        assert!(harris(w, h, &pixels, 100, 3.0, Some(&valid)).is_empty());
        assert!(register(w, h, &pixels, &pixels, Some(&valid), None, Model::Translation).is_none());
    }
}
