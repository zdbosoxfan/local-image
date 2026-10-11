//! Bounded projective and bilinear mapping of native cubic paths.
//!
//! A homography produces rational cubics; a bilinear map produces degree-six
//! polynomials. Endpoint Hermite cubics are subdivided until a Bernstein convex
//! hull certificate bounds the pointwise error over the entire parameter interval.
//! This is an original implementation; no sampled-error success or handle-only warp.
use photocraft_doc::{Knot, Path, Subpath};
use photocraft_geom::Point;

pub const MAX_NODES: usize = 131_072;
pub const COORD_LIMIT: f64 = 1e7;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Horizon,
    Budget,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid or unbounded distortion geometry",
            Self::Horizon => "distortion crosses the projective horizon",
            Self::Budget => "distortion exceeds subdivision budget",
        })
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Map {
    /// Row-major homogeneous matrix, evaluated as (X/W, Y/W).
    Projective([f64; 9]),
    /// Source [x,y,width,height], destination TL,TR,BR,BL.
    Bilinear { source: [f64; 4], quad: [Point; 4] },
}
fn valid(p: Point) -> bool {
    [p.x, p.y].iter().all(|v| v.is_finite() && v.abs() <= COORD_LIMIT)
}
fn rect(r: [f64; 4]) -> Result<()> {
    if r.iter().any(|v| !v.is_finite() || v.abs() > COORD_LIMIT) || r[2] < 1e-6 || r[3] < 1e-6 {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn quad(q: [Point; 4]) -> Result<()> {
    if q.iter().any(|p| !valid(*p)) {
        return Err(Error::Invalid);
    }
    let mut sign = 0.;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let c = q[(i + 2) % 4];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() < 1e-10 || (sign != 0. && cross * sign <= 0.) {
            return Err(Error::Invalid);
        }
        sign = cross;
    }
    Ok(())
}
impl Map {
    pub fn bilinear(source: [f64; 4], destination: [Point; 4]) -> Result<Self> {
        rect(source)?;
        quad(destination)?;
        Ok(Self::Bilinear { source, quad: destination })
    }
    pub fn perspective(source: [f64; 4], q: [Point; 4]) -> Result<Self> {
        rect(source)?;
        quad(q)?;
        let dx1 = q[1].x - q[2].x;
        let dx2 = q[3].x - q[2].x;
        let dy1 = q[1].y - q[2].y;
        let dy2 = q[3].y - q[2].y;
        let dx3 = q[0].x - q[1].x + q[2].x - q[3].x;
        let dy3 = q[0].y - q[1].y + q[2].y - q[3].y;
        let det = dx1 * dy2 - dx2 * dy1;
        if det.abs() < 1e-12 {
            return Err(Error::Invalid);
        }
        let g = (dx3 * dy2 - dx2 * dy3) / det;
        let h = (dx1 * dy3 - dx3 * dy1) / det;
        let a = q[1].x - q[0].x + g * q[1].x;
        let b = q[3].x - q[0].x + h * q[3].x;
        let d = q[1].y - q[0].y + g * q[1].y;
        let e = q[3].y - q[0].y + h * q[3].y;
        let [x, y, w, height] = source;
        let m = [
            a / w,
            b / height,
            q[0].x - a * x / w - b * y / height,
            d / w,
            e / height,
            q[0].y - d * x / w - e * y / height,
            g / w,
            h / height,
            1. - g * x / w - h * y / height,
        ];
        let map = Self::Projective(m);
        map.validate()?;
        Ok(map)
    }
    pub fn validate(self) -> Result<()> {
        match self {
            Self::Projective(m) => {
                if m.iter().any(|v| !v.is_finite() || v.abs() > 1e14) {
                    return Err(Error::Invalid);
                }
                let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6]) + m[2] * (m[3] * m[7] - m[4] * m[6]);
                if !det.is_finite() || det.abs() < 1e-14 {
                    return Err(Error::Invalid);
                }
                Ok(())
            }
            Self::Bilinear { source, quad: q } => {
                rect(source)?;
                quad(q)
            }
        }
    }
    pub fn point(self, p: Point) -> Result<Point> {
        if !valid(p) {
            return Err(Error::Invalid);
        }
        let out = match self {
            Self::Projective(m) => {
                let w = m[6] * p.x + m[7] * p.y + m[8];
                if w.abs() < 1e-10 {
                    return Err(Error::Horizon);
                }
                Point::new((m[0] * p.x + m[1] * p.y + m[2]) / w, (m[3] * p.x + m[4] * p.y + m[5]) / w)
            }
            Self::Bilinear { source: [x, y, w, h], quad: q } => {
                let u = (p.x - x) / w;
                let v = (p.y - y) / h;
                Point::new(
                    q[0].x * (1. - u) * (1. - v) + q[1].x * u * (1. - v) + q[2].x * u * v + q[3].x * (1. - u) * v,
                    q[0].y * (1. - u) * (1. - v) + q[1].y * u * (1. - v) + q[2].y * u * v + q[3].y * (1. - u) * v,
                )
            }
        };
        if valid(out) { Ok(out) } else { Err(Error::Invalid) }
    }
    fn coefficients(self, p: [Point; 4]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        match self {
            Self::Projective(m) => (
                p.iter().map(|p| m[0] * p.x + m[1] * p.y + m[2]).collect(),
                p.iter().map(|p| m[3] * p.x + m[4] * p.y + m[5]).collect(),
                p.iter().map(|p| m[6] * p.x + m[7] * p.y + m[8]).collect(),
            ),
            Self::Bilinear { source: [x, y, w, h], quad: q } => {
                let u: Vec<_> = p.iter().map(|p| (p.x - x) / w).collect();
                let v: Vec<_> = p.iter().map(|p| (p.y - y) / h).collect();
                let uv = product(&u, &v);
                let u = elevate(u, 6);
                let v = elevate(v, 6);
                let make = |f: fn(Point) -> f64| {
                    (0..7)
                        .map(|i| f(q[0]) + (f(q[1]) - f(q[0])) * u[i] + (f(q[3]) - f(q[0])) * v[i] + (f(q[0]) - f(q[1]) + f(q[2]) - f(q[3])) * uv[i])
                        .collect()
                };
                (make(|p| p.x), make(|p| p.y), vec![1.])
            }
        }
    }
}
fn binomial(n: usize, k: usize) -> f64 {
    (0..k).fold(1., |x, i| x * (n - i) as f64 / (i + 1) as f64)
}
fn product(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len() - 1;
    let m = b.len() - 1;
    let mut out = vec![0.; n + m + 1];
    for i in 0..=n {
        for j in 0..=m {
            out[i + j] += a[i] * b[j] * binomial(n, i) * binomial(m, j) / binomial(n + m, i + j);
        }
    }
    out
}
fn elevate(mut a: Vec<f64>, n: usize) -> Vec<f64> {
    while a.len() <= n {
        let degree = a.len();
        let mut b = vec![a[0]; degree + 1];
        b[degree] = a[degree - 1];
        for i in 1..degree {
            let t = i as f64 / degree as f64;
            b[i] = t * a[i - 1] + (1. - t) * a[i];
        }
        a = b;
    }
    a
}
fn hermite(n: &[f64], d: &[f64]) -> [f64; 4] {
    let end = n.len() - 1;
    let de = d.len() - 1;
    let a = n[0] / d[0];
    let b = n[end] / d[de];
    let start = (end as f64 * (n[1] - n[0]) - a * de as f64 * (d.get(1).copied().unwrap_or(d[0]) - d[0])) / d[0];
    let finish = (end as f64 * (n[end] - n[end - 1]) - b * de as f64 * (d[de] - d[de.saturating_sub(1)])) / d[de];
    [a, a + start / 3., b - finish / 3., b]
}
fn certificate(n: &[f64], d: &[f64], q: &[f64; 4]) -> f64 {
    let degree = (d.len() + 2).max(n.len() - 1);
    let qd = elevate(product(q, d), degree);
    let nn = elevate(n.to_vec(), degree);
    let min = d.iter().map(|v| v.abs()).fold(f64::INFINITY, f64::min);
    let magnitude = qd.iter().chain(&nn).map(|v| v.abs()).fold(1., f64::max);
    // Conservative allowance for coefficient arithmetic and degree elevation roundoff.
    (qd.iter().zip(nn).map(|(a, b)| (a - b).abs()).fold(0., f64::max) + 64. * f64::EPSILON * magnitude) / min
}
fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}
fn split(p: [Point; 4]) -> ([Point; 4], [Point; 4]) {
    let a = mid(p[0], p[1]);
    let b = mid(p[1], p[2]);
    let c = mid(p[2], p[3]);
    let d = mid(a, b);
    let e = mid(b, c);
    let f = mid(d, e);
    ([p[0], a, d, f], [f, e, c, p[3]])
}
#[derive(Clone, Debug)]
pub struct Mapping {
    pub path: Path,
    /// Certified max pointwise error (px in destination coordinates).
    pub error_bound: f64,
    /// Original segment and parameter interval for each output cubic, in path order.
    pub intervals: Vec<(usize, usize, f64, f64)>,
}
fn fit(map: Map, p: [Point; 4], tol: f64, depth: u32, span: (f64, f64), out: &mut Vec<([Point; 4], f64, f64, f64)>) -> Result<()> {
    let (nx, ny, d) = map.coefficients(p);
    if d.iter().all(|v| *v >= 1e-10) || d.iter().all(|v| *v <= -1e-10) {
        let x = hermite(&nx, &d);
        let y = hermite(&ny, &d);
        let error = certificate(&nx, &d, &x).hypot(certificate(&ny, &d, &y));
        let q = std::array::from_fn(|i| Point::new(x[i], y[i]));
        if q.iter().any(|p| !valid(*p)) {
            return Err(Error::Invalid);
        }
        if error.is_finite() && error <= tol {
            if out.len() >= MAX_NODES {
                return Err(Error::Budget);
            }
            out.push((q, span.0, span.1, error));
            return Ok(());
        }
    } else if d[0] * d[d.len() - 1] <= 0. {
        return Err(Error::Horizon);
    }
    if depth >= 24 {
        return Err(Error::Budget);
    }
    let (a, b) = split(p);
    let m = (span.0 + span.1) * 0.5;
    fit(map, a, tol, depth + 1, (span.0, m), out)?;
    fit(map, b, tol, depth + 1, (m, span.1), out)
}
/// Maps every segment, preserving topology, closure, fill rules and component operations.
/// The error certificate covers whole intervals, including extrema between test samples.
pub fn map_path(path: &Path, map: Map, tolerance: f64) -> Result<Mapping> {
    map.validate()?;
    if !tolerance.is_finite()
        || !(1e-6..=0.25).contains(&tolerance)
        || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_NODES
        || path.subpaths.len() > MAX_NODES
    {
        return Err(Error::Invalid);
    }
    if path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| !valid(p)) {
        return Err(Error::Invalid);
    }
    let mut result =
        Mapping { path: Path { subpaths: Vec::new(), fill_rule: path.fill_rule, inverted: path.inverted }, error_bound: 0., intervals: Vec::new() };
    let mut count = 0;
    let mut nodes = 0;
    for (si, s) in path.subpaths.iter().enumerate() {
        let mut next = Subpath { closed: s.closed, op: s.op, knots: Vec::new() };
        if let Some(k) = s.knots.first() {
            let p = map.point(k.anchor)?;
            next.knots.push(Knot::corner(p.x, p.y));
        }
        for (seg, p) in s.segments().into_iter().enumerate() {
            let mut pieces = Vec::new();
            fit(map, p, tolerance, 0, (0., 1.), &mut pieces)?;
            for (q, a, b, error) in pieces {
                count += 1;
                if count > MAX_NODES {
                    return Err(Error::Budget);
                }
                if let Some(last) = next.knots.last_mut() {
                    last.out_ctrl = q[1];
                }
                next.knots.push(Knot {
                    anchor: q[3],
                    in_ctrl: q[2],
                    out_ctrl: q[3],
                    smooth: if b == 1. { s.knots[(seg + 1) % s.knots.len()].smooth } else { true },
                });
                result.error_bound = result.error_bound.max(error);
                result.intervals.push((si, seg, a, b));
            }
        }
        if s.closed
            && next.knots.len() > 1
            && let Some(last) = next.knots.pop()
        {
            next.knots[0].in_ctrl = last.in_ctrl;
        }
        if let (Some(first), Some(source)) = (next.knots.first_mut(), s.knots.first()) {
            first.smooth = source.smooth;
        }
        nodes += next.knots.len();
        if nodes > MAX_NODES {
            return Err(Error::Budget);
        }
        result.path.subpaths.push(next);
    }
    Ok(result)
}
#[cfg(test)]
#[path = "projective/tests.rs"]
mod tests;
