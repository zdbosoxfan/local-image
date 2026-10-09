use photocraft_engine::{EngineError, Session, slice_cmds};
use serde_json::json;

#[test]
fn specs_contains_all_expected_commands() {
    let specs = slice_cmds::specs();
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    assert!(ids.contains(&"slice.new"));
    assert!(ids.contains(&"slice.fromGuides"));
    assert!(ids.contains(&"slice.set"));
    assert!(ids.contains(&"slice.promote"));
    assert!(ids.contains(&"slice.delete"));
    assert!(ids.contains(&"slice.divide"));
    assert!(ids.contains(&"slice.list"));
    assert!(ids.contains(&"layer.newLayerBasedSlice"));
    assert!(ids.contains(&"view.lockSlices"));
    assert!(ids.contains(&"view.clearSlices"));
    assert_eq!(specs.len(), 10);
}

#[test]
fn specs_have_unique_ids() {
    let specs = slice_cmds::specs();
    let mut seen = std::collections::HashSet::new();
    for s in &specs {
        assert!(seen.insert(s.id), "duplicate id: {}", s.id);
    }
}

#[test]
fn specs_params_are_non_empty() {
    for s in &slice_cmds::specs() {
        assert!(!s.params.trim().is_empty(), "params for {} is empty", s.id);
    }
}

#[test]
fn enabled_functions_return_err_for_no_document() {
    let session = Session::default();
    let specs = slice_cmds::specs();
    for s in &specs {
        let result = (s.enabled)(&session);
        assert!(result.is_err(), "expected Err for {} without doc, got {:?}", s.id, result);
    }
}

#[test]
fn specs_run_functions_do_not_panic_with_default_session_and_null_params() {
    let specs = slice_cmds::specs();
    for s in &specs {
        let mut session = Session::default();
        let params = serde_json::Value::Null;
        let _ = (s.run)(&mut session, &params);
    }
}

#[test]
fn slice_list_has_journal_false_others_true() {
    let specs = slice_cmds::specs();
    let list_spec = specs.iter().find(|s| s.id == "slice.list").expect("slice.list spec");
    assert!(!list_spec.journal);
    for s in specs.iter().filter(|s| s.id != "slice.list") {
        assert!(s.journal, "unexpected journal false for {}", s.id);
    }
}

#[test]
fn session_execute_slice_new_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.new", json!({}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_list_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.list", json!({}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_delete_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.delete", json!({"slice": 1}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_divide_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.divide", json!({"slice": 1, "horizontal": 2}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_set_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.set", json!({"slice": 1}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_promote_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.promote", json!({"slice": 1}));
    assert!(result.is_err());
}

#[test]
fn session_execute_slice_from_guides_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("slice.fromGuides", json!({}));
    assert!(result.is_err());
}

#[test]
fn session_execute_layer_new_layer_based_slice_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("layer.newLayerBasedSlice", json!({}));
    assert!(result.is_err());
}

#[test]
fn session_execute_view_clear_slices_without_doc_returns_error() {
    let mut session = Session::default();
    let result = session.execute("view.clearSlices", json!({}));
    assert!(result.is_err());
}

#[test]
fn session_execute_unknown_slice_command_returns_unknown_command() {
    let mut session = Session::default();
    let result = session.execute("slice.doesNotExist", json!({}));
    assert!(matches!(result, Err(EngineError::UnknownCommand(_))));
}

#[test]
fn lock_run_toggles_without_needing_document() {
    let mut session = Session::default();
    let lock_spec = slice_cmds::specs().into_iter().find(|s| s.id == "view.lockSlices").expect("lock spec");
    let result = (lock_spec.run)(&mut session, &json!({}));
    assert!(result.is_ok());
    assert!(session.file_menu.slices_locked);
    let result = (lock_spec.run)(&mut session, &json!({}));
    assert!(result.is_ok());
    assert!(!session.file_menu.slices_locked);
}
