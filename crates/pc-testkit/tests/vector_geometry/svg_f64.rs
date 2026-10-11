//! Original test-only SVG geometry parser. usvg owns style/scene normalization; preserve
//! the external tool's f64 path coordinates and approximate SVG arcs to 1e-6 device px.
use kurbo::{Arc, BezPath, Point, SvgArc, Vec2};
use svgtypes::{PathParser, PathSegment as S};

pub fn parse(data: &str, tolerance: f64) -> Result<BezPath, String> {
    let mut out = BezPath::new();
    let mut at = Point::ZERO;
    let mut start = at;
    let mut cubic = None;
    let mut quadratic = None;
    for segment in PathParser::from(data) {
        let segment = segment.map_err(|e| e.to_string())?;
        let point = |abs: bool, x: f64, y: f64| if abs { Point::new(x, y) } else { at + Vec2::new(x, y) };
        let mut next_cubic = None;
        let mut next_quadratic = None;
        match segment {
            S::MoveTo { abs, x, y } => {
                at = point(abs, x, y);
                start = at;
                out.move_to(at);
            }
            S::LineTo { abs, x, y } => {
                at = point(abs, x, y);
                out.line_to(at);
            }
            S::HorizontalLineTo { abs, x } => {
                at.x = if abs { x } else { at.x + x };
                out.line_to(at);
            }
            S::VerticalLineTo { abs, y } => {
                at.y = if abs { y } else { at.y + y };
                out.line_to(at);
            }
            S::CurveTo { abs, x1, y1, x2, y2, x, y } => {
                let a = point(abs, x1, y1);
                let b = point(abs, x2, y2);
                let end = point(abs, x, y);
                out.curve_to(a, b, end);
                at = end;
                next_cubic = Some(b);
            }
            S::SmoothCurveTo { abs, x2, y2, x, y } => {
                let a = cubic.map_or(at, |p: Point| at + (at - p));
                let b = point(abs, x2, y2);
                let end = point(abs, x, y);
                out.curve_to(a, b, end);
                at = end;
                next_cubic = Some(b);
            }
            S::Quadratic { abs, x1, y1, x, y } => {
                let a = point(abs, x1, y1);
                let end = point(abs, x, y);
                out.quad_to(a, end);
                at = end;
                next_quadratic = Some(a);
            }
            S::SmoothQuadratic { abs, x, y } => {
                let a = quadratic.map_or(at, |p: Point| at + (at - p));
                let end = point(abs, x, y);
                out.quad_to(a, end);
                at = end;
                next_quadratic = Some(a);
            }
            S::EllipticalArc { abs, rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
                let end = point(abs, x, y);
                let arc = SvgArc { from: at, to: end, radii: Vec2::new(rx, ry), x_rotation: x_axis_rotation.to_radians(), large_arc, sweep };
                if let Some(arc) = Arc::from_svg_arc(&arc) {
                    arc.to_cubic_beziers(tolerance, |a, b, c| out.curve_to(a, b, c));
                } else {
                    out.line_to(end);
                }
                at = end;
            }
            S::ClosePath { .. } => {
                out.close_path();
                at = start;
            }
        }
        cubic = next_cubic;
        quadratic = next_quadratic;
    }
    Ok(out)
}
