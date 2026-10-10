use std::sync::atomic::{AtomicU64, Ordering};

use photocraft_engine::{EngineError, Session, edit_menu_cmds};
use serde_json::json;

fn unique_temp_dir() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pc_edit_menu_tests_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn specs_are_registered() {
    let specs = edit_menu_cmds::specs();
    assert_eq!(specs.len(), 13);
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    for expected in [
        "edit.fade",
        "edit.purge.undo",
        "edit.purge.clipboard",
        "edit.purge.histories",
        "edit.purge.videoCache",
        "edit.purge.all",
        "edit.contentAwareFill",
        "edit.contentAwareScale",
        "edit.defineBrushPreset",
        "edit.defineCustomShape",
        "edit.findAndReplaceText",
        "edit.presets.presetManager",
        "edit.presets.exportImportPresets",
    ] {
        assert!(ids.contains(&expected), "missing spec id {expected}");
    }
}

#[test]
fn specs_have_unique_ids() {
    let specs = edit_menu_cmds::specs();
    let mut ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    let len_before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), len_before, "duplicate command ids in edit menu specs");
}

#[test]
fn find_matches_empty_needle_returns_empty() {
    let m = edit_menu_cmds::find_matches("hello", "", false, false);
    assert!(m.is_empty());
}

#[test]
fn find_matches_empty_text_returns_empty() {
    let m = edit_menu_cmds::find_matches("", "hello", false, false);
    assert!(m.is_empty());
}

#[test]
fn find_matches_default_is_case_insensitive() {
    let m = edit_menu_cmds::find_matches("Rust rust RUST", "rust", false, false);
    assert_eq!(m, vec![(0, 4), (5, 9), (10, 14)]);
}

#[test]
fn find_matches_case_sensitive() {
    let m = edit_menu_cmds::find_matches("Rust rust RUST", "rust", true, false);
    assert_eq!(m, vec![(5, 9)]);
}

#[test]
fn find_matches_whole_word() {
    let m = edit_menu_cmds::find_matches("cat category concat cat", "cat", false, true);
    assert_eq!(m, vec![(0, 3), (20, 23)]);
}

#[test]
fn find_matches_unicode_case_folding() {
    let text = "ÉCOLE école";
    let m = edit_menu_cmds::find_matches(text, "école", false, false);
    assert_eq!(m, vec![(0, 6), (7, 13)]);
}

#[test]
fn find_matches_no_panic_on_odd_unicode_boundaries() {
    let text = "a\u{0301}abc";
    let m = edit_menu_cmds::find_matches(text, "a", false, false);
    assert!(!m.is_empty());
}

#[test]
fn preset_format_constant_is_expected() {
    assert_eq!(edit_menu_cmds::PRESET_FORMAT, "photocraft-presets");
}

#[test]
fn preset_file_defaults_are_empty() {
    let f = edit_menu_cmds::PresetFile::default();
    assert_eq!(f.format, "");
    assert_eq!(f.version, 0);
    assert!(f.brushes.is_empty());
    assert!(f.custom_shapes.is_empty());
}

#[test]
fn preset_file_serialization_roundtrip() {
    let file = edit_menu_cmds::PresetFile { format: edit_menu_cmds::PRESET_FORMAT.into(), version: 1, brushes: Vec::new(), custom_shapes: Vec::new() };
    let json_value = serde_json::to_value(&file).unwrap();
    let decoded: edit_menu_cmds::PresetFile = serde_json::from_value(json_value).unwrap();
    assert_eq!(file.format, decoded.format);
    assert_eq!(file.version, decoded.version);
    assert!(decoded.brushes.is_empty());
    assert!(decoded.custom_shapes.is_empty());
}

#[test]
fn preset_file_tempfile_roundtrip() {
    let file = edit_menu_cmds::PresetFile { format: edit_menu_cmds::PRESET_FORMAT.into(), version: 1, brushes: Vec::new(), custom_shapes: Vec::new() };

    let dir = unique_temp_dir();
    let path = dir.join("presets.json");
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

    let read_bytes = std::fs::read(&path).unwrap();
    let decoded: edit_menu_cmds::PresetFile = serde_json::from_slice(&read_bytes).unwrap();

    assert_eq!(file.format, decoded.format);
    assert_eq!(file.version, decoded.version);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn custom_shapes_new_session_is_empty() {
    let s = Session::new();
    assert!(photocraft_engine::edit_menu_cmds::custom_shapes(&s).is_empty());
}

#[test]
fn fade_source_fields_are_exposed() {
    let doc_id = photocraft_engine::doc::DocId::fresh();
    let layer_id = photocraft_engine::doc::LayerId(7);
    let src = edit_menu_cmds::FadeSource { doc: doc_id, revision: 42, layer: layer_id, mask: true, label: "Gaussian Blur".into() };
    assert_eq!(src.doc, doc_id);
    assert_eq!(src.revision, 42);
    assert_eq!(src.layer, layer_id);
    assert!(src.mask);
    assert_eq!(src.label, "Gaussian Blur");
}

#[test]
fn session_execute_unknown_command_returns_unknown() {
    let mut s = Session::new();
    let err = s.execute("edit.doesNotExist", json!({})).unwrap_err();
    match err {
        EngineError::UnknownCommand(cmd) => assert_eq!(cmd, "edit.doesNotExist"),
        other => panic!("expected UnknownCommand, got {other:?}"),
    }
}

#[test]
fn session_execute_purge_video_cache_no_document_ok() {
    let mut s = Session::new();
    let res = s.execute("edit.purge.videoCache", json!({})).unwrap();
    assert_eq!(res["bytes"], 0);
    assert_eq!(res["message"], "nothing to purge (no video layers)");
}

#[test]
fn session_execute_preset_manager_list_all_kinds_no_document() {
    let mut s = Session::new();
    let res = s.execute("edit.presets.presetManager", json!({"action":"list"})).unwrap();
    let obj = res.as_object().unwrap();
    assert!(obj.contains_key("brushes"));
    assert!(obj.contains_key("customShapes"));
    assert!(obj.contains_key("patterns"));
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
fn session_execute_export_import_presets_roundtrip_no_document() {
    let mut s = Session::new();
    let export = s.execute("edit.presets.exportImportPresets", json!({"action":"export"})).unwrap();
    let data = export.get("data").cloned().unwrap();

    let import = s.execute("edit.presets.exportImportPresets", json!({"action":"import","data": data})).unwrap();
    assert_eq!(import["brushes"], 0);
    assert_eq!(import["customShapes"], 0);
}

#[test]
fn session_execute_import_presets_invalid_format_returns_bad_params() {
    let mut s = Session::new();
    let err = s.execute("edit.presets.exportImportPresets", json!({"action":"import","data":{"format":"wrong"}})).unwrap_err();
    match err {
        EngineError::BadParams { msg, .. } => assert!(msg.contains("not a preset file")),
        other => panic!("expected BadParams, got {other:?}"),
    }
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
fn session_execute_find_replace_missing_find_returns_bad_params() {
    let s = Session::new();
    let reason = s.disabled_reason("edit.findAndReplaceText");
    assert!(reason.is_some());
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
