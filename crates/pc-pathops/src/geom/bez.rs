//! Adapted from VectorCraft `crates/geom/src/bez.rs` at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Walking a [`BezPath`] as strokes see it: its subpaths (open or closed), their segments, unit
//! tangents and corners. Shared by the stroke geometry (`vectorcraft-effects`) and the visual
//! bounds and hit testing of strokes (`vectorcraft-doc`).

use kurbo::{BezPath, ParamCurve, ParamCurveDeriv, PathEl, PathSeg, Vec2};

/// Tangents meeting at more than about 1° (this cosine) make a corner: fitted dashes centre on it,
/// variable-width strokes give it the stroke's join and miter spikes grow from it.
pub const CORNER_COS: f64 = 0.9998;

/// A curve whose control polygon is longer than this, or with a point farther out, is drawn as a
/// line by [`flatten`]: kurbo squares and cubes coordinates when it sizes its work, and past this
/// that overflows and it reserves a buffer no machine has.
const MAX_FLATTEN_LENGTH: f64 = 1e60;

/// [`kurbo::flatten`], each curve's tolerance raised so that it makes at most about
/// [`MAX_FLATTEN_PER_CURVE`] segments: kurbo sizes its work and a buffer from the curve's
/// coordinates before producing anything, so a small curve with far-out handles asked for
/// gigabytes. The segments a curve needs grow with the square root of its control polygon's length
/// over the tolerance; that ratio is held under the square of the cap. A curve whose points aren't
/// finite or are past [`MAX_FLATTEN_LENGTH`] is drawn as a line to its end.
pub(crate) fn flatten(path: impl IntoIterator<Item = PathEl>, tolerance: f64, mut callback: impl FnMut(PathEl)) {
    let (mut last, mut start) = (None, None);
    for el in path {
        let (from, to, length) = match el {
            PathEl::MoveTo(p) => {
                (last, start) = (Some(p), Some(p));
                callback(el);
                continue;
            }
            PathEl::LineTo(p) => {
                last = Some(p);
                callback(el);
                continue;
            }
            PathEl::ClosePath => {
                last = start;
                callback(el);
                continue;
            }
            PathEl::QuadTo(a, b) => (last, b, last.map(|p| p.distance(a) + a.distance(b))),
            PathEl::CurveTo(a, b, c) => (last, c, last.map(|p| p.distance(a) + a.distance(b) + b.distance(c))),
        };
        last = Some(to);
        let (Some(from), Some(length)) = (from, length) else { continue };
        let far = |p: kurbo::Point| !(p.x.abs() <= MAX_FLATTEN_LENGTH && p.y.abs() <= MAX_FLATTEN_LENGTH);
        let controls_far = match el {
            PathEl::QuadTo(a, _) => far(a),
            PathEl::CurveTo(a, b, _) => far(a) || far(b),
            _ => false,
        };
        if length.is_nan() || length > MAX_FLATTEN_LENGTH || far(from) || far(to) || controls_far {
            callback(PathEl::LineTo(to));
            continue;
        }
        let tol = tolerance.max(length / (MAX_FLATTEN_PER_CURVE * MAX_FLATTEN_PER_CURVE));
        kurbo::flatten([PathEl::MoveTo(from), el], tol, |e| {
            if !matches!(e, PathEl::MoveTo(_)) {
                callback(e);
            }
        });
    }
}

/// Most line segments [`flatten`] makes of one curve, whatever its size and the tolerance.
pub(crate) const MAX_FLATTEN_PER_CURVE: f64 = 4096.0;

/// Element ranges of the subpaths of `bp` (each with whether it is closed).
pub fn subpaths(bp: &BezPath) -> Vec<(std::ops::Range<usize>, bool)> {
    let els = bp.elements();
    let mut out = vec![];
    let mut start = 0;
    for (i, el) in els.iter().enumerate() {
        match el {
            PathEl::MoveTo(_) if i > start => {
                out.push((start..i, false));
                start = i;
            }
            PathEl::ClosePath => {
                out.push((start..i + 1, true));
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < els.len() {
        out.push((start..els.len(), false));
    }
    // A lone MoveTo (or a MoveTo straight after a ClosePath) draws nothing.
    out.retain(|(r, _)| r.len() > 1);
    out
}

/// The segments of one subpath (`els` starts with its MoveTo; a ClosePath adds the closing line).
pub fn segments(els: &[PathEl]) -> Vec<PathSeg> {
    kurbo::segments(els.iter().copied()).collect()
}

/// Unit tangent of `s` at `t`, robust at ends where control points coincide with the end point.
pub fn tangent(s: &PathSeg, t: f64) -> Vec2 {
    let d = match s {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => {
            let v = q.deriv().eval(t).to_vec2();
            if v.hypot() > 1e-9 { v } else { q.p2 - q.p0 }
        }
        PathSeg::Cubic(c) => {
            let v = c.deriv().eval(t).to_vec2();
            if v.hypot() > 1e-9 {
                v
            } else if t < 0.5 {
                // The first control point that differs from the start gives the direction.
                [c.p2, c.p3].into_iter().map(|p| p - c.p0).find(|v| v.hypot() > 1e-9).unwrap_or_default()
            } else {
                [c.p1, c.p0].into_iter().map(|p| c.p3 - p).find(|v| v.hypot() > 1e-9).unwrap_or_default()
            }
        }
    };
    unit(d)
}

/// `v` normalised, or +x when it has no length.
pub fn unit(v: Vec2) -> Vec2 {
    let h = v.hypot();
    if h > 1e-12 { v / h } else { Vec2::new(1.0, 0.0) }
}

/// Do segments `a` and `b` (`b` following `a`) meet at a corner?
pub fn kink(a: &PathSeg, b: &PathSeg) -> bool {
    tangent(a, 1.0).dot(tangent(b, 0.0)) < CORNER_COS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpaths_and_tangents() {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((10.0, 0.0));
        bp.line_to((10.0, 10.0));
        bp.close_path();
        bp.move_to((20.0, 0.0));
        bp.curve_to((20.0, 0.0), (30.0, 0.0), (30.0, 10.0));
        let subs = subpaths(&bp);
        assert_eq!(subs, vec![(0..4, true), (4..6, false)]);
        let segs = segments(&bp.elements()[subs[0].0.clone()]);
        assert_eq!(segs.len(), 3, "the close adds a segment");
        assert!(kink(&segs[0], &segs[1]));
        let c = segments(&bp.elements()[subs[1].0.clone()]);
        // A handle on its anchor: the direction comes from the next control point.
        assert_eq!(tangent(&c[0], 0.0), Vec2::new(1.0, 0.0));
        assert_eq!(tangent(&c[0], 1.0), Vec2::new(0.0, 1.0));
        assert_eq!(unit(Vec2::ZERO), Vec2::new(1.0, 0.0));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod flatten_tests {
    use super::*;
    use kurbo::Point;

    #[test]
    fn flatten_bounds_the_segments_of_one_curve() {
        let count = |path: &[PathEl], tol: f64| {
            let mut n = 0usize;
            flatten(path.iter().copied(), tol, |_| n += 1);
            n
        };
        let wild = [PathEl::MoveTo(Point::ZERO), PathEl::CurveTo(Point::new(1e15, -1e15), Point::new(-1e15, 1e15), Point::new(1.0, 0.0))];
        assert!(count(&wild, 0.25) < 3 * MAX_FLATTEN_PER_CURVE as usize);
        let plain = [PathEl::MoveTo(Point::ZERO), PathEl::CurveTo(Point::new(0.0, 50.0), Point::new(100.0, 50.0), Point::new(100.0, 0.0)), PathEl::ClosePath];
        let mut ours = vec![];
        flatten(plain.iter().copied(), 0.25, |e| ours.push(e));
        let mut theirs = vec![];
        kurbo::flatten(plain.iter().copied(), 0.25, |e| theirs.push(e));
        assert_eq!(ours, theirs);
        let nan = [PathEl::MoveTo(Point::ZERO), PathEl::QuadTo(Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0))];
        assert_eq!(count(&nan, 0.25), 2);
        // Finite coordinates whose squares overflow are a line to the end too.
        let huge = [PathEl::MoveTo(Point::ZERO), PathEl::CurveTo(Point::new(1e200, 0.0), Point::new(-1e200, 1e200), Point::new(1.0, 0.0))];
        assert_eq!(count(&huge, 0.25), 2);
        // A short polygon out near f64::MAX: a line, and every point it emits is finite.
        let m = f64::MAX / 2.0;
        let far = [PathEl::MoveTo(Point::new(m, m)), PathEl::CurveTo(Point::new(m, m), Point::new(m, m), Point::new(m, m))];
        let mut points = vec![];
        flatten(far.iter().copied(), 0.25, |e| points.push(e));
        assert_eq!(points.len(), 2);
        assert!(points.iter().all(|e| matches!(e, PathEl::MoveTo(p) | PathEl::LineTo(p) if p.is_finite())), "{points:?}");
    }
}
