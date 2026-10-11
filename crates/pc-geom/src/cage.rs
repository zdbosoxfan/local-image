//! Cage deformations (Edit › Transform › Cage): a closed polygon drawn around content, whose
//! vertices are then moved; every point inside follows through generalized barycentric
//! coordinates of the cage.
//!
//! Two coordinate systems, both implemented from the papers:
//! - **Green coordinates** (Y. Lipman, D. Levin, D. Cohen-Or, *Green Coordinates*, ACM SIGGRAPH
//!   2008; the 2D closed form of their Algorithm 1). A point is a combination of the cage
//!   vertices *and* the outward edge normals (scaled by each edge's length change), which makes
//!   the map shape preserving (conformal in 2D) and exact for similarities. The deformed cage
//!   boundary is not interpolated exactly: content bulges smoothly rather than following straight
//!   cage edges, as in GIMP's and Krita's cage tools.
//! - **Mean value coordinates** for arbitrary polygons (M. Floater, *Mean value coordinates*,
//!   CAGD 2003; K. Hormann, M. Floater, *Mean value coordinates for arbitrary planar polygons*,
//!   ACM TOG 2006): interpolating (the cage edges map onto the target edges) and defined in the
//!   whole plane. They also drive seamless cloning (`photocraft-algo::seamless`).
//!
//! Pure geometry: resampling lives in `photocraft-algo::cage`.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

/// Which generalized barycentric coordinates drive a cage map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CageCoords {
    /// Green coordinates (shape preserving; the default).
    #[default]
    Green,
    /// Mean value coordinates (interpolating).
    MeanValue,
}

impl CageCoords {
    pub fn parse(s: &str) -> Option<CageCoords> {
        match s {
            "green" => Some(CageCoords::Green),
            "meanValue" | "mvc" => Some(CageCoords::MeanValue),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            CageCoords::Green => "green",
            CageCoords::MeanValue => "meanValue",
        }
    }
}

/// Why a cage was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CageError {
    /// Fewer than three vertices.
    TooFewPoints,
    /// The cage and the target have different vertex counts.
    LengthMismatch,
    /// A coordinate is NaN or infinite.
    NotFinite,
    /// Two consecutive vertices coincide, or the cage has no area.
    Degenerate,
    /// The source cage crosses itself.
    SelfIntersecting,
}

impl std::fmt::Display for CageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CageError::TooFewPoints => "a cage needs at least three points",
            CageError::LengthMismatch => "the target cage needs as many points as the cage",
            CageError::NotFinite => "cage coordinates must be finite numbers",
            CageError::Degenerate => "the cage has no area (or repeats a point)",
            CageError::SelfIntersecting => "the cage crosses itself",
        })
    }
}

impl std::error::Error for CageError {}

/// Twice the signed area (positive for counter-clockwise in a y-up frame).
pub fn signed_area2(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = poly.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Does the closed polygon cross itself (proper crossings of non-adjacent edges)?
pub fn polygon_self_intersects(poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_cross(poly[i], poly[(i + 1) % n], poly[j], poly[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

/// Nearest point on the closed polygon's boundary to `p`, the distance, and the inward
/// direction there (the edge's inward normal, or at a vertex the bisector of both edges').
/// `poly` must be counter-clockwise (positive [`signed_area2`]).
fn nearest_on_boundary(poly: &[[f64; 2]], p: [f64; 2]) -> ([f64; 2], f64, [f64; 2]) {
    let n = poly.len();
    let inward = |i: usize| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l = ex.hypot(ey).max(1e-300);
        // Counter-clockwise: the interior is on the left of each edge.
        [-ey / l, ex / l]
    };
    let mut best = (poly[0], f64::INFINITY, [0.0, 0.0]);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l2 = ex * ex + ey * ey;
        let t = if l2 > 0.0 { (((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let q = [a[0] + t * ex, a[1] + t * ey];
        let d = (q[0] - p[0]).hypot(q[1] - p[1]);
        if d < best.1 {
            let dir = if t <= 1e-9 {
                let (u, v) = (inward((i + n - 1) % n), inward(i));
                [u[0] + v[0], u[1] + v[1]]
            } else if t >= 1.0 - 1e-9 {
                let (u, v) = (inward(i), inward((i + 1) % n));
                [u[0] + v[0], u[1] + v[1]]
            } else {
                inward(i)
            };
            let l = dir[0].hypot(dir[1]);
            let dir = if l > 1e-12 { [dir[0] / l, dir[1] / l] } else { inward(i) };
            best = (q, d, dir);
        }
    }
    best
}

/// Green coordinates of `p` with respect to the counter-clockwise polygon `cage` (Lipman et al.
/// 2008, 2D closed form): `phi` per vertex, `psi` per edge (edge `j` runs from vertex `j` to
/// `j + 1`). `p` must lie strictly inside the cage.
pub fn green_coordinates(cage: &[[f64; 2]], p: [f64; 2], phi: &mut [f64], psi: &mut [f64]) {
    let n = cage.len();
    phi[..n].fill(0.0);
    psi[..n].fill(0.0);
    for j in 0..n {
        let (v1, v2) = (cage[j], cage[(j + 1) % n]);
        let a = [v2[0] - v1[0], v2[1] - v1[1]];
        let b = [v1[0] - p[0], v1[1] - p[1]];
        let q = a[0] * a[0] + a[1] * a[1];
        let s = b[0] * b[0] + b[1] * b[1];
        let r = 2.0 * (a[0] * b[0] + a[1] * b[1]);
        let la = q.sqrt();
        if la <= 0.0 || s <= 0.0 {
            continue;
        }
        // `b` against the edge's *inward* normal times its length (with the outward normal in
        // the deformation, this is the paper's convention for a counter-clockwise cage).
        let ba = b[1] * a[0] - b[0] * a[1];
        let srt = (4.0 * s * q - r * r).max(1e-300).sqrt();
        let l0 = s.ln();
        let l1 = (s + q + r).max(1e-300).ln();
        let a0 = (r / srt).atan() / srt;
        let a1 = ((2.0 * q + r) / srt).atan() / srt;
        let a10 = a1 - a0;
        let l10 = l1 - l0;
        psi[j] = -la / (4.0 * PI) * ((4.0 * s - r * r / q) * a10 + r / (2.0 * q) * l10 + l1 - 2.0);
        phi[(j + 1) % n] -= ba / (2.0 * PI) * (l10 / (2.0 * q) - a10 * r / q);
        phi[j] += ba / (2.0 * PI) * (l10 / (2.0 * q) - a10 * (2.0 + r / q));
    }
}

/// `tan(α/2)` of the signed angle at `x` between `s0 = v0 − x` and `s1 = v1 − x`, with the
/// radii `r0 = |s0|`, `r1 = |s1|` (Hormann & Floater 2006, eq. 10; stable for every angle but π).
#[inline]
pub fn half_tan(s0: [f64; 2], s1: [f64; 2], r0: f64, r1: f64) -> f64 {
    let a = s0[0] * s1[1] - s0[1] * s1[0];
    let d = s0[0] * s1[0] + s0[1] * s1[1];
    let den = r0 * r1 + d;
    if den.abs() > 1e-12 * (r0 * r1).max(1e-300) {
        a / den
    } else {
        // α = ±π: x lies on the segment (handled by the caller); the limit is infinite.
        a.signum() * 1e12
    }
}

/// Mean value coordinates of `p` with respect to the closed polygon `poly` (any orientation, any
/// shape, `p` anywhere). On the boundary they reduce to linear interpolation along the edge.
pub fn mean_value_coordinates(poly: &[[f64; 2]], p: [f64; 2], out: &mut [f64]) {
    let n = poly.len();
    out[..n].fill(0.0);
    let s: Vec<[f64; 2]> = poly.iter().map(|v| [v[0] - p[0], v[1] - p[1]]).collect();
    let r: Vec<f64> = s.iter().map(|v| v[0].hypot(v[1])).collect();
    for i in 0..n {
        if r[i] < 1e-12 {
            out[i] = 1.0;
            return;
        }
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let a = s[i][0] * s[j][1] - s[i][1] * s[j][0];
        let d = s[i][0] * s[j][0] + s[i][1] * s[j][1];
        if a.abs() <= 1e-12 * r[i] * r[j] && d < 0.0 {
            // On edge i → j.
            let t = r[i] / (r[i] + r[j]);
            out[i] = 1.0 - t;
            out[j] = t;
            return;
        }
    }
    let t: Vec<f64> = (0..n).map(|i| half_tan(s[i], s[(i + 1) % n], r[i], r[(i + 1) % n])).collect();
    let mut sum = 0.0;
    for i in 0..n {
        let w = (t[(i + n - 1) % n] + t[i]) / r[i];
        out[i] = w;
        sum += w;
    }
    if sum.abs() > 1e-300 {
        for v in &mut out[..n] {
            *v /= sum;
        }
    }
}

/// A cage deformation: points inside `cage` follow its vertices to `target`.
#[derive(Clone, Debug, PartialEq)]
pub struct CageMap {
    coords: CageCoords,
    /// Counter-clockwise source cage.
    src: Vec<[f64; 2]>,
    /// Target vertices, in the same (possibly reversed) order as `src`.
    dst: Vec<[f64; 2]>,
    /// Green coordinates: per edge, the target's outward normal times the edge's length ratio.
    edge_terms: Vec<[f64; 2]>,
    identity: bool,
}

impl CageMap {
    pub fn new(cage: &[[f64; 2]], target: &[[f64; 2]], coords: CageCoords) -> Result<CageMap, CageError> {
        if cage.len() < 3 {
            return Err(CageError::TooFewPoints);
        }
        if cage.len() != target.len() {
            return Err(CageError::LengthMismatch);
        }
        if cage.iter().chain(target).any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return Err(CageError::NotFinite);
        }
        let n = cage.len();
        if (0..n).any(|i| {
            let (a, b) = (cage[i], cage[(i + 1) % n]);
            (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9
        }) {
            return Err(CageError::Degenerate);
        }
        if polygon_self_intersects(cage) {
            return Err(CageError::SelfIntersecting);
        }
        let area = signed_area2(cage);
        if area.abs() < 1e-9 {
            return Err(CageError::Degenerate);
        }
        let (mut src, mut dst) = (cage.to_vec(), target.to_vec());
        if area < 0.0 {
            src.reverse();
            dst.reverse();
        }
        let edge_terms = (0..n)
            .map(|j| {
                let k = (j + 1) % n;
                let l = (src[k][0] - src[j][0]).hypot(src[k][1] - src[j][1]);
                let (ex, ey) = (dst[k][0] - dst[j][0], dst[k][1] - dst[j][1]);
                // Outward normal (ey, −ex)/|e'| times the scale |e'|/|e|.
                [ey / l, -ex / l]
            })
            .collect();
        let identity = src == dst;
        Ok(CageMap { coords, src, dst, edge_terms, identity })
    }

    pub fn coords(&self) -> CageCoords {
        self.coords
    }

    /// The source cage (counter-clockwise).
    pub fn source(&self) -> &[[f64; 2]] {
        &self.src
    }

    /// The target cage (same vertex order as [`source`](Self::source)).
    pub fn target(&self) -> &[[f64; 2]] {
        &self.dst
    }

    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// Is `(x, y)` inside the source cage?
    pub fn contains(&self, x: f64, y: f64) -> bool {
        point_in_polygon(&self.src, [x, y])
    }

    /// Bounding box `[x0, y0, x1, y1]` of the source cage.
    pub fn source_bounds(&self) -> [f64; 4] {
        bounds(&self.src)
    }

    /// Bounding box of the target cage.
    pub fn target_bounds(&self) -> [f64; 4] {
        bounds(&self.dst)
    }

    /// Where the source point `(x, y)` goes. Points outside the cage (or on its boundary) take
    /// the value of the nearest boundary point pushed a quarter pixel inside, so a resampling
    /// mesh straddling the cage stays continuous.
    pub fn map(&self, x: f64, y: f64) -> (f64, f64) {
        if self.identity {
            return (x, y);
        }
        let n = self.src.len();
        match self.coords {
            CageCoords::MeanValue => {
                let mut w = vec![0.0; n];
                mean_value_coordinates(&self.src, [x, y], &mut w);
                let (mut ox, mut oy) = (0.0, 0.0);
                for (wi, d) in w.iter().zip(&self.dst) {
                    ox += wi * d[0];
                    oy += wi * d[1];
                }
                (ox, oy)
            }
            CageCoords::Green => {
                const INSET: f64 = 0.25;
                let mut p = [x, y];
                let (q, d, dir) = nearest_on_boundary(&self.src, p);
                if d < INSET || !point_in_polygon(&self.src, p) {
                    p = [q[0] + dir[0] * INSET, q[1] + dir[1] * INSET];
                }
                let (mut phi, mut psi) = (vec![0.0; n], vec![0.0; n]);
                green_coordinates(&self.src, p, &mut phi, &mut psi);
                let (mut ox, mut oy) = (0.0, 0.0);
                for i in 0..n {
                    ox += phi[i] * self.dst[i][0] + psi[i] * self.edge_terms[i][0];
                    oy += phi[i] * self.dst[i][1] + psi[i] * self.edge_terms[i][1];
                }
                (ox, oy)
            }
        }
    }
}

fn bounds(p: &[[f64; 2]]) -> [f64; 4] {
    p.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[f64; 2]> {
        vec![[10.0, 10.0], [90.0, 10.0], [90.0, 70.0], [10.0, 70.0]]
    }

    fn l_shape() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [60.0, 0.0], [60.0, 20.0], [20.0, 20.0], [20.0, 80.0], [0.0, 80.0]]
    }

    fn close(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
        (a.0 - b.0).abs() < tol && (a.1 - b.1).abs() < tol
    }

    #[test]
    fn green_coordinates_reproduce_points_and_sum_to_one() {
        for cage in [square(), l_shape()] {
            let mut ccw = cage.clone();
            if signed_area2(&ccw) < 0.0 {
                ccw.reverse();
            }
            let n = ccw.len();
            for p in [[15.0, 15.0], [30.0, 12.0], [5.0, 60.0], [11.0, 18.0]] {
                if !point_in_polygon(&ccw, p) {
                    continue;
                }
                let (mut phi, mut psi) = (vec![0.0; n], vec![0.0; n]);
                green_coordinates(&ccw, p, &mut phi, &mut psi);
                let sum: f64 = phi.iter().sum();
                assert!((sum - 1.0).abs() < 1e-9, "Σφ = {sum}");
                let (mut x, mut y) = (0.0, 0.0);
                for i in 0..n {
                    let (a, b) = (ccw[i], ccw[(i + 1) % n]);
                    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
                    let l = ex.hypot(ey);
                    x += phi[i] * a[0] + psi[i] * ey / l;
                    y += phi[i] * a[1] - psi[i] * ex / l;
                }
                assert!(close((x, y), (p[0], p[1]), 1e-8), "{p:?} → {x},{y}");
            }
        }
    }

    #[test]
    fn identity_translation_and_similarity_are_exact() {
        for coords in [CageCoords::Green, CageCoords::MeanValue] {
            for cage in [square(), l_shape(), square().into_iter().rev().collect()] {
                let id = CageMap::new(&cage, &cage, coords).unwrap();
                assert!(close(id.map(15.0, 15.0), (15.0, 15.0), 1e-9));
                let moved: Vec<[f64; 2]> = cage.iter().map(|p| [p[0] + 7.5, p[1] - 3.0]).collect();
                let t = CageMap::new(&cage, &moved, coords).unwrap();
                assert!(close(t.map(15.0, 16.0), (22.5, 13.0), 1e-7), "{coords:?} {:?}", t.map(15.0, 16.0));
                // Rotation by 30° and scale 1.5 about (40, 40).
                let (sn, cs) = 30f64.to_radians().sin_cos();
                let f = |p: [f64; 2]| {
                    let (x, y) = (p[0] - 40.0, p[1] - 40.0);
                    [40.0 + 1.5 * (x * cs - y * sn), 40.0 + 1.5 * (x * sn + y * cs)]
                };
                let sim: Vec<[f64; 2]> = cage.iter().map(|p| f(*p)).collect();
                let m = CageMap::new(&cage, &sim, coords).unwrap();
                let e = f([15.0, 16.0]);
                assert!(close(m.map(15.0, 16.0), (e[0], e[1]), 1e-6), "{coords:?} {:?} vs {e:?}", m.map(15.0, 16.0));
            }
        }
    }

    #[test]
    fn mean_value_coordinates_interpolate_the_boundary() {
        let cage = l_shape();
        let mut tgt = cage.clone();
        tgt[2] = [80.0, 30.0];
        let m = CageMap::new(&cage, &tgt, CageCoords::MeanValue).unwrap();
        // Vertices and edge midpoints land on the target cage.
        assert!(close(m.map(60.0, 20.0), (80.0, 30.0), 1e-9));
        assert!(close(m.map(60.0, 10.0), (70.0, 15.0), 1e-9));
        // Far from the moved vertex, little moves.
        let p = m.map(5.0, 75.0);
        assert!(close(p, (5.0, 75.0), 1.0), "{p:?}");
    }

    #[test]
    fn green_map_follows_a_dragged_vertex_and_is_continuous_at_the_edge() {
        let cage = square();
        let mut tgt = cage.clone();
        tgt[2] = [110.0, 90.0];
        let m = CageMap::new(&cage, &tgt, CageCoords::Green).unwrap();
        let near = m.map(85.0, 65.0);
        assert!(near.0 > 92.0 && near.1 > 72.0, "{near:?}");
        let far = m.map(15.0, 15.0);
        assert!(close(far, (15.0, 15.0), 3.0), "{far:?}");
        // Just inside and just outside the left edge agree.
        let (a, b) = (m.map(10.3, 40.0), m.map(9.0, 40.0));
        assert!(close(a, b, 0.5), "{a:?} vs {b:?}");
    }

    #[test]
    fn bad_cages_are_refused() {
        assert_eq!(CageMap::new(&[[0.0, 0.0], [1.0, 0.0]], &[[0.0, 0.0], [1.0, 0.0]], CageCoords::Green), Err(CageError::TooFewPoints));
        assert_eq!(CageMap::new(&square(), &square()[..3], CageCoords::Green), Err(CageError::LengthMismatch));
        let bow = vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
        assert_eq!(CageMap::new(&bow, &bow, CageCoords::Green), Err(CageError::SelfIntersecting));
        let flat = vec![[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]];
        assert_eq!(CageMap::new(&flat, &flat, CageCoords::Green), Err(CageError::Degenerate));
        let nan = vec![[0.0, 0.0], [f64::NAN, 0.0], [5.0, 5.0]];
        assert_eq!(CageMap::new(&nan, &nan, CageCoords::Green), Err(CageError::NotFinite));
        assert!(CageError::SelfIntersecting.to_string().contains("crosses"));
    }

    #[test]
    fn coords_parse_round_trips() {
        for c in [CageCoords::Green, CageCoords::MeanValue] {
            assert_eq!(CageCoords::parse(c.id()), Some(c));
        }
        assert_eq!(CageCoords::parse("x"), None);
    }
}
