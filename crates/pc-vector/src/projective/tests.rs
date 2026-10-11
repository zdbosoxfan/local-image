#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{FillRule, PathOp};
fn eval(p: [Point; 4], t: f64) -> Point {
    let s = 1. - t;
    Point::new(
        s * s * s * p[0].x + 3. * s * s * t * p[1].x + 3. * s * t * t * p[2].x + t * t * t * p[3].x,
        s * s * s * p[0].y + 3. * s * s * t * p[1].y + 3. * s * t * t * p[2].y + t * t * t * p[3].y,
    )
}
fn assert_oracle(path: &Path, map: Map, oracle: impl Fn(Point) -> Point) {
    let result = map_path(path, map, 0.25).unwrap();
    assert!(result.error_bound <= 0.25);
    let old: Vec<_> = path.subpaths.iter().map(Subpath::segments).collect();
    let mapped: Vec<_> = result.path.subpaths.iter().flat_map(Subpath::segments).collect();
    assert_eq!(mapped.len(), result.intervals.len());
    for (q, (si, seg, a, b)) in mapped.into_iter().zip(result.intervals) {
        for i in 0..=128 {
            let t = i as f64 / 128.;
            let source = eval(old[si][seg], a + (b - a) * t);
            let want = oracle(source);
            let got = eval(q, t);
            assert!((got.x - want.x).hypot(got.y - want.y) <= 0.250000001, "{got:?} {want:?}");
        }
    }
}
#[test]
fn transform34_projective_square_trapezoid_exact_corners_straight_edges() {
    let q = [Point::new(20., 0.), Point::new(80., 0.), Point::new(100., 100.), Point::new(0., 100.)];
    let p = Path::new(vec![Subpath::polygon(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)])]);
    for map in [Map::bilinear([0., 0., 100., 100.], q).unwrap(), Map::perspective([0., 0., 100., 100.], q).unwrap()] {
        let r = map_path(&p, map, 0.25).unwrap();
        for (i, k) in p.subpaths[0].knots.iter().enumerate() {
            let got = map.point(k.anchor).unwrap();
            assert!((got.x - q[i].x).hypot(got.y - q[i].y) < 1e-9);
        }
        for (c, (_, seg, _, _)) in r.path.subpaths[0].segments().iter().zip(&r.intervals) {
            let a = q[*seg];
            let b = q[(*seg + 1) % 4];
            for p in c {
                assert!(((p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)).abs() < 1e-8);
            }
        }
    }
}
#[test]
fn transform34_projective_bilinear_curves_independent_degree_six_oracle() {
    let p = Path::new(vec![Subpath {
        knots: vec![
            Knot { anchor: Point::new(0., 0.), in_ctrl: Point::new(0., 0.), out_ctrl: Point::new(50., 120.), smooth: false },
            Knot { anchor: Point::new(100., 100.), in_ctrl: Point::new(150., -20.), out_ctrl: Point::new(100., 100.), smooth: false },
        ],
        ..Default::default()
    }]);
    let q = [Point::new(20., 0.), Point::new(80., 0.), Point::new(100., 100.), Point::new(0., 100.)];
    assert_oracle(&p, Map::bilinear([0., 0., 100., 100.], q).unwrap(), |p| Point::new(20. + 0.6 * p.x - 0.2 * p.y + 0.004 * p.x * p.y, p.y));
}
#[test]
fn transform34_projective_circle_independent_homography_oracle() {
    let p = crate::shapes::live_path(&photocraft_doc::LiveShape::Ellipse { rect: [0., 0., 100., 100.] });
    let q = [Point::new(20., 0.), Point::new(80., 0.), Point::new(100., 100.), Point::new(0., 100.)];
    // Solving the four corners independently: X=(.6x-.2y+20)/(1-.004y), Y=.6y/(1-.004y).
    assert_oracle(&p, Map::perspective([0., 0., 100., 100.], q).unwrap(), |p| {
        let w = 1. - 0.004 * p.y;
        Point::new((0.6 * p.x - 0.2 * p.y + 20.) / w, 0.6 * p.y / w)
    });
}
#[test]
fn transform34_projective_topology_empty_single_open_handles_and_error_budget() {
    let mut p = Path::new(vec![
        Subpath::polygon(&[(0., 0.), (100., 0.), (100., 100.)]),
        Subpath::polygon(&[(10., 10.), (20., 20.), (20., 10.)]).with_op(PathOp::Join),
        Subpath::polyline(&[(0., 0.), (30., 50.)]),
        Subpath { knots: vec![Knot::corner(1., 2.)], ..Default::default() },
        Subpath::default(),
    ]);
    p.fill_rule = FillRule::EvenOdd;
    p.inverted = true;
    let map = Map::Projective([1., 0., 5., 0., 1., 7., 0., 0., 1.]);
    let r = map_path(&p, map, 1e-6).unwrap();
    assert_eq!(r.path.fill_rule, p.fill_rule);
    assert!(r.path.inverted);
    assert_eq!(r.path.subpaths[1].op, PathOp::Join);
    assert!(!r.path.subpaths[2].closed);
    assert_eq!(r.path.subpaths[3].knots[0].anchor, Point::new(6., 9.));
    assert!(r.path.subpaths[4].knots.is_empty());
    assert_eq!(map_path(&p, map, f64::NAN).unwrap_err(), Error::Invalid);
    assert_eq!(map_path(&p, map, 0.3).unwrap_err(), Error::Invalid);
}
#[test]
fn transform34_projective_rejects_horizon_folds_singular_nonfinite() {
    let p = Path::new(vec![Subpath::polyline(&[(-2., 0.), (2., 0.)])]);
    assert_eq!(map_path(&p, Map::Projective([1., 0., 0., 0., 1., 0., 1., 0., 0.5]), 0.25).unwrap_err(), Error::Horizon);
    assert!(Map::Projective([0.; 9]).validate().is_err());
    assert!(Map::bilinear([0., 0., 1., 1.], [Point::new(0., 0.), Point::new(1., 1.), Point::new(0., 1.), Point::new(1., 0.)]).is_err());
    assert!(map_path(&p, Map::Projective([f64::NAN; 9]), 0.25).is_err());
}
