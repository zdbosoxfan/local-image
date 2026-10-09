use photocraft_engine::{EngineError, Session, retouch_cmds};

fn specs() -> Vec<photocraft_engine::commands::CommandSpec> {
    retouch_cmds::specs()
}

#[test]
fn specs_contain_twelve_commands() {
    let specs = specs();
    assert_eq!(specs.len(), 12, "expected exactly 12 retouch commands");
}

#[test]
fn specs_ids_are_unique() {
    let mut ids: Vec<_> = specs().iter().map(|s| s.id).collect();
    let original_len = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), original_len, "retouch command ids must be unique");
}

#[test]
fn specs_have_expected_ids() {
    let mut actual: Vec<_> = specs().iter().map(|s| s.id.to_string()).collect();
    actual.sort();

    let mut expected = vec![
        "paint.blur",
        "paint.burn",
        "paint.cloneStamp",
        "paint.contentAwareMove",
        "paint.dodge",
        "paint.healingBrush",
        "paint.historyBrush",
        "paint.patch",
        "paint.sharpen",
        "paint.smudge",
        "paint.sponge",
        "paint.spotHealing",
    ];
    expected.sort();

    assert_eq!(actual, expected);
}

#[test]
fn specs_ids_start_with_paint() {
    for spec in specs() {
        assert!(spec.id.starts_with("paint."), "retouch command id `{}` should start with `paint.`", spec.id);
    }
}

#[test]
fn specs_labels_are_nonempty() {
    for spec in specs() {
        assert!(!spec.label.is_empty(), "retouch command `{}` has empty label", spec.id);
    }
}

#[test]
fn brush_specs_require_points_param() {
    let brush_commands = [
        "paint.cloneStamp",
        "paint.healingBrush",
        "paint.spotHealing",
        "paint.dodge",
        "paint.burn",
        "paint.sponge",
        "paint.blur",
        "paint.sharpen",
        "paint.smudge",
        "paint.historyBrush",
    ];

    for id in brush_commands {
        let spec = specs().into_iter().find(|s| s.id == id).unwrap_or_else(|| panic!("missing retouch command `{id}`"));
        assert!(spec.params.contains("points"), "brush command `{id}` params should mention `points`");
    }
}

#[test]
fn specs_are_deterministic() {
    let a = specs();
    let b = specs();
    let ids_a: Vec<_> = a.iter().map(|s| s.id).collect();
    let ids_b: Vec<_> = b.iter().map(|s| s.id).collect();
    assert_eq!(ids_a, ids_b, "retouch specs() must be deterministic");
}

#[test]
fn enabled_functions_return_err_without_document() {
    let session = Session::new();
    for spec in specs() {
        let result = (spec.enabled)(&session);
        assert!(result.is_err(), "enabled for `{}` should return Err with no document", spec.id);
    }
}

#[test]
fn run_functions_return_err_with_empty_params_without_document() {
    let mut session = Session::new();
    let params = Default::default(); // serde_json::Value::Null
    for spec in specs() {
        let result = (spec.run)(&mut session, &params);
        assert!(result.is_err(), "run for `{}` should return Err with no document and empty params", spec.id);
    }
}

#[test]
fn execute_all_retouch_commands_without_document_returns_err() {
    let mut session = Session::new();
    for spec in specs() {
        let result = session.execute(spec.id, Default::default());
        assert!(result.is_err(), "execute `{}` with no document should return Err", spec.id);
    }
}

#[test]
fn execute_unknown_command_returns_unknown_command() {
    let mut session = Session::new();
    let result = session.execute("no.suchCommand", Default::default());
    match result {
        Err(EngineError::UnknownCommand(id)) => assert_eq!(id, "no.suchCommand"),
        other => panic!("expected UnknownCommand error, got {other:?}"),
    }
}

#[test]
fn all_retouch_specs_are_journaled() {
    for spec in specs() {
        assert!(spec.journal, "retouch command `{}` should be journaled", spec.id);
    }
}
