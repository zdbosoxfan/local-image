//! Deterministic skeleton graph extraction, adapted from VectorCraft centreline.rs.
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Control, StrokePath, skeleton::Grid};
use crate::Error;
use kurbo::{BezPath, Point};
use photocraft_doc::vector_design::appearance::{WidthPoint, WidthProfile};
use photocraft_doc::{LineCap, LineJoin, ShapeStroke};

#[derive(Clone, Copy)]
pub(super) struct Sample {
    point: Point,
    radius: f64,
    pixel: usize,
}
pub(super) struct Run {
    points: Vec<Sample>,
}

fn sample(g: &Grid, i: usize, d: &[u32], w: usize) -> Sample {
    let pixel = g.image_index(i, w);
    Sample { point: g.centre(i), radius: (f64::from(d[pixel]).sqrt() - 0.5).max(0.5), pixel }
}
// One byte per grid pixel marks the eight undirected graph edges. No hash-table
// traversal or worker-dependent ordering, and no per-edge allocation.
fn direction(g: &Grid, a: usize, b: usize) -> usize {
    let w = g.w as isize;
    let delta = b as isize - a as isize;
    [-w, -w + 1, 1, w + 1, w, w - 1, -1, -w - 1].iter().position(|&d| d == delta).unwrap_or(0)
}
fn used(g: &Grid, edges: &[u8], a: usize, b: usize) -> bool {
    edges[a] & (1 << direction(g, a, b)) != 0
}
fn mark(g: &Grid, edges: &mut [u8], a: usize, b: usize) {
    let k = direction(g, a, b);
    edges[a] |= 1 << k;
    edges[b] |= 1 << ((k + 4) % 8);
}

pub(super) fn runs(g: &Grid, field: &super::distance::Field, width: usize, c: &mut Control<'_, impl Fn() -> bool>) -> Result<Vec<Run>, Error> {
    let distances = &field.squared;
    let mut skeleton = Vec::new();
    for (k, i) in g.interior().enumerate() {
        if k.is_multiple_of(1024) {
            c.work(1024)?;
        }
        if g.on[i] {
            skeleton.push(i);
        }
    }
    let mut node = vec![usize::MAX; g.on.len()];
    let mut centres: Vec<Sample> = Vec::new();
    for (k, &i) in skeleton.iter().enumerate() {
        if k.is_multiple_of(1024) {
            c.work(1024)?;
        }
        let degree = g.degree(i);
        if degree == 2 || node[i] != usize::MAX {
            continue;
        }
        let id = centres.len();
        let mut members = vec![i];
        node[i] = id;
        if degree > 2 {
            let mut k = 0;
            while let Some(&p) = members.get(k) {
                if k.is_multiple_of(1024) {
                    c.work(1024)?;
                }
                k += 1;
                for q in g.neighbours(p) {
                    if node[q] == usize::MAX && g.degree(q) > 2 {
                        node[q] = id;
                        members.push(q);
                    }
                }
            }
        }
        let mut centre = sample(g, i, distances, width);
        let mut sum = Point::ZERO;
        for &m in &members {
            let s = sample(g, m, distances, width);
            sum += s.point.to_vec2();
            if s.radius > centre.radius {
                centre.radius = s.radius;
                centre.pixel = s.pixel;
            }
        }
        centre.point = Point::new(sum.x / members.len() as f64, sum.y / members.len() as f64);
        centres.push(centre);
    }
    let mut edges = vec![0u8; g.on.len()];
    let mut runs = Vec::new();
    // Isolated foreground points become a zero-length open round-capped stroke.
    // pc-vector explicitly renders these as discs; no arbitrary epsilon segment.
    for (k, &p) in skeleton.iter().enumerate() {
        if k.is_multiple_of(1024) {
            c.work(1024)?;
        }
        if g.degree(p) == 0 {
            if runs.len() >= super::STROKE_BUDGET {
                return Err(Error::BoundaryBudget);
            }
            runs.push(Run { points: vec![sample(g, p, distances, width); 2] });
        }
        if node[p] == usize::MAX {
            continue;
        }
        for q in g.neighbours(p) {
            if node[q] == node[p] || used(g, &edges, p, q) {
                continue;
            }
            if runs.len() >= super::STROKE_BUDGET {
                return Err(Error::BoundaryBudget);
            }
            mark(g, &mut edges, p, q);
            let mut points = vec![centres[node[p]], sample(g, q, distances, width)];
            let (mut prev, mut cur) = (p, q);
            while node[cur] == usize::MAX {
                if points.len().is_multiple_of(1024) {
                    c.work(1024)?;
                }
                if points.len() >= super::NODE_BUDGET {
                    return Err(Error::BoundaryBudget);
                }
                let Some(next) = g.neighbours(cur).find(|&n| n != prev && !used(g, &edges, cur, n)) else {
                    break;
                };
                mark(g, &mut edges, cur, next);
                points.push(sample(g, next, distances, width));
                (prev, cur) = (cur, next);
            }
            if node[cur] != usize::MAX {
                let last = points.len() - 1;
                points[last] = centres[node[cur]];
            }
            runs.push(Run { points });
        }
    }
    for (k, &s) in skeleton.iter().enumerate() {
        if k.is_multiple_of(1024) {
            c.work(1024)?;
        }
        if node[s] != usize::MAX || g.neighbours(s).any(|n| used(g, &edges, s, n)) {
            continue;
        }
        if runs.len() >= super::STROKE_BUDGET {
            return Err(Error::BoundaryBudget);
        }
        let mut points = vec![sample(g, s, distances, width)];
        let (mut prev, mut cur) = (usize::MAX, s);
        while let Some(next) = g.neighbours(cur).find(|&n| n != prev && !used(g, &edges, cur, n)) {
            if points.len().is_multiple_of(1024) {
                c.work(1024)?;
            }
            if points.len() >= super::NODE_BUDGET {
                return Err(Error::BoundaryBudget);
            }
            mark(g, &mut edges, cur, next);
            points.push(sample(g, next, distances, width));
            if next == s {
                break;
            }
            (prev, cur) = (cur, next);
        }
        runs.push(Run { points });
    }
    // Analytic Voronoi ridge refinement using the EDT's nearest background
    // sites. A half-pixel search across the local normal resolves even-width
    // plateaus and avoids diagonal lines losing width through lattice bias.
    // Junctions and endpoints retain their exact shared positions.
    for run in &mut runs {
        let original = run.points.clone();
        for i in 1..run.points.len().saturating_sub(1) {
            c.work(64)?;
            let tangent = original[i + 1].point - original[i - 1].point;
            if tangent.hypot2() < 1e-12 {
                continue;
            }
            let normal = kurbo::Vec2::new(-tangent.y, tangent.x) / tangent.hypot();
            let pixel = original[i].pixel;
            let mut sites = Vec::with_capacity(9);
            for q in std::iter::once(pixel).chain(super::skeleton::neighbours(pixel, width, distances.len() / width)) {
                let (x, y) = field.sites[q];
                let point = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                if !sites.contains(&point) {
                    sites.push(point);
                }
            }
            c.work(sites.len().pow(3) + 2 * sites.len().pow(2))?;
            let p = original[i].point;
            let nearest = |point: Point| sites.iter().map(|s| point.distance_squared(*s)).fold(f64::INFINITY, f64::min);
            let mut best = (nearest(p), 0.);
            for &a in &sites {
                for &b in &sites {
                    let denominator = 2. * normal.dot(b - a);
                    if denominator.abs() < 1e-12 {
                        continue;
                    }
                    let shift = (p.distance_squared(b) - p.distance_squared(a)) / denominator;
                    if shift.abs() > 0.500000001 {
                        continue;
                    }
                    let d = nearest(p + normal * shift);
                    if d > best.0 + 1e-12 {
                        best = (d, shift);
                    }
                }
            }
            run.points[i].point = p + normal * best.1;
            run.points[i].radius = (best.0.sqrt() - 0.5).max(0.5);
        }
    }
    Ok(runs)
}

pub(super) fn fit(run: Run, tolerance: f64, c: &mut Control<'_, impl Fn() -> bool>) -> Result<StrokePath, Error> {
    let mut bez = BezPath::new();
    let first = run.points.first().ok_or(Error::InvalidParams("empty centreline run"))?;
    bez.move_to(first.point);
    // Reuse T1's kurbo fitter. Bound each call to 256 edges, allowing cancellation
    // between fits and preventing one enormous uncancellable simplification.
    let mut start = 0;
    while start + 1 < run.points.len() {
        c.work(256)?;
        let end = (start + 256).min(run.points.len() - 1);
        let mut part = BezPath::new();
        part.move_to(run.points[start].point);
        for point in &run.points[start + 1..=end] {
            part.line_to(point.point);
        }
        let part =
            if run.points[start].point == run.points[end].point && end == start + 1 { part } else { crate::spline::refine_with_angle(&part, tolerance, 3.0) };
        bez.extend(part.elements().iter().skip(1).copied());
        start = end;
    }
    let mut path = crate::adapter::bezpath_to_path(&bez)?;
    // Fitter rounding must never separate shared junctions or a ring's seam.
    let knots = &mut path.subpaths.first_mut().ok_or(Error::InvalidParams("empty fitted centreline"))?.knots;
    for (index, point) in [(0, first.point), (knots.len() - 1, run.points[run.points.len() - 1].point)] {
        let knot = &mut knots[index];
        let dx = point.x - knot.anchor.x;
        let dy = point.y - knot.anchor.y;
        knot.anchor.x = point.x;
        knot.anchor.y = point.y;
        knot.in_ctrl.x += dx;
        knot.in_ctrl.y += dy;
        knot.out_ctrl.x += dx;
        knot.out_ctrl.y += dy;
    }
    let length: f64 = run.points.windows(2).map(|p| p[0].point.distance(p[1].point)).sum();
    let mut profile = Vec::new();
    let mut arc = 0.;
    let mut width = 0f64;
    for (i, sample) in run.points.iter().enumerate() {
        if i.is_multiple_of(1024) {
            c.work(1024)?;
        }
        if i > 0 {
            arc += sample.point.distance(run.points[i - 1].point);
        }
        let t = if length > 0. { arc / length } else { i as f64 / (run.points.len() - 1) as f64 };
        width = width.max(2. * sample.radius);
        if profile.last().is_some_and(|p: &WidthPoint| p.t == t) {
            profile.pop();
        }
        profile.push(WidthPoint { t, left: sample.radius, right: sample.radius });
        // Compress exactly linear width samples, never erase a local extremum.
        while profile.len() >= 3 {
            let n = profile.len();
            let (a, b, d) = (profile[n - 3], profile[n - 2], profile[n - 1]);
            let predicted = a.left + (d.left - a.left) * (b.t - a.t) / (d.t - a.t);
            if (predicted - b.left).abs() > 1e-9 {
                break;
            }
            profile.remove(n - 2);
        }
    }
    let constant = profile.iter().all(|p| (2. * p.left - width).abs() < 1e-9);
    if profile.len() > 4096 {
        return Err(Error::BoundaryBudget);
    }
    Ok(StrokePath {
        path,
        stroke: ShapeStroke { width: width as f32, cap: LineCap::Round, join: LineJoin::Round, ..Default::default() },
        profile: (!constant).then_some(WidthProfile { points: profile }),
    })
}
