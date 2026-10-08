//! A blocking ComfyUI client: node inventory, image upload, queueing, progress over the
//! WebSocket, history polling, job-specific cancellation and result download.
//!
//! Behaviour follows the 0.7 Python client (`local_comfy_client.py`): submit once, poll
//! `/history/{id}` (0.15 s for 10 s, then 0.5 s, then 1 s, 20 min deadline), use the WebSocket for
//! progress only, and cancel only our own prompt (never the global `/interrupt`).

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Upload subfolder inside ComfyUI's input directory.
pub const UPLOAD_SUBFOLDER: &str = "local-image";
const DEADLINE: Duration = Duration::from_secs(20 * 60);

/// What a running job is doing, for the progress bar and status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    /// Sampling step `value / max`, when ComfyUI reports it.
    pub step: Option<(u32, u32)>,
    pub message: String,
}

impl Progress {
    /// 0..1 when known.
    pub fn fraction(&self) -> Option<f32> {
        self.step.filter(|s| s.1 > 0).map(|(v, m)| v as f32 / m as f32)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Preparing,
    Uploading,
    Queued,
    Loading,
    Conditioning,
    Sampling,
    Decoding,
    Saving,
    Running,
    Finishing,
    Done,
    Failed,
    Cancelled,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::Preparing => "Preparing",
            Stage::Uploading => "Uploading",
            Stage::Queued => "Waiting in the ComfyUI queue",
            Stage::Loading => "Loading models",
            Stage::Conditioning => "Reading the prompt",
            Stage::Sampling => "Sampling",
            Stage::Decoding => "Decoding",
            Stage::Saving => "Saving",
            Stage::Running => "Running",
            Stage::Finishing => "Finishing",
            Stage::Done => "Done",
            Stage::Failed => "Failed",
            Stage::Cancelled => "Cancelled",
        }
    }
}

/// Shared between the UI thread and a job thread: progress out, cancellation in.
#[derive(Clone, Default)]
pub struct JobControl {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
}

impl JobControl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn progress(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }
    pub fn set_stage(&self, stage: Stage) {
        if let Ok(mut p) = self.progress.lock() {
            if p.stage != stage {
                p.step = None;
            }
            p.stage = stage;
        }
    }
    pub fn set_message(&self, message: impl Into<String>) {
        if let Ok(mut p) = self.progress.lock() {
            p.message = message.into();
        }
    }
    fn set_step(&self, value: u32, max: u32) {
        if let Ok(mut p) = self.progress.lock() {
            p.stage = Stage::Sampling;
            p.step = Some((value, max));
        }
    }
    /// Fails with a "cancelled" error once cancellation was requested.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(anyhow!(Cancelled)) } else { Ok(()) }
    }
}

/// The error a cancelled job ends with (test with `err.is::<Cancelled>()`).
#[derive(Debug, Clone, Copy)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cancelled")
    }
}
impl std::error::Error for Cancelled {}

/// ComfyUI's node inventory (`/object_info`).
#[derive(Clone, Debug, Default)]
pub struct ObjectInfo(pub Value);

impl ObjectInfo {
    pub fn has_node(&self, class: &str) -> bool {
        self.0.get(class).is_some()
    }
    /// Choices of a combo input, in both the legacy `[[options]]` and V3 `["COMBO", {options}]`
    /// shapes.
    pub fn choices(&self, class: &str, input: &str) -> Vec<String> {
        let Some(spec) = self.0.get(class).and_then(|n| n.get("input")) else { return Vec::new() };
        let field = ["required", "optional"].iter().find_map(|k| spec.get(*k).and_then(|s| s.get(input)));
        let Some(Value::Array(v)) = field else { return Vec::new() };
        let list = match v.first() {
            Some(Value::Array(opts)) => opts.clone(),
            Some(Value::String(s)) if s == "COMBO" => v.get(1).and_then(|o| o.get("options")).and_then(|o| o.as_array()).cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        list.into_iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()
    }
    /// The loader choice whose file name matches `filename` (case, separators and punctuation
    /// ignored), returned exactly as ComfyUI spells it (it may include a subfolder).
    pub fn find_file(&self, class: &str, input: &str, filename: &str) -> Option<String> {
        let want = normalized_name(filename);
        self.choices(class, input).into_iter().find(|c| normalized_name(c) == want)
    }
}

/// Basename, lower case, alphanumerics only.
pub fn normalized_name(name: &str) -> String {
    let base = name.replace('\\', "/");
    let base = base.rsplit('/').next().unwrap_or("");
    base.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

#[derive(Clone)]
pub struct ComfyClient {
    host: String,
    agent: ureq::Agent,
    quick: ureq::Agent,
}

impl std::fmt::Debug for ComfyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComfyClient").field("host", &self.host).finish()
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        // Loopback only: never route through a proxy.
        .proxy(None)
        .build()
        .into()
}

impl ComfyClient {
    /// `host` is `host:port`, e.g. `127.0.0.1:8188`.
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into(), agent: agent(Duration::from_secs(60)), quick: agent(Duration::from_secs(4)) }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.host, path)
    }

    fn get_json(&self, agent: &ureq::Agent, path: &str) -> Result<Value> {
        let mut r = agent.get(&self.url(path)).call().with_context(|| format!("Cannot reach ComfyUI at {}", self.host))?;
        if !r.status().is_success() {
            bail!("ComfyUI answered {} for {path}", r.status());
        }
        Ok(r.body_mut().with_config().limit(64 << 20).read_json()?)
    }

    /// True when the server answers at all.
    pub fn ping(&self) -> bool {
        agent(Duration::from_secs(3)).get(&self.url("/system_stats")).call().map(|r| r.status().is_success()).unwrap_or(false)
    }

    pub fn object_info(&self) -> Result<ObjectInfo> {
        let v = self.get_json(&self.agent, "/object_info")?;
        if !v.is_object() {
            bail!("ComfyUI returned an invalid node inventory");
        }
        Ok(ObjectInfo(v))
    }

    pub fn system_stats(&self) -> Result<Value> {
        self.get_json(&agent(Duration::from_secs(3)), "/system_stats")
    }

    pub fn queue_state(&self) -> Result<Value> {
        self.get_json(&self.quick, "/queue")
    }

    /// Uploads a PNG into ComfyUI's input folder; returns the `LoadImage` value for it.
    pub fn upload_png(&self, png: &[u8]) -> Result<String> {
        let name = format!("{}.png", hex::encode(&Sha256::digest(png)[..16]));
        let boundary = format!("----localimage{}", uuid::Uuid::new_v4().simple());
        let mut body = Vec::with_capacity(png.len() + 512);
        let mut field = |name: &str, value: &str| {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        };
        field("type", "input");
        field("subfolder", UPLOAD_SUBFOLDER);
        field("overwrite", "true");
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: image/png\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(png);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let mut r = self
            .agent
            .post(&self.url("/upload/image"))
            .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send(&body[..])
            .context("Uploading an image to ComfyUI failed")?;
        if !r.status().is_success() {
            bail!("ComfyUI refused the image upload ({})", r.status());
        }
        let v: Value = r.body_mut().read_json()?;
        let stored = v.get("name").and_then(Value::as_str).ok_or_else(|| anyhow!("ComfyUI did not name the uploaded image"))?;
        if stored.contains("..") || stored.contains('/') || stored.contains('\\') {
            bail!("ComfyUI returned an unexpected upload path");
        }
        let sub = v.get("subfolder").and_then(Value::as_str).unwrap_or("");
        Ok(if sub.is_empty() { format!("{stored} [input]") } else { format!("{sub}/{stored} [input]") })
    }

    pub fn queue_prompt(&self, graph: &Value, client_id: &str) -> Result<String> {
        let mut r = self
            .agent
            .post(&self.url("/prompt"))
            .send_json(json!({ "prompt": graph, "client_id": client_id }))
            .context("Queueing the workflow in ComfyUI failed")?;
        let status = r.status();
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("ComfyUI rejected the workflow: {}", describe_prompt_error(&v));
        }
        v.get("prompt_id").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| anyhow!("ComfyUI did not return a prompt id"))
    }

    pub fn history(&self, prompt_id: &str) -> Result<Option<Value>> {
        let v = self.get_json(&self.quick, &format!("/history/{prompt_id}"))?;
        Ok(v.get(prompt_id).cloned())
    }

    pub fn view(&self, filename: &str, subfolder: &str, kind: &str) -> Result<Vec<u8>> {
        let q = format!("/view?filename={}&subfolder={}&type={}", enc(filename), enc(subfolder), enc(kind));
        let mut r = self.agent.get(&self.url(&q)).call()?;
        if !r.status().is_success() {
            bail!("ComfyUI could not return the result image ({})", r.status());
        }
        Ok(r.body_mut().with_config().limit(512 << 20).read_to_vec()?)
    }

    /// Cancels only `prompt_id`: the job API when present, else deletes it from the pending
    /// queue. A prompt already running on an old server cannot be cancelled individually.
    pub fn cancel(&self, prompt_id: &str) -> Result<()> {
        if let Ok(r) = self.quick.post(&self.url(&format!("/api/jobs/{prompt_id}/cancel"))).send_json(json!({}))
            && r.status().is_success()
        {
            return Ok(());
        }
        let q = self.queue_state()?;
        let pending = q.get("queue_pending").and_then(Value::as_array).is_some_and(|items| items.iter().any(|i| item_has(i, prompt_id)));
        if pending {
            self.quick.post(&self.url("/queue")).send_json(json!({ "delete": [prompt_id] }))?;
            return Ok(());
        }
        let running = q.get("queue_running").and_then(Value::as_array).is_some_and(|items| items.iter().any(|i| item_has(i, prompt_id)));
        if running {
            bail!("This ComfyUI version cannot stop a single running job. Update ComfyUI, or wait for it to finish.");
        }
        Ok(())
    }

    /// Unloads models and frees GPU memory (only when the queue is empty).
    pub fn free_memory(&self) -> Result<()> {
        let q = self.queue_state()?;
        let busy = ["queue_running", "queue_pending"].iter().any(|k| q.get(*k).and_then(Value::as_array).is_some_and(|a| !a.is_empty()));
        if busy {
            bail!("ComfyUI is busy; models stay loaded until its queue is empty.");
        }
        self.quick.post(&self.url("/free")).send_json(json!({ "unload_models": true, "free_memory": true }))?;
        Ok(())
    }

    /// Runs `graph`: uploads `images` (node id → PNG bytes) into those `LoadImage` nodes, queues,
    /// reports progress into `ctl`, and returns the first output image's bytes.
    pub fn run(&self, mut graph: Value, images: &[(String, Vec<u8>)], ctl: &JobControl) -> Result<Vec<u8>> {
        ctl.check()?;
        ctl.set_stage(Stage::Uploading);
        for (node, png) in images {
            let name = self.upload_png(png)?;
            let slot = graph.get_mut(node).and_then(|n| n.get_mut("inputs")).ok_or_else(|| anyhow!("workflow has no node {node}"))?;
            slot["image"] = Value::String(name);
            ctl.check()?;
        }
        let client_id = uuid::Uuid::new_v4().to_string();
        let socket = ProgressSocket::start(&self.host, &client_id, ctl.clone());
        ctl.set_stage(Stage::Queued);
        let prompt_id = self.queue_prompt(&graph, &client_id)?;
        if let Some(s) = &socket {
            s.set_prompt(&prompt_id, &graph);
        }
        let started = Instant::now();
        let mut cancel_sent = false;
        loop {
            let elapsed = started.elapsed();
            if elapsed > DEADLINE {
                let _ = self.cancel(&prompt_id);
                bail!("ComfyUI did not finish within 20 minutes");
            }
            if ctl.is_cancelled() && !cancel_sent {
                cancel_sent = true;
                self.cancel(&prompt_id)?;
            }
            match self.history(&prompt_id) {
                Ok(Some(entry)) => {
                    if let Some(err) = history_error(&entry) {
                        if cancel_sent {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                        bail!("{err}");
                    }
                    if let Some((file, sub, kind)) = first_output_image(&entry) {
                        ctl.set_stage(Stage::Finishing);
                        let bytes = self.view(&file, &sub, &kind)?;
                        ctl.set_stage(Stage::Done);
                        return Ok(bytes);
                    }
                    if entry.get("status").and_then(|s| s.get("completed")).and_then(Value::as_bool) == Some(true) {
                        bail!("ComfyUI finished without an output image");
                    }
                }
                Ok(None) => {
                    if cancel_sent {
                        // Gone from the queue without a history entry: it was deleted.
                        let q = self.queue_state().unwrap_or(Value::Null);
                        let listed = ["queue_running", "queue_pending"]
                            .iter()
                            .any(|k| q.get(*k).and_then(Value::as_array).is_some_and(|a| a.iter().any(|i| item_has(i, &prompt_id))));
                        if !listed && self.history(&prompt_id).ok().flatten().is_none() {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                    }
                }
                Err(e) => log::debug!("history poll: {e:#}"),
            }
            let wait = if elapsed < Duration::from_secs(10) {
                150
            } else if elapsed < Duration::from_secs(60) {
                500
            } else {
                1000
            };
            std::thread::sleep(Duration::from_millis(wait));
        }
    }
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn item_has(item: &Value, prompt_id: &str) -> bool {
    item.as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(prompt_id)))
}

fn describe_prompt_error(v: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(m) = v.get("error").and_then(|e| e.get("message")).and_then(Value::as_str) {
        parts.push(m.to_owned());
    }
    if let Some(nodes) = v.get("node_errors").and_then(Value::as_object) {
        for (id, e) in nodes {
            let class = e.get("class_type").and_then(Value::as_str).unwrap_or("?");
            for err in e.get("errors").and_then(Value::as_array).into_iter().flatten() {
                let msg = err.get("details").and_then(Value::as_str).or_else(|| err.get("message").and_then(Value::as_str)).unwrap_or("");
                parts.push(format!("node {id} ({class}): {msg}"));
            }
        }
    }
    if parts.is_empty() { v.to_string() } else { parts.join("; ") }
}

fn history_error(entry: &Value) -> Option<String> {
    let status = entry.get("status")?;
    for m in status.get("messages").and_then(Value::as_array).into_iter().flatten() {
        let kind = m.get(0).and_then(Value::as_str).unwrap_or("");
        if kind == "execution_error" || kind == "execution_interrupted" {
            let data = m.get(1).cloned().unwrap_or(Value::Null);
            let msg = data.get("exception_message").and_then(Value::as_str).unwrap_or(kind).trim().to_owned();
            return Some(friendly_error(&msg));
        }
    }
    (status.get("status_str").and_then(Value::as_str) == Some("error")).then(|| "ComfyUI reported an error".to_owned())
}

/// Plain-language versions of the errors people actually hit.
pub fn friendly_error(msg: &str) -> String {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("out of memory") || lower.contains("allocation on device") {
        "The GPU ran out of memory. Try a smaller size, the compact model, or close other GPU work.".into()
    } else {
        format!("ComfyUI could not finish: {msg}")
    }
}

/// `(filename, subfolder, type)` of the first image in a history entry's outputs, preferring
/// saved outputs over previews.
pub fn first_output_image(entry: &Value) -> Option<(String, String, String)> {
    let outputs = entry.get("outputs")?.as_object()?;
    let mut keys: Vec<&String> = outputs.keys().collect();
    keys.sort_by_key(|k| k.parse::<i64>().unwrap_or(i64::MAX));
    let mut fallback = None;
    for k in keys {
        for img in outputs[k].get("images").and_then(Value::as_array).into_iter().flatten() {
            let f = img.get("filename").and_then(Value::as_str)?.to_owned();
            let s = img.get("subfolder").and_then(Value::as_str).unwrap_or("").to_owned();
            let t = img.get("type").and_then(Value::as_str).unwrap_or("output").to_owned();
            if t == "output" {
                return Some((f, s, t));
            }
            fallback.get_or_insert((f, s, t));
        }
    }
    fallback
}

/// Reads ComfyUI's WebSocket on a thread and turns events for our prompt into [`Progress`].
struct ProgressSocket {
    state: Arc<Mutex<SocketState>>,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
struct SocketState {
    prompt: Option<String>,
    classes: std::collections::HashMap<String, String>,
}

impl ProgressSocket {
    fn start(host: &str, client_id: &str, ctl: JobControl) -> Option<Self> {
        let stream = TcpStream::connect_timeout(&host.parse().ok().or_else(|| resolve(host))?, Duration::from_secs(2)).ok()?;
        stream.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
        let url = format!("ws://{host}/ws?clientId={client_id}");
        let (mut ws, _) = tungstenite::client(url.as_str(), stream).ok()?;
        let state = Arc::new(Mutex::new(SocketState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (st, sp) = (state.clone(), stop.clone());
        std::thread::Builder::new()
            .name("comfy-progress".into())
            .spawn(move || {
                let mut early: Vec<Value> = Vec::new();
                while !sp.load(Ordering::SeqCst) && !ctl.is_cancelled() {
                    match ws.read() {
                        Ok(tungstenite::Message::Text(t)) => {
                            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                            let known = st.lock().map(|s| s.prompt.is_some()).unwrap_or(false);
                            if !known {
                                if early.len() < 50 {
                                    early.push(v);
                                }
                                continue;
                            }
                            for ev in early.drain(..).chain(std::iter::once(v)) {
                                if handle_event(&ev, &st, &ctl) {
                                    return;
                                }
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => return,
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                        Err(_) => return,
                    }
                }
            })
            .ok()?;
        Some(Self { state, stop })
    }

    fn set_prompt(&self, id: &str, graph: &Value) {
        if let Ok(mut s) = self.state.lock() {
            s.prompt = Some(id.to_owned());
            if let Some(o) = graph.as_object() {
                for (k, n) in o {
                    s.classes.insert(k.clone(), n.get("class_type").and_then(Value::as_str).unwrap_or("").to_owned());
                }
            }
        }
    }
}

impl Drop for ProgressSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn resolve(host: &str) -> Option<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    host.to_socket_addrs().ok()?.next()
}

/// Returns true when the job finished (the reader can stop).
fn handle_event(ev: &Value, st: &Arc<Mutex<SocketState>>, ctl: &JobControl) -> bool {
    let kind = ev.get("type").and_then(Value::as_str).unwrap_or("");
    let data = ev.get("data").cloned().unwrap_or(Value::Null);
    let (prompt, class_of) = {
        let Ok(s) = st.lock() else { return true };
        let node = data.get("node").and_then(Value::as_str).map(str::to_owned);
        (s.prompt.clone(), node.and_then(|n| s.classes.get(&n).cloned()))
    };
    if let Some(p) = data.get("prompt_id").and_then(Value::as_str)
        && Some(p) != prompt.as_deref()
    {
        return false;
    }
    match kind {
        "execution_start" => ctl.set_stage(Stage::Loading),
        "executing" => {
            if data.get("node").is_none_or(Value::is_null) {
                ctl.set_stage(Stage::Saving);
                return true;
            }
            ctl.set_stage(stage_for_class(class_of.as_deref().unwrap_or("")));
        }
        "progress" => {
            let v = data.get("value").and_then(Value::as_u64).unwrap_or(0) as u32;
            let m = data.get("max").and_then(Value::as_u64).unwrap_or(0) as u32;
            if m > 0 {
                ctl.set_step(v, m);
            }
        }
        "execution_error" | "execution_interrupted" | "execution_success" => return true,
        _ => {}
    }
    false
}

/// The stage a node class belongs to.
pub fn stage_for_class(class: &str) -> Stage {
    let c = class.to_ascii_lowercase();
    if c.contains("sampler") {
        Stage::Sampling
    } else if c.contains("decode") {
        Stage::Decoding
    } else if c == "saveimage" || c == "previewimage" {
        Stage::Saving
    } else if c.contains("loader") || c.contains("loadimage") || c.contains("cache") {
        Stage::Loading
    } else if c.contains("encode") || c.contains("conditioning") {
        Stage::Conditioning
    } else {
        Stage::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_parse_both_shapes() {
        let info = ObjectInfo(json!({
            "UNETLoader": {"input": {"required": {"unet_name": [["a/Qwen_Image_2.1_BF16.safetensors", "b.safetensors"]]}}},
            "CLIPLoader": {"input": {"required": {"type": ["COMBO", {"options": ["flux2", "qwen_image"]}]}}}
        }));
        assert_eq!(info.choices("CLIPLoader", "type"), vec!["flux2", "qwen_image"]);
        assert_eq!(info.find_file("UNETLoader", "unet_name", "qwen_image_2.1_bf16.safetensors").as_deref(), Some("a/Qwen_Image_2.1_BF16.safetensors"));
        assert!(info.find_file("UNETLoader", "unet_name", "missing.safetensors").is_none());
    }

    #[test]
    fn stages_follow_node_classes() {
        assert_eq!(stage_for_class("KSampler"), Stage::Sampling);
        assert_eq!(stage_for_class("SamplerCustomAdvanced"), Stage::Sampling);
        assert_eq!(stage_for_class("VAEDecodeTiled"), Stage::Decoding);
        assert_eq!(stage_for_class("UNETLoader"), Stage::Loading);
        assert_eq!(stage_for_class("CLIPTextEncode"), Stage::Conditioning);
        assert_eq!(stage_for_class("SaveImage"), Stage::Saving);
    }

    #[test]
    fn output_prefers_saved_images() {
        let e = json!({"outputs": {"41": {"images": [{"filename": "p.png", "type": "temp"}]}, "8": {"images": [{"filename": "o.png", "subfolder": "", "type": "output"}]}}});
        assert_eq!(first_output_image(&e).unwrap().0, "o.png");
        let e = json!({"outputs": {"41": {"images": [{"filename": "p.png", "type": "temp"}]}}});
        assert_eq!(first_output_image(&e).unwrap().2, "temp");
    }
}
