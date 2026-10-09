use photocraft_algo::perspective::{PerspectiveMap, Plane, Straighten, linked_corners, plane_point, straighten, unify, unit_corners};

fn coord_close(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

fn close(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

fn planes_close(a: &Plane, b: &Plane, tol: f64) -> bool {
    (0..4).all(|i| coord_close(a.src[i], b.src[i], tol) && coord_close(a.dst[i], b.dst[i], tol))
}

#[test]
fn unit_corners_matches_expected() {
    assert_eq!(unit_corners(), [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
}

#[test]
fn plane_identity_sets_src_eq_dst() {
    let q = [[10.0, 20.0], [110.0, 20.0], [110.0, 120.0], [10.0, 120.0]];
    let p = Plane::identity(q);
    assert_eq!(p.src, q);
    assert_eq!(p.dst, q);
}

#[test]
fn linked_corners_empty() {
    assert!(linked_corners(&[]).is_empty());
}

#[test]
fn linked_corners_groups_coinciding_corners() {
    let planes =
        vec![Plane::identity([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]), Plane::identity([[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]])];
    let groups = linked_corners(&planes);
    assert_eq!(groups.len(), 7);
    let linked = groups.iter().find(|g| g.len() == 2).unwrap();
    assert!(linked.contains(&(0, 0)));
    assert!(linked.contains(&(1, 0)));
}

#[test]
fn linked_corners_epsilon_boundary() {
    let a = Plane::identity([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let b = Plane::identity([[0.5, 0.0], [10.5, 0.0], [10.5, 10.0], [0.5, 10.0]]);
    let groups_b = linked_corners(&[a.clone(), b]);
    assert_eq!(groups_b.len(), 4);
    assert!(groups_b.iter().all(|g| g.len() == 2));

    let c = Plane::identity([[0.51, 0.0], [10.51, 0.0], [10.51, 10.0], [0.51, 10.0]]);
    let groups_c = linked_corners(&[a, c]);
    assert_eq!(groups_c.len(), 8);
    assert!(groups_c.iter().all(|g| g.len() == 1));
}

#[test]
fn unify_sets_linked_corners_to_mean() {
    let p0 = Plane { src: [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], dst: [[1.0, 1.0], [11.0, 1.0], [11.0, 11.0], [1.0, 11.0]] };
    let p1 = Plane { src: [[0.4, 0.4], [20.0, 0.4], [20.0, 20.0], [0.4, 20.0]], dst: [[2.0, 5.0], [22.0, 5.0], [22.0, 22.0], [2.0, 22.0]] };
    let mut ps = [p0, p1];
    unify(&mut ps);

    assert!(coord_close(ps[0].src[0], [0.2, 0.2], 1e-12));
    assert!(coord_close(ps[1].src[0], [0.2, 0.2], 1e-12));
    assert!(coord_close(ps[0].dst[0], [1.5, 3.0], 1e-12));
    assert!(coord_close(ps[1].dst[0], [1.5, 3.0], 1e-12));
    assert!(coord_close(ps[0].src[1], [10.0, 0.0], 1e-12));
}

#[test]
fn unify_no_links_leaves_unchanged() {
    let p = Plane::identity([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let original = p.clone();
    let mut ps = [p];
    unify(&mut ps);
    assert!(planes_close(&ps[0], &original, 1e-12));
}

#[test]
fn straighten_parse_valid() {
    assert_eq!(Straighten::parse("h"), Some(Straighten::Horizontal));
    assert_eq!(Straighten::parse("Horizontal"), Some(Straighten::Horizontal));
    assert_eq!(Straighten::parse("LEVEL"), Some(Straighten::Horizontal));
    assert_eq!(Straighten::parse("v"), Some(Straighten::Vertical));
    assert_eq!(Straighten::parse("vertical"), Some(Straighten::Vertical));
    assert_eq!(Straighten::parse("AUTO"), Some(Straighten::Auto));
    assert_eq!(Straighten::parse("both"), Some(Straighten::Auto));
}

#[test]
fn straighten_parse_invalid() {
    assert_eq!(Straighten::parse("x"), None);
    assert_eq!(Straighten::parse(""), None);
    assert_eq!(Straighten::parse("diagonal"), None);
}

#[test]
fn straighten_horizontal_levels_edges() {
    let mut ps = vec![Plane { src: [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], dst: [[0.0, 5.0], [100.0, -5.0], [98.0, 100.0], [2.0, 90.0]] }];
    straighten(&mut ps, Straighten::Horizontal);
    assert!(coord_close(ps[0].dst[0], [0.0, 0.0], 1e-12));
    assert!(coord_close(ps[0].dst[1], [100.0, 0.0], 1e-12));
    assert!((ps[0].dst[0][1] - ps[0].dst[1][1]).abs() < 1e-12);
    assert!((ps[0].dst[2][1] - ps[0].dst[3][1]).abs() < 1e-12);
    assert!(coord_close(ps[0].dst[2], [98.0, 95.0], 1e-12));
    assert!(coord_close(ps[0].dst[3], [2.0, 95.0], 1e-12));
}

#[test]
fn straighten_vertical_plumbs_edges() {
    let mut ps = vec![Plane { src: [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], dst: [[5.0, 0.0], [100.0, 2.0], [95.0, 100.0], [-5.0, 98.0]] }];
    straighten(&mut ps, Straighten::Vertical);
    assert!(coord_close(ps[0].dst[0], [0.0, 0.0], 1e-12));
    assert!(coord_close(ps[0].dst[3], [0.0, 98.0], 1e-12));
    assert!((ps[0].dst[0][0] - ps[0].dst[3][0]).abs() < 1e-12);
    assert!((ps[0].dst[1][0] - ps[0].dst[2][0]).abs() < 1e-12);
    assert!(coord_close(ps[0].dst[1], [97.5, 2.0], 1e-12));
    assert!(coord_close(ps[0].dst[2], [97.5, 100.0], 1e-12));
}

#[test]
fn straighten_auto_does_both() {
    let mut ps = vec![Plane { src: [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], dst: [[5.0, 5.0], [100.0, -5.0], [95.0, 100.0], [-5.0, 90.0]] }];
    straighten(&mut ps, Straighten::Auto);
    // Top/bottom become horizontal.
    assert!((ps[0].dst[0][1] - ps[0].dst[1][1]).abs() < 1e-12);
    assert!((ps[0].dst[2][1] - ps[0].dst[3][1]).abs() < 1e-12);
    // Left/right become vertical.
    assert!((ps[0].dst[0][0] - ps[0].dst[3][0]).abs() < 1e-12);
    assert!((ps[0].dst[1][0] - ps[0].dst[2][0]).abs() < 1e-12);
}

#[test]
fn perspective_map_new_empty_returns_none() {
    assert!(PerspectiveMap::new(&[]).is_none());
}

#[test]
fn perspective_map_new_degenerate_src_returns_none() {
    let p = Plane { src: [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]], dst: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] };
    assert!(PerspectiveMap::new(&[p]).is_none());
}

#[test]
fn perspective_map_new_degenerate_dst_returns_none() {
    let p = Plane { src: [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], dst: [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]] };
    assert!(PerspectiveMap::new(&[p]).is_none());
}

#[test]
fn identity_map_returns_same_points() {
    let q = [[10.0, 10.0], [90.0, 10.0], [90.0, 70.0], [10.0, 70.0]];
    let m = PerspectiveMap::new(&[Plane::identity(q)]).unwrap();
    for p in [(20.0, 20.0), (50.0, 40.0), (80.0, 60.0), (0.0, 0.0), (120.0, 90.0)] {
        let r = m.map(p.0, p.1);
        assert!(close(r, p, 1e-9), "expected {p:?} got {r:?}");
    }
}

#[test]
fn corners_map_to_dst() {
    let src = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let dst = [[20.0, 0.0], [80.0, 10.0], [90.0, 110.0], [0.0, 90.0]];
    let m = PerspectiveMap::new(&[Plane { src, dst }]).unwrap();
    for i in 0..4 {
        let r = m.map(src[i][0], src[i][1]);
        assert!(close(r, (dst[i][0], dst[i][1]), 1e-9));
    }
}

#[test]
fn map_matches_plane_point() {
    let src = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let dst = [[20.0, 0.0], [80.0, 10.0], [90.0, 110.0], [0.0, 90.0]];
    let p = Plane { src, dst };
    let m = PerspectiveMap::new(std::slice::from_ref(&p)).unwrap();
    for (u, v) in [(0.2, 0.3), (0.5, 0.5), (0.8, 0.1), (0.1, 0.9)] {
        let s = plane_point(&p, u, v, false);
        let d = plane_point(&p, u, v, true);
        let r = m.map(s[0], s[1]);
        assert!(close(r, (d[0], d[1]), 1e-9), "uv=({u},{v}) got {r:?} expected {d:?}");
    }
}

#[test]
fn map_is_deterministic() {
    let p = Plane { src: [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], dst: [[20.0, 0.0], [80.0, 10.0], [90.0, 110.0], [0.0, 90.0]] };
    let m = PerspectiveMap::new(&[p]).unwrap();
    for (x, y) in [(0.0, 0.0), (50.0, 50.0), (25.0, 75.0), (-10.0, 120.0)] {
        let r1 = m.map(x, y);
        let r2 = m.map(x, y);
        assert!(r1.0 == r2.0 && r1.1 == r2.1);
    }
}

#[test]
fn map_nan_inf_does_not_panic_and_finite() {
    let q = [[10.0, 10.0], [90.0, 10.0], [90.0, 70.0], [10.0, 70.0]];
    let m = PerspectiveMap::new(&[Plane::identity(q)]).unwrap();
    let finite_inputs = [(f64::NAN, f64::NAN), (f64::INFINITY, 0.0), (0.0, f64::NEG_INFINITY)];
    for (x, y) in finite_inputs {
        let r = m.map(x, y);
        assert!(r.0.is_finite() && r.1.is_finite(), "({x},{y}) -> {r:?}");
    }
}

#[test]
fn shared_edges_stay_continuous() {
    let a = Plane { src: [[0.0, 0.0], [50.0, 0.0], [50.0, 100.0], [0.0, 100.0]], dst: [[0.0, 0.0], [50.0, 5.0], [55.0, 100.0], [0.0, 95.0]] };
    let b = Plane { src: [[50.0, 0.0], [100.0, 0.0], [100.0, 100.0], [50.0, 100.0]], dst: [[50.2, 4.8], [110.0, 0.0], [100.0, 90.0], [54.8, 100.1]] };
    let m = PerspectiveMap::new(&[a, b]).unwrap();
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let y = 100.0 * t;
        let left = m.map(50.0 - 1e-6, y);
        let right = m.map(50.0 + 1e-6, y);
        assert!(close(left, right, 1e-3), "t={t}: {left:?} vs {right:?}");
    }
}

#[test]
fn plane_point_returns_q0_on_degenerate() {
    let q = [[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]];
    let p = Plane::identity(q);
    let s = plane_point(&p, 0.5, 0.5, false);
    let d = plane_point(&p, 0.5, 0.5, true);
    assert!(coord_close(s, [1.0, 1.0], 1e-12));
    assert!(coord_close(d, [1.0, 1.0], 1e-12));
}

#[test]
fn plane_point_dst_and_src_corners() {
    let src = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let dst = [[1.0, 2.0], [9.0, 3.0], [8.0, 11.0], [2.0, 9.0]];
    let p = Plane { src, dst };
    for (u, v, src_corner, dst_corner) in [(0.0, 0.0, src[0], dst[0]), (1.0, 0.0, src[1], dst[1]), (1.0, 1.0, src[2], dst[2]), (0.0, 1.0, src[3], dst[3])] {
        let s = plane_point(&p, u, v, false);
        let d = plane_point(&p, u, v, true);
        assert!(coord_close(s, src_corner, 1e-12));
        assert!(coord_close(d, dst_corner, 1e-12));
    }
}

#[test]
fn plane_serde_roundtrip() {
    let p = Plane { src: [[0.1, 0.2], [1.3, 0.4], [1.5, 1.6], [0.7, 1.8]], dst: [[2.1, 2.3], [3.4, 2.5], [3.6, 3.7], [2.8, 3.9]] };
    let s = serde_json::to_string(&p).unwrap();
    let q: Plane = serde_json::from_str(&s).unwrap();
    assert!(planes_close(&p, &q, 1e-12));
}

#[test]
fn perspective_map_is_identity() {
    assert!(PerspectiveMap::is_identity(&[]));

    let identity = Plane::identity([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    assert!(PerspectiveMap::is_identity(std::slice::from_ref(&identity)));

    let moved = Plane { src: [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], dst: [[1.0, 0.0], [11.0, 0.0], [11.0, 10.0], [1.0, 10.0]] };
    assert!(!PerspectiveMap::is_identity(&[moved]));
}
