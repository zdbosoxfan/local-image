//! Background jobs: long commands run on a worker thread, report progress and can be cancelled
//! (#210).
//!
//! A command opts in by doing its heavy work through [`run`] (or [`edit_job`] for a plain document
//! edit). The work closure gets the app's one job handle, [`lightcraft_sysmem::JobCtl`]
//! (`report(fraction, message)` — progress never goes backwards —, `check()?`, `stage` for an
//! algorithm's [`Interrupt`](lightcraft_sysmem::jobs::Interrupt), `cancel_flag` for `&AtomicBool`
//! APIs), and runs against
//! a snapshot: documents are copy-on-write, so cloning one is O(layers). Its result is applied on
//! the UI thread as one undo step, through the same path as [`Session::execute`], so the journal,
//! history, damage and post-command hooks all behave as for a synchronous command.
//!
//! - [`Session::execute`] runs everything inline, as before: tests, the CLI and scripts see no
//!   difference.
//! - [`Session::start`] runs a job-capable command on a worker thread and returns its
//!   [`JobId`]; [`Session::poll_jobs`] (called every frame by the UI) applies finished jobs.
//!   On wasm `start` runs inline (same API, written once).
//! - [`Session::cancel_job`] sets the flag; the algorithms check it per tile, row band or
//!   iteration, and the result is discarded. The document is never touched by a worker.
//! - A panic in the worker becomes an error; the document is unchanged.
//! - While a job runs on a document, commands that would edit that document are disabled with a
//!   message naming the job (queries and document-independent commands still run).

use std::any::Any;
use std::sync::{Arc, Weak};

use lightcraft_sysmem::JobCtl;

use photocraft_doc::{DocId, Document, LayerId};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

/// The type of [`Session::authorize`].
type Authorization = fn(&str, &Value) -> Result<()>;

thread_local! {
    // Smart-filter rendering has no Session argument, and stored filter params (a Color Lookup
    // `file`, a Displace `mapPath`) can come from an untrusted document. The requesting session's
    // gate is carried through synchronous rendering here and reinstalled on background workers.
    static RENDER_AUTHORIZATION: std::cell::Cell<Option<Authorization>> = const { std::cell::Cell::new(None) };
}

/// Installs a render gate on this thread until dropped, then restores the previous one.
pub(crate) struct AuthorizationScope(Option<Authorization>);

impl AuthorizationScope {
    /// Enter with `gate`, or keep the gate already installed when `gate` is `None`, so trusted
    /// work nested inside an untrusted request never widens its authority.
    pub(crate) fn enter(gate: Option<Authorization>) -> Self {
        Self(RENDER_AUTHORIZATION.with(|current| current.replace(gate.or(current.get()))))
    }
}

impl Drop for AuthorizationScope {
    fn drop(&mut self) {
        RENDER_AUTHORIZATION.with(|current| current.set(self.0));
    }
}

/// Recheck a stored filter's command and params with the requesting session's gate before it is
/// evaluated. Trusted work (no gate installed) passes.
pub(crate) fn authorize_render_command(id: &str, params: &Value) -> Result<()> {
    match RENDER_AUTHORIZATION.with(std::cell::Cell::get) {
        Some(gate) => gate(id, params),
        None => Ok(()),
    }
}

/// Whether the work on this thread is done for an untrusted session (a gate is installed).
pub(crate) fn authorization_installed() -> bool {
    RENDER_AUTHORIZATION.with(std::cell::Cell::get).is_some()
}

#[cfg(not(target_arch = "wasm32"))]
mod open_pool;

#[cfg(not(target_arch = "wasm32"))]
enum WorkerHandle {
    Thread(std::thread::JoinHandle<()>),
    Open(Arc<open_pool::Completion>),
}
#[cfg(not(target_arch = "wasm32"))]
impl WorkerHandle {
    fn is_finished(&self) -> bool {
        match self {
            Self::Thread(thread) => thread.is_finished(),
            Self::Open(done) => done.is_finished(),
        }
    }
    fn join(self) {
        match self {
            Self::Thread(thread) => {
                let _ = thread.join();
            }
            Self::Open(done) => done.wait(),
        }
    }
}

/// Identifies one background job for the session's lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
pub struct JobId(pub u64);

/// A cancelled job's check (`job.check()?`) is the engine's [`EngineError::Cancelled`].
impl From<lightcraft_sysmem::Cancelled> for EngineError {
    fn from(_: lightcraft_sysmem::Cancelled) -> Self {
        EngineError::Cancelled
    }
}

/// Final state of a job.
#[derive(Clone, Debug, PartialEq)]
pub enum JobOutcome {
    /// Applied; the command's result.
    Done(Value),
    /// Failed (including a panic in the worker); the document is unchanged.
    Failed(String),
    /// Cancelled; the result (if any) was discarded and the document is unchanged.
    Cancelled,
}

/// A job that has ended, as reported by [`Session::poll_jobs`].
#[derive(Clone, Debug, PartialEq)]
pub struct JobEvent {
    pub id: JobId,
    pub command: String,
    pub label: String,
    pub document: Option<DocId>,
    pub outcome: JobOutcome,
}

/// A job's state for the UI, the control channel and MCP (`jobs.list`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub id: JobId,
    pub command: String,
    /// Shown in the progress UI ("Gaussian Blur").
    pub label: String,
    /// The document the job edits (and locks), if any.
    pub document: Option<DocId>,
    /// `running`, `done`, `failed` or `cancelled`.
    pub state: &'static str,
    pub progress: f32,
    pub message: String,
    pub elapsed_ms: f64,
    /// The command's result (done) or the error (failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

type Payload = Box<dyn Any + Send>;
type Apply = Box<dyn FnOnce(&mut Session, Payload) -> Result<Value> + Send>;

/// Wall-clock milliseconds since some fixed point (0 on wasm, where `Instant` is unavailable and
/// jobs run inline anyway).
fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
    }
    #[cfg(target_arch = "wasm32")]
    {
        0.0
    }
}

struct Running {
    id: JobId,
    command: String,
    label: String,
    params: Value,
    /// Requests with overlapping keys supersede this job (Develop layers).
    latest_keys: Vec<String>,
    journal: bool,
    doc: Option<DocId>,
    /// The document as the job saw it: applying fails if it was edited meanwhile.
    snapshot: Option<Weak<Document>>,
    /// Explicitly acknowledged disclosure changes must survive a snapshot-based result.
    preserve_group_view: bool,
    ctx: JobCtl,
    started_ms: f64,
    coalesce: Option<String>,
    color_restrict: Option<usize>,
    rx: std::sync::mpsc::Receiver<Result<Payload>>,
    apply: Option<Apply>,
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<WorkerHandle>,
}

impl Running {
    fn info(&self) -> JobInfo {
        JobInfo {
            id: self.id,
            command: self.command.clone(),
            label: self.label.clone(),
            document: self.doc,
            state: "running",
            progress: self.ctx.fraction().unwrap_or(0.0),
            message: self.ctx.status().message,
            elapsed_ms: now_ms() - self.started_ms,
            result: None,
            error: None,
        }
    }
}

/// The session's job table (see the module docs).
#[derive(Default)]
pub struct Jobs {
    next: u64,
    /// Set while [`Session::start`] runs a command: [`run`] spawns instead of running inline.
    spawn: bool,
    /// File opens use the bounded process-wide coordinators, including before the first read.
    open_queue: bool,
    /// The job a command registered while `spawn` was set (picked up by `start`).
    pending: Option<Running>,
    running: Vec<Running>,
    /// Ended jobs not yet collected by [`Session::poll_jobs`].
    events: Vec<JobEvent>,
    /// The last few ended jobs, for `jobs.list`.
    recent: Vec<JobInfo>,
    /// Workers of cancelled jobs that may still be unwinding (native).
    #[cfg(not(target_arch = "wasm32"))]
    draining: Vec<WorkerHandle>,
}

const RECENT: usize = 16;

impl Jobs {
    /// Source replacement must not invalidate an in-flight edit or source render.
    pub(crate) fn conflicts_source_rebase(
        &self,
        documents: &std::collections::HashSet<DocId>,
        keys: &std::collections::HashSet<String>,
        mut matches_path: impl FnMut(&str) -> bool,
    ) -> bool {
        self.running.iter().any(|job| {
            job.doc.is_some_and(|doc| documents.contains(&doc))
                || job.latest_keys.iter().any(|key| keys.contains(key))
                || job.params.get("path").and_then(Value::as_str).is_some_and(&mut matches_path)
        })
    }

    /// Advance a job snapshot across an explicitly view-only group disclosure change.
    /// Never acknowledge an intervening content edit: its pointer will not match `before`.
    pub(crate) fn rebase_group_view(&mut self, doc: DocId, before: &Weak<Document>, after: Weak<Document>) {
        for job in self.running.iter_mut().chain(self.pending.iter_mut()) {
            if job.doc == Some(doc) && job.snapshot.as_ref().is_some_and(|snapshot| Weak::ptr_eq(snapshot, before)) {
                job.snapshot = Some(after.clone());
                job.preserve_group_view = true;
            }
        }
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        // Workers own snapshots only; stop them so they don't burn CPU for a closed session.
        for r in self.running.iter().chain(self.pending.iter()) {
            r.ctx.cancel();
        }
        #[cfg(not(target_arch = "wasm32"))]
        open_pool::discard_cancelled();
    }
}

impl std::fmt::Debug for Jobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jobs").field("running", &self.running.len()).finish()
    }
}

/// How [`Session::start`] ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Started {
    /// The command ran to completion inline (it isn't a job, or this is wasm).
    Done(Value),
    /// The command is running in the background.
    Job(JobId),
}

/// Commands that never edit the active document, allowed while a job runs on it.
fn independent_of_document(id: &str) -> bool {
    const PREFIXES: &[&str] = &["jobs.", "tools.", "view.", "window.", "help.", "brush.presets.", "gradient.presets."];
    const IDS: &[&str] = &[
        "develop.openPhoto",
        "develop.syncPhoto",
        "file.new",
        "file.close",
        "file.closeAll",
        "edit.preferences",
        "edit.colorSettings",
        "edit.keyboardShortcuts",
        "edit.menus",
    ];
    PREFIXES.iter().any(|p| id.starts_with(p)) || IDS.contains(&id)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_path(path: &str) -> Result<Vec<u8>> {
    // Bounded reads, and a clear error for a file larger than memory (#375).
    photocraft_format::read_file(std::path::Path::new(path)).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn read_path(path: &str) -> Result<Vec<u8>> {
    Err(EngineError::Other(format!("cannot open paths on the web: {path}")))
}

/// Run a job-capable command's heavy part. `work` runs on a worker thread when the command was
/// started with [`Session::start`] (inline otherwise, and always on wasm); `apply` then runs on
/// the calling thread with its output and returns the command's result. With `lock_document`,
/// the active document is locked against other edits while the job runs, and applying fails if
/// it was edited anyway.
///
/// `work` must only read what it captured (clone the document `Arc` first): it never sees the
/// session.
pub fn run<T: Send + 'static>(
    s: &mut Session,
    label: &str,
    lock_document: bool,
    work: impl FnOnce(&JobCtl) -> Result<T> + Send + 'static,
    apply: impl FnOnce(&mut Session, T) -> Result<Value> + Send + 'static,
) -> Result<Value> {
    // The worker and the later apply run outside the request that started the job (the desktop
    // drops its automation gate as soon as the request returns), so both carry its gate.
    let authorization = s.authorize.or_else(|| RENDER_AUTHORIZATION.with(std::cell::Cell::get));
    let _authorization = AuthorizationScope::enter(authorization);
    if !s.jobs.spawn || cfg!(target_arch = "wasm32") {
        let t = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&JobCtl::new())))
            .map_err(|_| EngineError::Other("Import or job failed unexpectedly; the document is unchanged".into()))??;
        return apply(s, t);
    }
    if s.jobs.pending.is_some() {
        return Err(EngineError::Other(format!("`{label}` started a second background job in one command")));
    }
    let (doc, snapshot) = match (lock_document, s.active()) {
        (true, Some(st)) => (Some(st.doc.id), Some(Arc::downgrade(&st.doc))),
        (true, None) => return Err(EngineError::NoDocument),
        (false, _) => (None, None),
    };
    let ctx = JobCtl::new();
    let (tx, rx) = std::sync::mpsc::channel::<Result<Payload>>();
    let erased: Apply = Box::new(move |s, any| {
        let _authorization = AuthorizationScope::enter(authorization);
        let t = any.downcast::<T>().map_err(|_| EngineError::Other("internal error: job result type mismatch".into()))?;
        apply(s, *t)
    });
    s.jobs.next += 1;
    let id = JobId(s.jobs.next);
    #[cfg(not(target_arch = "wasm32"))]
    let handle = {
        let wctx = ctx.clone();
        let label_w = label.to_string();
        // The work closure is moved into the thread. If spawning fails (thread limits), there is
        // no way to get it back, so the error is reported instead.
        let task = move || {
            let _authorization = AuthorizationScope::enter(authorization);
            // local-image: a job with a progress bar (threading rule 7); fails soft.
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&wctx).map(|t| Box::new(t) as Payload)))
                .unwrap_or_else(|_| Err(EngineError::Other(format!("`{label_w}` failed with an internal error (logged); the document is unchanged"))));
            // The receiver is gone when the job was cancelled: nothing to report.
            let _ = tx.send(r);
        };
        Some(if s.jobs.open_queue {
            WorkerHandle::Open(open_pool::submit(ctx.clone(), task).map_err(EngineError::Other)?)
        } else {
            WorkerHandle::Thread(
                std::thread::Builder::new()
                    .name(format!("photocraft-job-{}", id.0))
                    .spawn(task)
                    .map_err(|e| EngineError::Other(format!("could not start a background job: {e}")))?,
            )
        })
    };
    #[cfg(target_arch = "wasm32")]
    {
        // Unreachable (wasm runs inline above); keep the types used.
        drop((work, tx));
    }
    s.jobs.pending = Some(Running {
        id,
        command: String::new(),
        label: label.to_string(),
        params: Value::Null,
        latest_keys: Vec::new(),
        journal: true,
        doc,
        snapshot,
        preserve_group_view: false,
        ctx,
        started_ms: now_ms(),
        coalesce: s.coalesce_request.clone(),
        color_restrict: s.color_restrict,
        rx,
        apply: Some(erased),
        #[cfg(not(target_arch = "wasm32"))]
        handle,
    });
    Ok(json!({"job": id.0}))
}

/// Like `run`, but a newer request cancels any pending result with an overlapping key.
/// These jobs do not lock documents: their apply closure validates only the affected layers,
/// so painting on other layers can continue while the old Develop pixels remain visible.
pub(crate) fn run_latest<T: Send + 'static>(
    s: &mut Session,
    label: &str,
    keys: Vec<String>,
    work: impl FnOnce(&JobCtl) -> Result<T> + Send + 'static,
    apply: impl FnOnce(&mut Session, T) -> Result<Value> + Send + 'static,
) -> Result<Value> {
    cancel_latest(s, &keys);
    let result = run(s, label, false, work, apply);
    if let Some(job) = &mut s.jobs.pending {
        job.latest_keys = keys;
    }
    result
}

pub(crate) fn latest(s: &Session, keys: &[String], settings: &Value) -> Option<JobId> {
    s.jobs.running.iter().find(|j| j.latest_keys == keys && j.params.get("settings") == Some(settings)).map(|j| j.id)
}

pub(crate) fn cancel_latest(s: &mut Session, keys: &[String]) {
    let ids: Vec<_> = s.jobs.running.iter().filter(|j| j.latest_keys.iter().any(|k| keys.contains(k))).map(|j| j.id).collect();
    for id in ids {
        s.cancel_job(id);
    }
}

/// A document edit whose body is the job: like [`Session::edit`] (one undo step named `label`),
/// but `f` runs on a worker against a copy of the active document when started in the
/// background. `finish` turns `f`'s output into the command's result.
pub fn edit_job<R: Send + 'static>(
    s: &mut Session,
    label: &str,
    f: impl FnOnce(&mut Document, &mut Option<LayerId>, &JobCtl) -> Result<R> + Send + 'static,
    finish: impl FnOnce(R) -> Value + Send + 'static,
) -> Result<Value> {
    if !s.jobs.spawn || cfg!(target_arch = "wasm32") {
        // Inline: exactly `Session::edit`, as before jobs existed.
        let ctx = JobCtl::new();
        let r = s.edit(label, |doc, active| f(doc, active, &ctx))?;
        return Ok(finish(r));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let (base, active) = (st.doc.clone(), st.active_layer);
    let label_owned = label.to_string();
    run(
        s,
        label,
        true,
        move |ctx| {
            let mut doc = (*base).clone();
            let mut active = active;
            let r = f(&mut doc, &mut active, ctx)?;
            Ok((doc, active, r))
        },
        move |s, (doc, new_active, r)| {
            s.edit(&label_owned, move |d, active| {
                *d = doc;
                *active = new_active;
                Ok(())
            })?;
            Ok(finish(r))
        },
    )
}

/// Where a background open reads its file from.
#[derive(Clone, Debug)]
pub enum OpenSource {
    /// File contents already in memory (web pickers, drops, automation).
    Bytes(Arc<Vec<u8>>),
    /// A path the worker reads itself (native), so even the read stays off the UI thread.
    Path(String),
}

/// The command id open jobs report (there is no engine command: opening is the shell's).
pub const OPEN_JOB: &str = "file.open";

/// Flat headers bound pixels without decoding. Layered/native/RAW containers can carry much
/// more than their canvas or compressed file, so use the native expanded-data ceiling instead.
fn open_working_bytes(header: &[u8], input_bytes: u64) -> u64 {
    use photocraft_codecs::{Format, Limits};
    let decoded = match photocraft_codecs::detect(header) {
        Some(Format::Psd | Format::Tiff) | None => photocraft_format::LoadOptions::default().max_total_bytes.saturating_mul(2),
        Some(format) => {
            let ceiling = Limits::desktop_photo().max_alloc.saturating_mul(4);
            photocraft_codecs::dimensions(format, header)
                .ok()
                .map(|(w, h)| u64::from(w).saturating_mul(u64::from(h)).saturating_mul(64).min(ceiling))
                .unwrap_or(ceiling)
        }
    };
    decoded.max(input_bytes.saturating_mul(8)).max(16 << 20)
}

fn admitted_open(source: OpenSource, ctx: &JobCtl) -> Result<(Arc<Vec<u8>>, lightcraft_sysmem::Permit<'static>)> {
    use std::io::{Read, Seek, SeekFrom};
    let failure =
        |error: std::io::Error| if error.kind() == std::io::ErrorKind::Interrupted { EngineError::Cancelled } else { EngineError::Other(error.to_string()) };
    ctx.check()?;
    let (mut file, existing, estimated) = match source {
        OpenSource::Bytes(bytes) => {
            let estimate = open_working_bytes(&bytes, bytes.len() as u64);
            (None, Some(bytes), estimate)
        }
        OpenSource::Path(path) => {
            let mut file = li_fs::read::open_regular(std::path::Path::new(&path), li_fs::read::MAX_FILE_BYTES).map_err(failure)?;
            let len = file.metadata().map_err(failure)?.len();
            let mut header = Vec::new();
            (&mut file).take(64 << 10).read_to_end(&mut header).map_err(failure)?;
            file.seek(SeekFrom::Start(0)).map_err(failure)?;
            (Some(file), None, open_working_bytes(&header, len))
        }
    };
    ctx.report(0.0, "Waiting for memory");
    let jobs = lightcraft_sysmem::heavy_jobs();
    let mut permit = jobs.admit(estimated, &|| ctx.is_cancelled()).ok_or(EngineError::Cancelled)?;
    ctx.check()?;
    ctx.report(0.0, "Reading");
    let bytes = match existing {
        Some(bytes) => bytes,
        None => {
            let input = file.as_mut().ok_or_else(|| EngineError::Other("Open file handle unavailable".into()))?;
            let len = input.metadata().map_err(failure)?.len();
            Arc::new(li_fs::read::all(input, len, li_fs::read::MAX_FILE_BYTES, 1 << 20, &ctx.interrupt()).map_err(failure)?)
        }
    };
    ctx.check()?;
    let actual = open_working_bytes(&bytes, bytes.len() as u64);
    if actual > estimated {
        // A source can change while admission waits. Retained input is bounded by the shared
        // file contract; never hold one permit while waiting to enlarge it (would deadlock).
        drop(permit);
        ctx.report(0.0, "Waiting for memory");
        permit = jobs.admit(actual, &|| ctx.is_cancelled()).ok_or(EngineError::Cancelled)?;
    }
    ctx.check()?;
    Ok((bytes, permit))
}

impl Session {
    /// Run a command, in the background when it supports it (see [`jobs`](crate::jobs)). Errors
    /// like [`Session::execute`] for unknown, disabled or failing commands.
    pub fn start(&mut self, id: &str, params: Value) -> Result<Started> {
        self.dispatch(id, params, true)
    }

    /// Open a file in the background: read (for a path) and decode on a worker, with progress
    /// (per layer for PSD/PSB) and cancellation, then add the document like
    /// [`Session::open_document`] (Color Settings policies apply). `name` names the document.
    /// The job's result is `{document, name, warnings, color}`; on wasm it runs inline.
    pub fn start_open(&mut self, name: &str, source: OpenSource) -> Result<Started> {
        self.start_open_policy(name, source, false)
    }

    /// Interactive file admission keeps profile decisions pending until confirmation.
    pub fn start_open_deferred(&mut self, name: &str, source: OpenSource) -> Result<Started> {
        self.start_open_policy(name, source, true)
    }

    fn start_open_policy(&mut self, name: &str, source: OpenSource, deferred: bool) -> Result<Started> {
        let label = format!("Opening {name}");
        let name_w = name.to_string();
        let name_a = name.to_string();
        self.jobs.open_queue = true;
        let result = self.start_job(
            OPEN_JOB,
            json!({"name": name}),
            &label,
            false,
            move |ctx| {
                let (bytes, permit) = admitted_open(source, ctx)?;
                ctx.report(0.02, "Decoding");
                if let Some(result) = crate::vector_integration::read_vector_file(&name_w, &bytes, ctx.cancel_flag()) {
                    return result.map(|r| (r, true, permit));
                }
                ctx.stage(0.02, 1.0, "Decoding", |ctl| photocraft_io::import_with(&name_w, &bytes, ctl))
                    .map_err(|e| match e {
                        photocraft_io::IoError::Cancelled => EngineError::Cancelled,
                        e => EngineError::Other(e.to_string()),
                    })
                    .map(|r| (r, false, permit))
            },
            move |s, (r, native_import, _permit): (photocraft_io::ImportResult, bool, lightcraft_sysmem::Permit<'static>)| {
                let (index, color) = if deferred { s.open_document_deferred(r.document, None) } else { s.open_document(r.document, None) };
                // `document` is the tab index at completion, as other commands report it. Completion
                // consumers must use the admitted ID: other jobs/callbacks may reorder or close tabs.
                let document = s.documents()[index].doc.id;
                Ok(json!({"document": index, "documentId": document.0, "name": name_a, "warnings": r.warnings, "color": color, "nativeImport": native_import}))
            },
        );
        self.jobs.open_queue = false;
        result
    }

    /// Start a job that isn't an engine command (opening a file, a shell operation, a test's fake
    /// job): `work` runs on a worker, `apply` on this thread when [`Session::poll_jobs`] or
    /// [`Session::wait_job`] picks the result up. `command` and `params` identify it in
    /// [`JobInfo`]; it is not journaled. With `lock_document` the active document is locked as
    /// for a command. Runs inline on wasm.
    pub fn start_job<T: Send + 'static>(
        &mut self,
        command: &str,
        params: Value,
        label: &str,
        lock_document: bool,
        work: impl FnOnce(&JobCtl) -> Result<T> + Send + 'static,
        apply: impl FnOnce(&mut Session, T) -> Result<Value> + Send + 'static,
    ) -> Result<Started> {
        self.jobs.spawn = true;
        let r = run(self, label, lock_document, work, apply);
        self.jobs.spawn = false;
        let pending = self.jobs.pending.take();
        match (r, pending) {
            (Err(e), Some(job)) => {
                job.ctx.cancel();
                Err(e)
            }
            (Err(e), None) => Err(e),
            (Ok(_), Some(mut job)) => {
                job.command = command.to_string();
                job.params = params;
                job.journal = false;
                let id = job.id;
                self.jobs.running.push(job);
                Ok(Started::Job(id))
            }
            (Ok(v), None) => Ok(Started::Done(v)),
        }
    }

    /// Apply finished background jobs (on this thread) and return every job that ended since
    /// the last call. Cheap when nothing runs; the UI calls it every frame.
    pub fn poll_jobs(&mut self) -> Vec<JobEvent> {
        let mut i = 0;
        while i < self.jobs.running.len() {
            match self.jobs.running.get(i).map(|r| r.rx.try_recv()) {
                Some(Ok(r)) => {
                    let job = self.jobs.running.remove(i);
                    self.finish_job(job, r);
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) => i += 1,
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    let job = self.jobs.running.remove(i);
                    let label = job.label.clone();
                    self.finish_job(job, Err(EngineError::Other(format!("`{label}` stopped unexpectedly; the document is unchanged"))));
                }
                None => break,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.jobs.draining.retain(|h| !h.is_finished());
        std::mem::take(&mut self.jobs.events)
    }

    /// Block until job `id` ends, apply it, and return its result (the control channel's and
    /// MCP's default `wait`, and the CLI). `Err(Cancelled)` if it was cancelled.
    pub fn wait_job(&mut self, id: JobId) -> Result<Value> {
        while self.jobs.running.iter().any(|job| job.id == id) {
            // Completed opens retain their admission until applied. Waiting for a later open
            // must also deliver earlier results, or they could hold every permit it needs.
            let events = self.poll_jobs();
            let outcome = events.iter().find(|event| event.id == id).map(|event| event.outcome.clone());
            self.jobs.events.extend(events);
            if let Some(outcome) = outcome {
                return match outcome {
                    JobOutcome::Done(value) => Ok(value),
                    JobOutcome::Cancelled => Err(EngineError::Cancelled),
                    JobOutcome::Failed(error) => Err(EngineError::Other(error)),
                };
            }
            if self.jobs.running.iter().any(|job| job.id == id) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        match self.jobs.recent.iter().rev().find(|j| j.id == id) {
            Some(j) if j.state == "done" => Ok(j.result.clone().unwrap_or(Value::Null)),
            Some(j) if j.state == "cancelled" => Err(EngineError::Cancelled),
            Some(j) => Err(EngineError::Other(j.error.clone().unwrap_or_else(|| "the job failed".into()))),
            None => Err(EngineError::Other(format!("no job {}", id.0))),
        }
    }

    /// Cancel job `id`: it stops at its next check, its result is discarded and the document
    /// stays unchanged. The document is unlocked at once. False if no such job is running.
    pub fn cancel_job(&mut self, id: JobId) -> bool {
        let Some(i) = self.jobs.running.iter().position(|r| r.id == id) else { return false };
        let job = self.jobs.running.remove(i);
        job.ctx.cancel();
        #[cfg(not(target_arch = "wasm32"))]
        open_pool::discard_cancelled();
        self.end_job(&job, JobOutcome::Cancelled);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(h) = job.handle {
            self.jobs.draining.push(h);
        }
        true
    }

    /// Cancel every running job (e.g. before quitting).
    pub fn cancel_all_jobs(&mut self) {
        let ids: Vec<JobId> = self.jobs.running.iter().map(|r| r.id).collect();
        for id in ids {
            self.cancel_job(id);
        }
    }

    /// Is any job running? (Cheap: the UI asks every frame.)
    pub fn has_jobs(&self) -> bool {
        !self.jobs.running.is_empty()
    }

    /// Running job `id`.
    pub fn job(&self, id: JobId) -> Option<JobInfo> {
        self.jobs.running.iter().find(|r| r.id == id).map(Running::info)
    }

    /// Running jobs.
    pub fn jobs(&self) -> Vec<JobInfo> {
        self.jobs.running.iter().map(Running::info).collect()
    }

    /// Running jobs followed by the last few that ended (newest last), for `jobs.list`.
    pub fn jobs_with_recent(&self) -> Vec<JobInfo> {
        self.jobs.recent.iter().cloned().chain(self.jobs()).collect()
    }

    /// The running job that locks document `doc`, if any.
    pub fn job_on(&self, doc: DocId) -> Option<JobInfo> {
        self.jobs.running.iter().find(|r| r.doc == Some(doc)).map(Running::info)
    }

    /// The running job that locks the active document, if any.
    pub fn active_job(&self) -> Option<JobInfo> {
        self.active().and_then(|st| self.job_on(st.doc.id))
    }

    /// Wait for background workers of cancelled jobs to exit (tests and benchmarks measure
    /// cancel latency with it). Native only; returns at once on wasm.
    pub fn join_cancelled_jobs(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        for h in self.jobs.draining.drain(..) {
            h.join();
        }
    }

    /// Why command `id` can't run now because of a background job (None = no conflict).
    pub(crate) fn job_conflict(&self, id: &str, journal: bool) -> Option<String> {
        if !journal || independent_of_document(id) {
            return None;
        }
        let job = self.active_job()?;
        Some(format!("“{}” is still running on this document; wait for it to finish or cancel it (Esc)", job.label))
    }

    /// Cancel the jobs editing document `doc` (it is closing).
    pub(crate) fn cancel_jobs_on(&mut self, doc: DocId) {
        let ids: Vec<JobId> = self.jobs.running.iter().filter(|r| r.doc == Some(doc)).map(|r| r.id).collect();
        for id in ids {
            self.cancel_job(id);
        }
    }

    /// Run the command `id`: inline, or (with `background`) as a job when it is one.
    pub(crate) fn dispatch(&mut self, id: &str, params: Value, background: bool) -> Result<Started> {
        let spec = crate::commands::find(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        // Params are named fields: anything but an object (or null, meaning none) is a caller
        // mistake, not "use every default".
        if !(params.is_object() || params.is_null()) {
            return Err(EngineError::BadParams { cmd: id.to_string(), msg: "params must be a JSON object".into() });
        }
        // Untrusted sessions gate every command, including the ones a command runs on its own
        // behalf (`file.automate.conditionalModeChange` runs `image.mode.*`), before any side
        // effect.
        if let Some(gate) = self.authorize {
            gate(id, &params)?;
        }
        let _authorization = AuthorizationScope::enter(self.authorize);
        crate::filters::validate_params(id, &params)?;
        // A floating selection drops before any other command (Undo puts it back instead).
        if let Some(v) = crate::float_cmds::before_command(self, id)? {
            return Ok(Started::Done(v));
        }
        // Pixel commands follow the Channels panel target unless the caller names one.
        let run_params = crate::channel_cmds::inject_target(self, id, crate::commands::inject_kind(id, params.clone()));
        if let Err(why) = crate::smart_cmds::target_enabled(self, spec, &params).unwrap_or_else(|| self.precondition(spec, &run_params)) {
            return Err(EngineError::Disabled(id.to_string(), why));
        }
        if let Some(why) = self.job_conflict(id, spec.journal) {
            return Err(EngineError::Disabled(id.to_string(), why));
        }
        self.coalesce_request = params.get("coalesce").and_then(Value::as_str).map(str::to_string);
        self.color_restrict = crate::channel_cmds::color_restriction(self, id, &run_params);
        self.jobs.spawn = background;
        // Last-resort guard (AGENTS.md, Never crash): a command that panics anyway fails with an
        // error instead of taking the app down. `edit` only commits a document after its closure
        // returns, so the documents are unchanged.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (spec.run)(self, &run_params)))
            .unwrap_or_else(|_| Err(EngineError::Other(format!("`{id}` failed with an internal error (logged); the document is unchanged"))));
        self.jobs.spawn = false;
        self.coalesce_request = None;
        self.color_restrict = None;
        let pending = self.jobs.pending.take();
        match (r, pending) {
            (Err(e), Some(job)) => {
                job.ctx.cancel();
                Err(e)
            }
            (Err(e), None) => Err(e),
            (Ok(_), Some(mut job)) => {
                job.command = id.to_string();
                job.params = params;
                job.journal = spec.journal;
                let jid = job.id;
                self.jobs.running.push(job);
                Ok(Started::Job(jid))
            }
            (Ok(v), None) => {
                self.after_command(id, params, spec.journal);
                Ok(Started::Done(v))
            }
        }
    }

    /// Bookkeeping after a command (or a job's apply) succeeded.
    pub(crate) fn after_command(&mut self, id: &str, params: Value, journal: bool) {
        // A layer-mask view ends when another layer becomes active (#196).
        if let Some(st) = self.active_mut() {
            crate::mask_view_cmds::fix(st);
        }
        crate::edit_menu_cmds::after_command(self, id);
        crate::automate_cmds::after_command(self, id);
        self.sync_preset_store();
        if journal && !crate::brush_cmds::coalesce_journal(self, id, &params) {
            self.journal.push((id.to_string(), params));
        }
    }

    /// Apply (or report the failure of) a job whose worker has delivered `r`.
    fn finish_job(&mut self, mut job: Running, r: Result<Payload>) {
        let apply = job.apply.take();
        let outcome = match r {
            Err(EngineError::Cancelled) => JobOutcome::Cancelled,
            Err(e) => JobOutcome::Failed(e.to_string()),
            Ok(_) if job.ctx.is_cancelled() => JobOutcome::Cancelled,
            Ok(payload) => match self.apply_job(&job, apply, payload) {
                Ok(v) => JobOutcome::Done(v),
                Err(EngineError::Cancelled) => JobOutcome::Cancelled,
                Err(e) => JobOutcome::Failed(e.to_string()),
            },
        };
        self.end_job(&job, outcome);
    }

    fn apply_job(&mut self, job: &Running, apply: Option<Apply>, payload: Payload) -> Result<Value> {
        let Running { label, doc, snapshot, coalesce, color_restrict, command, params, journal, .. } = job;
        let prev_active = self.active;
        let target = match doc {
            Some(d) => {
                let i =
                    self.documents().iter().position(|st| st.doc.id == *d).ok_or_else(|| EngineError::Other(format!("`{label}`: the document was closed")))?;
                let unchanged = snapshot.as_ref().is_some_and(|w| self.documents().get(i).is_some_and(|st| Weak::ptr_eq(w, &Arc::downgrade(&st.doc))));
                if !unchanged {
                    return Err(EngineError::Other(format!("`{label}`: the document changed while it ran; the result was discarded")));
                }
                self.active = Some(i);
                Some(i)
            }
            None => None,
        };
        let group_view =
            if job.preserve_group_view { target.and_then(|i| self.docs.get(i)).map(|st| crate::group_view_cmds::capture_expansion(&st.doc)) } else { None };
        self.coalesce_request = coalesce.clone();
        self.color_restrict = *color_restrict;
        let r = match apply {
            Some(f) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self, payload)))
                .unwrap_or_else(|_| Err(EngineError::Other(format!("`{label}` failed with an internal error (logged); the document is unchanged")))),
            None => Err(EngineError::Other("internal error: job applied twice".into())),
        };
        self.coalesce_request = None;
        self.color_restrict = None;
        // Keep the user's active document unless the job opened a new one.
        if target.is_some() && self.active == target {
            self.active = prev_active.filter(|i| *i < self.docs.len()).or(self.active);
        }
        let v = r?;
        if let (Some(id), Some(expanded)) = (doc, group_view)
            && let Some(st) = self.docs.iter_mut().find(|st| st.doc.id == *id)
        {
            let before = Arc::downgrade(&st.doc);
            crate::group_view_cmds::restore_expansion(&mut st.doc, &expanded);
            let after = Arc::downgrade(&st.doc);
            self.jobs.rebase_group_view(*id, &before, after);
        }
        self.after_command(command, params.clone(), *journal);
        Ok(v)
    }

    fn end_job(&mut self, job: &Running, outcome: JobOutcome) {
        let (state, result, error) = match &outcome {
            JobOutcome::Done(v) => ("done", Some(v.clone()), None),
            JobOutcome::Failed(e) => ("failed", None, Some(e.clone())),
            JobOutcome::Cancelled => ("cancelled", None, None),
        };
        let mut info = job.info();
        info.state = state;
        info.result = result;
        info.error = error;
        if state == "done" {
            info.progress = 1.0;
        }
        self.jobs.recent.push(info);
        if self.jobs.recent.len() > RECENT {
            let extra = self.jobs.recent.len() - RECENT;
            self.jobs.recent.drain(..extra);
        }
        self.jobs.events.push(JobEvent { id: job.id, command: job.command.clone(), label: job.label.clone(), document: job.doc, outcome });
    }
}

/// Engine commands for the job table (the control channel and MCP use them too).
pub fn specs() -> Vec<crate::commands::CommandSpec> {
    use crate::commands::CommandSpec;
    vec![
        CommandSpec {
            id: "jobs.list",
            label: "List Background Jobs",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: |_| Ok(()),
            run: |s, _| Ok(json!({"jobs": s.jobs_with_recent()})),
            journal: false,
        },
        CommandSpec {
            id: "jobs.cancel",
            label: "Cancel Background Job",
            menu: &[],
            shortcut: None,
            params: r##"{"job":u64? (default: every running job)}"##,
            enabled: |_| Ok(()),
            run: |s, p| match p.get("job") {
                None | Some(Value::Null) => {
                    let n = s.jobs.running.len();
                    s.cancel_all_jobs();
                    Ok(json!({"cancelled": n}))
                }
                Some(v) => {
                    let id = v.as_u64().ok_or_else(|| EngineError::BadParams { cmd: "jobs.cancel".into(), msg: "`job` must be a job id".into() })?;
                    if s.cancel_job(JobId(id)) { Ok(json!({"cancelled": 1})) } else { Err(EngineError::Other(format!("no running job {id}"))) }
                }
            },
            journal: false,
        },
    ]
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
