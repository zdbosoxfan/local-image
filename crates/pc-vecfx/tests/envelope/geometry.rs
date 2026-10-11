use kurbo::{Affine, ParamCurve, Point, Rect};
use photocraft_doc::{Knot, Path, Subpath, vector_design::live::EnvelopeSpec};
use photocraft_geom::Point as DocPoint;
use photocraft_pathops::to_geometry;
use photocraft_vecfx::envelope::*;
fn curve() -> Path {
    Path::new(vec![Subpath {
        closed: false,
        knots: vec![
            Knot { anchor: DocPoint::new(10.0, 20.0), in_ctrl: DocPoint::new(10.0, 20.0), out_ctrl: DocPoint::new(80.0, -20.0), smooth: false },
            Knot { anchor: DocPoint::new(110.0, 120.0), in_ctrl: DocPoint::new(0.0, 150.0), out_ctrl: DocPoint::new(110.0, 120.0), smooth: true },
        ],
        ..Default::default()
    }])
}
fn square() -> Path {
    Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)])])
}
fn hausdorff(a: &Path, b: &Path) -> f64 {
    let samples = |p: &Path| {
        to_geometry(p)
            .subpaths
            .iter()
            .flat_map(|s| (0..s.segment_count()).flat_map(move |i| (0..=256).map(move |j| s.segment(i).eval(j as f64 / 256.0))))
            .collect::<Vec<_>>()
    };
    let (a, b) = (samples(a), samples(b));
    let distance = |a: &[Point], b: &[Point]| {
        a.iter()
            .map(|p| {
                b.windows(2)
                    .map(|w| {
                        let d = w[1] - w[0];
                        let t = if d.hypot2() > 0.0 { ((*p - w[0]).dot(d) / d.hypot2()).clamp(0.0, 1.0) } else { 0.0 };
                        p.distance(w[0] + d * t)
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    };
    distance(&a, &b).max(distance(&b, &a))
}
#[test]
fn envelope_identity_is_exact_for_curves_and_compounds() {
    let mut p = curve();
    p.subpaths.extend(square().subpaths);
    p.subpaths[1].op = photocraft_doc::PathOp::Subtract;
    for spec in
        [identity_mesh(1, 1).unwrap(), identity_mesh(32, 32).unwrap(), EnvelopeSpec::Warp { style: "wave".into(), bend: 0.0, horizontal: 0.0, vertical: 0.0 }]
    {
        let out = evaluate(std::slice::from_ref(&p), &spec, Options::default()).unwrap().remove(0);
        assert_eq!(out, p);
        assert!(hausdorff(&p, &out) < 1e-6);
    }
}
#[test]
fn envelope_affine_mesh_matches_independent_matrix_on_every_handle() {
    let p = curve();
    let bounds = source_bounds(std::slice::from_ref(&p)).unwrap();
    let normalized = Affine::new([1.2, 0.3, -0.2, 0.8, 0.15, -0.1]);
    let mut spec = identity_mesh(3, 4).unwrap();
    if let EnvelopeSpec::Mesh { points, .. } = &mut spec {
        for point in points {
            let p = normalized * Point::new(point[0], point[1]);
            *point = [p.x, p.y];
        }
    }
    let out = evaluate(std::slice::from_ref(&p), &spec, Options::default()).unwrap().remove(0);
    let mut expected = p.clone();
    for k in expected.subpaths.iter_mut().flat_map(|s| &mut s.knots) {
        for v in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
            let q = normalized * Point::new((v.x - bounds.x0) / bounds.width(), (v.y - bounds.y0) / bounds.height());
            *v = DocPoint::new(bounds.x0 + q.x * bounds.width(), bounds.y0 + q.y * bounds.height());
        }
    }
    for (a, b) in out.subpaths.iter().flat_map(|s| &s.knots).zip(expected.subpaths.iter().flat_map(|s| &s.knots)) {
        for (x, y) in [(a.anchor, b.anchor), (a.in_ctrl, b.in_ctrl), (a.out_ctrl, b.out_ctrl)] {
            assert!((x.x - y.x).hypot(x.y - y.y) < 1e-10);
        }
    }
    assert!(hausdorff(&out, &expected) < 1e-6);
}
#[test]
fn envelope_coons_preserves_curved_boundaries_and_reversed_outline() {
    let mut top = square();
    top.subpaths[0].knots[0].out_ctrl = DocPoint::new(100.0 / 3.0, -30.0);
    top.subpaths[0].knots[1].in_ctrl = DocPoint::new(200.0 / 3.0, -30.0);
    let source = Rect::new(0.0, 0.0, 100.0, 100.0);
    for path in [top.clone(), photocraft_pathops::reverse(&top).unwrap()] {
        let map = Map::new(&EnvelopeSpec::TopObject { path }, source).unwrap();
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let expected = Point::new(100.0 * t, -90.0 * t * (1.0 - t));
            let p = map.point(Point::new(100.0 * t, 0.0)).unwrap();
            assert!(p.distance(expected) < 1e-10);
        }
        assert!(map.point(Point::new(50.0, 100.0)).unwrap().distance(Point::new(50.0, 100.0)) < 1e-10);
    }
    let out = evaluate(&[square()], &EnvelopeSpec::TopObject { path: top.clone() }, Options::default()).unwrap().remove(0);
    assert!(hausdorff(&out, &top) < 0.05);
}
#[test]
fn envelope_warp_arch_has_analytic_boundary_and_bounded_nodes() {
    let p = square();
    let spec = EnvelopeSpec::Warp { style: "arch".into(), bend: 50.0, horizontal: 0.0, vertical: 0.0 };
    let out = evaluate(std::slice::from_ref(&p), &spec, Options::default()).unwrap().remove(0);
    let q = to_geometry(&out);
    // Independent parabola oracle: arch maps y to y - 25*(1-((x-50)/50)^2).
    for i in 0..=100 {
        let x = i as f64;
        let expected = Point::new(x, -25.0 * (1.0 - ((x - 50.0) / 50.0).powi(2)));
        let mut distance = f64::INFINITY;
        for s in &q.subpaths {
            for j in 0..s.segment_count() {
                use kurbo::ParamCurveNearest;
                distance = distance.min(s.segment(j).nearest(expected, 1e-8).distance_sq.sqrt());
            }
        }
        assert!(distance <= 0.05, "{x}: {distance}");
    }
    assert!(out.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() <= 128);
    assert_eq!(evaluate(std::slice::from_ref(&p), &spec, Options::default()).unwrap(), evaluate(&[p], &spec, Options::default()).unwrap());
}
#[test]
fn envelope_all_styles_mesh_reject_malformed_and_budget_exhaustion() {
    let p = square();
    for style in
        ["arc", "arcLower", "arcUpper", "arch", "bulge", "shellLower", "shellUpper", "flag", "wave", "fish", "rise", "fisheye", "inflate", "squeeze", "twist"]
    {
        let out =
            evaluate(std::slice::from_ref(&p), &EnvelopeSpec::Warp { style: style.into(), bend: 40.0, horizontal: 10.0, vertical: -10.0 }, Options::default())
                .unwrap();
        assert!(
            out.iter()
                .flat_map(|p| &p.subpaths)
                .flat_map(|s| &s.knots)
                .all(|k| [k.anchor, k.in_ctrl, k.out_ctrl].iter().all(|p| p.x.is_finite() && p.y.is_finite()))
        );
    }
    for (rows, cols, points) in [(0, 1, vec![]), (u32::MAX, u32::MAX, vec![]), (1, 1, vec![[0.0, 0.0]; 3]), (1, 1, vec![[f64::NAN, 0.0]; 4])] {
        assert!(evaluate(std::slice::from_ref(&p), &EnvelopeSpec::Mesh { rows, cols, points }, Options::default()).is_err());
    }
    let bad = EnvelopeSpec::Warp { style: "unknown".into(), bend: 0.0, horizontal: 0.0, vertical: 0.0 };
    assert!(evaluate(std::slice::from_ref(&p), &bad, Options::default()).is_err());
    let mut spec = identity_mesh(2, 2).unwrap();
    if let EnvelopeSpec::Mesh { points, .. } = &mut spec {
        points[4] = [0.7, 0.2];
    }
    assert!(evaluate(std::slice::from_ref(&p), &spec, Options { tolerance: 0.000001, max_nodes: 2 }).is_err());
    assert!(evaluate(&[p], &spec, Options { tolerance: f64::INFINITY, max_nodes: 128 }).is_err());
}
#[test]
fn envelope_mesh_interpolates_all_nodes_and_detached_input_remains_identical() {
    let p = square();
    let saved = p.clone();
    let mut spec = identity_mesh(3, 3).unwrap();
    if let EnvelopeSpec::Mesh { points, .. } = &mut spec {
        points[5] = [0.25, 0.42];
        points[10] = [0.73, 0.6];
    }
    let map = Map::new(&spec, Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
    if let EnvelopeSpec::Mesh { points, .. } = &spec {
        for r in 0..=3 {
            for c in 0..=3 {
                let expected = points[r * 4 + c];
                assert!(
                    map.point(Point::new(c as f64 / 3.0 * 100.0, r as f64 / 3.0 * 100.0))
                        .unwrap()
                        .distance(Point::new(expected[0] * 100.0, expected[1] * 100.0))
                        < 1e-10
                );
            }
        }
    }
    evaluate(std::slice::from_ref(&p), &spec, Options::default()).unwrap();
    assert_eq!(p, saved);
}
#[test]
fn envelope_general_outline_and_flat_axis_are_bounded() {
    let top = Path::new(vec![Subpath::polygon(&[
        (0.0, 0.0),
        (40.0, -15.0),
        (100.0, 0.0),
        (115.0, 45.0),
        (100.0, 100.0),
        (60.0, 110.0),
        (0.0, 100.0),
        (-10.0, 55.0),
    ])]);
    let out = evaluate(&[square()], &EnvelopeSpec::TopObject { path: top }, Options::default()).unwrap();
    assert!(!out[0].is_empty());
    let line = Path::new(vec![Subpath::polyline(&[(20.0, 50.0), (150.0, 50.0)])]);
    assert_eq!(evaluate(std::slice::from_ref(&line), &identity_mesh(2, 2).unwrap(), Options::default()).unwrap(), [line]);
}
#[test]
fn envelope_nonlinear_cubic_fit_matches_independent_arch_oracle() {
    use kurbo::ParamCurveNearest;
    let p = curve();
    let b = source_bounds(std::slice::from_ref(&p)).unwrap();
    let out = evaluate(std::slice::from_ref(&p), &EnvelopeSpec::Warp { style: "arch".into(), bend: 50.0, horizontal: 0.0, vertical: 0.0 }, Options::default())
        .unwrap()
        .remove(0);
    let source = to_geometry(&p);
    let result = to_geometry(&out);
    let c = source.subpaths[0].segment(0);
    for i in 0..=512 {
        let q = c.eval(i as f64 / 512.0);
        let x = (q.x - (b.x0 + b.x1) * 0.5) / (b.width() * 0.5);
        let expected = Point::new(q.x, q.y - b.height() * 0.25 * (1.0 - x * x));
        let distance = (0..result.subpaths[0].segment_count())
            .map(|j| result.subpaths[0].segment(j).nearest(expected, 1e-9).distance_sq.sqrt())
            .fold(f64::INFINITY, f64::min);
        assert!(distance <= 0.05, "source t={} error={distance}", i as f64 / 512.0);
    }
    assert!(!out.subpaths[0].closed);
    assert!(out.subpaths[0].knots.len() <= 64);
}
