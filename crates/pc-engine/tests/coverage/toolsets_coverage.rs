use photocraft_engine::EngineError;
use photocraft_engine::Session;
use photocraft_engine::prefs::ToolbarCustomization;
use photocraft_engine::toolsets::{DEFAULT_ID, MAX_NAME, Persona, builtins, tools_for};
use serde_json::json;

fn session() -> Session {
    Session::new()
}

#[test]
fn builtin_detection_and_all_sets_merge() {
    assert!(ToolbarCustomization::is_builtin(DEFAULT_ID));
    assert!(ToolbarCustomization::is_builtin("photographer"));
    assert!(!ToolbarCustomization::is_builtin("custom-1"));
    assert!(!ToolbarCustomization::is_builtin(""));

    let mut s = session();
    let r = s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    let custom_id = r["created"].as_str().unwrap();
    assert_eq!(s.prefs().toolbar.all_sets().len(), 6);
    assert!(s.prefs().toolbar.all_sets().iter().any(|t| t.id == custom_id));
    assert!(!ToolbarCustomization::is_builtin(custom_id));
}

#[test]
fn active_set_selection_and_fallback() {
    let mut s = session();
    assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);

    s.execute("toolset.select", json!({"id": "photographer"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, "photographer");

    let created = s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    let id = created["created"].as_str().unwrap().to_string();
    assert_eq!(s.prefs().toolbar.active().id, id);

    s.edit_prefs(|pr| pr.toolbar.active_set = "missing".into());
    assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
}

#[test]
fn edit_toolbar_order_and_hidden_preserves_order_ignores_unknown_and_removes_hidden() {
    let mut s = session();
    s.execute("edit.toolbar", json!({"order": ["Brush", "UnknownTool", "Zoom"], "hidden": ["burn", "Sponge"]})).unwrap();
    let t = &s.prefs().toolbar;
    assert_eq!(t.sets.len(), 1);
    let set = &t.sets[0];
    assert_eq!(set.name, "My Tools");
    assert_eq!(set.tools.first().map(String::as_str), Some("Brush"));
    assert_eq!(set.tools.get(1).map(String::as_str), Some("Zoom"));
    assert!(!set.tools.iter().any(|x| x == "Burn" || x == "Sponge" || x == "UnknownTool"));
    assert_eq!(set.tools.len(), tools_for(Persona::Pixel).len() - 2);
}

#[test]
fn default_toolbar_stays_on_all_tools() {
    let mut s = session();
    s.load_prefs_json(r#"{"toolbar": {}}"#).unwrap();
    let t = &s.prefs().toolbar;
    assert!(t.sets.is_empty());
    assert_eq!(t.active().id, DEFAULT_ID);

    s.load_prefs_json(r#"{}"#).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
}

#[test]
fn unique_name_truncates_and_appends_numbers() {
    let s = session();
    let tb = &s.prefs().toolbar;
    assert_eq!(tb.unique_name(""), "My Tools");
    assert_eq!(tb.unique_name("Base"), "Base");
    assert_eq!(tb.unique_name(" allTools "), "allTools");

    let long = "a".repeat(MAX_NAME);
    assert_eq!(tb.unique_name(&long).len(), MAX_NAME);

    let mut s2 = session();
    s2.execute("toolset.save", json!({"name": long, "tools": []})).unwrap();
    let unique = s2.prefs().toolbar.unique_name(&long);
    assert!(unique.len() <= MAX_NAME);
    assert!(unique.ends_with(" 2"));
}

#[test]
fn save_canonicalizes_tools_and_deduplicates() {
    let mut s = session();
    let r = s.execute("toolset.save", json!({"name": "Mine", "tools": ["move", "Brush", "MoVe", "brush"]})).unwrap();
    let id = r["created"].as_str().unwrap().to_string();
    let all_sets = s.prefs().toolbar.all_sets();
    let set = all_sets.iter().find(|t| t.id == id).unwrap();
    assert_eq!(set.tools, vec!["Move", "Brush"]);
    assert_eq!(s.prefs().toolbar.active().id, id);
}

#[test]
fn save_rejects_invalid_inputs_without_mutation() {
    let mut s = session();
    let original = s.prefs().toolbar.clone();
    let cases = [
        json!({"name": "Bad", "tools": true}),
        json!({"name": "Bad", "tools": "Move"}),
        json!({"name": "Bad", "tools": ["Move", 1]}),
        json!({"name": "Bad", "tools": [null]}),
        json!({"name": "Bad", "tools": ["Unknown"]}),
        json!({"name": "", "tools": ["Move"]}),
        json!({"name": "   ", "tools": ["Move"]}),
        json!({"name": "a".repeat(MAX_NAME + 1), "tools": ["Move"]}),
    ];
    for params in &cases {
        assert!(s.execute("toolset.save", params.clone()).is_err());
        assert_eq!(s.prefs().toolbar, original, "mutation after {:?}", params);
    }

    s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    assert!(s.execute("toolset.save", json!({"name": "mine", "tools": ["Zoom"]})).is_err());
    assert_eq!(s.prefs().toolbar.sets.len(), 1);
}

#[test]
fn save_from_copies_tools_and_applies_default_name() {
    let mut s = session();
    let r = s.execute("toolset.save", json!({"from": "photographer"})).unwrap();
    let id = r["created"].as_str().unwrap().to_string();
    let all_sets = s.prefs().toolbar.all_sets();
    let set = all_sets.iter().find(|t| t.id == id).unwrap();
    assert_eq!(set.name, "Photographer copy");
    assert_eq!(set.tools.len(), builtins().iter().find(|b| b.id == "photographer").unwrap().tools.len());

    let r2 = s.execute("toolset.save", json!({"from": "retouching", "name": "My Retouch"})).unwrap();
    let all_sets = s.prefs().toolbar.all_sets();
    let set2 = all_sets.iter().find(|t| t.id == r2["created"].as_str().unwrap()).unwrap();
    assert_eq!(set2.name, "My Retouch");
}

#[test]
fn duplicate_creates_copy_with_default_and_custom_name() {
    let mut s = session();
    let r = s.execute("toolset.duplicate", json!({"id": "retouching"})).unwrap();
    let id = r["created"].as_str().unwrap().to_string();
    let all_sets = s.prefs().toolbar.all_sets();
    let set = all_sets.iter().find(|t| t.id == id).unwrap();
    assert_eq!(set.name, "Retouching copy");
    assert_eq!(set.tools.len(), builtins().iter().find(|b| b.id == "retouching").unwrap().tools.len());

    let r2 = s.execute("toolset.duplicate", json!({"id": "retouching", "newName": "My Copy"})).unwrap();
    let all_sets = s.prefs().toolbar.all_sets();
    let set2 = all_sets.iter().find(|t| t.id == r2["created"].as_str().unwrap()).unwrap();
    assert_eq!(set2.name, "My Copy");

    assert!(s.execute("toolset.duplicate", json!({"id": "missing"})).is_err());
}

#[test]
fn rename_custom_set_by_id_and_name() {
    let mut s = session();
    let r = s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    let id = r["created"].as_str().unwrap().to_string();
    s.execute("toolset.rename", json!({"id": id, "newName": "Faces"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().name, "Faces");

    s.execute("toolset.rename", json!({"name": "Faces", "to": "Portraits"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().name, "Portraits");

    s.execute("toolset.save", json!({"name": "Other", "tools": []})).unwrap();
    assert!(s.execute("toolset.rename", json!({"id": id, "newName": "Other"})).is_err());
    assert!(s.execute("toolset.rename", json!({"id": "photographer", "newName": "X"})).is_err());
    assert!(s.execute("toolset.rename", json!({"id": "missing", "newName": "X"})).is_err());
}

#[test]
fn select_by_id_or_name_case_insensitive() {
    let mut s = session();
    s.execute("toolset.select", json!({"id": "ai"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, "ai");

    s.execute("toolset.select", json!({"name": "pHotographer"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, "photographer");

    assert!(s.execute("toolset.select", json!({"id": "missing"})).is_err());
    assert!(s.execute("toolset.select", json!({})).is_err());
}

#[test]
fn reset_selects_builtin_and_keeps_custom_sets() {
    let mut s = session();
    s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    s.execute("toolset.select", json!({"id": "photographer"})).unwrap();
    s.execute("toolset.reset", json!({})).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
    assert_eq!(s.prefs().toolbar.sets.len(), 1);

    s.execute("toolset.reset", json!({"id": "retouching"})).unwrap();
    assert_eq!(s.prefs().toolbar.active().id, "retouching");

    assert!(s.execute("toolset.reset", json!({"id": s.prefs().toolbar.sets[0].id})).is_err());
}

#[test]
fn list_returns_sets_and_active() {
    let mut s = session();
    let res = s.execute("toolset.list", json!({})).unwrap();
    assert_eq!(res["active"], DEFAULT_ID);
    assert_eq!(res["sets"].as_array().unwrap().len(), 5);

    s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move"]})).unwrap();
    let res = s.execute("toolset.list", json!({})).unwrap();
    assert_eq!(res["sets"].as_array().unwrap().len(), 6);
    assert_eq!(res["active"], s.prefs().toolbar.active().id.as_str());
}

#[test]
fn new_sets_get_sequential_custom_ids_and_unique_names() {
    let mut s = session();
    let r1 = s.execute("toolset.save", json!({"name": "Base", "tools": ["Move"]})).unwrap();
    let id1 = r1["created"].as_str().unwrap().to_string();
    assert_eq!(id1, "custom-1");

    let unique = s.prefs().toolbar.unique_name("Base");
    assert_eq!(unique, "Base 2");

    // Duplicate names are rejected, not auto-uniquified.
    assert!(s.execute("toolset.save", json!({"name": "Base", "tools": ["Zoom"]})).is_err());

    let r2 = s.execute("toolset.save", json!({"name": "Other", "tools": ["Zoom"]})).unwrap();
    let id2 = r2["created"].as_str().unwrap().to_string();
    assert_eq!(id2, "custom-2");
    let all_sets = s.prefs().toolbar.all_sets();
    let set2 = all_sets.iter().find(|t| t.id == id2).unwrap();
    assert_eq!(set2.name, "Other");

    let r3 = s.execute("toolset.save", json!({"name": "Third", "tools": ["Hand"]})).unwrap();
    let all_sets = s.prefs().toolbar.all_sets();
    let set3 = all_sets.iter().find(|t| t.id == r3["created"].as_str().unwrap()).unwrap();
    assert_eq!(set3.name, "Third");
    assert_eq!(s.prefs().toolbar.sets.len(), 3);
}

#[test]
fn name_length_boundary_is_enforced() {
    let mut s = session();
    let name_max = "a".repeat(MAX_NAME);
    s.execute("toolset.save", json!({"name": name_max, "tools": ["Move"]})).unwrap();
    assert_eq!(s.prefs().toolbar.active().name.chars().count(), MAX_NAME);

    let name_too_long = "b".repeat(MAX_NAME + 1);
    assert!(s.execute("toolset.save", json!({"name": name_too_long, "tools": ["Move"]})).is_err());
    assert!(s.execute("toolset.rename", json!({"id": s.prefs().toolbar.active().id, "newName": name_too_long})).is_err());

    s.execute("toolset.rename", json!({"id": s.prefs().toolbar.active().id, "newName": name_max})).unwrap();
}

#[test]
fn errors_are_bad_params_with_messages() {
    let mut s = session();
    let err = s.execute("toolset.save", json!({"name": "X", "tools": ["Nope"]})).unwrap_err();
    match err {
        EngineError::BadParams { cmd, msg } => {
            assert_eq!(cmd, "toolset.save");
            assert!(msg.contains("unknown tool"));
        }
        _ => panic!("expected BadParams"),
    }

    let err = s.execute("toolset.delete", json!({"id": "missing"})).unwrap_err();
    assert!(matches!(
        err,
        EngineError::BadParams { cmd, msg }
            if cmd == "toolset.delete" && msg.contains("no such tool set")
    ));
}
