//! Bounded arrangement guard. A failure replaces ALL fitted shared chains.
//! Original Local Image code; no independent per-face geometry repair.
use super::{fit::Geometry, graph::BoundaryGraph};
use crate::Error;
use kurbo::{BezPath, ParamCurve, ParamCurveArea, PathEl, Point};

pub(super) fn valid(graph: &BoundaryGraph, fitted: &[Geometry], budget: usize, cancel: &dyn Fn() -> bool) -> Result<bool, Error> {
    let mut lines = Vec::new();
    for (id, (source, geom)) in graph.segments.iter().zip(fitted).enumerate() {
        if cancel() {
            return Err(Error::Cancelled);
        }
        if geom.is_empty() {
            return Ok(false);
        }
        if !source.is_ring() {
            let first = source.points[0];
            let last = source.points[source.points.len() - 1];
            if geom[0].start() != Point::new(f64::from(first.x), f64::from(first.y))
                || geom[geom.len() - 1].end() != Point::new(f64::from(last.x), f64::from(last.y))
            {
                return Ok(false);
            }
        } else {
            let area: f64 = source.points.windows(2).map(|p| f64::from(p[0].x) * f64::from(p[1].y) - f64::from(p[1].x) * f64::from(p[0].y)).sum();
            let fitted_area: f64 = geom.iter().map(|s| s.signed_area()).sum();
            if area * fitted_area <= 0. || geom[0].start() != geom[geom.len() - 1].end() {
                return Ok(false);
            }
        }
        for s in geom {
            let mut b = BezPath::new();
            b.move_to(s.start());
            b.push(s.as_path_el());
            if b.elements().iter().any(|e| !e.is_finite()) {
                return Ok(false);
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
    for (i, &(a, b, _)) in lines.iter().enumerate() {
        if i % 256 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        for &(c, d, _) in &lines[i + 1..] {
            if c.x.min(d.x) > a.x.max(b.x) {
                break;
            }
            probes = match probes.checked_sub(1) {
                Some(n) => n,
                None => return Ok(false),
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
                        return Ok(false);
                    }
                }
            } else {
                let t = ac.cross(cd) / det;
                let u = ac.cross(ab) / det;
                if (-1e-8..=1. + 1e-8).contains(&t) && (-1e-8..=1. + 1e-8).contains(&u) && ((1e-8..1. - 1e-8).contains(&t) || (1e-8..1. - 1e-8).contains(&u)) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
