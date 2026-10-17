use photocraft_engine::{EngineError, Session, edit_menu_cmds};
use serde_json::json;

#[test]
fn find_matches_empty_text_returns_empty() {
    let m = edit_menu_cmds::find_matches("", "hello", false, false);
    assert!(m.is_empty());
}

#[test]
fn find_matches_no_panic_on_odd_unicode_boundaries() {
    let text = "a\u{0301}abc";
    let m = edit_menu_cmds::find_matches(text, "a", false, false);
    assert!(!m.is_empty());
}

#[test]
fn session_execute_purge_video_cache_no_document_ok() {
    let mut s = Session::new();
    let res = s.execute("edit.purge.videoCache", json!({})).unwrap();
    assert_eq!(res["bytes"], 0);
    assert_eq!(res["message"], "nothing to purge (no video layers)");
}

#[test]
fn session_execute_preset_manager_list_unknown_kind_returns_bad_params() {
    let mut s = Session::new();
    let err = s.execute("edit.presets.presetManager", json!({"action":"list","kind":"unknownKind"})).unwrap_err();
    match err {
        EngineError::BadParams { msg, .. } => assert!(msg.contains("unknown preset kind")),
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn session_execute_preset_manager_delete_custom_shape_missing_index_returns_bad_params() {
    let mut s = Session::new();
    let err = s.execute("edit.presets.presetManager", json!({"action":"delete","kind":"customShapes","index":0})).unwrap_err();
    match err {
        EngineError::BadParams { msg, .. } => assert!(msg.contains("no preset 0")),
        other => panic!("expected BadParams, got {other:?}"),
    }
}

#[test]
fn session_execute_preset_manager_rename_brush_by_index() {
    let mut s = Session::new();
    let list = s.execute("edit.presets.presetManager", json!({"action":"list","kind":"brushes"})).unwrap();
    let brushes = list["brushes"].as_array().unwrap();
    if brushes.is_empty() {
        return;
    }

    let res = s.execute("edit.presets.presetManager", json!({"action":"rename","kind":"brushes","index":0,"newName":"Integration Test Brush"})).unwrap();
    let names = res["brushes"].as_array().unwrap();
    assert!(names.iter().any(|n| n.as_str() == Some("Integration Test Brush")));
}

#[test]
fn session_disabled_reason_fade_no_document() {
    let s = Session::new();
    let reason = s.disabled_reason("edit.fade");
    let reason = reason.expect("fade should be disabled without a document");
    assert!(reason.contains("no document"), "unexpected reason: {reason}");
}

#[test]
fn session_disabled_reason_purge_clipboard_empty() {
    let s = Session::new();
    let reason = s.disabled_reason("edit.purge.clipboard");
    let reason = reason.expect("purge clipboard should be disabled when empty");
    assert!(reason.contains("clipboard is empty"), "unexpected reason: {reason}");
}

#[test]
fn session_is_enabled_export_import_presets_true() {
    let s = Session::new();
    assert!(s.is_enabled("edit.presets.exportImportPresets"));
}

#[test]
fn session_is_enabled_find_replace_no_document_false() {
    let s = Session::new();
    assert!(!s.is_enabled("edit.findAndReplaceText"));
}

#[test]
fn preset_manager_move_missing_preset_returns_bad_params() {
    let mut s = Session::new();
    let err = s.execute("edit.presets.presetManager", json!({"action":"move","kind":"customShapes","index":0,"to":999})).unwrap_err();
    match err {
        EngineError::BadParams { msg, .. } => assert!(msg.contains("no preset 0")),
        other => panic!("expected BadParams, got {other:?}"),
    }
}
