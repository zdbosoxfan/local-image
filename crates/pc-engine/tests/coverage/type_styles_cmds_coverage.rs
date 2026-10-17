use photocraft_engine::{EngineError, Session, type_styles_cmds};
use serde_json::json;

fn style_specs() -> Vec<photocraft_engine::CommandSpec> {
    type_styles_cmds::specs()
}

#[test]
fn all_style_commands_disabled_without_doc() {
    let session = Session::new();
    for spec in style_specs() {
        let reason = session.disabled_reason(spec.id);
        assert_eq!(reason.as_deref(), Some("no document open"), "spec {} disabled reason mismatch", spec.id);
    }
}

#[test]
fn run_all_style_commands_without_doc_returns_no_document() {
    let mut session = Session::new();
    for spec in style_specs() {
        let result = (spec.run)(&mut session, &json!({}));
        match result {
            Err(EngineError::NoDocument) => {}
            other => panic!("run {} without doc should return NoDocument, got {:?}", spec.id, other),
        }
    }
}
