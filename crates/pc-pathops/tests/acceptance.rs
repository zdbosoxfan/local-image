use photocraft_doc::{FillRule, Knot, LineCap, LineJoin, Path, PathOp, ShapeStroke, StrokeAlign, Subpath};
use photocraft_geom::{Affine, Point, Rect};
use photocraft_pathops as ops;
use photocraft_vector as vector;
use proptest::prelude::*;

fn close(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} vs {b}, tolerance {t}");
}
fn square(x: f64, y: f64) -> Path {
    vector::shapes::rect(x, y, 100.0, 100.0)
}
#[test]
fn golden_overlapping_squares() {
    let (a, b) = (square(0.0, 0.0), square(50.0, 50.0));
    for (op, expected) in [(ops::BoolOp::Union, 17500.0), (ops::BoolOp::Intersect, 2500.0), (ops::BoolOp::Difference, 7500.0), (ops::BoolOp::Xor, 15000.0)] {
        let result = ops::boolean(&a, &b, op).unwrap();
        close(ops::area(&result).unwrap(), expected, 0.1);
        close(vector::path_coverage(&result, Rect::new(0, 0, 150, 150)).iter().map(|x| f64::from(*x)).sum(), expected, 0.1);
    }
}
#[test]
fn circle_difference_keeps_curves_and_holes() {
    let a = vector::shapes::ellipse(10.0, 10.0, 100.0, 100.0);
    let b = vector::shapes::ellipse(35.0, 35.0, 50.0, 50.0);
    let out = ops::boolean(&a, &b, ops::BoolOp::Difference).unwrap();
    assert_eq!(out.subpaths.len(), 2);
    assert!(out.subpaths.iter().flat_map(|s| &s.knots).any(|k| k.out_ctrl != k.anchor));
    assert!(!ops::contains(&out, Point::new(60.0, 60.0)).unwrap());
    close(ops::area(&out).unwrap(), std::f64::consts::PI * 1875.0, 2.0);
    close(vector::path_coverage(&out, Rect::new(0, 0, 128, 128)).iter().map(|x| f64::from(*x)).sum(), ops::area(&out).unwrap(), 2.0);
}
#[test]
fn coincident_reversed_and_tangent_curves_keep_set_semantics() {
    let a = vector::shapes::ellipse(10.0, 10.0, 100.0, 100.0);
    // Normalization refits curves at DEFAULT_PRECISION. Its area tolerance
    // scales with the contour length, rather than requiring exact refits.
    let tolerance = ops::to_geometry(&a).length() * ops::DEFAULT_PRECISION;
    for b in [a.clone(), ops::reverse(&a).unwrap()] {
        for op in [ops::BoolOp::Difference, ops::BoolOp::Xor] {
            assert!(ops::boolean(&a, &b, op).unwrap().is_empty());
        }
        for op in [ops::BoolOp::Union, ops::BoolOp::Intersect] {
            close(ops::area(&ops::boolean(&a, &b, op).unwrap()).unwrap(), ops::area(&a).unwrap(), tolerance);
        }
    }
    let tangent = vector::shapes::ellipse(110.0, 10.0, 100.0, 100.0);
    assert!(ops::boolean(&a, &tangent, ops::BoolOp::Intersect).unwrap().is_empty());
    close(ops::area(&ops::boolean(&a, &tangent, ops::BoolOp::Union).unwrap()).unwrap(), 2.0 * ops::area(&a).unwrap(), 2.0 * tolerance);
}
#[test]
fn twenty_random_pairs_agree_with_coverage() {
    let mut seed = 0x51a7u64;
    let mut number = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f64 / u32::MAX as f64
    };
    for _ in 0..20 {
        let mut shape = || vector::shapes::ellipse(10.0 + number() * 30.0, 10.0 + number() * 30.0, 50.0 + number() * 65.0, 50.0 + number() * 65.0);
        let (a, b) = (shape(), shape());
        for (operation, component_op) in [
            (ops::BoolOp::Union, PathOp::Combine),
            (ops::BoolOp::Difference, PathOp::Subtract),
            (ops::BoolOp::Intersect, PathOp::Intersect),
            (ops::BoolOp::Xor, PathOp::Exclude),
        ] {
            let output = ops::boolean(&a, &b, operation).unwrap();
            let mut oracle = a.clone();
            oracle.subpaths.extend(b.subpaths.iter().cloned().map(|s| s.with_op(component_op)));
            let rendered: f64 = vector::path_coverage(&oracle, Rect::new(0, 0, 160, 160)).iter().map(|x| f64::from(*x)).sum();
            close(ops::area(&output).unwrap(), rendered, rendered * 0.005 + 0.01);
        }
    }
}
#[test]
fn compound_evenodd_first_op_and_inversion() {
    let mut p = square(0.0, 0.0);
    p.subpaths[0].op = PathOp::Intersect;
    p.subpaths.extend(vector::shapes::rect(25.0, 25.0, 50.0, 50.0).subpaths.into_iter().map(|s| s.with_op(PathOp::Join)));
    p.fill_rule = FillRule::EvenOdd;
    let baked = ops::finish_compound(&p, None).unwrap();
    close(ops::area(&baked).unwrap(), 7500.0, 0.001);
    p.inverted = true;
    assert_eq!(ops::finish_compound(&p, None), Err(ops::PathOpsError::NeedsClip));
    let inverse = ops::finish_compound(&p, Some(ops::geom::Rect::new(0.0, 0.0, 120.0, 120.0))).unwrap();
    close(ops::area(&inverse).unwrap(), 6900.0, 0.001);
    assert!(ops::contains(&p, Point::new(50.0, 50.0)).unwrap());
}
#[test]
fn open_fill_ignores_unused_endpoint_handles() {
    let mut p = Path::new(vec![Subpath::polyline(&[(10.0, 10.0), (110.0, 10.0), (110.0, 110.0)])]);
    p.subpaths[0].knots[0].in_ctrl = Point::new(-90.0, 110.0);
    p.subpaths[0].knots[2].out_ctrl = Point::new(-90.0, 210.0);
    let original = p.clone();
    // The raw adapter and editing APIs still retain the unused handles.
    assert_eq!(ops::from_geometry(&ops::to_geometry(&p)), p);
    assert_eq!(ops::reverse(&ops::reverse(&p).unwrap()).unwrap(), p);
    let baked = ops::finish_compound(&p, None).unwrap();
    close(ops::area(&baked).unwrap(), 5000.0, 0.001);
    let rect = Rect::new(0, 0, 128, 128);
    let expected = vector::path_coverage(&p, rect);
    let actual = vector::path_coverage(&baked, rect);
    assert!(expected.iter().zip(actual).all(|(a, b)| (a - b).abs() <= 1.0 / 255.0));
    let clip = vector::shapes::rect(60.0, 10.0, 50.0, 50.0);
    close(ops::area(&ops::boolean(&p, &clip, ops::BoolOp::Intersect).unwrap()).unwrap(), 2500.0, 0.001);
    assert!(!ops::contains(&p, Point::new(10.0, 60.0)).unwrap());
    assert_eq!(p, original);
}
#[test]
fn adapter_handles_roundtrip_and_tight_bounds() {
    let p = Path::new(vec![Subpath {
        closed: false,
        op: PathOp::Combine,
        knots: vec![
            Knot::smooth(Point::new(0.0, 0.0), Point::new(-20.0, 0.0), Point::new(0.0, 100.0)),
            Knot::smooth(Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(120.0, 0.0)),
        ],
    }]);
    assert_eq!(ops::from_geometry(&ops::to_geometry(&p)), p);
    assert_eq!(p.bounds(), Some((0.0, 0.0, 100.0, 75.0)));
    let (_, _, t, point, distance) = ops::nearest(&p, Point::new(50.0, 80.0)).unwrap().unwrap();
    close(t, 0.5, 1e-5);
    close(point.y, 75.0, 1e-5);
    close(distance, 5.0, 1e-5);
    let split = ops::split_at(&p, 0, 0, 0.5).unwrap();
    close(split.iter().map(|p| ops::to_geometry(p).length()).sum(), ops::to_geometry(&p).length(), 1e-5);
    let reverse = ops::reverse(&ops::reverse(&p).unwrap()).unwrap();
    assert_eq!(reverse, p);
}
#[test]
fn builder_regions_and_open_cut_are_real_faces() {
    let shapes = [ops::Shape::new(square(0.0, 0.0), 0), ops::Shape::new(square(50.0, 50.0), 1)];
    let regions = ops::regions(&shapes).unwrap();
    assert_eq!(regions.len(), 3);
    let union = ops::merge_regions(&shapes, &regions.iter().collect::<Vec<_>>()).unwrap();
    close(ops::area(&union).unwrap(), 17500.0, 0.01);
    let shapes = [ops::Shape::new(square(0.0, 0.0), 0), ops::Shape::new(Path::new(vec![Subpath::polyline(&[(-10.0, 50.0), (110.0, 50.0)])]), 1)];
    let arr = ops::shape_builder(&shapes, true).unwrap();
    assert_eq!(arr.regions.len(), 2);
    assert!(!arr.lines.is_empty());
    assert!(!arr.edges.is_empty());
    close(arr.regions.iter().map(|r| ops::area(&r.path).unwrap()).sum(), 10000.0, 0.01);
}
#[test]
fn offsets_miter_bevel_round_and_inset() {
    let p = square(0.0, 0.0);
    for (join, area) in [(ops::Join::Miter, 14400.0), (ops::Join::Bevel, 14200.0), (ops::Join::Round, 14000.0 + 100.0 * std::f64::consts::PI)] {
        close(ops::area(&ops::offset_path(&p, 10.0, join, 4.0).unwrap()).unwrap(), area, 0.5);
    }
    close(ops::area(&ops::offset_path(&p, -10.0, ops::Join::Miter, 4.0).unwrap()).unwrap(), 6400.0, 0.01);
    assert!(ops::offset_path(&p, -60.0, ops::Join::Miter, 4.0).unwrap().is_empty());
}
#[test]
fn outlined_strokes_match_native_polygons_at_four_times_aa() {
    let paths =
        [square(20.0, 20.0), vector::shapes::ellipse(20.0, 20.0, 80.0, 60.0), Path::new(vec![Subpath::polyline(&[(15.0, 25.0), (70.0, 60.0), (110.0, 25.0)])])];
    for p in paths {
        for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                let stroke = ShapeStroke { width: 8.0, cap, join, miter_limit: 4.0, dashes: vec![2.0, 1.0], dash_offset: 0.25, ..Default::default() };
                let outline = ops::outline_stroke(&p, &stroke, 0.0025).unwrap().transform(&Affine::scale(4.0));
                let p = p.transform(&Affine::scale(4.0));
                let mut style = vector::stroke_style(&stroke);
                style.width *= 4.0;
                style.dashes.iter_mut().for_each(|d| *d *= 4.0);
                style.dash_offset *= 4.0;
                let native = vector::stroke_rasterizer(&p, &style, 0.01).render(Rect::new(0, 0, 520, 520));
                let result = vector::fill_rasterizer(&outline, 0.01).render(Rect::new(0, 0, 520, 520));
                let worst = native.iter().zip(&result).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
                assert!(worst <= 1.0 / 255.0, "{cap:?} {join:?}: {worst}");
            }
        }
    }
}
#[test]
fn aligned_strokes_and_empty_outline() {
    let p = square(20.0, 20.0);
    for (align, expected) in [(StrokeAlign::Inside, 3600.0), (StrokeAlign::Outside, 4400.0)] {
        let stroke = ShapeStroke { width: 10.0, align, ..Default::default() };
        close(ops::area(&ops::outline_stroke(&p, &stroke, 0.01).unwrap()).unwrap(), expected, 0.01);
        let inverted = Path { inverted: true, ..p.clone() };
        close(ops::area(&ops::outline_stroke(&inverted, &stroke, 0.01).unwrap()).unwrap(), 8000.0 - expected, 0.01);
    }
    assert!(ops::outline_stroke(&Path::default(), &ShapeStroke::default(), 0.01).unwrap().is_empty());
}
proptest! {
    #![proptest_config(ProptestConfig {cases:64,failure_persistence:None,..Default::default()})]
    #[test]
    fn degenerate_inputs_do_not_panic(points in prop::collection::vec((-10.0..10.0f64,-10.0..10.0f64),0..8),closed in any::<bool>()) {
        let path=Path::new(vec![Subpath {knots:points.iter().map(|&(x,y)|Knot::corner(x,y)).collect(),closed,op:PathOp::Combine}]);
        let _=ops::boolean(&path,&path,ops::BoolOp::Xor);
        let _=ops::offset_path(&path,1.0,ops::Join::Round,4.0);
        let _=ops::outline_stroke(&path,&ShapeStroke::default(),0.01);
        let _=ops::simplify(&path,0.5);
        let _=ops::shape_builder(&[ops::Shape::new(path,0)],true);
    }
    #[test]
    fn degenerate_curves_do_not_panic(knots in prop::collection::vec(
        ((-10.0..10.0f64, -10.0..10.0f64), (-10.0..10.0f64, -10.0..10.0f64), (-10.0..10.0f64, -10.0..10.0f64)), 0..5), closed in any::<bool>()) {
        let path = Path::new(vec![Subpath {
            knots: knots.iter().map(|&(a, i, o)| Knot::smooth(Point::new(a.0, a.1), Point::new(i.0, i.1), Point::new(o.0, o.1))).collect(),
            closed, op: PathOp::Combine,
        }]);
        let _ = ops::finish_compound(&path, None);
        let _ = ops::boolean(&path, &path, ops::BoolOp::Xor);
        let _ = ops::offset_path(&path, 1.0, ops::Join::Round, 4.0);
        let _ = ops::pathfinder(ops::PathfinderOp::Divide, &[ops::Shape::new(path, 0)]);
    }
}
#[test]
fn nonfinite_and_invalid_options_are_errors() {
    let p = Path::new(vec![Subpath::polyline(&[(f64::NAN, 0.0), (1.0, 1.0)])]);
    assert_eq!(ops::boolean(&p, &Path::default(), ops::BoolOp::Union), Err(ops::PathOpsError::NonFinite));
    assert!(ops::split_at(&square(0.0, 0.0), usize::MAX, 0, 0.5).is_err());
    assert!(ops::simplify(&square(0.0, 0.0), -1.0).is_err());
}
#[test]
fn kernel_failures_are_errors_instead_of_empty_geometry() {
    let good = ops::to_geometry(&square(0.0, 0.0));
    let bad = ops::to_geometry(&Path::new(vec![Subpath::polygon(&[(f64::NAN, 0.0), (1.0, 0.0), (1.0, 1.0)])]));
    let rule = ops::geom::FillRule::NonZero;
    assert_eq!(ops::kernel::try_unite_all(&[(&good, rule), (&bad, rule)]), Err(ops::PathOpsError::NonFinite));
    let shapes = [ops::kernel::Shape::new(good.clone(), rule, 0), ops::kernel::Shape::new(bad.clone(), rule, 1)];
    for op in [ops::PathfinderOp::Unite, ops::PathfinderOp::Divide, ops::PathfinderOp::Trim, ops::PathfinderOp::Crop, ops::PathfinderOp::Outline] {
        assert_eq!(ops::kernel::try_pathfinder(op, &shapes), Err(ops::PathOpsError::NonFinite));
    }
    assert_eq!(ops::kernel::try_regions(&shapes), Err(ops::PathOpsError::NonFinite));
    assert_eq!(ops::kernel::try_offset_path(&bad, 0.0, ops::Join::Miter, 4.0), Err(ops::PathOpsError::NonFinite));
    assert_eq!(ops::kernel::try_offset_path(&good, f64::NAN, ops::Join::Miter, 4.0), Err(ops::PathOpsError::InvalidOption));
    assert_eq!(ops::kernel::try_offset_path(&good, -100.0, ops::Join::Miter, 4.0).unwrap(), ops::geom::PathData::default());
}
#[test]
#[ignore = "release performance budget: coordinator machine, two 1000-segment paths"]
fn boolean_two_thousand_segments_benchmark() {
    let poly = |x: f64| {
        Path::new(vec![Subpath::polygon(
            &(0..1000)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 1000.0;
                    (x + 100.0 * a.cos(), 100.0 * a.sin())
                })
                .collect::<Vec<_>>(),
        )])
    };
    let (a, b) = (poly(0.0), poly(50.0));
    let start = std::time::Instant::now();
    let result = ops::boolean(&a, &b, ops::BoolOp::Union).unwrap();
    println!("1000 + 1000 segment union: {:?}; {} nodes", start.elapsed(), result.subpaths.iter().map(|s| s.knots.len()).sum::<usize>());
    assert!(ops::area(&result).unwrap() > 31400.0);
}

#[test]
fn aligned_curve_outlines_match_native_at_four_times_aa() {
    let original = vector::shapes::ellipse(15.0, 15.0, 70.0, 50.0);
    for align in [StrokeAlign::Inside, StrokeAlign::Outside] {
        for dashes in [vec![], vec![2.0, 1.0]] {
            let stroke = ShapeStroke { width: 6.0, align, cap: LineCap::Round, dashes, ..Default::default() };
            let out = ops::outline_stroke(&original, &stroke, 0.0025).unwrap().transform(&Affine::scale(4.0));
            let p = original.transform(&Affine::scale(4.0));
            let rect = Rect::new(0, 0, 400, 320);
            let mut style = vector::stroke_style(&stroke);
            style.width *= 8.0;
            style.dashes.iter_mut().for_each(|d| *d *= 4.0);
            style.dash_offset *= 4.0;
            let fill = vector::fill_rasterizer(&p, 0.01).render(rect);
            let native = vector::stroke_rasterizer(&p, &style, 0.01).render(rect);
            let result = vector::fill_rasterizer(&out, 0.01).render(rect);
            let worst = native
                .iter()
                .zip(fill)
                .zip(result)
                .map(|((s, f), r)| (s * if align == StrokeAlign::Inside { f } else { 1.0 - f } - r).abs())
                .fold(0.0, f32::max);
            assert!(worst <= 1.0 / 255.0, "{align:?} {:?}: {worst}", stroke.dashes);
        }
    }
}
#[test]
fn fit_samples_and_zero_length_curves() {
    let pts: Vec<_> = (0..=40)
        .map(|i| {
            let x = i as f64;
            Point::new(x, 10.0 * (x / 20.0).sin())
        })
        .collect();
    let fitted = ops::fit_path(&pts, 0.1).unwrap();
    assert!(fitted.subpaths[0].knots.len() < 10);
    for p in pts {
        assert!(ops::nearest(&fitted, p).unwrap().unwrap().4 < 0.15);
    }
    assert!(ops::fit_path(&[Point::new(0.0, 0.0); 5], 0.1).is_ok());
}

#[test]
fn closed_join_operands_keep_independent_psd_components() {
    let a = square(0.0, 0.0);
    let b = ops::reverse(&a).unwrap();
    let out = ops::join(&[a, b], 0.1).unwrap();
    close(ops::area(&out).unwrap(), 10000.0, 0.01);
}
