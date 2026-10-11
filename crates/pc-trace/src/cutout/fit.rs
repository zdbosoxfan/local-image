//! Fit each shared chain once, with exact junction endpoints.
// Adapted from visioncortex/vtracer crates/vtracer/src/mosaic/fit.rs
// at 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and licenses/vtracer-LICENSE-APACHE.
use super::graph::{BoundaryGraph, Segment};
use crate::{Error, Fitter, Params};
use kurbo::{BezPath, PathSeg, Point};

// Whole chain reversal swaps existing f64s, never computes new controls.
pub(super) type Geometry = Vec<PathSeg>;
const FIT_SLICE: usize = 256;
const RING_FIT_LIMIT: usize = 4096;
fn points(seg: &Segment, cancel: &dyn Fn() -> bool) -> Result<Vec<Point>, Error> {
    let mut points = Vec::with_capacity(seg.points.len());
    for (i, p) in seg.points.iter().enumerate() {
        if i % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        points.push(Point::new(f64::from(p.x), f64::from(p.y)));
    }
    Ok(points)
}
pub(super) fn pixel(seg: &Segment, cancel: &dyn Fn() -> bool) -> Result<Geometry, Error> {
    let p = points(seg, cancel)?;
    pixel_points(&p, cancel)
}
fn pixel_points(p: &[Point], cancel: &dyn Fn() -> bool) -> Result<Geometry, Error> {
    // Remove only exact collinear lattice vertices, preserving ring closure.
    let mut b = BezPath::new();
    b.move_to(p[0]);
    for i in 1..p.len() {
        if i % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        if i + 1 == p.len() || (p[i] - p[i - 1]).cross(p[i + 1] - p[i]) != 0. {
            b.line_to(p[i]);
        }
    }
    Ok(b.segments().collect())
}
fn open(points: &[Point], tolerance: f64, width: u32, height: u32) -> BezPath {
    let mut b = BezPath::new();
    b.move_to(points[0]);
    for i in 1..points.len() {
        let mut p = points[i];
        // Symmetric staircase centering; long edges/corners are retained.
        let nearby_turn = i >= 2 && (points[i - 1] - points[i - 2]).cross(p - points[i - 1]) != 0.
            || i + 2 < points.len() && (points[i + 1] - p).cross(points[i + 2] - points[i + 1]) != 0.;
        if p.x > 0.
            && p.y > 0.
            && p.x < f64::from(width)
            && p.y < f64::from(height)
            && i + 1 < points.len()
            && nearby_turn
            && points[i - 1].distance(p) == 1.
            && p.distance(points[i + 1]) == 1.
        {
            p = points[i - 1].midpoint(points[i + 1]);
        }
        b.line_to(p);
    }
    let options = kurbo::simplify::SimplifyOptions::default().angle_thresh(2.).opt_level(kurbo::simplify::SimplifyOptLevel::Subdivide);
    kurbo::simplify::simplify_bezpath(&b, (tolerance + 0.5).min(0.7), &options)
}
pub(super) fn fit_all(graph: &BoundaryGraph, params: &Params, cancel: &dyn Fn() -> bool) -> Result<Vec<Geometry>, Error> {
    let mut fitted = Vec::with_capacity(graph.segments.len());
    for seg in &graph.segments {
        if cancel() {
            return Err(Error::Cancelled);
        }
        if params.fitter == Fitter::Pixel {
            fitted.push(pixel(seg, cancel)?);
            continue;
        }
        let p = points(seg, cancel)?;
        // Bound a single uninterruptible call into T1's closed fitters.
        // Large rings retain exact lattice geometry instead of fitting an
        // artificial closing chord across individually fitted slices.
        let geom: Geometry = if params.fitter == Fitter::Pixel || (seg.is_ring() && p.len() > RING_FIT_LIMIT) {
            pixel_points(&p, cancel)?
        } else if seg.is_ring() {
            let ring = &p[..p.len() - 1];
            let b = match params.fitter {
                Fitter::Potrace => crate::spline::refine(&crate::potrace::fit(ring, params.alphamax, params.opttolerance), params.detail),
                Fitter::Spline => crate::spline::fit(ring, params.detail),
                Fitter::Pixel => crate::boundary::polygon(ring),
            };
            b.segments().collect()
        } else {
            let mut geom = Vec::new();
            let mut start = 0;
            while start + 1 < p.len() {
                if cancel() {
                    return Err(Error::Cancelled);
                }
                let end = (start + FIT_SLICE).min(p.len() - 1);
                geom.extend(open(&p[start..=end], params.detail, graph.width, graph.height).segments());
                start = end;
            }
            geom
        };
        // Raise quadratics ONCE before any reversal. Raising separately in
        // the adapter on either side can round 2/3 arithmetic differently.
        fitted.push(
            geom.into_iter()
                .map(|s| match s {
                    PathSeg::Quad(q) => PathSeg::Cubic(q.raise()),
                    other => other,
                })
                .collect(),
        );
    }
    Ok(fitted)
}

/// Bounded polygon approximation used before exact lattice fallback. It retains
/// junction endpoints, and the arrangement guard still checks every resulting edge.
pub(super) fn conservative(seg: &Segment, tolerance: f64, cancel: &dyn Fn() -> bool) -> Result<Geometry, Error> {
    let p = points(seg, cancel)?;
    if p.len() > RING_FIT_LIMIT {
        return pixel_points(&p, cancel);
    }
    fn rdp(points: &[Point], tolerance: f64, cancel: &dyn Fn() -> bool) -> Result<Vec<Point>, Error> {
        let mut keep = vec![false; points.len()];
        keep[0] = true;
        keep[points.len() - 1] = true;
        let mut work = vec![(0, points.len() - 1)];
        while let Some((a, b)) = work.pop() {
            if cancel() {
                return Err(Error::Cancelled);
            }
            let chord = points[b] - points[a];
            let length = chord.hypot2();
            let mut furthest = (0., a);
            for i in a + 1..b {
                let t = if length > 0. { ((points[i] - points[a]).dot(chord) / length).clamp(0., 1.) } else { 0. };
                let distance = points[i].distance(points[a] + chord * t);
                if distance > furthest.0 {
                    furthest = (distance, i);
                }
            }
            if furthest.0 > tolerance {
                keep[furthest.1] = true;
                work.push((a, furthest.1));
                work.push((furthest.1, b));
            }
        }
        Ok(points.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect())
    }
    // Centre short alternating stairs before simplifying; real corners remain exact.
    let mut centered = p.clone();
    for i in 1..p.len() - 1 {
        let a = p[i] - p[i - 1];
        let b = p[i + 1] - p[i];
        let nearby = i >= 2 && (p[i - 1] - p[i - 2]).cross(a) != 0. || i + 2 < p.len() && b.cross(p[i + 2] - p[i + 1]) != 0.;
        if nearby && a.hypot2() == 1. && b.hypot2() == 1. && a.cross(b) != 0. {
            centered[i] = p[i - 1].midpoint(p[i + 1]);
        }
    }
    let p = centered;
    let tolerance = tolerance.clamp(0., 0.5);
    let simplified = if seg.is_ring() {
        let split = (1..p.len() - 1).max_by(|a, b| p[*a].distance(p[0]).total_cmp(&p[*b].distance(p[0]))).ok_or(Error::Topology)?;
        let mut first = rdp(&p[..=split], tolerance, cancel)?;
        first.extend(rdp(&p[split..], tolerance, cancel)?.into_iter().skip(1));
        first
    } else {
        rdp(&p, tolerance, cancel)?
    };
    let mut path = BezPath::new();
    path.move_to(simplified[0]);
    for p in &simplified[1..] {
        path.line_to(*p);
    }
    Ok(path.segments().collect())
}
