//! local-image: Local Image's AI in the editor.
//!
//! - **AI Remove Brush** (R): paint over a distraction; on release the stroke is removed by the
//!   local model and the repair lands on its own layer (`ai.remove`).
//! - **AI Cutout** (K): Remove Background makes a layer mask from the AI matte
//!   (`ai.removeBackground`); dragging paints that mask (Erase hides, Restore reveals; X swaps);
//!   Add Background puts a solid colour, an image or a generated plate below the cutout.
//! - The **AI status pill** in the status bar and the **Local AI** window (connection, ComfyUI
//!   installation, start/stop, verified model downloads, GPU).
//! - Menu commands with ids `li.*` (File › New from Prompt, Open Folder, Automate › Remove
//!   Backgrounds, Edit › Generative Fill, Image › AI Enhance, Layer Mask › Generate Background,
//!   Window › Generate, Edit › Preferences › Local AI, Help › AI Models & GPU).
//!
//! The engine commands are in `photocraft_engine::ai_cmds`; the ComfyUI client in `li-ai`.

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
    /// AI Remove engine: `klein`, `qwen-int8` or `qwen-bf16`.
    pub remove_engine: String,
    /// AI Cutout engine: `qwen-int8`, `qwen-bf16` or `quick` (PhotoCraft's CPU subject finder).
    pub cutout_engine: String,
    /// What to keep, passed to the cutout model ("the red car").
    pub cutout_hint: String,
    /// Cutout refine strokes reveal (true) or hide (false) the layer.
    pub cutout_restore: bool,
    /// Last background description (Generate Background).
    pub background_prompt: String,
    /// Last Generative Fill prompt.
    pub fill_prompt: String,
    /// The Generate panel.
    pub generate: crate::generate_ui::GenerateState,
}

impl Default for AiOptions {
    fn default() -> Self {
        Self {
            remove_engine: "klein".into(),
            cutout_engine: "qwen-int8".into(),
            cutout_hint: String::new(),
            cutout_restore: false,
            background_prompt: String::new(),
            fill_prompt: String::new(),
            generate: Default::default(),
        }
    }
}

// ------------------------------------------------------------------------------ engine status

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

struct Shared {
    status: Mutex<EngineStatus>,
    refresh: std::sync::atomic::AtomicBool,
    child: Mutex<Option<std::process::Child>>,
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
            child: Mutex::new(None),
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
    shared().status.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Ask the poller to check again now.
pub fn refresh() {
    shared().refresh.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn poll_loop(s: Arc<Shared>) {
    let mut last_inventory: Option<Instant> = None;
    let mut last = Instant::now() - Duration::from_secs(60);
    loop {
        let forced = s.refresh.swap(false, std::sync::atomic::Ordering::SeqCst);
        let connected_before = s.status.lock().map(|x| x.connected).unwrap_or(false);
        let interval = if connected_before { Duration::from_secs(5) } else { Duration::from_secs(3) };
        if !forced && last.elapsed() < interval {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        }
        last = Instant::now();
        let ai = li_ai::service();
        let host = ai.client.host().to_owned();
        let stats = ai.client.system_stats().ok();
        let connected = stats.is_some();
        let gpu = stats.as_ref().and_then(|st| li_ai::setup::gpus_from_stats(st).into_iter().next());
        let mut presets = s.status.lock().map(|x| x.presets.clone()).unwrap_or_default();
        let mut error = String::new();
        if connected && (forced || !connected_before || last_inventory.is_none_or(|t| t.elapsed() > Duration::from_secs(30))) {
            match ai.client.object_info() {
                Ok(info) => {
                    // Installed models (by header, metadata or name) join the catalogue first.
                    let settings = AiSettings::load();
                    let model_dir = settings.model_dir();
                    li_ai::inventory::refresh(&info, model_dir.is_dir().then_some(model_dir.as_path()));
                    presets = catalog::presets()
                        .iter()
                        .map(|p| {
                            (p.id(), {
                                let a = catalog::availability(p, &info);
                                if a.available { Ok(()) } else { Err(a.reason) }
                            })
                        })
                        .collect();
                    last_inventory = Some(Instant::now());
                }
                Err(e) => error = format!("{e:#}"),
            }
        }
        if !connected {
            presets.clear();
            last_inventory = None;
        }
        let child_alive = s.child.lock().ok().and_then(|mut c| c.as_mut().map(|c| c.try_wait().map(|st| st.is_none()).unwrap_or(false))).unwrap_or(false);
        let changed;
        if let Ok(mut st) = s.status.lock() {
            let new = EngineStatus { checked: true, connected, host, gpu, presets, error, starting: child_alive && !connected };
            changed = st.connected != new.connected || st.presets != new.presets || st.starting != new.starting || !st.checked;
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
    let settings = AiSettings::load();
    let found = li_ai::setup::detect(&settings);
    let inst = found.first().ok_or(tl!("No ComfyUI installation found. Choose its folder in Local AI, or install ComfyUI first."))?;
    let child = li_ai::setup::start(inst, &settings).map_err(|e| format!("{e:#}"))?;
    if let Ok(mut c) = shared().child.lock() {
        *c = Some(child);
    }
    refresh();
    Ok(inst.label())
}

/// Stops ComfyUI if Local Image started it.
pub fn stop_engine() -> bool {
    let stopped = shared().child.lock().ok().and_then(|mut c| c.take()).map(|mut c| c.kill().is_ok() && c.wait().is_ok()).unwrap_or(false);
    refresh();
    stopped
}

/// Downloads a preset's missing files in the background (progress via [`downloads`]).
pub fn start_download(preset_id: &str) {
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

/// Downloads a CPU selection model (U²-Net / IS-Net) into `<model folder>/segmentation/`; keyed
/// `seg:<id>` in [`downloads`].
pub fn start_seg_download(spec: &'static li_seg::ModelSpec) {
    let key = format!("seg:{}", spec.id);
    let ctl = JobControl::new();
    if let Ok(mut d) = shared().downloads.lock() {
        if d.get(&key).is_some_and(|x| !x.finished) {
            return;
        }
        d.insert(key.clone(), Download { total: spec.bytes, file: spec.file.to_owned(), ctl: ctl.clone(), ..Default::default() });
    }
    let _ = std::thread::Builder::new().name("seg-download".into()).spawn(move || {
        let dest = li_seg::model_path(&photocraft_engine::seg::models_dir(), spec);
        let k2 = key.clone();
        let r = li_ai::download::download_file(spec.url, &dest, spec.bytes, spec.sha256, &ctl, &|n| {
            if let Ok(mut d) = shared().downloads.lock()
                && let Some(x) = d.get_mut(&k2)
            {
                x.done = n;
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

/// Local Image › Selection models: rows for the CPU segmentation models.
fn seg_rows(ui: &mut egui::Ui, t: &Tokens, dls: &BTreeMap<String, Download>) {
    let dir = photocraft_engine::seg::models_dir();
    let active = photocraft_engine::seg::installed().map(|s| s.id);
    for spec in li_seg::MODELS {
        let dl = dls.get(&format!("seg:{}", spec.id));
        let frame = egui::Frame::NONE.fill(t.field).corner_radius(t.radius).inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(spec.label).strong());
                    ui.label(
                        RichText::new(crate::i18n::fmt(
                            tl!("{size} · runs on the CPU · {licence}"),
                            &[("size", &li_ai::download::human_bytes(spec.bytes)), ("licence", spec.licence)],
                        ))
                        .color(t.text_dim)
                        .size(11.5),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match dl {
                    Some(d) if !d.finished => {
                        if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                            d.ctl.cancel();
                        }
                        let frac = if d.total > 0 { d.done as f32 / d.total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac).desired_width(140.0).text(format!("{:.0}%", frac * 100.0)));
                    }
                    _ if li_seg::model_path(&dir, spec).is_file() => {
                        let text = if active == Some(spec.id) { tl!("In use") } else { tl!("Installed") };
                        ui.label(RichText::new(text).color(Color32::from_rgb(70, 190, 110)));
                    }
                    _ => {
                        if widgets::secondary_button(ui, tl!("Download"), 0.0).clicked() {
                            start_seg_download(spec);
                        }
                    }
                });
            });
            if let Some(err) = dl.and_then(|d| d.error.as_ref()) {
                ui.label(RichText::new(err).color(t.danger).size(11.5));
            }
        });
        ui.add_space(4.0);
    }
}

pub fn downloads() -> BTreeMap<String, Download> {
    shared().downloads.lock().map(|d| d.clone()).unwrap_or_default()
}

// ------------------------------------------------------------------------------ tools

/// Finish a stroke with an AI tool. Returns false for other tools.
pub fn finish_stroke(app: &mut PhotocraftApp, tool: Tool, points: &[[f64; 3]]) -> bool {
    match tool {
        Tool::AiRemove => {
            let b = &app.session.tools.brush;
            let p = json!({ "points": points, "size": b.size, "hardness": b.hardness * 100.0, "engine": app.ui.ai.remove_engine });
            let r = app.run("ai.remove", p);
            report(app, r);
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
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn opt(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_dim));
}

const REMOVE_ENGINES: [(&str, &str, ModelId, &str); 3] = [
    ("klein", "FLUX.2 Klein", ModelId::KleinRemove, "bf16"),
    ("qwen-int8", "Qwen Compact", ModelId::Qwen, "int8"),
    ("qwen-bf16", "Qwen Full", ModelId::Qwen, "bf16"),
];
/// Qwen-powered removal (a real alpha matte, on the GPU through ComfyUI) or the editor's own
/// Remove Background (Select Subject plus edge refinement, on the CPU, no AI engine needed).
const CUTOUT_ENGINES: [(&str, &str); 3] = [("qwen-int8", "Qwen AI · Compact"), ("qwen-bf16", "Qwen AI · Full"), ("quick", "Standard (CPU)")];

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
fn readiness(app: &mut PhotocraftApp, ui: &mut egui::Ui, model: ModelId, variant: &str) {
    let st = status();
    if let Err(why) = st.ready(model, variant) {
        let t = Tokens::get(ui.ctx());
        widgets::vline(ui, 22.0);
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        crate::icons::paint(ui, r, "triangle-alert", 14.0, t.warning);
        ui.label(RichText::new(why).color(t.warning));
        let label = if st.connected { tl!("Get models…") } else { tl!("Set up AI…") };
        if widgets::secondary_button(ui, label, 0.0).clicked() {
            open_local_ai(app);
        }
    }
}

/// Options bar for the AI tools. Returns false for other tools.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) -> bool {
    match tool {
        Tool::AiRemove => {
            opt(ui, tl!("Engine:"));
            let opts: Vec<(String, &str)> = REMOVE_ENGINES.iter().map(|(k, l, _, _)| ((*k).to_owned(), *l)).collect();
            widgets::dropdown(ui, "ai-remove-engine", &mut app.ui.ai.remove_engine, &opts, 130.0);
            let has_sel = app.session.active().is_some_and(|d| d.doc.selection.is_some());
            if has_sel {
                widgets::vline(ui, 22.0);
                if widgets::secondary_button(ui, tl!("Remove Selection"), 0.0).clicked() {
                    let e = app.ui.ai.remove_engine.clone();
                    let r = app.run("ai.remove", json!({ "engine": e }));
                    report(app, r);
                }
            }
            let (m, v) = engine_for(&app.ui.ai.remove_engine);
            if status().ready(m, v).is_ok() {
                widgets::vline(ui, 22.0);
                opt(ui, tl!("Paint over a distraction; it's removed when you release"));
            } else {
                readiness(app, ui, m, v);
            }
            true
        }
        Tool::AiCutout => {
            let quick = app.ui.ai.cutout_engine == "quick";
            if widgets::primary_button(ui, tl!("Remove Background"), 0.0).clicked() {
                let r = if quick {
                    app.run("layer.removeBackground", json!({}))
                } else {
                    let p = json!({ "engine": app.ui.ai.cutout_engine, "hint": app.ui.ai.cutout_hint });
                    app.run("ai.removeBackground", p)
                };
                report(app, r);
            }
            let opts: Vec<(String, &str)> = CUTOUT_ENGINES.iter().map(|(k, l)| ((*k).to_owned(), *l)).collect();
            widgets::dropdown(ui, "ai-cutout-engine", &mut app.ui.ai.cutout_engine, &opts, 140.0);
            if !quick {
                ui.add(egui::TextEdit::singleline(&mut app.ui.ai.cutout_hint).hint_text(tl!("Keep… (optional)")).desired_width(120.0));
            }
            widgets::vline(ui, 22.0);
            opt(ui, tl!("Refine:"));
            let mut erase = !app.ui.ai.cutout_restore;
            if widgets::checkbox(ui, &mut erase, tl!("Erase")).clicked() {
                app.ui.ai.cutout_restore = false;
            }
            let mut restore = app.ui.ai.cutout_restore;
            if widgets::checkbox(ui, &mut restore, crate::i18n::tr_ctx(crate::i18n::current(), "ai", "Restore")).clicked() {
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
            if !quick {
                let v = if app.ui.ai.cutout_engine == "qwen-bf16" { "bf16" } else { "int8" };
                readiness(app, ui, ModelId::Qwen, v);
            }
            true
        }
        _ => false,
    }
}

// ------------------------------------------------------------------------------ status pill

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
        (Color32::from_rgb(70, 190, 110), crate::i18n::fmt(tl!("AI ready{gpu}"), &[("gpu", &gpu.unwrap_or_default())]))
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
        crate::i18n::fmt(tl!("ComfyUI at {host} — click for Local AI settings"), &[("host", &st.host)])
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

fn short_gpu(name: &str) -> String {
    name.replace("NVIDIA ", "").replace("GeForce ", "").replace("Laptop GPU", "Laptop")
}

// ------------------------------------------------------------------------------ dialogs

#[derive(Default)]
struct Dialogs {
    local_ai: bool,
    background: bool,
    fill: bool,
    enhance: bool,
    enhance_scale: f32,
    detected: Option<Vec<li_ai::setup::Installation>>,
    settings: Option<AiSettings>,
    message: String,
}

fn dialogs_mut<R>(f: impl FnOnce(&mut Dialogs) -> R) -> R {
    thread_local! { static D: std::cell::RefCell<Dialogs> = std::cell::RefCell::new(Dialogs { enhance_scale: 2.0, ..Default::default() }); }
    D.with(|d| f(&mut d.borrow_mut()))
}

pub fn open_local_ai(_app: &mut PhotocraftApp) {
    refresh();
    dialogs_mut(|d| {
        d.local_ai = true;
        d.settings = Some(AiSettings::load());
    });
}

/// Floating windows: Local AI, Generate Background, Generative Fill, AI Enhance, batch.
pub fn windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if dialogs_mut(|d| d.local_ai) {
        local_ai_window(app, ctx);
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
    crate::generate_ui::batch_window(app, ctx);
    crate::model_browser::window(app, ctx);
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
                ui.label(RichText::new(crate::i18n::fmt(tl!("{why} Qwen Image 2.1 runs these."), &[("why", &why)])).color(Tokens::get(ctx).warning));
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
                    let r = app.run(cmd, p);
                    report(app, r);
                    close = true;
                }
                if widgets::secondary_button(ui, tl!("Cancel"), 80.0).clicked() {
                    close = true;
                }
            });
        });
    if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        dialogs_mut(|d| if background { d.background = false } else { d.fill = false });
    }
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
            ui.label(tl!("Enhances detail and enlarges the image with SeedVR2. The result opens as a new document."));
            ui.add_space(8.0);
            let mut scale = dialogs_mut(|d| d.enhance_scale);
            widgets::slider_row(ui, tl!("Scale"), &mut scale, 1.25..=4.0, "×", None);
            dialogs_mut(|d| d.enhance_scale = scale);
            if let Some((w, h)) = size {
                let (nw, nh) = (((w as f32 * scale) as u32) & !1, ((h as f32 * scale) as u32) & !1);
                ui.label(RichText::new(format!("{w} × {h}  →  {nw} × {nh} px")).color(Tokens::get(ctx).text_dim));
            }
            if let Err(why) = status().ready(ModelId::SeedVr2, "fp16") {
                ui.label(RichText::new(why).color(Tokens::get(ctx).warning));
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary_button(ui, tl!("Enhance"), 96.0).clicked() {
                    let r = app.run("ai.enhance", json!({ "scale": scale }));
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

fn local_ai_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let t = Tokens::get(ctx);
    let st = status();
    let mut open = true;
    egui::Window::new(tl!("Local AI"))
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(640.0, 560.0))
        .frame(window_frame(ctx))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(ctx.content_rect().height() - 160.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Status.
                ui.horizontal(|ui| {
                    let (dot, text) = if st.connected {
                        (Color32::from_rgb(70, 190, 110), crate::i18n::fmt(tl!("Connected to ComfyUI at {host}"), &[("host", &st.host)]))
                    } else if st.starting {
                        (t.warning, tl!("Starting ComfyUI… (the first start can take a minute)").to_owned())
                    } else {
                        (t.text_faint, crate::i18n::fmt(tl!("ComfyUI is not running at {host}"), &[("host", &st.host)]))
                    };
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.5, dot);
                    ui.label(RichText::new(text).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::icons::button(ui, "rotate-cw", 24.0, false, tl!("Check again")).clicked() {
                            refresh();
                        }
                    });
                });
                if let Some(g) = &st.gpu {
                    ui.label(
                        RichText::new(crate::i18n::fmt(
                            tl!("{name} · {gb} GB GPU memory"),
                            &[("name", &g.name), ("gb", &format!("{:.0}", g.vram_total as f64 / 1e9))],
                        ))
                        .color(t.text_dim),
                    );
                }
                ui.add_space(10.0);
                widgets::section_label(ui, tl!("AI ENGINE"));
                let mut settings = dialogs_mut(|d| d.settings.clone()).unwrap_or_else(AiSettings::load);
                let before = settings.clone();
                ui.horizontal(|ui| {
                    opt(ui, tl!("Address:"));
                    ui.add(egui::TextEdit::singleline(&mut settings.comfy_host).desired_width(120.0));
                    opt(ui, tl!("Port:"));
                    let mut port = settings.comfy_port as f32;
                    if widgets::value_field(ui, &mut port, 1.0..=65535.0, "", 70.0).changed() {
                        settings.comfy_port = port as u16;
                    }
                });
                ui.horizontal(|ui| {
                    opt(ui, tl!("Installation:"));
                    let label =
                        if settings.comfy_directory.is_empty() { tl!("Not chosen — detected automatically").to_owned() } else { settings.comfy_directory.clone() };
                    ui.label(RichText::new(label).color(t.text_dim));
                });
                ui.horizontal(|ui| {
                    if widgets::secondary_button(ui, tl!("Detect"), 0.0).clicked() {
                        let found = li_ai::setup::detect(&settings);
                        dialogs_mut(|d| {
                            d.message = if found.is_empty() {
                                tl!("No ComfyUI installation found in the usual places.").into()
                            } else {
                                crate::i18n::fmt(tl!("Found {n} installation(s)."), &[("n", &found.len().to_string())])
                            };
                            d.detected = Some(found);
                        });
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if widgets::secondary_button(ui, tl!("Choose Folder…"), 0.0).clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_title(tl!("Choose your ComfyUI folder")).pick_folder()
                    {
                        if li_ai::setup::installation(&dir, None).is_some() {
                            settings.comfy_directory = dir.display().to_string();
                            dialogs_mut(|d| d.message = tl!("ComfyUI installation found.").into());
                        } else {
                            dialogs_mut(|d| d.message = tl!("That folder has no ComfyUI (main.py and a Python environment).").into());
                        }
                    }
                    let running_ours = shared().child.lock().map(|c| c.is_some()).unwrap_or(false);
                    if !st.connected && !st.starting && widgets::primary_button(ui, tl!("Start AI Engine"), 0.0).clicked() {
                        let _ = settings.save();
                        let m = match start_engine() {
                            Ok(l) => crate::i18n::fmt(tl!("Starting {name}…"), &[("name", &l)]),
                            Err(e) => e,
                        };
                        dialogs_mut(|d| d.message = m);
                    }
                    if running_ours && widgets::secondary_button(ui, tl!("Stop"), 0.0).clicked() {
                        stop_engine();
                    }
                    if st.connected && widgets::secondary_button(ui, tl!("Free GPU Memory"), 0.0).clicked() {
                        let m = match li_ai::service().client.free_memory() {
                            Ok(()) => tl!("Models unloaded.").to_owned(),
                            Err(e) => format!("{e:#}"),
                        };
                        dialogs_mut(|d| d.message = m);
                    }
                });
                if let Some(found) = dialogs_mut(|d| d.detected.clone()) {
                    for inst in found {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(inst.label()).color(t.text_dim));
                            if widgets::secondary_button(ui, tl!("Use"), 0.0).clicked() {
                                settings.comfy_directory = inst.code.display().to_string();
                                settings.comfy_python = inst.python.display().to_string();
                            }
                        });
                    }
                }
                let msg = dialogs_mut(|d| d.message.clone());
                if !msg.is_empty() {
                    ui.label(RichText::new(msg).color(t.text_dim));
                }
                ui.add_space(10.0);
                widgets::section_label(ui, tl!("MODELS"));
                ui.horizontal(|ui| {
                    opt(ui, tl!("Model folder:"));
                    ui.label(RichText::new(settings.model_dir().display().to_string()).color(t.text_dim));
                    #[cfg(not(target_arch = "wasm32"))]
                    if widgets::secondary_button(ui, tl!("Change…"), 0.0).clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_title(tl!("Choose the model folder")).pick_folder()
                    {
                        settings.model_directory = dir.display().to_string();
                    }
                });
                ui.label(
                    RichText::new(tl!("Downloads are checked against the publisher's size and SHA-256, and ComfyUI's model list is refreshed after each install."))
                        .color(t.text_faint)
                        .size(11.0),
                );
                ui.add_space(4.0);
                let dls = downloads();
                // The curated models (the Model Browser has everything else).
                for m in ModelId::all() {
                    let info = m.info();
                    if info.origin != catalog::Origin::Profile || !(info.starter || info.tool || ModelId::ORIGINAL.contains(&m)) {
                        continue;
                    }
                    for p in catalog::presets_for(m) {
                        model_row(ui, &t, info, p, &st, dls.get(&p.id()), &settings);
                    }
                }
                if widgets::secondary_button(ui, tl!("Browse Models…"), 0.0).on_hover_text(tl!("Find, compare and install models and LoRAs for any family")).clicked() {
                    crate::model_browser::open();
                }
                ui.add_space(8.0);
                widgets::section_label(ui, tl!("CLOUD (OPTIONAL)"));
                ui.label(
                    RichText::new(tl!("With your own API key, Generate can also use cloud models (they appear under Cloud in the model picker). Your prompt and images then go from this computer to that provider and are billed to your account; nothing else is sent anywhere."))
                        .color(t.text_faint)
                        .size(11.0),
                );
                for p in li_ai::cloud::Provider::ALL {
                    ui.horizontal(|ui| {
                        opt(ui, &format!("{}:", p.label()));
                        let mut v = settings.extra_string(p.key_name()).unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut v).password(true).hint_text(tl!("API key")).desired_width(220.0)).changed() {
                            settings.set_extra_string(p.key_name(), &v);
                        }
                        if ui.link(RichText::new(tl!("Get a key")).size(11.0)).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(p.keys_url()));
                        }
                    });
                }
                ui.add_space(8.0);
                widgets::section_label(ui, tl!("ACCOUNTS (OPTIONAL)"));
                ui.label(
                    RichText::new(tl!("Tokens let the Model Browser download gated or sign-in-only models. They're sent only to their own site."))
                        .color(t.text_faint)
                        .size(11.0),
                );
                for (key, label, hint) in [("hf_token", tl!("Hugging Face token:"), "hf_…"), ("civitai_token", tl!("Civitai API key:"), "")] {
                    ui.horizontal(|ui| {
                        opt(ui, label);
                        let mut v = settings.extra_string(key).unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut v).password(true).hint_text(hint).desired_width(260.0)).changed() {
                            settings.set_extra_string(key, &v);
                        }
                    });
                }
                ui.add_space(8.0);
                widgets::section_label(ui, tl!("SELECTION MODELS (CPU)"));
                ui.label(
                    RichText::new(
                        tl!("Select Subject, Remove Background (Quick) and Object Selection clicks use the best one installed. No GPU or ComfyUI needed."),
                    )
                    .color(t.text_faint)
                    .size(11.0),
                );
                ui.add_space(4.0);
                seg_rows(ui, &t, &dls);
                if settings != before {
                    if let Err(e) = settings.save() {
                        dialogs_mut(|d| d.message = crate::i18n::fmt(tl!("Could not save settings: {error}"), &[("error", &format!("{e:#}"))]));
                    }
                    refresh();
                }
                dialogs_mut(|d| d.settings = Some(settings));
                ui.add_space(8.0);
                widgets::section_label(ui, tl!("GPU MEMORY GUIDE"));
                for (what, gb) in catalog::VRAM_GUIDE {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{gb} GB")).font(crate::theme::mono(12.0)));
                        ui.label(RichText::new(*what).color(t.text_dim));
                    });
                }
            });
        });
    if !open {
        dialogs_mut(|d| d.local_ai = false);
    }
    if dls_running() {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    let _ = app;
}

fn dls_running() -> bool {
    downloads().values().any(|d| !d.finished)
}

fn model_row(ui: &mut egui::Ui, t: &Tokens, info: &catalog::ModelInfo, p: &catalog::Preset, st: &EngineStatus, dl: Option<&Download>, settings: &AiSettings) {
    let frame = egui::Frame::NONE.fill(t.field).corner_radius(t.radius).inner_margin(egui::Margin::symmetric(10, 8));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{} · {}", info.label, p.label)).strong());
                ui.label(
                    RichText::new(crate::i18n::fmt(
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
                match (dl, state) {
                    (Some(d), _) if !d.finished => {
                        if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                            d.ctl.cancel();
                        }
                        let frac = if d.total > 0 { d.done as f32 / d.total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac).desired_width(140.0).text(format!("{:.0}%", frac * 100.0)));
                    }
                    (_, Some(Ok(()))) => {
                        ui.label(RichText::new(tl!("Ready")).color(Color32::from_rgb(70, 190, 110)));
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

// ------------------------------------------------------------------------------ menus

const IDS: &[&str] = &[
    "li.newFromPrompt",
    "li.openFolder",
    "li.batchRemoveBackgrounds",
    "li.generativeFill",
    "li.enhance",
    "li.generateBackground",
    "li.panel.generate",
    "li.localAi",
    "li.aiModels",
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
        "li.generativeFill" => app.session.active().is_some_and(|d| d.doc.selection.is_some()),
        "li.enhance" | "li.generateBackground" => has_doc,
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
        "li.batchRemoveBackgrounds" => crate::generate_ui::open_batch(),
        "li.generativeFill" => dialogs_mut(|d| d.fill = true),
        "li.generateBackground" => dialogs_mut(|d| d.background = true),
        "li.enhance" => dialogs_mut(|d| d.enhance = true),
        "li.localAi" | "li.aiModels" => open_local_ai(app),
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
