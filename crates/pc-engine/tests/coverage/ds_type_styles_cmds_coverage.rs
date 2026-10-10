use photocraft_engine::{EngineError, Session, command_specs, type_styles_cmds};
use serde_json::json;

fn style_specs() -> Vec<photocraft_engine::CommandSpec> {
    type_styles_cmds::specs()
}

#[test]
fn style_spec_count_is_18() {
    assert_eq!(style_specs().len(), 18);
}

#[test]
fn style_spec_ids_are_stable_and_prefixed() {
    let specs = style_specs();
    let char_ids = [
        "type.characterStyle.new",
        "type.characterStyle.duplicate",
        "type.characterStyle.delete",
        "type.characterStyle.rename",
        "type.characterStyle.set",
        "type.characterStyle.apply",
        "type.characterStyle.redefine",
        "type.characterStyle.clearOverride",
        "type.characterStyle.list",
    ];
    let para_ids = [
        "type.paragraphStyle.new",
        "type.paragraphStyle.duplicate",
        "type.paragraphStyle.delete",
        "type.paragraphStyle.rename",
        "type.paragraphStyle.set",
        "type.paragraphStyle.apply",
        "type.paragraphStyle.redefine",
        "type.paragraphStyle.clearOverride",
        "type.paragraphStyle.list",
    ];
    for id in char_ids.iter().chain(para_ids.iter()) {
        assert!(specs.iter().any(|s| &s.id == id), "missing spec {id}");
    }
}

#[test]
fn all_style_specs_have_non_empty_labels() {
    for spec in style_specs() {
        assert!(!spec.label.is_empty(), "spec {} has empty label", spec.id);
    }
}

#[test]
fn all_style_specs_params_mention_targeting() {
    for spec in style_specs() {
        let p = spec.params;
        assert!(p.contains("range") || p.contains("layer") || p.contains("id"), "spec {} params missing target info: {p}", spec.id);
    }
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

#[test]
fn execute_all_style_commands_without_doc_returns_disabled() {
    let mut session = Session::new();
    for spec in style_specs() {
        let result = session.execute(spec.id, json!({}));
        match result {
            Err(EngineError::Disabled(id, reason)) if id == spec.id && reason.contains("no document open") => {}
            other => panic!("execute {} without doc should return Disabled, got {:?}", spec.id, other),
        }
    }
}

#[test]
fn unknown_style_command_returns_unknown() {
    let mut session = Session::new();
    let result = session.execute("type.characterStyle.doesNotExist", json!({}));
    assert!(matches!(result, Err(EngineError::UnknownCommand(_))));
}

#[test]
fn character_style_list_journal_false() {
    let specs = style_specs();
    let char_list_journal = match specs.iter().find(|s| s.id == "type.characterStyle.list") {
        Some(s) => s.journal,
        None => panic!("missing characterStyle.list"),
    };
    assert!(!char_list_journal);
    for spec in &specs {
        if spec.id != "type.characterStyle.list" && spec.id != "type.paragraphStyle.list" {
            assert!(spec.journal, "spec {} should journal true", spec.id);
        }
    }
}

#[test]
fn paragraph_style_list_journal_false() {
    let specs = style_specs();
    let para_list_journal = match specs.iter().find(|s| s.id == "type.paragraphStyle.list") {
        Some(s) => s.journal,
        None => panic!("missing paragraphStyle.list"),
    };
    assert!(!para_list_journal);
    for spec in &specs {
        if spec.id != "type.characterStyle.list" && spec.id != "type.paragraphStyle.list" {
            assert!(spec.journal, "spec {} should journal true", spec.id);
        }
    }
}

#[test]
fn enabled_function_for_character_new_requires_doc() {
    let specs = style_specs();
    let spec = match specs.iter().find(|s| s.id == "type.characterStyle.new") {
        Some(s) => s,
        None => panic!("missing characterStyle.new"),
    };
    let session = Session::new();
    let res = (spec.enabled)(&session);
    assert_eq!(res.err().as_deref(), Some("no document open"));
}

#[test]
fn enabled_function_for_paragraph_new_requires_doc() {
    let specs = style_specs();
    let spec = match specs.iter().find(|s| s.id == "type.paragraphStyle.new") {
        Some(s) => s,
        None => panic!("missing paragraphStyle.new"),
    };
    let session = Session::new();
    let res = (spec.enabled)(&session);
    assert_eq!(res.err().as_deref(), Some("no document open"));
}

#[test]
fn style_commands_registered_globally() {
    let all_specs = command_specs();
    let all_ids: Vec<&str> = all_specs.iter().map(|s| s.id).collect();
    for spec in style_specs() {
        assert!(all_ids.contains(&spec.id), "style command {} not registered globally", spec.id);
    }
}

#[test]
fn specs_include_apply_and_set_for_both_kinds() {
    let specs = style_specs();
    for prefix in ["type.characterStyle", "type.paragraphStyle"] {
        let apply_id = format!("{prefix}.apply");
        let set_id = format!("{prefix}.set");
        assert!(specs.iter().any(|s| s.id == apply_id), "missing {apply_id}");
        assert!(specs.iter().any(|s| s.id == set_id), "missing {set_id}");
    }
}
