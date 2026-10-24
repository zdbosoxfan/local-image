//! local-image: Local Image's AI in the editor.
//!
//! - **AI Remove Brush** (R): paint over a distraction; after confirmation the painted mask is removed by the
//!   local model and the repair lands on its own layer (`ai.remove`).
//! - **AI Cutout** (K): Remove Background makes a layer mask from the AI matte
//!   (`ai.removeBackground`); dragging paints that mask (Erase hides, Restore reveals; X swaps);
//!   Add Background puts a solid colour, an image or a generated plate below the cutout.
//! - The **AI status pill** in the status bar and the **Local AI** window (connection, ComfyUI
//!   installation, start/stop, verified model downloads, GPU).
//! - Menu commands with ids `li.*` (File › New from Prompt, Open Folder, Automate › Remove
//!   Backgrounds, Edit › Generative Fill, Image › AI Enhance, Layer Mask › Generate Background,
//!   Window › Generate, Edit › Preferences › Local AI).
//!
//! The engine commands are in `photocraft_engine::ai_cmds`; the ComfyUI client in `li-ai`.

mod enhance_settings;
mod lan_discovery;
mod owned_engine;
pub(crate) mod settings_ui;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Sense, Stroke, vec2};
use li_ai::catalog::{self, ModelId};
use li_ai::{AiSettings, JobControl};
use photocraft_engine::jobs::{JobEvent, JobOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::Tool;
use crate::theme::Tokens;
use crate::widgets;

/// Persistent AI options (part of the saved UI state).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiOptions {
    /// Expanded model families in AI Preferences.
    pub settings_families: std::collections::BTreeSet<String>,
    /// AI Remove engine: `auto`, `lama`, `klein`, `qwen-int8` or `qwen-bf16`.
    pub remove_engine: String,
    /// Apply a painted removal stroke on release. Off by default (owner decision): the user brushes,
    /// adjusts the mask, then confirms with Remove.
    pub remove_immediately: bool,
    /// Shared single/batch cutout engine: `fast`, `better`, `qwen-int8` or `qwen-bf16`.
    pub cutout_engine: String,
    pub cutout_refine: bool,
    pub cutout_decontaminate: bool,
    /// What to keep, passed to the cutout model ("the red car").
    pub cutout_hint: String,
    /// Shared output for Properties, the AI Cutout options bar and the contextual task bar.
    pub cutout_output: String,
    pub cutout_color: [f32; 3],
    /// Gaussian blur radius in pixels.
    pub cutout_blur: f32,
    /// Cutout refine strokes reveal (true) or hide (false) the layer.
    pub cutout_restore: bool,
    /// Last background description (Generate Background).
    pub background_prompt: String,
    /// Last Generative Fill prompt.
    pub fill_prompt: String,
    /// The Generate panel.
    pub generate: crate::generate_ui::GenerateState,
    pub draft_refine: crate::generate_ui::DraftRefineSettings,
    pub enhance_method: li_ai::enhance::Method,
    pub enhance_backend: li_ai::enhance::Backend,
}

impl Default for AiOptions {
    fn default() -> Self {
        Self {
            settings_families: Default::default(),
            remove_engine: "auto".into(),
            remove_immediately: false,
            cutout_engine: "fast".into(),
            cutout_refine: true,
            cutout_decontaminate: false,
            cutout_hint: String::new(),
            cutout_output: "mask".into(),
            cutout_color: [1.0; 3],
            cutout_blur: 12.0,
            cutout_restore: false,
            background_prompt: String::new(),
            fill_prompt: String::new(),
            generate: Default::default(),
            draft_refine: Default::default(),
            enhance_method: Default::default(),
            enhance_backend: Default::default(),
        }
    }
}

// Engine status

/// What we know about the AI engine (ComfyUI), refreshed by a background poller.
#[derive(Clone, Debug, Default)]
pub struct EngineStatus {
    /// A check has completed at least once.
    pub checked: bool,
    pub connected: bool,
    pub host: String,
    pub gpu: Option<li_ai::setup::Gpu>,
    /// Per preset id (`model:variant`): ready, or why not.
    pub presets: BTreeMap<String, Result<(), String>>,
    pub error: String,
    /// ComfyUI was started by us and is still loading.
    pub starting: bool,
    /// The sampler limits the connected ComfyUI reports (steps, guidance, denoise).
    pub limits: Option<li_ai::comfy::SamplerLimits>,
    pub object_info: Option<Arc<li_ai::comfy::ObjectInfo>>,
}

impl EngineStatus {
    pub fn ready(&self, model: ModelId, variant: &str) -> Result<(), String> {
        if !self.connected {
            return Err(if self.starting { tl!("The AI engine is starting…").into() } else { tl!("The AI engine (ComfyUI) is not running.").into() });
        }
        self.presets.get(&format!("{}:{variant}", model.key())).cloned().unwrap_or_else(|| Err(tl!("Checking models…").into()))
    }
    pub fn any_ready(&self) -> bool {
        self.connected && self.presets.values().any(Result::is_ok)
    }
}

/// The ComfyUI process Local Image started (never one the user started themselves). Kept outside
/// [`Shared`] so quitting can stop it without starting the status poller.
static CHILD: owned_engine::OwnedEngine = owned_engine::OwnedEngine::new();

struct Shared {
    status: Mutex<EngineStatus>,
    refresh: std::sync::atomic::AtomicBool,
    downloads: Mutex<BTreeMap<String, Download>>,
    ctx: Mutex<Option<egui::Context>>,
}

#[derive(Clone, Default)]
pub struct Download {
    pub done: u64,
    pub total: u64,
    pub file: String,
    pub error: Option<String>,
    pub finished: bool,
    pub ctl: JobControl,
}

fn shared() -> &'static Arc<Shared> {
    static S: OnceLock<Arc<Shared>> = OnceLock::new();
    S.get_or_init(|| {
        let s = Arc::new(Shared {
            status: Mutex::new(EngineStatus::default()),
            refresh: std::sync::atomic::AtomicBool::new(true),
            downloads: Mutex::new(BTreeMap::new()),
            ctx: Mutex::new(None),
        });
        let w = s.clone();
        let _ = std::thread::Builder::new().name("ai-status".into()).spawn(move || poll_loop(w));
        s
    })
}

/// The current engine status (cheap).
pub fn status() -> EngineStatus {
    #[cfg(test)]
    if let Some(status) = TEST_STATUS.with(|s| s.borrow().clone()) {
        return status;
    }
    shared().status.lock().map(|s| s.clone()).unwrap_or_default()
}

#[cfg(test)]
pub(crate) static AI_TEST_LOCK: Mutex<()> = Mutex::new(());

// UI tests use the real mock server for commands, with deterministic readiness on their
// own thread rather than racing the asynchronous status poller.
#[cfg(test)]
thread_local! {
    pub(crate) static TEST_STATUS: std::cell::RefCell<Option<EngineStatus>> = const { std::cell::RefCell::new(None) };
}

/// Start resource telemetry even when Generate/Local AI is closed. The existing status worker
/// performs the network probes; a changed pressure sample wakes all host views.
pub(crate) fn watch_resources(ctx: &egui::Context) {
    let repaint = ctx.clone();
    lightcraft_sysmem::gpu::set_waker(Arc::new(move || repaint.request_repaint()));
    if let Ok(mut c) = shared().ctx.lock() {
        *c = Some(ctx.clone());
    }
}

/// Ask the poller to check again now.
pub fn refresh() {
    dialogs_mut(|d| d.subject_quality = None);
    shared().refresh.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn poll_loop(s: Arc<Shared>) {
    let mut last_inventory: Option<Instant> = None;
    let mut last = Instant::now() - Duration::from_secs(60);
    loop {
        let forced = s.refresh.swap(false, std::sync::atomic::Ordering::SeqCst);
        let connected_before = s.status.lock().map(|x| x.connected).unwrap_or(false);
        let interval = if connected_before { Duration::from_secs(2) } else { Duration::from_secs(3) };
        if !forced && last.elapsed() < interval {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        }
        last = Instant::now();
        let ai = li_ai::service();
        let host = ai.client.host().to_owned();
        let stats = ai.client.system_stats().ok();
        let old_sample = lightcraft_sysmem::gpu::sample();
        let was_pressured = lightcraft_sysmem::gpu::pressured();
        let old_host = s.status.lock().map(|x| x.host.clone()).unwrap_or_default();
        if old_host != host {
            lightcraft_sysmem::gpu::clear_sample();
        }
        if let Some(stats) = &stats
            && let Ok(queue) = ai.client.queue_state()
            && ai.client.host() == li_ai::service().client.host()
        {
            if let Some(sample) = li_ai::resources::parse(&host, stats, &queue) {
                lightcraft_sysmem::gpu::publish(sample);
            } else {
                lightcraft_sysmem::gpu::clear_sample();
            }
        }
        let pressure_changed = old_sample != lightcraft_sysmem::gpu::sample() || was_pressured != lightcraft_sysmem::gpu::pressured();
        let connected = stats.is_some();
        let gpu = stats.as_ref().and_then(|st| li_ai::setup::gpus_from_stats(st).into_iter().next());
        let mut presets = s.status.lock().map(|x| x.presets.clone()).unwrap_or_default();
        let mut limits = s.status.lock().ok().and_then(|x| x.limits);
        let mut object_info = s.status.lock().ok().and_then(|x| x.object_info.clone());
        let mut error = String::new();
        if connected && (forced || !connected_before || last_inventory.is_none_or(|t| t.elapsed() > Duration::from_secs(30))) {
            match ai.client.object_info() {
                Ok(info) => {
                    // Installed models (by header, metadata or name) join the catalogue first.
                    let settings = AiSettings::load();
                    let model_dir = settings.model_dir();
                    li_ai::inventory::refresh(&info, (!settings.is_remote() && model_dir.is_dir()).then_some(model_dir.as_path()));
                    li_ai::refine::checker().invalidate_changed_files();
                    limits = info.sampler_limits();
                    presets = catalog::presets()
                        .iter()
                        .map(|p| {
                            (p.id(), {
                                let a = catalog::availability(p, &info);
                                if a.available { Ok(()) } else { Err(a.reason) }
                            })
                        })
                        .collect();
                    object_info = Some(Arc::new(info));
                    last_inventory = Some(Instant::now());
                }
                Err(e) => error = format!("{e:#}"),
            }
        }
        if !connected {
            presets.clear();
            limits = None;
            object_info = None;
            last_inventory = None;
        }
        let child_alive = match CHILD.is_running() {
            Ok(alive) => alive,
            Err(e) => {
                if error.is_empty() {
                    error = format!("Could not check whether the AI engine is running: {e}");
                }
                true // Preserve the handle/reservation when its state is uncertain.
            }
        };
        let changed;
        if let Ok(mut st) = s.status.lock() {
            let new = EngineStatus { checked: true, connected, host, gpu, presets, error, starting: child_alive && !connected, limits, object_info };
            changed = pressure_changed
                || st.host != new.host
                || st.connected != new.connected
                || st.presets != new.presets
                || st.starting != new.starting
                || st.limits != new.limits
                || st.object_info.as_ref().map(|i| &i.0) != new.object_info.as_ref().map(|i| &i.0)
                || !st.checked;
            *st = new;
        } else {
            changed = false;
        }
        if changed && let Some(ctx) = s.ctx.lock().ok().and_then(|c| c.clone()) {
            ctx.request_repaint();
        }
    }
}

/// Starts the configured (or first detected) ComfyUI installation.
pub fn start_engine() -> Result<String, String> {
    let label = CHILD.start(|| {
        let settings = AiSettings::load();
        let found = li_ai::setup::detect(&settings);
        let inst = found.first().ok_or(tl!("No ComfyUI installation found. Choose its folder in Local AI, or install ComfyUI first."))?;
        let label = inst.label();
        let child = li_ai::setup::start(inst, &settings).map_err(|e| format!("{e:#}"))?;
        Ok((child, label))
    })?;
    refresh();
    Ok(label)
}

/// Stops ComfyUI if Local Image started it.
pub fn stop_engine() -> bool {
    let stopped = kill_child();
    refresh();
    stopped
}

/// Called when the app quits: stops the ComfyUI Local Image started, so it doesn't keep holding
/// tens of GB of RAM and VRAM after the window closes. A ComfyUI the user started is left alone.
pub fn shutdown_engine() -> bool {
    kill_child()
}

fn kill_child() -> bool {
    CHILD.stop()
}

/// Downloads a preset's missing files in the background (progress via [`downloads`]).
pub fn start_download(preset_id: &str) {
    if AiSettings::load().is_remote() {
        dialogs_mut(|d| d.message = tl!("Install generative models on the remote ComfyUI server, then check the connection again.").into());
        return;
    }
    let Some(preset) = catalog::presets().iter().find(|p| p.id() == preset_id) else { return };
    let ctl = JobControl::new();
    if let Ok(mut d) = shared().downloads.lock() {
        if d.get(preset_id).is_some_and(|x| !x.finished) {
            return;
        }
        d.insert(preset_id.to_owned(), Download { total: preset.total_bytes(), ctl: ctl.clone(), ..Default::default() });
    }
    let id = preset_id.to_owned();
    let _ = std::thread::Builder::new().name("model-download".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        let dir = AiSettings::load().model_dir();
        let id2 = id.clone();
        let r = li_ai::download::download_preset(&dir, preset, &ctl, &|p| {
            if let Ok(mut d) = shared().downloads.lock()
                && let Some(x) = d.get_mut(&id2)
            {
                x.done = p.done_bytes;
                x.total = p.total_bytes;
                x.file = p.file;
            }
        });
        if let Ok(mut d) = shared().downloads.lock()
            && let Some(x) = d.get_mut(&id)
        {
            x.finished = true;
            x.error = r.err().map(|e| format!("{e:#}"));
        }
        refresh();
    });
}

/// Downloads an official on-device model or bundle into `<model folder>/segmentation/`; keyed
/// `seg:<id>` in [`downloads`].
pub fn start_seg_download(spec: &'static li_seg::ModelSpec) {
    let key = format!("seg:{}", spec.id);
    let ctl = JobControl::new();
    if let Ok(mut d) = shared().downloads.lock() {
        if d.get(&key).is_some_and(|x| !x.finished) {
            return;
        }
        d.insert(key.clone(), Download { total: spec.download_bytes(), file: spec.file.to_owned(), ctl: ctl.clone(), ..Default::default() });
    }
    let models_dir = photocraft_engine::seg::models_dir();
    let _ = std::thread::Builder::new().name("seg-download".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        let r = download_seg_files(spec, &models_dir, &ctl, &|done| {
            if let Ok(mut d) = shared().downloads.lock()
                && let Some(x) = d.get_mut(&key)
            {
                x.done = done;
            }
        });
        if let Ok(mut d) = shared().downloads.lock()
            && let Some(x) = d.get_mut(&key)
        {
            x.finished = true;
            x.error = r.err().map(|e| format!("{e:#}"));
        }
        refresh();
    });
}

/// Compatibility adapter: bundle verification, progress and installation live in the registry.
pub fn download_seg_files(spec: &li_seg::ModelSpec, dir: &std::path::Path, ctl: &JobControl, progress: &dyn Fn(u64)) -> anyhow::Result<()> {
    li_ai::inventory::download_on_device(spec, dir, ctl, progress)
}

/// Where a function's custom-model test stands (see [`CUSTOM_JOBS`]).
#[derive(Clone, Debug, PartialEq)]
enum CustomJob {
    /// Loading the picked model and running it once.
    Testing,
    /// The model was set; the row closes its options.
    Done,
    /// The model doesn't fit; why.
    Failed(String),
}

/// Custom-model tests in flight or just finished, keyed by models folder and function.
static CUSTOM_JOBS: Mutex<BTreeMap<String, CustomJob>> = Mutex::new(BTreeMap::new());

fn job_key(dir: &std::path::Path, group: li_seg::Group) -> String {
    format!("{}|{}", dir.display(), group.key())
}

fn custom_job(dir: &std::path::Path, group: li_seg::Group) -> Option<CustomJob> {
    CUSTOM_JOBS.lock().ok().and_then(|j| j.get(&job_key(dir, group)).cloned())
}

fn set_custom_job(dir: &std::path::Path, group: li_seg::Group, job: Option<CustomJob>) {
    if let Ok(mut j) = CUSTOM_JOBS.lock() {
        match job {
            Some(job) => j.insert(job_key(dir, group), job),
            None => j.remove(&job_key(dir, group)),
        };
    }
}

/// What the custom-model options of one function currently show.
#[derive(Clone, Debug, Default, PartialEq)]
struct SegOpts {
    denoise_linear: bool,
    open: bool,
    /// Subject, sky and depth: the input normalisation (`None`: the function's usual one).
    norm: Option<li_seg::Norm>,
    /// Sky: the class index of the sky for class-logit models (default 2, ADE20K).
    sky_class: Option<usize>,
    /// Depth: the model outputs metric depth (farther = larger).
    depth_metric: bool,
}

impl SegOpts {
    fn options(&self, group: li_seg::Group) -> li_seg::CustomOptions {
        li_seg::CustomOptions {
            norm: self.norm.unwrap_or(li_seg::Norm::default_for(group)),
            sky_class: self.sky_class.unwrap_or(2),
            denoise_linear: self.denoise_linear,
            depth_metric: self.depth_metric,
        }
    }
}

/// The depth output choices: (metric, label).
const DEPTH_OUTPUTS: [(bool, &str); 2] = [(false, "Inverse depth (nearer = larger)"), (true, "Metric depth (farther = larger)")];

/// An input-style picker; true when `norm` changed.
fn norm_picker(ui: &mut egui::Ui, id: (&str, &str), norm: &mut li_seg::Norm) -> bool {
    let before = *norm;
    egui::ComboBox::from_id_salt(id).selected_text(tl!(norm.label())).show_ui(ui, |ui| {
        for n in li_seg::Norm::ALL {
            ui.selectable_value(norm, n, tl!(n.label()));
        }
    });
    *norm != before
}

/// A depth-output picker; true when `metric` changed.
fn depth_output_picker(ui: &mut egui::Ui, id: (&str, &str), metric: &mut bool) -> bool {
    let before = *metric;
    let shown = DEPTH_OUTPUTS.iter().find(|(m, _)| *m == *metric).map_or("", |(_, l)| *l);
    egui::ComboBox::from_id_salt(id).selected_text(tl!(shown)).show_ui(ui, |ui| {
        for (m, label) in DEPTH_OUTPUTS {
            ui.selectable_value(metric, m, tl!(label));
        }
    });
    *metric != before
}

/// How the function's custom model `c` runs (input style, depth output), changeable in place.
fn custom_model_options(ui: &mut egui::Ui, t: &Tokens, dir: &std::path::Path, group: li_seg::Group, c: &li_seg::Custom) {
    if !matches!(group, li_seg::Group::Subject | li_seg::Group::Sky | li_seg::Group::Depth) {
        return;
    }
    let mut opts =
        li_seg::CustomOptions { norm: c.norm, sky_class: c.class, depth_metric: group == li_seg::Group::Depth && c.class == 1, ..Default::default() };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Model input:")).color(t.text_dim).size(11.5));
        changed |= norm_picker(ui, ("seg-custom-norm", group.key()), &mut opts.norm);
    });
    if group == li_seg::Group::Depth {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Model output:")).color(t.text_dim).size(11.5));
            changed |= depth_output_picker(ui, ("seg-custom-depth", group.key()), &mut opts.depth_metric);
        });
    }
    if changed && let Err(e) = li_seg::set_custom_options(dir, group, opts) {
        set_custom_job(dir, group, Some(CustomJob::Failed(format!("{e:#}"))));
    }
}

#[cfg(test)]
thread_local! {
    /// The file the next "Choose an ONNX file…" click picks, in place of a real dialog.
    pub(crate) static TEST_PICK: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// The file dialog for an `.onnx` model.
pub(crate) fn onnx_request() -> crate::FileDialogRequest {
    crate::FileDialogRequest::Described(crate::FileDialogSpec::new(crate::Pick::File).title(tl!("Choose an ONNX model")).filter(tl!("ONNX model"), &["onnx"]))
}

/// Asks for an `.onnx` model; `then` gets it once chosen (tests inject the pick instead of opening
/// a dialog).
#[cfg(not(test))]
fn pick_onnx(ctx: &egui::Context, then: impl FnOnce(std::path::PathBuf) + 'static) {
    crate::file_dialog::ask_later(ctx, onnx_request(), move |_, answer| {
        if let Some(path) = answer.into_path() {
            then(path.into());
        }
        Ok(Value::Null)
    });
}

#[cfg(test)]
fn pick_onnx(_: &egui::Context, then: impl FnOnce(std::path::PathBuf) + 'static) {
    if let Some(path) = TEST_PICK.with(|p| p.borrow_mut().take()) {
        then(path);
    }
}

/// Translates a custom-model validation error into a one-line reason.
fn custom_model_reason(e: &li_seg::CustomModelError) -> String {
    match e {
        li_seg::CustomModelError::UnsupportedFunction => tl!("Custom models are not supported for this function").to_owned(),
        li_seg::CustomModelError::NotAFile { path } => li_i18n::fmt(tl!("{path} isn't a file"), &[("path", &path.display().to_string())]),
        li_seg::CustomModelError::NotOnnx => tl!("Choose an ONNX model (a file ending in .onnx)").to_owned(),
        li_seg::CustomModelError::NoInput => tl!("The model has no input").to_owned(),
        li_seg::CustomModelError::NotAnImageInput => tl!("The model's input must be an image (batch × 3 × height × width)").to_owned(),
        li_seg::CustomModelError::WrongChannels { channels } => {
            li_i18n::fmt(tl!("The model's input has {channels} channels; an RGB image (3) is needed"), &[("channels", &channels.to_string())])
        }
        li_seg::CustomModelError::NonSquareInput { height, width } => li_i18n::fmt(
            tl!("The model's input is {height}×{width}; a square input (or a flexible size) is needed"),
            &[("height", &height.to_string()), ("width", &width.to_string())],
        ),
        li_seg::CustomModelError::NotSkyModelOutput { shape } => {
            li_i18n::fmt(tl!("Not a sky model: its output {shape} should be 1 map or one map per class"), &[("shape", &format!("{shape:?}"))])
        }
        li_seg::CustomModelError::SkyClassOutOfRange { class, classes } => li_i18n::fmt(
            tl!("The sky class {class} is out of range: this model has {classes} classes (0 to {max})"),
            &[("class", &class.to_string()), ("classes", &classes.to_string()), ("max", &(classes - 1).to_string())],
        ),
        li_seg::CustomModelError::NotSubjectModel => tl!("Not a subject model: it must output one foreground map").to_owned(),
        li_seg::CustomModelError::NotDepthModel => tl!("Not a depth model: it must output one depth map").to_owned(),
        li_seg::CustomModelError::UnsupportedRestoration { reason } => format!("{} ({reason})", tl!(li_seg::RESTORATION_UNSUPPORTED)),
        li_seg::CustomModelError::Other(msg) => msg.clone(),
    }
}

/// Tests the picked model and, when it fits, sets it as the function's custom model; in the
/// background (loading a model takes a moment).
fn start_custom(dir: std::path::PathBuf, group: li_seg::Group, src: std::path::PathBuf, opts: li_seg::CustomOptions) {
    if custom_job(&dir, group) == Some(CustomJob::Testing) {
        return;
    }
    set_custom_job(&dir, group, Some(CustomJob::Testing));
    // the drawing language is per thread: translate the reason in the UI's language
    let lang = li_i18n::current();
    let job_dir = dir.clone();
    let started = std::thread::Builder::new().name("seg-custom".into()).spawn(move || {
        // a model file is untrusted: a panic while testing it fails the test, it doesn't hang it
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| li_seg::add_custom(&dir, group, &src, opts, &dispose)));
        let job = match result {
            Ok(Ok(_)) => CustomJob::Done,
            Ok(Err(e)) => {
                let reason =
                    li_i18n::with_language(lang, || e.downcast_ref::<li_seg::CustomModelError>().map(custom_model_reason)).unwrap_or_else(|| e.to_string());
                CustomJob::Failed(reason)
            }
            Err(_) => CustomJob::Failed(li_i18n::with_language(lang, || tl!("The model crashed while it was being tested.").to_owned())),
        };
        set_custom_job(&dir, group, Some(job));
        refresh();
        if let Some(ctx) = shared().ctx.lock().ok().and_then(|c| c.clone()) {
            ctx.request_repaint();
        }
    });
    if let Err(e) = started {
        set_custom_job(&job_dir, group, Some(CustomJob::Failed(e.to_string())));
    }
}

/// Settings › Local AI › On-device models: one row per function, including Smart Sort tagging.
/// Mask/depth rows offer a custom ONNX flow; tagging uses its official bundle.
#[cfg(test)]
fn seg_rows(ui: &mut egui::Ui, t: &Tokens, dls: &BTreeMap<String, Download>, dir: &std::path::Path) {
    for group in li_seg::Group::ALL {
        function_row(ui, t, dls, dir, group);
    }
    for method in li_ai::enhance::Method::ALL {
        enhance_settings::row(ui, t, dls, dir, method);
    }
    if let Some(spec) = li_seg::spec("inspyrenet") {
        model_function_row(ui, t, dls, dir, li_seg::Group::Subject, spec, false);
    }
}

/// One function: its name and what it does, the model in use, and the custom-model controls.
#[cfg(test)]
fn function_row(ui: &mut egui::Ui, t: &Tokens, dls: &BTreeMap<String, Download>, dir: &std::path::Path, group: li_seg::Group) {
    model_function_row(ui, t, dls, dir, group, group.official(), true);
}

fn model_function_row(
    ui: &mut egui::Ui,
    t: &Tokens,
    dls: &BTreeMap<String, Download>,
    dir: &std::path::Path,
    group: li_seg::Group,
    spec: &'static li_seg::ModelSpec,
    custom_controls_enabled: bool,
) {
    let key = format!("seg:{}", spec.id);
    let dl = dls.get(&key);
    let installed = li_seg::installed_bytes(dir, spec);
    let custom = if custom_controls_enabled { li_seg::custom(dir, group) } else { None };
    let official_in_use = matches!(group.in_use(dir), Some(li_seg::InUse::Official(active)) if active.id == spec.id);
    let frame = egui::Frame::NONE.fill(t.field).corner_radius(t.radius).inner_margin(egui::Margin::symmetric(10, 8));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let title = if custom_controls_enabled { group.label() } else { crate::background_ui::MODELS[1].1 };
        ui.label(RichText::new(tl!(title)).size(13.0).strong().color(t.text));
        if !matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) {
            widgets::cpu_estimate(ui, li_seg::estimate::estimate(spec.id, (spec.size * spec.size) as u64));
        }
        if custom_controls_enabled {
            ui.label(RichText::new(tl!(group.about())).color(t.text_dim).size(11.5));
        }
        ui.add_space(4.0);
        if let Some(c) = &custom {
            ui.horizontal(|ui| {
                ui.label(RichText::new(li_i18n::fmt(tl!("Custom: {name}"), &[("name", &c.name)])).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = widgets::secondary_button(
                        ui,
                        if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) { tl!("Use default") } else { tl!("Use official model") },
                        0.0,
                    );
                    if r.clicked() {
                        match li_seg::clear_custom(dir, group, &dispose) {
                            Ok(_) => set_custom_job(dir, group, None),
                            Err(e) => set_custom_job(dir, group, Some(CustomJob::Failed(format!("{e:#}")))),
                        }
                    }
                    ui.label(RichText::new(tl!("In use")).color(t.accent));
                });
            });
            ui.label(
                RichText::new(if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) {
                    tl!("Your selected restoration model. Requires a GPU.")
                } else {
                    tl!("Your own model, run on the CPU instead of the official one.")
                })
                .color(t.text_faint)
                .size(11.0),
            );
            custom_model_options(ui, t, dir, group, c);
            if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) {
                let size = c.path(dir).metadata().map(|m| li_ai::download::human_bytes(m.len())).unwrap_or_default();
                ui.label(
                    RichText::new(li_i18n::fmt(tl!("Selected model · {size} · licence from its provider"), &[("size", &size)])).color(t.text_faint).size(11.0),
                );
            }
            ui.add_space(4.0);
        }
        // the official model
        ui.horizontal(|ui| {
            ui.label(RichText::new(spec.label).color(if custom.is_some() { t.text_dim } else { t.text }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match dl {
                Some(d) if !d.finished => {
                    if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                        d.ctl.cancel();
                    }
                    let frac = if d.total > 0 { d.done as f32 / d.total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(frac).desired_width(140.0).text(format!("{:.0}%", frac * 100.0)));
                }
                _ if removing(&key) => {
                    ui.spinner();
                }
                _ if installed.is_some() => {
                    if remove_button(ui) {
                        ask_remove(spec.label.to_owned(), RemoveTarget::Seg(spec));
                    }
                    let text = if official_in_use { tl!("In use") } else { tl!("Installed") };
                    ui.label(RichText::new(text).color(t.accent));
                }
                _ => {
                    if widgets::secondary_button(ui, tl!("Download"), 0.0).clicked() {
                        start_seg_download(spec);
                    }
                }
            });
        });
        ui.label(RichText::new(tl!(spec.about)).color(t.text_dim).size(11.5));
        ui.label(
            RichText::new(li_i18n::fmt(
                if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) {
                    tl!("{name} · {size} download · GPU required · {licence}")
                } else {
                    tl!("{name} · {size} download · runs on the CPU · {licence}")
                },
                &[
                    ("name", spec.label),
                    ("size", &li_ai::download::human_bytes(spec.download_bytes())),
                    ("licence", if group == li_seg::Group::Faces { "Apache-2.0 (SFace); MIT (YuNet)" } else { spec.licence }),
                ],
            ))
            .color(t.text_faint)
            .size(11.0),
        );
        if let Some(err) = dl.and_then(|d| d.error.as_ref()) {
            ui.label(RichText::new(err).color(t.danger).size(11.5));
        }
        if custom_controls_enabled && group.supports_custom() {
            ui.add_space(4.0);
            custom_controls(ui, t, dir, group);
        }
    });
    ui.add_space(4.0);
}

/// The small "Custom model…" button and, when open, its options and file picker.
fn custom_controls(ui: &mut egui::Ui, t: &Tokens, dir: &std::path::Path, group: li_seg::Group) {
    if !group.supports_custom() {
        return;
    }
    let mut o = dialogs_mut(|d| d.seg_opts.entry(group).or_default().clone());
    let before = o.clone();
    let mut job = custom_job(dir, group);
    if job == Some(CustomJob::Done) {
        // the model was set: tidy up
        set_custom_job(dir, group, None);
        o.open = false;
        job = None;
    }
    if widgets::secondary_button(
        ui,
        if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) { tl!("Change…") } else { tl!("Custom model…") },
        0.0,
    )
    .clicked()
    {
        o.open = !o.open;
        if !o.open && matches!(job, Some(CustomJob::Failed(_))) {
            set_custom_job(dir, group, None);
            job = None;
        }
    }
    if o.open {
        if matches!(group, li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise) {
            let path = li_seg::companion_path(dir, li_seg::denoise::Variant::Linear.file());
            if path.is_file() && widgets::secondary_button(ui, tl!("Use RawNIND linear variant"), 0.0).clicked() {
                start_custom(dir.to_path_buf(), group, path, li_seg::CustomOptions { denoise_linear: true, ..Default::default() });
            }
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new(tl!("Pick any .onnx model that does this. It is copied into the models folder and used instead of the official one."))
                .color(t.text_faint)
                .size(11.0),
        );
        if matches!(group, li_seg::Group::Subject | li_seg::Group::Sky | li_seg::Group::Depth) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("Input style:")).color(t.text_dim).size(11.5));
                let mut norm = o.norm.unwrap_or(li_seg::Norm::default_for(group));
                if norm_picker(ui, ("seg-norm", group.key()), &mut norm) {
                    o.norm = Some(norm);
                }
            });
        }
        match group {
            li_seg::Group::Subject => {}
            li_seg::Group::Depth => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Depth output:")).color(t.text_dim).size(11.5));
                    depth_output_picker(ui, ("seg-depth-output", group.key()), &mut o.depth_metric);
                });
            }
            li_seg::Group::Sky => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Sky class:")).color(t.text_dim).size(11.5));
                    let mut class = o.sky_class.unwrap_or(2);
                    if ui.add(egui::DragValue::new(&mut class).range(0..=9999)).changed() {
                        o.sky_class = Some(class);
                    }
                    ui.label(RichText::new(tl!("Only used when the model gives one map per class (ADE20K: 2).")).color(t.text_faint).size(11.0));
                });
            }
            li_seg::Group::RgbDenoise => {
                ui.checkbox(&mut o.denoise_linear, tl!("Linear Rec.2020 input (RawNIND)"));
            }
            li_seg::Group::RawDenoise => {
                ui.label(tl!("Custom Raw Denoise models use three-channel linear Rec.2020 input and output."));
            }
            li_seg::Group::Tagging | li_seg::Group::Faces | li_seg::Group::Remove => {}
        }
        if job == Some(CustomJob::Testing) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(tl!("Testing the model…")).color(t.text_dim).size(11.5));
            });
        } else if widgets::secondary_button(ui, tl!("Choose an ONNX file…"), 0.0).clicked() {
            let (models, options) = (dir.to_path_buf(), o.options(group));
            pick_onnx(ui.ctx(), move |src| {
                set_custom_job(&models, group, None);
                start_custom(models, group, src, options);
            });
            job = custom_job(dir, group);
        }
        if let Some(CustomJob::Failed(why)) = job {
            ui.label(RichText::new(li_i18n::fmt(tl!("That model can't be used: {error}"), &[("error", &why)])).color(t.danger).size(11.5));
        }
    } else if let Some(CustomJob::Failed(why)) = job {
        ui.label(RichText::new(why).color(t.danger).size(11.5));
    }
    if o != before {
        dialogs_mut(|d| {
            d.seg_opts.insert(group, o);
        });
    }
}

/// The trash button beside "Installed" / "Ready".
fn remove_button(ui: &mut egui::Ui) -> bool {
    let tip = if li_ai::trash::available() { tl!("Remove (moves the files to the Trash)") } else { tl!("Remove (deletes the files)") };
    let r = crate::icons::button(ui, "trash", 22.0, false, tip);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Remove")));
    r.clicked()
}

// Removing models

/// What a confirmed removal takes out.
#[derive(Clone, Debug)]
pub enum RemoveTarget {
    /// An on-device CPU model.
    Seg(&'static li_seg::ModelSpec),
    /// Curated presets' verified files (by preset id; files other installed presets use stay).
    Presets(Vec<String>),
    /// One file ComfyUI lists (an installed model or LoRA), from the model folder.
    Listed { folders: Vec<&'static str>, name: String },
}

impl RemoveTarget {
    fn key(&self) -> String {
        match self {
            RemoveTarget::Seg(s) => format!("seg:{}", s.id),
            RemoveTarget::Presets(ids) => ids.join("+"),
            RemoveTarget::Listed { name, .. } => format!("file:{name}"),
        }
    }
}

/// The catalogue presets `ids` names, and every other one (those may hold shared files back).
fn split_presets(ids: &[String]) -> (Vec<&'static catalog::Preset>, Vec<catalog::Preset>) {
    let all = catalog::presets();
    (all.iter().filter(|p| ids.contains(&p.id())).collect(), all.iter().filter(|p| !ids.contains(&p.id())).cloned().collect())
}

/// A removal waiting for the user's confirmation.
#[derive(Clone, Debug)]
pub struct RemoveAsk {
    pub what: String,
    /// What it frees.
    pub bytes: u64,
    /// Files kept because other installed models use them.
    pub kept: Vec<String>,
    pub target: RemoveTarget,
}

static REMOVING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The last removal's outcome for the status bar: (message, is an error).
static REMOVE_NOTE: Mutex<Option<(String, bool)>> = Mutex::new(None);

fn removing(key: &str) -> bool {
    REMOVING.lock().is_ok_and(|v| v.iter().any(|k| k == key))
}

/// Puts a file out of the way: into the system Trash where there is one, else deletes it (the
/// confirmation says which).
pub fn dispose(path: &std::path::Path) -> std::io::Result<()> {
    // tests delete: they must not fill the real Trash
    if !cfg!(test) && li_ai::trash::available() { li_ai::trash::move_to_trash(path).map(|_| ()) } else { std::fs::remove_file(path) }
}

/// What removing `target` would free and keep (cheap: sizes only, no hashing).
pub fn plan_removal(mut what: String, target: RemoveTarget) -> RemoveAsk {
    if let RemoveTarget::Seg(spec) = &target {
        let shared = li_seg::shared_models(spec);
        if shared.len() > 1 {
            what = shared.iter().map(|s| s.label).collect::<Vec<_>>().join(" / ");
        }
    }
    let model_dir = AiSettings::load().model_dir();
    let (bytes, kept) = match &target {
        RemoveTarget::Seg(spec) => (li_seg::installed_bytes(&photocraft_engine::seg::models_dir(), spec).unwrap_or(0), Vec::new()),
        RemoveTarget::Presets(ids) => {
            let (these, others) = split_presets(ids);
            let mut files = BTreeMap::new();
            let mut kept = Vec::new();
            for p in these {
                let plan = li_ai::download::removal_plan(&model_dir, p, &others);
                files.extend(plan.remove);
                kept.extend(plan.shared.into_iter().filter(|n| !kept.contains(n)).collect::<Vec<_>>());
            }
            (files.values().sum(), kept)
        }
        RemoveTarget::Listed { folders, name } => {
            (li_ai::download::listed_file(&model_dir, folders, name).and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len()), Vec::new())
        }
    };
    RemoveAsk { what, bytes, kept, target }
}

/// Asks to remove `target` (the confirmation shows what it frees).
pub fn ask_remove(what: String, target: RemoveTarget) {
    let ask = plan_removal(what, target);
    dialogs_mut(|d| d.remove = Some(ask));
}

/// Removes in the background; the outcome goes to the status bar and the engine re-checks.
fn remove_now(ask: RemoveAsk) {
    let key = ask.target.key();
    if let Ok(mut v) = REMOVING.lock() {
        if v.contains(&key) {
            return;
        }
        v.push(key.clone());
    }
    let lang = li_i18n::current();
    let _ = std::thread::Builder::new().name("model-remove".into()).spawn(move || {
        li_i18n::set_current(lang);
        let model_dir = AiSettings::load().model_dir();
        let r: anyhow::Result<(u64, Vec<String>)> = match &ask.target {
            RemoveTarget::Seg(spec) => li_seg::remove_bundle_with(&photocraft_engine::seg::models_dir(), spec, &dispose).map(|b| (b, Vec::new())),
            RemoveTarget::Presets(ids) => {
                let (these, others) = split_presets(ids);
                these.into_iter().try_fold((0u64, Vec::<String>::new()), |(freed, mut kept), p| {
                    let r = li_ai::download::remove_preset(&model_dir, p, &others, &dispose)?;
                    for n in r.shared.into_iter().chain(r.mismatched) {
                        if !kept.contains(&n) {
                            kept.push(n);
                        }
                    }
                    Ok((freed + r.freed, kept))
                })
            }
            RemoveTarget::Listed { folders, name } => li_ai::download::remove_listed_file(&model_dir, folders, name, &dispose).map(|b| (b, Vec::new())),
        };
        let note = match r {
            Ok((freed, kept)) => {
                let mut m = li_i18n::fmt(tl!("Removed {name} ({size} freed)"), &[("name", &ask.what), ("size", &li_ai::download::human_bytes(freed))]);
                if !kept.is_empty() {
                    m = format!("{m} · {}", li_i18n::fmt(tl!("kept {files}"), &[("files", &kept.join(", "))]));
                }
                (m, false)
            }
            Err(e) => (li_i18n::fmt(tl!("Couldn't remove {name}: {error}"), &[("name", &ask.what), ("error", &format!("{e:#}"))]), true),
        };
        if let Ok(mut n) = REMOVE_NOTE.lock() {
            *n = Some(note);
        }
        if let Ok(mut d) = shared().downloads.lock() {
            d.remove(&key);
        }
        if let Ok(mut v) = REMOVING.lock() {
            v.retain(|k| *k != key);
        }
        refresh();
        if let Some(ctx) = shared().ctx.lock().ok().and_then(|c| c.clone()) {
            ctx.request_repaint();
        }
    });
}

/// The removal confirmation (states what it frees, and whether it goes to the Trash).
fn remove_confirm(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if let Some((msg, err)) = REMOVE_NOTE.lock().ok().and_then(|mut n| n.take()) {
        app.ui.status = msg;
        app.ui.status_error = err;
    }
    let Some(ask) = dialogs_mut(|d| d.remove.clone()) else { return };
    let t = Tokens::get(ctx);
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut go = false;
    use widgets::{ButtonRole, DialogButton};
    egui::Modal::new(egui::Id::new("li-remove-model")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(RichText::new(li_i18n::fmt(tl!("Remove {name}?"), &[("name", &ask.what)])).size(15.0).strong().color(t.text));
        ui.add_space(6.0);
        let size = li_ai::download::human_bytes(ask.bytes);
        let what = if ask.bytes == 0 {
            tl!("None of its files are in the model folder.").to_owned()
        } else if li_ai::trash::available() {
            li_i18n::fmt(tl!("This frees {size}. The files go to the Trash, so you can restore them from there."), &[("size", &size)])
        } else {
            li_i18n::fmt(tl!("This frees {size}. The files are deleted permanently."), &[("size", &size)])
        };
        ui.label(RichText::new(what).color(t.text_dim));
        if !ask.kept.is_empty() {
            ui.label(
                RichText::new(li_i18n::fmt(tl!("Kept because other installed models use them: {files}"), &[("files", &ask.kept.join(", "))]))
                    .size(11.5)
                    .color(t.text_faint),
            );
        }
        ui.add_space(10.0);
        let label = if li_ai::trash::available() { tl!("Move to Trash") } else { tl!("Delete") };
        let r = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::dialog_buttons(
                ui,
                &[DialogButton::new(ButtonRole::Default, label, 80.0).enabled(ask.bytes > 0), DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0)],
            )
        });
        match r.inner {
            Some(ButtonRole::Default) => go = true,
            Some(_) => close = true,
            None => {}
        }
    });
    if go {
        remove_now(ask);
        close = true;
    }
    if close {
        dialogs_mut(|d| d.remove = None);
    }
}

pub fn downloads() -> BTreeMap<String, Download> {
    shared().downloads.lock().map(|d| d.clone()).unwrap_or_default()
}

// Tools

/// Finish a stroke with an AI tool. Returns false for other tools.
pub fn finish_stroke(app: &mut PhotocraftApp, tool: Tool, points: &[[f64; 3]], mods: egui::Modifiers) -> bool {
    match tool {
        Tool::AiRemove => {
            crate::ai_remove_ui::paint(app, points, mods.alt);
            true
        }
        Tool::AiCutout => {
            let has_mask = app.session.active().and_then(|d| d.active_layer.and_then(|id| d.doc.layer(id))).is_some_and(|l| l.mask.is_some());
            if !has_mask {
                app.ui.status = tl!("Click Remove Background first; then drag to erase or restore parts of the cutout.").into();
                app.ui.status_error = true;
                return true;
            }
            let color = if app.ui.ai.cutout_restore { "#ffffff" } else { "#000000" };
            let p = json!({ "points": points, "color": color, "target": "mask", "erase": false, "zoom": app.current_zoom() });
            let r = app.run("paint.stroke", p);
            report(app, r);
            true
        }
        _ => false,
    }
}

fn report(app: &mut PhotocraftApp, r: Result<Value, String>) {
    if let Err(e) = r {
        crate::command_feedback::report(app, e);
    }
}

fn opt(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_dim));
}

pub(crate) const REMOVE_ENGINES: [(&str, &str, ModelId, &str); 3] = [
    ("klein", "FLUX.2 Klein", ModelId::KleinRemove, "bf16"),
    ("qwen-int8", "Qwen Compact", ModelId::Qwen, "int8"),
    ("qwen-bf16", "Qwen Full", ModelId::Qwen, "bf16"),
];
#[cfg(test)]
thread_local! { pub(crate) static TEST_LAMA_READY: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) }; }

/// Resolve Automatic from the background connection status. Explicit choices persist.
pub fn effective_remove_key(key: &str) -> &str {
    if key == "auto" || key.is_empty() { if status().connected { "klein" } else { "lama" } } else { key }
}

/// Shared readiness for Remove, selection actions and the Develop host.
pub fn remove_ready(key: &str) -> Result<(), String> {
    let key = effective_remove_key(key);
    if key == "lama" {
        #[cfg(test)]
        if let Some(ready) = TEST_LAMA_READY.with(|r| *r.borrow()) {
            return if ready { Ok(()) } else { Err("Download AI Remove in Preferences › AI › On-device Tools first.".into()) };
        }
        if li_seg::installed_bytes(&photocraft_engine::seg::models_dir(), catalog::on_device_default(li_seg::Group::Remove)).is_some() {
            Ok(())
        } else {
            Err(tl!("Download AI Remove in Preferences › AI › On-device Tools first.").into())
        }
    } else {
        let (model, variant) = remove_engine(key);
        status().ready(model, variant)
    }
}

pub(crate) fn remove_readiness(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if let Err(why) = remove_ready(&app.ui.ai.remove_engine) {
        if effective_remove_key(&app.ui.ai.remove_engine) == "lama" {
            readiness_warning_to(app, ui, &why, tl!("On-device Tools…"), "ai.tools");
        } else {
            readiness_warning(app, ui, &why, status().connected);
        }
    }
}

/// The model behind an AI Remove engine key (`klein`, `qwen-int8`, `qwen-bf16`).
pub fn remove_engine(key: &str) -> (ModelId, &'static str) {
    engine_for(key)
}

fn engine_for(key: &str) -> (ModelId, &'static str) {
    match key {
        "qwen-bf16" => (ModelId::Qwen, "bf16"),
        "qwen-int8" => (ModelId::Qwen, "int8"),
        _ => (ModelId::KleinRemove, "bf16"),
    }
}

/// An engine problem as a one-line warning with the button that fixes it.
pub(crate) fn readiness(app: &mut PhotocraftApp, ui: &mut egui::Ui, model: ModelId, variant: &str) {
    let st = status();
    if let Err(why) = st.ready(model, variant) {
        readiness_warning(app, ui, &why, st.connected);
    }
}

pub(crate) fn readiness_warning(app: &mut PhotocraftApp, ui: &mut egui::Ui, why: &str, connected: bool) {
    let label = if connected { tl!("Get models…") } else { tl!("Set up AI…") };
    readiness_warning_to(app, ui, why, label, settings_ui::CONNECTION);
}

fn readiness_warning_to(app: &mut PhotocraftApp, ui: &mut egui::Ui, why: &str, label: &str, section: &str) {
    let t = Tokens::get(ui.ctx());
    let compact = ui.layout().main_dir().is_horizontal();
    let mut warning = |ui: &mut egui::Ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        crate::icons::paint(ui, r, "triangle-alert", 14.0, t.warning);
        resp.on_hover_text(why);
        if !compact {
            ui.label(RichText::new(why).color(t.warning));
        }
        if widgets::secondary_button(ui, label, 0.0).clicked() {
            crate::prefs_ui::open_preferences(app, section);
        }
    };
    if compact {
        warning(ui);
    } else {
        ui.horizontal_wrapped(warning);
    }
}

/// Options bar for the AI tools. Returns false for other tools.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) -> bool {
    match tool {
        Tool::AiRemove => {
            crate::ai_remove_ui::options(app, ui);
            true
        }
        Tool::AiCutout => {
            crate::background_ui::remove_button(app, ui, None, 0.0);
            crate::background_ui::readiness(app, ui);
            crate::background_ui::settings(app, ui, true);
            widgets::vline(ui, 22.0);
            opt(ui, tl!("Refine:"));
            let mut erase = !app.ui.ai.cutout_restore;
            if widgets::checkbox(ui, &mut erase, tl!("Erase")).clicked() {
                app.ui.ai.cutout_restore = false;
            }
            let mut restore = app.ui.ai.cutout_restore;
            if widgets::checkbox(ui, &mut restore, li_i18n::tr_ctx(li_i18n::current(), "ai", "Restore")).clicked() {
                app.ui.ai.cutout_restore = true;
            }
            widgets::vline(ui, 22.0);
            let resp = widgets::secondary_button(ui, tl!("Add Background ▾"), 0.0);
            egui::Popup::menu(&resp).show(|ui| {
                ui.set_min_width(200.0);
                if ui.button(tl!("Solid Color")).clicked() {
                    let r =
                        app.run("layer.newFillLayer.solidColor", json!({ "color": "#ffffff" })).and_then(|_| app.run("layer.arrange.sendBackward", json!({})));
                    report(app, r);
                    ui.close();
                }
                if ui.button(tl!("Image…")).clicked() {
                    let _ = crate::menus::invoke(app, ui.ctx(), "file.placeEmbedded", json!({}));
                    ui.close();
                }
                if ui.button(tl!("Generate (AI)…")).clicked() {
                    dialogs_mut(|d| d.background = true);
                    ui.close();
                }
            });
            true
        }
        _ => false,
    }
}

// Status pill

/// The AI engine status in the status bar (right-to-left layout); click opens Local AI.
pub fn status_pill(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if let Ok(mut c) = shared().ctx.lock()
        && c.is_none()
    {
        *c = Some(ui.ctx().clone());
    }
    let t = Tokens::get(ui.ctx());
    let st = status();
    let (dot, text) = if !st.checked {
        (t.text_faint, tl!("AI").to_owned())
    } else if st.starting {
        (t.warning, tl!("AI starting…").to_owned())
    } else if !st.connected {
        (t.text_faint, tl!("AI off").to_owned())
    } else if !st.any_ready() {
        (t.warning, tl!("AI · no models").to_owned())
    } else {
        let gpu = st.gpu.as_ref().map(|g| {
            let used = g.vram_used.map(|u| format!(" · {:.0}/{:.0} GB", u as f64 / 1e9, g.vram_total as f64 / 1e9)).unwrap_or_default();
            format!(" · {}{used}", short_gpu(&g.name))
        });
        (Color32::from_rgb(70, 190, 110), li_i18n::fmt(tl!("AI ready{gpu}"), &[("gpu", &gpu.unwrap_or_default())]))
    };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(11.5), t.text_dim);
    let size = vec2(galley.size().x + 26.0, 18.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 9.0, t.hover);
    }
    ui.painter().circle_filled(egui::pos2(r.left() + 10.0, r.center().y), 3.5, dot);
    ui.painter().galley(egui::pos2(r.left() + 18.0, r.center().y - galley.size().y / 2.0), galley, t.text_dim);
    let resp = resp.on_hover_text(if st.connected {
        li_i18n::fmt(tl!("ComfyUI at {host} — click for Local AI settings"), &[("host", &st.host)])
    } else {
        tl!("Local AI is off — click to set it up").into()
    });
    if resp.clicked() {
        open_local_ai(app);
    }
    if !st.checked || st.starting {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

/// The status bar's AI controls, drawn just left of [`status_pill`] (right-to-left layout):
/// a red Stop while AI jobs run, and Eject (unload the models from GPU memory) while the
/// engine is connected.
pub fn status_controls(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    status_controls_with(app, ui, &status());
}

/// AI jobs running in the session (Generate, AI Remove, Enhance, …).
pub fn ai_jobs(app: &PhotocraftApp) -> Vec<photocraft_engine::jobs::JobId> {
    app.session.jobs().into_iter().filter(|j| j.command.starts_with("ai.")).map(|j| j.id).collect()
}

/// A small status-bar icon button with an accessible name; disabled ones are faint and inert.
fn bar_icon(ui: &mut egui::Ui, icon: &str, tint: Color32, enabled: bool, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(20.0, 18.0), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    crate::icons::paint(ui, r, icon, 13.0, if enabled { tint } else { t.text_faint });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

pub(crate) fn status_controls_with(app: &mut PhotocraftApp, ui: &mut egui::Ui, st: &EngineStatus) {
    let t = Tokens::get(ui.ctx());
    let jobs = ai_jobs(app);
    if !jobs.is_empty() && bar_icon(ui, "circle-stop", t.danger, true, tl!("Stop generation")).clicked() {
        stop_generation(app, &jobs);
    }
    let tip = if st.connected { tl!("Unload AI models from GPU memory") } else { tl!("Unload AI models from GPU memory (the AI engine is off)") };
    if bar_icon(ui, "eject", t.text_dim, st.connected, tip).clicked() {
        app.ui.status = tl!("Unloading AI models…").into();
        app.ui.status_error = false;
        let lang = li_i18n::current();
        let ctx = ui.ctx().clone();
        let _ = std::thread::Builder::new().name("ai-unload".into()).spawn(move || {
            li_i18n::set_current(lang);
            let note = match li_ai::service().client.free_memory() {
                Ok(()) => (tl!("Models unloaded.").to_owned(), false),
                Err(e) => (format!("{e:#}"), true),
            };
            if let Ok(mut n) = REMOVE_NOTE.lock() {
                *n = Some(note);
            }
            refresh();
            ctx.request_repaint();
        });
    }
}

/// Cancel only the requested jobs. Each worker's bridged control owns its ComfyUI
/// prompt and handles targeted cancellation/retry; a process-wide prompt snapshot
/// could interrupt another feature's job.
pub fn stop_generation(app: &mut PhotocraftApp, jobs: &[photocraft_engine::jobs::JobId]) {
    for id in jobs {
        crate::jobs_ui::cancel(app, *id);
    }
    app.ui.status = tl!("Stopping generation…").into();
    app.ui.status_error = false;
}

fn short_gpu(name: &str) -> String {
    name.replace("NVIDIA ", "").replace("GeForce ", "").replace("Laptop GPU", "Laptop")
}

// Dialogs

#[derive(Default)]
struct Dialogs {
    subject_quality: Option<String>,
    background: bool,
    fill: bool,
    fill_target: Option<crate::command_feedback::SelectionTarget>,
    enhance: bool,
    enhance_scale: f32,
    detected: Option<Vec<li_ai::setup::Installation>>,
    settings: Option<AiSettings>,
    applied_settings: Option<AiSettings>,
    model_search: String,
    installed_only: Option<bool>,
    lan: Option<li_ai::discovery::Scan>,
    message: String,
    remove: Option<RemoveAsk>,
    /// The custom-model options each on-device function shows.
    seg_opts: BTreeMap<li_seg::Group, SegOpts>,
}

fn dialogs_mut<R>(f: impl FnOnce(&mut Dialogs) -> R) -> R {
    thread_local! { static D: std::cell::RefCell<Dialogs> = std::cell::RefCell::new(Dialogs { enhance_scale: 2.0, ..Default::default() }); }
    D.with(|d| f(&mut d.borrow_mut()))
}

pub fn open_local_ai(app: &mut PhotocraftApp) {
    refresh();
    crate::prefs_ui::open_preferences(app, settings_ui::CONNECTION);
}

/// Floating windows: Local AI, Generate Background, Generative Fill, AI Enhance, batch.
pub fn windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    li_seg::init_executor();
    // the saved on-device subject tier, until one is picked in this session
    if dialogs_mut(|d| d.subject_quality.is_none()) && app.ui.ai.cutout_engine == "fast" && li_seg::subject_quality(&photocraft_engine::seg::models_dir()) {
        app.ui.ai.cutout_engine = "better".into();
    }
    let changed = dialogs_mut(|d| subject_quality_changed(&mut d.subject_quality, &app.ui.ai.cutout_engine));
    if changed && matches!(app.ui.ai.cutout_engine.as_str(), "fast" | "better") {
        li_seg::set_subject_quality(&photocraft_engine::seg::models_dir(), app.ui.ai.cutout_engine == "better");
    }
    if dialogs_mut(|d| d.background) {
        prompt_dialog(app, ctx, tl!("Generate Background"), tl!("Describe the empty scene behind your subject"), true);
    }
    if dialogs_mut(|d| d.fill) {
        prompt_dialog(app, ctx, tl!("Generative Fill"), tl!("Describe what should appear in the selection"), false);
    }
    if dialogs_mut(|d| d.enhance) {
        enhance_dialog(app, ctx);
    }
    crate::generate_ui::draft_refine::window(app, ctx);
    crate::generate_ui::batch_window(app, ctx);
    crate::generate_ui::cloud_confirm_window(app, ctx);
    crate::model_browser::window(app, ctx);
    remove_confirm(app, ctx);
}

fn window_frame(ctx: &egui::Context) -> egui::Frame {
    let t = Tokens::get(ctx);
    egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(16))
}

fn prompt_dialog(app: &mut PhotocraftApp, ctx: &egui::Context, title: &str, hint: &str, background: bool) {
    let mut open = true;
    let mut close = false;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .frame(window_frame(ctx))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -80.0))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            let text = if background { &mut app.ui.ai.background_prompt } else { &mut app.ui.ai.fill_prompt };
            let r = ui.add(egui::TextEdit::multiline(text).hint_text(hint).desired_rows(3).desired_width(f32::INFINITY));
            r.request_focus();
            ui.add_space(8.0);
            let (m, v) = (ModelId::Qwen, "int8");
            if let Err(why) = status().ready(m, v) {
                ui.label(RichText::new(li_i18n::fmt(tl!("{why} Qwen Image 2.1 runs these."), &[("why", &why)])).color(Tokens::get(ctx).warning));
            }
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                if widgets::primary_button(ui, tl!("Generate"), 96.0).clicked() || enter {
                    let (cmd, p) = if background {
                        ("ai.generateBackground", json!({ "prompt": app.ui.ai.background_prompt }))
                    } else {
                        ("ai.generativeFill", json!({ "prompt": app.ui.ai.fill_prompt }))
                    };
                    let r = if !background && !dialogs_mut(|d| d.fill_target.as_ref().is_some_and(|target| target.is_current(app))) {
                        Err(tl!("The document or selection changed. Open Generative Fill again.").into())
                    } else {
                        app.run(cmd, p)
                    };
                    report(app, r);
                    close = true;
                }
                if widgets::secondary_button(ui, tl!("Cancel"), 80.0).clicked() {
                    close = true;
                }
            });
        });
    if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        dialogs_mut(|d| {
            if background {
                d.background = false
            } else {
                d.fill = false;
                d.fill_target = None;
            }
        });
    }
}

pub(crate) fn enhance_controls(ui: &mut egui::Ui, method: &mut li_ai::enhance::Method, backend: &mut li_ai::enhance::Backend, scale: &mut f32) {
    use li_ai::enhance::{Backend, Method};
    ui.horizontal(|ui| {
        for m in Method::ALL {
            ui.selectable_value(method, m, tl!(m.label()));
        }
    });
    ui.label(tl!(method.about()));
    ui.horizontal(|ui| {
        ui.label(tl!("Scale"));
        ui.selectable_value(scale, 2.0, "2×");
        ui.selectable_value(scale, 4.0, "4×");
    });
    if !matches!(*scale, 2.0 | 4.0) {
        *scale = 2.0;
    }
    // All enlargement routes require a GPU/NPU.
    *backend = Backend::Gpu;
    ui.label(tl!("AI enlargement requires a GPU/NPU."));
}

fn custom_seed_ready(status: &EngineStatus, unet: &str, vae: &str) -> bool {
    status.connected
        && status.object_info.as_ref().is_some_and(|info| {
            catalog::preset(ModelId::SeedVr2, "fp16").is_some_and(|p| p.required_nodes.iter().all(|node| info.has_node(node)))
                && ["UNETLoader", "VAELoader"].iter().all(|node| info.has_node(node))
                && info.choices("UNETLoader", "unet_name").iter().any(|name| name == unet)
                && info.choices("VAELoader", "vae_name").iter().any(|name| name == vae)
        })
}

pub(crate) fn enhance_ready(method: li_ai::enhance::Method, backend: li_ai::enhance::Backend, _scale: u32) -> Result<(), String> {
    use li_ai::enhance::{Backend, Method};
    if method == Method::Generative {
        let choice = li_ai::enhance::selection::load(&photocraft_engine::seg::models_dir(), method);
        if choice != li_ai::enhance::selection::default_choice(method) {
            let st = status();
            if !st.connected {
                return Err(tl!("Start ComfyUI to use the custom SeedVR2 models.").into());
            }
            let li_ai::enhance::selection::Choice::Seed { unet, vae } = choice else {
                return Err(tl!("Checking models…").into());
            };
            let ready = custom_seed_ready(&st, &unet, &vae);
            return if ready { Ok(()) } else { Err(tl!("Install the selected SeedVR2 model and VAE, then refresh ComfyUI.").into()) };
        }
        return status().ready(ModelId::SeedVr2, "fp16");
    }
    if backend == Backend::Cpu {
        return Err(tl!("AI enlargement requires a GPU/NPU.").into());
    }
    let dir = photocraft_engine::seg::models_dir();
    let spec = enhance_settings::selected_spec(&dir, method).ok_or_else(|| tl!("Choose a GPU enlargement model in Settings › Local AI.").to_owned())?;
    status().ready(ModelId::intern(spec.id), "default")
}

fn enhance_dialog(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut open = true;
    let mut close = false;
    let size = app.session.active().map(|d| (d.doc.size.width, d.doc.size.height));
    egui::Window::new(tl!("AI Enhance"))
        .collapsible(false)
        .resizable(false)
        .frame(window_frame(ctx))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -80.0))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(tl!("Enlarges the image. The result opens as a new document."));
            ui.add_space(8.0);
            let mut scale = dialogs_mut(|d| d.enhance_scale);
            enhance_controls(ui, &mut app.ui.ai.enhance_method, &mut app.ui.ai.enhance_backend, &mut scale);
            dialogs_mut(|d| d.enhance_scale = scale);
            if let Some((w, h)) = size {
                let (nw, nh) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
                ui.label(RichText::new(format!("{w} × {h}  →  {nw} × {nh} px")).color(Tokens::get(ctx).text_dim));
            }
            let ready = enhance_ready(app.ui.ai.enhance_method, app.ui.ai.enhance_backend, scale as u32);
            if let Err(why) = &ready {
                ui.label(RichText::new(why).color(Tokens::get(ctx).warning));
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled_ui(ready.is_ok(), |ui| widgets::primary_button(ui, tl!("Enhance"), 96.0)).inner.clicked() {
                    let r = app.run("ai.enhance", json!({ "scale": scale, "method": app.ui.ai.enhance_method, "backend": app.ui.ai.enhance_backend }));
                    report(app, r);
                    close = true;
                }
                if widgets::secondary_button(ui, tl!("Cancel"), 80.0).clicked() {
                    close = true;
                }
            });
        });
    if !open || close {
        dialogs_mut(|d| d.enhance = false);
    }
}

fn dls_running() -> bool {
    downloads().values().any(|d| !d.finished)
}

fn model_row(ui: &mut egui::Ui, t: &Tokens, info: &catalog::ModelInfo, p: &catalog::Preset, st: &EngineStatus, dl: Option<&Download>, settings: &AiSettings) {
    ui.scope(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{} · {}", info.label, p.label)).strong());
                ui.label(
                    RichText::new(li_i18n::fmt(
                        tl!("{best_for} · {size} · {gb} GB GPU"),
                        &[("best_for", &info.best_for), ("size", &li_ai::download::human_bytes(p.total_bytes())), ("gb", &info.vram_gb.to_string())],
                    ))
                    .color(t.text_dim)
                    .size(11.5),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let missing = li_ai::download::missing_files(&settings.model_dir(), p);
                let state = st.presets.get(&p.id());
                let downloading = dl.is_some_and(|d| !d.finished);
                // Some of its files are on disk: it can be removed (shared ones stay).
                if !settings.is_remote() && !downloading && missing.len() < p.files.len() {
                    if removing(&p.id()) {
                        ui.spinner();
                    } else if remove_button(ui) {
                        ask_remove(format!("{} · {}", info.label, p.label), RemoveTarget::Presets(vec![p.id()]));
                    }
                }
                match (dl, state) {
                    (Some(d), _) if !d.finished => {
                        if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                            d.ctl.cancel();
                        }
                        let frac = if d.total > 0 { d.done as f32 / d.total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac).desired_width(140.0).text(format!("{:.0}%", frac * 100.0)));
                    }
                    (_, Some(Ok(()))) if st.connected => {
                        ui.label(RichText::new(tl!("Ready")).color(t.text_dim));
                    }
                    _ if settings.is_remote() && !st.connected => {
                        ui.label(RichText::new(tl!("Server offline")).color(t.text_dim));
                    }
                    _ if settings.is_remote() && p.installed => {
                        ui.label(RichText::new(tl!("Needs setup")).color(t.warning));
                    }
                    _ if settings.is_remote() => {
                        ui.label(RichText::new(tl!("Install on server")).color(t.text_dim));
                    }
                    _ if missing.is_empty() => {
                        ui.label(RichText::new(if st.connected { tl!("Files present — restart ComfyUI") } else { tl!("Files present") }).color(t.text_dim));
                    }
                    _ => {
                        let label = if p.access_url.is_some() { tl!("Request Access…") } else { tl!("Download") };
                        if widgets::secondary_button(ui, label, 0.0).clicked() {
                            if let Some(url) = p.access_url.clone().filter(|_| dl.and_then(|d| d.error.as_ref()).is_some()) {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            } else {
                                start_download(&p.id());
                            }
                        }
                    }
                }
            });
        });
        if let Some(err) = dl.and_then(|d| d.error.as_ref()) {
            ui.label(RichText::new(err).color(t.danger).size(11.5));
        } else if let Some(Err(why)) = st.presets.get(&p.id()).filter(|_| st.connected) {
            ui.label(RichText::new(why).color(t.text_faint).size(11.0));
        }
    });
    ui.add_space(4.0);
}

// Menus

const IDS: &[&str] = &[
    "ai.restore",
    "li.newFromPrompt",
    "li.openFolder",
    "li.batchRemoveBackgrounds",
    "li.removeSelection",
    "ai.remove",
    "li.generativeFill",
    "li.enhance",
    "li.generateBackground",
    "li.panel.generate",
    "li.draftRefine",
    "li.localAi",
    "li.browseModels",
    crate::context_bar::TOGGLE_ID,
    "li.filmstrip",
    crate::develop_layer::DEVELOP_ID,
];

pub fn handles(id: &str) -> bool {
    IDS.contains(&id)
}

pub fn is_enabled(app: &PhotocraftApp, id: &str) -> Option<bool> {
    let has_doc = app.session.active().is_some();
    Some(match id {
        "ai.remove" | "li.removeSelection" | "li.generativeFill" => crate::command_feedback::has_pixel_selection(app),
        "li.enhance" | "li.generateBackground" | "ai.restore" => has_doc,
        "li.filmstrip" => crate::filmstrip_ui::has_folder(),
        crate::develop_layer::DEVELOP_ID => crate::develop_layer::active_photo(app).is_some(),
        _ if handles(id) => true,
        _ => return None,
    })
}

pub fn menu(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !handles(id) {
        return None;
    }
    match id {
        "li.newFromPrompt" | "li.panel.generate" => {
            app.ui.panels.generate = true;
            app.ui.dock_tabs.generate = 0;
            if app.ui.dock.is_collapsed(crate::dock::Group::Generate) {
                app.ui.dock.set_collapsed(crate::dock::Group::Generate, false);
            }
            // Prompting needs room: Properties folds away (its tab stays one click from open),
            // as Photoshop's contextual panels make way for the one in use.
            app.ui.dock.set_collapsed(crate::dock::Group::Properties, true);
            // Room for the prompt, settings and a row of results, unless the user sized it.
            app.ui.dock.heights.entry(crate::dock::Group::Generate).or_insert(440.0);
            if id == "li.newFromPrompt" {
                app.ui.ai.generate.mode = crate::generate_ui::Mode::Create;
                crate::generate_ui::focus_prompt(ctx);
            }
        }
        // `{"path": dir}` opens that folder without the dialog (automation, the control channel).
        "li.openFolder" => {
            return Some(match params.get("path").and_then(Value::as_str) {
                Some(dir) => crate::generate_ui::open_folder_path(app, std::path::Path::new(dir)),
                None => crate::generate_ui::open_folder(app),
            });
        }
        "li.draftRefine" => crate::generate_ui::draft_refine::open(app),
        "ai.restore" => crate::restoration_ui::open(app),
        "li.batchRemoveBackgrounds" => crate::generate_ui::open_batch(),
        "ai.remove" | "li.removeSelection" => crate::ai_remove_ui::selection(app),
        "li.generativeFill" => dialogs_mut(|d| {
            d.fill = true;
            d.fill_target = crate::command_feedback::SelectionTarget::capture(app);
        }),
        "li.generateBackground" => dialogs_mut(|d| d.background = true),
        "li.enhance" => dialogs_mut(|d| d.enhance = true),
        "li.localAi" => open_local_ai(app),
        "li.filmstrip" => crate::filmstrip_ui::toggle(),
        crate::develop_layer::DEVELOP_ID => return Some(crate::develop_layer::develop(app)),
        crate::context_bar::TOGGLE_ID => return crate::context_bar::menu(app, id),
        // `{"kind": "lora", "family": id}` opens it on a family's LoRAs.
        "li.browseModels" => match (params.get("kind").and_then(Value::as_str), params.get("family").and_then(Value::as_str)) {
            (Some("lora"), Some(f)) => crate::model_browser::open_loras(f),
            _ => crate::model_browser::open(),
        },
        _ => {}
    }
    Some(Ok(Value::Null))
}

/// AI jobs that end: Generate results and batch items. Returns true when handled.
pub fn on_job_event(app: &mut PhotocraftApp, e: &JobEvent) -> bool {
    if e.command == crate::generate_ui::GENERATE_JOB {
        crate::generate_ui::on_generated(app, e);
        return true;
    }
    if e.command.starts_with("ai.")
        && let JobOutcome::Failed(err) = &e.outcome
    {
        app.ui.status = format!("{}: {err}", e.label);
        app.ui.status_error = true;
        refresh();
        return false;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT, Queryable};

    #[test]
    fn remove_confirm_default_preserves_explicit_saved_immediate_choice() {
        // Owner decision: brush, adjust, then confirm. Apply-on-release is an opt-in.
        assert!(!AiOptions::default().remove_immediately);
        assert!(!serde_json::from_value::<AiOptions>(json!({})).unwrap().remove_immediately);
        let immediate: AiOptions = serde_json::from_value(json!({"remove_immediately": true})).unwrap();
        assert!(immediate.remove_immediately);
        assert!(serde_json::from_value::<AiOptions>(serde_json::to_value(immediate).unwrap()).unwrap().remove_immediately);
    }

    #[test]
    fn enhance_dialog_clicks_method_scale_and_backend() {
        let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 31, "height": 19})).unwrap();
        let app = PhotocraftApp::new(session, crate::Services::default());
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(900.0, 700.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                enhance_dialog(app, ui.ctx());
            },
            app,
        );
        h.run_steps(3);
        h.get_by_label("Faithful").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.enhance_method, li_ai::enhance::Method::Faithful);
        assert!(h.query_by_label("CPU").is_none());
        h.get_by_label("AI enlargement requires a GPU/NPU.");
        assert_eq!(h.state().ui.ai.enhance_backend, li_ai::enhance::Backend::Gpu);
        h.get_by_label("4×").click();
        h.run_steps(2);
        assert_eq!(dialogs_mut(|d| d.enhance_scale), 4.0);
        h.get_by_label("Rescue").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.enhance_method, li_ai::enhance::Method::Rescue);
        h.get_by_label("Generative (SeedVR2)").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.ai.enhance_method, li_ai::enhance::Method::Generative);
        assert_eq!(serde_json::from_value::<AiOptions>(json!({})).unwrap().enhance_method, li_ai::enhance::Method::Generative);
        dialogs_mut(|d| d.enhance_scale = 2.0);
    }

    /// Quitting stops the ComfyUI Local Image started (the backlog item: it kept running after
    /// the app closed, holding 29-46 GB of RAM).
    #[cfg(unix)]
    #[test]
    fn quitting_stops_the_engine_we_started() {
        let _g = AI_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut pid = 0;
        CHILD
            .start(|| {
                let child = std::process::Command::new("sleep").arg("60").spawn().map_err(|e| e.to_string())?;
                pid = child.id();
                Ok((child, "test child".into()))
            })
            .unwrap();
        assert_ne!(pid, 0);
        assert!(shutdown_engine(), "the started engine is stopped");
        assert!(!CHILD.is_running().unwrap());
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "process {pid} is gone (reaped)");
        assert!(!shutdown_engine(), "nothing to stop the second time (a user-started ComfyUI is never touched)");
    }

    /// Every local CPU model Settings offers must come from a host the downloader accepts (the
    /// sky models' host was missing, so both refused to download).
    #[test]
    fn every_local_model_downloads_from_an_allowed_host() {
        let refused: Vec<_> =
            li_seg::MODELS.iter().flat_map(|m| m.files()).filter(|f| !li_ai::download::host_allowed(f.url)).map(|f| (f.file, f.url)).collect();
        assert!(refused.is_empty(), "{refused:?}");
    }

    const MAP_32: &[u8] = include_bytes!("test_fixtures/map_32.onnx");
    /// Echoes its RGB input: not a single map (so not a subject model), but 3 class channels.
    const ECHO_32: &[u8] = include_bytes!("test_fixtures/echo_32.onnx");

    pub(super) fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("li-seg-ui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("segmentation")).unwrap();
        dir
    }

    fn pick_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join("downloads").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// The file the next "Choose an ONNX file…" click picks.
    fn will_pick(path: Option<std::path::PathBuf>) {
        TEST_PICK.with(|p| *p.borrow_mut() = path);
    }

    /// A harness drawing the on-device model rows (all of them, or one function's) for `dir`.
    fn seg_harness(dir: &std::path::Path, only: Option<li_seg::Group>) -> egui_kittest::Harness<'static> {
        let dir = dir.to_path_buf();
        // This fixture draws all model functions without the Settings scroll area.
        let mut h = egui_kittest::Harness::builder().with_size(vec2(760.0, 2400.0)).build_ui(move |ui| {
            // fonts set on the context only apply from the next frame
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            let t = Tokens::get(ui.ctx());
            let dls = BTreeMap::new();
            match only {
                Some(g) => function_row(ui, &t, &dls, &dir, g),
                None => seg_rows(ui, &t, &dls, &dir),
            }
        });
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(3);
        h
    }

    /// Waits for the background custom-model test of `group` to finish.
    fn settle(h: &mut egui_kittest::Harness<'static>, dir: &std::path::Path, group: li_seg::Group) {
        for _ in 0..300 {
            h.run_steps(1);
            if !matches!(custom_job(dir, group), Some(CustomJob::Testing)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        h.run_steps(3);
    }

    /// One row per function; tagging offers CLIP, and the three mask functions
    /// retain custom-model buttons. None of the retired models is offered.
    #[test]
    fn settings_lists_one_row_per_function() {
        let dir = temp_dir("rows");
        let h = seg_harness(&dir, None);
        for title in ["Subject & Background", "Sky", "Depth", "Smart Sort (scenes)", "Faces", "Raw Denoise", "RGB Denoise", "AI Remove"] {
            h.get_by_label(title);
        }
        for model in [
            "Fast · BRIA RMBG-1.4",
            "Better Quality · InSPyReNet SwinB Plus-Ultra",
            "PP-MobileSeg",
            "Depth Anything V2 Small",
            "CLIP ViT-B-32 LAION",
            "YuNet + SFace",
            "LaMa · On-device",
        ] {
            h.get_by_label(model);
        }
        for gone in ["IS-Net general", "U²-Net small", "U²-Net", "TinySkyNet", "MiDaS v2.1 small"] {
            assert!(h.query_by_label(gone).is_none(), "{gone} is still listed");
        }
        assert_eq!(h.get_all_by_label("Custom model…").count(), 3);
        assert_eq!(h.get_all_by_label("Download").count(), 9);
        for title in ["Upscale Faithful", "Upscale Rescue", "Generative"] {
            h.get_by_label(title);
        }
        assert_eq!(h.get_all_by_label("Change…").count(), 5);
        assert!(h.query_by_label_contains("Finds the main subject").is_some());
        assert!(h.query_by_label_contains("Finds the sky.").is_some());
        assert!(h.query_by_label_contains("Estimates how far away").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ai_denoise_settings_change_custom_and_use_default_clicks() {
        for group in [li_seg::Group::RawDenoise, li_seg::Group::RgbDenoise] {
            let dir = temp_dir(group.key());
            let mut h = seg_harness(&dir, Some(group));
            h.get_by_label(group.label());
            h.get_by_label("Change…").click();
            h.run_steps(2);
            will_pick(Some(pick_file(&dir, "restore.onnx", ECHO_32)));
            h.get_by_label("Choose an ONNX file…").click();
            settle(&mut h, &dir, group);
            assert!(li_seg::custom(&dir, group).is_some());
            h.get_by_label("Custom: restore.onnx");
            h.get_by_label("Your selected restoration model. Requires a GPU.");
            assert!(h.query_by_label_contains("CPU fallback").is_none());
            h.get_by_label("Use default").click();
            h.run_steps(3);
            assert!(li_seg::custom(&dir, group).is_none());
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Picking a model sets it; its row says "Custom: <file>" and offers the way back.
    #[test]
    fn the_custom_model_flow_sets_and_reverts_a_model() {
        let dir = temp_dir("flow");
        std::fs::write(li_seg::model_path(&dir, li_seg::Group::Subject.official()), b"x").unwrap();
        let mut h = seg_harness(&dir, Some(li_seg::Group::Subject));
        h.get_by_label("In use");
        assert!(h.query_by_label("Choose an ONNX file…").is_none(), "options are hidden until asked for");
        h.get_by_label("Custom model…").click();
        h.run_steps(2);
        h.get_by_label("Input style:");
        // pick a model (the dialog is replaced by the test hook)
        will_pick(Some(pick_file(&dir, "my-subject.onnx", MAP_32)));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Subject);
        let c = li_seg::custom(&dir, li_seg::Group::Subject).expect("the model was accepted");
        assert_eq!((c.name.as_str(), c.size, c.norm), ("my-subject.onnx", 32, li_seg::Norm::IsNet));
        assert!(c.path(&dir).is_file());
        h.get_by_label("Custom: my-subject.onnx");
        assert!(h.query_by_label("Choose an ONNX file…").is_none(), "the options close");
        // the official model stays installed but is no longer the one in use
        h.get_by_label("Installed");
        assert!(matches!(li_seg::Group::Subject.in_use(&dir), Some(li_seg::InUse::Custom(_))));
        // back to the official model: the copy goes away
        h.get_by_label("Use official model").click();
        h.run_steps(3);
        assert!(li_seg::custom(&dir, li_seg::Group::Subject).is_none() && !c.path(&dir).exists());
        assert!(h.query_by_label_contains("Custom: ").is_none());
        h.get_by_label("In use");
        assert!(matches!(li_seg::Group::Subject.in_use(&dir), Some(li_seg::InUse::Official(_))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_model_that_does_not_fit_is_explained_and_not_kept() {
        let dir = temp_dir("bad");
        let mut h = seg_harness(&dir, Some(li_seg::Group::Subject));
        h.get_by_label("Custom model…").click();
        h.run_steps(2);
        will_pick(Some(pick_file(&dir, "echo.onnx", ECHO_32)));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Subject);
        assert!(li_seg::custom(&dir, li_seg::Group::Subject).is_none());
        h.get_by_label_contains("That model can't be used: Not a subject model");
        // a text file is refused too, and cancelling the dialog changes nothing
        will_pick(Some(pick_file(&dir, "notes.txt", b"hi")));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Subject);
        h.get_by_label_contains("a file ending in .onnx");
        will_pick(None);
        h.get_by_label("Choose an ONNX file…").click();
        h.run_steps(2);
        assert!(li_seg::custom(&dir, li_seg::Group::Subject).is_none());
        assert!(!li_seg::custom_dir(&dir).exists() || std::fs::read_dir(li_seg::custom_dir(&dir)).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Each function shows only the options that matter to it, and the choices are used.
    #[test]
    fn the_options_follow_the_function() {
        let dir = temp_dir("opts");
        let mut h = seg_harness(&dir, Some(li_seg::Group::Depth));
        h.get_by_label("Custom model…").click();
        h.run_steps(2);
        // depth models start ImageNet-normalised, as inverse depth
        h.get_by_label("Input style:");
        h.get_by_value("U²-Net / ImageNet style");
        h.get_by_value("Inverse depth (nearer = larger)");
        assert!(h.query_by_label("Sky class:").is_none());
        h.get_by_label("Choose an ONNX file…");
        let mut h = seg_harness(&dir, Some(li_seg::Group::Subject));
        h.get_by_label("Custom model…").click();
        h.run_steps(2);
        h.get_by_value("IS-Net style");
        assert!(h.query_by_label("Sky class:").is_none() && h.query_by_label("Depth output:").is_none());
        // the U²-Net / ImageNet choice is what gets stored
        dialogs_mut(|d| d.seg_opts.get_mut(&li_seg::Group::Subject).unwrap().norm = Some(li_seg::Norm::ImageNet));
        h.run_steps(2);
        h.get_by_value("U²-Net / ImageNet style");
        will_pick(Some(pick_file(&dir, "u2.onnx", MAP_32)));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Subject);
        assert_eq!(li_seg::custom(&dir, li_seg::Group::Subject).unwrap().norm, li_seg::Norm::ImageNet);
        // sky: the echo model has 3 class channels; the default class 2 fits, class 7 doesn't
        let mut h = seg_harness(&dir, Some(li_seg::Group::Sky));
        h.get_by_label("Custom model…").click();
        h.run_steps(2);
        h.get_by_label("Sky class:");
        h.get_by_value("U²-Net / ImageNet style");
        dialogs_mut(|d| d.seg_opts.get_mut(&li_seg::Group::Sky).unwrap().sky_class = Some(7));
        will_pick(Some(pick_file(&dir, "sky.onnx", ECHO_32)));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Sky);
        assert!(li_seg::custom(&dir, li_seg::Group::Sky).is_none());
        h.get_by_label_contains("out of range");
        dialogs_mut(|d| d.seg_opts.get_mut(&li_seg::Group::Sky).unwrap().sky_class = None);
        will_pick(Some(pick_file(&dir, "sky.onnx", ECHO_32)));
        h.get_by_label("Choose an ONNX file…").click();
        settle(&mut h, &dir, li_seg::Group::Sky);
        let c = li_seg::custom(&dir, li_seg::Group::Sky).expect("class 2 of 3 fits");
        assert_eq!((c.classes, c.class), (3, 2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancel_click_reaches_shared_registry_downloader_without_io() {
        let dir = temp_dir("cancel");
        let draw_dir = dir.clone();
        let ctl = JobControl::new();
        let observed = ctl.clone();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(760.0, 400.0)).build_ui(move |ui| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            let t = Tokens::get(ui.ctx());
            let dls = BTreeMap::from([(
                "seg:sface".into(),
                Download { total: li_ai::catalog::on_device_files("sface").unwrap().download_bytes(), done: 100, ctl: ctl.clone(), ..Default::default() },
            )]);
            function_row(ui, &t, &dls, &draw_dir, li_seg::Group::Faces);
        });
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(3);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        let error =
            download_seg_files(li_ai::catalog::on_device_spec("sface").unwrap(), &dir, &observed, &|_| panic!("cancel must not advance progress")).unwrap_err();
        assert!(error.is::<lightcraft_sysmem::jobs::Cancelled>());
        assert_eq!(std::fs::read_dir(dir.join("segmentation")).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bundle_download_progress_is_cumulative_and_stops_on_failure() {
        let spec = li_seg::spec("clip-b32-laion").unwrap();
        let calls = std::cell::RefCell::new(Vec::new());
        li_ai::catalog::on_device_files(spec.id)
            .unwrap()
            .visit(&|file, offset| {
                calls.borrow_mut().push((file.name.clone(), offset, offset + file.bytes));
                Ok(())
            })
            .unwrap();
        let calls = calls.into_inner();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].1, 0);
        for pair in calls.windows(2) {
            assert_eq!(pair[0].2, pair[1].1);
        }
        assert_eq!(calls.last().unwrap().2, spec.download_bytes());
        let count = std::cell::Cell::new(0);
        assert!(
            li_ai::catalog::on_device_files(spec.id)
                .unwrap()
                .visit(&|_, _| {
                    count.set(count.get() + 1);
                    if count.get() == 2 {
                        anyhow::bail!("cancelled");
                    }
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(count.get(), 2);
    }

    /// Tagging is its own function row and never offers the single-ONNX custom flow.
    #[test]
    fn smart_sort_function_has_no_custom_model_flow() {
        let dir = temp_dir("tagging");
        let h = seg_harness(&dir, Some(li_seg::Group::Tagging));
        h.get_by_label("Smart Sort (scenes)");
        h.get_by_label("CLIP ViT-B-32 LAION");
        assert!(h.query_by_label("Custom model…").is_none());
        assert!(h.query_by_label("Use official model").is_none());
        assert!(h.query_by_label("Choose an ONNX file…").is_none());
        let size = li_ai::download::human_bytes(li_seg::Group::Tagging.official().download_bytes());
        h.get_by_label_contains(&format!("{size} download"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Clicking Cancel on the Faces bundle cancels the entire two-file job, without network.
    #[test]
    fn faces_function_is_one_row_without_custom_options_and_cancel_works() {
        let dir = temp_dir("faces");
        let h = seg_harness(&dir, Some(li_seg::Group::Faces));
        h.get_by_label("Faces");
        h.get_by_label("YuNet + SFace");
        h.get_by_label("Download");
        assert!(h.query_by_label("Custom model…").is_none());
        assert!(h.query_by_label("Choose an ONNX file…").is_none());
        let ctl = JobControl::new();
        let observed = ctl.clone();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(760.0, 400.0)).build_ui(move |ui| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            let t = Tokens::get(ui.ctx());
            let mut dls = BTreeMap::new();
            dls.insert(
                "seg:sface".into(),
                Download { total: li_seg::Group::Faces.official().download_bytes(), done: 100, ctl: ctl.clone(), ..Default::default() },
            );
            function_row(ui, &t, &dls, &dir, li_seg::Group::Faces);
        });
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(3);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(observed.is_cancelled());
        let files = std::cell::RefCell::new(Vec::new());
        li_ai::catalog::on_device_files("sface")
            .unwrap()
            .visit(&|file, offset| {
                files.borrow_mut().push((file.name.clone(), offset));
                Ok(())
            })
            .unwrap();
        assert_eq!(files.borrow().len(), 2);
        assert_eq!(files.borrow()[1].1, li_seg::faces::SFACE.bytes);
    }

    #[test]
    fn smart_sort_clip_cancel_click_stops_download() {
        let dir = temp_dir("clip-download");
        let ctl = JobControl::new();
        let observed = ctl.clone();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(760.0, 600.0)).build_ui(move |ui| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            let t = Tokens::get(ui.ctx());
            let mut dls = BTreeMap::new();
            dls.insert(
                "seg:clip-b32-laion".into(),
                Download { total: li_seg::Group::Tagging.official().download_bytes(), done: 100, ctl: ctl.clone(), ..Default::default() },
            );
            function_row(ui, &t, &dls, &dir, li_seg::Group::Tagging);
        });
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(3);
        h.get_by_label("CLIP ViT-B-32 LAION");
        assert_eq!(h.get_all_by_label("Cancel").count(), 1);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(observed.is_cancelled());
    }

    #[test]
    fn custom_model_reasons_follow_the_ui_language() {
        let reason = li_i18n::with_language(li_i18n::Lang::from_code("ja").unwrap(), || custom_model_reason(&li_seg::CustomModelError::UnsupportedFunction));
        assert_eq!(reason, "この機能ではカスタムモデルを使用できません");
    }

    /// A job that runs until cancelled.
    fn endless(app: &mut PhotocraftApp, command: &str, gate: &Arc<std::sync::atomic::AtomicBool>) -> photocraft_engine::jobs::JobId {
        let gate = gate.clone();
        match app
            .session
            .start_job(
                command,
                json!({}),
                command,
                false,
                move |ctx| {
                    while !gate.load(std::sync::atomic::Ordering::Relaxed) {
                        ctx.check()?;
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                },
                |_, ()| Ok(Value::Null),
            )
            .unwrap()
        {
            photocraft_engine::jobs::Started::Job(id) => id,
            photocraft_engine::jobs::Started::Done(v) => panic!("ran inline: {v}"),
        }
    }

    #[test]
    fn stop_generation_cancels_only_requested_ai_jobs() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let generate = endless(&mut app, "ai.generate", &gate);
        let remove = endless(&mut app, "ai.remove", &gate);
        stop_generation(&mut app, &[generate]);
        assert_eq!(ai_jobs(&app), vec![remove], "another AI feature must retain its job");
        gate.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(app.session.wait_job(generate).is_err());
        assert!(app.session.wait_job(remove).is_ok());
    }

    /// The status bar's AI controls: Eject and a red Stop (shown while AI jobs run) render, and
    /// Stop cancels the AI jobs only.
    #[test]
    fn stop_cancels_ai_jobs_and_shared_icons_render() {
        assert!(li_ui_kit::icons::exists("eject") && li_ui_kit::icons::exists("circle-stop"));
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let connected = EngineStatus { checked: true, connected: true, ..Default::default() };
        let mut h = egui_kittest::Harness::builder().with_size(vec2(400.0, 40.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| status_controls_with(app, ui, &connected));
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);
        // No AI work: Eject only.
        assert!(h.query_by_label("Stop generation").is_none());
        h.get_by_label("Unload AI models from GPU memory");
        let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ai = endless(h.state_mut(), "ai.generate", &gate);
        let other = endless(h.state_mut(), "test.slow", &gate);
        h.run_steps(2);
        assert_eq!(ai_jobs(h.state()), vec![ai]);
        h.get_by_label("Stop generation").click();
        h.run_steps(2);
        assert!(ai_jobs(h.state()).is_empty(), "the AI job was cancelled");
        assert!(h.state().session.job(other).is_some(), "other work keeps running");
        assert!(h.query_by_label("Stop generation").is_none());
        assert_eq!(h.state().ui.status, "Stopping generation…");
        // Both icons were rasterised from their SVGs.
        for icon in ["eject", "circle-stop"] {
            let poll = li_ui_kit::icons::image(icon, 13.0, Color32::WHITE).load_for_size(&h.ctx, vec2(13.0, 13.0));
            assert!(matches!(poll, Ok(egui::load::TexturePoll::Ready { .. })), "{icon} did not rasterise");
        }
        gate.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = h.state_mut().session.wait_job(other);
    }

    #[test]
    fn ds_finding_unload_completion_wakes_idle_ui() {
        // Full-app tests consume REMOVE_NOTE and other AI tests replace the service.
        // Keep both globals private to this regression so completion remains observable.
        if std::env::var_os("PC_AI_UNLOAD_TEST_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "ai_ui::tests::ds_finding_unload_completion_wakes_idle_ui", "--nocapture"])
                .env("PC_AI_UNLOAD_TEST_CHILD", "1")
                .output()
                .unwrap();
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            assert!(output.status.success(), "Unload completion child failed: {}", String::from_utf8_lossy(&output.stdout));
            return;
        }
        let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = li_ai::mock::MockComfy::start().unwrap();
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                li_ai::set_service_override(None);
            }
        }
        let _reset = Reset;
        li_ai::set_service_override(Some(server.host().into()));
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let connected = EngineStatus { checked: true, connected: true, ..Default::default() };
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(400.0, 40.0))
            .build_ui_state(move |ui, app: &mut PhotocraftApp| status_controls_with(app, ui, &connected), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);

        // Hold status publication until the click, SVG loading and hover animations settle.
        let mut note = REMOVE_NOTE.lock().unwrap();
        *note = None;
        h.get_by_label("Unload AI models from GPU memory").click();
        h.remove_cursor();
        h.run();
        assert_eq!(h.state().ui.status, "Unloading AI models…");
        assert!(!h.ctx.has_requested_repaint(), "the UI must be idle before worker completion");

        let (tx, rx) = std::sync::mpsc::channel();
        h.ctx.set_request_repaint_callback(move |info| {
            if std::thread::current().name() == Some("ai-unload") {
                let _ = tx.send(info);
            }
        });
        drop(note);
        let info = rx.recv_timeout(Duration::from_secs(10)).expect("worker must request repaint after publishing its status");
        assert_eq!(info.viewport_id, egui::ViewportId::ROOT);
        assert_eq!(info.delay, Duration::ZERO);
        assert!(h.ctx.has_requested_repaint());
        assert_eq!(REMOVE_NOTE.lock().unwrap().take().unwrap(), ("Models unloaded.".into(), false));
    }

    /// Disconnected: Eject is shown but inert.
    #[test]
    fn eject_is_disabled_without_the_engine() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let off = EngineStatus { checked: true, ..Default::default() };
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(400.0, 40.0))
            .build_ui_state(move |ui, app: &mut PhotocraftApp| status_controls_with(app, ui, &off), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);
        let node = h.get_by_label("Unload AI models from GPU memory (the AI engine is off)");
        assert!(node.accesskit_node().is_disabled());
    }
}

#[cfg(test)]
#[test]
fn ds2_custom_seed_readiness_requires_files_and_nodes() {
    let mut info = li_ai::comfy::ObjectInfo(li_ai::mock::object_info());
    info.0["UNETLoader"]["input"]["required"]["unet_name"] = json!([["custom-seed.safetensors"]]);
    info.0["VAELoader"]["input"]["required"]["vae_name"] = json!([["custom-vae.safetensors"]]);
    let mut st = EngineStatus { connected: true, object_info: Some(Arc::new(info.clone())), ..Default::default() };
    assert!(custom_seed_ready(&st, "custom-seed.safetensors", "custom-vae.safetensors"));
    assert!(!custom_seed_ready(&st, "missing.safetensors", "custom-vae.safetensors"));
    info.0.as_object_mut().unwrap().remove("SeedVR2Preprocess");
    st.object_info = Some(Arc::new(info));
    assert!(!custom_seed_ready(&st, "custom-seed.safetensors", "custom-vae.safetensors"));
}

#[cfg(test)]
#[test]
fn reasons_ux_generative_fill_captures_context_and_rejects_retarget_before_generate() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width":96,"height":64})).unwrap();
    app.run("select.rect", json!({"x":8,"y":8,"width":20,"height":20})).unwrap();
    app.ui.tool = crate::Tool::RectMarquee;
    crate::canvas_tool_menu::open(&mut app, crate::Tool::RectMarquee, [20., 20.]);
    let mut h = Harness::builder().with_size(vec2(900., 850.)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            crate::canvas_tool_menu::show(app, ui.ctx());
            if dialogs_mut(|d| d.fill) {
                prompt_dialog(app, ui.ctx(), "Generative Fill", "Describe the selection", false);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(3);
    h.get_by_label("Generative Fill…").click();
    h.run_steps(3);
    assert!(dialogs_mut(|d| d.fill_target.as_ref().unwrap().is_current(h.state())));
    let journal = h.state().session.journal.len();
    h.state_mut().run("select.all", json!({})).unwrap();
    h.get_by_label("Generate").click();
    h.run_steps(3);
    assert!(!dialogs_mut(|d| d.fill));
    assert_eq!(h.state().ui.status, "The document or selection changed. Open Generative Fill again.");
    assert!(!h.state().session.journal[journal..].iter().any(|(id, _)| id == "ai.generativeFill"));
}

#[cfg(test)]
#[test]
fn reasons_ux_generative_fill_real_click_submits_captured_selection_to_existing_engine() {
    use egui_kittest::{Harness, kittest::Queryable};
    let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = li_ai::mock::MockComfy::start().unwrap();
    li_ai::set_service_override(Some(server.host().into()));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
        }
    }
    let _reset = Reset;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.background_jobs = false;
    app.run("file.new", json!({"width":96,"height":64})).unwrap();
    app.run("select.rect", json!({"x":8,"y":10,"width":20,"height":16})).unwrap();
    app.ui.tool = crate::Tool::RectMarquee;
    let original = app.session.active().unwrap().active_layer;
    crate::canvas_tool_menu::open(&mut app, crate::Tool::RectMarquee, [20., 20.]);
    let mut h = Harness::builder().with_size(vec2(900., 850.)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            crate::canvas_tool_menu::show(app, ui.ctx());
            if dialogs_mut(|d| d.fill) {
                prompt_dialog(app, ui.ctx(), "Generative Fill", "Describe the selection", false);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(3);
    h.get_by_label("Generative Fill…").click();
    h.run_steps(3);
    assert!(server.prompts().is_empty());
    h.get_by_label("Generate").click();
    h.run_steps(3);
    assert_eq!(server.prompts().len(), 1);
    let st = h.state().session.active().unwrap();
    let result = st.doc.layer(st.active_layer.unwrap()).unwrap();
    assert_ne!(st.active_layer, original);
    let surface = result.surface().unwrap();
    let selection = st.doc.selection.as_ref().unwrap();
    for y in 0..64 {
        for x in 0..96 {
            assert_eq!(surface.sample_channel(x, y, 3), selection.sample_channel(x, y, 0), "captured selection coverage at {x},{y}");
        }
    }
    assert_eq!(result.generation.as_ref().unwrap()["bounds"], json!([8, 10, 20, 16]));
}

fn subject_quality_changed(previous: &mut Option<String>, engine: &str) -> bool {
    if previous.as_deref() == Some(engine) {
        return false;
    }
    *previous = Some(engine.to_owned());
    true
}

#[test]
fn dsfix_subject_quality_refresh_is_change_driven() {
    let mut previous = None;
    assert!(subject_quality_changed(&mut previous, "fast"));
    for _ in 0..120 {
        assert!(!subject_quality_changed(&mut previous, "fast"));
    }
    assert!(subject_quality_changed(&mut previous, "better"));
    assert!(!subject_quality_changed(&mut previous, "better"));
    previous = None; // Local AI settings refresh invalidates the cache.
    assert!(subject_quality_changed(&mut previous, "better"));
}

#[cfg(test)]
mod lama_screens {
    use super::*;
    use egui_kittest::Harness;
    #[test]
    #[ignore = "real RTX 5090 offscreen screenshots; run explicitly"]
    fn lama_model_manager_rtx5090_screens() {
        let _gpu_gate = crate::gpu_test_support::lock();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lama/ui");
        std::fs::create_dir_all(&root).unwrap();
        for theme in [crate::theme::ThemeKind::Pro, crate::theme::ThemeKind::StudioLight] {
            for width in [250.0, 400.0] {
                for scale in [1.0, 1.5] {
                    let mut h = Harness::builder()
                        .with_size(vec2(width, 460.0))
                        .with_pixels_per_point(scale)
                        .renderer(crate::prefs_ui::design_system_shots::renderer())
                        .build_ui(|ui| {
                            if !crate::prefs_ui::design_system_shots::fonts_ready(ui) {
                                return;
                            }
                            crate::theme::apply(ui.ctx(), theme);
                            let t = Tokens::get(ui.ctx());
                            model_function_row(
                                ui,
                                &t,
                                &BTreeMap::new(),
                                std::path::Path::new("/nonexistent-lama-screens"),
                                li_seg::Group::Remove,
                                &li_seg::lama::MODEL,
                                true,
                            );
                        });
                    h.run_steps(6);
                    h.render().unwrap().save(root.join(format!("model-{theme:?}-{width}-{scale}.png"))).unwrap();
                }
            }
        }
    }
}
