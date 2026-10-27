//! Native path conversion using pc-text outlines. Quadratic elevation follows the
//! existing type_extra_cmds adapter; contours retain their glyph's winding.
use super::on_path::PaintedOutline;
use photocraft_doc::{Color, Knot, Path, PathOp, Subpath};
use photocraft_geom::Point;
use photocraft_text::render::PathEl;
pub fn paths(paints: &[PaintedOutline]) -> Vec<(Color, Path)> {
    let mut out: Vec<(Color, Path)> = Vec::new();
    for paint in paints {
        let index = out.last().filter(|(c, _)| *c == paint.color).map(|_| out.len() - 1).unwrap_or_else(|| {
            out.push((paint.color, Path::default()));
            out.len() - 1
        });
        out[index].1.subpaths.extend(glyph_subpaths(&paint.elements));
    }
    out
}

pub(in crate::vector_packages) fn glyph_subpaths(elements: &[PathEl]) -> Vec<Subpath> {
    let mut subpaths = Vec::new();
    let mut knots: Vec<Knot> = Vec::new();
    let mut first = true;
    let finish = |knots: &mut Vec<Knot>, first: &mut bool, subpaths: &mut Vec<Subpath>| {
        if knots.len() > 1
            && knots.first().zip(knots.last()).is_some_and(|(a, b)| a.anchor == b.anchor)
            && let Some(last) = knots.pop()
        {
            knots[0].in_ctrl = last.in_ctrl;
        }
        if knots.len() > 1 {
            subpaths.push(Subpath { closed: true, knots: std::mem::take(knots), op: if *first { PathOp::Combine } else { PathOp::Join } });
            *first = false;
        }
        knots.clear();
    };
    let point = |[x, y]: [f64; 2]| Point::new(x, y);
    for el in elements {
        match *el {
            PathEl::MoveTo([x, y]) => {
                finish(&mut knots, &mut first, &mut subpaths);
                knots.push(Knot::corner(x, y));
            }
            PathEl::LineTo([x, y]) => knots.push(Knot::corner(x, y)),
            PathEl::QuadTo(c, p) => {
                let Some(last) = knots.last_mut() else { continue };
                last.out_ctrl = Point::new(last.anchor.x + (c[0] - last.anchor.x) * 2. / 3., last.anchor.y + (c[1] - last.anchor.y) * 2. / 3.);
                let control = Point::new(p[0] + (c[0] - p[0]) * 2. / 3., p[1] + (c[1] - p[1]) * 2. / 3.);
                knots.push(Knot { anchor: point(p), in_ctrl: control, out_ctrl: point(p), smooth: false });
            }
            PathEl::CurveTo(a, b, p) => {
                if let Some(last) = knots.last_mut() {
                    last.out_ctrl = point(a);
                }
                knots.push(Knot { anchor: point(p), in_ctrl: point(b), out_ctrl: point(p), smooth: false });
            }
            PathEl::Close => finish(&mut knots, &mut first, &mut subpaths),
        }
    }
    finish(&mut knots, &mut first, &mut subpaths);
    subpaths
}
