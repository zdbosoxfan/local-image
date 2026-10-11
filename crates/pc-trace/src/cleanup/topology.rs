//! Extrema-split flattened curve guard. Splitting at both coordinate extrema
//! prevents a small reversal/cusp from being flattened into one straight chord.
use super::check;
use crate::Error;
use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveExtrema, PathEl, Point};
use photocraft_doc::Path;
pub(super) fn crosses(path: &Path, cancel: &impl Fn() -> bool) -> Result<bool, Error> {
    let mut lines: Vec<(Point, Point)> = Vec::new();
    for s in &path.subpaths {
        for c in s.segments() {
            check(cancel)?;
            let c = CubicBez::new((c[0].x, c[0].y), (c[1].x, c[1].y), (c[2].x, c[2].y), (c[3].x, c[3].y));
            for range in c.extrema_ranges() {
                let part = c.subsegment(range);
                let mut bez = BezPath::new();
                bez.move_to(part.p0);
                bez.curve_to(part.p1, part.p2, part.p3);
                let mut last = part.p0;
                kurbo::flatten(bez, 0.002, |el| {
                    if let PathEl::LineTo(p) = el {
                        if last != p && lines.len() <= 65536 {
                            lines.push((last, p));
                        }
                        last = p;
                    }
                });
                if lines.len() > 65536 {
                    return Err(Error::BoundaryBudget);
                }
            }
        }
    }
    lines.sort_by(|a, b| a.0.x.min(a.1.x).total_cmp(&b.0.x.min(b.1.x)));
    let mut comparisons = 0usize;
    for (i, &(a, b)) in lines.iter().enumerate() {
        if i % 256 == 0 {
            check(cancel)?;
        }
        for &(c, d) in &lines[i + 1..] {
            if c.x.min(d.x) > a.x.max(b.x) {
                break;
            }
            comparisons += 1;
            if comparisons > 8_000_000 {
                return Err(Error::BoundaryBudget);
            }
            if c.y.min(d.y) > a.y.max(b.y) || c.y.max(d.y) < a.y.min(b.y) {
                continue;
            }
            let (ab, cd) = (b - a, d - c);
            let det = ab.cross(cd);
            if det.abs() < 1e-12 {
                continue;
            }
            let ac = c - a;
            let (t, u) = (ac.cross(cd) / det, ac.cross(ab) / det);
            if t > 1e-7 && t < 1. - 1e-7 && u > 1e-7 && u < 1. - 1e-7 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
