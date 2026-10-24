//! Set Up Models (Window › Set Up Models…): what every main model file on the connected ComfyUI
//! is, whether it runs and why not, with fixes, and small test renders on request. The work is
//! `li_ai::modelsetup`'s (scan, choices, moves, downloads, verification); this window runs it on
//! one worker thread at a time and renders the stored result. Nothing renders, downloads or moves
//! without a click.

mod detail;
mod dialogs;
mod list;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{RichText, vec2};
use li_ai::JobControl;
use li_ai::modelsetup::fixes::{Choice, Moved};
use li_ai::modelsetup::issue::{Fix, Issue};
use li_ai::modelsetup::state::{CapabilityResult, ModelState, Status};
use li_ai::modelsetup::{self, Scan};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets::{self, Tone};

/// The steps of one model's page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Step {
    #[default]
    Identify,
    Check,
    Verify,
    Use,
}

impl Step {
    const ALL: [Step; 4] = [Step::Identify, Step::Check, Step::Verify, Step::Use];

    fn label(self) -> &'static str {
        match self {
            Self::Identify => tl!("1 Identify"),
            Self::Check => tl!("2 Check"),
            Self::Verify => tl!("3 Verify"),
            Self::Use => tl!("4 Use"),
        }
    }
}

/// What the person asked for, run by [`dispatch`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    Recheck,
    Verify(Vec<String>),
    Cancel,
    Choose(String, Choice),
    Move { file: String, to_folder: String },
    Undo,
    PlanDownload(Fix),
    Download(li_ai::browser::Plan),
    OpenUrl(String),
    Copy(String),
    CopyReport,
    AddToken,
    OpenModelBrowser,
    StartEngine,
    UseInGenerate(String),
}

/// What a background task produced.
enum Done {
    Scanned(Box<Scan>),
    /// A download plan waiting for the person's go-ahead.
    Planned(li_ai::browser::Plan),
    /// A file moved (kept for Undo); the scan after it.
    Moved(Moved, Box<Scan>),
    /// Models that could not be tested and why; the scan after it.
    Verified(Vec<String>, Box<Scan>),
}

/// The one background task (scan, fix, download or verification) and what it reports.
#[derive(Default)]
struct Task {
    label: String,
    /// Bytes done and total while downloading.
    progress: Option<(u64, u64)>,
    /// The models a verification tests, the one in progress and each finished capability.
    verifying: Vec<String>,
    current: Option<String>,
    steps: Vec<(String, String, CapabilityResult)>,
    ctl: JobControl,
    result: Option<Result<Done, String>>,
}

/// A Move fix waiting for confirmation.
#[derive(Clone, Debug, PartialEq)]
struct MoveAsk {
    file: String,
    to_folder: String,
    kind: String,
}

/// The template chooser for one model.
#[derive(Clone, Debug, Default)]
struct ChooseTemplate {
    key: String,
    search: String,
}

#[derive(Default)]
struct State {
    open: bool,
    selected: Option<String>,
    step: Step,
    scan: Option<Arc<Scan>>,
    task: Option<Arc<Mutex<Task>>>,
    /// The last task's outcome or error, shown under the header.
    message: Option<(String, bool)>,
    undo: Option<(Instant, Moved)>,
    confirm_move: Option<MoveAsk>,
    confirm_download: Option<li_ai::browser::Plan>,
    choose_template: Option<ChooseTemplate>,
    show_other: bool,
    show_ignored: bool,
    /// The new-model count last seen (a rise during the session posts one notice).
    seen_new: Option<usize>,
    /// Tests: actions are recorded here instead of run.
    #[cfg(test)]
    recorded: Option<Vec<Action>>,
}

impl State {
    fn model(&self, key: &str) -> Option<&ModelState> {
        self.scan.as_ref()?.server.models.get(key)
    }

    fn busy(&self) -> bool {
        self.task.is_some()
    }

    fn set_scan(&mut self, scan: Scan) {
        let keys = list::order(&scan);
        if self.selected.as_ref().is_none_or(|k| !scan.server.models.contains_key(k)) {
            self.selected = keys.first().cloned();
        }
        self.scan = Some(Arc::new(scan));
    }
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    thread_local! { static S: std::cell::RefCell<State> = std::cell::RefCell::new(State::default()); }
    S.with(|s| f(&mut s.borrow_mut()))
}

static CTX: Mutex<Option<egui::Context>> = Mutex::new(None);

fn repaint() {
    if let Some(c) = CTX.lock().ok().and_then(|c| c.clone()) {
        c.request_repaint();
    }
}

/// Opens the window (on `model`, a catalogue or setup key, when given) and checks the models.
pub fn open(model: Option<&str>) {
    with(|s| {
        s.open = true;
        if let Some(m) = model {
            s.selected = Some(m.to_owned());
            s.step = Step::Identify;
        }
        if !s.busy() {
            run(s, Action::Recheck);
        }
    });
    repaint();
}

pub fn is_open() -> bool {
    with(|s| s.open)
}

/// Tests: the window records what it would do instead of scanning or downloading.
#[cfg(test)]
pub(crate) fn record_actions() {
    with(|s| *s = State { recorded: Some(Vec::new()), ..Default::default() });
}

/// Tests: the model the window shows.
#[cfg(test)]
pub(crate) fn selected() -> Option<String> {
    with(|s| s.selected.clone())
}

/// The scan, choices and caches for the configured ComfyUI.
fn options() -> modelsetup::Options {
    let host = li_ai::service().client.host().to_owned();
    let model_dir = modelsetup::is_local(&host).then(|| li_ai::AiSettings::load().model_dir());
    modelsetup::Options::for_host(&host, model_dir)
}

/// Starts `work` as the window's background task.
fn start(s: &mut State, label: &str, verifying: Vec<String>, work: impl FnOnce(&Mutex<Task>) -> anyhow::Result<Done> + Send + 'static) {
    let current = verifying.first().cloned();
    let task = Arc::new(Mutex::new(Task { label: label.to_owned(), verifying, current, ..Default::default() }));
    let t = task.clone();
    let lang = li_i18n::current();
    let spawned = std::thread::Builder::new().name("model-setup".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        li_i18n::set_current(lang);
        let result = work(&t).map_err(|e| format!("{e:#}"));
        if let Ok(mut x) = t.lock() {
            x.result = Some(result);
        }
        crate::ai_ui::refresh();
        repaint();
    });
    match spawned {
        Ok(_) => {
            s.task = Some(task);
            s.message = None;
        }
        Err(e) => s.message = Some((e.to_string(), true)),
    }
}

/// Runs an action that needs no app state.
fn run(s: &mut State, a: Action) {
    #[cfg(test)]
    if let Some(r) = s.recorded.as_mut() {
        r.push(a);
        return;
    }
    let opts = options();
    match a {
        Action::Recheck => start(s, tl!("Checking models…"), Vec::new(), move |_| Ok(Done::Scanned(Box::new(modelsetup::scan(&opts))))),
        Action::Verify(keys) => {
            let Some(scan) = s.scan.clone() else { return };
            let label = if keys.len() == 1 { tl!("Testing…") } else { tl!("Testing one model at a time…") };
            start(s, label, keys.clone(), move |t| {
                let ctl = t.lock().map(|x| x.ctl.clone()).unwrap_or_default();
                let mut progress = |key: &str, step: modelsetup::verify::Step| {
                    if let Ok(mut x) = t.lock() {
                        x.current = Some(key.to_owned());
                        x.steps.push((key.to_owned(), step.capability.to_owned(), step.result.clone()));
                    }
                    repaint();
                };
                let problems = match modelsetup::verify_models(&opts, &scan, &keys, &ctl, &mut progress) {
                    Ok(done) => done.into_iter().filter_map(|v| v.result.err().map(|e| format!("{}: {e}", v.label))).collect(),
                    Err(_) if ctl.is_cancelled() => Vec::new(),
                    Err(e) => vec![format!("{e:#}")],
                };
                Ok(Done::Verified(problems, Box::new(modelsetup::scan(&opts))))
            });
        }
        Action::Cancel => {
            if let Some(t) = &s.task
                && let Ok(t) = t.lock()
            {
                t.ctl.cancel();
            }
        }
        Action::Choose(key, choice) => start(s, tl!("Checking models…"), Vec::new(), move |_| {
            modelsetup::fixes::choose(&opts.state_path, &opts.host, &key, choice)?;
            Ok(Done::Scanned(Box::new(modelsetup::scan(&opts))))
        }),
        Action::Move { file, to_folder } => start(s, tl!("Moving the file…"), Vec::new(), move |_| {
            let dir = opts.model_dir.clone().ok_or_else(|| anyhow::anyhow!("{}", tl!("The model folder is on another computer; move the file there.")))?;
            let moved = modelsetup::fixes::move_file(&dir, &file, &to_folder)?;
            Ok(Done::Moved(moved, Box::new(modelsetup::scan(&opts))))
        }),
        Action::Undo => {
            if let Some((_, moved)) = s.undo.take() {
                start(s, tl!("Moving the file back…"), Vec::new(), move |_| {
                    modelsetup::fixes::undo_move(&moved)?;
                    Ok(Done::Scanned(Box::new(modelsetup::scan(&opts))))
                });
            }
        }
        Action::PlanDownload(fix) => start(s, tl!("Looking up the download…"), Vec::new(), move |_| {
            let cfg = crate::model_browser::config();
            let net = li_ai::browser::HttpNet::new(cfg.clone());
            let info = li_ai::service().client.object_info()?;
            Ok(Done::Planned(modelsetup::fixes::download_plan(&fix, &info, &cfg, &net)?))
        }),
        Action::Download(plan) => start(s, tl!("Downloading…"), Vec::new(), move |t| {
            let dir = opts.model_dir.clone().ok_or_else(|| anyhow::anyhow!("{}", tl!("The model folder is on another computer; install the file there.")))?;
            let ctl = t.lock().map(|x| x.ctl.clone()).unwrap_or_default();
            let progress = |done: u64, total: u64, _: &str| {
                if let Ok(mut x) = t.lock() {
                    x.progress = Some((done, total));
                }
                repaint();
            };
            li_ai::browser::install(&plan, &dir, &crate::model_browser::config(), &ctl, &progress)?;
            // ComfyUI lists a new file on its next folder scan; give it a moment.
            std::thread::sleep(Duration::from_secs(2));
            Ok(Done::Scanned(Box::new(modelsetup::scan(&opts))))
        }),
        other => log::warn!("model setup: {other:?} needs the app"),
    }
}

/// Runs any action (those that open other windows need the app).
fn dispatch(app: &mut PhotocraftApp, ctx: &egui::Context, a: Action) {
    #[cfg(test)]
    if with(|s| s.recorded.as_mut().map(|r| r.push(a.clone())).is_some()) {
        return;
    }
    match a {
        Action::OpenUrl(url) => {
            crate::links::open(app, ctx, &url);
        }
        Action::Copy(text) => {
            ctx.copy_text(text);
            with(|s| s.message = Some((tl!("Copied.").into(), false)));
        }
        Action::CopyReport => {
            if let Some(scan) = with(|s| s.scan.clone()) {
                ctx.copy_text(modelsetup::report::render_text(&modelsetup::report(&options(), &scan)));
                with(|s| s.message = Some((tl!("Copied the report.").into(), false)));
            }
        }
        Action::AddToken => {
            crate::prefs_ui::open_preferences(app, "ai.providers");
        }
        Action::OpenModelBrowser => crate::model_browser::open(),
        Action::StartEngine => {
            let m = match crate::ai_ui::start_engine() {
                Ok(l) => (li_i18n::fmt(tl!("Starting {name}…"), &[("name", &l)]), false),
                Err(e) => (e, true),
            };
            with(|s| s.message = Some(m));
        }
        Action::UseInGenerate(key) => {
            crate::generate_ui::use_model(app, ctx, &key);
            with(|s| s.open = false);
        }
        other => with(|s| run(s, other)),
    }
}

/// Collects a finished task.
fn collect(s: &mut State) {
    let Some(task) = s.task.clone() else { return };
    let Some(result) = task.lock().ok().and_then(|mut t| t.result.take()) else { return };
    s.task = None;
    match result {
        Ok(Done::Scanned(scan)) => s.set_scan(*scan),
        Ok(Done::Planned(plan)) if plan.files.is_empty() => s.message = Some((tl!("ComfyUI already has these files. Press Re-check.").into(), false)),
        Ok(Done::Planned(plan)) => s.confirm_download = Some(plan),
        Ok(Done::Moved(moved, scan)) => {
            s.undo = Some((Instant::now(), moved));
            s.set_scan(*scan);
        }
        Ok(Done::Verified(problems, scan)) => {
            s.set_scan(*scan);
            if !problems.is_empty() {
                s.message = Some((problems.join("\n"), true));
            }
        }
        Err(e) => s.message = Some((e, true)),
    }
}

/// One notice when the number of new models rises during the session (not at start-up: the
/// Generate banner says so then).
fn notice_new_models(app: &mut PhotocraftApp) {
    let st = crate::ai_ui::status();
    if !st.connected {
        return;
    }
    let n = st.setup.new.len();
    let before = with(|s| s.seen_new.replace(n));
    if before.is_some_and(|b| n > b) {
        crate::notices::post(
            app,
            li_i18n::fmt(tl!("{n} new models found"), &[("n", &n.to_string())]),
            vec![tl!("Window › Set Up Models… checks and tests them.").into()],
            false,
            None,
        );
    }
}

/// The issue's sentence in the UI language.
pub(crate) fn sentence(i: &Issue) -> String {
    modelsetup::issue::fill(li_i18n::t(i.template()), &i.args)
}

/// How a status reads: chip icon, text and tone.
pub(crate) fn chip(status: Status) -> (&'static str, &'static str, Tone) {
    match status {
        Status::New | Status::Identified => ("circle-dot", tl!("New"), Tone::Neutral),
        Status::Ready => ("circle-dashed", tl!("Not tested"), Tone::Neutral),
        Status::Verifying => ("refresh-cw", tl!("Testing…"), Tone::Neutral),
        Status::Verified => ("check", tl!("Verified"), Tone::Good),
        Status::PartiallyVerified => ("triangle-alert", tl!("Partly verified"), Tone::Warning),
        Status::NeedsReverify => ("refresh-cw", tl!("Re-check"), Tone::Warning),
        Status::Blocked => ("triangle-alert", tl!("Needs attention"), Tone::Warning),
        Status::Unmatched => ("triangle-alert", tl!("No template"), Tone::Warning),
        Status::Failed => ("x", tl!("Failed"), Tone::Danger),
        Status::Broken => ("x", tl!("Broken"), Tone::Danger),
        Status::Builtin => ("check", tl!("Built-in"), Tone::Good),
        Status::Ignored | Status::Removed => ("ban", tl!("Ignored"), Tone::Neutral),
    }
}

/// The status in words, for screen readers.
fn spoken(status: Status) -> &'static str {
    match status {
        Status::New | Status::Identified => tl!("new, not set up"),
        Status::Ready => tl!("ready, not tested"),
        Status::Verifying => tl!("testing"),
        Status::Verified => tl!("verified"),
        Status::PartiallyVerified => tl!("partly verified"),
        Status::NeedsReverify => tl!("needs a re-check"),
        Status::Blocked => tl!("needs attention"),
        Status::Unmatched => tl!("no matching template"),
        Status::Failed => tl!("test failed"),
        Status::Broken => tl!("stopped working after an update"),
        Status::Builtin => tl!("built-in support"),
        Status::Ignored | Status::Removed => tl!("ignored"),
    }
}

/// The window (call every frame).
pub fn window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if let Ok(mut c) = CTX.lock()
        && c.is_none()
    {
        *c = Some(ctx.clone());
    }
    notice_new_models(app);
    if !is_open() {
        return;
    }
    with(collect);
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut actions = Vec::new();
    let screen = ctx.content_rect();
    let size = vec2((screen.width() - 160.0).clamp(720.0, 1100.0), (screen.height() - 160.0).clamp(480.0, 720.0));
    let shown = egui::Window::new(tl!("Set Up Models"))
        .id(egui::Id::new("model-setup-window"))
        .collapsible(false)
        .resizable(true)
        .default_size(size)
        .min_size(vec2(720.0, 480.0))
        .default_pos(screen.center() - size / 2.0)
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(egui::Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(12)))
        .open(&mut open)
        .show(ctx, |ui| with(|s| content(ui, s, &t, &mut actions)));
    with(|s| dialogs::show(ctx, s, &t, &mut actions));
    let on_top = shown.as_ref().is_some_and(|w| ctx.top_layer_id() == Some(w.response.layer_id));
    if on_top && !with(|s| dialogs::any_open(s)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        open = false;
    }
    for a in actions {
        dispatch(app, ctx, a);
    }
    if !open {
        with(|s| {
            s.open = false;
            s.confirm_move = None;
            s.confirm_download = None;
            s.choose_template = None;
        });
    }
    if with(|s| s.busy() || s.undo.is_some()) {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn content(ui: &mut egui::Ui, s: &mut State, t: &Tokens, out: &mut Vec<Action>) {
    header(ui, s, t, out);
    ui.add_space(4.0);
    widgets::hairline(ui);
    let footer_h = 36.0;
    let body_h = (ui.available_height() - footer_h - 8.0).max(240.0);
    ui.horizontal_top(|ui| {
        let list_w = (ui.available_width() * 0.3).clamp(220.0, 300.0);
        ui.allocate_ui_with_layout(vec2(list_w, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical().id_salt("model-setup-list").auto_shrink([false, false]).show(ui, |ui| list::show(ui, s, t, out));
        });
        widgets::vline(ui, body_h);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical().id_salt("model-setup-detail").auto_shrink([false, false]).show(ui, |ui| detail::show(ui, s, t, out));
        });
    });
    widgets::hairline(ui);
    footer(ui, s, out);
}

/// Server line, banners, the running task and the last message.
fn header(ui: &mut egui::Ui, s: &mut State, t: &Tokens, out: &mut Vec<Action>) {
    let st = crate::ai_ui::status();
    let snapshot = s.scan.as_ref().and_then(|x| x.snapshot.clone());
    ui.horizontal(|ui| {
        let line = match &snapshot {
            Some(sn) => {
                let mut parts = vec![li_i18n::fmt(tl!("ComfyUI {version} at {host}"), &[("version", &sn.version_label()), ("host", &sn.host)])];
                if let Some(g) = &sn.gpu {
                    parts.push(format!("{}, {:.1} GB", g.name.replace("NVIDIA ", "").replace("GeForce ", ""), g.vram_total as f64 / 1e9));
                }
                if !sn.templates_version.is_empty() {
                    parts.push(li_i18n::fmt(tl!("templates {version}"), &[("version", &sn.templates_version)]));
                }
                parts.join(" · ")
            }
            None if s.scan.is_some() => li_i18n::fmt(tl!("ComfyUI is not reachable at {host}"), &[("host", &options().host)]),
            None => tl!("Checking models…").into(),
        };
        ui.label(RichText::new(line).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let recheck = ui.add_enabled_ui(!s.busy(), |ui| widgets::secondary_button(ui, tl!("Re-check"), 0.0)).inner;
            if recheck.on_hover_text(tl!("Check every model again (no test renders)")).clicked() {
                out.push(Action::Recheck);
            }
        });
    });
    if s.scan.is_some() && snapshot.is_none() && !st.connected {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tl!("Start the AI engine to check new models.")).color(t.warning));
            if widgets::primary_button(ui, tl!("Start AI Engine"), 0.0).clicked() {
                out.push(Action::StartEngine);
            }
        });
    }
    if let Some(sc) = &s.scan {
        let updated: Vec<&Issue> = sc.server.models.values().flat_map(|m| &m.issues).filter(|i| i.code == "ReverifyNeeded").collect();
        if let Some(first) = updated.first() {
            let text = li_i18n::fmt(
                tl!("ComfyUI was updated from {old} to {new}. {n} models need a quick re-check."),
                &[
                    ("old", first.args.get("old").map(String::as_str).unwrap_or("")),
                    ("new", first.args.get("new").map(String::as_str).unwrap_or("")),
                    ("n", &updated.len().to_string()),
                ],
            );
            ui.label(RichText::new(text).color(t.warning));
        }
    }
    if let Some(task) = s.task.clone()
        && let Ok(x) = task.lock()
    {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(160.0, 6.0), egui::Sense::hover());
            li_ui_kit::progress::bar(ui, r, x.progress.map(|(d, n)| d as f32 / n.max(1) as f32), t);
            ui.label(RichText::new(&x.label).color(t.text_dim));
            if (x.progress.is_some() || !x.verifying.is_empty()) && widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                out.push(Action::Cancel);
            }
        });
    }
    if let Some((at, moved)) = &s.undo {
        if at.elapsed() > Duration::from_secs(30) {
            s.undo = None;
        } else {
            let name = moved.to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new(li_i18n::fmt(tl!("Moved {file}."), &[("file", &name)])).color(t.text_dim));
                if widgets::secondary_button(ui, tl!("Undo"), 0.0).clicked() {
                    out.push(Action::Undo);
                }
            });
        }
    }
    if let Some((text, error)) = &s.message {
        ui.add(egui::Label::new(RichText::new(text).color(if *error { t.danger } else { t.text_dim })).wrap());
    }
}

/// The models a "Verify all" tests: set up, usable and not tested (or due a re-check).
fn untested(scan: &Scan) -> Vec<String> {
    scan.server.models.iter().filter(|(_, m)| matches!(m.status, Status::Ready | Status::NeedsReverify)).map(|(k, _)| k.clone()).collect()
}

fn footer(ui: &mut egui::Ui, s: &mut State, out: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let keys = s.scan.as_deref().map(untested).unwrap_or_default();
        let connected = s.scan.as_ref().is_some_and(|x| x.snapshot.is_some());
        let label = li_i18n::fmt(tl!("Verify all ready ({n})"), &[("n", &keys.len().to_string())]);
        let verify = ui.add_enabled_ui(!keys.is_empty() && connected && !s.busy(), |ui| widgets::primary_button(ui, &label, 0.0)).inner;
        if verify.on_hover_text(tl!("Test each model with a few small renders, one model at a time")).clicked() {
            out.push(Action::Verify(keys));
        }
        if ui.add_enabled_ui(s.scan.is_some(), |ui| widgets::secondary_button(ui, tl!("Copy report"), 0.0)).inner.clicked() {
            out.push(Action::CopyReport);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::secondary_button(ui, tl!("Close"), 0.0).clicked() {
                s.open = false;
            }
        });
    });
}

#[cfg(test)]
mod tests;
