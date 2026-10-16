//! Live shape projection onto the existing document Path; no second object tree.
//!
//! Basic anchor layouts and grid/spiral conventions adapted from VectorCraft
//! `crates/geom/src/shapes.rs`, d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; retained notices: licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Extended families are original implementations of the shared ShapeParameters contract.
//! Ratios are dimensionless; corners/radii/arrow widths are document pixels; angles radians.
//! Segment is an annular sector (inner=0 gives a chord-bounded circular segment).
//! Grid rows/cols are cells, rings includes the outer ellipse. Grids/lines/arcs/spirals are stroked paths.

use photocraft_doc::vector_design::drawing::{CornerKind, DesignShape, ShapeParameters};
use photocraft_doc::{Knot, Path, PathOp, Subpath};
use photocraft_geom::Point;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub const MAX_COORDINATE: f64 = 1_000_000.0;
pub const MAX_COUNT: u32 = 256;
const EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeError(pub &'static str);
impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ShapeError {}
type Result<T> = std::result::Result<T, ShapeError>;
fn check(ok: bool, message: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(ShapeError(message)) }
}
fn bounded(v: f64, lo: f64, hi: f64) -> Result<()> {
    check(v.is_finite() && v >= lo && v <= hi, "shape parameter outside finite bounds")
}
fn count(n: u32, min: u32) -> Result<()> {
    check((min..=MAX_COUNT).contains(&n), "shape count outside bounds")
}
fn point(p: [f64; 2]) -> Result<()> {
    for v in p {
        bounded(v, -MAX_COORDINATE, MAX_COORDINATE)?;
    }
    Ok(())
}
fn endpoints(a: [f64; 2], b: [f64; 2]) -> Result<()> {
    point(a)?;
    point(b)?;
    check((a[0] - b[0]).hypot(a[1] - b[1]) >= EPS, "coincident endpoints")
}
fn ratio(v: f64) -> Result<()> {
    bounded(v, 0.0, 1.0)
}
fn inner(v: f64) -> Result<()> {
    bounded(v, EPS, 1.0 - EPS)
}
fn angles(a: f64, b: f64) -> Result<()> {
    bounded(a, -TAU * 100.0, TAU * 100.0)?;
    bounded(b, -TAU * 100.0, TAU * 100.0)?;
    check((b - a).abs() >= EPS && (b - a).abs() <= TAU + 1e-10, "arc sweep must be nonzero and at most one turn")
}

/// Validate the complete parameter set before allocating geometry or caches. No silent clamping.
pub fn validate(s: &DesignShape) -> Result<()> {
    for v in s.rect {
        bounded(v, -MAX_COORDINATE, MAX_COORDINATE)?;
    }
    let [x, y, w, h] = s.rect;
    check(w >= EPS && h >= EPS && (x + w).abs() <= MAX_COORDINATE && (y + h).abs() <= MAX_COORDINATE, "shape rectangle must have positive bounded dimensions")?;
    bounded(s.angle, -TAU * 100.0, TAU * 100.0)?;
    use ShapeParameters::*;
    match &s.parameters {
        Rect { radii, kinds } => {
            for r in radii {
                bounded(*r, 0.0, w.min(h) / 2.0)?;
            }
            let cut: f64 = radii
                .iter()
                .zip(kinds)
                .map(|(r, kind)| {
                    r * r
                        * match kind {
                            CornerKind::Round => 1. - PI / 4.,
                            CornerKind::Inverted => PI / 4.,
                            CornerKind::Chamfer => 0.5,
                            CornerKind::Straight => 1.,
                        }
                })
                .sum();
            check(w * h - cut > w * h * 1e-12, "rectangle corners consume the whole shape")?;
        }
        Ellipse => {}
        Polygon { sides, curved, corner } => {
            count(*sides, 3)?;
            bounded(*curved, -1.0, 1.0)?;
            bounded(*corner, 0.0, w.min(h) / 2.0)?;
        }
        Star { points, inner: r, corner } | SquareStar { points, inner: r, corner } => {
            count(*points, 3)?;
            inner(*r)?;
            bounded(*corner, 0.0, w.min(h) / 2.0)?;
        }
        DoubleStar { points, inner: r, secondary, corner } => {
            count(*points, 3)?;
            inner(*r)?;
            bounded(*secondary, *r + EPS, 1.0 - EPS)?;
            bounded(*corner, 0.0, w.min(h) / 2.0)?;
        }
        Triangle { top } | Diamond { top } => ratio(*top)?,
        Trapezoid { top_left, top_right } => {
            ratio(*top_left)?;
            ratio(*top_right)?;
            check(top_right - top_left >= EPS, "trapezoid top edge is degenerate")?;
        }
        Donut { hole } => bounded(*hole, 0.0, 1.0 - EPS)?,
        Pie { start, end } | Arc { start, end } => angles(*start, *end)?,
        Segment { start, end, inner: r } => {
            angles(*start, *end)?;
            bounded(*r, 0.0, 1.0 - EPS)?;
        }
        Arrow { from, to, head_width, head_length, shaft, double } => {
            endpoints(*from, *to)?;
            let len = (to[0] - from[0]).hypot(to[1] - from[1]);
            bounded(*head_width, EPS, MAX_COORDINATE)?;
            bounded(*shaft, EPS, *head_width)?;
            bounded(*head_length, EPS, len / if *double { 2.0 } else { 1.0 } - EPS)?;
        }
        Callout { tail, radius, ellipse } => {
            point(*tail)?;
            bounded(*radius, 0.0, w.min(h) / 2.0)?;
            let u = (tail[0] - x - w / 2.0) / (w / 2.0);
            let v = (tail[1] - y - h / 2.0) / (h / 2.0);
            check(if *ellipse { u * u + v * v > 1.0 + EPS } else { u.abs() > 1.0 + EPS || v.abs() > 1.0 + EPS }, "callout tail must be outside the body")?;
        }
        Cog { teeth, inner: r, hole, curve } => {
            count(*teeth, 3)?;
            inner(*r)?;
            bounded(*hole, 0.0, *r * (PI / f64::from(*teeth)).cos() - EPS)?;
            bounded(*curve, -1.0, 1.0)?;
        }
        Heart { depth } => ratio(*depth)?,
        Tear { bend } => bounded(*bend, -1.0, 1.0)?,
        Crescent { offset } => bounded(*offset, EPS, 2.0 - EPS)?,
        Cloud { bubbles } => count(*bubbles, 3)?,
        Spiral { center, radius, decay, turns } => {
            point(*center)?;
            bounded(*radius, EPS, MAX_COORDINATE / 4.0)?;
            bounded(*decay, 0.05, 1.0)?;
            bounded(*turns, 0.25, 64.0)?;
        }
        Line { from, to } => endpoints(*from, *to)?,
        RectGrid { rows, cols } => {
            count(*rows, 1)?;
            count(*cols, 1)?;
        }
        PolarGrid { rings, spokes } => {
            count(*rings, 1)?;
            count(*spokes, 1)?;
        }
    }
    Ok(())
}
fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}
fn mix(a: Point, b: Point, t: f64) -> Point {
    p(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn poly(pts: &[Point], closed: bool) -> Subpath {
    Subpath { closed, knots: pts.iter().map(|v| Knot::corner(v.x, v.y)).collect(), op: PathOp::Combine }
}
fn reverse(sp: &mut Subpath) {
    sp.knots.reverse();
    for k in &mut sp.knots {
        std::mem::swap(&mut k.in_ctrl, &mut k.out_ctrl);
    }
}
fn hole(path: &mut Path, mut sp: Subpath) {
    reverse(&mut sp);
    sp.op = PathOp::Join;
    path.subpaths.push(sp);
}
/// Cubic elliptical arcs, subdivided at eighth turns. Works with negative sweeps as well.
fn arc(cx: f64, cy: f64, rx: f64, ry: f64, a: f64, b: f64) -> Subpath {
    let n = ((b - a).abs() / (PI / 4.0)).ceil().max(1.0) as usize;
    let step = (b - a) / n as f64;
    let mut knots = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = a + step * i as f64;
        let q = p(cx + rx * t.cos(), cy + ry * t.sin());
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let tangent = p(-rx * t.sin() * k, ry * t.cos() * k);
        knots.push(Knot {
            anchor: q,
            in_ctrl: if i == 0 { q } else { p(q.x - tangent.x, q.y - tangent.y) },
            out_ctrl: if i == n { q } else { p(q.x + tangent.x, q.y + tangent.y) },
            smooth: true,
        });
    }
    Subpath { closed: false, knots, op: PathOp::Combine }
}
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Subpath {
    let mut sp = arc(cx, cy, rx, ry, -FRAC_PI_2, TAU - FRAC_PI_2);
    if let Some(last) = sp.knots.pop() {
        sp.knots[0].in_ctrl = last.in_ctrl;
    }
    sp.closed = true;
    sp
}
fn rounded(pts: &[Point], radius: f64) -> Subpath {
    if radius == 0.0 {
        return poly(pts, true);
    }
    // Quadratic fillets converted exactly to cubic segments. Each cut is capped to half an edge.
    let mut knots = Vec::with_capacity(pts.len() * 2);
    for i in 0..pts.len() {
        let q = pts[i];
        let prev = pts[(i + pts.len() - 1) % pts.len()];
        let next = pts[(i + 1) % pts.len()];
        let d1 = (q.x - prev.x).hypot(q.y - prev.y);
        let d2 = (q.x - next.x).hypot(q.y - next.y);
        let a = mix(q, prev, (radius / d1).min(0.5));
        let b = mix(q, next, (radius / d2).min(0.5));
        knots.push(Knot { anchor: a, in_ctrl: a, out_ctrl: mix(a, q, 2.0 / 3.0), smooth: false });
        knots.push(Knot { anchor: b, in_ctrl: mix(b, q, 2.0 / 3.0), out_ctrl: b, smooth: false });
    }
    Subpath { closed: true, knots, op: PathOp::Combine }
}
fn curved(mut sp: Subpath, amount: f64) -> Subpath {
    if amount == 0.0 {
        return sp;
    }
    let n = sp.knots.len();
    for i in 0..n {
        let a = sp.knots[i].anchor;
        let b = sp.knots[(i + 1) % n].anchor;
        let normal = p((b.y - a.y) * amount / 3.0, -(b.x - a.x) * amount / 3.0);
        let c = mix(a, b, 1.0 / 3.0);
        let d = mix(a, b, 2.0 / 3.0);
        sp.knots[i].out_ctrl = p(c.x + normal.x, c.y + normal.y);
        sp.knots[(i + 1) % n].in_ctrl = p(d.x + normal.x, d.y + normal.y);
    }
    sp
}
fn rectangle(x: f64, y: f64, w: f64, h: f64, r: [f64; 4], kinds: [CornerKind; 4]) -> Subpath {
    let vertices = [p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h)];
    let mut knots = Vec::new();
    for i in 0..4 {
        let q = vertices[i];
        let prev = vertices[(i + 3) % 4];
        let next = vertices[(i + 1) % 4];
        if r[i] == 0.0 {
            knots.push(Knot::corner(q.x, q.y));
            continue;
        }
        let a = mix(q, prev, r[i] / (q.x - prev.x).hypot(q.y - prev.y));
        let b = mix(q, next, r[i] / (q.x - next.x).hypot(q.y - next.y));
        let v = p(a.x + b.x - q.x, a.y + b.y - q.y);
        if kinds[i] == CornerKind::Straight {
            knots.extend([Knot::corner(a.x, a.y), Knot::corner(v.x, v.y), Knot::corner(b.x, b.y)]);
            continue;
        }
        let target = match kinds[i] {
            CornerKind::Round => q,
            CornerKind::Inverted => v,
            CornerKind::Chamfer => mix(a, b, 0.5),
            CornerKind::Straight => v,
        };
        let k = 0.552_284_749_830_793_6;
        let (c, d) = if kinds[i] == CornerKind::Straight { (v, v) } else { (mix(a, target, k), mix(b, target, k)) };
        knots.push(Knot { anchor: a, in_ctrl: a, out_ctrl: c, smooth: false });
        knots.push(Knot { anchor: b, in_ctrl: d, out_ctrl: b, smooth: false });
    }
    Subpath { closed: true, knots, op: PathOp::Combine }
}
fn cubic_path(segments: &[[Point; 4]]) -> Subpath {
    let mut knots: Vec<Knot> = Vec::new();
    for [a, c, d, b] in segments {
        if let Some(last) = knots.last_mut() {
            last.out_ctrl = *c;
        } else {
            knots.push(Knot { anchor: *a, in_ctrl: *a, out_ctrl: *c, smooth: false });
        }
        knots.push(Knot { anchor: *b, in_ctrl: *d, out_ctrl: *b, smooth: false });
    }
    if let Some(last) = knots.pop() {
        knots[0].in_ctrl = last.in_ctrl;
    }
    Subpath { closed: true, knots, op: PathOp::Combine }
}

/// Generate authoritative base geometry. All handles and numeric controls regenerate this path.
pub fn generate(s: &DesignShape) -> Result<Path> {
    validate(s)?;
    let [x, y, w, h] = s.rect;
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    let at = |u: f64, v: f64| p(cx + rx * u, cy + ry * v);
    let radial = |a: f64, r: f64| at(r * a.cos(), r * a.sin());
    let mut path = Path::default();
    use ShapeParameters::*;
    let sp = match &s.parameters {
        Rect { radii, kinds } => rectangle(x, y, w, h, *radii, *kinds),
        Ellipse => ellipse(cx, cy, rx, ry),
        Polygon { sides, curved: c, corner } => {
            let pts: Vec<_> = (0..*sides).map(|i| radial(-FRAC_PI_2 + TAU * f64::from(i) / f64::from(*sides), 1.0)).collect();
            curved(rounded(&pts, *corner), *c)
        }
        Star { points, inner: r, corner } | DoubleStar { points, inner: r, corner, .. } | SquareStar { points, inner: r, corner } => {
            let mut pts = Vec::new();
            let n = f64::from(*points);
            for i in 0..*points {
                let a = -FRAC_PI_2 + TAU * f64::from(i) / n;
                match &s.parameters {
                    DoubleStar { secondary, .. } => {
                        pts.extend([radial(a, 1.0), radial(a + PI / (2.0 * n), *r), radial(a + PI / n, *secondary), radial(a + 3.0 * PI / (2.0 * n), *r)]);
                    }
                    SquareStar { .. } => {
                        let t = PI / (2.0 * n);
                        pts.extend([radial(a - t, 1.0), radial(a + t, 1.0), radial(a + t, *r), radial(a + 3.0 * t, *r)]);
                    }
                    _ => pts.extend([radial(a, 1.0), radial(a + PI / n, *r)]),
                }
            }
            rounded(&pts, *corner)
        }
        Triangle { top } => poly(&[p(x + w * top, y), p(x + w, y + h), p(x, y + h)], true),
        Trapezoid { top_left, top_right } => poly(&[p(x + w * top_left, y), p(x + w * top_right, y), p(x + w, y + h), p(x, y + h)], true),
        Diamond { top } => poly(&[at(0.0, -1.0), p(x + w, y + h * top), at(0.0, 1.0), p(x, y + h * top)], true),
        Donut { hole: r } => {
            path.subpaths.push(ellipse(cx, cy, rx, ry));
            if *r > 0.0 {
                hole(&mut path, ellipse(cx, cy, rx * r, ry * r));
            }
            return rotate_checked(s, path);
        }
        Pie { start, end } => {
            let mut sp = arc(cx, cy, rx, ry, *start, *end);
            if (end - start).abs() < TAU - 1e-10 {
                sp.knots.push(Knot::corner(cx, cy));
            } else if let Some(last) = sp.knots.pop() {
                sp.knots[0].in_ctrl = last.in_ctrl;
            }
            sp.closed = true;
            sp
        }
        Segment { start, end, inner: r } => {
            if (end - start).abs() >= TAU - 1e-10 {
                path.subpaths.push(ellipse(cx, cy, rx, ry));
                if *r > 0.0 {
                    hole(&mut path, ellipse(cx, cy, rx * r, ry * r));
                }
                return rotate_checked(s, path);
            }
            let mut sp = arc(cx, cy, rx, ry, *start, *end);
            if *r > 0.0 {
                sp.knots.extend(arc(cx, cy, rx * r, ry * r, *end, *start).knots);
            }
            sp.closed = true;
            sp
        }
        Arc { start, end } => arc(cx, cy, rx, ry, *start, *end),
        Line { from, to } => poly(&[p(from[0], from[1]), p(to[0], to[1])], false),
        Arrow { from, to, head_width, head_length, shaft, double } => {
            let a = p(from[0], from[1]);
            let b = p(to[0], to[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
            let pos = |t: f64, d: f64| p(a.x + ux * t - uy * d, a.y + uy * t + ux * d);
            let mut pts = vec![
                pos(if *double { *head_length } else { 0.0 }, -shaft / 2.0),
                pos(len - head_length, -shaft / 2.0),
                pos(len - head_length, -head_width / 2.0),
                b,
                pos(len - head_length, head_width / 2.0),
                pos(len - head_length, shaft / 2.0),
                pos(if *double { *head_length } else { 0.0 }, shaft / 2.0),
            ];
            if *double {
                pts.extend([pos(*head_length, head_width / 2.0), a, pos(*head_length, -head_width / 2.0)]);
            }
            poly(&pts, true)
        }
        Callout { ellipse: e, tail, radius } => {
            let mut sp = if *e { ellipse(cx, cy, rx, ry) } else { rectangle(x, y, w, h, [*radius; 4], [CornerKind::Round; 4]) };
            // Split the nearest boundary segment at two parameters and splice a tail into that edge.
            let t = p(tail[0], tail[1]);
            let segments = sp.segments();
            let (index, _) = segments
                .iter()
                .enumerate()
                .map(|(i, q)| {
                    let m = bezier(*q, 0.5);
                    (i, (m.x - t.x).hypot(m.y - t.y))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .ok_or(ShapeError("empty callout"))?;
            let mut q = segments[index];
            if q[0] == q[1] && q[2] == q[3] {
                q[1] = mix(q[0], q[3], 1. / 3.);
                q[2] = mix(q[0], q[3], 2. / 3.);
            }
            let left = split(q, 0.4).0;
            let right = split(q, 0.6).1;
            sp.knots[index].out_ctrl = left[1];
            sp.knots[(index + 1) % segments.len()].in_ctrl = right[2];
            sp.knots.splice(
                index + 1..index + 1,
                [
                    Knot { anchor: left[3], in_ctrl: left[2], out_ctrl: left[3], smooth: false },
                    Knot::corner(t.x, t.y),
                    Knot { anchor: right[0], in_ctrl: right[0], out_ctrl: right[1], smooth: false },
                ],
            );
            sp
        }
        Cog { teeth, inner: r, hole: h, curve } => {
            let pts: Vec<_> =
                (0..*teeth * 4).map(|i| radial(-FRAC_PI_2 + TAU * f64::from(i) / f64::from(*teeth * 4), if i % 4 < 2 { 1.0 } else { *r })).collect();
            path.subpaths.push(curved(poly(&pts, true), *curve));
            if *h > 0.0 {
                hole(&mut path, ellipse(cx, cy, rx * h, ry * h));
            }
            return rotate_checked(s, path);
        }
        Heart { depth } => {
            let d = -1.0 + depth;
            cubic_path(&[
                [at(0.0, d), at(-0.5, -1.5), at(-1.0, -1.0), at(-1.0, -0.3)],
                [at(-1.0, -0.3), at(-1.0, 0.2), at(-0.3, 0.7), at(0.0, 1.0)],
                [at(0.0, 1.0), at(0.3, 0.7), at(1.0, 0.2), at(1.0, -0.3)],
                [at(1.0, -0.3), at(1.0, -1.0), at(0.5, -1.5), at(0.0, d)],
            ])
        }
        Tear { bend } => cubic_path(&[
            [at(0.0, -1.0), at(1.0 + bend, -0.3), at(1.0, 1.0), at(0.0, 1.0)],
            [at(0.0, 1.0), at(-1.0, 1.0), at(-1.0 + bend, -0.3), at(0.0, -1.0)],
        ]),
        Crescent { offset } => {
            let theta = (offset / 2.0).acos();
            let mut sp = arc(cx, cy, rx, ry, theta, TAU - theta);
            let inside = arc(cx + rx * offset, cy, rx, ry, PI + theta, PI - theta);
            if let Some(first) = inside.knots.first()
                && let Some(last) = sp.knots.last_mut()
            {
                last.out_ctrl = first.out_ctrl;
            }
            sp.knots.extend(inside.knots.into_iter().skip(1));
            if let Some(last) = sp.knots.pop() {
                sp.knots[0].in_ctrl = last.in_ctrl;
            }
            sp.closed = true;
            sp
        }
        Cloud { bubbles } => {
            let n = *bubbles as usize * 16;
            let dt = TAU / n as f64;
            let mut knots = Vec::with_capacity(n);
            for i in 0..n {
                let a = -FRAC_PI_2 + dt * i as f64;
                let r = 0.85 + 0.15 * (f64::from(*bubbles) * (a + FRAC_PI_2)).cos();
                let dr = -0.15 * f64::from(*bubbles) * (f64::from(*bubbles) * (a + FRAC_PI_2)).sin();
                let q = radial(a, r);
                let dx = rx * (dr * a.cos() - r * a.sin()) * dt / 3.0;
                let dy = ry * (dr * a.sin() + r * a.cos()) * dt / 3.0;
                knots.push(Knot::smooth(q, p(q.x - dx, q.y - dy), p(q.x + dx, q.y + dy)));
            }
            Subpath { closed: true, knots, op: PathOp::Combine }
        }
        Spiral { center, radius, decay, turns } => {
            let n = (turns * 16.0).ceil() as usize;
            let dt = turns * TAU / n as f64;
            let log = decay.ln() / FRAC_PI_2;
            let mut knots = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let a = dt * i as f64;
                let r = radius * (log * a).exp();
                let q = p(center[0] + r * a.cos(), center[1] + r * a.sin());
                let dx = r * (log * a.cos() - a.sin()) * dt / 3.0;
                let dy = r * (log * a.sin() + a.cos()) * dt / 3.0;
                knots.push(Knot::smooth(q, if i == 0 { q } else { p(q.x - dx, q.y - dy) }, if i == n { q } else { p(q.x + dx, q.y + dy) }));
            }
            Subpath { closed: false, knots, op: PathOp::Combine }
        }
        RectGrid { rows, cols } => {
            path.subpaths.push(poly(&[p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h)], true));
            for i in 1..*rows {
                let yy = y + h * f64::from(i) / f64::from(*rows);
                path.subpaths.push(poly(&[p(x, yy), p(x + w, yy)], false));
            }
            for i in 1..*cols {
                let xx = x + w * f64::from(i) / f64::from(*cols);
                path.subpaths.push(poly(&[p(xx, y), p(xx, y + h)], false));
            }
            return rotate_checked(s, path);
        }
        PolarGrid { rings, spokes } => {
            for i in 1..=*rings {
                let r = f64::from(i) / f64::from(*rings);
                path.subpaths.push(ellipse(cx, cy, rx * r, ry * r));
            }
            for i in 0..*spokes {
                path.subpaths.push(poly(&[p(cx, cy), radial(-FRAC_PI_2 + TAU * f64::from(i) / f64::from(*spokes), 1.0)], false));
            }
            return rotate_checked(s, path);
        }
    };
    path.subpaths.push(sp);
    rotate_checked(s, path)
}
fn rotate_checked(s: &DesignShape, mut path: Path) -> Result<Path> {
    let [x, y, w, h] = s.rect;
    let c = p(x + w / 2.0, y + h / 2.0);
    for k in path.subpaths.iter_mut().flat_map(|sp| &mut sp.knots) {
        for q in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
            *q = rotate(*q, c, s.angle);
            bounded(q.x, -MAX_COORDINATE, MAX_COORDINATE)?;
            bounded(q.y, -MAX_COORDINATE, MAX_COORDINATE)?;
        }
    }
    Ok(path)
}
pub fn rotate(q: Point, c: Point, a: f64) -> Point {
    let (sn, cs) = a.sin_cos();
    p(c.x + (q.x - c.x) * cs - (q.y - c.y) * sn, c.y + (q.x - c.x) * sn + (q.y - c.y) * cs)
}
fn bezier(q: [Point; 4], t: f64) -> Point {
    let a = mix(q[0], q[1], t);
    let b = mix(q[1], q[2], t);
    let c = mix(q[2], q[3], t);
    mix(mix(a, b, t), mix(b, c, t), t)
}
fn split(q: [Point; 4], t: f64) -> ([Point; 4], [Point; 4]) {
    let a = mix(q[0], q[1], t);
    let b = mix(q[1], q[2], t);
    let c = mix(q[2], q[3], t);
    let d = mix(a, b, t);
    let e = mix(b, c, t);
    let f = mix(d, e, t);
    ([q[0], a, d, f], [f, e, c, q[3]])
}

/// True for families intended to be stroked rather than implicitly filled.
pub fn stroke_only(parameters: &ShapeParameters) -> bool {
    matches!(
        parameters,
        ShapeParameters::Line { .. }
            | ShapeParameters::Arc { .. }
            | ShapeParameters::Spiral { .. }
            | ShapeParameters::RectGrid { .. }
            | ShapeParameters::PolarGrid { .. }
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    Size,
    Rotation,
    Corner(usize),
    Inner,
    Secondary,
    Hole,
    Start,
    End,
    Top,
    TopLeft,
    TopRight,
    Depth,
    Bend,
    Offset,
    Curvature,
    From,
    To,
    Tail,
    Center,
    Radius,
    HeadWidth,
    HeadLength,
    Shaft,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub position: Point,
}
/// Parameter handles in document space, including the common resize and rotation handles.
pub fn handles(s: &DesignShape) -> Result<Vec<Handle>> {
    validate(s)?;
    let [x, y, w, h] = s.rect;
    let c = p(x + w / 2.0, y + h / 2.0);
    let at = |u: f64, v: f64| p(x + w * u, y + h * v);
    let mut out = vec![Handle { kind: HandleKind::Rotation, position: at(0.5, -0.15) }];
    if !matches!(s.parameters, ShapeParameters::Line { .. } | ShapeParameters::Arrow { .. } | ShapeParameters::Spiral { .. }) {
        out.insert(0, Handle { kind: HandleKind::Size, position: at(1.0, 1.0) });
    }
    let mut add = |kind, position| out.push(Handle { kind, position });
    use HandleKind as H;
    use ShapeParameters::*;
    match &s.parameters {
        Rect { radii, .. } => {
            for (i, r) in radii.iter().enumerate() {
                let q = match i {
                    0 => p(x + r, y + r),
                    1 => p(x + w - r, y + r),
                    2 => p(x + w - r, y + h - r),
                    _ => p(x + r, y + h - r),
                };
                add(H::Corner(i), q);
            }
        }
        Polygon { curved, corner, .. } => {
            add(H::Curvature, at(0.5 + curved / 2.0, 0.0));
            add(H::Corner(0), p(x + corner, y + corner));
        }
        Star { points, inner, corner } | DoubleStar { points, inner, corner, .. } | SquareStar { points, inner, corner } => {
            let a = -FRAC_PI_2 + PI / f64::from(*points);
            add(H::Inner, p(c.x + w / 2.0 * inner * a.cos(), c.y + h / 2.0 * inner * a.sin()));
            add(H::Corner(0), p(x + corner, y + corner));
            if let DoubleStar { secondary, .. } = &s.parameters {
                add(H::Secondary, p(c.x + w / 2.0 * secondary, c.y));
            }
        }
        Triangle { top } | Diamond { top } => add(H::Top, if matches!(s.parameters, Triangle { .. }) { at(*top, 0.0) } else { at(1.0, *top) }),
        Trapezoid { top_left, top_right } => {
            add(H::TopLeft, at(*top_left, 0.0));
            add(H::TopRight, at(*top_right, 0.0));
        }
        Donut { hole } => add(H::Hole, at(0.5 + hole / 2.0, 0.5)),
        Pie { start, end } | Arc { start, end } | Segment { start, end, .. } => {
            add(H::Start, at(0.5 + start.cos() / 2.0, 0.5 + start.sin() / 2.0));
            add(H::End, at(0.5 + end.cos() / 2.0, 0.5 + end.sin() / 2.0));
            if let Segment { inner, .. } = &s.parameters {
                add(H::Inner, at(0.5 + inner / 2.0, 0.5));
            }
        }
        Arrow { from, to, head_width, head_length, shaft, .. } => {
            add(H::From, p(from[0], from[1]));
            add(H::To, p(to[0], to[1]));
            let len = (to[0] - from[0]).hypot(to[1] - from[1]);
            let (ux, uy) = ((to[0] - from[0]) / len, (to[1] - from[1]) / len);
            let pos = |t: f64, d: f64| p(from[0] + ux * t - uy * d, from[1] + uy * t + ux * d);
            add(H::HeadLength, pos(len - head_length, 0.0));
            add(H::HeadWidth, pos(len - head_length, head_width / 2.0));
            add(H::Shaft, pos(len / 2.0, shaft / 2.0));
        }
        Callout { tail, radius, ellipse } => {
            add(H::Tail, p(tail[0], tail[1]));
            if !ellipse {
                add(H::Corner(0), p(x + radius, y + radius));
            }
        }
        Cog { inner, hole, curve, .. } => {
            add(H::Inner, at(0.5, 0.5 - inner / 2.0));
            add(H::Hole, at(0.5 + hole / 2.0, 0.5));
            add(H::Curvature, at(0.5 + curve / 2.0, 0.0));
        }
        Heart { depth } => add(H::Depth, at(0.5, depth / 2.0)),
        Tear { bend } => add(H::Bend, at(0.5 + bend / 2.0, 0.35)),
        Crescent { offset } => add(H::Offset, at(0.5 + offset / 4.0, 0.5)),
        Spiral { center, radius, .. } => {
            add(H::Center, p(center[0], center[1]));
            add(H::Radius, p(center[0] + radius, center[1]));
        }
        Line { from, to } => {
            add(H::From, p(from[0], from[1]));
            add(H::To, p(to[0], to[1]));
        }
        Ellipse | Cloud { .. } | RectGrid { .. } | PolarGrid { .. } => {}
    }
    for h in &mut out {
        h.position = rotate(h.position, c, s.angle);
    }
    Ok(out)
}

/// Returns a validated detached replacement. Invalid drags never mutate the starting metadata.
pub fn drag_handle(s: &DesignShape, kind: HandleKind, position: Point) -> Result<DesignShape> {
    validate(s)?;
    point([position.x, position.y])?;
    let mut out = s.clone();
    let [x, y, w, h] = s.rect;
    let c = p(x + w / 2.0, y + h / 2.0);
    let q = rotate(position, c, -s.angle);
    let (u, v) = ((q.x - x) / w, (q.y - y) / h);
    let r = ((u - 0.5) * 2.0).hypot((v - 0.5) * 2.0);
    let cr = (q.x - x).abs().min((q.y - y).abs());
    use HandleKind as H;
    use ShapeParameters::*;
    match kind {
        H::Size => {
            check(
                !matches!(s.parameters, ShapeParameters::Line { .. } | ShapeParameters::Arrow { .. } | ShapeParameters::Spiral { .. }),
                "use endpoint or radius handles for this shape",
            )?;
            let fixed = rotate(p(x, y), c, s.angle);
            let delta = rotate(p(position.x - fixed.x, position.y - fixed.y), p(0., 0.), -s.angle);
            let center = rotate(p(delta.x / 2., delta.y / 2.), p(0., 0.), s.angle);
            out.rect = [fixed.x + center.x - delta.x / 2., fixed.y + center.y - delta.y / 2., delta.x, delta.y];
        }
        H::Rotation => {
            out.angle = (position.y - c.y).atan2(position.x - c.x) + FRAC_PI_2;
        }
        _ => match (&mut out.parameters, kind) {
            (Rect { radii, .. }, H::Corner(i)) if i < 4 => {
                let corner = match i {
                    0 => p(x, y),
                    1 => p(x + w, y),
                    2 => p(x + w, y + h),
                    _ => p(x, y + h),
                };
                radii[i] = (q.x - corner.x).abs().min((q.y - corner.y).abs());
            }
            (Polygon { corner, .. } | Star { corner, .. } | DoubleStar { corner, .. } | SquareStar { corner, .. }, H::Corner(_)) => *corner = cr,
            (Star { inner, .. } | DoubleStar { inner, .. } | SquareStar { inner, .. } | Segment { inner, .. } | Cog { inner, .. }, H::Inner) => *inner = r,
            (DoubleStar { secondary, .. }, H::Secondary) => *secondary = r,
            (Donut { hole } | Cog { hole, .. }, H::Hole) => *hole = r,
            (Pie { start, end } | Segment { start, end, .. } | Arc { start, end }, H::Start) => {
                let a = ((v - 0.5) * 2.0).atan2((u - 0.5) * 2.0);
                *start = *end - handle_sweep(a, *end, *end - *start);
            }
            (Pie { start, end } | Segment { start, end, .. } | Arc { start, end }, H::End) => {
                let a = ((v - 0.5) * 2.0).atan2((u - 0.5) * 2.0);
                *end = *start + handle_sweep(*start, a, *end - *start);
            }
            (Triangle { top }, H::Top) => *top = u,
            (Diamond { top }, H::Top) => *top = v,
            (Trapezoid { top_left, .. }, H::TopLeft) => *top_left = u,
            (Trapezoid { top_right, .. }, H::TopRight) => *top_right = u,
            (Heart { depth }, H::Depth) => *depth = v * 2.0,
            (Tear { bend }, H::Bend) => *bend = (u - 0.5) * 2.0,
            (Crescent { offset }, H::Offset) => *offset = (u - 0.5) * 4.0,
            (Polygon { curved, .. }, H::Curvature) => *curved = (u - 0.5) * 2.0,
            (Cog { curve, .. }, H::Curvature) => *curve = (u - 0.5) * 2.0,
            (Line { from, .. } | Arrow { from, .. }, H::From) => *from = [q.x, q.y],
            (Line { to, .. } | Arrow { to, .. }, H::To) => *to = [q.x, q.y],
            (Callout { tail, .. }, H::Tail) => *tail = [q.x, q.y],
            (Callout { radius, .. }, H::Corner(_)) => *radius = cr,
            (Spiral { center, .. }, H::Center) => *center = [q.x, q.y],
            (Spiral { center, radius, .. }, H::Radius) => *radius = (q.x - center[0]).hypot(q.y - center[1]),
            (Arrow { from, to, head_width, head_length, shaft, .. }, H::HeadWidth | H::HeadLength | H::Shaft) => {
                let len = (to[0] - from[0]).hypot(to[1] - from[1]);
                let (ux, uy) = ((to[0] - from[0]) / len, (to[1] - from[1]) / len);
                let along = (q.x - from[0]) * ux + (q.y - from[1]) * uy;
                let width = 2.0 * ((q.y - from[1]) * ux - (q.x - from[0]) * uy).abs();
                match kind {
                    H::HeadLength => *head_length = len - along,
                    H::HeadWidth => *head_width = width,
                    _ => *shaft = width,
                }
            }
            _ => return Err(ShapeError("handle does not belong to this shape")),
        },
    }
    generate(&out)?;
    Ok(out)
}
fn handle_sweep(start: f64, end: f64, original: f64) -> f64 {
    let direction = original.signum();
    let mut sweep = ((end - start) * direction).rem_euclid(TAU);
    if original.abs() >= TAU - 1e-10 && (sweep < 1e-10 || TAU - sweep < 1e-10) {
        sweep = TAU;
    }
    direction * sweep
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_ellipse_callout_has_no_inert_radius_handle() {
        use photocraft_doc::vector_design::drawing::{DesignShape, ShapeParameters};
        for ellipse in [false, true] {
            let shape = DesignShape { rect: [0., 0., 100., 100.], angle: 0., parameters: ShapeParameters::Callout { ellipse, tail: [50., 130.], radius: 10. } };
            let handles = handles(&shape).unwrap();
            assert_eq!(handles.iter().any(|h| h.kind == HandleKind::Corner(0)), !ellipse);
        }
    }
}
