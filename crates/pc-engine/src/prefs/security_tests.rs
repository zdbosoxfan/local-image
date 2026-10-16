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

#[test]
fn first_provider_credentials_stage_through_root_and_ai_section() {
    let ai = json!({"comfy_port":8191,"openai_api_key":"synthetic-first-provider-key","future_provider":{"client_secret":"synthetic-future-secret"}});
    for params in [json!({"path":"","value":{"ai":ai}}), json!({"path":"ai","value":ai}), json!({"values":{"ai":ai}})] {
        let mut session = Session::new();
        assert!(session.prefs().ai.extra_string("openai_api_key").is_none());
        let prepared = session.prepare_preferences_update(&params).unwrap();
        assert!(session.prefs().ai.extra_string("openai_api_key").is_none(), "preparation cannot publish before the checked writer succeeds");
        session.publish_prepared_preferences(prepared);
        assert_eq!(session.prefs().ai.comfy_port, 8191);
        assert_eq!(session.prefs().ai.extra_string("openai_api_key").as_deref(), Some("synthetic-first-provider-key"));
        assert_eq!(session.prefs().get("ai.future_provider.client_secret"), Some(json!("synthetic-future-secret")));

        let mut ordinary = Session::new();
        ordinary.execute("prefs.set", params).unwrap();
        assert_eq!(ordinary.prefs().ai, session.prefs().ai, "command and prepared publication must use identical typed validation");
    }
}

#[test]
fn whole_ai_preparation_rejects_invalid_known_fields_without_publication() {
    for params in [
        json!({"path":"","value":{"ai":{"comfy_port":0,"openai_api_key":"synthetic-key"}}}),
        json!({"path":"ai","value":{"comfy_port":"invalid","openai_api_key":"synthetic-key"}}),
    ] {
        let session = Session::new();
        let before = session.prefs().clone();
        assert!(session.prepare_preferences_update(&params).is_err());
        assert_eq!(session.prefs(), &before);
    }
}

#[test]
fn first_provider_updates_authorize_original_input_and_redact_publication() {
    fn deny_original(_: &str, params: &Value) -> Result<()> {
        assert!(params.to_string().contains("synthetic-first-provider-key"), "authorization must inspect the original unredacted input");
        Err(bad("prefs.set", "synthetic policy denial"))
    }
    let ai = json!({"openai_api_key":"synthetic-first-provider-key","future_provider":{"client_secret":"synthetic-future-secret"}});
    for params in [json!({"path":"","value":{"ai":ai}}), json!({"path":"ai","value":ai}), json!({"values":{"ai":ai}})] {
        let mut session = Session::new();
        session.authorize = Some(deny_original);
        assert!(session.prepare_preferences_update(&params).is_err());
        assert!(session.execute("prefs.set", params.clone()).is_err());
        assert!(session.prefs().ai.extra_string("openai_api_key").is_none());

        session.authorize = Some(allow);
        let prepared = session.prepare_preferences_update(&params).unwrap();
        let published = session.publish_prepared_preferences(prepared);
        assert!(!published.to_string().contains("synthetic-first-provider-key"));
        assert!(!published.to_string().contains("synthetic-future-secret"));
        let command_reply = session.execute("prefs.set", params).unwrap();
        assert!(!command_reply.to_string().contains("synthetic-first-provider-key"));
        assert!(!command_reply.to_string().contains("synthetic-future-secret"));
        let read = session.execute("prefs.get", json!({"path":"ai"})).unwrap();
        assert_eq!(read["openai_api_key"], json!({"configured":true}));
        assert_eq!(read["future_provider"]["client_secret"], json!({"configured":true}));
        assert!(session.prefs_to_json().contains("synthetic-first-provider-key"));
    }
}
