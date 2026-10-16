use super::*;

fn allow(_: &str, _: &Value) -> Result<()> {
    Ok(())
}

#[test]
fn scoped_preference_reads_redact_direct_parent_whole_and_section_aliases() {
    let mut session = Session::new();
    session.prefs.edit(|prefs| {
        for key in ["openai_api_key", "google_api_key", "bfl_api_key", "stability_api_key", "hf_token", "civitai_token", "future_client_secret"] {
            prefs.ai.set_extra_string(key, "isolated-synthetic-credential");
        }
    });
    session.authorize = Some(allow);
    for path in ["ai.openai_api_key", "ai.hf_token", "ai", ""] {
        let result = session.execute("prefs.get", json!({"path": path})).unwrap();
        assert!(!result.to_string().contains("isolated-synthetic-credential"), "{path}");
    }
    assert_eq!(session.execute("prefs.get", json!({"path":"ai.openai_api_key"})).unwrap(), json!({"configured":true}));
    session.prefs.edit(|prefs| {
        let mut value = serde_json::to_value(&prefs.ai).unwrap();
        value["future_account"] = json!({"client_secret":{"value":"nested-synthetic-credential"}});
        prefs.ai = li_ai::AiSettings::from_value(value);
    });
    let nested = session.execute("prefs.get", json!({"path":"ai.future_account.client_secret.value"})).unwrap();
    assert_eq!(nested, json!({"configured":true}));
    let alias = session.execute("edit.preferences.general", json!({"__section":"ai"})).unwrap();
    assert!(!alias.to_string().contains("isolated-synthetic-credential"));
    let updated = session.execute("prefs.set", json!({"path":"ai.comfy_port","value":8190})).unwrap();
    assert_eq!(updated["ai.comfy_port"], 8190);
    assert!(session.prefs_to_json().contains("isolated-synthetic-credential"), "persistence must retain the actual credential");
    session.authorize = None;
    assert_eq!(session.execute("prefs.get", json!({"path":"ai.openai_api_key"})).unwrap(), "isolated-synthetic-credential");
}

#[test]
fn prepared_update_has_no_live_effects_until_infallible_publication() {
    let mut session = Session::new();
    let before = session.prefs().clone();
    let revision = session.prefs.rev();
    let prepared = session.prepare_preferences_update(&json!({"values":{"performance.historyStates":12,"interface.showTooltips":false}})).unwrap();
    assert_eq!(session.prefs(), &before);
    assert_eq!(session.prefs.rev(), revision);
    let result = session.publish_prepared_preferences(prepared);
    assert_eq!(result["performance.historyStates"], 12);
    assert_eq!(session.prefs().performance.history_states, 12);
    assert!(!session.prefs().interface.show_tooltips);
    let changed = session.prefs().clone();
    assert!(session.prepare_preferences_update(&json!({"values":{"performance.historyStates":8,"interface.uiScale":"invalid"}})).is_err());
    assert_eq!(session.prefs(), &changed);
}

#[test]
fn prepared_update_authorizes_once_before_any_publication() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CHECKS: AtomicUsize = AtomicUsize::new(0);
    fn authorize(_: &str, _: &Value) -> Result<()> {
        CHECKS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    let mut session = Session::new();
    session.authorize = Some(authorize);
    let prepared = session.prepare_preferences_update(&json!({"path":"interface.showTooltips","value":false})).unwrap();
    assert_eq!(CHECKS.load(Ordering::SeqCst), 1);
    session.publish_prepared_preferences(prepared);
    assert_eq!(CHECKS.load(Ordering::SeqCst), 1);
}
