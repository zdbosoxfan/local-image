use photocraft_engine::commands::{adjustment_from_params, blend_from_str, command_specs, find};
use photocraft_engine::{EngineError, Session};
use serde_json::json;

#[test]
fn command_specs_contains_core_ids() {
    let ids: Vec<_> = command_specs().iter().map(|c| c.id).collect();
    for expected in ["file.new", "edit.undo", "edit.redo", "layer.new.layer", "image.adjustments.invert", "paint.stroke", "command.list"] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
}

#[test]
fn command_specs_ids_are_unique() {
    let specs = command_specs();
    let mut seen = std::collections::HashSet::new();
    for spec in specs {
        assert!(seen.insert(spec.id), "duplicate id {}", spec.id);
    }
}

#[test]
fn find_returns_spec_for_known_id() {
    let spec = find("file.new").expect("file.new should exist");
    assert_eq!(spec.id, "file.new");
    assert!(spec.label.contains("New"));
    assert!(!spec.menu.is_empty());
}

#[test]
fn find_returns_none_for_unknown_id() {
    assert!(find("not.a.real.command").is_none());
}

#[test]
fn blend_from_str_accepts_common_modes() {
    for name in ["normal", "Normal", "NORMAL", "multiply", "screen", "overlay", "soft-light", "soft_light", "soft light", "pass through", "PassThrough"] {
        assert!(blend_from_str(name).is_some(), "should parse {name}");
    }
}

#[test]
fn blend_from_str_rejects_unknown() {
    assert!(blend_from_str("").is_none());
    assert!(blend_from_str("garbage").is_none());
    assert!(blend_from_str("not a blend mode").is_none());
}

#[test]
fn adjustment_from_params_invert_returns_invert() {
    let adj = adjustment_from_params("invert", &json!({}));
    assert!(matches!(adj, photocraft_engine::doc::Adjustment::Invert));
}

#[test]
fn adjustment_from_params_unknown_kind_does_not_panic() {
    let result = std::panic::catch_unwind(|| adjustment_from_params("not_a_kind", &json!({"foo": true})));
    assert!(result.is_ok());
}

#[test]
fn always_enabled_commands_succeed_without_document() {
    let s = Session::new();
    for id in ["file.new", "tools.setColors", "tools.swapColors", "tools.defaultColors", "session.inspect", "command.list"] {
        if let Some(spec) = find(id) {
            assert!((spec.enabled)(&s).is_ok(), "{} should be always enabled", id);
        }
    }
}

#[test]
fn has_doc_enabled_requires_document() {
    let mut s = Session::new();
    let spec = find("file.close").unwrap();
    assert!((spec.enabled)(&s).is_err());
    s.execute("file.new", json!({"width": 2, "height": 2})).unwrap();
    assert!((spec.enabled)(&s).is_ok());
}

#[test]
fn can_undo_enabled_tracks_history() {
    let mut s = Session::new();
    let spec = find("edit.undo").unwrap();
    assert!((spec.enabled)(&s).is_err()); // no document
    s.execute("file.new", json!({"width": 1, "height": 1})).unwrap();
    assert!((spec.enabled)(&s).is_err()); // no history to undo
    s.execute("layer.new.layer", json!({})).unwrap();
    assert!((spec.enabled)(&s).is_ok());
}

#[test]
fn run_file_new_creates_document_with_black_background() {
    let mut s = Session::new();
    let result = s.execute("file.new", json!({"width": 1, "height": 1, "background": "black"})).unwrap();
    assert!(result.get("document").is_some());
    assert_eq!(s.documents().len(), 1);
    assert_eq!(s.active_index(), Some(0));

    let px = s.execute("document.pixel", json!({"x": 0, "y": 0})).unwrap();
    let arr = px.as_array().unwrap();
    assert!(arr.len() >= 4);
    assert!(arr[0].as_f64().unwrap() < 0.01);
    assert!(arr[1].as_f64().unwrap() < 0.01);
    assert!(arr[2].as_f64().unwrap() < 0.01);
    assert!((arr[3].as_f64().unwrap() - 1.0).abs() < 0.01);
}

#[test]
fn run_file_new_float_size_rounds() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 1.2, "height": 2.8, "background": "white"})).unwrap();
    let bounds = s.active().unwrap().doc.bounds();
    assert_eq!(bounds.width(), 1, "1.2 rounds to 1");
    assert_eq!(bounds.height(), 3, "2.8 rounds to 3");
}

#[test]
fn run_file_new_zero_dimension_clamps_to_one() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 0.0, "height": 0.0})).unwrap();
    let bounds = s.active().unwrap().doc.bounds();
    assert_eq!(bounds.width(), 1);
    assert_eq!(bounds.height(), 1);
}

#[test]
fn execute_unknown_command_returns_unknown_command_error() {
    let mut s = Session::new();
    let err = s.execute("not.a.command", json!({})).unwrap_err();
    assert!(matches!(err, EngineError::UnknownCommand(_)));
}

#[test]
fn malformed_params_return_err_not_panic() {
    let mut s = Session::new();
    // First create a document so commands requiring one are enabled.
    s.execute("file.new", json!({"width": 4, "height": 4})).unwrap();

    let err = s.execute("document.activate", json!({})).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));

    let err = s.execute("select.rect", json!({"x": 0})).unwrap_err();
    assert!(matches!(err, EngineError::BadParams { .. }));
}

#[test]
fn tools_set_colors_swaps_and_defaults() {
    let mut s = Session::new();
    let fg = [0.25, 0.5, 0.75, 1.0];
    let bg = [0.0, 0.25, 0.5, 1.0];
    s.execute("tools.setColors", json!({"foreground": fg, "background": bg})).unwrap();
    assert_eq!(s.tools.foreground, fg);
    assert_eq!(s.tools.background, bg);

    s.execute("tools.swapColors", json!({})).unwrap();
    assert_eq!(s.tools.foreground, bg);
    assert_eq!(s.tools.background, fg);

    s.execute("tools.defaultColors", json!({})).unwrap();
    assert_eq!(s.tools.foreground, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(s.tools.background, [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn tools_set_colors_empty_params_is_no_op() {
    let mut s = Session::new();
    let before_fg = s.tools.foreground;
    let before_bg = s.tools.background;
    s.execute("tools.setColors", json!({})).unwrap();
    assert_eq!(s.tools.foreground, before_fg);
    assert_eq!(s.tools.background, before_bg);
}

#[test]
fn undo_redo_layer_new_round_trip() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 2, "height": 2})).unwrap();
    let rev_before = s.active().unwrap().revision;

    let layer_result = s.execute("layer.new.layer", json!({})).unwrap();
    let rev_after_new = s.active().unwrap().revision;
    assert!(rev_after_new > rev_before);
    assert!(layer_result.get("layer").is_some());

    assert!(s.undo());
    let rev_after_undo = s.active().unwrap().revision;
    assert!(rev_after_undo > rev_after_new);

    assert!(s.redo());
    let rev_after_redo = s.active().unwrap().revision;
    assert!(rev_after_redo > rev_after_undo);
}

#[test]
fn select_all_deselect_updates_enabled() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 4, "height": 4})).unwrap();
    let deselect_spec = find("select.deselect").unwrap();

    assert!((deselect_spec.enabled)(&s).is_err()); // no selection yet
    s.execute("select.all", json!({})).unwrap();
    assert!((deselect_spec.enabled)(&s).is_ok());
    s.execute("select.deselect", json!({})).unwrap();
    assert!((deselect_spec.enabled)(&s).is_err());
}

#[test]
fn document_activate_errors() {
    let mut s = Session::new();

    // No document: command is disabled.
    let err = s.execute("document.activate", json!({"document": 0})).unwrap_err();
    assert!(matches!(err, EngineError::Disabled(..)));

    // With a document but out-of-range index: run returns NoDocument.
    s.execute("file.new", json!({"width": 2, "height": 2})).unwrap();
    let err = s.execute("document.activate", json!({"document": 999})).unwrap_err();
    assert!(matches!(err, EngineError::NoDocument));
}

#[test]
fn file_new_default_white_background_pixel_reads_white() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 1, "height": 1})).unwrap();
    let px = s.execute("document.pixel", json!({"x": 0, "y": 0})).unwrap();
    let arr = px.as_array().unwrap();
    assert!(arr.len() >= 3);
    for i in 0..3 {
        assert!((arr[i].as_f64().unwrap() - 1.0).abs() < 0.01);
    }
    assert!((arr[3].as_f64().unwrap() - 1.0).abs() < 0.01);
}

#[test]
fn document_pixel_far_out_returns_transparent() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 2, "height": 2})).unwrap();
    let px = s.execute("document.pixel", json!({"x": 1000, "y": 1000})).unwrap();
    let arr = px.as_array().unwrap();
    assert!(arr.len() >= 4);
    assert!(arr[3].as_f64().unwrap() < 0.01);
}
