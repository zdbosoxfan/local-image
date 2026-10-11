//! Package 41 ARAP independent edge/rigid-motion oracles. Synthetic GPL-3.0-or-later paths.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{FillRule, PathOp};
fn bar() -> Path {
    Path::new(vec![Subpath::polygon(&[(0., 0.), (200., 0.), (200., 100.), (0., 100.)])])
}
#[test]
fn distort41_puppet_all_pins_translate_paths_exactly_including_handles_and_metadata() {
    let mut p = bar();
    p.fill_rule = FillRule::EvenOdd;
    p.subpaths[0].op = PathOp::Exclude;
    let rig = Rig::new(std::slice::from_ref(&p), Default::default()).unwrap();
    let mut pins = rig.automatic_pins();
    assert!(pins.len() >= 3);
    for q in &mut pins {
        q.at = [q.rest[0] + 24., q.rest[1] - 17.];
    }
    let pos = rig.deform(&pins).unwrap();
    for (r, q) in rig.mesh.verts.iter().zip(&pos) {
        assert_eq!(*q, *r + kurbo::Vec2::new(24., -17.));
    }
    let got = rig.warp(&p, &pins, &pos).unwrap();
    let expected = p.transform(&photocraft_geom::Affine::translate(24., -17.));
    assert_eq!(got, expected);
}
#[test]
fn distort41_puppet_rotation_with_fixed_pins_has_under_two_percent_edge_stretch() {
    let p = bar();
    let rig = Rig::new(std::slice::from_ref(&p), Options { resolution: 16, ..Default::default() }).unwrap();
    // A small local turn is a feasible near-isometry with the other two pins held fixed.
    let pins = [Pin { angle: Some(0.04), ..Pin::new([20., 50.]) }, Pin::new([180., 30.]), Pin::new([180., 70.])];
    let pos = rig.deform(&pins).unwrap();
    let worst = rig
        .mesh
        .tris
        .iter()
        .flat_map(|t| (0..3).map(move |i| (t[i], t[(i + 1) % 3])))
        .map(|(i, j)| (pos[i].distance(pos[j]) / rig.mesh.verts[i].distance(rig.mesh.verts[j]) - 1.).abs())
        .fold(0., f64::max);
    assert!(worst < 0.02, "edge stretch {worst}");
    for q in pins {
        assert!(rig.mesh.map(&pos, Point::new(q.rest[0], q.rest[1])).distance(Point::new(q.at[0], q.at[1])) < 0.01);
    }
    let turn = rig.mesh.turn_at(&pos, Point::new(20., 50.)).unwrap();
    assert!((turn - 0.04).abs() < 0.01, "turn {turn}");
    let out = rig.warp(&p, &pins, &pos).unwrap();
    assert_ne!(out, p);
    validate_path(&out, rig.options.node_budget).unwrap();
    assert!(out.subpaths[0].closed);
}
#[test]
fn distort41_puppet_automatic_pins_lie_on_artwork_and_identity_is_exact() {
    let p = bar();
    let rig = Rig::new(std::slice::from_ref(&p), Default::default()).unwrap();
    let pins = rig.automatic_pins();
    assert_eq!(pins.len(), 3);
    for q in &pins {
        assert!((0.0..=200.).contains(&q.at[0]) && (0.0..=100.).contains(&q.at[1]));
    }
    let pos = rig.deform(&pins).unwrap();
    assert_eq!(pos, rig.mesh.verts);
    assert_eq!(rig.warp(&p, &pins, &pos).unwrap(), p);
    let pos = rig.deform(&[]).unwrap();
    assert_eq!(rig.warp(&p, &[], &pos).unwrap(), p);
}
#[test]
fn distort41_puppet_holes_and_open_paths_keep_topology_after_refit() {
    let mut p = bar();
    p.subpaths.push(Subpath::polygon(&[(60., 30.), (60., 70.), (140., 70.), (140., 30.)]).with_op(PathOp::Subtract));
    let rig = Rig::new(std::slice::from_ref(&p), Default::default()).unwrap();
    assert!(!rig.mesh.contains(Point::new(100., 50.)), "hole must stay outside the mesh");
    let pins = [Pin::new([20., 30.]), Pin { at: [190., 80.], ..Pin::new([180., 70.]) }];
    let pos = rig.deform(&pins).unwrap();
    let out = rig.warp(&p, &pins, &pos).unwrap();
    assert_eq!(out.subpaths.len(), 2);
    assert_eq!(out.subpaths[1].op, PathOp::Subtract);
    assert!(out.subpaths.iter().all(|s| s.closed));
    let line = Path::new(vec![Subpath::polyline(&[(0., 0.), (100., 0.)])]);
    let rig = Rig::new(std::slice::from_ref(&line), Default::default()).unwrap();
    let pins = [Pin::new([5., 0.]), Pin { at: [95., 10.], ..Pin::new([95., 0.]) }];
    let pos = rig.deform(&pins).unwrap();
    let out = rig.warp(&line, &pins, &pos).unwrap();
    assert!(!out.subpaths[0].closed);
}
#[test]
fn distort41_puppet_bad_mesh_options_pins_and_deformed_vertices_are_errors() {
    let p = bar();
    assert!(Rig::new(std::slice::from_ref(&p), Options { resolution: usize::MAX, ..Default::default() }).is_err());
    let rig = Rig::new(std::slice::from_ref(&p), Default::default()).unwrap();
    assert!(rig.deform(&[Pin::new([10000., 10000.])]).is_err());
    assert!(rig.deform(&[Pin::new([20., 30.]), Pin::new([20., 30.])]).is_err());
    assert!(rig.deform(&[Pin { angle: Some(f64::NAN), ..Pin::new([20., 30.]) }]).is_err());
    assert!(rig.warp(&p, &[Pin { angle: Some(0.1), ..Pin::new([20., 30.]) }], &[]).is_err());
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
}
