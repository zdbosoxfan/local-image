//! Independent raster reference and metric regression tests, no network or external programs.
use photocraft_doc::{FillRule, Knot, Path, PathOp, Subpath};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_testkit::vector::*;
use photocraft_vector as vector;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn cubic_green_area_and_native_coverage_match_resvg_reference() -> TestResult {
    // Original Local Image test input, CC0. Same cubic circle approximation in SVG and pc-vector.
    let p = vector::shapes::ellipse(16.0, 16.0, 64.0, 64.0);
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><path d="M80 48 C80 65.673112 65.673112 80 48 80 C30.326888 80 16 65.673112 16 48 C16 30.326888 30.326888 16 48 16 C65.673112 16 80 30.326888 80 48 Z"/></svg>"#;
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(768, 768).ok_or("pixmap allocation failed")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(8.0, 8.0), &mut pixmap.as_mut());
    let reference: Vec<f32> = pixmap.pixels().iter().map(|p| f32::from(p.alpha()) / 255.0).collect();
    let native = vector::fill_rasterizer(&p.transform(&Affine::scale(8.0)), 0.001).render(Rect::new(0, 0, 768, 768));
    assert!(iou(&reference, &native, 0.5)? > 0.999);
    assert!((area(&p, FillRule::NonZero)? - native.iter().map(|&v| f64::from(v) / 64.0).sum::<f64>()).abs() < 0.1);
    Ok(())
}

#[test]
fn self_intersection_parameters_locate_a_cubic_crossing() {
    let p = Path::new(vec![Subpath {
        closed: false,
        op: PathOp::Combine,
        knots: vec![
            Knot::smooth(Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(10.0, 20.0)),
            Knot::smooth(Point::new(20.0, 0.0), Point::new(10.0, 20.0), Point::new(20.0, 0.0)),
            Knot::corner(0.0, 10.0),
            Knot::corner(20.0, 10.0),
        ],
    }]);
    let crossings = self_intersections(&p);
    assert!(!crossings.is_empty());
    use kurbo::ParamCurve;
    let segs = p.subpaths[0].segments();
    for (a, b, t, u) in crossings {
        let curve = |i: usize| {
            let pts = segs[i].map(|p| kurbo::Point::new(p.x, p.y));
            kurbo::CubicBez::new(pts[0], pts[1], pts[2], pts[3])
        };
        assert!(curve(a).eval(t).distance(curve(b).eval(u)) < 0.05);
    }
}
