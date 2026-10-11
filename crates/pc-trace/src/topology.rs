//! Local flattened-segment topology guard, pending the shared V1-Q checker.
//! A fitted region that crosses itself is replaced by its exact lattice boundaries.
use kurbo::{CubicBez, ParamCurve, Point};
use photocraft_doc::Path;
pub fn crosses(path: &Path) -> bool {
    let mut lines: Vec<(Point, Point)> = Vec::new();
    for s in &path.subpaths {
        for c in s.segments() {
            let cubic = CubicBez::new((c[0].x, c[0].y), (c[1].x, c[1].y), (c[2].x, c[2].y), (c[3].x, c[3].y));
            let mut stack = vec![(cubic, 0u8)];
            while let Some((part, depth)) = stack.pop() {
                let chord = part.p3 - part.p0;
                let length = chord.hypot();
                let flatness = if length < 1e-12 {
                    part.p0.distance(part.p1).max(part.p0.distance(part.p2))
                } else {
                    (part.p1 - part.p0).cross(chord).abs().max((part.p2 - part.p0).cross(chord).abs()) / length
                };
                let along = |p: Point| (p - part.p0).dot(chord);
                let monotone = along(part.p1) >= 0. && along(part.p2) >= along(part.p1) && along(part.p2) <= length * length;
                if depth >= 20 || flatness <= 0.001 && monotone {
                    if part.p0 != part.p3 {
                        lines.push((part.p0, part.p3));
                    }
                } else {
                    stack.push((part.subsegment(0.5..1.), depth + 1));
                    stack.push((part.subsegment(0.0..0.5), depth + 1));
                }
            }
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
