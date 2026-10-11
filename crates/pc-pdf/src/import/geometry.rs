//! PDF cubics/quadratics to native knots. Adapted from VectorCraft geom/path.rs
//! (from_bezpath), pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
use super::{ImportError, Result};
use kurbo::{Affine, BezPath, PathEl, Point};
use photocraft_doc::{FillRule, Knot, Path, PathOp, Rect, Subpath};

pub(super) const MAX_COORD: f64 = 1_000_000.0;
pub(super) fn transform(m: Affine) -> Result<()> {
    if m.as_coeffs().iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) { Err(ImportError::Limit("transform")) } else { Ok(()) }
}
pub(super) fn native(p: &BezPath, m: Affine, rule: hayro_interpret::FillRule) -> Result<Path> {
    transform(m)?;
    let mut out = Path { fill_rule: if rule == hayro_interpret::FillRule::EvenOdd { FillRule::EvenOdd } else { FillRule::NonZero }, ..Default::default() };
    let mut sub: Option<Subpath> = None;
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Some(s) = sub.take() {
                    out.subpaths.push(s);
                }
                let p = m * p;
                sub = Some(Subpath {
                    closed: false,
                    knots: vec![Knot::corner(p.x, p.y)],
                    op: if out.subpaths.is_empty() { PathOp::Combine } else { PathOp::Join },
                });
            }
            PathEl::LineTo(p) => {
                if let Some(s) = &mut sub {
                    let p = m * p;
                    s.knots.push(Knot::corner(p.x, p.y));
                }
            }
            PathEl::QuadTo(c, p) => {
                if let Some(s) = &mut sub
                    && let Some(last) = s.knots.last_mut()
                {
                    let (c, p) = (m * c, m * p);
                    last.out_ctrl =
                        photocraft_geom::Point::new(last.anchor.x + (c.x - last.anchor.x) * 2.0 / 3.0, last.anchor.y + (c.y - last.anchor.y) * 2.0 / 3.0);
                    let mut k = Knot::corner(p.x, p.y);
                    k.in_ctrl = photocraft_geom::Point::new(p.x + (c.x - p.x) * 2.0 / 3.0, p.y + (c.y - p.y) * 2.0 / 3.0);
                    s.knots.push(k);
                }
            }
            PathEl::CurveTo(a, b, p) => {
                if let Some(s) = &mut sub
                    && let Some(last) = s.knots.last_mut()
                {
                    let (a, b, p) = (m * a, m * b, m * p);
                    last.out_ctrl = photocraft_geom::Point::new(a.x, a.y);
                    let mut k = Knot::corner(p.x, p.y);
                    k.in_ctrl = photocraft_geom::Point::new(b.x, b.y);
                    s.knots.push(k);
                }
            }
            PathEl::ClosePath => {
                if let Some(s) = &mut sub {
                    s.closed = true;
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
    validate(&out)?;
    Ok(out)
}
pub(super) fn validate(path: &Path) -> Result<()> {
    if path
        .subpaths
        .iter()
        .flat_map(|s| &s.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > MAX_COORD)
    {
        Err(ImportError::Limit("coordinate"))
    } else {
        Ok(())
    }
}
pub(super) fn rect(points: &[Point]) -> Result<Rect> {
    if points.is_empty() {
        return Ok(Rect::EMPTY);
    }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > MAX_COORD) {
        return Err(ImportError::Limit("render coordinate"));
    }
    let b = points
        .iter()
        .fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, p| (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y)));
    Ok(Rect::new(b.0.floor() as i32, b.1.floor() as i32, b.2.ceil() as i32, b.3.ceil() as i32))
}
pub(super) fn bands(rect: Rect) -> impl Iterator<Item = Rect> {
    (rect.y0..rect.y1).step_by(32).map(move |y| Rect::new(rect.x0, y, rect.x1, (y + 32).min(rect.y1)))
}
