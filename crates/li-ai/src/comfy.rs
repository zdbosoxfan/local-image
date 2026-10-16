//! A blocking ComfyUI client: node inventory, image upload, queueing, progress over the
//! WebSocket, history polling, job-specific cancellation and result download.
//!
//! Behaviour follows the 0.7 Python client (`local_comfy_client.py`): submit once, poll
//! `/history/{id}` (0.15 s for 10 s, then 0.5 s, then 1 s, 20 min deadline), use the WebSocket for
//! progress only, and cancel only our own prompt (explicit Stop signals local controls as well as the job-specific API:
//! [`ComfyClient::interrupt_ours`]).

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use lightcraft_sysmem::jobs::Cancelled;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Upload subfolder inside ComfyUI's input directory.
pub const UPLOAD_SUBFOLDER: &str = "local-image";
const DEADLINE: Duration = Duration::from_secs(20 * 60);

#[cfg(test)]
#[path = "comfy_review_tests.rs"]
mod review_tests;

/// What a running job is doing, for the progress bar and status line: the shared
/// [`lightcraft_sysmem::JobStatus`] with this crate's [`Stage`] (`stage`, `step`, `message`;
/// `fraction()` is the step fraction).
pub type Progress = lightcraft_sysmem::JobStatus<Stage>;

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
///
/// A thin adapter over the app's one job primitive, [`lightcraft_sysmem::JobCtl`] with this
/// crate's [`Stage`] (it derefs to it: `cancel`, `is_cancelled`, `set_stage`, `set_message`,
/// `fraction`, `cancel_flag`, `interrupt` for `pc-raster` algorithms…). Only two methods differ:
/// [`JobControl::set_step`] also enters [`Stage::Sampling`], and [`JobControl::check`] returns an
/// `anyhow` error (still `is::<Cancelled>()`).
#[derive(Clone, Debug, Default)]
pub struct JobControl(lightcraft_sysmem::JobCtl<Stage>);

impl JobControl {
    pub fn new() -> Self {
        Self::default()
    }
    /// The shared job handle (for runners and code written against `JobCtl`).
    pub fn ctl(&self) -> &lightcraft_sysmem::JobCtl<Stage> {
        &self.0
    }
    /// Stage, sampling step and message (a snapshot).
    pub fn progress(&self) -> Progress {
        self.0.status()
    }
    /// Sampling step `value / max` (enters [`Stage::Sampling`]).
    pub fn set_step(&self, value: u32, max: u32) {
        self.0.set_stage(Stage::Sampling);
        self.0.set_step(value, max);
    }
    /// Fails with a "cancelled" error once cancellation was requested.
    pub fn check(&self) -> Result<()> {
        Ok(self.0.check()?)
    }
}

impl std::ops::Deref for JobControl {
    type Target = lightcraft_sysmem::JobCtl<Stage>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<lightcraft_sysmem::JobCtl<Stage>> for JobControl {
    fn from(ctl: lightcraft_sysmem::JobCtl<Stage>) -> Self {
        Self(ctl)
    }
}

/// Queue contention is transient; background checks must never persist it as unsupported.
#[derive(Debug)]
pub struct Busy;
impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ComfyUI is busy")
    }
}
impl std::error::Error for Busy {}

/// ComfyUI's node inventory (`/object_info`).
#[derive(Clone, Debug, Default)]
pub struct ObjectInfo(pub Value);

/// The limits ComfyUI's own sampler node reports for its inputs (`KSampler` in `/object_info`): what the backend accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerLimits {
    pub steps: (f32, f32),
    pub cfg: (f32, f32),
    pub denoise: (f32, f32),
}

impl ObjectInfo {
    /// The sampler's input limits as ComfyUI reports them (`KSampler`, else `KSamplerAdvanced`).
    pub fn sampler_limits(&self) -> Option<SamplerLimits> {
        let node = ["KSampler", "KSamplerAdvanced"].iter().find_map(|c| self.0.get(*c))?;
        let input = |name: &str| -> Option<(f32, f32)> {
            let spec = node.pointer("/input/required").and_then(|r| r.get(name)).or_else(|| node.pointer("/input/optional").and_then(|o| o.get(name)))?;
            let opts = spec.get(1)?;
            Some((opts.get("min")?.as_f64()? as f32, opts.get("max")?.as_f64()? as f32))
        };
        Some(SamplerLimits { steps: input("steps")?, cfg: input("cfg")?, denoise: input("denoise").unwrap_or((0.0, 1.0)) })
    }

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
        // ComfyUI API calls stay on this endpoint. In particular a connect error must not
        // be a later redirect failure after the original POST was already accepted.
        .max_redirects(0)
        // Direct LAN/Tailscale or loopback tunnel: do not inherit an unrelated system proxy.
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
        self.queue_prompt_with_id(graph, client_id, None)
    }

    fn queue_prompt_with_id(&self, graph: &Value, client_id: &str, proposed_id: Option<&str>) -> Result<String> {
        let mut body = json!({"prompt": graph, "client_id": client_id});
        if let Some(id) = proposed_id {
            body["prompt_id"] = json!(id);
        }
        let mut r = match self.agent.post(&self.url("/prompt")).send_json(body) {
            Ok(response) => response,
            Err(error) if submission_not_sent(&error) => return Err(anyhow!(SubmissionNotSent(format!("Cannot submit ComfyUI workflow: {error}")))),
            Err(error) => return Err(anyhow!(error).context("Queueing the workflow in ComfyUI failed")),
        };
        let status = r.status();
        let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
        if !status.is_success() {
            if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 405 | 422 | 429) {
                return Err(anyhow!(PromptRejected(format!("ComfyUI rejected the workflow: {}", describe_prompt_error(&v)))));
            }
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
            let response = self.quick.post(&self.url("/queue")).send_json(json!({ "delete": [prompt_id] }))?;
            if !response.status().is_success() {
                bail!("ComfyUI could not cancel the queued job ({})", response.status());
            }
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
        let response = self.quick.post(&self.url("/free")).send_json(json!({ "unload_models": true, "free_memory": true }))?;
        if !response.status().is_success() {
            bail!("ComfyUI could not unload models ({})", response.status());
        }
        Ok(())
    }

    /// Stops this process's running prompts using atomic job-specific cancellation, and
    /// signals their local job controls. Ordinary cancellation uses [`Self::cancel`].
    pub fn interrupt_ours(&self) -> Result<bool> {
        let prompts = OURS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|p| p.host == self.host)
            .map(|p| (p.id.clone(), p.control.clone()))
            .collect::<Vec<_>>();
        let ids = prompts.iter().filter_map(|(id, _)| id.clone()).collect::<Vec<_>>();
        for (_, control) in prompts {
            control.cancel();
        }
        self.interrupt_if_running(&ids)
    }

    /// [`Self::interrupt_ours`] for prompts noted earlier with [`our_prompts`] (before their jobs
    /// were cancelled, which takes them off the list).
    pub fn interrupt_if_running(&self, ours: &[String]) -> Result<bool> {
        if ours.is_empty() {
            return Ok(false);
        }
        let q = self.queue_state()?;
        let running = q.get("queue_running").and_then(Value::as_array);
        let mut stopped = false;
        for id in ours.iter().filter(|id| running.is_some_and(|items| items.iter().any(|i| item_has(i, id)))) {
            // ComfyUI's job endpoint checks and interrupts under the queue lock. A stale
            // snapshot can only become a harmless no-op, never an interrupt of another job.
            let mut response = self.quick.post(&self.url(&format!("/api/jobs/{}/cancel", enc(id)))).send_json(json!({}))?;
            if !response.status().is_success() {
                bail!("This ComfyUI version cannot stop a single running job. Update ComfyUI, or wait for it to finish.");
            }
            let result: Value = response.body_mut().read_json()?;
            stopped |= result["cancelled"].as_bool().unwrap_or(false);
        }
        Ok(stopped)
    }

    /// Runs `graph`: uploads `images` (node id → PNG bytes) into those `LoadImage` nodes, queues,
    /// reports progress into `ctl`, and returns the first output image's bytes.
    pub fn run(&self, graph: Value, images: &[(String, Vec<u8>)], ctl: &JobControl) -> Result<Vec<u8>> {
        self.run_with_admission(graph, images, ctl, false, None)
    }

    /// Background checks repeat queue admission after uploads, immediately before submission.
    pub fn run_when_idle(&self, graph: Value, images: &[(String, Vec<u8>)], ctl: &JobControl) -> Result<Vec<u8>> {
        self.run_with_admission(graph, images, ctl, true, None)
    }

    pub(crate) fn run_when_idle_checked(
        &self,
        graph: Value,
        images: &[(String, Vec<u8>)],
        ctl: &JobControl,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<u8>> {
        self.run_with_admission(graph, images, ctl, true, Some(check))
    }

    fn run_with_admission(
        &self,
        graph: Value,
        images: &[(String, Vec<u8>)],
        ctl: &JobControl,
        idle_only: bool,
        check: Option<&dyn Fn() -> Result<()>>,
    ) -> Result<Vec<u8>> {
        self.run_bounded(graph, images, ctl, idle_only, check, DEADLINE, Duration::from_secs(30))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_bounded(
        &self,
        mut graph: Value,
        images: &[(String, Vec<u8>)],
        ctl: &JobControl,
        idle_only: bool,
        check: Option<&dyn Fn() -> Result<()>>,
        deadline: Duration,
        unhealthy_limit: Duration,
    ) -> Result<Vec<u8>> {
        ctl.check()?;
        ctl.set_stage(Stage::Uploading);
        for (node, png) in images {
            let name = self.upload_png(png)?;
            let slot = graph.get_mut(node).and_then(|n| n.get_mut("inputs")).ok_or_else(|| anyhow!("workflow has no node {node}"))?;
            slot["image"] = Value::String(name);
            ctl.check()?;
        }
        if idle_only && !crate::refine::queue_idle(self)? {
            return Err(anyhow!(Busy));
        }
        let client_id = uuid::Uuid::new_v4().to_string();
        let socket = ProgressSocket::start(&self.host, &client_id, ctl.clone());
        ctl.set_stage(Stage::Queued);
        if let Some(check) = check {
            check()?;
            ctl.check()?;
            if idle_only && !crate::refine::queue_idle(self)? {
                return Err(anyhow!(Busy));
            }
        }
        ctl.check()?;
        let permit = PromptPermit::reserve()?;
        let vram = lightcraft_sysmem::gpu::AiJob::new(crate::resources::local(&self.host));
        // Reserve ownership BEFORE POST: an accepted prompt can outlive a lost/malformed reply.
        // Current ComfyUI accepts proposed UUIDs; older versions are reconciled by client_id.
        let mut ours = OurPrompt::new(&self.host, &client_id, ctl, permit, vram);
        let prompt_id = match self.queue_prompt_with_id(&graph, &client_id, Some(&client_id)) {
            Ok(id) => id,
            Err(error) => {
                ours.terminal = error.is::<PromptRejected>() || error.is::<SubmissionNotSent>();
                if ours.terminal {
                    return Err(error);
                }
                return Err(error.context("ComfyUI submission failed; uncertain submissions remain owned for reconciliation"));
            }
        };
        ours.identify(&prompt_id);
        if let Some(s) = &socket {
            s.set_prompt(&prompt_id, &graph);
        }
        let started = Instant::now();
        let mut cancel_sent = false;
        let mut last_cancel_attempt = started - Duration::from_secs(1);
        let mut unhealthy_since: Option<Instant> = None;
        let mut last_queue_check = started - Duration::from_secs(1);
        let mut last_poll_error = String::new();
        loop {
            let elapsed = started.elapsed();
            if elapsed > deadline {
                let _ = self.cancel(&prompt_id);
                bail!("ComfyUI did not finish within 20 minutes");
            }
            if ctl.is_cancelled() && !cancel_sent && last_cancel_attempt.elapsed() >= Duration::from_secs(1) {
                last_cancel_attempt = Instant::now();
                // Keep ownership and poll to a confirmed terminal state after transient
                // cancellation failures. Never abandon the live server prompt on `?`.
                match self.cancel(&prompt_id) {
                    Ok(()) => {
                        cancel_sent = true;
                        ctl.set_message("Cancellation requested; waiting for ComfyUI");
                    }
                    Err(e) => ctl.set_message(format!("Cancellation pending; retrying: {e:#}")),
                }
            }
            match self.history(&prompt_id) {
                Ok(Some(entry)) => {
                    unhealthy_since = None;
                    if let Some(err) = history_error(&entry) {
                        ours.terminal = true;
                        if ctl.is_cancelled() {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                        bail!("{err}");
                    }
                    if let Some((file, sub, kind)) = first_output_image(&entry) {
                        ours.terminal = true;
                        if ctl.is_cancelled() {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                        ctl.set_stage(Stage::Finishing);
                        let bytes = self.view(&file, &sub, &kind)?;
                        if ctl.is_cancelled() {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                        ctl.set_stage(Stage::Done);
                        return Ok(bytes);
                    }
                    if entry.get("status").and_then(|s| s.get("completed")).and_then(Value::as_bool) == Some(true) {
                        ours.terminal = true;
                        if ctl.is_cancelled() {
                            ctl.set_stage(Stage::Cancelled);
                            return Err(anyhow!(Cancelled));
                        }
                        bail!("ComfyUI finished without an output image");
                    }
                }
                Ok(None) => {
                    if last_queue_check.elapsed() >= Duration::from_secs(1) || cancel_sent {
                        last_queue_check = Instant::now();
                        match self.queue_state().and_then(|queue| queue_contains(&queue, &prompt_id)) {
                            Ok(true) => unhealthy_since = None,
                            Ok(false) => match self.history(&prompt_id) {
                                Ok(None) => {
                                    if ctl.is_cancelled() {
                                        ours.terminal = true;
                                        ctl.set_stage(Stage::Cancelled);
                                        return Err(anyhow!(Cancelled));
                                    }
                                    last_poll_error = "The job is no longer in ComfyUI's queue or history. The server may have restarted; try again.".into();
                                    unhealthy_since.get_or_insert_with(Instant::now);
                                }
                                Ok(Some(_)) => unhealthy_since = None,
                                Err(e) => {
                                    last_poll_error = format!("Cannot confirm ComfyUI job completion: {e:#}");
                                    unhealthy_since.get_or_insert_with(Instant::now);
                                }
                            },
                            Err(e) => {
                                last_poll_error = format!("Cannot read the ComfyUI queue: {e:#}");
                                unhealthy_since.get_or_insert_with(Instant::now);
                            }
                        }
                    }
                }
                Err(e) => {
                    last_poll_error = format!("Cannot read ComfyUI job history: {e:#}");
                    unhealthy_since.get_or_insert_with(Instant::now);
                }
            }
            if unhealthy_since.is_some_and(|t| t.elapsed() >= unhealthy_limit) {
                ctl.set_stage(Stage::Failed);
                bail!("{last_poll_error}");
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

/// Prompts this process queued and still waits for, as `(host, prompt id)`.
struct OwnedPrompt {
    host: String,
    client_id: String,
    id: Option<String>,
    control: JobControl,
    unresolved: bool,
    _permit: PromptPermit,
    local: bool,
    vram: Option<lightcraft_sysmem::gpu::AiJob>,
    idle_observations: u8,
}
static OURS: Mutex<Vec<OwnedPrompt>> = Mutex::new(Vec::new());
static CLEANUP_WAKE: Condvar = Condvar::new();
static CLEANUP_STARTED: AtomicBool = AtomicBool::new(false);
static OWNED_COUNT: AtomicUsize = AtomicUsize::new(0);
const MAX_OWNED: usize = 64;

struct PromptPermit;
impl PromptPermit {
    fn reserve() -> Result<Self> {
        OWNED_COUNT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < MAX_OWNED).then_some(n + 1))
            .map_err(|_| anyhow!("Too many unresolved ComfyUI jobs. Reconnect to finish cancellation before starting another."))?;
        Ok(Self)
    }
}
impl Drop for PromptPermit {
    fn drop(&mut self) {
        OWNED_COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The prompts this process is waiting for on `host`.
pub fn our_prompts(host: &str) -> Vec<String> {
    OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().filter(|p| p.host == host).filter_map(|p| p.id.clone()).collect()
}

/// Lists a prompt in [`OURS`] while [`ComfyClient::run`] waits for it.
struct OurPrompt {
    host: String,
    client_id: String,
    terminal: bool,
}

impl OurPrompt {
    fn new(host: &str, client_id: &str, ctl: &JobControl, permit: PromptPermit, vram: lightcraft_sysmem::gpu::AiJob) -> Self {
        OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(OwnedPrompt {
            host: host.into(),
            client_id: client_id.into(),
            id: None,
            control: ctl.clone(),
            unresolved: false,
            _permit: permit,
            local: crate::resources::local(host),
            vram: Some(vram),
            idle_observations: 0,
        });
        Self { host: host.into(), client_id: client_id.into(), terminal: false }
    }

    fn identify(&self, id: &str) {
        if let Some(prompt) =
            OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter_mut().find(|p| p.host == self.host && p.client_id == self.client_id)
        {
            prompt.id = Some(id.into());
        }
    }
}

impl Drop for OurPrompt {
    fn drop(&mut self) {
        let mut owned = OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.terminal {
            owned.retain(|p| !(p.host == self.host && p.client_id == self.client_id));
        } else {
            if let Some(prompt) = owned.iter_mut().find(|p| p.host == self.host && p.client_id == self.client_id) {
                prompt.unresolved = true;
                prompt.control.cancel();
            }
            drop(owned);
            start_cleanup();
        }
    }
}

/// One sleeping worker reconciles failed/timed-out prompts. Ownership and the local GPU
/// reservation survive until the backend confirms a terminal state; no result is published.
fn start_cleanup() {
    if CLEANUP_STARTED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
        && let Err(error) = std::thread::Builder::new().name("comfy-cleanup".into()).spawn(cleanup_loop)
    {
        CLEANUP_STARTED.store(false, Ordering::Release);
        log::warn!("Could not start ComfyUI cancellation cleanup: {error}");
    }
    CLEANUP_WAKE.notify_one();
}

fn cleanup_loop() {
    lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
    loop {
        let owned = OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let owned = CLEANUP_WAKE.wait_while(owned, |p| !p.iter().any(|p| p.unresolved)).unwrap_or_else(std::sync::PoisonError::into_inner);
        let pending: Vec<_> = owned.iter().filter(|p| p.unresolved).map(|p| (p.host.clone(), p.client_id.clone(), p.id.clone())).collect();
        drop(owned);
        for (host, client_id, id) in pending {
            let client = ComfyClient::new(&host);
            let observation = if id.is_none() { find_submission(&client, &client_id).ok() } else { None };
            if id.is_none() {
                let mut owned = OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(prompt) = owned.iter_mut().find(|p| p.host == host && p.client_id == client_id) {
                    update_submission_pressure(prompt, observation.as_ref());
                }
            }
            let id = id.or_else(|| observation.and_then(|observation| observation.id));
            if let Some(id) = id {
                {
                    let mut owned = OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(prompt) = owned.iter_mut().find(|p| p.host == host && p.client_id == client_id) {
                        prompt.id = Some(id.clone());
                    }
                }
                if reconcile_prompt(&client, &id).unwrap_or(false) {
                    OURS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|p| !(p.unresolved && p.host == host && p.client_id == client_id));
                }
            }
        }
        std::thread::sleep(if cfg!(test) { Duration::from_millis(100) } else { Duration::from_secs(5) });
    }
}

#[derive(Debug)]
struct PromptRejected(String);
impl std::fmt::Display for PromptRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PromptRejected {}

#[derive(Debug)]
struct SubmissionNotSent(String);
impl std::fmt::Display for SubmissionNotSent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SubmissionNotSent {}

fn submission_not_sent(error: &ureq::Error) -> bool {
    matches!(
        error,
        ureq::Error::BadUri(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Timeout(ureq::Timeout::Connect | ureq::Timeout::Resolve)
    ) || matches!(error, ureq::Error::Io(error) if error.kind() == std::io::ErrorKind::ConnectionRefused)
}

struct SubmissionObservation {
    id: Option<String>,
    busy: bool,
}

fn update_submission_pressure(prompt: &mut OwnedPrompt, observation: Option<&SubmissionObservation>) {
    match observation {
        Some(observation) if observation.busy || observation.id.is_some() => {
            prompt.idle_observations = 0;
            if prompt.vram.is_none() {
                prompt.vram = Some(lightcraft_sysmem::gpu::AiJob::new(prompt.local));
            }
        }
        Some(_) => {
            prompt.idle_observations = prompt.idle_observations.saturating_add(1);
            if prompt.idle_observations >= 3 {
                // The queue proves no GPU prompt is active now. Keep identity/permit tracking
                // for delayed validation; restore pressure if any later work appears.
                prompt.vram = None;
            }
        }
        None => {
            prompt.idle_observations = 0;
            if prompt.vram.is_none() {
                prompt.vram = Some(lightcraft_sysmem::gpu::AiJob::new(prompt.local));
            }
        }
    }
}

/// Queue and history retain (number, prompt_id, graph, extra_data, ...) in official ComfyUI.
/// Match the unique per-submission client_id, including servers that ignore proposed IDs.
fn find_submission(client: &ComfyClient, client_id: &str) -> Result<SubmissionObservation> {
    let matches = |item: &Value| (item[3]["client_id"].as_str() == Some(client_id)).then(|| item[1].as_str().map(str::to_owned)).flatten();
    let queue = client.queue_state()?;
    let mut busy = false;
    let mut found = None;
    for key in ["queue_running", "queue_pending"] {
        let items = queue[key].as_array().with_context(|| format!("Invalid ComfyUI queue: missing {key}"))?;
        busy |= !items.is_empty();
        found = found.or_else(|| items.iter().find_map(matches));
    }
    if found.is_some() {
        return Ok(SubmissionObservation { id: found, busy });
    }
    if let Ok(Some(entry)) = client.history(client_id)
        && let Some(id) = matches(&entry["prompt"])
    {
        return Ok(SubmissionObservation { id: Some(id), busy });
    }
    // A matching completed prompt may no longer be in the queue. Never equate no match with
    // rejection: validation may still be running, or history may have been pruned.
    let id = client
        .get_json(&client.quick, "/history?max_items=64")
        .ok()
        .and_then(|history| history.as_object().and_then(|entries| entries.values().find_map(|entry| matches(&entry["prompt"]))));
    Ok(SubmissionObservation { id, busy })
}

fn reconcile_prompt(client: &ComfyClient, id: &str) -> Result<bool> {
    let terminal = |entry: &Value| history_error(entry).is_some() || first_output_image(entry).is_some() || entry["status"]["completed"] == true;
    if let Ok(Some(entry)) = client.history(id)
        && terminal(&entry)
    {
        return Ok(true);
    }
    let listed = queue_contains(&client.queue_state()?, id)?;
    if listed {
        client.cancel(id)?;
        return Ok(false); // Cancellation acknowledgement is not terminal-state proof.
    }
    // A valid fresh queue proves it is neither running nor pending, even if the history
    // endpoint itself remains broken. An explicitly unfinished entry is inconsistent: retry.
    Ok(match client.history(id) {
        Ok(Some(entry)) => terminal(&entry),
        Ok(None) | Err(_) => true,
    })
}

fn queue_contains(queue: &Value, id: &str) -> Result<bool> {
    let mut listed = false;
    for key in ["queue_running", "queue_pending"] {
        let items = queue[key].as_array().with_context(|| format!("Invalid ComfyUI queue: missing {key}"))?;
        listed |= items.iter().any(|item| item_has(item, id));
    }
    Ok(listed)
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
    fn job_control_is_a_view_of_the_shared_jobctl() {
        let ctl = JobControl::new();
        let shared = ctl.ctl().clone();
        ctl.set_stage(Stage::Loading);
        ctl.set_step(2, 8);
        let p = ctl.progress();
        assert_eq!((p.stage, p.step, p.fraction()), (Stage::Sampling, Some((2, 8)), Some(0.25)));
        assert_eq!(shared.fraction(), Some(0.25));
        // pc-raster algorithms report through the borrowed view; an li-seg/pc-raster cancel is
        // the same `Cancelled` an AI job ends with
        ctl.interrupt().progress(0.5);
        assert_eq!(shared.fraction(), Some(0.5));
        shared.cancel();
        assert!(ctl.is_cancelled() && ctl.cancel_flag().load(Ordering::SeqCst));
        assert!(ctl.check().unwrap_err().is::<Cancelled>());
        assert!(anyhow::Error::new(ctl.interrupt().check().unwrap_err()).is::<lightcraft_sysmem::jobs::Cancelled>());
        let wrapped = JobControl::from(shared);
        assert!(wrapped.is_cancelled());
    }

    #[test]
    fn cancellation_retries_keep_prompt_ownership_and_never_fetch_cancelled_output() {
        for finishes_during_retry in [false, true] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let host = server.server_addr().to_string();
            let ctl = JobControl::new();
            let cancel = ctl.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let seen = requests.clone();
            let owned = Arc::new(AtomicBool::new(false));
            let still_owned = owned.clone();
            let server_host = host.clone();
            let worker = std::thread::spawn(move || {
                let (mut attempts, mut queue_failed) = (0, false);
                let deadline = Instant::now() + Duration::from_secs(8);
                while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
                    let Some(mut req) = server.recv_timeout(Duration::from_millis(50)).unwrap() else { continue };
                    let path = req.url().to_string();
                    let mut body = String::new();
                    req.as_reader().read_to_string(&mut body).unwrap();
                    seen.lock().unwrap().push(path.clone());
                    let (status, value) = match path.as_str() {
                        "/prompt" => {
                            cancel.cancel();
                            (200, json!({"prompt_id":"ours"}))
                        }
                        "/api/jobs/ours/cancel" => {
                            attempts += 1;
                            if attempts > 1 {
                                still_owned.store(our_prompts(&server_host).contains(&"ours".to_string()), Ordering::SeqCst);
                            }
                            (if attempts == 1 { 503 } else { 200 }, json!({"cancelled": attempts > 1}))
                        }
                        "/queue" if !queue_failed => {
                            queue_failed = true;
                            (503, json!({}))
                        }
                        "/queue" => (200, json!({"queue_running":[[0,"ours"],[1,"unrelated"]],"queue_pending":[]})),
                        "/history/ours" if finishes_during_retry => {
                            (200, json!({"ours":{"status":{"completed":true},"outputs":{"1":{"images":[{"filename":"must-not-fetch.png","type":"output"}]}}}}))
                        }
                        "/history/ours" if attempts > 1 => {
                            (200, json!({"ours":{"status":{"status_str":"error","messages":[["execution_interrupted",{}]]},"outputs":{}}}))
                        }
                        "/history/ours" => (200, json!({})),
                        _ => (404, json!({})),
                    };
                    let _ = req.respond(tiny_http::Response::from_string(value.to_string()).with_status_code(status));
                }
            });
            let started = Instant::now();
            let result = ComfyClient::new(&host).run(json!({}), &[], &ctl);
            stop.store(true, Ordering::SeqCst);
            worker.join().unwrap();
            assert!(result.unwrap_err().is::<Cancelled>());
            assert!(started.elapsed() < Duration::from_secs(5));
            assert!(our_prompts(&host).is_empty());
            let calls = requests.lock().unwrap();
            assert!(!calls.iter().any(|p| p == "/interrupt" || p.starts_with("/view") || p.contains("unrelated")), "{calls:?}");
            if !finishes_during_retry {
                assert_eq!(calls.iter().filter(|p| p.as_str() == "/api/jobs/ours/cancel").count(), 2);
                assert!(owned.load(Ordering::SeqCst), "a failed cancellation lost prompt ownership");
            }
        }
    }

    #[test]
    fn bugfix_develop_lost_comfy_prompt_fails_before_deadline() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = server.server_addr().to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                if let Some(req) = server.recv_timeout(Duration::from_millis(100)).unwrap() {
                    let value = match req.url() {
                        "/prompt" => json!({"prompt_id":"lost"}),
                        "/queue" => json!({"queue_running":[],"queue_pending":[]}),
                        _ => json!({}),
                    };
                    let _ = req.respond(tiny_http::Response::from_string(value.to_string()));
                }
            }
        });
        let ctl = JobControl::new();
        let abort = ctl.clone();
        let timer = std::thread::spawn(move || {
            for _ in 0..350 {
                if abort.is_cancelled() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            abort.cancel();
        });
        let started = Instant::now();
        let result = ComfyClient::new(host).run(json!({}), &[], &ctl);
        ctl.cancel();
        timer.join().unwrap();
        stop.store(true, Ordering::SeqCst);
        worker.join().unwrap();
        let error = result.unwrap_err().to_string();
        assert!(error.contains("no longer in ComfyUI"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(34));
    }

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
