//! Free Transform: projective warps of surfaces.
//!
//! A transform is described by where the four corners of a source rectangle land (a quad). Scale,
//! rotate, skew and flips are affine special cases; Distort and Perspective use the full
//! homography. Warping is an inverse mapping with premultiplied-alpha sampling (nearest, bilinear
//! or Catmull-Rom bicubic), processed per destination tile (in parallel on native) so memory stays
//! bounded by the tile working set. Large reductions are pre-filtered with a proper resize first,
//! so shrinking a layer doesn't alias.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde::{Deserialize, Serialize};

use crate::resample::{Resample, resize_surface};

/// 3×3 projective matrix, row-major: `[x', y', w'] = H · [x, y, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    pub const IDENTITY: Homography = Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.0;
        let w = m[6] * x + m[7] * y + m[8];
        ((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w)
    }

    pub fn mul(&self, o: &Homography) -> Homography {
        let (a, b) = (&self.0, &o.0);
        let mut r = [0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
            }
        }
        Homography(r)
    }

    pub fn inverse(&self) -> Option<Homography> {
        let m = &self.0;
        let c = [
            m[4] * m[8] - m[5] * m[7],
            m[2] * m[7] - m[1] * m[8],
            m[1] * m[5] - m[2] * m[4],
            m[5] * m[6] - m[3] * m[8],
            m[0] * m[8] - m[2] * m[6],
            m[2] * m[3] - m[0] * m[5],
            m[3] * m[7] - m[4] * m[6],
            m[1] * m[6] - m[0] * m[7],
            m[0] * m[4] - m[1] * m[3],
        ];
        let det = m[0] * c[0] + m[1] * c[3] + m[2] * c[6];
        if det.abs() < 1e-12 {
            return None;
        }
        Some(Homography(c.map(|v| v / det)))
    }

    /// Unit square → quad (corners in order: (0,0), (1,0), (1,1), (0,1)).
    fn square_to_quad(q: [[f64; 2]; 4]) -> Option<Homography> {
        let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q;
        let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
        if sx.abs() < 1e-12 && sy.abs() < 1e-12 {
            // Affine.
            return Some(Homography([x1 - x0, x3 - x0, x0, y1 - y0, y3 - y0, y0, 0.0, 0.0, 1.0]));
        }
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() < 1e-12 {
            return None;
        }
        let g = (sx * dy2 - dx2 * sy) / den;
        let h = (dx1 * sy - sx * dy1) / den;
        Some(Homography([x1 - x0 + g * x1, x3 - x0 + h * x3, x0, y1 - y0 + g * y1, y3 - y0 + h * y3, y0, g, h, 1.0]))
    }

    /// The transform taking rectangle `r` (x0, y0, x1, y1) onto `quad` (corners clockwise from
    /// top-left). `None` for degenerate quads.
    pub fn rect_to_quad(r: [f64; 4], quad: [[f64; 2]; 4]) -> Option<Homography> {
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let to_unit = Homography([1.0 / w, 0.0, -r[0] / w, 0.0, 1.0 / h, -r[1] / h, 0.0, 0.0, 1.0]);
        Some(Self::square_to_quad(quad)?.mul(&to_unit))
    }

    /// Approximate linear scale of the mapping near `(x, y)` (sqrt of the Jacobian determinant).
    pub fn local_scale(&self, x: f64, y: f64) -> f64 {
        let e = 0.5;
        let (a, b) = (self.apply(x - e, y), self.apply(x + e, y));
        let (c, d) = (self.apply(x, y - e), self.apply(x, y + e));
        let (jx, jy) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
        (jx.0 * jy.1 - jx.1 * jy.0).abs().sqrt()
    }
}

/// Is the quad (corners in order) strictly convex? Concave, folded (self-intersecting) and
/// degenerate quads are not.
pub fn quad_is_convex(q: &[[f64; 2]; 4]) -> bool {
    let turns = turn_signs(q);
    turns.iter().all(|t| *t > 0.0) || turns.iter().all(|t| *t < 0.0)
}

/// Cross product of the two edges meeting at each corner.
fn turn_signs(q: &[[f64; 2]; 4]) -> [f64; 4] {
    std::array::from_fn(|i| {
        let (a, b, c) = (q[(i + 3) % 4], q[i], q[(i + 1) % 4]);
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    })
}

/// Where a Free Transform frame goes: a projective map for convex quads, or — when Distort or
/// Perspective drags a corner past its neighbours (a concave or folded quad, which a homography
/// would turn inside out through the horizon) — Compositor's fallback: the frame is split along a
/// diagonal into two triangles, each mapped affinely onto its triangle of the quad (a piecewise
/// affine map, which still draws as Photoshop does). For a concave quad the diagonal through the
/// reflex corner keeps both triangles inside it; for a folded one the second triangle draws over
/// the first. Idea from Compositor (MIT); our implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuadMap {
    Projective(Homography),
    Folded {
        /// Source frame `[x0, y0, x1, y1]`.
        rect: [f64; 4],
        quad: [[f64; 2]; 4],
        /// The diagonal: corners 0–2 (`false`) or 1–3 (`true`).
        diag13: bool,
    },
}

/// 2×3 affine `[a, b, c, d, e, f]`: `(x, y) → (a·x + b·y + c, d·x + e·y + f)`.
type Aff = [f64; 6];

/// The affine map taking triangle `s` onto triangle `d` (`None` when `s` is degenerate).
fn tri_affine(s: [[f64; 2]; 3], d: [[f64; 2]; 3]) -> Option<Aff> {
    let det = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
    if det.abs() < 1e-12 {
        return None;
    }
    // Barycentric (l1, l2) of a point p: l1 = ((p−s0)×(s2−s0))/det, l2 = ((s1−s0)×(p−s0))/det.
    let (ax, ay) = (s[1][0] - s[0][0], s[1][1] - s[0][1]);
    let (bx, by) = (s[2][0] - s[0][0], s[2][1] - s[0][1]);
    // l1 = (px·by − py·bx − (s0x·by − s0y·bx)) / det; l2 = (ax·py − ay·px − (ax·s0y − ay·s0x)) / det.
    let l1 = [by / det, -bx / det, -(s[0][0] * by - s[0][1] * bx) / det];
    let l2 = [-ay / det, ax / det, -(ax * s[0][1] - ay * s[0][0]) / det];
    let row = |k: usize| {
        let (d0, d1, d2) = (d[0][k], d[1][k], d[2][k]);
        [(d1 - d0) * l1[0] + (d2 - d0) * l2[0], (d1 - d0) * l1[1] + (d2 - d0) * l2[1], d0 + (d1 - d0) * l1[2] + (d2 - d0) * l2[2]]
    };
    let (x, y) = (row(0), row(1));
    Some([x[0], x[1], x[2], y[0], y[1], y[2]])
}

fn aff_apply(m: &Aff, p: [f64; 2]) -> [f64; 2] {
    [m[0] * p[0] + m[1] * p[1] + m[2], m[3] * p[0] + m[4] * p[1] + m[5]]
}

impl QuadMap {
    /// The map taking rectangle `rect` onto `quad` (corners clockwise from top-left). `None` when
    /// the frame or the quad is degenerate (or not finite).
    pub fn new(rect: [f64; 4], quad: [[f64; 2]; 4]) -> Option<QuadMap> {
        if rect.iter().chain(quad.iter().flatten()).any(|v| !v.is_finite()) || rect[2] <= rect[0] || rect[3] <= rect[1] {
            return None;
        }
        if quad_is_convex(&quad) {
            let h = Homography::rect_to_quad(rect, quad)?;
            h.inverse()?;
            return Some(QuadMap::Projective(h));
        }
        let turns = turn_signs(&quad);
        let pos = turns.iter().filter(|t| **t > 0.0).count();
        // Concave: one corner turns against the other three; the diagonal through it splits
        // the quad into two triangles inside it. Folded: either diagonal (0–2).
        let reflex = match pos {
            1 => turns.iter().position(|t| *t > 0.0),
            3 => turns.iter().position(|t| *t <= 0.0),
            _ => None,
        };
        let diag13 = matches!(reflex, Some(1 | 3));
        let m = QuadMap::Folded { rect, quad, diag13 };
        // At least one triangle must have area on both sides.
        m.pieces().iter().any(|(s, d)| tri_affine(*s, *d).is_some() && tri_affine(*d, *s).is_some()).then_some(m)
    }

    /// The homography, for convex quads.
    pub fn homography(&self) -> Option<&Homography> {
        match self {
            QuadMap::Projective(h) => Some(h),
            QuadMap::Folded { .. } => None,
        }
    }

    pub fn is_folded(&self) -> bool {
        matches!(self, QuadMap::Folded { .. })
    }

    /// Folded maps: the two (source triangle, destination triangle) pieces in drawing order.
    /// Empty for projective maps.
    pub fn pieces(&self) -> Vec<([[f64; 2]; 3], [[f64; 2]; 3])> {
        let QuadMap::Folded { rect, quad, diag13 } = *self else { return Vec::new() };
        let c = [[rect[0], rect[1]], [rect[2], rect[1]], [rect[2], rect[3]], [rect[0], rect[3]]];
        let idx: [[usize; 3]; 2] = if diag13 { [[0, 1, 3], [1, 2, 3]] } else { [[0, 1, 2], [0, 2, 3]] };
        idx.iter().map(|t| (t.map(|i| c[i]), t.map(|i| quad[i]))).collect()
    }

    /// Folded maps: the diagonal's source endpoints and each piece's affine map, with the side
    /// (sign of the cross product against the diagonal) its source triangle lies on.
    fn sides(&self) -> Vec<([f64; 2], [f64; 2], f64, Aff)> {
        let QuadMap::Folded { rect, diag13, .. } = *self else { return Vec::new() };
        let c = [[rect[0], rect[1]], [rect[2], rect[1]], [rect[2], rect[3]], [rect[0], rect[3]]];
        let (a, b) = if diag13 { (c[1], c[3]) } else { (c[0], c[2]) };
        self.pieces()
            .into_iter()
            .filter_map(|(s, d)| {
                let m = tri_affine(s, d)?;
                // The apex: the triangle's corner off the diagonal.
                let apex = s.into_iter().find(|p| *p != a && *p != b)?;
                Some((a, b, cross(a, b, apex).signum(), m))
            })
            .collect()
    }

    /// Where the source point `(x, y)` goes. Folded maps extend each piece's affine map over its
    /// side of the diagonal (where both pieces would apply, the later one wins, as drawn).
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        match self {
            QuadMap::Projective(h) => h.apply(x, y),
            QuadMap::Folded { .. } => {
                let p = [x, y];
                let sides = self.sides();
                let pick = sides.iter().rev().find(|(a, b, s, _)| cross(*a, *b, p) * s >= 0.0).or(sides.last());
                pick.map_or((x, y), |(_, _, _, m)| {
                    let q = aff_apply(m, p);
                    (q[0], q[1])
                })
            }
        }
    }
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Clips a convex polygon to the half-plane where `cross(a, b, p)·side ≥ 0`.
fn clip_half_plane(poly: &[[f64; 2]], a: [f64; 2], b: [f64; 2], side: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let (dp, dq) = (cross(a, b, p) * side, cross(a, b, q) * side);
        if dp >= 0.0 {
            out.push(p);
        }
        if (dp >= 0.0) != (dq >= 0.0) {
            let t = dp / (dp - dq);
            out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    out
}

/// [`warp_surface`] through a [`QuadMap`]: the homography for convex quads (unchanged), and for
/// folded quads the two affine pieces, each over its side of the diagonal (so content beyond the
/// frame and the anti-aliased edges are mapped too), the second drawn over the first.
pub fn warp_surface_map(src: &Surface, src_rect: Rect, m: &QuadMap, interp: Interp) -> Surface {
    let rect = match m {
        QuadMap::Projective(h) => return warp_surface(src, src_rect, h, interp),
        QuadMap::Folded { rect, .. } => *rect,
    };
    // The area to map: the content (plus a filter margin) and the frame.
    let r = src_rect.inflate(2);
    let area = [
        f64::from(r.x0).min(rect[0]),
        f64::from(r.y0).min(rect[1]),
        f64::from(r.x1).max(rect[2]),
        f64::from(r.y1).max(rect[3]),
    ];
    let boxp = vec![[area[0], area[1]], [area[2], area[1]], [area[2], area[3]], [area[0], area[3]]];
    let mut verts: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for (a, b, side, aff) in m.sides() {
        let poly = clip_half_plane(&boxp, a, b, side);
        if poly.len() < 3 {
            continue;
        }
        let base = verts.len();
        verts.extend(poly.iter().map(|p| (aff_apply(&aff, *p), *p)));
        for k in 1..poly.len() - 1 {
            tris.push([base, base + k, base + k + 1]);
        }
    }
    crate::warp::warp_triangles(src, src_rect, &verts, &tris, interp)
}

/// Resampling used by transforms (Photoshop's interpolation menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Interp {
    Nearest,
    Bilinear,
    #[default]
    Bicubic,
}

impl Interp {
    pub fn parse(s: &str) -> Self {
        match s {
            "nearest" | "nearestNeighbor" => Interp::Nearest,
            "bilinear" => Interp::Bilinear,
            _ => Interp::Bicubic,
        }
    }
}

fn catmull_rom(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2]
}

/// Warp `src` (its content inside `src_rect`) by `h` (source → destination document space).
/// The result has the same format as `src`, with alpha added if `src` had none; pixels outside
/// the warped quad are transparent.
pub fn warp_surface(src: &Surface, src_rect: Rect, h: &Homography, interp: Interp) -> Surface {
    let mut fmt = src.format();
    let converted;
    let mut src = if fmt.alpha {
        src
    } else {
        fmt = PixelFormat::new(fmt.mode, fmt.sample, true);
        converted = src.convert(fmt);
        &converted
    };
    let mut out = Surface::new(fmt);
    if src_rect.is_empty() {
        return out;
    }
    let mut h = *h;
    let mut src_rect = src_rect;
    // Pre-reduce for large downscales (bicubic alone aliases below ~50%).
    let (cx, cy) = ((src_rect.x0 + src_rect.x1) as f64 / 2.0, (src_rect.y0 + src_rect.y1) as f64 / 2.0);
    let scale = h.local_scale(cx, cy);
    let reduced;
    if interp != Interp::Nearest && scale < 0.5 && scale > 0.0 {
        let f = 2f64.powf(scale.log2().ceil()).min(1.0); // remaining scale lands in [0.5, 1)
        reduced = resize_surface(src, f, f, Resample::Bicubic);
        src = &reduced;
        h = h.mul(&Homography([1.0 / f, 0.0, 0.0, 0.0, 1.0 / f, 0.0, 0.0, 0.0, 1.0]));
        src_rect = crate::resample::scaled_rect(src_rect, f, f);
    }
    let Some(inv) = h.inverse() else { return out };
    // Destination bounds: the warped corners (plus a pixel for filter support).
    let corners = [(src_rect.x0, src_rect.y0), (src_rect.x1, src_rect.y0), (src_rect.x1, src_rect.y1), (src_rect.x0, src_rect.y1)]
        .map(|(x, y)| h.apply(x as f64, y as f64));
    if corners.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
        return out;
    }
    let lim = 1 << 20;
    let bx0 = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min).floor().max(-(lim as f64)) as i32 - 1;
    let by0 = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min).floor().max(-(lim as f64)) as i32 - 1;
    let bx1 = corners.iter().map(|c| c.0).fold(f64::MIN, f64::max).ceil().min(lim as f64) as i32 + 1;
    let by1 = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max).ceil().min(lim as f64) as i32 + 1;
    let dst = Rect::new(bx0, by0, bx1, by1);
    let n = fmt.channels();
    let a = n - 1;
    let tiles: Vec<Rect> = dst.tiles().map(|tc| tc.rect().intersect(&dst)).filter(|r| !r.is_empty()).collect();
    let src_ref: &Surface = src;
    let work = |t: &Rect| -> Option<(Rect, Vec<f32>)> {
        // Source footprint of this tile (inverse-mapped corners), padded for the filter.
        let tc = [(t.x0, t.y0), (t.x1, t.y0), (t.x1, t.y1), (t.x0, t.y1)].map(|(x, y)| inv.apply(x as f64, y as f64));
        let fx0 = tc.iter().map(|c| c.0).fold(f64::MAX, f64::min).floor() as i32 - 3;
        let fy0 = tc.iter().map(|c| c.1).fold(f64::MAX, f64::min).floor() as i32 - 3;
        let fx1 = tc.iter().map(|c| c.0).fold(f64::MIN, f64::max).ceil() as i32 + 3;
        let fy1 = tc.iter().map(|c| c.1).fold(f64::MIN, f64::max).ceil() as i32 + 3;
        let foot = Rect::new(fx0, fy0, fx1, fy1).intersect(&src_rect);
        if foot.is_empty() || !src_ref.has_tiles_in(foot) {
            return None;
        }
        // Premultiplied source window.
        let mut px = src_ref.read_region(foot);
        for p in px.chunks_exact_mut(n) {
            let al = p[a];
            for v in &mut p[..a] {
                *v *= al;
            }
        }
        let fw = foot.width() as usize;
        let at = |x: i32, y: i32, c: usize| -> f64 {
            if x < foot.x0 || y < foot.y0 || x >= foot.x1 || y >= foot.y1 {
                0.0
            } else {
                px[((y - foot.y0) as usize * fw + (x - foot.x0) as usize) * n + c] as f64
            }
        };
        let w = t.width() as usize;
        let mut outp = vec![0.0f32; w * t.height() as usize * n];
        let mut any = false;
        let mut acc = [0.0f64; 8];
        for y in t.y0..t.y1 {
            for x in t.x0..t.x1 {
                let (u, v) = inv.apply(x as f64 + 0.5, y as f64 + 0.5);
                let (u, v) = (u - 0.5, v - 0.5);
                if u < src_rect.x0 as f64 - 1.0 || v < src_rect.y0 as f64 - 1.0 || u > src_rect.x1 as f64 || v > src_rect.y1 as f64 {
                    continue;
                }
                acc[..n].fill(0.0);
                match interp {
                    Interp::Nearest => {
                        let (ix, iy) = ((u + 0.5).floor() as i32, (v + 0.5).floor() as i32);
                        for (c, s) in acc.iter_mut().enumerate().take(n) {
                            *s = at(ix, iy, c);
                        }
                    }
                    Interp::Bilinear => {
                        let (ix, iy) = (u.floor() as i32, v.floor() as i32);
                        let (fx, fy) = (u - ix as f64, v - iy as f64);
                        for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                            for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
                                for (c, s) in acc.iter_mut().enumerate().take(n) {
                                    *s += at(ix + dx, iy + dy, c) * wx * wy;
                                }
                            }
                        }
                    }
                    Interp::Bicubic => {
                        let (ix, iy) = (u.floor() as i32, v.floor() as i32);
                        let (wx, wy) = (catmull_rom(u - ix as f64), catmull_rom(v - iy as f64));
                        for (j, wyj) in wy.iter().enumerate() {
                            for (i, wxi) in wx.iter().enumerate() {
                                let k = wxi * wyj;
                                for (c, s) in acc.iter_mut().enumerate().take(n) {
                                    *s += at(ix - 1 + i as i32, iy - 1 + j as i32, c) * k;
                                }
                            }
                        }
                    }
                }
                let al = acc[a].clamp(0.0, 1.0);
                if al <= 0.0 {
                    continue;
                }
                any = true;
                let o = ((y - t.y0) as usize * w + (x - t.x0) as usize) * n;
                for c in 0..a {
                    outp[o + c] = (acc[c] / al).clamp(0.0, 1.0) as f32;
                }
                outp[o + a] = al as f32;
            }
        }
        any.then_some((*t, outp))
    };
    #[cfg(not(target_arch = "wasm32"))]
    let done: Vec<(Rect, Vec<f32>)> = {
        use rayon::prelude::*;
        tiles.par_iter().filter_map(work).collect()
    };
    #[cfg(target_arch = "wasm32")]
    let done: Vec<(Rect, Vec<f32>)> = tiles.iter().filter_map(work).collect();
    for (r, v) in done {
        out.write_region(r, &v);
    }
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba() -> Surface {
        Surface::new(PixelFormat::RGBA8)
    }

    #[test]
    fn homography_maps_rect_corners_and_inverts() {
        let r = [10.0, 20.0, 110.0, 70.0];
        let q = [[0.0, 0.0], [200.0, 10.0], [180.0, 150.0], [-20.0, 120.0]];
        let h = Homography::rect_to_quad(r, q).unwrap();
        for (p, e) in [(10.0, 20.0), (110.0, 20.0), (110.0, 70.0), (10.0, 70.0)].iter().zip(q) {
            let (x, y) = h.apply(p.0, p.1);
            assert!((x - e[0]).abs() < 1e-9 && (y - e[1]).abs() < 1e-9, "{p:?} → {x},{y} vs {e:?}");
        }
        let inv = h.inverse().unwrap();
        let (x, y) = inv.apply(90.0, 60.0);
        let (bx, by) = h.apply(x, y);
        assert!((bx - 90.0).abs() < 1e-9 && (by - 60.0).abs() < 1e-9);
        assert!(Homography::rect_to_quad(r, [[0.0, 0.0]; 4]).is_none() || Homography::rect_to_quad(r, [[0.0, 0.0]; 4]).unwrap().inverse().is_none());
    }

    #[test]
    fn identity_and_translation_are_exact() {
        let mut s = rgba();
        s.fill_rect(Rect::new(4, 4, 20, 12), &[1.0, 0.5, 0.0, 1.0]);
        let r = s.content_bounds();
        for interp in [Interp::Nearest, Interp::Bilinear, Interp::Bicubic] {
            let h = Homography([1.0, 0.0, 7.0, 0.0, 1.0, -3.0, 0.0, 0.0, 1.0]);
            let o = warp_surface(&s, r, &h, interp);
            assert_eq!(o.content_bounds(), Rect::new(11, 1, 27, 9), "{interp:?}");
            assert_eq!(o.pixel(15, 5), vec![1.0, 128.0 / 255.0, 0.0, 1.0], "{interp:?}");
        }
    }

    #[test]
    fn scale_doubles_size_and_rotation_keeps_area() {
        let mut s = rgba();
        s.fill_rect(Rect::new(0, 0, 40, 20), &[0.2, 0.4, 0.6, 1.0]);
        let r = s.content_bounds();
        let up = warp_surface(
            &s,
            r,
            &Homography::rect_to_quad([0.0, 0.0, 40.0, 20.0], [[0.0, 0.0], [80.0, 0.0], [80.0, 40.0], [0.0, 40.0]]).unwrap(),
            Interp::Bicubic,
        );
        let b = up.content_bounds();
        assert!(b.width() >= 80 && b.width() <= 82 && b.height() >= 40 && b.height() <= 42, "{b:?}");
        // 90° rotation about (20, 10): a 40×20 box becomes 20×40.
        let q = [[30.0, -10.0], [30.0, 30.0], [10.0, 30.0], [10.0, -10.0]];
        let rot = warp_surface(&s, r, &Homography::rect_to_quad([0.0, 0.0, 40.0, 20.0], q).unwrap(), Interp::Bilinear);
        let b = rot.content_bounds();
        assert!(b.width().abs_diff(20) <= 2 && b.height().abs_diff(40) <= 2, "{b:?}");
        let p = rot.pixel(20, 10);
        assert!((p[0] - 0.2).abs() < 0.01 && p[3] > 0.99, "{p:?}");
    }

    #[test]
    fn large_downscale_is_prefiltered() {
        // A 1px checkerboard shrunk 8× must come out ~50% grey, not aliased black/white.
        let mut s = rgba();
        for y in 0..64 {
            for x in 0..64 {
                let v = ((x + y) % 2) as f32;
                s.fill_rect(Rect::new(x, y, x + 1, y + 1), &[v, v, v, 1.0]);
            }
        }
        let h = Homography::rect_to_quad([0.0, 0.0, 64.0, 64.0], [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]]).unwrap();
        let o = warp_surface(&s, s.content_bounds(), &h, Interp::Bicubic);
        let p = o.pixel(4, 4);
        assert!((p[0] - 0.5).abs() < 0.1, "{p:?}");
    }

    fn poly_contains(q: &[[f64; 2]; 4], p: [f64; 2]) -> bool {
        photocraft_geom::cage::point_in_polygon(q, p)
    }

    #[test]
    fn quad_shapes_pick_the_right_map() {
        let r = [0.0, 0.0, 40.0, 40.0];
        assert!(!QuadMap::new(r, [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]]).unwrap().is_folded());
        // Corner 2 dragged inside: concave, the diagonal runs through it (0–2).
        let dart = [[0.0, 0.0], [40.0, 0.0], [12.0, 12.0], [0.0, 40.0]];
        let m = QuadMap::new(r, dart).unwrap();
        assert_eq!(m, QuadMap::Folded { rect: r, quad: dart, diag13: false });
        // Corner 1 inside: diagonal 1–3.
        let dart1 = [[0.0, 0.0], [20.0, 26.0], [40.0, 40.0], [0.0, 40.0]];
        assert!(matches!(QuadMap::new(r, dart1), Some(QuadMap::Folded { diag13: true, .. })));
        // Folded (bow-tie) still maps.
        assert!(QuadMap::new(r, [[0.0, 0.0], [40.0, 40.0], [40.0, 0.0], [0.0, 40.0]]).unwrap().is_folded());
        // Every corner lands where it was dragged.
        for (c, q) in [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]].iter().zip(dart) {
            let (x, y) = m.apply(c[0], c[1]);
            assert!((x - q[0]).abs() < 1e-9 && (y - q[1]).abs() < 1e-9, "{c:?} → {x},{y}");
        }
        // Degenerate: everything on a line.
        assert!(QuadMap::new(r, [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]]).is_none());
        assert!(QuadMap::new([0.0, 0.0, 0.0, 10.0], dart).is_none());
        assert!(QuadMap::new(r, [[f64::NAN, 0.0], [10.0, 0.0], [20.0, 5.0], [30.0, 0.0]]).is_none());
    }

    #[test]
    fn concave_distort_draws_inside_the_quad_without_ghosts() {
        let mut s = rgba();
        s.fill_rect(Rect::new(0, 0, 40, 40), &[0.2, 0.6, 0.9, 1.0]);
        let dart = [[0.0, 0.0], [40.0, 0.0], [12.0, 12.0], [0.0, 40.0]];
        let m = QuadMap::new([0.0, 0.0, 40.0, 40.0], dart).unwrap();
        let o = warp_surface_map(&s, s.content_bounds(), &m, Interp::Bicubic);
        let b = o.content_bounds();
        assert!(b.x1 <= 41 && b.y1 <= 41 && b.x0 >= -1 && b.y0 >= -1, "{b:?}");
        let (mut inside, mut outside) = (0, 0);
        for y in -2..44 {
            for x in -2..44 {
                let a = o.pixel(x, y)[3];
                let c = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let deep = poly_contains(&dart, c) && [[-1.5, 0.0], [1.5, 0.0], [0.0, -1.5], [0.0, 1.5]].iter().all(|d| poly_contains(&dart, [c[0] + d[0], c[1] + d[1]]));
                let near = [[-1.5, 0.0], [1.5, 0.0], [0.0, -1.5], [0.0, 1.5], [0.0, 0.0]].iter().any(|d| poly_contains(&dart, [c[0] + d[0], c[1] + d[1]]));
                if deep {
                    assert!(a > 0.99, "hole at ({x},{y}): {a}");
                    inside += 1;
                }
                if !near {
                    assert!(a < 0.01, "ghost at ({x},{y}): {a}");
                    outside += 1;
                }
            }
        }
        assert!(inside > 300 && outside > 500, "{inside} {outside}");
        // The projective map of the same quad wraps through the horizon and paints outside it.
        let h = Homography::rect_to_quad([0.0, 0.0, 40.0, 40.0], dart).unwrap();
        let bad = warp_surface(&s, s.content_bounds(), &h, Interp::Bicubic);
        let ghost = (0..40).flat_map(|y| (0..40).map(move |x| (x, y))).any(|(x, y)| {
            bad.pixel(x, y)[3] > 0.5 && ![[-1.5, 0.0], [1.5, 0.0], [0.0, -1.5], [0.0, 1.5], [0.0, 0.0]].iter().any(|d| poly_contains(&dart, [f64::from(x) + 0.5 + d[0], f64::from(y) + 0.5 + d[1]]))
        });
        let hole = (0..40).flat_map(|y| (0..40).map(move |x| (x, y))).any(|(x, y)| bad.pixel(x, y)[3] < 0.5 && poly_contains(&dart, [f64::from(x) + 0.5, f64::from(y) + 0.5]));
        assert!(ghost || hole, "the homography should misrender this quad");
    }

    #[test]
    fn folded_quad_draws_both_triangles_and_convex_is_unchanged() {
        let mut s = rgba();
        s.fill_rect(Rect::new(0, 0, 20, 40), &[1.0, 0.0, 0.0, 1.0]);
        s.fill_rect(Rect::new(20, 0, 40, 40), &[0.0, 0.0, 1.0, 1.0]);
        let bow = [[0.0, 0.0], [40.0, 40.0], [40.0, 0.0], [0.0, 40.0]];
        let m = QuadMap::new([0.0, 0.0, 40.0, 40.0], bow).unwrap();
        let o = warp_surface_map(&s, s.content_bounds(), &m, Interp::Bilinear);
        assert!(o.pixel(35, 20)[3] > 0.99 && o.pixel(5, 20)[3] > 0.99, "both lobes drawn");
        // Convex quads go through the homography exactly as before.
        let q = [[2.0, 1.0], [44.0, 3.0], [40.0, 38.0], [-3.0, 41.0]];
        let a = warp_surface_map(&s, s.content_bounds(), &QuadMap::new([0.0, 0.0, 40.0, 40.0], q).unwrap(), Interp::Bicubic);
        let b = warp_surface(&s, s.content_bounds(), &Homography::rect_to_quad([0.0, 0.0, 40.0, 40.0], q).unwrap(), Interp::Bicubic);
        assert!(a == b, "bit-identical");
    }

    #[test]
    fn surfaces_without_alpha_gain_it() {
        let mut s = Surface::new(PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8, false));
        s.fill_rect(Rect::new(0, 0, 10, 10), &[1.0, 0.0, 0.0]);
        let h = Homography([1.0, 0.0, 5.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let o = warp_surface(&s, Rect::new(0, 0, 10, 10), &h, Interp::Nearest);
        assert!(o.format().alpha);
        assert_eq!(o.pixel(2, 2)[3], 0.0);
        assert_eq!(o.pixel(7, 2), vec![1.0, 0.0, 0.0, 1.0]);
    }
}
