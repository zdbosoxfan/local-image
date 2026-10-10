//! Variable width geometry adapted from VectorCraft crates/effects/src/stroke/width.rs and
//! profile interpolation/presets from crates/doc/src/appearance.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! Local Image profiles contain absolute pixel distances, not upstream weight factors.
//! Each subpath runs the same profile over its own arc length. Positive left is up for +x
//! in document y-down coordinates. Outlines are normalized through V1 before rasterization.

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, ParamCurveNearest, PathEl, PathSeg, Point, Vec2};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, StrokeItem};
pub use photocraft_doc::vector_design::appearance::{WidthPoint, WidthProfile};
use photocraft_doc::{LineCap, LineJoin, Path, PathOp, ShapeStroke, StrokeAlign, Subpath};
use photocraft_pathops::{
    BoolOp,
    geom::{
        self,
        bez::{kink, segments, subpaths, tangent, unit},
    },
    kernel,
};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

type Result<T> = std::result::Result<T, String>;
pub const MAX_POINTS: usize = 4096;
pub const MAX_WIDTH: f64 = 5000.0;
const MAX_GEOMETRY: usize = 16384;
const TOLERANCE: f64 = 0.01;

pub fn validate_profile(p: &WidthProfile) -> Result<()> {
    if p.points.len() > MAX_POINTS
        || p.points.windows(2).any(|w| w[0].t >= w[1].t)
        || p.points.iter().any(|q| {
            !q.t.is_finite()
                || !(0.0..=1.0).contains(&q.t)
                || !q.left.is_finite()
                || !(0.0..=MAX_WIDTH).contains(&q.left)
                || !q.right.is_finite()
                || !(0.0..=MAX_WIDTH).contains(&q.right)
        })
    {
        return Err("width points must be ordered, finite and bounded".into());
    }
    Ok(())
}
/// Linear interpolation with endpoint extension. Empty profiles retain the legacy half width.
pub fn at(p: &WidthProfile, half: f64, t: f64) -> (f64, f64) {
    let Some(first) = p.points.first() else { return (half, half) };
    if t <= first.t {
        return (first.left, first.right);
    }
    for w in p.points.windows(2) {
        if t <= w[1].t {
            let u = (t - w[0].t) / (w[1].t - w[0].t);
            return (w[0].left + u * (w[1].left - w[0].left), w[0].right + u * (w[1].right - w[0].right));
        }
    }
    p.points.last().map_or((half, half), |q| (q.left, q.right))
}
/// Add or replace a point. Moves collide by replacing the point at the destination (the
/// shared model intentionally forbids duplicate t). Failure leaves the profile unchanged.
pub fn set_point(p: &mut WidthProfile, index: Option<usize>, point: WidthPoint) -> Result<usize> {
    validate_profile(&WidthProfile { points: vec![point] })?;
    let mut next = p.clone();
    if let Some(i) = index {
        if i >= next.points.len() {
            return Err("width point is absent".into());
        }
        next.points.remove(i);
    }
    next.points.retain(|q| (q.t - point.t).abs() > 1e-9);
    next.points.push(point);
    next.points.sort_by(|a, b| a.t.total_cmp(&b.t));
    validate_profile(&next)?;
    let i = next.points.iter().position(|q| q.t == point.t).ok_or("width point is absent")?;
    *p = next;
    Ok(i)
}
pub fn uniform(half: f64) -> WidthProfile {
    WidthProfile { points: vec![WidthPoint { t: 0.0, left: half, right: half }, WidthPoint { t: 1.0, left: half, right: half }] }
}
pub const PRESETS: [(&str, &str); 7] = [
    ("uniform", "Uniform"),
    ("lens", "Lens"),
    ("taperStart", "Taper Start"),
    ("taperEnd", "Taper End"),
    ("pinch", "Pinch"),
    ("teardrop", "Teardrop"),
    ("wave", "Wave"),
];
pub fn preset(id: &str, half: f64) -> Result<WidthProfile> {
    let points: &[(f64, f64)] = match id {
        "uniform" => &[(0.0, 1.0), (1.0, 1.0)],
        "lens" => &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)],
        "taperStart" => &[(0.0, 0.0), (1.0, 1.0)],
        "taperEnd" => &[(0.0, 1.0), (1.0, 0.0)],
        "pinch" => &[(0.0, 1.0), (0.5, 0.25), (1.0, 1.0)],
        "teardrop" => &[(0.0, 0.0), (0.2, 1.0), (1.0, 0.0)],
        "wave" => &[(0.0, 0.3), (0.25, 1.0), (0.5, 0.3), (0.75, 1.0), (1.0, 0.3)],
        _ => return Err("unknown width profile preset".into()),
    };
    let p = WidthProfile { points: points.iter().map(|&(t, w)| WidthPoint { t, left: w * half, right: w * half }).collect() };
    validate_profile(&p)?;
    Ok(p)
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedProfile {
    pub name: String,
    pub profile: WidthProfile,
}
impl SavedProfile {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err("profile name must contain 1 to 128 bytes".into());
        }
        validate_profile(&self.profile)
    }
}

/// Exact kurbo arc-length map shared by canvas handles and hit testing; source segment index
/// is never used as a fraction. Empty/zero-length subpaths keep their original indices.
#[derive(Clone)]
pub struct Metric {
    subs: Vec<Vec<(PathSeg, f64, f64)>>,
    lengths: Vec<f64>,
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub subpath: usize,
    pub t: f64,
    pub point: Point,
    pub normal: Vec2,
    pub distance: f64,
}
impl Metric {
    pub fn new(path: &Path) -> Result<Self> {
        validate_path(path)?;
        let geometry = photocraft_pathops::to_geometry(path);
        let mut subs = Vec::new();
        let mut lengths = Vec::new();
        for sub in &geometry.subpaths {
            let mut acc = 0.0;
            let mut bp = BezPath::new();
            sub.to_bezpath_into(&mut bp);
            let segs = bp
                .segments()
                .filter_map(|seg| {
                    let len = seg.arclen(1e-7);
                    if len <= 1e-12 {
                        return None;
                    }
                    let entry = (seg, acc, len);
                    acc += len;
                    Some(entry)
                })
                .collect();
            subs.push(segs);
            lengths.push(acc);
        }
        Ok(Self { subs, lengths })
    }
    pub fn sample(&self, subpath: usize, t: f64) -> Option<Sample> {
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return None;
        }
        let segs = self.subs.get(subpath)?;
        let distance = t * self.lengths[subpath];
        let (seg, start, len) = segs.iter().find(|(_, s, l)| distance <= s + l).or_else(|| segs.last())?;
        let u = seg.inv_arclen((distance - start).clamp(0.0, *len), 1e-7);
        Some(Sample { subpath, t, point: seg.eval(u), normal: left(tangent(seg, u)), distance: 0.0 })
    }
    pub fn nearest(&self, at: Point, only_subpath: Option<usize>) -> Option<Sample> {
        if !at.x.is_finite() || !at.y.is_finite() {
            return None;
        }
        self.subs
            .iter()
            .enumerate()
            .filter(|(i, _)| only_subpath.is_none_or(|s| s == *i))
            .flat_map(|(i, segs)| {
                segs.iter().map(move |(seg, start, _)| {
                    let nearest = seg.nearest(at, 1e-7);
                    Sample {
                        subpath: i,
                        t: (start + seg.subsegment(0.0..nearest.t).arclen(1e-7)) / self.lengths[i],
                        point: seg.eval(nearest.t),
                        normal: left(tangent(seg, nearest.t)),
                        distance: nearest.distance_sq.sqrt(),
                    }
                })
            })
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }
}
fn validate_path(path: &Path) -> Result<()> {
    photocraft_pathops::validate(path).map_err(|e| e.to_string())?;
    if path.subpaths.len() > 1024
        || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > 4096
        || path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| p.x.abs() > 1e6 || p.y.abs() > 1e6)
    {
        return Err("width path exceeds the coordinate / node budget".into());
    }
    Ok(())
}

/// Package 08 projection interface; no dependency from pc-vector back to V1.
pub struct Projection;
impl photocraft_vector::appearance::StrokeProjection for Projection {
    fn outline(&self, path: &Path, stroke: &StrokeItem) -> Result<Path> {
        outline(path, stroke)
    }
}
fn normalize(raw: &Path) -> Result<Path> {
    let p = kernel::try_boolean(
        &photocraft_pathops::to_geometry(raw),
        geom::FillRule::NonZero,
        &geom::PathData::default(),
        geom::FillRule::NonZero,
        BoolOp::Union,
        1e-8,
    )
    .map_err(|e| e.to_string())?;
    let out = photocraft_pathops::from_geometry(&p);
    if out.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_GEOMETRY {
        return Err("width union exceeds the geometry budget".into());
    }
    Ok(out)
}
/// Filled outline, including native cap/join/dash/alignment semantics. Unsupported brush or
/// arrow combinations are explicit provider errors rather than disappearing decorations.
pub fn outline(path: &Path, stroke: &StrokeItem) -> Result<Path> {
    validate_path(path)?;
    photocraft_vector::appearance::validate(&Appearance { items: vec![AppearanceItem::Stroke(stroke.clone())] })?;
    if stroke.brush.is_some() || stroke.start_arrow.is_some() || stroke.end_arrow.is_some() {
        return Err("width with brushes / arrowheads requires the combined appearance projection provider".into());
    }
    let default;
    let profile = if let Some(p) = &stroke.profile {
        p
    } else {
        default = WidthProfile::default();
        &default
    };
    validate_profile(profile)?;
    let half = f64::from(stroke.base.width) / 2.0;
    let symmetric = profile.points.first().map(|p| p.left).unwrap_or(half);
    if profile.points.iter().all(|p| p.left == symmetric && p.right == symmetric) {
        // Use identical native flattening and polygons, not a second approximation of caps.
        let mut base = stroke.base.clone();
        base.width = (2.0 * symmetric) as f32;
        if base.width <= 0.0 {
            return Ok(Path::default());
        }
        let mut style = photocraft_vector::stroke_style(&base);
        // Dash units are tied to the base stroke weight, not to the varying side distance.
        style.dashes = stroke.base.dashes.iter().map(|d| f64::from(*d) * f64::from(stroke.base.width)).collect();
        style.dash_offset = f64::from(stroke.base.dash_offset) * f64::from(stroke.base.width);
        if path.subpaths.iter().all(|s| s.closed) && stroke.base.align != StrokeAlign::Center {
            style.width *= 2.0;
        }
        let lines = photocraft_vector::flatten_path(path, TOLERANCE);
        if lines.iter().map(|l| l.pts.len()).sum::<usize>() > MAX_GEOMETRY {
            return Err("width flattening exceeds the geometry budget".into());
        }
        dash_budget(&style.dashes, &lines.iter().map(|l| l.pts.windows(2).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum::<f64>()).sum::<f64>())?;
        if stroke.dash_align && style.dashes.iter().any(|d| *d > 0.0) {
            return Err("fitted width dashes require the combined appearance projection provider".into());
        }
        let polys = photocraft_vector::stroke_polygons(&lines, &style, TOLERANCE);
        if polys.iter().map(Vec::len).sum::<usize>() > MAX_GEOMETRY {
            return Err("width outline exceeds the geometry budget".into());
        }
        let raw = Path::new(polys.iter().enumerate().map(|(i, p)| Subpath::polygon(p).with_op(if i == 0 { PathOp::Combine } else { PathOp::Join })).collect());
        return align(normalize(&raw)?, path, &stroke.base);
    }
    let mut p = profile.clone();
    if path.subpaths.iter().all(|s| s.closed) && stroke.base.align != StrokeAlign::Center {
        for q in &mut p.points {
            q.left *= 2.0;
            q.right *= 2.0;
        }
    }
    let ctx = Ctx { profile: &p, cap: stroke.base.cap, join: stroke.base.join, miter_limit: f64::from(stroke.base.miter_limit), tol: TOLERANCE };
    let bp = photocraft_pathops::to_geometry(path).to_bezpath();
    let mut out = BezPath::new();
    let mut input_count = 0;
    for (r, closed) in subpaths(&bp) {
        let (pts, corner) = flatten(&segments(&bp.elements()[r]), closed, TOLERANCE);
        input_count += pts.len();
        if input_count > MAX_GEOMETRY || pts.len().saturating_mul(p.points.len()) > 1_048_576 {
            return Err("width sampling exceeds the geometry budget".into());
        }
        if pts.len() < 2 {
            continue;
        }
        let total = length(&pts, closed);
        let pattern: Vec<_> = stroke.base.dashes.iter().map(|d| f64::from(*d) * f64::from(stroke.base.width)).collect();
        dash_budget(&pattern, &total)?;
        if pattern.iter().any(|d| *d > 0.0) {
            if stroke.dash_align {
                return Err("fitted width dashes require the combined appearance projection provider".into());
            }
            for (a, b) in dash_spans(total, &pattern, f64::from(stroke.base.dash_offset) * f64::from(stroke.base.width)) {
                let (piece, flags) = slice(&pts, &corner, closed, a, b);
                if piece.len() > 1 {
                    ctx.subpath(&mut out, &piece, &flags, false, (a / total, b / total));
                }
                if out.elements().len() > MAX_GEOMETRY {
                    return Err("width dashes exceed the geometry budget".into());
                }
            }
        } else {
            ctx.subpath(&mut out, &pts, &corner, closed, (0.0, 1.0));
        }
        if out.elements().len() > MAX_GEOMETRY {
            return Err("width outline exceeds the geometry budget".into());
        }
    }
    let raw = photocraft_pathops::from_geometry(&geom::PathData::from_bezpath(&out));
    align(normalize(&raw)?, path, &stroke.base)
}
fn align(out: Path, path: &Path, base: &ShapeStroke) -> Result<Path> {
    if base.align == StrokeAlign::Center || !path.subpaths.iter().all(|s| s.closed) {
        return Ok(out);
    }
    let mut mask = path.clone();
    mask.inverted = false;
    let op = if (base.align == StrokeAlign::Inside) != path.inverted { BoolOp::Intersect } else { BoolOp::Difference };
    photocraft_pathops::boolean(&out, &mask, op).map_err(|e| e.to_string())
}
fn dash_budget(pattern: &[f64], length: &f64) -> Result<()> {
    let period: f64 = pattern.iter().sum();
    if period > 0.0 && (length / period * pattern.len() as f64 > 4096.0 || pattern.iter().any(|d| *d > 0.0 && *d < 1e-6)) {
        return Err("width dashes exceed the segment budget".into());
    }
    Ok(())
}
fn dash_spans(total: f64, pattern: &[f64], offset: f64) -> Vec<(f64, f64)> {
    let mut pattern = pattern.to_vec();
    if pattern.len() % 2 == 1 {
        pattern.extend_from_within(..);
    }
    let period: f64 = pattern.iter().sum();
    let mut phase = offset.rem_euclid(period);
    let mut i = 0;
    while phase >= pattern[i] {
        phase -= pattern[i];
        i = (i + 1) % pattern.len();
    }
    let mut remaining = pattern[i] - phase;
    let mut d = 0.0;
    let mut out = Vec::new();
    while d < total {
        let end = (d + remaining).min(total);
        if i % 2 == 0 && end > d {
            out.push((d, end));
        }
        d = end;
        i = (i + 1) % pattern.len();
        remaining = pattern[i];
    }
    out
}
fn slice(pts: &[Point], corners: &[bool], closed: bool, a: f64, b: f64) -> (Vec<Point>, Vec<bool>) {
    let mut out = (Vec::new(), Vec::new());
    let mut d = 0.0;
    for i in 0..if closed { pts.len() } else { pts.len() - 1 } {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        let len = p.distance(q);
        if len > 1e-12 && d + len > a && d < b {
            let start = a.max(d);
            let end = b.min(d + len);
            if out.0.is_empty() {
                out.0.push(p.lerp(q, (start - d) / len));
                out.1.push(false);
            }
            out.0.push(p.lerp(q, (end - d) / len));
            out.1.push(end == d + len && corners[(i + 1) % pts.len()]);
        }
        d += len;
    }
    (out.0, out.1)
}

/// Left normal in y-down space (left of the direction of travel).
fn left(t: Vec2) -> Vec2 {
    Vec2::new(t.y, -t.x)
}

/// The flattened points of one subpath (consecutive duplicates removed; a closed one doesn't
/// repeat its start), each marked when it is a corner of the path.
fn flatten(segs: &[PathSeg], closed: bool, tol: f64) -> (Vec<Point>, Vec<bool>) {
    let Some(s0) = segs.first() else { return (vec![], vec![]) };
    let (mut pts, mut corner) = (vec![s0.start()], vec![false]);
    // The first and the last segment that drew something.
    let (mut first, mut prev): (Option<&PathSeg>, Option<&PathSeg>) = (None, None);
    for seg in segs {
        let at = pts.len() - 1;
        let mut push = |p: Point| {
            if pts.last().is_none_or(|l| l.distance(p) > 1e-9) {
                pts.push(p);
                corner.push(false);
            }
        };
        match seg {
            PathSeg::Line(l) => push(l.p1),
            _ => kurbo::flatten([PathEl::MoveTo(seg.start()), seg.as_path_el()], tol, |el| {
                if let PathEl::LineTo(p) = el {
                    push(p);
                }
            }),
        }
        if pts.len() > at + 1 {
            match prev {
                Some(p) => corner[at] = kink(p, seg),
                None => first = Some(seg),
            }
            prev = Some(seg);
        }
    }
    if closed {
        if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) <= 1e-9 {
            pts.pop();
            corner.pop();
        }
        if let (Some(a), Some(b)) = (prev, first) {
            corner[0] = kink(a, b);
        }
    }
    (pts, corner)
}

/// Insert samples at width points so the piecewise-linear profile is exact along straight
/// runs. `span` maps a dashed piece onto the original subpath's normalized profile.
fn densify(pts: &[Point], corner: &[bool], closed: bool, profile: &WidthProfile, span: (f64, f64)) -> (Vec<Point>, Vec<bool>) {
    let total = length(pts, closed);
    let range = span.1 - span.0;
    if total <= 1e-12 || range <= 1e-12 {
        return (pts.to_vec(), corner.to_vec());
    }
    // Where the profile's points fall along this piece (a span past 1 wraps round a closed path).
    let marks: Vec<f64> = profile.points.iter().flat_map(|p| [p.t, p.t + 1.0]).map(|t| (t - span.0) / range * total).collect();
    split_at(pts, corner, closed, &marks)
}

/// Length of a flattened subpath (see [`flatten`]).
fn length(pts: &[Point], closed: bool) -> f64 {
    let n = pts.len();
    let seg_count = if closed { n } else { n.saturating_sub(1) };
    (0..seg_count).map(|i| pts[i].distance(pts[(i + 1) % n])).sum()
}

/// A flattened subpath (see [`flatten`]) with points added `marks` along it (distances from its
/// start; those on a point or off the subpath are ignored).
fn split_at(pts: &[Point], corner: &[bool], closed: bool, marks: &[f64]) -> (Vec<Point>, Vec<bool>) {
    let n = pts.len();
    let seg_count = if closed { n } else { n.saturating_sub(1) };
    let mut out = (Vec::with_capacity(n + marks.len()), Vec::with_capacity(n + marks.len()));
    let mut acc = 0.0;
    for i in 0..seg_count {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = a.distance(b);
        out.0.push(a);
        out.1.push(corner[i]);
        let mut inner: Vec<f64> = marks.iter().copied().filter(|d| *d > acc + 1e-9 && *d < acc + len - 1e-9).collect();
        inner.sort_by(f64::total_cmp);
        inner.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        for d in inner {
            out.0.push(a.lerp(b, (d - acc) / len));
            out.1.push(false);
        }
        acc += len;
    }
    if !closed && n > 0 {
        out.0.push(pts[n - 1]);
        out.1.push(corner[n - 1]);
    }
    out
}

/// What outlines one variable-width stroke.
struct Ctx<'a> {
    profile: &'a WidthProfile,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f64,
    tol: f64,
}

/// A vertex of a flattened subpath: where it is, the directions in and out, the lengths of the
/// segments either side and whether it is a corner of the path (else a smooth point).
struct Vertex {
    p: Point,
    a: Vec2,
    b: Vec2,
    la: f64,
    lb: f64,
    corner: bool,
}

impl Ctx<'_> {
    fn subpath(&self, out: &mut BezPath, pts: &[Point], corner: &[bool], closed: bool, span: (f64, f64)) {
        let (pts, corner) = densify(pts, corner, closed, self.profile, span);
        let n = pts.len();
        let seg_count = if closed { n } else { n - 1 };
        let seg_dir: Vec<Vec2> = (0..seg_count).map(|i| unit(pts[(i + 1) % n] - pts[i])).collect();
        let mut cum = vec![0.0; n + 1];
        for i in 0..seg_count {
            cum[i + 1] = cum[i] + pts[i].distance(pts[(i + 1) % n]);
        }
        let total = cum[seg_count];
        if total <= 1e-12 {
            return;
        }
        // A closed subpath reaches t=1 immediately before the seam and t=0 just after it.
        // Elsewhere this strictly ordered profile is continuous, so both offsets agree.
        let width_at = |d: f64| {
            let t = span.0 + (span.1 - span.0) * d / total;
            let w = at(self.profile, 0.0, if t > 1.0 { t - 1.0 } else { t });
            let before = if closed && d == 0.0 { at(self.profile, 0.0, span.1) } else { w };
            (before, w)
        };
        let mut lpts = Vec::with_capacity(n);
        let mut rpts = Vec::with_capacity(n);
        for (i, &p) in pts.iter().enumerate() {
            let (before, after) = width_at(cum[i]);
            if !closed && (i == 0 || i == n - 1) {
                let nm = left(seg_dir[i.min(seg_count - 1)]);
                let (wl, wr) = if i == 0 { after } else { before };
                lpts.push(p + nm * wl);
                rpts.push(p - nm * wr);
                continue;
            }
            let ip = if i == 0 { seg_count - 1 } else { i - 1 };
            let v = Vertex { p, a: seg_dir[ip], b: seg_dir[i], la: cum[ip + 1] - cum[ip], lb: cum[i + 1] - cum[i], corner: corner[i] };
            // The seam can step between its endpoint widths; other points emit once.
            for (k, (wl, wr)) in [before, after].into_iter().enumerate() {
                if k == 0 || (wl, wr) != before {
                    self.join(&mut lpts, &v, wl, 1.0);
                    self.join(&mut rpts, &v, wr, -1.0);
                }
            }
        }
        if closed {
            polygon(out, lpts.into_iter());
            polygon(out, rpts.into_iter().rev());
            return;
        }
        let (t0, t1) = (seg_dir[0], seg_dir[seg_count - 1]);
        let ((wl0, wr0), (wl1, wr1)) = (width_at(0.0).1, width_at(total).0);
        let mut ring = lpts;
        // End cap: from the left side around the end to the right side.
        cap_points(&mut ring, pts[n - 1], left(t1), t1, wl1, wr1, self.cap, self.tol);
        ring.extend(rpts.into_iter().rev());
        // Start cap: from the right side around the start back to the left side.
        cap_points(&mut ring, pts[0], -left(t0), -t0, wr0, wl0, self.cap, self.tol);
        polygon(out, ring.into_iter());
    }

    /// The offset points at `v` on one side (`side` 1: left, −1: right), `w` from the path: the
    /// join on the outer side of a turn, the inner miter point (or a detour through the vertex
    /// when that would overshoot a segment) on the inner side.
    fn join(&self, pts: &mut Vec<Point>, v: &Vertex, w: f64, side: f64) {
        let (na, nb) = (left(v.a) * side, left(v.b) * side);
        let mid = na + nb;
        // Cosine of half the turn; the miter point is `w / c` out along `mid`.
        let c = mid.hypot() / 2.0;
        let miter = |pts: &mut Vec<Point>| pts.push(v.p + mid * (w / (2.0 * c * c)));
        if w <= 0.0 {
            pts.push(v.p);
        } else if v.b.dot(na) <= 0.0 {
            // Outer side: the path turns away from it.
            let excess = if c > 1e-9 { w * (1.0 / c - 1.0) } else { f64::INFINITY };
            match self.join {
                _ if excess <= self.tol => miter(pts),
                LineJoin::Miter if v.corner && 1.0 / c <= self.miter_limit => miter(pts),
                LineJoin::Miter | LineJoin::Bevel if v.corner => pts.extend([v.p + na * w, v.p + nb * w]),
                _ => self.arc(pts, v, na, nb, w),
            }
        } else if c > 1e-9 && w * (1.0 - c * c).sqrt() / c <= v.la.min(v.lb) {
            miter(pts);
        } else {
            pts.extend([v.p + na * w, v.p, v.p + nb * w]);
        }
    }

    /// A round join from `na` to `nb` (unit normals) at radius `w` around `v`; a full reversal
    /// rounds through the direction of travel.
    fn arc(&self, pts: &mut Vec<Point>, v: &Vertex, na: Vec2, nb: Vec2, w: f64) {
        let phi = if (na + nb).hypot() < 1e-9 { if Vec2::new(-na.y, na.x).dot(v.a) > 0.0 { PI } else { -PI } } else { na.cross(nb).atan2(na.dot(nb)) };
        let step = 2.0 * (1.0 - (self.tol / w).min(1.0)).acos();
        let steps = ((phi.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 256);
        for k in 0..=steps {
            let (s, c) = (phi * k as f64 / steps as f64).sin_cos();
            pts.push(v.p + Vec2::new(na.x * c - na.y * s, na.x * s + na.y * c) * w);
        }
    }
}

/// Cap points between `c + u·r0` and `c − u·r1`, bulging towards `v`.
fn cap_points(ring: &mut Vec<Point>, c: Point, u: Vec2, v: Vec2, r0: f64, r1: f64, cap: LineCap, tol: f64) {
    match cap {
        LineCap::Butt => {}
        LineCap::Square => {
            let h = (r0 + r1) / 2.0;
            ring.push(c + u * r0 + v * h);
            ring.push(c - u * r1 + v * h);
        }
        LineCap::Round => {
            let radius = r0.max(r1);
            let step = 2.0 * (1.0 - (tol / radius.max(tol)).min(1.0)).acos();
            let steps = (PI / step.max(1e-3)).ceil().clamp(16.0, 512.0) as usize;
            for k in 1..steps {
                let a = std::f64::consts::PI * k as f64 / steps as f64;
                let r = r0 + (r1 - r0) * (k as f64 / steps as f64);
                ring.push(c + u * (a.cos() * r) + v * (a.sin() * r));
            }
        }
    }
}

fn polygon(out: &mut BezPath, mut pts: impl Iterator<Item = Point>) {
    let Some(first) = pts.next() else { return };
    out.move_to(first);
    for p in pts {
        out.line_to(p);
    }
    out.close_path();
}

#[cfg(test)]
#[path = "width/tests.rs"]
mod tests;
