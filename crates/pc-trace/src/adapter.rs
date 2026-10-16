//! Local kurbo → document path adapter. Joined rings preserve nonzero winding and counters.
use crate::Error;
use kurbo::{BezPath, PathEl};
use photocraft_doc::{Knot, Path, PathOp, Subpath};
use photocraft_geom::Point;
fn pt(p: kurbo::Point) -> Result<Point, Error> {
    if p.x.is_finite() && p.y.is_finite() { Ok(Point::new(p.x, p.y)) } else { Err(Error::InvalidParams("non-finite path")) }
}
fn finish(out: &mut Vec<Subpath>, sub: &mut Subpath) {
    if !sub.knots.is_empty() {
        if sub.closed
            && sub.knots.len() > 1
            && sub.knots[0].anchor == sub.knots[sub.knots.len() - 1].anchor
            && let Some(last) = sub.knots.pop()
        {
            sub.knots[0].in_ctrl = last.in_ctrl;
        }
        // The document model needs at least two knots for a segment. Preserve
        // a closed single-cubic loop by splitting it exactly at t=1/2.
        if sub.closed && sub.knots.len() == 1 {
            let k = sub.knots[0];
            if k.in_ctrl != k.anchor || k.out_ctrl != k.anchor {
                let mid = |a: Point, b: Point| Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                let a = mid(k.anchor, k.out_ctrl);
                let b = mid(k.out_ctrl, k.in_ctrl);
                let c = mid(k.in_ctrl, k.anchor);
                let ab = mid(a, b);
                let bc = mid(b, c);
                let p = mid(ab, bc);
                sub.knots[0].out_ctrl = a;
                sub.knots[0].in_ctrl = c;
                sub.knots.push(Knot { anchor: p, in_ctrl: ab, out_ctrl: bc, smooth: true });
            }
        }
        for k in &mut sub.knots {
            let a = (k.anchor.x - k.in_ctrl.x, k.anchor.y - k.in_ctrl.y);
            let b = (k.out_ctrl.x - k.anchor.x, k.out_ctrl.y - k.anchor.y);
            k.smooth = (a.0 * b.1 - a.1 * b.0).abs() < 1e-8 && a.0 * b.0 + a.1 * b.1 > 0.;
        }
        out.push(std::mem::take(sub));
    }
}
pub fn bezpath_to_path(bez: &BezPath) -> Result<Path, Error> {
    let mut out = Vec::new();
    let mut sub = Subpath::default();
    for el in bez.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut out, &mut sub);
                let p = pt(p)?;
                sub.knots.push(Knot::corner(p.x, p.y));
            }
            PathEl::LineTo(p) => {
                if sub.knots.is_empty() {
                    return Err(Error::InvalidParams("path segment before move"));
                }
                let p = pt(p)?;
                sub.knots.push(Knot::corner(p.x, p.y));
            }
            PathEl::QuadTo(c, p) => {
                let c = pt(c)?;
                let p = pt(p)?;
                let last = sub.knots.last_mut().ok_or(Error::InvalidParams("path segment before move"))?;
                last.out_ctrl = Point::new(last.anchor.x + (c.x - last.anchor.x) * 2. / 3., last.anchor.y + (c.y - last.anchor.y) * 2. / 3.);
                sub.knots.push(Knot { anchor: p, in_ctrl: Point::new(p.x + (c.x - p.x) * 2. / 3., p.y + (c.y - p.y) * 2. / 3.), out_ctrl: p, smooth: false });
            }
            PathEl::CurveTo(c1, c2, p) => {
                let c1 = pt(c1)?;
                let c2 = pt(c2)?;
                let p = pt(p)?;
                sub.knots.last_mut().ok_or(Error::InvalidParams("path segment before move"))?.out_ctrl = c1;
                sub.knots.push(Knot { anchor: p, in_ctrl: c2, out_ctrl: p, smooth: false });
            }
            PathEl::ClosePath => {
                sub.closed = true;
                finish(&mut out, &mut sub);
            }
        }
    }
    finish(&mut out, &mut sub);
    for (i, s) in out.iter_mut().enumerate() {
        s.op = if i == 0 { PathOp::Combine } else { PathOp::Join };
    }
    Ok(Path::new(out))
}
