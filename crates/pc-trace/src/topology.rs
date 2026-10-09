//! Local flattened-segment topology guard, pending the shared V1-Q checker.
//! A fitted region that crosses itself is replaced by its exact lattice boundaries.
use kurbo::{BezPath, PathEl, Point};
use photocraft_doc::Path;
pub fn crosses(path: &Path) -> bool {
    let mut lines: Vec<(Point, Point)> = Vec::new();
    for s in &path.subpaths {
        for c in s.segments() {
            let mut bez = BezPath::new();
            bez.move_to((c[0].x, c[0].y));
            bez.curve_to((c[1].x, c[1].y), (c[2].x, c[2].y), (c[3].x, c[3].y));
            let mut last = Point::new(c[0].x, c[0].y);
            kurbo::flatten(bez, 0.025, |el| {
                if let PathEl::LineTo(p) = el {
                    if p != last {
                        lines.push((last, p));
                    }
                    last = p;
                }
            });
        }
    }
    lines.sort_by(|a, b| a.0.x.min(a.1.x).total_cmp(&b.0.x.min(b.1.x)));
    for i in 0..lines.len() {
        let (a, b) = lines[i];
        for &(c, d) in &lines[i + 1..] {
            if c.x.min(d.x) > a.x.max(b.x) {
                break;
            }
            if c.y.min(d.y) > a.y.max(b.y) || c.y.max(d.y) < a.y.min(b.y) {
                continue;
            }
            let ab = b - a;
            let cd = d - c;
            let det = ab.cross(cd);
            if det.abs() < 1e-12 {
                continue;
            }
            let ac = c - a;
            let t = ac.cross(cd) / det;
            let u = ac.cross(ab) / det;
            if t > 1e-8 && t < 1. - 1e-8 && u > 1e-8 && u < 1. - 1e-8 {
                return true;
            }
        }
    }
    false
}
