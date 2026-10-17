use photocraft_engine::{Session, slice_cmds};
use serde_json::json;

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
