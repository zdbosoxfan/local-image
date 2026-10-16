//! Shape recognition adapted from VectorCraft `crates/geom/src/recognize.rs`
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092: distance resampling,
//! covariance orientation and bounds in the fitted frame. Unlike the Shaper,
//! this evaluator requires a bidirectional residual and preserves ring winding.
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vectorcraft-LICENSE-MIT, -APACHE and -NOTICE.
use super::Options;
use kurbo::{Arc, BezPath, Point};
use photocraft_doc::Subpath;

pub(super) fn resample(points: &[Point], count: usize) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if length <= 1e-12 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    let mut walked = 0.;
    for w in points.windows(2) {
        let segment = w[0].distance(w[1]);
        if segment <= 0. {
            continue;
        }
        while out.len() < count - 1 {
            let target = length * out.len() as f64 / (count - 1) as f64;
            if target > walked + segment {
                break;
            }
            out.push(w[0] + (w[1] - w[0]) * ((target - walked) / segment).clamp(0., 1.));
        }
        walked += segment;
    }
    if let Some(&last) = points.last() {
        out.push(last);
    }
    out
}

pub(super) fn area(points: &[Point]) -> f64 {
    let Some(&origin) = points.first() else { return 0. };
    points.windows(2).map(|w| (w[0] - origin).cross(w[1] - origin)).sum::<f64>() / 2.
}

pub(super) fn ellipse(points: &[Point], source: &Subpath, options: &Options) -> Option<(Subpath, bool)> {
    if !source.closed || source.knots.len() < 5 || points.len() < 8 {
        return None;
    }
    let sampled = resample(points, 257);
    let pts = &sampled[..sampled.len() - 1];
    let n = pts.len() as f64;
    let origin = pts[0];
    let center = origin + pts.iter().fold(kurbo::Vec2::ZERO, |sum, p| sum + (*p - origin) / n);
    let (xx, yy, xy) = pts.iter().fold((0., 0., 0.), |(xx, yy, xy), p| {
        let v = *p - center;
        (xx + v.x * v.x, yy + v.y * v.y, xy + v.x * v.y)
    });
    let rotation = if (xx - yy).hypot(2. * xy) < 0.01 * (xx + yy) { 0. } else { (2. * xy).atan2(xx - yy) / 2. };
    let (sin, cos) = rotation.sin_cos();
    let local = |p: Point| {
        let p = p - center;
        Point::new(p.x * cos + p.y * sin, -p.x * sin + p.y * cos)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &p in points {
        let p = local(p);
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let (mut rx, mut ry) = ((x1 - x0) / 2., (y1 - y0) / 2.);
    if rx.min(ry) < 0.5 {
        return None;
    }
    let circle = (rx - ry).abs() <= options.tolerance;
    if circle && options.circles {
        let r = (rx + ry) / 2.;
        rx = r;
        ry = r;
    } else if !options.ellipses {
        return None;
    }
    let (cx, cy) = ((x0 + x1) / 2., (y0 + y1) / 2.);
    let center = center + kurbo::Vec2::new(cx * cos - cy * sin, cx * sin + cy * cos);
    let sweep = std::f64::consts::TAU.copysign(area(points));
    let arc = Arc::new(center, (rx, ry), 0., sweep, rotation);
    let mut bez = BezPath::new();
    bez.move_to(center + kurbo::Vec2::new(rx * cos, rx * sin));
    bez.extend(arc.append_iter(options.tolerance / 16.));
    let first = center + kurbo::Vec2::new(rx * cos, rx * sin);
    if let Some(kurbo::PathEl::CurveTo(_, _, end)) = bez.elements_mut().last_mut() {
        *end = first;
    }
    bez.close_path();
    let mut result = crate::adapter::bezpath_to_path(&bez).ok()?.subpaths.pop()?;
    result.op = source.op;
    Some((result, circle && options.circles))
}

/// Recognize an open circular run, or a closed arc with a straight closing chord.
/// The centre is the circumcentre of three distance-spaced samples; a full residual
/// guard in geometry.rs rejects noncircular, reversing or overswept runs.
pub(super) fn arc(points: &[Point], source: &Subpath, options: &Options) -> Option<Subpath> {
    if source.knots.len() < 4 || points.len() < 5 {
        return None;
    }
    let run = if source.closed {
        let first = source.knots.first()?;
        let last = source.knots.last()?;
        if first.in_ctrl != first.anchor || last.out_ctrl != last.anchor {
            return None;
        }
        &points[..points.len() - 1]
    } else {
        points
    };
    let sampled = resample(run, 129);
    let (&a, &b, &c) = (sampled.first()?, sampled.get(sampled.len() / 2)?, sampled.last()?);
    let (u, v) = (b - a, c - a);
    let det = 2. * u.cross(v);
    if det.abs() < 1e-8 {
        return None;
    }
    let center = a + kurbo::Vec2::new((u.hypot2() * v.y - v.hypot2() * u.y) / det, (v.hypot2() * u.x - u.hypot2() * v.x) / det);
    let radius = a.distance(center);
    if !radius.is_finite() || !(0.5..=1e6).contains(&radius) {
        return None;
    }
    let start = (a - center).atan2();
    let mut last = start;
    let mut sweep = 0.;
    let mut direction = 0_f64;
    for &p in sampled.iter().skip(1) {
        if (p.distance(center) - radius).abs() > options.tolerance {
            return None;
        }
        let angle = (p - center).atan2();
        let delta = (angle - last + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        if delta.abs() > 1e-8 {
            if direction != 0. && delta.signum() != direction {
                return None;
            }
            direction = delta.signum();
        }
        sweep += delta;
        last = angle;
    }
    if sweep.abs() < 0.2 || sweep.abs() > std::f64::consts::TAU {
        return None;
    }
    let mut bez = BezPath::new();
    bez.move_to(a);
    bez.extend(Arc::new(center, (radius, radius), start, sweep, 0.).append_iter(options.tolerance / 16.));
    if source.closed {
        bez.close_path();
    }
    let mut result = crate::adapter::bezpath_to_path(&bez).ok()?.subpaths.pop()?;
    result.op = source.op;
    Some(result)
}

// Bounded Douglas–Peucker and seam-independent closed-ring splitting adapted
// from the same pinned VectorCraft recognize.rs (simplify/closed_corners).
fn simplify(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut pending = vec![(0, points.len() - 1)];
    while let Some((start, end)) = pending.pop() {
        let a = points[start];
        let b = points[end];
        let v = b - a;
        let len = v.hypot2();
        let mut worst = (tolerance, 0);
        for (i, &p) in points.iter().enumerate().take(end).skip(start + 1) {
            let q = if len > 1e-20 { a + v * ((p - a).dot(v) / len).clamp(0., 1.) } else { a };
            let d = p.distance(q);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.1 != 0 {
            keep[worst.1] = true;
            pending.push((start, worst.1));
            pending.push((worst.1, end));
        }
    }
    points.iter().zip(keep).filter_map(|(&p, k)| k.then_some(p)).collect()
}
pub(super) fn polygon(points: &[Point], source: &Subpath, tolerance: f64) -> Option<Subpath> {
    let result = if source.closed {
        let pts = points.get(..points.len().checked_sub(1)?)?;
        let first = *pts.first()?;
        let split = pts.iter().enumerate().max_by(|(_, a), (_, b)| a.distance(first).total_cmp(&b.distance(first))).map(|(i, _)| i)?;
        let mut left = simplify(&pts[..=split], tolerance);
        left.pop();
        let right: Vec<_> = pts[split..].iter().copied().chain(std::iter::once(first)).collect();
        let mut right = simplify(&right, tolerance);
        right.pop();
        left.extend(right);
        left
    } else {
        simplify(points, tolerance)
    };
    if result.len() < if source.closed { 3 } else { 2 } || result.len() >= source.knots.len() {
        return None;
    }
    Some(Subpath { closed: source.closed, op: source.op, knots: result.iter().map(|p| photocraft_doc::Knot::corner(p.x, p.y)).collect() })
}

/// Fit long straight sides by covariance, then intersect their support lines.
/// Unlike dropping seam vertices, this averages raster stair-step error instead
/// of preserving the most displaced sample as a new polygon corner.
pub(super) fn straight_polygon(points: &[Point], source: &Subpath, tolerance: f64) -> Option<Subpath> {
    if !source.closed {
        return None;
    }
    let mut coarse = polygon(points, source, tolerance)?;
    let mut i = 0;
    while coarse.knots.len() > 3 && i < coarse.knots.len() {
        let n = coarse.knots.len();
        let point = |k: photocraft_doc::Knot| Point::new(k.anchor.x, k.anchor.y);
        let a = point(coarse.knots[(i + n - 1) % n]);
        let b = point(coarse.knots[i]);
        let c = point(coarse.knots[(i + 1) % n]);
        let (u, v) = (b - a, c - b);
        if u.dot(v) > 0. && u.cross(v).abs() < 0.1 * u.dot(v) {
            coarse.knots.remove(i);
        } else {
            i += 1;
        }
    }
    let corners: Vec<_> = coarse.knots.iter().map(|k| Point::new(k.anchor.x, k.anchor.y)).collect();
    if corners.len() > 32 {
        return None;
    }
    let n = corners.len();
    let mut groups = vec![Vec::new(); n];
    for &p in points {
        let index = (0..n).min_by(|&i, &j| {
            let dist = |i: usize| {
                let a = corners[i];
                let v = corners[(i + 1) % n] - a;
                p.distance(a + v * ((p - a).dot(v) / v.hypot2().max(1e-20)).clamp(0., 1.))
            };
            dist(i).total_cmp(&dist(j))
        })?;
        groups[index].push(p);
    }
    let mut lines = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if group.len() < 3 || corners[i].distance(corners[(i + 1) % n]) < 6. {
            return None;
        }
        let origin = group[0];
        let center = origin + group.iter().fold(kurbo::Vec2::ZERO, |sum, p| sum + (*p - origin) / group.len() as f64);
        let (xx, yy, xy) = group.iter().fold((0., 0., 0.), |(xx, yy, xy), p| {
            let v = *p - center;
            (xx + v.x * v.x, yy + v.y * v.y, xy + v.x * v.y)
        });
        let angle = (2. * xy).atan2(xx - yy) / 2.;
        let direction = kurbo::Vec2::new(angle.cos(), angle.sin());
        let error = group.iter().map(|p| (*p - center).cross(direction).abs()).sum::<f64>() / group.len() as f64;
        if error > tolerance / 3. {
            return None;
        }
        lines.push((center, direction));
    }
    let mut knots = Vec::new();
    for i in 0..n {
        let (a, u) = lines[(i + n - 1) % n];
        let (b, v) = lines[i];
        let det = u.cross(v);
        if det.abs() < 0.2 {
            return None;
        }
        let p = a + u * ((b - a).cross(v) / det);
        if p.distance(corners[i]) > tolerance * 2. {
            return None;
        }
        knots.push(photocraft_doc::Knot::corner(p.x, p.y));
    }
    Some(Subpath { closed: true, op: source.op, knots })
}
