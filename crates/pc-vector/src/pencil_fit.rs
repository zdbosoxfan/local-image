//! Freehand fitting and interval replacement for Vector Pencil (never raster Pencil).
//! Adapted from VectorCraft crates/pathops/src/fit.rs and tools/src/draw2.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! The native sampled-point fitter works without a pc-pathops dependency (which would
//! create a cycle). `fit_with` accepts V1's fit_path adapter for the integrated engine.
//! Tolerance measures deviation from the stabilized polyline, in document pixels.

use photocraft_doc::{Knot, Path, Subpath};
use photocraft_geom::Point;
use std::collections::VecDeque;

pub const MAX_SAMPLES: usize = 8192;
pub const MAX_COORD: f64 = 1_000_000.0;
pub type FitResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub point: Point,
    pub pressure: f64,
}
impl Sample {
    pub fn validate(self) -> FitResult<()> {
        if valid_point(self.point) && self.pressure.is_finite() && (0.0..=1.0).contains(&self.pressure) {
            Ok(())
        } else {
            Err("pencil samples must have bounded finite coordinates and pressure 0..1".into())
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Stabilization {
    #[default]
    None,
    Rope {
        length: f64,
    },
    Window {
        samples: usize,
    },
}
impl Stabilization {
    pub fn validate(self) -> FitResult<()> {
        match self {
            Self::None => Ok(()),
            Self::Rope { length } if length.is_finite() && (0.0..=1000.0).contains(&length) => Ok(()),
            Self::Window { samples } if (1..=64).contains(&samples) => Ok(()),
            _ => Err("invalid pencil stabilizer".into()),
        }
    }
}

/// Per-gesture filter. Release appends the real pointer endpoint; cancel drops all state.
#[derive(Clone, Debug)]
pub struct Stabilizer {
    mode: Stabilization,
    tail: Option<Sample>,
    window: VecDeque<Sample>,
}
impl Stabilizer {
    pub fn new(mode: Stabilization) -> FitResult<Self> {
        mode.validate()?;
        Ok(Self { mode, tail: None, window: VecDeque::new() })
    }
    pub fn push(&mut self, sample: Sample) -> FitResult<Sample> {
        sample.validate()?;
        let filtered = match (self.mode, self.tail) {
            (Stabilization::Rope { length }, Some(tail)) => {
                let d = distance(tail.point, sample.point);
                Sample { point: if d > length { mix(tail.point, sample.point, (d - length) / d) } else { tail.point }, ..sample }
            }
            (Stabilization::Window { samples }, _) => {
                self.window.push_back(sample);
                while self.window.len() > samples {
                    self.window.pop_front();
                }
                let count = self.window.len() as f64;
                let point = self.window.iter().fold(Point::default(), |sum, s| add(sum, s.point));
                Sample { point: mul(point, 1.0 / count), pressure: self.window.iter().map(|s| s.pressure).sum::<f64>() / count }
            }
            _ => sample,
        };
        self.tail = Some(filtered);
        Ok(filtered)
    }
    pub fn finish(&mut self, endpoint: Sample) -> FitResult<Sample> {
        endpoint.validate()?;
        self.cancel();
        Ok(endpoint)
    }
    pub fn cancel(&mut self) {
        self.tail = None;
        self.window.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitOptions {
    pub tolerance: f64,
    pub smoothness: f64,
    pub closed: bool,
}
impl Default for FitOptions {
    fn default() -> Self {
        Self { tolerance: 1.5, smoothness: 0.5, closed: false }
    }
}
impl FitOptions {
    pub fn validate(self) -> FitResult<()> {
        if self.tolerance.is_finite() && (0.05..=100.0).contains(&self.tolerance) && self.smoothness.is_finite() && (0.0..=1.0).contains(&self.smoothness) {
            Ok(())
        } else {
            Err("pencil tolerance must be 0.05..100 and smoothness 0..1".into())
        }
    }
}

pub fn valid_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD
}
pub fn validate_path(path: &Path) -> FitResult<()> {
    if path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_SAMPLES
        || path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| !valid_point(p))
    {
        return Err("pencil path exceeds its coordinate or node budget".into());
    }
    Ok(())
}

/// Default native fitter, with bounded smoothing and sharp-turn preservation.
pub fn fit(samples: &[Sample], options: FitOptions) -> FitResult<Path> {
    fit_with(samples, options, native_fit)
}

/// V1 integration: pass `|p,t| photocraft_pathops::fit_path(p,t).map_err(|e|e.to_string())`.
/// The backend receives corner-separated open runs; closure is assembled without duplicate
/// seam anchors. Errors propagate; they never turn into successful empty paths.
pub fn fit_with(samples: &[Sample], options: FitOptions, fitter: impl Fn(&[Point], f64) -> FitResult<Path>) -> FitResult<Path> {
    options.validate()?;
    if !(2..=MAX_SAMPLES).contains(&samples.len()) {
        return Err("pencil needs 2..8192 samples".into());
    }
    for s in samples {
        s.validate()?;
    }
    let mut points: Vec<Point> = samples.iter().map(|s| s.point).collect();
    points.dedup_by(|a, b| distance(*a, *b) < 1e-10);
    if options.closed && points.len() > 2 && distance(points[0], points[points.len() - 1]) < 1e-10 {
        points.pop();
    }
    if points.len() < if options.closed { 3 } else { 2 } {
        return Err("pencil stroke is too short".into());
    }
    let original = points.clone();
    // Preserve corners. Clamp movement to a quarter of the tolerance, rather than allowing
    // the smoothing slider to silently increase the declared geometric error.
    for i in 0..points.len() {
        if !options.closed && (i == 0 || i + 1 == points.len()) {
            continue;
        }
        let prev = original[(i + original.len() - 1) % original.len()];
        let next = original[(i + 1) % original.len()];
        if corner(prev, original[i], next) {
            continue;
        }
        let delta = sub(mix(prev, next, 0.5), original[i]);
        let len = distance(delta, Point::default());
        if len > 1e-12 {
            points[i] = add(original[i], mul(delta, (options.tolerance * 0.25 / len).min(0.5) * options.smoothness));
        }
    }
    let mut cuts: Vec<usize> = (1..points.len() - 1).filter(|&i| corner(points[i - 1], points[i], points[i + 1])).collect();
    if options.closed {
        // Four regular spans also make a smooth closed loop fit compactly at its seam.
        cuts.extend([points.len() / 4, points.len() / 2, points.len() * 3 / 4]);
        points.push(points[0]);
    }
    cuts.extend([0, points.len() - 1]);
    cuts.sort_unstable();
    cuts.dedup();
    let mut knots = Vec::new();
    for cut in cuts.windows(2) {
        let run = &points[cut[0]..=cut[1]];
        let fitted = fitter(run, options.tolerance * 0.5)?;
        validate_path(&fitted)?;
        let Some(sp) = fitted.subpaths.first().filter(|s| fitted.subpaths.len() == 1 && !s.closed && s.knots.len() >= 2) else {
            return Err("pencil fitter must return one nonempty open subpath".into());
        };
        if sp.knots[0].anchor != run[0] || sp.knots[sp.knots.len() - 1].anchor != run[run.len() - 1] {
            return Err("pencil fitter must retain run endpoints".into());
        }
        append_knots(&mut knots, &sp.knots);
    }
    if options.closed
        && let Some(end) = knots.pop()
        && let Some(first) = knots.first_mut()
    {
        first.in_ctrl = end.in_ctrl;
    }
    // Continuity is inferred from the final handles, including sharp backend split points.
    for k in &mut knots {
        k.smooth = smooth_handles(*k);
    }
    let result = Path::new(vec![Subpath { closed: options.closed, knots, ..Default::default() }]);
    validate_path(&result)?;
    Ok(result)
}

fn append_knots(knots: &mut Vec<Knot>, next: &[Knot]) {
    if let (Some(last), Some(first)) = (knots.last_mut(), next.first()) {
        last.out_ctrl = first.out_ctrl;
        knots.extend_from_slice(&next[1..]);
    } else {
        knots.extend_from_slice(next);
    }
}

/// Extend either endpoint. Existing anchors and their outside-facing handles survive.
pub fn continue_path(path: &Path, subpath: usize, at_start: bool, extension: &Path, close: bool) -> FitResult<Path> {
    validate_path(path)?;
    validate_path(extension)?;
    let mut out = path.clone();
    let sp = out.subpaths.get_mut(subpath).ok_or("pencil subpath is missing")?;
    if sp.closed || sp.knots.len() < 2 {
        return Err("pencil continuation requires an open path".into());
    }
    let add = extension
        .subpaths
        .first()
        .filter(|s| extension.subpaths.len() == 1 && !s.closed && s.knots.len() >= 2)
        .ok_or("pencil continuation needs one open extension")?;
    if at_start {
        reverse(sp);
    }
    if sp.knots[sp.knots.len() - 1].anchor != add.knots[0].anchor {
        return Err("pencil continuation does not meet the captured endpoint".into());
    }
    append_knots(&mut sp.knots, &add.knots);
    if close {
        let duplicate = sp.knots[sp.knots.len() - 1].anchor == sp.knots[0].anchor;
        if duplicate && let Some(last) = sp.knots.pop() {
            sp.knots[0].in_ctrl = last.in_ctrl;
        }
        sp.closed = true;
    }
    if at_start {
        reverse(sp);
    }
    validate_path(&out)?;
    Ok(out)
}

/// Replace the forward interval between two existing anchors. A closed interval may wrap
/// across the seam. All anchors/handles outside it and the path's component semantics stay.
pub fn sculpt(path: &Path, subpath: usize, first: usize, last: usize, replacement: &Path) -> FitResult<Path> {
    validate_path(path)?;
    validate_path(replacement)?;
    let mut out = path.clone();
    let sp = out.subpaths.get_mut(subpath).ok_or("sculpt subpath is missing")?;
    let n = sp.knots.len();
    if first >= n || last >= n || first == last || (!sp.closed && first > last) {
        return Err("invalid sculpt interval".into());
    }
    let fit = replacement
        .subpaths
        .first()
        .filter(|s| replacement.subpaths.len() == 1 && !s.closed && s.knots.len() >= 2)
        .ok_or("sculpt requires one open replacement")?;
    if fit.knots[0].anchor != sp.knots[first].anchor || fit.knots[fit.knots.len() - 1].anchor != sp.knots[last].anchor {
        return Err("sculpt must retain interval endpoints".into());
    }
    let mut replacement = fit.knots.clone();
    replacement[0].in_ctrl = sp.knots[first].in_ctrl;
    let end = replacement.len() - 1;
    replacement[end].out_ctrl = sp.knots[last].out_ctrl;
    replacement[0].smooth = smooth_handles(replacement[0]);
    replacement[end].smooth = smooth_handles(replacement[end]);
    if first < last {
        sp.knots.splice(first..=last, replacement);
    } else {
        replacement.extend_from_slice(&sp.knots[last + 1..first]);
        sp.knots = replacement;
    }
    validate_path(&out)?;
    Ok(out)
}

/// Insert exact de Casteljau boundaries for Sculpt. Original anchors stay fixed and the
/// untouched portions of boundary cubics retain their geometry, including same-segment edits.
/// Positions are (segment leaving knot, parameter); closed ranges can wrap across the seam.
pub fn sculpt_boundaries(path: &Path, subpath: usize, from: (usize, f64), to: (usize, f64)) -> FitResult<(Path, usize, usize)> {
    validate_path(path)?;
    let sp = path.subpaths.get(subpath).ok_or("sculpt subpath is missing")?;
    let segments = sp.segments();
    for (segment, t) in [from, to] {
        if segment >= segments.len() || !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return Err("invalid sculpt curve position".into());
        }
    }
    if !sp.closed && from.0 as f64 + from.1 > to.0 as f64 + to.1 {
        return Err("open sculpt interval runs backwards".into());
    }
    let mut knots = vec![sp.knots[0]];
    let mut indices = [None, None];
    for (i, c) in segments.iter().enumerate() {
        let mut cuts = vec![0.0, 1.0];
        cuts.extend([from, to].into_iter().filter_map(|(s, t)| (s == i).then_some(t)));
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        for cut in cuts.windows(2) {
            for (which, (s, t)) in [from, to].into_iter().enumerate() {
                if s == i && t == cut[0] {
                    indices[which] = Some(knots.len() - 1);
                }
            }
            let c = subcurve(c, cut[0], cut[1]);
            if let Some(last) = knots.last_mut() {
                last.out_ctrl = c[1];
            }
            let mut knot = if cut[1] == 1.0 { sp.knots[(i + 1) % sp.knots.len()] } else { Knot::smooth(c[3], c[2], c[3]) };
            knot.in_ctrl = c[2];
            knots.push(knot);
            for (which, (s, t)) in [from, to].into_iter().enumerate() {
                if s == i && t == cut[1] {
                    indices[which] = Some(knots.len() - 1);
                }
            }
        }
    }
    if sp.closed {
        if let Some(last) = knots.pop() {
            knots[0].in_ctrl = last.in_ctrl;
        }
        for index in indices.iter_mut().flatten() {
            if *index == knots.len() {
                *index = 0;
            }
        }
    }
    let first = indices[0].ok_or("sculpt start boundary missing")?;
    let last = indices[1].ok_or("sculpt end boundary missing")?;
    if first == last {
        return Err("sculpt interval is empty".into());
    }
    let mut out = path.clone();
    out.subpaths[subpath].knots = knots;
    validate_path(&out)?;
    Ok((out, first, last))
}
fn split_curve(c: &[Point; 4], t: f64) -> ([Point; 4], [Point; 4]) {
    let a = mix(c[0], c[1], t);
    let b = mix(c[1], c[2], t);
    let d = mix(c[2], c[3], t);
    let e = mix(a, b, t);
    let f = mix(b, d, t);
    let m = mix(e, f, t);
    ([c[0], a, e, m], [m, f, d, c[3]])
}
fn subcurve(c: &[Point; 4], from: f64, to: f64) -> [Point; 4] {
    let left = split_curve(c, to).0;
    if from == 0.0 { left } else { split_curve(&left, from / to).1 }
}

fn reverse(sp: &mut Subpath) {
    sp.knots.reverse();
    for k in &mut sp.knots {
        std::mem::swap(&mut k.in_ctrl, &mut k.out_ctrl);
    }
}
fn smooth_handles(k: Knot) -> bool {
    let a = sub(k.anchor, k.in_ctrl);
    let b = sub(k.out_ctrl, k.anchor);
    let lengths = dot(a, a) * dot(b, b);
    lengths > 1e-18 && dot(a, b) > 0.0 && (a.x * b.y - a.y * b.x).abs() <= lengths.sqrt() * 1e-6
}
fn corner(a: Point, b: Point, c: Point) -> bool {
    let u = unit(sub(b, a));
    let v = unit(sub(c, b));
    dot(u, v) < 0.5
}

// Native adaptation of the pinned VC fixed-tangent LSQ fitter. No geometry dependency
// points back from pc-vector to pc-pathops. V1 is supplied through fit_with in the engine.
fn native_fit(points: &[Point], tolerance: f64) -> FitResult<Path> {
    let mut curves = Vec::new();
    fit_rec(points, unit(sub(points[1], points[0])), unit(sub(points[points.len() - 1], points[points.len() - 2])), tolerance, 0, &mut curves);
    let mut knots = vec![Knot::corner(points[0].x, points[0].y)];
    for c in curves {
        if let Some(last) = knots.last_mut() {
            last.out_ctrl = c[1];
        }
        knots.push(Knot { anchor: c[3], in_ctrl: c[2], out_ctrl: c[3], smooth: false });
    }
    Ok(Path::new(vec![Subpath { knots, ..Default::default() }]))
}
fn fit_rec(p: &[Point], t0: Point, t1: Point, tol: f64, depth: usize, out: &mut Vec<[Point; 4]>) {
    if p.len() == 2 || depth >= 32 {
        // A two-sample interval is exactly a straight segment, never a tangent-induced loop.
        for w in p.windows(2) {
            out.push([w[0], mix(w[0], w[1], 1.0 / 3.0), mix(w[0], w[1], 2.0 / 3.0), w[1]]);
        }
        return;
    }
    let mut u = vec![0.0];
    for w in p.windows(2) {
        u.push(u[u.len() - 1] + distance(w[0], w[1]));
    }
    let length = u[u.len() - 1];
    for t in &mut u {
        *t /= length;
    }
    let mut curve = generate(p, &u, t0, t1);
    let (mut error, mut split) = max_error(p, &u, &curve);
    for _ in 0..6 {
        let next: Vec<_> = p.iter().zip(&u).map(|(&p, &t)| newton(&curve, p, t)).collect();
        if next.windows(2).any(|w| w[0] >= w[1]) {
            break;
        }
        let trial = generate(p, &next, t0, t1);
        let (e, i) = max_error(p, &next, &trial);
        if e >= error {
            break;
        }
        u = next;
        curve = trial;
        error = e;
        split = i;
    }
    // Check the curve back against the polyline as well; sampled point residual alone
    // can accept a large excursion between two sparse samples.
    if error <= tol && curve_near_polyline(&curve, p, tol) {
        out.push(curve);
    } else {
        let split = split.clamp(1, p.len() - 2);
        let tangent = unit(sub(p[split + 1], p[split - 1]));
        fit_rec(&p[..=split], t0, tangent, tol, depth + 1, out);
        fit_rec(&p[split..], tangent, t1, tol, depth + 1, out);
    }
}
fn generate(p: &[Point], u: &[f64], t0: Point, t1: Point) -> [Point; 4] {
    let a = p[0];
    let b = p[p.len() - 1];
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (&p, &t) in p.iter().zip(u) {
        let v = 1.0 - t;
        let bern = [v * v * v, 3.0 * v * v * t, 3.0 * v * t * t, t * t * t];
        let h0 = mul(t0, bern[1]);
        let h1 = mul(t1, -bern[2]);
        let residual = sub(p, add(mul(a, bern[0] + bern[1]), mul(b, bern[2] + bern[3])));
        c00 += dot(h0, h0);
        c01 += dot(h0, h1);
        c11 += dot(h1, h1);
        x0 += dot(h0, residual);
        x1 += dot(h1, residual);
    }
    let det = c00 * c11 - c01 * c01;
    let len = distance(a, b);
    let (mut l, mut r) = if det.abs() > 1e-12 { ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det) } else { (len / 3.0, len / 3.0) };
    if !l.is_finite() || !r.is_finite() || l < len * 1e-6 || r < len * 1e-6 {
        l = len / 3.0;
        r = l;
    }
    [a, add(a, mul(t0, l.min(len * 4.0))), sub(b, mul(t1, r.min(len * 4.0))), b]
}
fn max_error(p: &[Point], u: &[f64], c: &[Point; 4]) -> (f64, usize) {
    let mut result = (0.0, p.len() / 2);
    for (i, (&p, &t)) in p.iter().zip(u).enumerate() {
        let d = distance(p, eval(c, t));
        if d > result.0 {
            result = (d, i);
        }
    }
    result
}
fn newton(c: &[Point; 4], p: Point, t: f64) -> f64 {
    let q = sub(eval(c, t), p);
    let v = 1.0 - t;
    let d1 = mul(add(add(mul(sub(c[1], c[0]), v * v), mul(sub(c[2], c[1]), 2.0 * v * t)), mul(sub(c[3], c[2]), t * t)), 3.0);
    let d2 = mul(add(mul(add(sub(c[2], mul(c[1], 2.0)), c[0]), v), mul(add(sub(c[3], mul(c[2], 2.0)), c[1]), t)), 6.0);
    let den = dot(d1, d1) + dot(q, d2);
    if den.abs() < 1e-12 { t } else { (t - dot(q, d1) / den).clamp(0.0, 1.0) }
}
fn curve_near_polyline(c: &[Point; 4], points: &[Point], tol: f64) -> bool {
    // Adaptive convex-hull bound: if all controls lie within tol of the same source
    // segment, the entire cubic does. Otherwise subdivide; refusing at the bound splits
    // the original fit instead of accepting an unverified curve.
    fn check(c: &[Point; 4], p: &[Point], tol: f64, depth: usize) -> bool {
        if p.windows(2).any(|w| c.iter().all(|&q| segment_distance(q, w[0], w[1]) <= tol)) {
            return true;
        }
        if depth >= 10 {
            return false;
        }
        let a = mix(c[0], c[1], 0.5);
        let b = mix(c[1], c[2], 0.5);
        let d = mix(c[2], c[3], 0.5);
        let e = mix(a, b, 0.5);
        let f = mix(b, d, 0.5);
        let m = mix(e, f, 0.5);
        check(&[c[0], a, e, m], p, tol, depth + 1) && check(&[m, f, d, c[3]], p, tol, depth + 1)
    }
    // Exact collinear runs are one source segment for this certificate. Keeping thousands
    // of identical-direction samples as separate segments would force unnecessary subdivision.
    let mut line: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        while line.len() >= 2 {
            let a = line[line.len() - 2];
            let b = line[line.len() - 1];
            let u = sub(b, a);
            let v = sub(p, b);
            if (u.x * v.y - u.y * v.x).abs() > 1e-12 * distance(a, p).max(1.0) || dot(u, v) < 0.0 {
                break;
            }
            line.pop();
        }
        line.push(p);
    }
    check(c, &line, tol, 0)
}
fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let d = sub(b, a);
    let l = dot(d, d);
    distance(p, mix(a, b, if l > 1e-20 { (dot(sub(p, a), d) / l).clamp(0.0, 1.0) } else { 0.0 }))
}
pub fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn eval(c: &[Point; 4], t: f64) -> Point {
    let a = mix(c[0], c[1], t);
    let b = mix(c[1], c[2], t);
    let d = mix(c[2], c[3], t);
    mix(mix(a, b, t), mix(b, d, t), t)
}
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Point, t: f64) -> Point {
    Point::new(a.x * t, a.y * t)
}
fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}
fn unit(a: Point) -> Point {
    let len = distance(a, Point::default());
    if len > 1e-12 { mul(a, 1.0 / len) } else { Point::new(1.0, 0.0) }
}
fn mix(a: Point, b: Point, t: f64) -> Point {
    add(mul(a, 1.0 - t), mul(b, t))
}

#[cfg(test)]
#[path = "../../pc-engine/src/vector_packages/pencil/fit_tests.rs"]
mod tests;
