//! Editor registration adapters onto the shared MOPS/ANMS and native RANSAC backend.
//! Legacy binary BRIEF descriptors are retained solely for source-compatible callers.
use super::geom::Homography;
use super::support::par_map;
pub use crate::ransac::Model;
use lightcraft_geom::Point;
use lightcraft_raster::Plane;
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
    let plane = Plane { width: w, height: h, data: img.to_vec() };
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
/// Compatibility corner selection from native MOPS detector.
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
/// Native adaptive RANSAC. `iters` remains accepted for source compatibility.
pub fn ransac(model: Model, src: &[[f64; 2]], dst: &[[f64; 2]], _iters: usize, threshold: f64, seed: u64) -> Option<(Homography, Vec<usize>)> {
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
mod j19_tests {
    use super::*;
    #[test]
    fn j19_native_mops_adapter_keeps_float_descriptors_and_centres() {
        let (w, h) = (128, 96);
        let img: Vec<_> =
            (0..w * h).map(|i| (((i % w / 7) * 37 + (i / w / 8) * 23 + (i % w / 13) * (i / w / 11) * 17) % 127) as f32 / 127.0).collect();
        let native = crate::features::detect(&Plane { width: w, height: h, data: img.clone() }, 100);
        let editor = mops(w, h, &img, None, 100);
        assert!(!editor.points.is_empty());
        assert_eq!(native.desc, editor.desc);
        for (native, editor) in native.points.iter().zip(editor.points) {
            assert_eq!(native.x - 0.5, editor.x);
            assert_eq!(native.y - 0.5, editor.y)
        }
    }
}
