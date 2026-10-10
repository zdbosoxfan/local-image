//! Native polygon projection. Arrow shapes and fitted-dash semantics adapted from
//! VectorCraft crates/effects/src/stroke/{arrow,dash,gradient}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Uses Local Image's existing polygon stroker; no second geometry dependency.
use crate::{DEFAULT_TOLERANCE, Polyline, Rasterizer, flatten_path, stroke_polygons, stroke_style};
use photocraft_doc::vector_design::appearance::{Arrowhead, StrokeItem, WidthProfile};
use photocraft_doc::{FillRule, LineCap, LineJoin, Path, PathOp, StrokeAlign, Subpath};
type P = (f64, f64);

pub(super) fn validate_path(p: &Path) -> Result<usize, String> {
    let n: usize = p.subpaths.iter().map(|s| s.knots.len()).sum();
    if n > 16384
        || p.subpaths.len() > 16384
        || p.subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
            .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > super::MAX_COORDINATE || p.y.abs() > super::MAX_COORDINATE)
    {
        return Err("appearance path exceeds finite coordinate / node limits".into());
    }
    // Check the existing flattener's subdivision bound before allocating even one
    // subpath: many long cubic handles in one subpath could otherwise allocate first.
    let mut count = p.subpaths.len();
    for sub in &p.subpaths {
        for [a, b, c, d] in sub.segments() {
            let (dx, dy) = (d.x - a.x, d.y - a.y);
            let len2 = dx * dx + dy * dy;
            let on_chord = |p: photocraft_geom::Point| {
                let (x, y) = (p.x - a.x, p.y - a.y);
                if len2 < 1e-24 {
                    return x.abs() < 1e-9 && y.abs() < 1e-9;
                }
                let t = (x * dx + y * dy) / len2;
                (x * dy - y * dx).abs() / len2.sqrt() < 1e-9 && (-1e-9..=1. + 1e-9).contains(&t)
            };
            let pieces = if on_chord(b) && on_chord(c) {
                1
            } else {
                let m = (a.x - 2. * b.x + c.x).hypot(a.y - 2. * b.y + c.y).max((b.x - 2. * c.x + d.x).hypot(b.y - 2. * c.y + d.y));
                let size = (b.x - a.x).hypot(b.y - a.y) + (c.x - b.x).hypot(c.y - b.y) + (d.x - c.x).hypot(d.y - c.y);
                let tol = DEFAULT_TOLERANCE.min(size * 1e-3).max(1e-4);
                ((0.75 * m / tol).sqrt().ceil() as usize).clamp(1, 1 << 16)
            };
            count += pieces;
            if count > 262144 {
                return Err("appearance flattening exceeds point budget".into());
            }
        }
    }
    Ok(count)
}

#[derive(Clone)]
struct Segment {
    a: P,
    b: P,
    start: f64,
    len: f64,
    total: f64,
    side: Option<bool>,
}
pub(super) struct Metric {
    segments: Vec<Segment>,
    width: f64,
    profile: Option<WidthProfile>,
}
impl Metric {
    pub fn new(path: &Path, stroke: &StrokeItem) -> Result<Self, String> {
        let mut segments = Vec::new();
        let lines = flatten_path(path, DEFAULT_TOLERANCE);
        let aligned = path.subpaths.iter().all(|p| p.closed) && stroke.base.align != StrokeAlign::Center;
        if aligned && lines.iter().map(|l| l.pts.len()).sum::<usize>().saturating_mul(lines.len()) > 2_097_152 {
            return Err("aligned stroke gradient exceeds the probe budget".into());
        }
        for line in &lines {
            // Probe the full boolean fill, rather than infer the visible side from winding:
            // joined holes, subtractions and reversed paths can all change which side paints.
            let side = if aligned {
                line.pts
                    .iter()
                    .copied()
                    .zip(line.pts.iter().copied().cycle().skip(1))
                    .take(line.pts.len())
                    .max_by(|(a, b), (c, d)| (b.0 - a.0).hypot(b.1 - a.1).total_cmp(&(d.0 - c.0).hypot(d.1 - c.1)))
                    .map(|(a, b)| {
                        let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
                        let off = (4. * DEFAULT_TOLERANCE).min(f64::from(stroke.base.width) / 2.).max(1e-6);
                        let p = ((a.0 + b.0) / 2. - (b.1 - a.1) / len * off, (a.1 + b.1) / 2. + (b.0 - a.0) / len * off);
                        contains(path, &lines, p) == (stroke.base.align == StrokeAlign::Inside)
                    })
            } else {
                None
            };
            let first = segments.len();
            let mut total = 0.;
            let n = line.pts.len();
            let end = if line.closed { n } else { n.saturating_sub(1) };
            for i in 0..end {
                let (a, b) = (line.pts[i], line.pts[(i + 1) % n]);
                let len = (b.0 - a.0).hypot(b.1 - a.1);
                if len > 1e-9 {
                    segments.push(Segment { a, b, start: total, len, total: 0., side });
                    total += len;
                }
            }
            for s in &mut segments[first..] {
                s.total = total;
            }
        }
        if segments.len() > 262144 {
            return Err("stroke gradient exceeds segment budget".into());
        }
        Ok(Self { segments, width: f64::from(stroke.base.width), profile: stroke.profile.clone() })
    }
    pub fn sample(&self, x: f64, y: f64) -> (f64, f64) {
        let mut best = f64::INFINITY;
        let mut out = (0., 0.);
        for s in &self.segments {
            let (dx, dy) = (s.b.0 - s.a.0, s.b.1 - s.a.1);
            let t = ((x - s.a.0) * dx + (y - s.a.1) * dy) / (s.len * s.len);
            let t = t.clamp(0., 1.);
            let (vx, vy) = (x - s.a.0 - t * dx, y - s.a.1 - t * dy);
            let d = vx * vx + vy * vy;
            if d < best {
                best = d;
                let along = (s.start + t * s.len) / s.total.max(1e-9);
                let widths = self.profile.as_ref().and_then(|p| profile_at(p, along));
                let (mut left, mut right) = widths.unwrap_or((self.width / 2., self.width / 2.));
                if let Some(on_left) = s.side {
                    if widths.is_none() {
                        left *= 2.;
                        right *= 2.;
                    }
                    if on_left {
                        right = 0.;
                    } else {
                        left = 0.;
                    }
                }
                let across = ((vx * dy - vy * dx) / s.len + left) / (left + right).max(1e-9);
                out = (along, across);
            }
        }
        out
    }
}

/// Linear interpolation of absolute document-space profile widths from the native contract.
fn profile_at(p: &WidthProfile, t: f64) -> Option<(f64, f64)> {
    let first = p.points.first()?;
    let i = p.points.partition_point(|p| p.t < t);
    if i == 0 {
        return Some((first.left, first.right));
    }
    let a = p.points[i - 1];
    let Some(b) = p.points.get(i) else {
        return Some((a.left, a.right));
    };
    let k = (t - a.t) / (b.t - a.t);
    Some((a.left + (b.left - a.left) * k, a.right + (b.right - a.right) * k))
}
fn contains(path: &Path, lines: &[Polyline], p: P) -> bool {
    let mut inside = false;
    for c in path.components() {
        let mut winding = 0;
        for line in &lines[c.clone()] {
            for (a, b) in line.pts.iter().zip(line.pts.iter().cycle().skip(1)).take(line.pts.len()) {
                if (a.1 <= p.1 && b.1 > p.1) || (b.1 <= p.1 && a.1 > p.1) {
                    let x = a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1);
                    if x > p.0 {
                        winding += if b.1 > a.1 { 1 } else { -1 };
                    }
                }
            }
        }
        let v = match path.fill_rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        };
        inside = match path.effective_op(c.start) {
            PathOp::Combine | PathOp::Join => inside || v,
            PathOp::Subtract => inside && !v,
            PathOp::Intersect => inside && v,
            PathOp::Exclude => inside != v,
        };
    }
    inside != path.inverted
}

fn positive(mut p: Vec<P>) -> Vec<P> {
    let area: f64 = p.iter().zip(p.iter().cycle().skip(1)).take(p.len()).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
    if area < 0. {
        p.reverse();
    }
    p
}
fn heads(kind: Arrowhead, at: P, dir: P, w: f64) -> Vec<Vec<P>> {
    let l = 4. * w;
    let half = l / 2.;
    let map = |p: P| (at.0 + dir.0 * p.0 - dir.1 * p.1, at.1 + dir.1 * p.0 + dir.0 * p.1);
    let poly = match kind {
        Arrowhead::Triangle | Arrowhead::TriangleOpen => vec![(0., 0.), (-l, half), (-l, -half)],
        Arrowhead::Square | Arrowhead::SquareOpen => vec![(0., half), (-l, half), (-l, -half), (0., -half)],
        Arrowhead::Diamond => vec![(0., 0.), (-half, half), (-l, 0.), (-half, -half)],
        Arrowhead::Circle | Arrowhead::CircleOpen => {
            let steps = (std::f64::consts::PI / (1. - DEFAULT_TOLERANCE / half).clamp(-1., 1.).acos()).ceil().max(16.) as usize;
            (0..steps)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / steps as f64;
                    (-half + half * a.cos(), half * a.sin())
                })
                .collect()
        }
        Arrowhead::Bar => vec![(0., half), (-0.75 * w, half), (-0.75 * w, -half), (0., -half)],
        Arrowhead::Arrow => vec![(0., 0.), (-l, half), (-0.7 * l, 0.), (-l, -half)],
        Arrowhead::Barbed => {
            vec![(0., 0.), (-3.6 * w, 2. * w), (-2.6 * w, 0.55 * w), (-4. * w, 0.55 * w), (-4. * w, -0.55 * w), (-2.6 * w, -0.55 * w), (-3.6 * w, -2. * w)]
        }
        Arrowhead::ArrowOpen => {
            let len = l.hypot(half);
            let nx = half / len * w;
            let ny = l / len * w;
            vec![(0., 0.), (-l, half), (-l - nx, half - ny), (-len / half * w, 0.), (-l - nx, -half + ny), (-l, -half)]
        }
    };
    let mut out = vec![positive(poly.iter().copied().map(map).collect())];
    if matches!(kind, Arrowhead::TriangleOpen | Arrowhead::SquareOpen | Arrowhead::CircleOpen) {
        // Inset half a weight wall: concentric regular polygons, reversed winding for the hole.
        let (center, radius) = if kind == Arrowhead::TriangleOpen {
            let radius = l * half / (l.hypot(half) + half);
            (-l + radius, radius)
        } else {
            (-half, half)
        };
        let factor = (radius - 0.5 * w).max(0.) / radius;
        let mut inner = positive(poly.iter().map(|p| map((center + (p.0 - center) * factor, p.1 * factor))).collect());
        inner.reverse();
        out.push(inner);
    }
    out
}

// Fitting must inspect the actual Bezier tangents at source knots. Adjacent flattening
// chords on a smooth curve are angled; treating that angle as a corner would restart dashes.
fn fitted_line(source: &Polyline, sub: &Subpath) -> Polyline {
    let mut line = source.clone();
    let n = sub.knots.len();
    let mut corners = std::collections::HashSet::new();
    let key = |p: P| (if p.0 == 0. { 0 } else { p.0.to_bits() }, if p.1 == 0. { 0 } else { p.1.to_bits() });
    for i in 0..n {
        if !sub.closed && (i == 0 || i + 1 == n) {
            continue;
        }
        let p = sub.knots[i].anchor;
        let prev = sub.knots[(i + n - 1) % n];
        let next = sub.knots[(i + 1) % n];
        let tangent = |pts: &[photocraft_geom::Point], incoming: bool| {
            pts.iter().map(|q| if incoming { (p.x - q.x, p.y - q.y) } else { (q.x - p.x, q.y - p.y) }).find(|d| d.0.hypot(d.1) > 1e-9).unwrap_or((0., 0.))
        };
        let a = tangent(&[sub.knots[i].in_ctrl, prev.out_ctrl, prev.anchor], true);
        let b = tangent(&[sub.knots[i].out_ctrl, next.in_ctrl, next.anchor], false);
        if (a.0 * b.1 - a.1 * b.0).abs() > 1e-6 * a.0.hypot(a.1) * b.0.hypot(b.1) || a.0 * b.0 + a.1 * b.1 < 0. {
            corners.insert(key((p.x, p.y)));
        }
    }
    for (p, k) in line.pts.iter().zip(&mut line.knot) {
        *k = corners.contains(&key(*p));
    }
    if line.closed
        && let Some(i) = line.knot.iter().position(|k| *k)
    {
        line.pts.rotate_left(i);
        line.knot.rotate_left(i);
    }
    line
}

pub(super) fn stroke(path: &Path, s: &StrokeItem) -> Result<(Rasterizer, usize), String> {
    let mut style = stroke_style(&s.base);
    if path.subpaths.iter().all(|p| p.closed) && s.base.align != StrokeAlign::Center {
        style.width *= 2.;
    }
    let lines = flatten_path(path, DEFAULT_TOLERANCE);
    let period: f64 = style.dashes.iter().sum();
    let length: f64 = lines
        .iter()
        .map(|l| {
            let n = l.pts.len();
            let end = if l.closed { n } else { n.saturating_sub(1) };
            (0..end)
                .map(|i| {
                    let (a, b) = (l.pts[i], l.pts[(i + 1) % n]);
                    (b.0 - a.0).hypot(b.1 - a.1)
                })
                .sum::<f64>()
        })
        .sum();
    let knots = lines.iter().map(|l| l.knot.iter().filter(|k| **k).count()).sum::<usize>();
    let pieces = if period > 0. {
        if length / period * style.dashes.len() as f64 > 100000. || style.dashes.iter().any(|d| *d > 0. && *d < 1e-6) {
            return Err("appearance dash geometry exceeds the segment budget".into());
        }
        // Fitting short runs can produce one period per corner, not just length / period.
        (length / period + knots as f64 + lines.len() as f64).ceil() * style.dashes.len() as f64
    } else {
        lines.len() as f64
    };
    let radius = style.width / 2.;
    let step = if radius <= DEFAULT_TOLERANCE {
        std::f64::consts::FRAC_PI_2
    } else {
        (2. * (1. - DEFAULT_TOLERANCE / radius).clamp(-1., 1.).acos()).clamp(0.01, std::f64::consts::FRAC_PI_2)
    };
    let circle = (std::f64::consts::TAU / step).ceil().max(8.);
    let points = lines.iter().map(|l| l.pts.len()).sum::<usize>();
    let estimate = points as f64 * 16.
        + knots as f64 * if style.join == LineJoin::Round { circle } else { 8. }
        + pieces * if style.cap == LineCap::Round { 2. * circle } else { 8. };
    if estimate > 262144. {
        return Err("appearance stroke tessellation exceeds the point budget".into());
    }
    let mut polys = Vec::new();
    for (source, sub) in lines.iter().zip(&path.subpaths) {
        let fitted;
        let line = if s.dash_align {
            fitted = fitted_line(source, sub);
            &fitted
        } else {
            source
        };
        if s.dash_align && !style.dashes.is_empty() && style.dashes.iter().any(|v| *v > 0.) {
            let mut pattern = style.dashes.clone();
            if pattern.len() % 2 == 1 {
                pattern.extend_from_within(..);
            }
            let period: f64 = pattern.iter().sum();
            // Fitted runs restart at corners. Curved spans remain one run; sampled curve points
            // are not corners. Dashes are centred on every corner and open endpoint.
            let n = line.pts.len();
            let end = if line.closed { n } else { n.saturating_sub(1) };
            let mut run = Vec::new();
            let mut run_knots = Vec::new();
            for i in 0..end {
                if run.is_empty() {
                    run.push(line.pts[i]);
                    run_knots.push(line.knot[i]);
                }
                run.push(line.pts[(i + 1) % n]);
                run_knots.push(line.knot[(i + 1) % n]);
                let j = (i + 1) % n;
                let corner = j == 0 || i + 1 == end || line.knot[j];
                if corner {
                    let len: f64 = run.windows(2).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum();
                    let k = len / (len / period).round().max(1.) / period;
                    let mut fitted = style.clone();
                    fitted.dashes = pattern.iter().map(|v| v * k).collect();
                    fitted.dash_offset = fitted.dashes[0] / 2.;
                    let p = Polyline { knot: std::mem::take(&mut run_knots), pts: std::mem::take(&mut run), closed: false };
                    polys.extend(stroke_polygons(&[p], &fitted, DEFAULT_TOLERANCE));
                    // Supply the native join at the dash-centred corner (caps alone miss miters).
                    if (line.closed || j > 0) && (line.closed || j + 1 < n) {
                        let b = line.pts[j];
                        let a = line.pts[i];
                        let c = line.pts[(j + 1) % n];
                        let d = (pattern[0] * k / 4.).min((b.0 - a.0).hypot(b.1 - a.1) / 2.).min((c.0 - b.0).hypot(c.1 - b.1) / 2.);
                        let short = |p: P| {
                            let len = (p.0 - b.0).hypot(p.1 - b.1).max(1e-9);
                            (b.0 + (p.0 - b.0) * d / len, b.1 + (p.1 - b.1) * d / len)
                        };
                        let join = Polyline { pts: vec![short(a), b, short(c)], knot: vec![true; 3], closed: false };
                        let mut solid = style.clone();
                        solid.dashes.clear();
                        solid.cap = LineCap::Butt;
                        polys.extend(stroke_polygons(&[join], &solid, DEFAULT_TOLERANCE));
                    }
                }
            }
        } else {
            polys.extend(stroke_polygons(std::slice::from_ref(line), &style, DEFAULT_TOLERANCE));
        }
    }
    let mut count: usize = polys.iter().map(Vec::len).sum();
    if count > 262144 {
        return Err("appearance stroke exceeds the point budget".into());
    }
    let mut r = Rasterizer::new(false);
    r.add_component(&polys, PathOp::Combine, FillRule::NonZero);
    // Arrowheads apply to the first/last open subpath. They and the line share one coverage,
    // so overlapping translucent heads take opacity only once.
    for (line, head, start, k) in [(lines.first(), s.start_arrow, true, s.arrow_scale[0]), (lines.last(), s.end_arrow, false, s.arrow_scale[1])] {
        if let (Some(line), Some(kind)) = (line, head)
            && !line.closed
            && line.pts.len() > 1
        {
            let (at, inside) = if start { (line.pts[0], line.pts[1]) } else { (line.pts[line.pts.len() - 1], line.pts[line.pts.len() - 2]) };
            let d = (at.0 - inside.0, at.1 - inside.1);
            let len = d.0.hypot(d.1);
            if len > 1e-9 {
                let dir = (d.0 / len, d.1 / len);
                let w = f64::from(s.base.width) * k;
                // Extend solid heads by half their length; hollow heads extend from their back wall,
                // keeping the line out of the hole (also accounts for projecting/round caps).
                let hollow = matches!(kind, Arrowhead::TriangleOpen | Arrowhead::SquareOpen | Arrowhead::CircleOpen);
                let cap = if s.base.cap == LineCap::Butt { 0. } else { style.width / 2. };
                let inset = if hollow {
                    (3.75 * w).max(3.5 * w + cap)
                } else {
                    match kind {
                        Arrowhead::Bar => (0.375 * w).max(cap),
                        Arrowhead::Arrow => (1.4 * w).max(cap),
                        Arrowhead::ArrowOpen => (5.0_f64.sqrt() * w).max(cap),
                        _ => (2. * w).max(cap),
                    }
                };
                let tip = (at.0 + dir.0 * inset, at.1 + dir.1 * inset);
                let head = heads(kind, tip, dir, w);
                count += head.iter().map(Vec::len).sum::<usize>();
                if count > 262144 {
                    return Err("appearance arrows exceed the point budget".into());
                }
                r.add_component(&head, PathOp::Combine, FillRule::NonZero);
            }
        }
    }
    Ok((r, count))
}
