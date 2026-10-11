//! Curve stroking shared by raster paint and editable stroke expansion.
//! Uses kurbo's pure Rust offset curves and SVG dash semantics.

use crate::StrokeStyle;
use kurbo::BezPath;
use photocraft_doc::{LineCap, LineJoin, Path};

fn bezpath(path: &Path) -> BezPath {
    let pt = |p: photocraft_geom::Point| kurbo::Point::new(p.x, p.y);
    let mut out = BezPath::new();
    for sub in &path.subpaths {
        let Some(first) = sub.knots.first() else { continue };
        out.move_to(pt(first.anchor));
        let segments = if sub.closed { sub.knots.len() } else { sub.knots.len().saturating_sub(1) };
        for i in 0..segments {
            let a = &sub.knots[i];
            let b = &sub.knots[(i + 1) % sub.knots.len()];
            if a.out_ctrl == a.anchor && b.in_ctrl == b.anchor {
                out.line_to(pt(b.anchor));
            } else {
                out.curve_to(pt(a.out_ctrl), pt(b.in_ctrl), pt(b.anchor));
            }
        }
        if sub.closed {
            out.close_path();
        }
    }
    out
}

/// Editable curve outline, in the same pixel units as `StrokeStyle`.
pub fn stroke_outline(path: &Path, style: &StrokeStyle, tolerance: f64) -> BezPath {
    if !style.width.is_finite() || style.width <= 0. {
        return BezPath::new();
    }
    let cap = match style.cap {
        LineCap::Butt => kurbo::Cap::Butt,
        LineCap::Round => kurbo::Cap::Round,
        LineCap::Square => kurbo::Cap::Square,
    };
    let join = match style.join {
        LineJoin::Miter => kurbo::Join::Miter,
        LineJoin::Round => kurbo::Join::Round,
        LineJoin::Bevel => kurbo::Join::Bevel,
    };
    // kurbo's dash iterator drops repeated zero-length on intervals. Retain
    // native dot semantics using the existing bounded polygon stroker.
    if style.dashes.iter().enumerate().any(|(i, d)| i % 2 == 0 && *d == 0.) {
        let lines = crate::flatten::flatten_path(path, tolerance * 0.1);
        let mut dots = BezPath::new();
        for poly in crate::stroke_polygons(&lines, style, tolerance * 0.1) {
            if let Some(&(x, y)) = poly.first() {
                dots.move_to((x, y));
                for &(x, y) in &poly[1..] {
                    dots.line_to((x, y));
                }
                dots.close_path();
            }
        }
        return dots;
    }
    let source = bezpath(path);
    let mut stroke = kurbo::Stroke::new(style.width).with_caps(cap).with_join(join).with_miter_limit(style.miter_limit);
    let dashed = style.dashes.iter().any(|d| *d > 0.);
    if dashed {
        stroke = stroke.with_dashes(style.dash_offset, style.dashes.iter());
    }
    kurbo::stroke(source.iter(), &stroke, &Default::default(), tolerance.min(style.width * 1e-5))
}
