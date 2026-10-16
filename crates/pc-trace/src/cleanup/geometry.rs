//! Conservative per-ring cleanup. No component/counter is deleted. Every proposal
//! is checked against the original curve in both directions before publication.
use super::{Options, Stats, check, recognize, symmetry};
use crate::{Error, Trace};
use kurbo::{BezPath, PathEl, Point};
use photocraft_doc::{Knot, Subpath};

const SAMPLE_BUDGET: usize = 8192;
const DISTANCE_BUDGET: usize = 4_000_000;

pub(super) fn points(sub: &Subpath, tolerance: f64, cancel: &impl Fn() -> bool) -> Result<Vec<Point>, Error> {
    let mut result = Vec::new();
    if let Some(k) = sub.knots.first() {
        result.push(Point::new(k.anchor.x, k.anchor.y));
    }
    for c in sub.segments() {
        check(cancel)?;
        let mut bez = BezPath::new();
        bez.move_to((c[0].x, c[0].y));
        bez.curve_to((c[1].x, c[1].y), (c[2].x, c[2].y), (c[3].x, c[3].y));
        kurbo::flatten(bez, tolerance / 16., |el| {
            if let PathEl::LineTo(p) = el
                && result.len() <= SAMPLE_BUDGET
            {
                result.push(p);
            }
        });
        if result.len() > SAMPLE_BUDGET {
            return Err(Error::BoundaryBudget);
        }
    }
    Ok(result)
}
fn distance(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let l2 = ab.hypot2();
    if l2 < 1e-20 {
        return p.distance(a);
    }
    p.distance(a + ab * ((p - a).dot(ab) / l2).clamp(0., 1.))
}
fn directed(a: &[Point], b: &[Point], tolerance: f64, cancel: &impl Fn() -> bool) -> Result<bool, Error> {
    for (i, &p) in a.iter().enumerate() {
        if i % 64 == 0 {
            check(cancel)?;
        }
        if b.windows(2).map(|w| distance(p, w[0], w[1])).fold(f64::INFINITY, f64::min) > tolerance {
            return Ok(false);
        }
    }
    Ok(true)
}
fn acceptable(original: &[Point], candidate: &Subpath, tolerance: f64, cancel: &impl Fn() -> bool) -> Result<bool, Error> {
    let sample = points(candidate, tolerance, cancel)?;
    if original.len().saturating_mul(sample.len()) > DISTANCE_BUDGET {
        return Ok(false);
    }
    if candidate.closed && recognize::area(original) * recognize::area(&sample) <= 0. {
        return Ok(false);
    }
    // Reserve flattening error on both curves, so the requested tolerance remains an upper bound.
    let limit = tolerance * 0.875;
    Ok(directed(original, &sample, limit, cancel)? && directed(&sample, original, limit, cancel)?)
}
fn straight(a: &Knot, b: &Knot) -> bool {
    a.out_ctrl == a.anchor && b.in_ctrl == b.anchor
}
fn move_to(k: &mut Knot, x: f64, y: f64) {
    let (dx, dy) = (x - k.anchor.x, y - k.anchor.y);
    k.anchor.x = x;
    k.anchor.y = y;
    k.in_ctrl.x += dx;
    k.in_ctrl.y += dy;
    k.out_ctrl.x += dx;
    k.out_ctrl.y += dy;
}
fn linear(sub: &Subpath, options: &Options, stats: &mut Stats) -> Subpath {
    let mut out = sub.clone();
    if options.collinear {
        let mut i = 0;
        while out.knots.len() > if out.closed { 3 } else { 2 } && i < out.knots.len() {
            let n = out.knots.len();
            if !out.closed && (i == 0 || i + 1 == n) {
                i += 1;
                continue;
            }
            let (a, b, c) = (out.knots[(i + n - 1) % n], out.knots[i], out.knots[(i + 1) % n]);
            let pt = |k: Knot| Point::new(k.anchor.x, k.anchor.y);
            if straight(&a, &b) && straight(&b, &c) && distance(pt(b), pt(a), pt(c)) <= options.tolerance * 0.25 && (pt(b) - pt(a)).dot(pt(c) - pt(b)) >= 0. {
                out.knots.remove(i);
                stats.collinear_nodes += 1;
                i = i.saturating_sub(1);
            } else {
                i += 1;
            }
        }
    }
    if options.axes {
        let n = out.knots.len();
        for i in 0..if out.closed { n } else { n.saturating_sub(1) } {
            let j = (i + 1) % n;
            let (a, b) = (out.knots[i], out.knots[j]);
            if !straight(&a, &b) {
                continue;
            }
            let (dx, dy) = (b.anchor.x - a.anchor.x, b.anchor.y - a.anchor.y);
            let limit = options.angle_degrees.to_radians().tan();
            if dy.abs() > 1e-12 && dy.abs() <= dx.abs() * limit && dy.abs() <= options.tolerance {
                let y = (a.anchor.y + b.anchor.y) / 2.;
                move_to(&mut out.knots[i], a.anchor.x, y);
                move_to(&mut out.knots[j], b.anchor.x, y);
                stats.axis_segments += 1;
            } else if dx.abs() > 1e-12 && dx.abs() <= dy.abs() * limit && dx.abs() <= options.tolerance {
                let x = (a.anchor.x + b.anchor.x) / 2.;
                move_to(&mut out.knots[i], x, a.anchor.y);
                move_to(&mut out.knots[j], x, b.anchor.y);
                stats.axis_segments += 1;
            }
        }
    }
    if options.right_angles {
        let n = out.knots.len();
        for i in 0..n {
            if !out.closed && (i == 0 || i + 1 == n) {
                continue;
            }
            let (a, b, c) = (out.knots[(i + n - 1) % n], out.knots[i], out.knots[(i + 1) % n]);
            if !straight(&a, &b) || !straight(&b, &c) {
                continue;
            }
            let pt = |k: Knot| Point::new(k.anchor.x, k.anchor.y);
            let (u, v) = (pt(a) - pt(b), pt(c) - pt(b));
            if u.hypot() < 1e-10 || v.hypot() < 1e-10 {
                continue;
            }
            let cosine = u.dot(v) / (u.hypot() * v.hypot());
            if cosine.abs() > options.angle_degrees.to_radians().sin() || cosine.abs() < 1e-12 {
                continue;
            }
            // Thales' circle: every point on the diameter a–c makes a right angle.
            let center = pt(a).lerp(pt(c), 0.5);
            let direction = pt(b) - center;
            if direction.hypot() < 1e-10 {
                continue;
            }
            let p = center + direction * (pt(a).distance(pt(c)) / (2. * direction.hypot()));
            if p.distance(pt(b)) <= options.tolerance * 0.5 {
                move_to(&mut out.knots[i], p.x, p.y);
                stats.right_angle_corners += 1;
            }
        }
    }
    out
}

pub(super) fn clean(trace: &mut Trace, options: &Options, pixels: &[u8], scale: u32, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<(), Error> {
    for layer_index in 0..trace.layers.len() {
        check(cancel)?;
        let axes = if options.symmetry { symmetry::axes(trace, layer_index, pixels, scale, stats, cancel)? } else { Vec::new() };
        let original = trace.layers[layer_index].path.clone();
        let mut path = original.clone();
        let mut local = Stats::default();
        for sub in &mut path.subpaths {
            check(cancel)?;
            let sampled = points(sub, options.tolerance, cancel)?;
            let mut chosen = None;
            if (options.circles || options.ellipses)
                && let Some((candidate, circle)) = recognize::ellipse(&sampled, sub, options)
                && candidate.knots.len() < sub.knots.len()
                && acceptable(&sampled, &candidate, options.tolerance, cancel)?
            {
                if circle {
                    local.circles += 1;
                } else {
                    local.ellipses += 1;
                }
                chosen = Some(candidate);
            }
            if chosen.is_none()
                && options.arcs
                && let Some(candidate) = recognize::arc(&sampled, sub, options)
                && candidate.knots.len() < sub.knots.len()
                && acceptable(&sampled, &candidate, options.tolerance, cancel)?
            {
                local.arcs += 1;
                chosen = Some(candidate);
            }
            if let Some(candidate) = chosen {
                *sub = candidate;
            } else {
                if options.collinear
                    && let Some(candidate) = recognize::straight_polygon(&sampled, sub, options.tolerance)
                    && candidate.knots.len() < sub.knots.len()
                    && acceptable(&sampled, &candidate, options.tolerance, cancel)?
                {
                    local.collinear_nodes += sub.knots.len() - candidate.knots.len();
                    *sub = candidate;
                    continue;
                }
                if options.collinear && sub.knots.len() <= 32 && sub.knots.iter().any(|k| k.in_ctrl != k.anchor || k.out_ctrl != k.anchor) {
                    let mut bez = BezPath::new();
                    if let Some(k) = sub.knots.first() {
                        bez.move_to((k.anchor.x, k.anchor.y));
                    }
                    for c in sub.segments() {
                        bez.curve_to((c[1].x, c[1].y), (c[2].x, c[2].y), (c[3].x, c[3].y));
                    }
                    if sub.closed {
                        bez.close_path();
                    }
                    let opts = kurbo::simplify::SimplifyOptions::default().opt_level(kurbo::simplify::SimplifyOptLevel::Optimize).angle_thresh(0.25);
                    let refit = kurbo::simplify::simplify_bezpath(&bez, options.tolerance.min(0.4), &opts);
                    let mut fitted = crate::adapter::bezpath_to_path(&refit)?;
                    if let Some(mut candidate) = fitted.subpaths.pop() {
                        candidate.op = sub.op;
                        if candidate.knots.len() < sub.knots.len() && acceptable(&sampled, &candidate, options.tolerance.min(0.4), cancel)? {
                            local.collinear_nodes += sub.knots.len() - candidate.knots.len();
                            *sub = candidate;
                            continue;
                        }
                    }
                }
                if options.collinear
                    && let Some(candidate) = recognize::polygon(&sampled, sub, (options.tolerance * 0.5).min(0.2))
                    && acceptable(&sampled, &candidate, options.tolerance, cancel)?
                {
                    local.collinear_nodes += sub.knots.len() - candidate.knots.len();
                    *sub = candidate;
                    continue;
                }
                let mut proposed = Stats::default();
                let candidate = linear(sub, options, &mut proposed);
                if candidate != *sub && acceptable(&sampled, &candidate, options.tolerance, cancel)? {
                    *sub = candidate;
                    local.axis_segments += proposed.axis_segments;
                    local.collinear_nodes += proposed.collinear_nodes;
                    local.right_angle_corners += proposed.right_angle_corners;
                }
            }
        }
        for axis in axes {
            check(cancel)?;
            if let Some(candidate) = symmetry::symmetrize(&path, axis, options.tolerance) {
                let mut valid = true;
                for (a, b) in original.subpaths.iter().zip(&candidate.subpaths) {
                    let sampled = points(a, options.tolerance, cancel)?;
                    if !acceptable(&sampled, b, options.tolerance, cancel)? {
                        valid = false;
                        break;
                    }
                }
                if valid {
                    path = candidate;
                    local.symmetry_axes += 1;
                }
            }
        }
        if path == original {
            continue;
        }
        if super::topology::crosses(&path, cancel)? {
            stats.rejected_paths += 1;
            continue;
        }
        trace.layers[layer_index].path = path;
        stats.axis_segments += local.axis_segments;
        stats.collinear_nodes += local.collinear_nodes;
        stats.right_angle_corners += local.right_angle_corners;
        stats.circles += local.circles;
        stats.ellipses += local.ellipses;
        stats.arcs += local.arcs;
        stats.symmetry_axes += local.symmetry_axes;
    }
    Ok(())
}

/// Geometry-only evaluation for callers that already traced a mask. Symmetry requires
/// the original image mask and is deliberately rejected at this narrower boundary.
pub fn clean_geometry(trace: &Trace, options: &Options, cancel: impl Fn() -> bool) -> Result<super::Output, Error> {
    options.validate()?;
    check(&cancel)?;
    let nodes: usize = trace.layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    if nodes > 16384 || trace.layers.len() > 64 {
        return Err(Error::BoundaryBudget);
    }
    if options.symmetry {
        return Err(Error::InvalidParams("symmetry requires the source mask"));
    }
    if trace
        .layers
        .iter()
        .flat_map(|l| &l.path.subpaths)
        .flat_map(|s| &s.knots)
        .any(|k| [k.anchor, k.in_ctrl, k.out_ctrl].iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1e6 || p.y.abs() > 1e6))
    {
        return Err(Error::InvalidParams("cleanup path coordinates must be finite and bounded"));
    }
    let mut out = trace.clone();
    let mut stats = Stats { scale: 1, nodes_before_geometry: nodes, ..Stats::default() };
    clean(&mut out, options, &[], 1, &mut stats, &cancel)?;
    out.nodes = out.layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    stats.nodes_after = out.nodes;
    Ok(super::Output { trace: out, stats })
}
