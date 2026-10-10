//! Non-destructive corner projection in native Path coordinates.
//!
//! Line-corner bisectors, setback/clamping and circular cuts adapted from VectorCraft
//! crates/geom/src/corners.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; notices in licenses/vectorcraft-LICENSE-{MIT,APACHE}.
//! Curved adjacency is a Local Image extension: trim the original cubics with de Casteljau,
//! solve a tangent circular fillet (or a scallop), and conservatively cap the cut to preserve a simple contour.
//! No pathops dependency: projection is consumed as an ordinary Path by renderers/adapters.

use photocraft_doc::vector_design::drawing::{CornerKind, KnotCorner};
use photocraft_doc::{Knot, Path, Subpath};
use photocraft_geom::Point;
use std::collections::BTreeSet;

pub const MAX_RADIUS: f64 = 1_000_000.0;
pub const MAX_COORDINATE: f64 = 10_000_000.0;
pub const MAX_KNOTS: usize = 16_384;
const MAX_SAMPLES: usize = 65_536;
const EPS: f64 = 1e-9;
const TOL: f64 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CornerKey {
    pub subpath: usize,
    pub knot: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerHandle {
    pub key: CornerKey,
    pub at: Point,
    /// Unit directions from the knot along the arriving and departing sides.
    pub u: Point,
    pub v: Point,
    pub max_radius: f64,
}

impl CornerHandle {
    pub fn radius_change(&self, delta: Point) -> f64 {
        dot(delta, add(self.u, self.v)) * cross(self.u, self.v).abs() / (2.0 + 2.0 * dot(self.u, self.v))
    }
    /// The circular fillet's centre; a minimum inset keeps sharp-corner controls hittable.
    pub fn widget(&self, radius: f64, zoom: f64) -> Point {
        let sin = cross(self.u, self.v).abs();
        let min = 10.0 / (zoom.max(0.01) * (1.0 + dot(self.u, self.v)).sqrt());
        add(self.at, mul(add(self.u, self.v), (radius.min(self.max_radius) / sin).max(min).min(self.max_radius / sin)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    pub path: Path,
    /// Each projected knot addresses a base knot. Never feed these indices back as base indices.
    pub sources: Vec<Vec<CornerKey>>,
    /// Effective, finite/clamped metadata, in base-index order.
    pub corners: Vec<KnotCorner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerError {
    NonFinite,
    Limit,
    InvalidIndex,
    Duplicate,
    NotCorner,
    SelfIntersection,
    NoFillet,
}
impl std::fmt::Display for CornerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "corner coordinates and radii must be finite",
            Self::Limit => "corner geometry exceeds the numeric or complexity budget",
            Self::InvalidIndex => "corner index does not address the base path",
            Self::Duplicate => "duplicate corner index",
            Self::NotCorner => "corner target is smooth, straight, degenerate or an open endpoint",
            Self::SelfIntersection => "corner projection cannot preserve a simple contour",
            Self::NoFillet => "a circular corner does not fit within the incident curves",
        })
    }
}
impl std::error::Error for CornerError {}

type Cubic = [Point; 4];
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Point, s: f64) -> Point {
    Point::new(a.x * s, a.y * s)
}
fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}
fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}
fn length(a: Point) -> f64 {
    a.x.hypot(a.y)
}
fn unit(a: Point) -> Option<Point> {
    let n = length(a);
    (n > EPS).then(|| mul(a, 1.0 / n))
}
fn lerp(a: Point, b: Point, t: f64) -> Point {
    add(mul(a, 1.0 - t), mul(b, t))
}
fn split(c: Cubic, t: f64) -> (Cubic, Cubic) {
    let a = lerp(c[0], c[1], t);
    let b = lerp(c[1], c[2], t);
    let d = lerp(c[2], c[3], t);
    let e = lerp(a, b, t);
    let f = lerp(b, d, t);
    let p = lerp(e, f, t);
    ([c[0], a, e, p], [p, f, d, c[3]])
}
fn eval(c: Cubic, t: f64) -> Point {
    split(c, t).0[3]
}
fn trim(c: Cubic, a: f64, b: f64) -> Cubic {
    if b <= a + 1e-14 {
        let p = eval(c, a);
        return [p; 4];
    }
    let left = split(c, b).0;
    if a <= 0.0 { left } else { split(left, a / b).1 }
}
fn tangent(c: Cubic, end: bool) -> Option<Point> {
    if end {
        unit(sub(c[3], c[2])).or_else(|| unit(sub(c[3], c[1]))).or_else(|| unit(sub(c[3], c[0])))
    } else {
        unit(sub(c[1], c[0])).or_else(|| unit(sub(c[2], c[0]))).or_else(|| unit(sub(c[3], c[0])))
    }
}
fn is_line(c: Cubic) -> bool {
    let d = sub(c[3], c[0]);
    let n = length(d);
    n > EPS
        && c[1..3].iter().all(|p| {
            let v = sub(*p, c[0]);
            cross(d, v).abs() <= 1e-9 * n && dot(v, d) >= -1e-9 * n && dot(v, d) <= dot(d, d) + 1e-9 * n
        })
}

/// Adaptive arclength table, also used for the independent-contour safety check. The second
/// difference bound controls chord deviation; control-polygon excess controls arclength error.
fn sample(c: Cubic, t0: f64, t1: f64, depth: usize, out: &mut Vec<(f64, Point)>) -> Result<(), CornerError> {
    if out.len() >= MAX_SAMPLES {
        return Err(CornerError::Limit);
    }
    let d = length(sub(add(c[0], c[2]), mul(c[1], 2.0))).max(length(sub(add(c[1], c[3]), mul(c[2], 2.0))));
    let polygon = length(sub(c[1], c[0])) + length(sub(c[2], c[1])) + length(sub(c[3], c[2]));
    let chord = length(sub(c[3], c[0]));
    if is_line(c) || (0.75 * d <= TOL && polygon - chord <= TOL * 0.01) {
        out.push((t1, c[3]));
        return Ok(());
    }
    if depth >= 24 {
        return Err(CornerError::Limit);
    }
    let (a, b) = split(c, 0.5);
    let mid = (t0 + t1) * 0.5;
    sample(a, t0, mid, depth + 1, out)?;
    sample(b, mid, t1, depth + 1, out)
}
struct Edge {
    c: Cubic,
    table: Vec<(f64, Point)>,
    lengths: Vec<f64>,
    len: f64,
}
impl Edge {
    fn new(c: Cubic) -> Result<Self, CornerError> {
        let mut table = vec![(0.0, c[0])];
        sample(c, 0.0, 1.0, 0, &mut table)?;
        let mut lengths = vec![0.0];
        let mut len = 0.0;
        for w in table.windows(2) {
            len += length(sub(w[1].1, w[0].1));
            lengths.push(len);
        }
        Ok(Self { c, table, lengths, len })
    }
    fn at_distance(&self, distance: f64) -> f64 {
        let distance = distance.clamp(0.0, self.len);
        if is_line(self.c) {
            let mut lo = 0.0;
            let mut hi = 1.0;
            for _ in 0..48 {
                let t = (lo + hi) * 0.5;
                if length(sub(eval(self.c, t), self.c[0])) < distance {
                    lo = t;
                } else {
                    hi = t;
                }
            }
            return (lo + hi) * 0.5;
        }
        let i = self.lengths.partition_point(|l| *l < distance).clamp(1, self.lengths.len() - 1);
        let f = (distance - self.lengths[i - 1]) / (self.lengths[i] - self.lengths[i - 1]).max(EPS);
        self.table[i - 1].0 + (self.table[i].0 - self.table[i - 1].0) * f
    }
}
fn validate(path: &Path) -> Result<(), CornerError> {
    if path.subpaths.len() > MAX_KNOTS || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_KNOTS {
        return Err(CornerError::Limit);
    }
    for p in path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(CornerError::NonFinite);
        }
        if p.x.abs().max(p.y.abs()) > MAX_COORDINATE {
            return Err(CornerError::Limit);
        }
    }
    Ok(())
}
fn edges(sp: &Subpath) -> Result<Vec<Edge>, CornerError> {
    sp.segments().into_iter().map(Edge::new).collect()
}
fn handle(sp: &Subpath, es: &[Edge], key: CornerKey) -> Option<CornerHandle> {
    let n = sp.knots.len();
    let i = key.knot;
    if n < 3 || sp.knots.get(i)?.smooth || (!sp.closed && (i == 0 || i + 1 == n)) {
        return None;
    }
    let incoming = es.get((i + n - 1) % n)?;
    let outgoing = es.get(i)?;
    let u = mul(tangent(incoming.c, true)?, -1.0);
    let v = tangent(outgoing.c, false)?;
    let sin = cross(u, v).abs();
    let cos = dot(u, v).clamp(-1.0, 1.0);
    if sin < 1e-6 || incoming.len.min(outgoing.len) <= EPS {
        return None;
    }
    Some(CornerHandle { key, at: sp.knots[i].anchor, u, v, max_radius: (incoming.len.min(outgoing.len) * 0.5 * sin / (1.0 + cos)).min(MAX_RADIUS) })
}

pub fn handles(path: &Path) -> Result<Vec<CornerHandle>, CornerError> {
    validate(path)?;
    let mut out = Vec::new();
    let mut samples = 0;
    for (s, sp) in path.subpaths.iter().enumerate() {
        let es = edges(sp)?;
        samples += es.iter().map(|e| e.table.len()).sum::<usize>();
        if samples > MAX_SAMPLES {
            return Err(CornerError::Limit);
        }
        let mut local: Vec<_> = (0..sp.knots.len()).filter_map(|knot| handle(sp, &es, CornerKey { subpath: s, knot })).collect();
        // A radius cap is a property of the base geometry, independent of the requested radius,
        // selected subset or kind. This keeps effective radii monotone while dragging. Test the
        // full eligible set and all cutting kinds at four fractions of the conservative cap.
        if !local.is_empty() && simple(sp)? {
            let mut factor = 1.0;
            let mut safe = false;
            for _ in 0..24 {
                safe = true;
                for kind in [CornerKind::Round, CornerKind::Inverted, CornerKind::Chamfer] {
                    let corners: Vec<_> = local
                        .iter()
                        .map(|h| KnotCorner {
                            subpath: s,
                            knot: h.key.knot,
                            corner: photocraft_doc::vector_design::drawing::Corner { radius: h.max_radius, kind },
                        })
                        .collect();
                    for fraction in [0.25, 0.5, 0.75, 1.0] {
                        let candidate = project_subpath(sp, &es, &corners, s, factor * fraction);
                        if match candidate {
                            Ok((path, _)) => !simple(&path)?,
                            Err(_) => true,
                        } {
                            safe = false;
                            break;
                        }
                    }
                    if !safe {
                        break;
                    }
                }
                if safe {
                    break;
                }
                factor *= 0.5;
            }
            if !safe {
                return Err(CornerError::SelfIntersection);
            }
            for h in &mut local {
                h.max_radius *= factor;
            }
        }
        out.extend(local);
    }
    Ok(out)
}

/// Circular arcs split to <= 11.25 degrees. Rounded-square area error <= 2e-8*r² + 1e-8
/// (Green's theorem on cubic polynomials), unlike a single kappa cubic per quarter circle.
fn arc(center: Point, start: Point, end: Point) -> Vec<Cubic> {
    let a = sub(start, center);
    let b = sub(end, center);
    let sweep = cross(a, b).atan2(dot(a, b));
    let radius = length(a);
    let n = (sweep.abs() / (std::f64::consts::PI / 16.0)).ceil().max(1.0) as usize;
    let theta = a.y.atan2(a.x);
    let step = sweep / n as f64;
    let h = radius * (4.0 / 3.0) * (step / 4.0).tan();
    (0..n)
        .map(|i| {
            let t0 = theta + i as f64 * step;
            let t1 = t0 + step;
            let p = if i == 0 { start } else { add(center, Point::new(radius * t0.cos(), radius * t0.sin())) };
            let q = if i + 1 == n { end } else { add(center, Point::new(radius * t1.cos(), radius * t1.sin())) };
            [p, add(p, Point::new(-h * t0.sin(), h * t0.cos())), sub(q, Point::new(-h * t1.sin(), h * t1.cos())), q]
        })
        .collect()
}
fn direction(c: Cubic, t: f64) -> Option<Point> {
    let u = 1.0 - t;
    unit(add(add(mul(sub(c[1], c[0]), u * u), mul(sub(c[2], c[1]), 2.0 * u * t)), mul(sub(c[3], c[2]), t * t))).or_else(|| tangent(c, t > 0.5))
}
fn offset(c: Cubic, t: f64, radius: f64, sign: f64) -> Option<Point> {
    let v = direction(c, t)?;
    Some(add(eval(c, t), mul(Point::new(-v.y, v.x), radius * sign)))
}
/// Intersection of the two radius-offset curves. Damped Newton stays within half of each
/// incident edge, so two neighbouring cuts cannot consume overlapping parameter intervals.
fn fillet(left: &Edge, right: &Edge, h: CornerHandle, radius: f64, setback: f64) -> Result<(f64, f64, Point), CornerError> {
    let min = left.at_distance(left.len * 0.5);
    let max = right.at_distance(right.len * 0.5);
    let mut a = left.at_distance(left.len - setback).clamp(min, 1.0);
    let mut b = right.at_distance(setback).clamp(0.0, max);
    let sign = cross(mul(h.u, -1.0), h.v).signum();
    for _ in 0..40 {
        let p = offset(left.c, a, radius, sign).ok_or(CornerError::NoFillet)?;
        let q = offset(right.c, b, radius, sign).ok_or(CornerError::NoFillet)?;
        let r = sub(p, q);
        let error = length(r);
        if error <= 1e-9 * radius.max(1.0) {
            return Ok((a, b, mul(add(p, q), 0.5)));
        }
        let derivative = |c: Cubic, t: f64| -> Option<Point> {
            let lo = (t - 1e-5).max(0.0);
            let hi = (t + 1e-5).min(1.0);
            Some(mul(sub(offset(c, hi, radius, sign)?, offset(c, lo, radius, sign)?), 1.0 / (hi - lo)))
        };
        let da = derivative(left.c, a).ok_or(CornerError::NoFillet)?;
        let db = mul(derivative(right.c, b).ok_or(CornerError::NoFillet)?, -1.0);
        let det = cross(da, db);
        if det.abs() < 1e-14 {
            return Err(CornerError::NoFillet);
        }
        let step_a = -cross(r, db) / det;
        let step_b = -cross(da, r) / det;
        let mut damping = 1.0;
        let mut improved = false;
        for _ in 0..12 {
            let na = (a + step_a * damping).clamp(min, 1.0);
            let nb = (b + step_b * damping).clamp(0.0, max);
            if let (Some(p), Some(q)) = (offset(left.c, na, radius, sign), offset(right.c, nb, radius, sign))
                && length(sub(p, q)) < error
            {
                a = na;
                b = nb;
                improved = true;
                break;
            }
            damping *= 0.5;
        }
        if !improved {
            return Err(CornerError::NoFillet);
        }
    }
    Err(CornerError::NoFillet)
}
fn radial_parameter(edge: &Edge, at: Point, distance: f64, arriving: bool) -> Result<f64, CornerError> {
    let half = edge.at_distance(edge.len * 0.5);
    let (mut lo, mut hi) = if arriving { (half, 1.0) } else { (0.0, half) };
    let reach = if arriving { lo } else { hi };
    if length(sub(eval(edge.c, reach), at)) + 1e-8 < distance {
        return Err(CornerError::NoFillet);
    }
    // Locally simple monotone branch from the knot. A turning/looping edge is bounded by the
    // half-edge domain and the final contour check; unsupported offset roots fail closed.
    for _ in 0..48 {
        let t = (lo + hi) * 0.5;
        let d = length(sub(eval(edge.c, t), at));
        if (d < distance) == arriving {
            hi = t;
        } else {
            lo = t;
        }
    }
    Ok((lo + hi) * 0.5)
}
fn cut(center: Point, incoming: Cubic, outgoing: Cubic, kind: CornerKind) -> Vec<Cubic> {
    let a = incoming[3];
    let b = outgoing[0];
    match kind {
        CornerKind::Round | CornerKind::Inverted => arc(center, a, b),
        CornerKind::Chamfer => vec![[a, a, b, b]],
        CornerKind::Straight => Vec::new(),
    }
}
fn push_segment(knots: &mut Vec<Knot>, sources: &mut Vec<CornerKey>, c: Cubic, from: CornerKey, to: CornerKey) {
    if knots.is_empty() {
        knots.push(Knot { anchor: c[0], in_ctrl: c[0], out_ctrl: c[0], smooth: false });
        sources.push(from);
    }
    if let Some(last) = knots.last_mut() {
        last.out_ctrl = c[1];
    }
    knots.push(Knot { anchor: c[3], in_ctrl: c[2], out_ctrl: c[3], smooth: false });
    sources.push(to);
}
fn project_subpath(sp: &Subpath, es: &[Edge], corners: &[KnotCorner], s: usize, factor: f64) -> Result<(Subpath, Vec<CornerKey>), CornerError> {
    let n = sp.knots.len();
    let mut incoming = vec![1.0; n];
    let mut outgoing = vec![0.0; n];
    let mut centers = vec![Point::default(); n];
    for kc in corners {
        if kc.corner.kind == CornerKind::Straight || kc.corner.radius <= EPS {
            continue;
        }
        if let Some(h) = handle(sp, es, CornerKey { subpath: s, knot: kc.knot }) {
            let setback = kc.corner.radius * factor * (1.0 + dot(h.u, h.v)) / cross(h.u, h.v).abs();
            let prev = (kc.knot + n - 1) % n;
            let radius = kc.corner.radius * factor;
            if kc.corner.kind == CornerKind::Round && !(is_line(es[prev].c) && is_line(es[kc.knot].c)) {
                let (a, b, center) = fillet(&es[prev], &es[kc.knot], h, radius, setback)?;
                incoming[kc.knot] = a;
                outgoing[kc.knot] = b;
                centers[kc.knot] = center;
            } else if kc.corner.kind == CornerKind::Inverted {
                incoming[kc.knot] = radial_parameter(&es[prev], h.at, setback, true)?;
                outgoing[kc.knot] = radial_parameter(&es[kc.knot], h.at, setback, false)?;
                centers[kc.knot] = h.at;
            } else {
                incoming[kc.knot] = es[prev].at_distance((es[prev].len - setback).max(0.0));
                outgoing[kc.knot] = es[kc.knot].at_distance(setback);
                centers[kc.knot] = add(h.at, mul(add(h.u, h.v), radius / cross(h.u, h.v).abs()));
            }
        }
    }
    let trimmed: Vec<_> = es.iter().enumerate().map(|(i, e)| trim(e.c, outgoing[i], incoming[(i + 1) % n])).collect();
    let mut knots = Vec::new();
    let mut sources = Vec::new();
    for (i, c) in trimmed.iter().enumerate() {
        let next = (i + 1) % n;
        let key = CornerKey { subpath: s, knot: next };
        push_segment(&mut knots, &mut sources, *c, CornerKey { subpath: s, knot: i }, key);
        if let Some(kc) = corners.iter().find(|k| k.knot == next && k.corner.kind != CornerKind::Straight && k.corner.radius > EPS) {
            for cut in cut(centers[next], *c, trimmed[next], kc.corner.kind) {
                push_segment(&mut knots, &mut sources, cut, key, key);
            }
        }
    }
    if sp.closed
        && knots.len() > 1
        && let Some(last) = knots.pop()
    {
        knots[0].in_ctrl = last.in_ctrl;
        sources.pop();
    }
    // Preserve original smoothness and unused open endpoint handles on untouched base knots.
    for (k, source) in knots.iter_mut().zip(&sources) {
        let base = sp.knots[source.knot];
        if k.anchor == base.anchor {
            k.smooth = base.smooth;
        }
    }
    if !sp.closed && !knots.is_empty() {
        knots[0].in_ctrl = sp.knots[0].in_ctrl;
        if let Some(last) = knots.last_mut() {
            last.out_ctrl = sp.knots[n - 1].out_ctrl;
        }
    }
    Ok((Subpath { closed: sp.closed, op: sp.op, knots }, sources))
}

fn simple(sp: &Subpath) -> Result<bool, CornerError> {
    let mut points = Vec::new();
    for c in sp.segments() {
        let mut table = vec![(0.0, c[0])];
        sample(c, 0.0, 1.0, 0, &mut table)?;
        for (_, p) in table {
            if points.last().is_none_or(|last| length(sub(p, *last)) > EPS) {
                points.push(p);
            }
        }
        if points.len() > MAX_SAMPLES {
            return Err(CornerError::Limit);
        }
    }
    if sp.closed && points.len() > 1 && length(sub(points[0], points[points.len() - 1])) < EPS {
        points.pop();
    }
    let count = if sp.closed { points.len() } else { points.len().saturating_sub(1) };
    let mut order: Vec<_> = (0..count).collect();
    order.sort_by(|a, b| points[*a].x.min(points[(*a + 1) % points.len()].x).total_cmp(&points[*b].x.min(points[(*b + 1) % points.len()].x)));
    let mut comparisons = 0;
    for (pos, &i) in order.iter().enumerate() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        for &j in &order[pos + 1..] {
            let c = points[j];
            let d = points[(j + 1) % points.len()];
            if c.x.min(d.x) > a.x.max(b.x) + EPS {
                break;
            }
            // Adjacent edges may share their endpoint, but a fold that overlaps for positive
            // length is invalid too. The proper-crossing products vanish at a shared endpoint.
            if c.y.min(d.y) > a.y.max(b.y) + EPS || a.y.min(b.y) > c.y.max(d.y) + EPS {
                continue;
            }
            comparisons += 1;
            if comparisons > 2_000_000 {
                return Err(CornerError::Limit);
            }
            let ab = sub(b, a);
            let cd = sub(d, c);
            let p = cross(ab, sub(c, a));
            let q = cross(ab, sub(d, a));
            let r = cross(cd, sub(a, c));
            let t = cross(cd, sub(b, c));
            if p * q < -1e-18 && r * t < -1e-18 {
                return Ok(false);
            }
            // Collinear overlaps, including a folded-back segment, are not simple either.
            if p.abs().max(q.abs()) < EPS && r.abs().max(t.abs()) < EPS {
                let overlap = if ab.x.abs() > ab.y.abs() {
                    a.x.max(b.x).min(c.x.max(d.x)) - a.x.min(b.x).max(c.x.min(d.x))
                } else {
                    a.y.max(b.y).min(c.y.max(d.y)) - a.y.min(b.y).max(c.y.min(d.y))
                };
                if overlap > EPS {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

/// Evaluate indexed metadata without editing the base path. Straight means sharp (radius is
/// retained for switching back). Round/Inverted/Chamfer share the same tangent-angle setback.
/// Round solves the two radius-offset curves; Inverted intersects the sides with a circle
/// centred on the base knot. Chamfer uses tangent-angle arclength setbacks. The static cap is
/// determined from base geometry; pathological/missing offset roots fail closed.
pub fn evaluate(path: &Path, metadata: &[KnotCorner]) -> Result<Projection, CornerError> {
    validate(path)?;
    if metadata.len() > MAX_KNOTS {
        return Err(CornerError::Limit);
    }
    if metadata.is_empty() {
        return Ok(Projection {
            path: path.clone(),
            corners: Vec::new(),
            sources: path.subpaths.iter().enumerate().map(|(subpath, sp)| (0..sp.knots.len()).map(|knot| CornerKey { subpath, knot }).collect()).collect(),
        });
    }
    let hs = handles(path)?;
    let mut seen = BTreeSet::new();
    let mut corners = metadata.to_vec();
    for kc in &mut corners {
        let key = CornerKey { subpath: kc.subpath, knot: kc.knot };
        if path.subpaths.get(kc.subpath).and_then(|s| s.knots.get(kc.knot)).is_none() {
            return Err(CornerError::InvalidIndex);
        }
        if !seen.insert(key) {
            return Err(CornerError::Duplicate);
        }
        if !kc.corner.radius.is_finite() {
            return Err(CornerError::NonFinite);
        }
        if kc.corner.radius.abs() > MAX_RADIUS {
            return Err(CornerError::Limit);
        }
        let h = hs.iter().find(|h| h.key == key).ok_or(CornerError::NotCorner)?;
        kc.corner.radius = kc.corner.radius.clamp(0.0, h.max_radius);
    }
    corners.sort_by_key(|c| (c.subpath, c.knot));
    let mut projected = path.clone();
    let mut sources = Vec::new();
    for (s, sp) in path.subpaths.iter().enumerate() {
        let own: Vec<_> = corners.iter().copied().filter(|c| c.subpath == s).collect();
        if own.iter().all(|c| c.corner.kind == CornerKind::Straight || c.corner.radius <= EPS) {
            sources.push((0..sp.knots.len()).map(|knot| CornerKey { subpath: s, knot }).collect());
            continue;
        }
        if !simple(sp)? {
            return Err(CornerError::SelfIntersection);
        }
        let es = edges(sp)?;
        let (new, from) = project_subpath(sp, &es, &own, s, 1.0)?;
        // The static cap is conservative; pathological intermediate fillets are rejected,
        // never silently given a nonmonotone radius or an unsafe contour.
        if !simple(&new)? {
            return Err(CornerError::SelfIntersection);
        }
        for c in &mut corners {
            if let Some(effective) = own.iter().find(|k| k.knot == c.knot && k.subpath == c.subpath) {
                *c = *effective;
            }
        }
        projected.subpaths[s] = new;
        sources.push(from);
    }
    if projected.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_SAMPLES {
        return Err(CornerError::Limit);
    }
    Ok(Projection { path: projected, sources, corners })
}

/// Package-03 bridge: after insert/delete/reorder, a complete old-base -> new-base index map
/// remaps corner metadata in the SAME node-edit transaction. Deleted knots map to None. New
/// knots acquire no corner automatically. The caller evaluates against the new path before commit.
pub fn remap_metadata(corners: &[KnotCorner], old_to_new: &[Vec<Option<CornerKey>>]) -> Result<Vec<KnotCorner>, CornerError> {
    let mut source = BTreeSet::new();
    let mut destination = BTreeSet::new();
    let mut out = Vec::new();
    if corners.len() > MAX_KNOTS {
        return Err(CornerError::Limit);
    }
    for corner in corners {
        let key = CornerKey { subpath: corner.subpath, knot: corner.knot };
        if !source.insert(key) {
            return Err(CornerError::Duplicate);
        }
        let mapped = old_to_new.get(key.subpath).and_then(|row| row.get(key.knot)).ok_or(CornerError::InvalidIndex)?;
        if let Some(key) = mapped {
            if !destination.insert(*key) {
                return Err(CornerError::Duplicate);
            }
            out.push(KnotCorner { subpath: key.subpath, knot: key.knot, corner: corner.corner });
        }
    }
    out.sort_by_key(|c| (c.subpath, c.knot));
    Ok(out)
}
