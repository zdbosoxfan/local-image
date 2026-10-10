use photocraft_doc::LayerId;
use photocraft_engine::vp_cmds::{VP, scene, specs};
use photocraft_engine::{EngineError, Session};
use photocraft_geom::Rect;
use serde_json::json;

fn setup_raster_doc(width: u32, height: u32) -> (Session, LayerId) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": width, "height": height})).unwrap();
    s.execute("layer.new.layer", json!({"name": "test"})).unwrap();
    let id = s.active().unwrap().active_layer.unwrap();
    (s, id)
}

#[test]
fn specs_returns_vanishing_point_command() {
    let specs = specs();
    assert!(!specs.is_empty());
    let vp_spec = specs.iter().find(|s| s.id == VP).expect("VP spec missing");
    assert!(!vp_spec.label.is_empty());
    assert!(vp_spec.params.contains("planes"));
    let (s, _) = setup_raster_doc(100, 100);
    assert!(s.is_enabled(VP));
}

#[test]
fn scene_valid_corners_returns_one_plane() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]});
    let sc = scene(&p, canvas).unwrap();
    assert_eq!(sc.planes.len(), 1);
    assert_eq!(sc.planes[0].corners[0], [0.0, 0.0]);
}

#[test]
fn scene_missing_planes_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_empty_planes_array_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": []});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_corners_not_four_points_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0]]}]});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_invalid_edge_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [
        {"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]},
        {"from": 0, "edge": "diagonal"}
    ]});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_from_out_of_bounds_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [
        {"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]},
        {"from": 5, "edge": "top"}
    ]});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_degenerate_plane_returns_bad_params() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [{"corners": [[0.0,0.0],[50.0,0.0],[100.0,0.0],[150.0,0.0]]}]});
    let err = scene(&p, canvas).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn scene_negative_focal_length_ignored() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}], "focalLength": -50.0});
    let sc = scene(&p, canvas).unwrap();
    assert!(sc.focal.is_finite());
    assert_eq!(sc.planes.len(), 1);
}

#[test]
fn full_command_without_document_returns_error() {
    let mut s = Session::new();
    let res = s.execute(VP, json!({}));
    // The command precondition fails with "no document open" -> Disabled.
    assert!(matches!(res, Err(EngineError::Disabled(_, _))));
}

#[test]
fn full_command_missing_planes_returns_bad_params() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let res = s.execute(VP, json!({}));
    assert!(matches!(res, Err(EngineError::BadParams { .. })));
}

#[test]
fn full_command_valid_planes_returns_ok() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = s.execute(VP, json!({"planes": planes})).unwrap();
    assert_eq!(res["planes"].as_array().unwrap().len(), 1);
    assert_eq!(res["pasted"].as_u64().unwrap(), 0);
    assert_eq!(res["dabs"].as_u64().unwrap(), 0);
}

#[test]
fn full_command_new_layer_creates_layer() {
    let (mut s, original_id) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = s.execute(VP, json!({"planes": planes, "newLayer": true})).unwrap();
    let new_id = res["layer"].as_u64().unwrap();
    assert_ne!(new_id, original_id.0);
    let doc = &s.active().unwrap().doc;
    assert!(doc.layer(LayerId(new_id)).is_some());
    assert!(doc.layer(original_id).is_some());
}

#[test]
fn full_command_paste_unknown_layer_errors() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = s.execute(VP, json!({"planes": planes, "paste": [{"plane":0, "layer": 999}]}));
    assert!(matches!(res, Err(EngineError::NoLayer(_))));
}

#[test]
fn full_command_paste_empty_clipboard_errors() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = s.execute(VP, json!({"planes": planes, "paste": [{"plane":0}]}));
    assert!(matches!(res, Err(EngineError::BadParams { .. })));
}

#[test]
fn full_command_clone_no_points_returns_zero_dabs() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = s.execute(VP, json!({"planes": planes, "clone": [{"source": [50,50], "points": []}]})).unwrap();
    assert_eq!(res["dabs"].as_u64().unwrap(), 0);
}

#[test]
fn vp_command_undo_restores_pixels() {
    let (mut s, _) = setup_raster_doc(100, 100);
    // Create source and target layers
    s.execute("layer.new.layer", json!({"name": "source"})).unwrap();
    let source_id = s.active().unwrap().active_layer.unwrap();
    s.execute("layer.new.layer", json!({"name": "target"})).unwrap();
    let target_id = s.active().unwrap().active_layer.unwrap();

    // Fill source with red, target with blue
    s.edit("fill source", |doc, _| {
        let surf = doc.layer_mut(source_id).unwrap().surface_mut().unwrap();
        surf.fill_rect(Rect::new(0, 0, 100, 100), &[1.0, 0.0, 0.0, 1.0]);
        Ok(())
    })
    .unwrap();
    s.edit("fill target", |doc, _| {
        let surf = doc.layer_mut(target_id).unwrap().surface_mut().unwrap();
        surf.fill_rect(Rect::new(0, 0, 100, 100), &[0.0, 0.0, 1.0, 1.0]);
        Ok(())
    })
    .unwrap();

    // Select target
    s.execute("layer.select", json!({"layer": target_id.0})).unwrap();

    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    s.execute(
        VP,
        json!({
            "planes": planes,
            "paste": [{"plane": 0, "layer": source_id.0, "at": [0.5,0.5], "width": 1.0}]
        }),
    )
    .unwrap();
    // Pixels should change to red at center
    let surf = s.active().unwrap().doc.layer(target_id).unwrap().surface().unwrap();
    assert!(surf.rgba(50, 50)[0] > 0.5, "expected red component dominant after paste");

    // Undo should restore blue
    assert!(s.undo());
    let surf = s.active().unwrap().doc.layer(target_id).unwrap().surface().unwrap();
    assert!(surf.rgba(50, 50)[2] > 0.5, "expected blue component dominant after undo");
}

#[test]
fn determinism_same_input_same_output() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res1 = s.execute(VP, json!({"planes": planes.clone()})).unwrap();
    assert!(s.undo());
    let res2 = s.execute(VP, json!({"planes": planes})).unwrap();
    assert_eq!(res1["planes"], res2["planes"]);
    assert_eq!(res1["focal"], res2["focal"]);
}

#[test]
fn scene_tear_off_zero_depth_is_ok() {
    let canvas = Rect::new(0, 0, 100, 100);
    let p = json!({"planes": [
        {"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]},
        {"from": 0, "edge": "top", "depth": 0.0}
    ]});
    // The library accepts depth=0.0 and simply does not extrude a plane.
    let sc = scene(&p, canvas).unwrap();
    assert_eq!(sc.planes.len(), 2);
}

#[test]
fn invalid_clone_param_type_does_not_panic() {
    let (mut s, _) = setup_raster_doc(100, 100);
    let planes = json!([{"corners": [[0.0,0.0],[100.0,0.0],[100.0,100.0],[0.0,100.0]]}]);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        s.execute(
            VP,
            json!({
                "planes": planes,
                "clone": [{"source": ["not", "numbers"], "points": "not an array", "size": "not a number"}]
            }),
        )
    }));
    assert!(res.is_ok(), "VP command panicked on invalid clone params");
}
