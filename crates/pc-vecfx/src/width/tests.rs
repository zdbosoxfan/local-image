#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, Fill, Knot, ShapeLayer};
use photocraft_geom::Rect;
fn line() -> Path {
    Path::new(vec![Subpath::polyline(&[(10.0, 30.0), (110.0, 30.0)])])
}
fn stroke(p: WidthProfile) -> StrokeItem {
    StrokeItem { base: ShapeStroke { width: 20.0, ..Default::default() }, profile: Some(p), ..Default::default() }
}
fn polygon_area(p: &Path) -> f64 {
    p.subpaths
        .iter()
        .map(|s| {
            s.knots.iter().zip(s.knots.iter().cycle().skip(1)).take(s.knots.len()).map(|(a, b)| a.anchor.x * b.anchor.y - b.anchor.x * a.anchor.y).sum::<f64>()
                / 2.0
        })
        .sum::<f64>()
        .abs()
}
fn crossing(p: &Path) -> bool {
    let lines = photocraft_vector::flatten_path(p, 0.001);
    for line in &lines {
        let n = line.pts.len();
        for i in 0..n {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                let (a, b, c, d) = (line.pts[i], line.pts[(i + 1) % n], line.pts[j], line.pts[(j + 1) % n]);
                let cross = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
                if cross(a, b, c) * cross(a, b, d) < -1e-12 && cross(c, d, a) * cross(c, d, b) < -1e-12 {
                    return true;
                }
            }
        }
    }
    false
}
#[test]
fn width_package_lens_area_and_absolute_asymmetric_sides() {
    let p = preset("lens", 10.0).unwrap();
    let out = outline(&line(), &stroke(p)).unwrap();
    // Two triangles, each 100 * 10 / 2; no renderer or geometry algorithm in this oracle.
    assert!((polygon_area(&out) - 1000.0).abs() / 1000.0 <= 0.005);
    let p = WidthProfile { points: vec![WidthPoint { t: 0.0, left: 8.0, right: 2.0 }, WidthPoint { t: 1.0, left: 8.0, right: 2.0 }] };
    let out = outline(&line(), &stroke(p)).unwrap();
    assert!((polygon_area(&out) - 1000.0).abs() < 1e-5);
    assert!(photocraft_pathops::contains(&out, photocraft_geom::Point::new(50.0, 23.0)).unwrap());
    assert!(!photocraft_pathops::contains(&out, photocraft_geom::Point::new(50.0, 33.0)).unwrap());
    assert!(!crossing(&out));
}
#[test]
fn width_package_constant_profile_native_coverage_oracle() {
    let rect = Rect::new(0, 0, 144, 120);
    let paths = [
        line(),
        Path::new(vec![Subpath::polyline(&[(20.0, 80.0), (65.0, 20.0), (110.0, 80.0)])]),
        Path::new(vec![Subpath::polygon(&[(20.0, 20.0), (110.0, 20.0), (110.0, 90.0), (20.0, 90.0)])]),
    ];
    for path in paths {
        for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                let mut st = stroke(uniform(5.0));
                st.base.width = 10.0;
                st.base.cap = cap;
                st.base.join = join;
                let native = photocraft_vector::CompiledShape::new(
                    &ShapeLayer { path: path.clone(), stroke: Some(st.base.clone()), ..Default::default() },
                    TOLERANCE,
                    rect,
                )
                .render_rgba(rect);
                let out = outline(&path, &st).unwrap();
                let projected = photocraft_vector::CompiledShape::new(
                    &ShapeLayer { path: out, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() },
                    TOLERANCE,
                    rect,
                )
                .render_rgba(rect);
                let max = native.iter().zip(projected).map(|(a, b)| (a[3] - b[3]).abs()).fold(0.0, f32::max);
                assert!(max <= 1.0 / 255.0, "{cap:?} {join:?}: {max}");
            }
        }
    }
}
#[test]
fn width_package_arc_length_and_reverse_left_right() {
    let p = Path::new(vec![Subpath::polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 90.0)])]);
    let metric = Metric::new(&p).unwrap();
    let q = metric.sample(0, 0.5).unwrap();
    assert!(q.point.distance((10.0, 40.0).into()) < 1e-6);
    assert!((metric.nearest((10.0, 40.0).into(), None).unwrap().t - 0.5).abs() < 1e-6);
    assert_eq!((q.normal.x, q.normal.y), (1.0, 0.0));
    // Collinear cubic with nonlinear parameter speed: x(t) = 100 t^3.
    let mut a = Knot::corner(0.0, 0.0);
    let mut b = Knot::corner(100.0, 0.0);
    a.out_ctrl = a.anchor;
    b.in_ctrl = a.anchor;
    let cubic = Path::new(vec![Subpath { closed: false, op: PathOp::Combine, knots: vec![a, b] }]);
    let metric = Metric::new(&cubic).unwrap();
    assert!((metric.sample(0, 0.25).unwrap().point.x - 25.0).abs() < 1e-5);
    let mut reversed = line();
    reversed.subpaths[0].knots.reverse();
    let q = Metric::new(&reversed).unwrap().sample(0, 0.5).unwrap();
    assert_eq!((q.normal.x, q.normal.y), (0.0, 1.0));
}
#[test]
fn width_package_tight_bends_corners_and_closed_ring_union_without_crossings() {
    for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
        let mut st = stroke(preset("wave", 15.0).unwrap());
        st.base.join = join;
        for p in [
            Path::new(vec![Subpath::polyline(&[(20.0, 20.0), (90.0, 20.0), (25.0, 25.0), (85.0, 30.0)])]),
            Path::new(vec![Subpath::polygon(&[(20.0, 20.0), (100.0, 20.0), (100.0, 90.0), (20.0, 90.0)])]),
        ] {
            let out = outline(&p, &st).unwrap();
            assert!(!out.subpaths.is_empty());
            assert!(out.subpaths.iter().all(|s| s.closed));
            assert!(!crossing(&out));
            assert!(polygon_area(&out) > 10.0);
        }
    }
}
#[test]
fn width_package_dashes_read_original_profile_span_and_alignment() {
    let mut st = stroke(preset("taperStart", 10.0).unwrap());
    st.base.width = 10.0;
    st.base.dashes = vec![2.0, 2.0];
    let out = outline(&line(), &st).unwrap();
    // Three dash triangles/trapezoids: t ranges [0,.2],[.4,.6],[.8,1].
    assert!((polygon_area(&out) - 600.0).abs() < 0.001);
    assert!(!photocraft_pathops::contains(&out, photocraft_geom::Point::new(40.0, 30.0)).unwrap());
    let p = Path::new(vec![Subpath::polygon(&[(20.0, 20.0), (100.0, 20.0), (100.0, 90.0), (20.0, 90.0)])]);
    st.base.dashes.clear();
    st.base.align = StrokeAlign::Inside;
    let out = outline(&p, &st).unwrap();
    assert!(!photocraft_pathops::contains(&out, photocraft_geom::Point::new(50.0, 19.0)).unwrap());
}
#[test]
fn width_package_profiles_mutation_validation_and_saved_roundtrip() {
    let mut p = uniform(5.0);
    let index = set_point(&mut p, None, WidthPoint { t: 0.25, left: 12.0, right: 4.0 }).unwrap();
    assert_eq!(index, 1);
    assert_eq!(at(&p, 99.0, 0.25), (12.0, 4.0));
    set_point(&mut p, Some(index), WidthPoint { t: 0.75, left: 8.0, right: 1.0 }).unwrap();
    assert_eq!(p.points[1].t, 0.75);
    let before = p.clone();
    for value in [f64::NAN, f64::INFINITY, -1.0, 5001.0] {
        assert!(set_point(&mut p, None, WidthPoint { t: 0.5, left: value, right: 1.0 }).is_err());
        assert_eq!(p, before);
    }
    let saved = SavedProfile { name: "Calligraphy".into(), profile: p };
    assert!(saved.validate().is_ok());
    assert_eq!(serde_json::from_str::<SavedProfile>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    assert_eq!(at(&WidthProfile::default(), 7.0, 0.5), (7.0, 7.0));
}

#[test]
fn width_package_closed_profile_seam_preserves_incoming_and_outgoing_widths() {
    let path = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)])]);
    let profile = WidthProfile { points: vec![WidthPoint { t: 0.0, left: 1.0, right: 1.0 }, WidthPoint { t: 1.0, left: 10.0, right: 2.0 }] };
    for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
        let mut st = stroke(profile.clone());
        st.base.join = join;
        let out = outline(&path, &st).unwrap();
        let contains = |x, y| photocraft_pathops::contains(&out, photocraft_geom::Point::new(x, y)).unwrap();
        // On the incoming vertical side t=.99 gives 9.91/1.99px. On the outgoing
        // horizontal side t=.01 gives 1.09/1.01px. These come from linear interpolation
        // over the independent 400px perimeter, rather than sampling the implementation.
        assert!(contains(-9.0, 4.0), "incoming left, {join:?}");
        assert!(contains(1.5, 4.0), "incoming right, {join:?}");
        assert!(!contains(-11.0, 4.0), "incoming outside, {join:?}");
        assert!(contains(4.0, -0.5), "outgoing left, {join:?}");
        assert!(!contains(4.0, -5.0), "outgoing outside, {join:?}");
        assert!(!crossing(&out));
    }
}
