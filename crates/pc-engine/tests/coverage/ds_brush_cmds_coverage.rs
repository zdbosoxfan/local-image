use photocraft_engine::brush_cmds;
use photocraft_engine::{BrushSettings, Session};
use serde_json::json;

#[test]
fn parse_points_accepts_array_form() {
    let p = json!({
        "points": [
            [1.0, 2.0, 0.5, 0.1, -0.2, 0.3, 4.0, 1.0],
            [3.0, 4.0]
        ]
    });
    let pts = brush_cmds::parse_points(&p, "test").unwrap();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0].x, 1.0);
    assert_eq!(pts[0].y, 2.0);
    assert_eq!(pts[0].pressure, 0.5);
    assert_eq!(pts[0].tilt_x, 0.1);
    assert_eq!(pts[0].tilt_y, -0.2);
    assert_eq!(pts[0].rotation, 0.3);
    assert_eq!(pts[0].time, 4.0);
    assert_eq!(pts[0].wheel, 1.0);
    assert_eq!(pts[1].x, 3.0);
    assert_eq!(pts[1].y, 4.0);
    assert_eq!(pts[1].pressure, 1.0);
}

#[test]
fn parse_points_accepts_object_form() {
    let p = json!({
        "points": [
            {"x": 10.0, "y": 20.0, "pressure": 0.7, "tiltX": 0.2, "tiltY": -0.1, "rotation": 0.9, "time": 123.0, "wheel": 2.0}
        ]
    });
    let pts = brush_cmds::parse_points(&p, "test").unwrap();
    assert_eq!(pts.len(), 1);
    assert_eq!(pts[0].x, 10.0);
    assert_eq!(pts[0].y, 20.0);
    assert_eq!(pts[0].pressure, 0.7);
    assert_eq!(pts[0].tilt_x, 0.2);
    assert_eq!(pts[0].tilt_y, -0.1);
    assert_eq!(pts[0].rotation, 0.9);
    assert_eq!(pts[0].time, 123.0);
    assert_eq!(pts[0].wheel, 2.0);
}

#[test]
fn parse_points_rejects_missing_points() {
    let p = json!({});
    assert!(brush_cmds::parse_points(&p, "test").is_err());
}

#[test]
fn parse_points_rejects_empty_points() {
    let p = json!({"points": []});
    assert!(brush_cmds::parse_points(&p, "test").is_err());
}

#[test]
fn parse_points_rejects_too_few_elements() {
    let p = json!({"points": [[1.0]]});
    assert!(brush_cmds::parse_points(&p, "test").is_err());
}

#[test]
fn parse_points_rejects_huge_coordinate() {
    let p = json!({"points": [[1_000_001.0, 0.0]]});
    assert!(brush_cmds::parse_points(&p, "test").is_err());
}

#[test]
fn parse_points_rejects_non_finite_json_becomes_null_and_errors() {
    // NaN cannot be represented in JSON; serde_json turns it into Null,
    // which parse_points sees as a missing coordinate and skips the point,
    // leaving an empty list -> Err.
    let p = json!({"points": [[f64::NAN, 0.0]]});
    assert!(brush_cmds::parse_points(&p, "test").is_err());
}

#[test]
fn merge_brush_deep_merges_nested_objects() {
    let base = BrushSettings::default();
    let patch = json!({"smoothing": {"amount": 0.75}});
    let merged = brush_cmds::merge_brush(&base, &patch, "test").unwrap();
    assert_eq!(merged.smoothing.amount, 0.75);
}

#[test]
fn merge_brush_rejects_invalid_tip() {
    let base = BrushSettings::default();
    let patch = json!({"tip": "not_a_tip"});
    assert!(brush_cmds::merge_brush(&base, &patch, "test").is_err());
}

#[test]
fn resolve_brush_uses_session_foreground_by_default() {
    let mut s = Session::new();
    s.tools.foreground = [0.1, 0.2, 0.3, 1.0];
    let brush = brush_cmds::resolve_brush(&s, &json!({}), "test").unwrap();
    assert_eq!(brush.color, [0.1, 0.2, 0.3, 1.0]);
}

#[test]
fn resolve_brush_applies_scalar_overrides() {
    let s = Session::new();
    let p = json!({
        "size": 100.0,
        "hardness": 0.7,
        "opacity": 0.5,
        "flow": 0.2,
        "spacing": 0.5,
        "smoothing": 0.3,
        "color": "#ff0000"
    });
    let brush = brush_cmds::resolve_brush(&s, &p, "test").unwrap();
    assert_eq!(brush.size, 100.0);
    assert_eq!(brush.hardness, 0.7);
    assert_eq!(brush.opacity, 0.5);
    assert_eq!(brush.flow, 0.2);
    assert_eq!(brush.spacing, 0.5);
    assert_eq!(brush.smoothing.amount, 0.3);
    assert_eq!(brush.color, [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn resolve_brush_clamps_scalar_ranges() {
    let s = Session::new();
    let p = json!({
        "hardness": -1.0,
        "opacity": 2.0,
        "flow": -0.5,
        "spacing": 20.0,
        "smoothing": 2.0
    });
    let brush = brush_cmds::resolve_brush(&s, &p, "test").unwrap();
    assert_eq!(brush.hardness, 0.0);
    assert_eq!(brush.opacity, 1.0);
    assert_eq!(brush.flow, 0.0);
    assert_eq!(brush.spacing, 10.0);
    assert_eq!(brush.smoothing.amount, 1.0);
}

#[test]
fn resolve_brush_ignores_non_numeric_size() {
    // JSON null (e.g. from NaN) is not a number, so it is ignored.
    let s = Session::new();
    let p = json!({"size": null});
    let brush = brush_cmds::resolve_brush(&s, &p, "test").unwrap();
    assert_eq!(brush.size, s.tools.brush.size);
}

#[test]
fn resolve_brush_unknown_preset_errors() {
    let s = Session::new();
    let p = json!({"preset": "nonexistent"});
    assert!(brush_cmds::resolve_brush(&s, &p, "test").is_err());
}

#[test]
fn brush_patch_no_diff_is_empty_object() {
    let old = BrushSettings::default();
    let new = BrushSettings::default();
    let patch = brush_cmds::brush_patch(&old, &new);
    assert_eq!(patch, json!({}));
}

#[test]
fn brush_patch_diffs_top_level_fields() {
    let old = BrushSettings::default();
    let new = brush_cmds::merge_brush(&old, &json!({"size": 25.0, "opacity": 0.6}), "test").unwrap();
    let patch = brush_cmds::brush_patch(&old, &new);
    assert_eq!(patch.get("size").unwrap(), 25.0);
    let opacity = patch.get("opacity").unwrap().as_f64().unwrap();
    assert!((opacity - 0.6_f64).abs() < 1e-6);
    assert_eq!(patch.as_object().unwrap().len(), 2);
}

#[test]
fn brush_patch_diffs_nested_objects() {
    let old = BrushSettings::default();
    let new = brush_cmds::merge_brush(&old, &json!({"smoothing": {"amount": 0.9}}), "test").unwrap();
    let patch = brush_cmds::brush_patch(&old, &new);
    let amount = patch["smoothing"]["amount"].as_f64().unwrap();
    assert!((amount - 0.9_f64).abs() < 1e-6);
}

#[test]
fn coalesce_journal_folds_consecutive_set_brush() {
    let mut s = Session::new();
    let start = brush_cmds::merge_brush(&BrushSettings::default(), &json!({"size": 10.0}), "test").unwrap();
    let current = brush_cmds::merge_brush(&start, &json!({"size": 20.0}), "test").unwrap();

    s.tools.brush_gesture = Some(("drag".to_string(), start.clone()));
    s.journal.push(("tools.setBrush".to_string(), json!({"coalesce": "drag", "size": 10.0})));
    s.tools.brush = current;

    let params = json!({"coalesce": "drag"});
    assert!(brush_cmds::coalesce_journal(&mut s, "tools.setBrush", &params));

    let (id, lp) = &s.journal[0];
    assert_eq!(id, "tools.setBrush");
    assert_eq!(lp["coalesce"], "drag");
    assert_eq!(lp["brush"]["size"], 20.0);
}

#[test]
fn tools_set_brush_updates_session_and_journal() {
    let mut s = Session::new();
    let res = s.execute("tools.setBrush", json!({"size": 15.0, "hardness": 0.3}));
    assert!(res.is_ok());
    assert_eq!(s.tools.brush.size, 15.0);
    assert_eq!(s.tools.brush.hardness, 0.3);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.journal[0].0, "tools.setBrush");
}

#[test]
fn tools_set_brush_reset_restores_defaults() {
    let mut s = Session::new();
    s.tools.brush.size = 99.0;
    let res = s.execute("tools.setBrush", json!({"reset": true}));
    assert!(res.is_ok());
    assert_eq!(s.tools.brush.size, BrushSettings::default().size);
}

#[test]
fn tools_set_brush_applies_preset_by_name() {
    let mut s = Session::new();
    s.execute("brush.presets.save", json!({"name": "P1", "brush": {"size": 42.0}})).unwrap();
    let res = s.execute("tools.setBrush", json!({"preset": "P1"}));
    assert!(res.is_ok());
    assert_eq!(s.tools.brush.size, 42.0);
}

#[test]
fn brush_presets_save_and_delete_roundtrip() {
    let mut s = Session::new();
    let name = "RoundtripPreset";
    s.execute("brush.presets.save", json!({"name": name, "brush": {"size": 5.0}})).unwrap();

    let list = s.execute("brush.presets.list", json!({})).unwrap();
    let presets = list["presets"].as_array().unwrap();
    assert!(presets.iter().any(|p| p["name"] == name));

    s.execute("brush.presets.delete", json!({"name": name})).unwrap();
    assert!(s.execute("brush.presets.delete", json!({"name": name})).is_err());
    let list_after = s.execute("brush.presets.list", json!({})).unwrap();
    let presets_after = list_after["presets"].as_array().unwrap();
    assert!(!presets_after.iter().any(|p| p["name"] == name));
}

#[test]
fn brush_presets_delete_missing_errors() {
    let mut s = Session::new();
    let res = s.execute("brush.presets.delete", json!({"name": "NoSuchPreset"}));
    assert!(res.is_err());
}

#[test]
fn brush_presets_list_full_contains_brush_objects() {
    let mut s = Session::new();
    let res = s.execute("brush.presets.list", json!({"full": true})).unwrap();
    let presets = res["presets"].as_array().unwrap();
    assert!(!presets.is_empty());
    assert!(presets[0].get("brush").is_some());
}

#[test]
fn brush_get_returns_default_brush_json() {
    let mut s = Session::new();
    let res = s.execute("brush.get", json!({})).unwrap();
    assert!(res.is_object());
    assert!(res.get("size").is_some());
}

#[test]
fn paint_stroke_without_document_returns_error() {
    let mut s = Session::new();
    let res = s.execute("paint.pencil", json!({"points": [[0.0, 0.0]]}));
    assert!(res.is_err());
}
