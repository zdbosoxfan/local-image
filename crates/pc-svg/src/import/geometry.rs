//! usvg absolute segments to the native cubic-knot model. SVG subpaths share one fill rule.
//! Adapted from VectorCraft crates/svg/src/import.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{ImportError, Result};
use photocraft_doc::{Affine, FillRule, Knot, Path, PathOp, Subpath};
use photocraft_geom::Point;
use usvg::tiny_skia_path::PathSegment;

pub(super) const MAX_COORD: f64 = 1_000_000.0;
pub(super) fn affine(t: usvg::Transform) -> Affine {
    Affine { m: [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from) }
}
pub(super) fn bounded(a: Affine) -> Result<Affine> {
    if a.m.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) { Err(ImportError::Limit("coordinate/transform")) } else { Ok(a) }
}
pub(super) fn path(data: &usvg::tiny_skia_path::Path, transform: Affine, rule: usvg::FillRule) -> Result<Path> {
    bounded(transform)?;
    let mut out = Path { fill_rule: if rule == usvg::FillRule::EvenOdd { FillRule::EvenOdd } else { FillRule::NonZero }, ..Default::default() };
    let mut sub: Option<Subpath> = None;
    let point = |p: usvg::tiny_skia_path::Point| transform.apply(Point::new(f64::from(p.x), f64::from(p.y)));
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if let Some(s) = sub.take() {
                    out.subpaths.push(s);
                }
                let p = point(p);
                sub = Some(Subpath {
                    closed: false,
                    knots: vec![Knot::corner(p.x, p.y)],
                    op: if out.subpaths.is_empty() { PathOp::Combine } else { PathOp::Join },
                });
            }
            PathSegment::LineTo(p) => {
                let p = point(p);
                if let Some(s) = &mut sub {
                    s.knots.push(Knot::corner(p.x, p.y));
                }
            }
            PathSegment::QuadTo(c, p) => {
                if let Some(s) = &mut sub
                    && let Some(last) = s.knots.last_mut()
                {
                    let c = point(c);
                    let p = point(p);
                    last.out_ctrl = Point::new(last.anchor.x + (c.x - last.anchor.x) * 2.0 / 3.0, last.anchor.y + (c.y - last.anchor.y) * 2.0 / 3.0);
                    let mut next = Knot::corner(p.x, p.y);
                    next.in_ctrl = Point::new(p.x + (c.x - p.x) * 2.0 / 3.0, p.y + (c.y - p.y) * 2.0 / 3.0);
                    s.knots.push(next);
                }
            }
            PathSegment::CubicTo(a, b, p) => {
                if let Some(s) = &mut sub
                    && let Some(last) = s.knots.last_mut()
                {
                    last.out_ctrl = point(a);
                    let p = point(p);
                    let mut next = Knot::corner(p.x, p.y);
                    next.in_ctrl = point(b);
                    s.knots.push(next);
                }
            }
            PathSegment::Close => {
                if let Some(s) = &mut sub {
                    s.closed = true;
                    // An explicit last endpoint equal to the start carries the closing
                    // cubic's incoming handle; don't lose it or keep a zero-length segment.
                    if s.knots.len() > 1
                        && s.knots.first().map(|k| k.anchor) == s.knots.last().map(|k| k.anchor)
                        && let Some(last) = s.knots.pop()
                        && let Some(first) = s.knots.first_mut()
                    {
                        first.in_ctrl = last.in_ctrl;
                    }
                }
            }
        }
    }
    if let Some(s) = sub {
        out.subpaths.push(s);
    }
    if out
        .subpaths
        .iter()
        .flat_map(|s| &s.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > MAX_COORD)
    {
        return Err(ImportError::Limit("coordinate"));
    }
    Ok(out)
}
