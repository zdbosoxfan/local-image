//! Bounded arrangement guard. Repair only implicated shared chains, retaining both faces.
//! Original Local Image code; no independent per-face geometry repair.
use super::{fit::Geometry, graph::BoundaryGraph};
use crate::Error;
use kurbo::{BezPath, ParamCurve, ParamCurveArea, PathEl, Point};

#[cfg(test)]
pub(super) fn valid(graph: &BoundaryGraph, fitted: &[Geometry], budget: usize, cancel: &dyn Fn() -> bool) -> Result<bool, Error> {
    Ok(invalid(graph, fitted, budget, cancel)?.is_empty())
}
fn invalid(graph: &BoundaryGraph, fitted: &[Geometry], budget: usize, cancel: &dyn Fn() -> bool) -> Result<std::collections::BTreeSet<usize>, Error> {
    let mut bad = std::collections::BTreeSet::new();
    let mut lines = Vec::new();
    for (id, (source, geom)) in graph.segments.iter().zip(fitted).enumerate() {
        if cancel() {
            return Err(Error::Cancelled);
        }
        if geom.is_empty() {
            bad.insert(id);
            continue;
        }
        if !source.is_ring() {
            let first = source.points[0];
            let last = source.points[source.points.len() - 1];
            if geom[0].start() != Point::new(f64::from(first.x), f64::from(first.y))
                || geom[geom.len() - 1].end() != Point::new(f64::from(last.x), f64::from(last.y))
            {
                bad.insert(id);
                continue;
            }
        } else {
            let area: f64 = source.points.windows(2).map(|p| f64::from(p[0].x) * f64::from(p[1].y) - f64::from(p[1].x) * f64::from(p[0].y)).sum();
            let fitted_area: f64 = geom.iter().map(|s| s.signed_area()).sum();
            if area * fitted_area <= 0. || geom[0].start() != geom[geom.len() - 1].end() {
                bad.insert(id);
                continue;
            }
        }
        for s in geom {
            let mut b = BezPath::new();
            b.move_to(s.start());
            b.push(s.as_path_el());
            if b.elements().iter().any(|e| !e.is_finite()) {
                bad.insert(id);
                continue;
            }
            let mut prev = s.start();
            kurbo::flatten(b, 0.01, |el| {
                if let PathEl::LineTo(p) = el {
                    if p != prev {
                        lines.push((prev, p, id));
                    }
                    prev = p;
                }
            });
            if lines.len() > budget.saturating_mul(4) {
                return Err(Error::BoundaryBudget);
            }
        }
    }
    lines.sort_by(|a, b| a.0.x.min(a.1.x).total_cmp(&b.0.x.min(b.1.x)));
    let mut probes = budget.saturating_mul(64);
    for (i, &(a, b, left)) in lines.iter().enumerate() {
        if i % 256 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        for &(c, d, right) in &lines[i + 1..] {
            if c.x.min(d.x) > a.x.max(b.x) {
                break;
            }
            probes = match probes.checked_sub(1) {
                Some(n) => n,
                None => return Err(Error::BoundaryBudget),
            };
            if probes.is_multiple_of(1024) && cancel() {
                return Err(Error::Cancelled);
            }
            if c.y.min(d.y) > a.y.max(b.y) || c.y.max(d.y) < a.y.min(b.y) {
                continue;
            }
            let ab = b - a;
            let cd = d - c;
            let ac = c - a;
            let det = ab.cross(cd);
            if det.abs() < 1e-12 {
                if ab.cross(ac).abs() < 1e-10 {
                    let length = ab.hypot2();
                    let t = ac.dot(ab) / length;
                    let u = (d - a).dot(ab) / length;
                    if t.min(u).max(0.) + 1e-8 < t.max(u).min(1.) {
                        bad.insert(left);
                        bad.insert(right);
                    }
                }
            } else {
                let t = ac.cross(cd) / det;
                let u = ac.cross(ab) / det;
                if (-1e-8..=1. + 1e-8).contains(&t) && (-1e-8..=1. + 1e-8).contains(&u) && ((1e-8..1. - 1e-8).contains(&t) || (1e-8..1. - 1e-8).contains(&u)) {
                    bad.insert(left);
                    bad.insert(right);
                }
            }
        }
    }
    Ok(bad)
}

/// Exact fallback is shared by every adjacent face. A fresh whole-arrangement check
/// follows each repair, because a pixel chain may conflict with a remaining fit.
pub(super) fn repair(graph: &BoundaryGraph, fitted: &mut [Geometry], budget: usize, tolerance: f64, cancel: &dyn Fn() -> bool) -> Result<(), Error> {
    let mut repaired = vec![0u8; fitted.len()];
    loop {
        let bad = invalid(graph, fitted, budget, cancel)?;
        if bad.is_empty() {
            return Ok(());
        }
        let mut progress = false;
        for id in bad {
            match repaired[id] {
                0 => {
                    fitted[id] = super::fit::conservative(&graph.segments[id], tolerance, cancel)?;
                    repaired[id] = 1;
                    progress = true;
                }
                1 => {
                    fitted[id] = super::fit::pixel(&graph.segments[id], cancel)?;
                    repaired[id] = 2;
                    progress = true;
                }
                _ => {}
            }
        }
        if !progress {
            return Err(Error::Topology);
        }
    }
}
