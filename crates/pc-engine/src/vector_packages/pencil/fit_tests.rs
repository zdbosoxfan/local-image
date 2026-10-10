#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
fn samples(points: &[(f64, f64)]) -> Vec<Sample> {
    points.iter().map(|&(x, y)| Sample { point: Point::new(x, y), pressure: 1.0 }).collect()
}
// Independent oracle: dense Bernstein evaluation and point-to-line distance. Does not
// call the fitter, its residuals, or the production flattening implementation.
fn bernstein(c: &[Point; 4], t: f64) -> Point {
    let u = 1.0 - t;
    Point::new(
        u * u * u * c[0].x + 3.0 * u * u * t * c[1].x + 3.0 * u * t * t * c[2].x + t * t * t * c[3].x,
        u * u * u * c[0].y + 3.0 * u * u * t * c[1].y + 3.0 * u * t * t * c[2].y + t * t * t * c[3].y,
    )
}
fn dense(path: &Path) -> Vec<Point> {
    path.subpaths.iter().flat_map(|s| s.segments()).flat_map(|c| (0..=256).map(move |i| bernstein(&c, i as f64 / 256.0))).collect()
}
fn nearest(p: Point, line: &[Point]) -> f64 {
    line.windows(2)
        .map(|w| {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let den = dx * dx + dy * dy;
            let t = if den == 0.0 { 0.0 } else { (((p.x - w[0].x) * dx + (p.y - w[0].y) * dy) / den).clamp(0.0, 1.0) };
            (p.x - w[0].x - t * dx).hypot(p.y - w[0].y - t * dy)
        })
        .fold(f64::INFINITY, f64::min)
}
fn hausdorff(a: &[Point], b: &[Point]) -> f64 {
    a.iter().map(|&p| nearest(p, b)).chain(b.iter().map(|&p| nearest(p, a))).fold(0.0, f64::max)
}
fn crossings(p: &[Point], closed: bool) -> usize {
    let area = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut n = 0;
    for i in 0..p.len() - 1 {
        for j in i + 2..p.len() - 1 {
            if closed && i == 0 && j == p.len() - 2 {
                continue;
            }
            let (a, b, c, d) = (p[i], p[i + 1], p[j], p[j + 1]);
            if area(a, b, c) * area(a, b, d) < -1e-12 && area(c, d, a) * area(c, d, b) < -1e-12 {
                n += 1;
            }
        }
    }
    n
}
#[test]
fn pencil_fit_circle_compact_symmetric_hausdorff_and_no_crossings() {
    for radius in [10.0, 50.0, 100.0] {
        let pts: Vec<_> = (0..=240)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 240.0;
                (150.0 + radius * a.cos(), 150.0 + radius * a.sin())
            })
            .collect();
        let input = samples(&pts);
        let path = fit(&input, FitOptions { closed: true, ..Default::default() }).unwrap();
        assert!(path.subpaths[0].knots.len() <= 8, "{} knots at radius {radius}", path.subpaths[0].knots.len());
        let curve = dense(&path);
        let source: Vec<_> = input.iter().map(|s| s.point).collect();
        let error = hausdorff(&curve, &source);
        assert!(error <= 1.5, "Hausdorff {error} at radius {radius}");
        assert_eq!(crossings(&curve, true), 0);
    }
}
#[test]
fn pencil_fit_preserves_corners_endpoints_and_sparse_curve_fidelity() {
    for pts in [
        vec![(0., 0.), (40., 0.), (40., 40.), (80., 40.)],
        vec![(0., 0.), (10., 20.), (30., -20.), (50., 0.)],
        (0..=64).map(|i| (i as f64, (i as f64 * 0.08).sin() * 8.0)).collect(),
    ] {
        let input = samples(&pts);
        let path = fit(&input, FitOptions { tolerance: 0.5, ..Default::default() }).unwrap();
        assert_eq!(path.subpaths[0].knots[0].anchor, input[0].point);
        assert_eq!(path.subpaths[0].knots.last().unwrap().anchor, input.last().unwrap().point);
        let curve = dense(&path);
        assert!(hausdorff(&curve, &input.iter().map(|s| s.point).collect::<Vec<_>>()) <= 0.5);
        assert_eq!(crossings(&curve, false), 0);
    }
}
#[test]
fn pencil_fit_256_deterministic_simple_regressions() {
    for seed in 0..256 {
        let radius = 5.0 + seed as f64 * 0.3;
        let input: Vec<_> =
            (0..=48).map(|i| Sample { point: Point::new(i as f64 * 2.0, radius * (i as f64 * 0.02).sin()), pressure: i as f64 / 48.0 }).collect();
        let options = FitOptions { tolerance: 0.5, smoothness: seed as f64 / 255.0, closed: false };
        let a = fit(&input, options).unwrap();
        assert_eq!(a, fit(&input, options).unwrap());
        let curve = dense(&a);
        assert!(hausdorff(&curve, &input.iter().map(|s| s.point).collect::<Vec<_>>()) <= 0.5, "seed {seed}");
        assert_eq!(crossings(&curve, false), 0, "seed {seed}");
    }
}
#[test]
fn pencil_stabilizers_have_independent_rope_and_average_oracles_and_cancel() {
    let s = |x, y, p| Sample { point: Point::new(x, y), pressure: p };
    let mut rope = Stabilizer::new(Stabilization::Rope { length: 5.0 }).unwrap();
    assert_eq!(rope.push(s(0., 0., 1.)).unwrap().point, Point::new(0., 0.));
    assert_eq!(rope.push(s(3., 4., 0.5)).unwrap().point, Point::new(0., 0.));
    assert_eq!(rope.push(s(6., 8., 0.2)).unwrap().point, Point::new(3., 4.));
    assert_eq!(rope.finish(s(6., 8., 0.2)).unwrap().point, Point::new(6., 8.));
    assert_eq!(rope.push(s(100., 0., 1.)).unwrap().point, Point::new(100., 0.));
    let mut window = Stabilizer::new(Stabilization::Window { samples: 3 }).unwrap();
    for x in [0., 3., 6.] {
        window.push(s(x, 0., x / 6.)).unwrap();
    }
    let avg = window.push(s(9., 0., 1.)).unwrap();
    assert_eq!(avg.point.x, 6.0);
    assert!((avg.pressure - 5.0 / 6.0).abs() < 1e-12);
    window.cancel();
    assert_eq!(window.push(s(30., 40., 0.1)).unwrap(), s(30., 40., 0.1));
}
#[test]
fn pencil_sculpt_retains_outside_knots_and_component_semantics() {
    let mut path = Path::new(vec![
        Subpath::polyline(&[(0., 0.), (10., 0.), (20., 0.), (30., 0.), (40., 0.), (50., 0.)]),
        Subpath::polygon(&[(100., 0.), (110., 0.), (110., 10.)]).with_op(photocraft_doc::PathOp::Exclude),
    ]);
    path.fill_rule = photocraft_doc::FillRule::EvenOdd;
    let replacement = fit(&samples(&[(10., 0.), (20., 10.), (40., 0.)]), Default::default()).unwrap();
    let result = sculpt(&path, 0, 1, 4, &replacement).unwrap();
    assert_eq!(result.subpaths[0].knots[0], path.subpaths[0].knots[0]);
    assert_eq!(result.subpaths[0].knots.last(), path.subpaths[0].knots.last());
    assert_eq!(result.subpaths[1], path.subpaths[1]);
    assert_eq!(result.fill_rule, path.fill_rule);
    assert_eq!(result.subpaths[0].knots[1].anchor, path.subpaths[0].knots[1].anchor);
    assert!(!result.subpaths[0].closed);
    path.subpaths[0].closed = true;
    let replacement = fit(&samples(&[(40., 0.), (40., 20.), (10., 0.)]), Default::default()).unwrap();
    let result = sculpt(&path, 0, 4, 1, &replacement).unwrap();
    assert!(result.subpaths[0].closed);
    for k in &path.subpaths[0].knots[2..4] {
        assert!(result.subpaths[0].knots.contains(k));
    }
}
#[test]
fn pencil_fit_rejects_nonfinite_options_budget_and_bad_backend() {
    let input = samples(&[(0., 0.), (10., 0.)]);
    for t in [f64::NAN, f64::INFINITY, -1., 0., 101.] {
        assert!(fit(&input, FitOptions { tolerance: t, ..Default::default() }).is_err());
    }
    assert!(fit(&vec![input[0]; MAX_SAMPLES + 1], Default::default()).is_err());
    assert!(fit(&[Sample { point: Point::new(f64::NAN, 0.), pressure: 1.0 }, input[1]], Default::default()).is_err());
    assert!(fit_with(&input, Default::default(), |_, _| Ok(Path::default())).is_err());
    assert!(Stabilizer::new(Stabilization::Window { samples: 0 }).is_err());
}
#[test]
fn pencil_sculpt_range_splits_same_cubic_and_retains_outside_geometry() {
    let path = Path::new(vec![Subpath {
        knots: vec![
            Knot::smooth(Point::new(0., 0.), Point::new(0., 0.), Point::new(30., 60.)),
            Knot::smooth(Point::new(100., 0.), Point::new(70., 60.), Point::new(130., -60.)),
            Knot::smooth(Point::new(200., 0.), Point::new(170., -60.), Point::new(200., 0.)),
        ],
        ..Default::default()
    }]);
    let (base, first, last) = sculpt_boundaries(&path, 0, (0, 0.25), (0, 0.75)).unwrap();
    let p0 = bernstein(&path.subpaths[0].segments()[0], 0.25);
    let p1 = bernstein(&path.subpaths[0].segments()[0], 0.75);
    assert_eq!(base.subpaths[0].knots[first].anchor, p0);
    assert_eq!(base.subpaths[0].knots[last].anchor, p1);
    let replacement = fit(
        &[Sample { point: p0, pressure: 1. }, Sample { point: Point::new(50., 90.), pressure: 1. }, Sample { point: p1, pressure: 1. }],
        Default::default(),
    )
    .unwrap();
    let result = sculpt(&base, 0, first, last, &replacement).unwrap();
    for knot in &path.subpaths[0].knots {
        assert!(result.subpaths[0].knots.iter().any(|k| k.anchor == knot.anchor));
    }
    assert_eq!(result.subpaths[0].segments().last(), path.subpaths[0].segments().last());
    let untouched = result.subpaths[0].segments()[0];
    let original = path.subpaths[0].segments()[0];
    for i in 0..=100 {
        assert!(distance(bernstein(&untouched, i as f64 / 100.), bernstein(&original, i as f64 / 400.)) < 1e-10);
    }
    assert!(sculpt_boundaries(&path, 0, (0, f64::NAN), (0, 0.75)).is_err());
}
#[test]
fn pencil_sculpt_range_closed_wrap_retains_outside_anchors_and_components() {
    let path = Path::new(vec![Subpath::polygon(&[(10., 10.), (90., 10.), (90., 90.), (10., 90.)]), Subpath::polyline(&[(120., 10.), (150., 10.)])]);
    let (base, first, last) = sculpt_boundaries(&path, 0, (3, 0.5), (1, 0.5)).unwrap();
    assert!(first > last);
    let a = base.subpaths[0].knots[first].anchor;
    let b = base.subpaths[0].knots[last].anchor;
    let replacement =
        fit(&[Sample { point: a, pressure: 1. }, Sample { point: Point::new(50., 40.), pressure: 1. }, Sample { point: b, pressure: 1. }], Default::default())
            .unwrap();
    let result = sculpt(&base, 0, first, last, &replacement).unwrap();
    assert!(result.subpaths[0].closed);
    assert_eq!(result.subpaths[1], path.subpaths[1]);
    for k in &path.subpaths[0].knots[2..] {
        assert!(result.subpaths[0].knots.iter().any(|n| n.anchor == k.anchor));
    }
}

#[test]
fn pencil_fit_spiral_flat_ellipse_and_noisy_monotone_regressions() {
    let fixtures = [
        (
            (0..=180)
                .map(|i| {
                    let t = i as f64 / 180.0;
                    let angle = t * std::f64::consts::TAU * 1.8;
                    let radius = 20.0 + 80.0 * t;
                    (150.0 + radius * angle.cos(), 150.0 + radius * angle.sin())
                })
                .collect::<Vec<_>>(),
            false,
        ),
        (
            (0..=180)
                .map(|i| {
                    let angle = i as f64 * std::f64::consts::TAU / 180.0;
                    (150.0 + 100.0 * angle.cos(), 150.0 + 3.0 * angle.sin())
                })
                .collect::<Vec<_>>(),
            true,
        ),
        ((0..=120).map(|i| (i as f64, (i as f64 * 0.08).sin() * 20.0 + (i as f64 * 1.8).sin() * 0.4)).collect::<Vec<_>>(), false),
    ];
    for (points, closed) in fixtures {
        let input = samples(&points);
        let path = fit(&input, FitOptions { tolerance: 0.5, smoothness: 1.0, closed }).unwrap();
        let curve = dense(&path);
        assert!(hausdorff(&curve, &input.iter().map(|s| s.point).collect::<Vec<_>>()) <= 0.5);
        assert_eq!(crossings(&curve, closed), 0);
    }
}
