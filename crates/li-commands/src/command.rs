//! Engine-independent command descriptions with engine-specific callbacks.

use serde::Serialize;
use serde_json::Value;

/// Execute one command against its owning engine's session.
pub type Run<S, E> = fn(&mut S, &Value) -> Result<Value, E>;
/// Explain why a command cannot currently run, or allow it.
pub type Enabled<S> = fn(&S) -> Result<(), String>;

/// Metadata and implementation for one command.
///
/// Both engine façades alias this type to retain their existing public APIs. Function
/// pointers keep static registries allocation-free apart from the registry storage itself.
pub struct CommandSpec<S, E> {
    pub id: &'static str,
    pub label: &'static str,
    /// Menu placement, e.g. `["Layer", "New"]`. Empty = not in menus.
    pub menu: &'static [&'static str],
    /// Default portable shortcut (`Cmd+Shift+N`), mapped per platform by the UI.
    pub shortcut: Option<&'static str>,
    /// Human/agent-readable parameter description (JSON-ish).
    pub params: &'static str,
    pub enabled: Enabled<S>,
    pub run: Run<S, E>,
    /// Record in the session journal (false for queries).
    pub journal: bool,
}

/// A command's metadata and enablement snapshot, with the Library's stable JSON shape.
#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: Vec<&'static str>,
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

impl<S, E> CommandSpec<S, E> {
    /// Evaluate enablement once so the boolean and explanation describe the same state.
    pub fn info(&self, session: &S) -> CommandInfo {
        let enabled = (self.enabled)(session);
        CommandInfo {
            id: self.id,
            label: self.label,
            menu: self.menu.to_vec(),
            shortcut: self.shortcut,
            params: self.params,
            enabled: enabled.is_ok(),
            disabled_reason: enabled.err(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use serde_json::json;

    struct Session {
        available: bool,
        enablement_calls: Cell<usize>,
        value: Value,
    }

    fn spec() -> CommandSpec<Session, &'static str> {
        CommandSpec {
            id: "test.command",
            label: "Test…",
            menu: &["Photo", "Rotate"],
            shortcut: Some("Cmd+Shift+R"),
            params: "{value:json}",
            enabled: |s| {
                s.enablement_calls.set(s.enablement_calls.get() + 1);
                if s.available { Ok(()) } else { Err("no photo selected".into()) }
            },
            run: |s, value| {
                if value.is_null() {
                    return Err("missing value");
                }
                s.value = value.clone();
                Ok(s.value.clone())
            },
            journal: true,
        }
    }

    #[test]
    fn shared_command_info_matches_legacy_json_golden() {
        let session = Session { available: true, enablement_calls: Cell::new(0), value: Value::Null };
        let command = spec();
        let enabled = serde_json::to_value(command.info(&session)).unwrap();
        assert_eq!(
            enabled,
            json!({
                "id": "test.command", "label": "Test…", "menu": ["Photo", "Rotate"],
                "shortcut": "Cmd+Shift+R", "params": "{value:json}", "enabled": true,
            })
        );
        assert_eq!(session.enablement_calls.get(), 1);

        let session = Session { available: false, ..session };
        let disabled = serde_json::to_value(command.info(&session)).unwrap();
        assert_eq!(
            disabled,
            json!({
                "id": "test.command", "label": "Test…", "menu": ["Photo", "Rotate"],
                "shortcut": "Cmd+Shift+R", "params": "{value:json}", "enabled": false,
                "disabled_reason": "no photo selected",
            })
        );
        assert_eq!(session.enablement_calls.get(), 2);
    }

    #[test]
    fn shared_command_callbacks_keep_session_error_and_query_metadata() {
        let mut session = Session { available: true, enablement_calls: Cell::new(0), value: Value::Null };
        let command = spec();
        assert_eq!((command.run)(&mut session, &json!({"value": 42})), Ok(json!({"value": 42})));
        assert_eq!(session.value, json!({"value": 42}));
        assert_eq!((command.run)(&mut session, &Value::Null), Err("missing value"));
        assert!(command.journal);

        // A second engine binds different session/error types without wrappers or conversion.
        let query: CommandSpec<u32, u8> = CommandSpec {
            id: "test.query",
            label: "Query",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: |_| Ok(()),
            run: |s, _| Ok(json!(*s)),
            journal: false,
        };
        assert_eq!((query.run)(&mut 7, &Value::Null), Ok(json!(7)));
        assert!(!query.journal);
        assert_eq!(
            serde_json::to_value(query.info(&7)).unwrap(),
            json!({
                "id": "test.query", "label": "Query", "menu": [], "shortcut": null,
                "params": "{}", "enabled": true,
            })
        );
    }
}
