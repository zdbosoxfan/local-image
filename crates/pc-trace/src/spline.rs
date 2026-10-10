//! Vtracer/visioncortex spline pipeline: staircase removal, polygon reduction,
//! four-point subdivision preserving corners, then optimal kurbo cubic refitting.
// Copyright (c) 2026 TSANG, Hao Fung, visioncortex contributors.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// Ports of visioncortex path/{simplify,smooth,spline}.rs and vtracer simplify.rs.
// flo_curves fitting is replaced with kurbo 0.13.1.
use kurbo::{BezPath, Point};
fn reduce(points: &[Point], tolerance: f64) -> Vec<Point> {
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut todo = vec![(0, points.len() - 1)];
    while let Some((a, b)) = todo.pop() {
        let d = points[b] - points[a];
        let l = d.hypot2();
        let mut worst = (tolerance * tolerance, 0);
        for i in a + 1..b {
            let v = points[i] - points[a];
            let t = if l == 0. { 0. } else { v.dot(d) / l }.clamp(0., 1.);
            let e = (v - d * t).hypot2();
            if e > worst.0 {
                worst = (e, i);
            }
        }
        if worst.1 != 0 {
            keep[worst.1] = true;
            todo.push((a, worst.1));
            todo.push((worst.1, b));
        }
    }
    points.iter().enumerate().filter_map(|(i, &p)| keep[i].then_some(p)).collect()
}
pub fn fit(points: &[Point], tolerance: f64) -> BezPath {
    if points.len() < 3 {
        return BezPath::new();
    }
    let mut points = crate::boundary::remove_collinear(points);
    let n = points.len();
    if n < 3 {
        return crate::boundary::polygon(&points);
    }
    let area: f64 = (0..n).map(|i| points[i].x * points[(i + 1) % n].y - points[(i + 1) % n].x * points[i].y).sum();
    // Keep outward turns in a one-pixel staircase, as in PathSimplify::remove_staircase.
    let filtered: Vec<Point> = (0..n)
        .filter_map(|i| {
            let a = points[(i + n - 1) % n];
            let p = points[i];
            let b = points[(i + 1) % n];
            let short = (p - a).hypot() <= 1. || (b - p).hypot() <= 1.;
            let turn = (p - a).cross(b - p);
            (!short || turn.signum() == area.signum()).then_some(p)
        })
        .collect();
    if filtered.len() >= 3 {
        points = filtered;
    }
    points.push(points[0]);
    points = reduce(&points, tolerance.clamp(0.25, 1.));
    points.pop();
    let n = points.len();
    if n < 3 {
        return crate::boundary::polygon(&points);
    }
    let mut corners: Vec<bool> = (0..n)
        .map(|i| {
            let a = points[i] - points[(i + n - 1) % n];
            let b = points[(i + 1) % n] - points[i];
            a.cross(b).atan2(a.dot(b)).abs() >= std::f64::consts::PI / 3.
        })
        .collect();
    for _ in 0..10 {
        let n = points.len();
        let mut next = Vec::new();
        let mut flags = Vec::new();
        let mut changed = false;
        for i in 0..n {
            let j = (i + 1) % n;
            next.push(points[i]);
            flags.push(corners[i]);
            let length = points[i].distance(points[j]);
            if length <= 4. {
                continue;
            }
            let prev = if corners[i] { i } else { (i + n - 1) % n };
            let after = if corners[j] { j } else { (j + 1) % n };
            if prev == i && after == j {
                continue;
            }
            let lp = points[prev].distance(points[i]);
            let ln = points[after].distance(points[j]);
            if lp / length >= 2. || ln / length >= 2. {
                continue;
            }
            let mid = points[i].midpoint(points[j]);
            let inner = points[prev].midpoint(points[after]);
            let p = mid + (mid - inner) * 0.25;
            next.push(p);
            flags.push(false);
            changed = true;
        }
        points = next;
        corners = flags;
        if !changed {
            break;
        }
    }
    let n = points.len();
    let mut path = BezPath::new();
    path.move_to(points[0]);
    for i in 0..n {
        let j = (i + 1) % n;
        let d = points[j] - points[i];
        let a = if corners[i] { d } else { (points[j] - points[(i + n - 1) % n]) * 0.5 };
        let b = if corners[j] { d } else { (points[(j + 1) % n] - points[i]) * 0.5 };
        if corners[i] && corners[j] {
            path.line_to(points[j]);
        } else {
            path.curve_to(points[i] + a / 3., points[j] - b / 3., points[j]);
        }
    }
    path.close_path();
    refine(&path, tolerance)
}
pub fn refine(path: &BezPath, tolerance: f64) -> BezPath {
    let options = kurbo::simplify::SimplifyOptions::default().opt_level(kurbo::simplify::SimplifyOptLevel::Optimize).angle_thresh(1e-3);
    let result = kurbo::simplify::simplify_bezpath(path, tolerance, &options);
    if result.elements().len() <= path.elements().len()
        && result.elements().iter().all(|e| match e {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => p.x.is_finite() && p.y.is_finite(),
            kurbo::PathEl::CurveTo(a, b, c) => [a, b, c].iter().all(|p| p.x.is_finite() && p.y.is_finite()),
            kurbo::PathEl::QuadTo(a, b) => [a, b].iter().all(|p| p.x.is_finite() && p.y.is_finite()),
            kurbo::PathEl::ClosePath => true,
        })
    {
        result
    } else {
        path.clone()
    }
}
