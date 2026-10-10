use photocraft_engine::doc::{FillRule, PathOp};
use photocraft_engine::vector_cmds::{parse_path, path_json, shape_path, specs};
use serde_json::json;
use std::collections::HashSet;

#[test]
fn parse_path_accepts_corner_arrays() {
    let v = json!({"subpaths":[{"closed":true,"knots":[[10.0,20.0],[30.0,40.0]]}]});
    let p = parse_path(&v).unwrap();
    assert_eq!(p.subpaths.len(), 1);
    let sub = &p.subpaths[0];
    assert_eq!(sub.knots.len(), 2);
    assert_eq!(sub.knots[0].anchor.x, 10.0);
    assert_eq!(sub.knots[0].anchor.y, 20.0);
    assert_eq!(sub.knots[0].in_ctrl.x, 10.0);
    assert_eq!(sub.knots[0].in_ctrl.y, 20.0);
    assert_eq!(sub.knots[0].out_ctrl.x, 10.0);
    assert_eq!(sub.knots[0].out_ctrl.y, 20.0);
    assert!(!sub.knots[0].smooth);
}

#[test]
fn parse_path_accepts_object_knots_with_handles() {
    let v = json!({
        "subpaths":[{
            "knots":[{"anchor":[1.0,2.0],"in":[0.0,0.0],"out":[3.0,4.0],"smooth":true}]
        }]
    });
    let p = parse_path(&v).unwrap();
    let k = &p.subpaths[0].knots[0];
    assert_eq!(k.anchor.x, 1.0);
    assert_eq!(k.anchor.y, 2.0);
    assert_eq!(k.in_ctrl.x, 0.0);
    assert_eq!(k.in_ctrl.y, 0.0);
    assert_eq!(k.out_ctrl.x, 3.0);
    assert_eq!(k.out_ctrl.y, 4.0);
    assert!(k.smooth);
}

#[test]
fn parse_path_defaults_closed_op_fill_rule() {
    let v = json!({"subpaths":[{"knots":[[0.0,0.0]]}]});
    let p = parse_path(&v).unwrap();
    assert!(p.subpaths[0].closed);
    assert!(matches!(p.subpaths[0].op, PathOp::Combine));
    assert!(matches!(p.fill_rule, FillRule::NonZero));
    assert!(!p.inverted);
}

#[test]
fn parse_path_rejects_missing_subpaths() {
    let v = json!({});
    assert!(parse_path(&v).is_err());
}

#[test]
fn parse_path_rejects_missing_knots() {
    let v = json!({"subpaths":[{"closed":true}]});
    assert!(parse_path(&v).is_err());
}

#[test]
fn parse_path_rejects_bad_op() {
    let v = json!({"subpaths":[{"op":"bogus","knots":[[0.0,0.0]]}]});
    assert!(parse_path(&v).is_err());
}

#[test]
fn parse_path_rejects_bad_knot() {
    let v = json!({"subpaths":[{"knots":[[1.0]]}]});
    assert!(parse_path(&v).is_err());
}

#[test]
fn parse_path_accepts_fill_rule_variants() {
    for (s, expected) in [
        ("evenodd", FillRule::EvenOdd),
        ("even-odd", FillRule::EvenOdd),
        ("even_odd", FillRule::EvenOdd),
        ("nonzero", FillRule::NonZero),
        ("unknown", FillRule::NonZero),
    ] {
        let v = json!({"subpaths":[{"knots":[[0.0,0.0]]}], "fillRule": s});
        let p = parse_path(&v).unwrap();
        match expected {
            FillRule::EvenOdd => assert!(matches!(p.fill_rule, FillRule::EvenOdd)),
            FillRule::NonZero => assert!(matches!(p.fill_rule, FillRule::NonZero)),
        }
    }
}

#[test]
fn parse_path_accepts_fill_rule_alternative_key() {
    let v = json!({"subpaths":[{"knots":[[0.0,0.0]]}], "fill_rule":"evenodd"});
    let p = parse_path(&v).unwrap();
    assert!(matches!(p.fill_rule, FillRule::EvenOdd));
}

#[test]
fn parse_path_accepts_op_aliases() {
    for (s, expected) in [
        ("combine", PathOp::Combine),
        ("add", PathOp::Combine),
        ("union", PathOp::Combine),
        ("subtract", PathOp::Subtract),
        ("minus", PathOp::Subtract),
        ("intersect", PathOp::Intersect),
        ("exclude", PathOp::Exclude),
        ("xor", PathOp::Exclude),
        ("join", PathOp::Join),
    ] {
        let v = json!({"subpaths":[{"op": s, "knots":[[0.0,0.0]]}]});
        let p = parse_path(&v).unwrap();
        match expected {
            PathOp::Combine => assert!(matches!(p.subpaths[0].op, PathOp::Combine)),
            PathOp::Subtract => assert!(matches!(p.subpaths[0].op, PathOp::Subtract)),
            PathOp::Intersect => assert!(matches!(p.subpaths[0].op, PathOp::Intersect)),
            PathOp::Exclude => assert!(matches!(p.subpaths[0].op, PathOp::Exclude)),
            PathOp::Join => assert!(matches!(p.subpaths[0].op, PathOp::Join)),
        }
    }
}

#[test]
fn path_json_round_trips() {
    let v = json!({
        "subpaths":[
            {"closed":true,"op":"combine","knots":[[0.0,0.0],[10.0,0.0],[10.0,10.0]]},
            {"closed":false,"op":"subtract","knots":[
                {"anchor":[1.0,1.0],"in":[0.0,0.0],"out":[2.0,2.0],"smooth":true}
            ]}
        ],
        "fillRule":"evenodd",
        "inverted":true
    });
    let p = parse_path(&v).unwrap();
    let j = path_json(&p);
    let p2 = parse_path(&j).unwrap();
    assert_eq!(path_json(&p), path_json(&p2));
}

#[test]
fn path_json_includes_fill_rule_and_inverted() {
    let v = json!({"subpaths":[{"knots":[[0.0,0.0]]}], "fillRule":"evenodd", "inverted":true});
    let p = parse_path(&v).unwrap();
    let j = path_json(&p);
    assert_eq!(j["fillRule"], "evenodd");
    assert_eq!(j["inverted"], true);
}

#[test]
fn shape_path_rect_requires_rect() {
    assert!(shape_path(&json!({"kind":"rect"})).is_err());
}

#[test]
fn shape_path_rect_returns_finite_path() {
    let p = shape_path(&json!({"kind":"rect","rect":[0.0,0.0,20.0,10.0]})).unwrap();
    assert!(!p.subpaths.is_empty());
    for sub in &p.subpaths {
        for k in &sub.knots {
            assert!(k.anchor.x.is_finite());
            assert!(k.anchor.y.is_finite());
            assert!(k.in_ctrl.x.is_finite());
            assert!(k.in_ctrl.y.is_finite());
            assert!(k.out_ctrl.x.is_finite());
            assert!(k.out_ctrl.y.is_finite());
        }
    }
}

#[test]
fn shape_path_ellipse_requires_rect() {
    assert!(shape_path(&json!({"kind":"ellipse"})).is_err());
}

#[test]
fn shape_path_ellipse_returns_path() {
    let p = shape_path(&json!({"kind":"ellipse","rect":[5.0,5.0,15.0,10.0]})).unwrap();
    assert!(!p.subpaths.is_empty());
}

#[test]
fn shape_path_line_requires_from_to() {
    assert!(shape_path(&json!({"kind":"line","from":[0.0,0.0]})).is_err());
    assert!(shape_path(&json!({"kind":"line","to":[10.0,10.0]})).is_err());
}

#[test]
fn shape_path_line_returns_path() {
    let p = shape_path(&json!({"kind":"line","from":[0.0,0.0],"to":[10.0,10.0],"weight":2.0})).unwrap();
    assert!(!p.subpaths.is_empty());
    assert!(p.subpaths[0].knots.len() >= 2);
}

#[test]
fn shape_path_polygon_defaults() {
    let p = shape_path(&json!({"kind":"polygon","rect":[0.0,0.0,10.0,10.0]})).unwrap();
    assert!(!p.subpaths.is_empty());
    assert!(!p.subpaths[0].knots.is_empty());
}

#[test]
fn shape_path_unknown_kind_is_err() {
    assert!(shape_path(&json!({"kind":"spiral","rect":[0.0,0.0,10.0,10.0]})).is_err());
}

#[test]
fn specs_have_unique_ids_and_metadata() {
    let specs = specs();
    let mut ids = HashSet::new();
    for spec in &specs {
        assert!(!spec.id.is_empty());
        assert!(!spec.label.is_empty());
        assert!(!spec.params.is_empty());
        assert!(ids.insert(spec.id), "duplicate spec id {}", spec.id);
    }
}

#[test]
fn specs_include_expected_vector_commands() {
    let specs = specs();
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    for expected in
        ["shape.create", "shape.edit", "path.list", "path.set", "path.toSelection", "layer.vectorMask.add", "select.toWorkPath", "select.convertToShape"]
    {
        assert!(ids.contains(&expected), "missing spec {expected}");
    }
}
