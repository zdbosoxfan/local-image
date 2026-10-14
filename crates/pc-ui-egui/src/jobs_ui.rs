//! Background jobs in the shell (#210): long commands and file opens run on worker threads (see
//! `photocraft_engine::jobs`) while the window keeps drawing at full frame rate.
//!
//! - [`run`] starts a job-capable command in the background (when [`PhotocraftApp::background_jobs`]
//!   is on: the desktop app; tests and the web run inline as before).
//! - [`tick`] (every frame) applies finished jobs, reports failures, answers control requests
//!   waiting on a job, finishes background opens and keeps frames coming while anything runs.
//! - [`status_progress`] draws the status-bar progress with a Cancel button; [`dialog`] the modal
//!   progress window for jobs that lock the active document (filters, Content-Aware Fill, …),
//!   shown after a short delay so quick jobs don't flash it. Esc cancels. AI selections
//!   ([`non_modal_text`]) show only the status-bar readout so the canvas stays usable.
//! - Files opening in the background get a document tab at once ([`OpenTab`]); its canvas area
//!   shows the progress, and closing the tab cancels the open.
//!
//! The canvas keeps showing the pre-job state: workers never touch the session's documents.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use egui::{Color32, Rect, RichText, Sense, Stroke, vec2};
use li_ui_kit::progress::bar;
use photocraft_engine::jobs::{JobEvent, JobId, JobInfo, JobOutcome, OPEN_JOB, OpenSource, Started};
use serde_json::{Value, json};

use crate::theme::Tokens;
use crate::{ControlResponse, PhotocraftApp, notices};

/// Delay before the modal progress dialog appears (quick jobs finish without it).
pub const DIALOG_DELAY_MS: f64 = 250.0;

/// Jobs that keep the canvas usable while they run: no modal dialog, only the status bar's
/// "Selecting subject…" readout with its Cancel (×) and Esc, as Photoshop's AI selections do.
/// Pan and zoom keep working; edits to the document are refused with the job's name until it
/// ends. Returns the status text.
pub fn non_modal_text(command: &str) -> Option<&'static str> {
    // Kept as data (not `tl!` literals) so they wait for the next translation pass.
    const TEXTS: &[(&str, &str)] =
        &[("select.subject", "Selecting subject…"), ("select.object", "Selecting object…"), ("select.sky", "Selecting sky…"), ("select.quick", "Selecting…")];
    TEXTS.iter().find(|(c, _)| *c == command).map(|(_, t)| *t)
}

/// A file opening in the background: shown as a document tab with progress.
#[derive(Clone, Debug)]
pub struct OpenTab {
    pub job: JobId,
    /// Display name (the file name).
    pub name: String,
    /// Where it was read from (File › Save writes back there; added to Open Recent).
    pub path: Option<String>,
    /// The tab position it goes to when it opens (a file dropped on the tabs); `None`: the end.
    pub slot: Option<usize>,
}

/// The shell's job bookkeeping.
#[derive(Default)]
pub struct JobsUi {
    /// Control replies waiting for a job to end.
    pub(crate) waiters: Vec<(JobId, Sender<ControlResponse>)>,
    /// Background opens, in tab order.
    pub opens: Vec<OpenTab>,
    /// The open tab shown in the canvas area instead of the active document.
    pub focus: Option<JobId>,
    /// The job the latest [`run`] started (the control channel waits on it).
    pub(crate) last_started: Option<JobId>,
    place_finished: bool,
    /// Durable filter choices wait for this exact submitted job to apply.
    pub(crate) filter_defaults: Vec<(JobId, String, crate::filter_dialog::Remembered)>,
}

/// Run command `id`: in the background when background jobs are on and the command is a job
/// (the result is then `{"job": id, "pending": true}` and the edit lands when [`tick`] applies
/// it), else synchronously like [`photocraft_engine::Session::execute`].
pub fn run(app: &mut PhotocraftApp, id: &str, params: Value) -> Result<Value, String> {
    if !app.background_jobs {
        return app.session.execute(id, params).map_err(|e| e.to_string());
    }
    let open_name = (id == photocraft_engine::develop_layer_cmds::OPEN_JOB)
        .then(|| params["name"].as_str().or_else(|| params["path"].as_str()).unwrap_or("Photo").to_string());
    match app.session.start(id, params).map_err(|e| e.to_string())? {
        Started::Done(v) => {
            if v["pending"].as_bool() == Some(true)
                && let Some(job) = v["job"].as_u64().map(JobId).filter(|j| app.session.job(*j).is_some())
            {
                app.jobs.last_started = Some(job);
                if app.jobs.opens.iter().any(|o| o.job == job) {
                    app.jobs.focus = Some(job);
                }
            }
            Ok(v)
        }
        Started::Job(job) => {
            app.jobs.last_started = Some(job);
            if let Some(name) = open_name {
                // Develop sources are originals, not a compositing document's Save path.
                app.jobs.opens.push(OpenTab { job, name, path: None, slot: None });
                app.jobs.focus = Some(job);
                app.ui.chrome.home = None;
            }
            // The status bar's progress readout names the job; clear any old message.
            app.ui.status.clear();
            app.ui.status_error = false;
            Ok(json!({"job": job.0, "pending": true}))
        }
    }
}

/// Open a file in the background: a tab appears at once and shows the progress; the document
/// replaces it when decoded. `path` is remembered for File › Save and Open Recent.
pub fn start_open(app: &mut PhotocraftApp, name: &str, path: Option<String>, source: OpenSource) -> Result<(), String> {
    let name = &app.open_name(name);
    match app.session.start_open_deferred(name, source).map_err(|e| e.to_string())? {
        // Inline (wasm): finish now, like a background open that ended at once.
        Started::Done(v) => {
            let document = opened_document(&v).ok_or("The opened document is no longer available")?;
            finish_open(app, document, name, path.as_deref(), &v, None, None)
        }
        Started::Job(job) => {
            app.jobs.opens.push(OpenTab { job, name: name.to_string(), path, slot: None });
            app.jobs.focus = Some(job);
            app.ui.chrome.home = None;
            app.ui.status = crate::i18n::fmt(tl!("Opening {name}…"), &[("name", name)]);
            app.ui.status_error = false;
            Ok(())
        }
    }
}

/// Per frame: apply finished jobs and react to them; keep frames coming while jobs run; Esc
/// cancels the job in view.
pub fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    crate::save_worker::tick(app, ctx);
    // A queued Escape is considered before publishing a ready save in this frame.
    if let Some(job) = job_in_view(app)
        && crate::save_worker::owns_job(app, job.id)
        && !ctx.text_edit_focused()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        cancel(app, job.id);
    }
    dispatch_batch(app);
    if std::mem::take(&mut app.jobs.place_finished)
        && let Err(error) = crate::transform_tool::begin_placed(app, ctx)
    {
        notices::error(app, error);
    }
    // A focused open tab that ended (or was cancelled elsewhere) gives way to the documents.
    if app.jobs.focus.is_some_and(|j| !app.jobs.opens.iter().any(|o| o.job == j)) {
        app.jobs.focus = app.jobs.opens.last().map(|o| o.job);
    }
    if !keep_polling(app, ctx) {
        return;
    }
    if let Some(job) = job_in_view(app)
        && !ctx.text_edit_focused()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        cancel(app, job.id);
    }
}

/// local-image: [`tick`] while the editor is hidden: apply finished jobs, no key handling.
/// Keep the event loop awake while a background document job exists.
///
/// Jobs can be started by a menu or dialog while the current frame is being drawn, after
/// [`tick`] has already run. Calling this from the shell after drawing avoids leaving such a job
/// unapplied on an otherwise idle event loop (notably Wayland): its document would stay locked,
/// making Undo and Redo look unavailable until the mouse moves (upstream PhotoCraft be877c27).
pub fn keep_polling(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    if !app.session.has_jobs() {
        return false;
    }
    // Animate progress and poll for results at display rate; the UI thread only draws.
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
    true
}

pub fn tick_background(app: &mut PhotocraftApp, ctx: &egui::Context) {
    crate::save_worker::tick(app, ctx);
    dispatch_batch(app);
    if app.session.has_jobs() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn dispatch_batch(app: &mut PhotocraftApp) {
    let previous = app.session.active().map(|st| st.doc.id);
    let events = app.session.poll_jobs();
    dispatch_events(app, events, previous);
}

fn opened_document(result: &Value) -> Option<photocraft_doc::DocId> {
    // Required admission-time identity; the `document` tab index can be stale.
    result.get("documentId")?.as_u64().map(photocraft_doc::DocId)
}

fn dispatch_events(app: &mut PhotocraftApp, events: Vec<JobEvent>, mut previous: Option<photocraft_doc::DocId>) {
    // Read the whole batch's admission identities before any handoff or callback.
    // They remain valid even when engine apply callbacks already changed tab order.
    let events: Vec<_> = events
        .into_iter()
        .map(|event| {
            let opening = matches!(event.command.as_str(), OPEN_JOB | photocraft_engine::develop_layer_cmds::OPEN_JOB);
            let document = match &event.outcome {
                JobOutcome::Done(value) if opening => opened_document(value),
                _ => None,
            };
            (event, document)
        })
        .collect();
    if events.iter().any(|(event, _)| {
        matches!(event.command.as_str(), OPEN_JOB | photocraft_engine::develop_layer_cmds::OPEN_JOB) && matches!(event.outcome, JobOutcome::Done(_))
    }) {
        // Engine admission activates new documents. Restore first, even if a
        // non-open event precedes the open: its sync_views must see the real owner.
        restore_open_focus(app, previous);
    }
    let mut handoff = std::mem::take(&mut app.library_vectorize);
    let errors: Vec<_> = events.iter().filter_map(|(event, _)| handoff.on_job_event(app, event).err()).collect();
    app.library_vectorize = handoff;
    for (event, document) in events {
        if matches!(event.command.as_str(), "file.placeEmbedded" | "file.placeLinked") && matches!(event.outcome, JobOutcome::Done(_)) {
            app.jobs.place_finished = true;
        }
        on_event(app, event, previous, document);
        previous = app.session.active().map(|st| st.doc.id);
    }
    for error in errors {
        notices::error(app, error);
    }
}

/// The job the user is looking at: the open tab in view, the active document's job, or (for
/// the status bar) the newest running job.
pub fn job_in_view(app: &PhotocraftApp) -> Option<JobInfo> {
    if let Some(j) = app.jobs.focus.and_then(|j| app.session.job(j)) {
        return Some(j);
    }
    app.session.active_job().or_else(|| app.session.jobs().pop())
}

/// Cancel job `id` (the status bar's ×, the dialog's Cancel, Esc, closing an opening tab).
pub fn cancel(app: &mut PhotocraftApp, id: JobId) {
    let label = app.session.job(id).map(|j| j.label);
    if app.session.cancel_job(id) {
        // The event (and its status text) arrives with the next poll; say so now as well.
        if let Some(l) = label {
            app.ui.status = crate::i18n::fmt(tl!("Cancelled {label}"), &[("label", &l)]);
            app.ui.status_error = false;
        }
    }
}

fn on_event(app: &mut PhotocraftApp, mut e: JobEvent, previous: Option<photocraft_doc::DocId>, document: Option<photocraft_doc::DocId>) {
    crate::filter_dialog::on_job_event(app, &e);
    crate::ai_remove_ui::on_job_event(app, &e);
    if e.command == "clipboard.import"
        && let JobOutcome::Done(value) = &e.outcome
        && let Some(fields) = value.get("clipboardPrompt")
    {
        e.outcome = JobOutcome::Done(crate::clipboard_ui::open(app, fields.clone()));
    }
    crate::restoration_ui::repair_finished(app, &e);
    // Control requests waiting on this job.
    let reply = match &e.outcome {
        JobOutcome::Done(v) => json!({"ok": true, "result": v}),
        JobOutcome::Failed(err) => json!({"ok": false, "error": err}),
        JobOutcome::Cancelled => json!({"ok": false, "error": "cancelled"}),
    };
    app.jobs.waiters.retain(|(job, tx)| {
        if *job == e.id {
            let _ = tx.send(reply.clone());
            false
        } else {
            true
        }
    });
    if crate::save_worker::on_event(app, &e) {
        return;
    }
    // The owning export dialog reports staged publication/failure/cancellation once.
    // Control waiters above still receive the terminal job result; unrelated jobs
    // retain their ordinary reporting below.
    if (e.command == "file.svg.export" && app.ui.svg_export.on_job_event(&e)) || (e.command == "file.pdf.export" && app.ui.pdf_export.on_job_event(&e)) {
        return;
    }
    if e.command == OPEN_JOB || e.command == photocraft_engine::develop_layer_cmds::OPEN_JOB {
        let Some(i) = app.jobs.opens.iter().position(|o| o.job == e.id) else { return };
        let tab = app.jobs.opens.remove(i);
        match e.outcome {
            JobOutcome::Done(v) => {
                let restore = (app.jobs.focus != Some(e.id)).then_some(previous).flatten();
                let result = document
                    .ok_or_else(|| "The opened document is no longer available".to_string())
                    .and_then(|document| finish_open(app, document, &tab.name, tab.path.as_deref(), &v, tab.slot, restore));
                if let Err(err) = result {
                    app.open_failed(&tab.name, &err);
                }
            }
            JobOutcome::Failed(err) => app.open_failed(&tab.name, &err),
            JobOutcome::Cancelled => {
                app.ui.status = crate::i18n::fmt(tl!("Cancelled opening {name}"), &[("name", &tab.name)]);
                app.ui.status_error = false;
            }
        }
        return;
    }
    if e.command == "develop.syncPhoto"
        && let JobOutcome::Done(v) = &e.outcome
    {
        crate::develop_layer::finish_sync(app, v);
        return;
    }
    // local-image: AI jobs (Generate results, downloads).
    if crate::ai_ui::on_job_event(app, &e) {
        return;
    }
    match e.outcome {
        JobOutcome::Done(v) if e.command == "brush.presets.importAbr" => {
            app.ui.status = crate::preset_files_ui::summary(&v);
            app.ui.status_error = false;
            app.ui.panels.brush_settings = true;
            app.ui.brush_tab = 1;
        }
        JobOutcome::Done(v) => {
            app.sync_views();
            // A selection job that found nothing says so ("No sky was detected").
            app.ui.status = v.get("message").and_then(Value::as_str).filter(|_| non_modal_text(&e.command).is_some()).map_or(e.label, str::to_string);
            app.ui.status_error = false;
        }
        JobOutcome::Failed(err) => {
            app.sync_views();
            notices::error(app, format!("{}: {err}", e.label));
        }
        JobOutcome::Cancelled => {
            app.ui.status = crate::i18n::fmt(tl!("Cancelled {label}"), &[("label", &e.label)]);
            app.ui.status_error = false;
        }
    }
}

fn restore_open_focus(app: &mut PhotocraftApp, document: Option<photocraft_doc::DocId>) {
    if let Some(index) = document.and_then(|id| app.session.documents().iter().position(|st| st.doc.id == id)) {
        app.session.set_active(index);
    }
}

/// What [`crate::PhotocraftApp::open_bytes`] does after decoding, for a background open's result.
fn finish_open(
    app: &mut PhotocraftApp,
    document: photocraft_doc::DocId,
    name: &str,
    path: Option<&str>,
    v: &Value,
    slot: Option<usize>,
    restore: Option<photocraft_doc::DocId>,
) -> Result<(), String> {
    // Restore before document.move's UI synchronization can inspect a gesture.
    restore_open_focus(app, restore);
    let index = app.session.documents().iter().position(|st| st.doc.id == document).ok_or("The opened document is no longer available")?;
    if let Some(slot) = slot {
        let _ = app.run("document.move", json!({"document": index, "to": slot}));
    }
    let index = app.session.documents().iter().position(|st| st.doc.id == document).ok_or("The opened document is no longer available")?;
    let mut warnings: Vec<String> =
        v.get("warnings").and_then(Value::as_array).map(|a| a.iter().filter_map(|w| w.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if let Some(error) = v.get("color").and_then(|color| color.get("error")).and_then(Value::as_str) {
        warnings.push(format!("Colour conversion was not applied: {error}"));
    }
    if !app.session.set_active(index) {
        return Err("The opened document is no longer available".into());
    }
    let photo = v["nativeImport"] != true && app.session.active().is_some_and(|st| crate::toolsets_ui::persona::contains_photo(&st.doc));
    let color_target = crate::prefs_ui::mismatch_target(&app.session);
    if let Some(p) = path {
        if v["nativeImport"] == true {
            app.push_recent(p);
            crate::filmstrip_ui::follow_file(app, p);
        } else {
            app.opened_from(p);
        }
    }
    app.ui.status = format!("Opened {name}");
    app.ui.status_error = false;
    notices::io_warnings(app, &format!("Opened {name}"), &warnings);
    // Script events bound to "Open Document".
    photocraft_engine::automate_cmds::document_opened(&mut app.session);
    restore_open_focus(app, restore);
    app.sync_views();
    if photo && restore.is_none() {
        crate::toolsets_ui::persona::photo_admitted(app, document);
    }
    let color = v.get("color").cloned().unwrap_or(Value::Null);
    let ask = color.get("ask").and_then(Value::as_bool) == Some(true);
    if ask
        && (color.get("mismatch").and_then(Value::as_bool) == Some(true) || color.get("missing").is_some())
        && let Some(target) = color_target
    {
        crate::prefs_ui::open_mismatch_for(app, &color, target);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------- drawing

// Bars and percentages are the shared progress widget (`li_ui_kit::progress`, also the
// Library's task panel): nothing reported yet = an indeterminate bar and no number.
fn shown_fraction(j: &JobInfo) -> Option<f32> {
    li_ui_kit::progress::shown(Some(j.progress))
}

fn percent(j: &JobInfo) -> String {
    li_ui_kit::progress::percent(Some(j.progress))
}

/// The status bar's job readout: "Gaussian Blur ▬▬▬▭▭ 45 %  ×". Draws nothing when no job runs.
pub fn status_progress(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(j) = job_in_view(app) else { return };
    let t = Tokens::get(ui.ctx());
    let (bar_w, bar_h) = if t.pro { (120.0, 6.0) } else { (140.0, 6.0) };
    let more = app.session.jobs().len().saturating_sub(1);
    // Right to left: ×, percentage, bar, label.
    let xresp = crate::icons::button(ui, "x", 18.0, false, tl!("Cancel (Esc)"));
    ui.label(RichText::new(percent(&j)).color(t.text_dim).size(11.5).monospace());
    let (br, _) = ui.allocate_exact_size(vec2(bar_w, bar_h), Sense::hover());
    bar(ui, br, shown_fraction(&j), &t);
    let name = non_modal_text(&j.command).unwrap_or(&j.label);
    let label = if more > 0 { format!("{} (+{more})", tl!(name)) } else { tl!(name).to_owned() };
    ui.label(RichText::new(label).color(t.text_dim).size(12.0));
    if xresp.clicked() {
        cancel(app, j.id);
    }
}

/// The modal progress window for a job locking the active document (Photoshop shows one for
/// long filters): title, status, bar, percentage and elapsed time, Cancel. Appears after
/// [`DIALOG_DELAY_MS`]; Esc cancels (see [`tick`]).
pub fn dialog(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(j) = app.session.active_job() else { return };
    if j.elapsed_ms < DIALOG_DELAY_MS || non_modal_text(&j.command).is_some() {
        return;
    }
    let t = Tokens::get(ctx);
    let mut cancel_it = false;
    // Photoshop doesn't dim the window behind its progress dialog either.
    let modal = egui::Modal::new(egui::Id::new("job-progress")).backdrop_color(Color32::from_black_alpha(36)).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.label(RichText::new(tl!(&j.label)).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(10.0);
        let msg = if j.message.is_empty() || j.message == j.label { tl!("Working…").to_string() } else { j.message.clone() };
        ui.label(RichText::new(msg).color(t.text_dim));
        ui.add_space(8.0);
        let (br, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
        bar(ui, br, shown_fraction(&j), &t);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(percent(&j)).color(t.text_faint).size(11.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(elapsed(j.elapsed_ms)).color(t.text_faint).size(11.5));
            });
        });
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel_it = crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked();
            ui.label(RichText::new(tl!("Esc to cancel")).color(t.text_faint).size(11.0));
        });
    });
    // A click on the backdrop doesn't cancel the work; Esc is handled in `tick`.
    let _ = modal;
    if cancel_it {
        cancel(app, j.id);
    }
}

fn elapsed(ms: f64) -> String {
    let s = (ms / 1000.0).max(0.0);
    if s < 60.0 { format!("{s:.1} s") } else { format!("{}:{:02}", (s / 60.0).floor() as u64, (s % 60.0).floor() as u64) }
}

/// The canvas area of an opening tab: the file name, a bar and Cancel, centred on the
/// workspace backdrop.
pub fn open_card(app: &mut PhotocraftApp, ui: &mut egui::Ui, job: JobId) {
    let Some(tab) = app.jobs.opens.iter().find(|o| o.job == job).cloned() else { return };
    let Some(j) = app.session.job(job) else { return };
    let t = Tokens::get(ui.ctx());
    let area = ui.available_rect_before_wrap();
    ui.painter().rect_filled(area, 0.0, t.canvas);
    let card = Rect::from_center_size(area.center(), vec2(340.0, 132.0));
    ui.painter().rect_filled(card, t.radius_lg, t.card);
    ui.painter().rect_stroke(card, t.radius_lg, Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(card.shrink(18.0)));
    child.label(RichText::new(crate::i18n::fmt(tl!("Opening {name}"), &[("name", &tab.name)])).font(crate::theme::semibold(14.0)));
    child.add_space(4.0);
    let msg = if j.message.is_empty() { tl!("Reading…").to_string() } else { format!("{}…", j.message) };
    child.label(RichText::new(msg).color(t.text_dim).size(12.0));
    child.add_space(10.0);
    let (br, _) = child.allocate_exact_size(vec2(child.available_width(), 8.0), Sense::hover());
    bar(&child, br, shown_fraction(&j), &t);
    child.add_space(10.0);
    let mut cancel_it = false;
    child.horizontal(|ui| {
        ui.label(RichText::new(percent(&j)).color(t.text_faint).size(11.5));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel_it = crate::widgets::secondary_button(ui, tl!("Cancel"), 76.0).clicked();
        });
    });
    if cancel_it {
        cancel(app, job);
    }
}

/// Open tabs for the tab strips: (job, title, progress).
pub fn open_tabs(app: &PhotocraftApp) -> Vec<(JobId, String, f32)> {
    app.jobs.opens.iter().map(|o| (o.job, o.name.clone(), app.session.job(o.job).map_or(0.0, |j| j.progress))).collect()
}

/// Draw one opening tab's progress underline inside tab rect `r`.
pub fn tab_underline(ui: &egui::Ui, r: Rect, frac: f32, t: &Tokens) {
    let line = Rect::from_min_max(egui::pos2(r.left() + 4.0, r.bottom() - 3.0), egui::pos2(r.right() - 4.0, r.bottom() - 1.0));
    ui.painter().rect_filled(line, 1.0, t.field_border);
    let f = Rect::from_min_size(line.min, vec2(line.width() * frac.clamp(0.0, 1.0), line.height()));
    ui.painter().rect_filled(f, 1.0, t.accent);
}

/// Jobs for `ui.inspect`.
pub fn inspect(app: &PhotocraftApp) -> Value {
    json!({
        "running": app.session.jobs(),
        "opening": app.jobs.opens.iter().map(|o| json!({"job": o.job.0, "name": o.name, "path": o.path})).collect::<Vec<_>>(),
        "focus": app.jobs.focus.map(|j| j.0),
        "background": app.background_jobs,
    })
}

/// Bytes shared with a background open without copying them again.
pub fn bytes(b: &[u8]) -> OpenSource {
    OpenSource::Bytes(Arc::new(b.to_vec()))
}

#[cfg(test)]
#[path = "jobs_ui_tests.rs"]
mod tests;
