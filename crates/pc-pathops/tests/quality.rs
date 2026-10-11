//! Geometry quality gates from vector-quality-plan A.1–A.4. Deterministic and offline.
use photocraft_doc::{FillRule, Knot, Path, PathOp, ShapeStroke, Subpath};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_pathops as ops;
use photocraft_testkit::vector as metrics;
use photocraft_vector as vector;
use proptest::prelude::*;
mod common;

fn area(p: &Path) -> f64 {
    metrics::area(p, FillRule::NonZero).unwrap()
}
fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-6 * a.max(b).max(1.0), "area {a} != {b}");
}
fn oracle(a: &Path, b: &Path, op: PathOp) -> Path {
    let mut p = a.clone();
    p.subpaths.extend(b.subpaths.iter().enumerate().map(|(i, s)| s.clone().with_op(if i == 0 { op } else { PathOp::Join })));
    p
}

#[test]
fn coverage_two_hundred_pairs_five_operations_at_four_times_aa() {
    let mut seed = 0x93ea71u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f64 / u32::MAX as f64
    };
    let rect = Rect::new(0, 0, 256, 256);
    let mut maximum = 0.0f32;
    for i in 0..200 {
        let mut shape = || {
            let (x, y, w, h) = (4.0 + random() * 18.0, 4.0 + random() * 18.0, 12.0 + random() * 24.0, 12.0 + random() * 24.0);
            if i % 3 == 0 { vector::shapes::rect(x, y, w, h) } else { vector::shapes::ellipse(x, y, w, h) }
        };
        let (a, b) = (shape(), shape());
        let mut outputs = Vec::new();
        for (op, component) in [
            (ops::BoolOp::Union, PathOp::Combine),
            (ops::BoolOp::Difference, PathOp::Subtract),
            (ops::BoolOp::Intersect, PathOp::Intersect),
            (ops::BoolOp::Xor, PathOp::Exclude),
        ] {
            let result = ops::boolean(&a, &b, op).unwrap();
            assert!(metrics::is_finite(&result));
            assert!(metrics::self_intersections(&result).is_empty(), "pair {i}, {op:?}");
            let expected = vector::fill_rasterizer(&oracle(&a, &b, component).transform(&Affine::scale(4.0)), 0.001).render(rect);
            let actual = vector::fill_rasterizer(&result.transform(&Affine::scale(4.0)), 0.001).render(rect);
            let diff = metrics::max_abs_diff(&expected, &actual).unwrap();
            maximum = maximum.max(diff);
            assert!(diff <= 2.0 / 255.0, "pair {i}, {op:?}: coverage difference {diff}");
            outputs.push(result);
        }
        assert_close(area(&outputs[0]) + area(&outputs[2]), area(&a) + area(&b));
        assert_close(area(&outputs[3]), area(&outputs[0]) - area(&outputs[2]));
        assert_close(area(&outputs[1]) + area(&outputs[2]), area(&a));
        let faces = ops::pathfinder(ops::PathfinderOp::Divide, &[ops::Shape::new(a.clone(), 0), ops::Shape::new(b.clone(), 1)]).unwrap();
        assert_close(faces.iter().map(|s| area(&s.path)).sum(), area(&outputs[0]));
        let mut reconstructed = Path::default();
        for f in faces {
            reconstructed.subpaths.extend(f.path.subpaths.into_iter().enumerate().map(|(j, s)| s.with_op(if j == 0 { PathOp::Combine } else { PathOp::Join })));
        }
        let expected = vector::fill_rasterizer(&oracle(&a, &b, PathOp::Combine).transform(&Affine::scale(4.0)), 0.001).render(rect);
        let actual = vector::fill_rasterizer(&reconstructed.transform(&Affine::scale(4.0)), 0.001).render(rect);
        let diff = metrics::max_abs_diff(&expected, &actual).unwrap();
        maximum = maximum.max(diff);
        assert!(diff <= 2.0 / 255.0, "pair {i}, divide: {diff}");
    }
    println!("200 pairs × 5 operations; maximum 4× AA coverage error {maximum}");
}

#[test]
fn round_offsets_and_strokes_agree_with_eight_times_edt() {
    let scale = 8.0;
    let rect = Rect::new(0, 0, 1024, 1024);
    let p = vector::shapes::rect(24.0, 24.0, 80.0, 80.0);
    let mask = vector::fill_rasterizer(&p.transform(&Affine::scale(scale)), 0.001).render(rect);
    let mut previous_area = 0.0;
    let mut worst_boundary = 0.0f64;
    for delta in [-10.0f64, -4.0, 4.0, 10.0] {
        let offset = ops::offset_path(&p, delta, ops::Join::Round, 4.0).unwrap();
        let measured_area = area(&offset);
        assert!(measured_area > previous_area);
        previous_area = measured_area;
        use kurbo::{ParamCurve, ParamCurveNearest};
        let source = ops::to_geometry(&p).to_bezpath();
        for segment in ops::to_geometry(&offset).to_bezpath().segments() {
            for i in 0..=32 {
                let point = segment.eval(i as f64 / 32.0);
                let distance = source.segments().map(|s| s.nearest(point, 1e-8).distance_sq).fold(f64::INFINITY, f64::min).sqrt();
                worst_boundary = worst_boundary.max((distance - delta.abs()).abs());
            }
        }
        assert!(worst_boundary < 0.05, "round boundary error {worst_boundary}");
        let expected = metrics::offset_mask(&mask, 1024, 1024, delta, scale).unwrap();
        let actual = vector::fill_rasterizer(&offset.transform(&Affine::scale(scale)), 0.001).render(rect);
        let iou = metrics::iou(&expected, &actual, 0.5).unwrap();
        println!("round offset {delta}: EDT IoU {iou}");
        assert!(iou > 0.999, "{delta}: {iou}");
    }
    println!("round offset maximum sampled boundary error: {worst_boundary} px");
    // A one-pixel-wide centerline grid: its round dilation independently checks round caps.
    let mut line = vec![0.0; 1024 * 1024];
    for x in 256..=768 {
        line[512 * 1024 + x] = 1.0;
    }
    let path = Path::new(vec![Subpath::polyline(&[(32.0625, 64.0625), (96.0625, 64.0625)])]);
    let stroke = ShapeStroke { width: 20.0, cap: photocraft_doc::LineCap::Round, ..Default::default() };
    let out = ops::outline_stroke(&path, &stroke, 0.001).unwrap();
    let actual = vector::fill_rasterizer(&out.transform(&Affine::scale(scale)), 0.001).render(rect);
    let expected = metrics::offset_mask(&line, 1024, 1024, 10.0 - 0.0625, scale).unwrap();
    let iou = metrics::iou(&expected, &actual, 0.5).unwrap();
    println!("round stroke: EDT IoU {iou}");
    assert!(iou > 0.999, "stroke EDT IoU {iou}");
}

#[test]
fn tricky_corpus_keeps_tiny_large_tangent_and_coincident_regions() {
    for scale in [1e-4, 1.0, 1e6] {
        let a = vector::shapes::rect(0.0, 0.0, 100.0 * scale, 100.0 * scale);
        let b = vector::shapes::rect(50.0 * scale, 50.0 * scale, 100.0 * scale, 100.0 * scale);
        let out = ops::boolean(&a, &b, ops::BoolOp::Union).unwrap();
        assert!((area(&out) / (17500.0 * scale * scale) - 1.0).abs() < 1e-6, "scale {scale}");
    }
    let a = vector::shapes::rect(1e6, 1e6, 0.0001, 20.0);
    assert!((area(&ops::boolean(&a, &a, ops::BoolOp::Union).unwrap()) / 0.002 - 1.0).abs() < 0.001);
    for delta in [-0.00002, 0.0, 0.00002] {
        let out = ops::offset_path(&a, delta, ops::Join::Miter, 4.0).unwrap();
        let expected = (0.0001 + 2.0 * delta) * (20.0 + 2.0 * delta);
        assert!((area(&out) / expected - 1.0).abs() < 0.001, "thin offset {delta}");
    }
    let line = Path::new(vec![Subpath::polyline(&[(1e6, 1e6), (1e6 + 20.0, 1e6)])]);
    let outline = ops::from_geometry(&ops::kernel::outline_stroke(&ops::to_geometry(&line), 0.0001, ops::Cap::Butt, ops::Join::Miter, 4.0));
    assert!((area(&outline) / 0.002 - 1.0).abs() < 0.001, "thin kernel stroke");
    for b in [a.clone(), ops::reverse(&a).unwrap()] {
        assert!(ops::boolean(&a, &b, ops::BoolOp::Xor).unwrap().is_empty());
    }
    let a = vector::shapes::ellipse(0.0, 0.0, 40.0, 40.0);
    let b = vector::shapes::ellipse(40.0, 0.0, 40.0, 40.0);
    assert!(ops::boolean(&a, &b, ops::BoolOp::Intersect).unwrap().is_empty());
}

#[test]
fn simplify_reduces_nodes_and_keeps_corners_within_tolerance() {
    let pts: Vec<_> = (0..=100)
        .map(|i| {
            let t = i as f64 / 100.0 * std::f64::consts::PI;
            (20.0 + 15.0 * t.cos(), 20.0 + 15.0 * t.sin())
        })
        .collect();
    let p = Path::new(vec![Subpath::polyline(&pts)]);
    let out = ops::simplify(&p, 0.05).unwrap();
    assert!(metrics::node_count(&out) < metrics::node_count(&p));
    assert!(metrics::hausdorff(&p, &out, 32).unwrap() <= 0.05);
    let mut previous = metrics::node_count(&out);
    for tolerance in [0.1, 0.25, 0.5, 1.0, 2.0] {
        let result = ops::simplify(&p, tolerance).unwrap();
        let count = metrics::node_count(&result);
        assert!(count <= previous, "node count increased at tolerance {tolerance}");
        assert!(metrics::hausdorff(&p, &result, 32).unwrap() <= tolerance);
        previous = count;
    }
    let square = vector::shapes::rect(0.0, 0.0, 40.0, 40.0);
    let simplified = ops::simplify(&square, 0.1).unwrap();
    assert!(metrics::hausdorff(&square, &simplified, 32).unwrap() < 1e-6);
}

proptest! {
    #![proptest_config(common::config(256))]
    #[test]
    fn finite_cubic_and_degenerate_boundary_inputs_never_panic(
        knots in prop::collection::vec((-10.0..10.0f64, -10.0..10.0f64, -10.0..10.0f64, -10.0..10.0f64), 0..6),
        scale in prop_oneof![Just(1e-4), Just(1.0), Just(1e6)], closed in any::<bool>()
    ) {
        let p = Path::new(vec![Subpath { closed, op: PathOp::Combine, knots: knots.iter().map(|&(x,y,hx,hy)| {
            Knot::smooth(Point::new(x*scale,y*scale), Point::new(hx*scale,hy*scale), Point::new(hx*scale,hy*scale))
        }).collect() }]);
        for op in [ops::BoolOp::Union, ops::BoolOp::Intersect, ops::BoolOp::Difference, ops::BoolOp::Xor] {
            if let Ok(out) = ops::boolean(&p, &p, op) { prop_assert!(metrics::is_finite(&out)); }
        }
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let bad = Path::new(vec![Subpath::polyline(&[(invalid, 0.0), (0.0, 0.0)])]);
            prop_assert_eq!(ops::boolean(&bad, &p, ops::BoolOp::Union), Err(ops::PathOpsError::NonFinite));
        }
    }
}

#[test]
fn reduced_degenerate_contour_extraction_returns_checked_errors() {
    let path = Path::new(vec![Subpath {
        closed: true,
        op: PathOp::Combine,
        knots: vec![
            Knot::smooth(Point::new(0., -8.074030517693085), Point::new(0., -8.144552439366418), Point::new(0., -8.144552439366418)),
            Knot::smooth(
                Point::new(0., -5.618997047213918),
                Point::new(-2.6579120785643333, -4.077406646208811),
                Point::new(-2.6579120785643333, -4.077406646208811),
            ),
        ],
    }]);
    assert_eq!(ops::boolean(&path, &path, ops::BoolOp::Union), Err(ops::PathOpsError::Degenerate));
    for scale in [1e-4, 1., 1e6] {
        let p = path.transform(&Affine::scale(scale));
        let shapes = [ops::Shape::new(p.clone(), 0), ops::Shape::new(p.clone(), 1)];
        for op in [
            ops::PathfinderOp::Unite,
            ops::PathfinderOp::Divide,
            ops::PathfinderOp::Trim,
            ops::PathfinderOp::Crop,
            ops::PathfinderOp::Merge,
            ops::PathfinderOp::MinusFront,
            ops::PathfinderOp::MinusBack,
            ops::PathfinderOp::Intersect,
            ops::PathfinderOp::Exclude,
        ] {
            match ops::pathfinder(op, &shapes) {
                Ok(out) => assert!(out.iter().all(|s| metrics::is_finite(&s.path))),
                Err(e) => assert_eq!(e, ops::PathOpsError::Degenerate),
            }
        }
    }
}

#[test]
fn simplify_excursion_before_a_flat_tail_keeps_tolerance_and_endpoints() {
    let ys = [83.53770334567852, 89.82001844736132, 96.94535701753479, 110.49214914185464, 100., 100., 100.];
    let knots = ys
        .iter()
        .enumerate()
        .map(|(i, &y)| {
            let x = i as f64 * 10.;
            let (hx, dy) = if i == 0 {
                (10. / 6., (ys[1] - y) / 6.)
            } else if i + 1 == ys.len() {
                (10. / 6., (y - ys[i - 1]) / 6.)
            } else {
                (10. / 3., (ys[i + 1] - ys[i - 1]) / 6.)
            };
            Knot::smooth(Point::new(x, y), Point::new(x - hx, y - dy), Point::new(x + hx, y + dy))
        })
        .collect();
    let source = Path::new(vec![Subpath { knots, closed: false, op: PathOp::Combine }]);
    let tolerance = 2.968424593080055;
    for (index, simplify) in [ops::simplify, ops::simplify_interactive].into_iter().enumerate() {
        let actual = simplify(&source, tolerance).unwrap();
        let error = metrics::hausdorff(&source, &actual, 64).unwrap();
        assert!(error <= tolerance, "refitter {index} error {error} > {tolerance}");
        assert!(metrics::node_count(&actual) <= metrics::node_count(&source));
        assert_eq!(actual.subpaths[0].knots[0].anchor, source.subpaths[0].knots[0].anchor);
        assert_eq!(actual.subpaths[0].knots.last().unwrap().anchor, source.subpaths[0].knots.last().unwrap().anchor);
    }
}

#[test]
fn sampled_single_cubic_fit_keeps_error_and_endpoint_tangents() {
    use kurbo::{CubicBez, ParamCurve};
    for c in [CubicBez::new((0., 0.), (10., 0.), (20., 0.), (30., 0.)), CubicBez::new((0., 0.), (10., 4.), (20., 4.), (30., 0.))] {
        let points: Vec<_> = (0..=64)
            .map(|i| {
                let p = c.eval(i as f64 / 64.);
                Point::new(p.x, p.y)
            })
            .collect();
        let path = ops::fit_path(&points, 0.05).unwrap();
        let sp = &path.subpaths[0];
        assert_eq!(sp.knots.len(), 2);
        assert_eq!(sp.knots[0].anchor, points[0]);
        assert_eq!(sp.knots[1].anchor, points[64]);
        let delta = |a: Point, b: Point| kurbo::Vec2::new(a.x - b.x, a.y - b.y);
        for (tangent, handle) in [
            (delta(points[1], points[0]), delta(sp.knots[0].out_ctrl, sp.knots[0].anchor)),
            (delta(points[64], points[63]), delta(sp.knots[1].anchor, sp.knots[1].in_ctrl)),
        ] {
            assert!(tangent.cross(handle).abs() <= 1e-9);
            assert!(tangent.dot(handle) > 0.);
        }
        for p in points {
            assert!(ops::nearest(&path, p).unwrap().unwrap().4 <= 0.05);
        }
    }
}
