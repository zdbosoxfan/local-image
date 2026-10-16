//! Client for the desktop app's JSON-lines control protocol
//! (`docs/control-protocol.md`): one JSON request per line, replies matched
//! by `id`. Keeps one connection and reconnects on failure.
//!
//! The live desktop session holds the user's cloud AI provider keys, so every request first
//! passes the [`BridgePolicy`] guard ([`authorize_bridge_request`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::AutomationError;
use crate::budgets::MAX_RESPONSE_BYTES;
use crate::security::{AUTH_METHOD, MAX_REQUEST_BYTES, validate_token};
use crate::workspace::{authorize_cloud_command, mentions_secret, secret_name, secret_path};

type Conn = (BufReader<tokio::net::tcp::OwnedReadHalf>, tokio::net::tcp::OwnedWriteHalf);

/// What a bridge may ask of the live desktop session. Stored credentials are never readable or
/// settable; paid cloud AI generation needs `allow_cloud_ai` (`--allow-cloud-ai`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BridgePolicy {
    pub allow_cloud_ai: bool,
}

pub struct BridgeClient {
    addr: String,
    token: String,
    conn: Mutex<Option<Conn>>,
    next_id: AtomicU64,
    timeout: Duration,
    policy: BridgePolicy,
}

impl BridgeClient {
    /// `addr` such as `127.0.0.1:7878`. Only loopback addresses are accepted,
    /// matching the server, which binds to loopback only.
    pub fn new(addr: impl Into<String>, token: impl Into<String>) -> Result<Self, AutomationError> {
        let addr = addr.into();
        let token = token.into();
        let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(&addr);
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
            return Err(AutomationError::BadRequest(format!("bridge address must be loopback, got `{addr}`")));
        }
        validate_token(&token)?;
        Ok(BridgeClient {
            addr,
            token: token.to_ascii_lowercase(),
            conn: Mutex::new(None),
            next_id: AtomicU64::new(1),
            timeout: Duration::from_secs(60),
            policy: BridgePolicy::default(),
        })
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn with_policy(mut self, policy: BridgePolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Call a control method; returns its `result` or the app's error. The request is refused
    /// before it is sent when the [`BridgePolicy`] denies it; synthetic input is judged against
    /// the live UI state first.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, AutomationError> {
        authorize_bridge_request(method, &params, self.policy)?;
        if SYNTHETIC_INPUT.contains(&method) {
            let ui = self.send("ui.inspect", json!({})).await?;
            authorize_synthetic_input(method, &ui, self.policy)?;
        }
        self.send(method, params).await
    }

    async fn send(&self, method: &str, params: Value) -> Result<Value, AutomationError> {
        let mut guard = self.conn.lock().await;
        // A transport failure is retried once on a fresh connection (the app may have restarted),
        // but only when the request never left or cannot change anything: a write that reached
        // the app may already have run.
        for attempt in 0..2 {
            if guard.is_none() {
                let s = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&self.addr))
                    .await
                    .map_err(|_| AutomationError::Bridge(format!("timed out connecting to {}", self.addr)))?
                    .map_err(|e| {
                        AutomationError::Bridge(format!(
                            "cannot connect to {} ({e}); start the app with `photocraft --control <port>` and matching control credentials",
                            self.addr
                        ))
                    })?;
                let (r, w) = s.into_split();
                let mut conn = (BufReader::new(r), w);
                let auth_id = self.next_id.fetch_add(1, Ordering::Relaxed);
                let auth = tokio::time::timeout(Duration::from_secs(5), exchange(&mut conn, auth_id, AUTH_METHOD, &json!({"token": self.token})))
                    .await
                    .map_err(|_| AutomationError::Bridge("control authentication timed out".into()))?
                    .map_err(|e| e.error)?;
                auth?;
                *guard = Some(conn);
            }
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let Some(conn) = guard.as_mut() else {
                return Err(AutomationError::Bridge(format!("not connected to {}", self.addr)));
            };
            match tokio::time::timeout(self.timeout, exchange(conn, id, method, &params)).await {
                Ok(Ok(Err(error @ AutomationError::BadRequest(_)))) => {
                    // An oversized frame leaves unread bytes. Drop this connection and
                    // report the budget failure without retrying a possibly completed edit.
                    *guard = None;
                    return Err(error);
                }
                Ok(Ok(v)) => return v,
                Ok(Err(e)) if attempt == 0 && (!e.sent || read_only_request(method, &params)) => {
                    *guard = None;
                }
                Ok(Err(e)) => {
                    *guard = None;
                    if e.sent && !read_only_request(method, &params) {
                        return Err(AutomationError::Bridge(format!("{}; `{method}` was sent and may have completed, so it was not retried", e.error)));
                    }
                    return Err(e.error);
                }
                Err(_) => {
                    *guard = None;
                    return Err(AutomationError::Bridge(format!("`{method}` timed out after {:?}", self.timeout)));
                }
            }
        }
        Err(AutomationError::Bridge("unreachable".into()))
    }
}

/// Requests that change nothing in the app, so a transport failure may safely repeat them.
pub fn read_only_request(method: &str, params: &Value) -> bool {
    match method {
        "ui.inspect" | "ui.menu.list" | "engine.commands" | "jobs.list" => true,
        "ui.screenshot" => params.get("path").is_none_or(Value::is_null),
        "engine.execute" => matches!(params.get("command").and_then(Value::as_str), Some("document.inspect" | "command.list" | "document.pixel")),
        _ => false,
    }
}

/// Synthetic UI input: it lands on whatever control is under it, so it is judged against the
/// live UI state ([`authorize_synthetic_input`]).
pub const SYNTHETIC_INPUT: &[&str] = &["ui.click", "ui.key", "ui.type"];

/// The [`BridgePolicy`] guard for one control request. Engine and menu command ids (also those
/// a command dialog or context menu would run) pass [`authorize_cloud_command`]; `ui.set` may
/// not open the Generate panel without `allow_cloud_ai`; dialog fields never carry credentials.
pub fn authorize_bridge_request(method: &str, params: &Value, policy: BridgePolicy) -> Result<(), AutomationError> {
    let s = |k: &str| params.get(k).and_then(Value::as_str);
    let command = match method {
        "engine.execute" | "ui.menu.invoke" | "command" => s("command").or(s("id")),
        "ui.context.choose" => s("id"),
        "ui.dialog.open" => s("command").or(params.get("fields").and_then(|f| f.get("command")).and_then(Value::as_str)),
        m if m.starts_with("prefs.") => Some(m),
        _ => None,
    };
    if let Some(id) = command {
        let command_params = match method {
            "engine.execute" | "ui.menu.invoke" | "command" => params.get("params").cloned().unwrap_or(Value::Null),
            "ui.dialog.open" => params.get("fields").cloned().unwrap_or(Value::Null),
            _ => params.clone(),
        };
        authorize_cloud_command(id, &command_params, policy.allow_cloud_ai).map_err(bridge_denied)?;
    }
    match method {
        "ui.set" if !policy.allow_cloud_ai && params.get("panels").and_then(|p| p.get("generate")).and_then(Value::as_bool) == Some(true) => {
            Err(AutomationError::BadRequest(
                "bridge: opening the Generate panel can start paid cloud AI generation with the API keys saved in the app; \
                 start the MCP server with --allow-cloud-ai to permit it"
                    .into(),
            ))
        }
        "ui.dialog.set" | "ui.dialog.open" if dialog_fields_reach_credentials(method, params) => Err(AutomationError::BadRequest(format!(
            "bridge: `{method}` would read or change stored API keys, tokens or passwords; enter credentials in the app yourself"
        ))),
        _ => Ok(()),
    }
}

/// Dialog fields naming a credential, holding one, or switching Preferences to the Local AI
/// pages (where provider keys are entered).
fn dialog_fields_reach_credentials(method: &str, params: &Value) -> bool {
    if method == "ui.dialog.open" {
        return params.get("fields").is_some_and(mentions_secret);
    }
    let field = params.get("field").and_then(Value::as_str).unwrap_or("");
    let value = params.get("value").unwrap_or(&Value::Null);
    secret_name(field)
        || mentions_secret(value)
        || (field == "path" && value.as_str().is_some_and(secret_path))
        || (field == "section" && value.as_str().is_some_and(|section| section.starts_with("ai.")))
}

/// Judge synthetic input against `ui` (the live `ui.inspect` state). Always refused while the
/// Local AI pages of Preferences are open (they hold API keys); without `allow_cloud_ai`, also
/// while the Generate panel is open or no document is open (the start screen's Generate
/// prompt), as a click or Enter there can start paid cloud generation.
pub fn authorize_synthetic_input(method: &str, ui: &Value, policy: BridgePolicy) -> Result<(), AutomationError> {
    let credential_page = ui.get("dialogs").and_then(Value::as_array).is_some_and(|dialogs| {
        dialogs.iter().any(|d| d.get("fields").and_then(|f| f.get("section")).and_then(Value::as_str).is_some_and(|s| s.starts_with("ai.")))
    });
    if credential_page {
        return Err(AutomationError::BadRequest(format!(
            "bridge: `{method}` is refused while the Local AI settings are open (they hold API keys); close them first"
        )));
    }
    if policy.allow_cloud_ai {
        return Ok(());
    }
    let generate_open = ui.get("panels").and_then(|p| p.get("generate")).and_then(Value::as_bool) == Some(true);
    let start_screen = ui.get("session").and_then(|s| s.get("documents")).and_then(Value::as_array).is_some_and(Vec::is_empty);
    let why = match (generate_open, start_screen) {
        (true, _) => "while the Generate panel is open; close it with ui_set {\"panels\": {\"generate\": false}}",
        (_, true) => "while no document is open (the start screen offers Generate); open or create one with doc_open or doc_new",
        _ => return Ok(()),
    };
    Err(AutomationError::BadRequest(format!(
        "bridge: `{method}` is refused {why}, as it could start paid cloud AI generation with the API keys saved in the app; \
         or start the MCP server with --allow-cloud-ai"
    )))
}

fn bridge_denied(error: AutomationError) -> AutomationError {
    match error {
        AutomationError::BadRequest(message) => AutomationError::BadRequest(format!("bridge: {message}")),
        other => other,
    }
}

/// A transport failure; `sent` when any byte of the request may have reached the app.
struct TransportError {
    error: AutomationError,
    sent: bool,
}

/// Outer `Err` = transport failure; inner = app-level result.
async fn exchange(conn: &mut Conn, id: u64, method: &str, params: &Value) -> Result<Result<Value, AutomationError>, TransportError> {
    let unsent = |error| TransportError { error, sent: false };
    let mut line = serde_json::to_string(&json!({"id": id, "method": method, "params": params})).map_err(|e| unsent(AutomationError::Other(e.to_string())))?;
    line.push('\n');
    if line.len() > MAX_REQUEST_BYTES {
        return Ok(Err(AutomationError::BadRequest(format!("request is {} bytes; maximum is {MAX_REQUEST_BYTES}", line.len()))));
    }
    let io = |sent: bool| move |e: std::io::Error| TransportError { error: AutomationError::Bridge(e.to_string()), sent };
    let bytes = line.as_bytes();
    let mut written = 0;
    while written < bytes.len() {
        match conn.1.write(&bytes[written..]).await.map_err(io(written > 0))? {
            0 => return Err(TransportError { error: AutomationError::Bridge("connection closed by the app".into()), sent: written > 0 }),
            n => written += n,
        }
    }
    conn.1.flush().await.map_err(io(true))?;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // Read raw bytes so truncation inside a UTF-8 character still reports the
        // budget error instead of a retryable decoding/transport error.
        let n = (&mut conn.0).take((MAX_RESPONSE_BYTES + 1) as u64).read_until(b'\n', &mut buf).await.map_err(io(true))?;
        if n > MAX_RESPONSE_BYTES {
            return Ok(Err(AutomationError::BadRequest(format!("bridge response exceeds {MAX_RESPONSE_BYTES} bytes; operation may have completed"))));
        }
        if n == 0 {
            return Err(TransportError { error: AutomationError::Bridge("connection closed by the app".into()), sent: true });
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        if v.get("id").and_then(Value::as_u64) != Some(id) {
            continue; // stale reply from an earlier, timed-out request
        }
        return Ok(if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(AutomationError::App(v.get("error").and_then(Value::as_str).unwrap_or("unknown error").to_owned()))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DENY: BridgePolicy = BridgePolicy { allow_cloud_ai: false };
    const ALLOW: BridgePolicy = BridgePolicy { allow_cloud_ai: true };

    #[test]
    fn bridge_guard_refuses_credentials_always_and_cloud_ai_without_the_flag() {
        let credentials = [
            ("engine.execute", json!({"command": "prefs.get", "params": {"path": "ai.openai_api_key"}})),
            ("engine.execute", json!({"command": "prefs.set", "params": {"values": {"ai.google_api_key": "k"}}})),
            ("engine.execute", json!({"command": "prefs.reset", "params": {}})),
            ("ui.menu.invoke", json!({"id": "li.localAi"})),
            ("ui.dialog.open", json!({"kind": "command", "command": "prefs.set", "fields": {"path": "ai.bfl_api_key"}})),
            ("ui.dialog.set", json!({"dialog": 1, "field": "values", "value": {"ai": {"stability_api_key": "k"}}})),
            ("ui.dialog.set", json!({"dialog": 1, "field": "path", "value": "ai.openai_api_key"})),
            ("ui.dialog.set", json!({"dialog": 1, "field": "section", "value": "ai.providers"})),
        ];
        for (method, params) in &credentials {
            for policy in [DENY, ALLOW] {
                let e = authorize_bridge_request(method, params, policy).unwrap_err().to_string();
                assert!(e.starts_with("bridge:") && (e.contains("API keys") || e.contains("passwords")), "{method} {params}: {e}");
            }
        }
        let cloud = [
            ("engine.execute", json!({"command": "li.panel.generate"})),
            ("ui.menu.invoke", json!({"id": "li.newFromPrompt"})),
            ("ui.context.choose", json!({"id": "li.newFromPrompt"})),
            ("ui.set", json!({"panels": {"generate": true}})),
        ];
        for (method, params) in &cloud {
            let e = authorize_bridge_request(method, params, DENY).unwrap_err().to_string();
            assert!(e.contains("--allow-cloud-ai"), "{method}: {e}");
            assert!(authorize_bridge_request(method, params, ALLOW).is_ok(), "{method}");
        }
        for (method, params) in [
            ("engine.execute", json!({"command": "layer.new.layer", "params": {"name": "Ink"}})),
            ("engine.execute", json!({"command": "ai.generativeFill", "params": {"prompt": "sky"}})),
            ("engine.execute", json!({"command": "prefs.set", "params": {"path": "interface.language", "value": "fr"}})),
            ("ui.set", json!({"panels": {"generate": false, "layers": true}})),
            ("ui.dialog.set", json!({"dialog": 1, "field": "radius", "value": 4})),
            ("ui.inspect", json!({})),
        ] {
            assert!(authorize_bridge_request(method, &params, DENY).is_ok(), "{method} {params}");
        }
    }

    #[test]
    fn synthetic_input_is_judged_against_the_live_ui() {
        let editing = json!({"panels": {"generate": false}, "session": {"documents": [{"index": 0}]}, "dialogs": []});
        let generate = json!({"panels": {"generate": true}, "session": {"documents": [{"index": 0}]}, "dialogs": []});
        let start_screen = json!({"panels": {"generate": false}, "session": {"documents": []}, "dialogs": []});
        let key_page = json!({"panels": {}, "session": {"documents": [{"index": 0}]}, "dialogs": [{"id": 3, "fields": {"section": "ai.providers"}}]});
        for method in SYNTHETIC_INPUT {
            assert!(authorize_synthetic_input(method, &editing, DENY).is_ok());
            for ui in [&generate, &start_screen] {
                assert!(authorize_synthetic_input(method, ui, DENY).unwrap_err().to_string().contains("--allow-cloud-ai"), "{method} {ui}");
                assert!(authorize_synthetic_input(method, ui, ALLOW).is_ok());
            }
            for policy in [DENY, ALLOW] {
                assert!(authorize_synthetic_input(method, &key_page, policy).unwrap_err().to_string().contains("API keys"));
            }
        }
    }

    #[test]
    fn only_read_only_requests_are_safe_to_repeat() {
        for (method, params) in [
            ("ui.inspect", json!({})),
            ("engine.commands", json!({})),
            ("ui.screenshot", json!({})),
            ("engine.execute", json!({"command": "document.inspect", "params": {}})),
        ] {
            assert!(read_only_request(method, &params), "{method} {params}");
        }
        for (method, params) in [
            ("engine.execute", json!({"command": "layer.new.layer"})),
            ("ui.screenshot", json!({"path": "shot.png"})),
            ("ui.click", json!({"x": 1, "y": 1})),
            ("app.save", json!({"path": "out.png"})),
            ("jobs.cancel", json!({})),
        ] {
            assert!(!read_only_request(method, &params), "{method} {params}");
        }
    }
}
