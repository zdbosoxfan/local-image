//! Photo Merge in the UI: the HDR / Panorama / HDR Panorama dialog (options + live preview) and
//! the background merge jobs.
//!
//! Previews and final merges run [`MergeJob`]s (or [`BatchMergeJob`]s for grouped brackets) on worker threads with
//! progress and cancellation; the window stays responsive. Changing an option cancels the running
//! preview and starts a new one. The final merge keeps running after the dialog closes (progress
//! in a toast); when it finishes, the result is written, imported and selected
//! ([`lightcraft_engine::Session::finish_merge`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_engine::merge::{BatchMergeJob, MergeJob, MergeKind, MergeOutput, PreparedMerge};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::LightcraftApp;

/// The dialog's options (also the parameters of the `merge.*` commands).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MergeDialog {
    /// "merge.hdr" | "merge.panorama" | "merge.hdrPanorama".
    pub command: String,
    pub align: bool,
    /// Detect separate bracket sets and merge them sequentially in the background.
    pub auto_brackets: bool,
    /// none | low | medium | high
    pub deghost: String,
    pub show_overlay: bool,
    pub auto_settings: bool,
    pub stack: bool,
    /// auto | spherical | cylindrical | perspective
    pub projection: String,
    pub boundary_warp: f64,
    pub auto_crop: bool,
    pub fill_edges: bool,
    /// HDR panorama: photos per bracket (0 = from EXIF).
    pub bracket: u32,
}

impl Default for MergeDialog {
    fn default() -> Self {
        MergeDialog {
            command: "merge.hdr".into(),
            align: true,
            auto_brackets: false,
            deghost: "none".into(),
            show_overlay: false,
            auto_settings: true,
            stack: false,
            projection: "auto".into(),
            boundary_warp: 0.0,
            auto_crop: true,
            fill_edges: false,
            bracket: 0,
        }
    }
}

impl MergeDialog {
    pub fn for_command(command: &str) -> MergeDialog {
        MergeDialog { command: command.into(), ..Default::default() }
    }
    pub fn title(&self) -> &'static str {
        match self.command.as_str() {
            "merge.pixelShift" => "Sony Pixel Shift Merge Preview",
            "merge.panorama" => "Panorama Merge Preview",
            "merge.hdrPanorama" => "HDR Panorama Merge Preview",
            _ => "HDR Merge Preview",
        }
    }
    pub fn is_hdr(&self) -> bool {
        matches!(self.command.as_str(), "merge.hdr" | "merge.hdrPanorama")
    }
    pub fn is_pano(&self) -> bool {
        matches!(self.command.as_str(), "merge.panorama" | "merge.hdrPanorama")
    }
    pub fn params(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// A merge running on a worker thread.
pub struct MergeTask {
    pub preview: bool,
    pub job: MergeJob,
    /// The dialog options it was started with.
    pub options: MergeDialog,
    pub progress: Arc<Mutex<(f32, String)>>,
    pub cancel: Arc<AtomicBool>,
    rx: Receiver<Result<MergeOutput, String>>,
    batch_rx: Option<Receiver<BatchEvent>>,
    batch_results: Vec<Value>,
    library_generation: u64,
}

enum BatchEvent {
    Output { job: MergeJob, prepared: Box<PreparedMerge>, acknowledged: SyncSender<Result<(), String>> },
    Finished(Result<Value, String>),
}

/// Merge state of the app.
#[derive(Default)]
pub struct MergeState {
    pub preview_task: Option<MergeTask>,
    pub final_task: Option<MergeTask>,
    /// The latest preview (texture, size, its options, stats) and the latest error.
    pub preview: Option<(egui::TextureHandle, [usize; 2], MergeDialog, Value)>,
    pub preview_pixels: Option<Arc<egui::ColorImage>>,
    pub error: Option<String>,
    /// Options whose preview failed (not retried until they change).
    pub failed_options: Option<MergeDialog>,
    /// The last finished merge (`{id, path, …}`), or a batch summary with its imported results.
    pub last_result: Option<Value>,
    /// Photos the dialog merges (the selection when it opened).
    pub ids: Vec<PhotoId>,
    /// The options of the last merge started, per kind ("merge.hdr" …): Merge with Last Settings.
    pub last: std::collections::HashMap<String, MergeDialog>,
}

impl MergeState {
    /// A preview or a merge is running.
    pub fn busy(&self) -> bool {
        self.preview_task.is_some() || self.final_task.is_some()
    }
}

fn spawn(app: &LightcraftApp, options: &MergeDialog, ids: &[PhotoId], preview: bool) -> Result<MergeTask, String> {
    let (kind, finish) = lightcraft_engine::merge::parse(&options.command, &options.params()).map_err(|e| e.to_string())?;
    let batch = if options.auto_brackets && options.command == "merge.hdr" {
        let MergeKind::Hdr { hdr } = &kind else { return Err("automatic brackets require HDR".into()) };
        Some(app.session.plan_batch_hdr(hdr.clone(), finish.clone(), ids).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let job = if let Some(batch) = &batch {
        batch.source.clone()
    } else {
        app.session.plan_merge(kind, finish, ids, preview).map_err(|e| e.to_string())?
    };
    let progress = Arc::new(Mutex::new((0.0, "Starting".to_string())));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    // Prepare the DNG write and Auto Settings on the worker; only one completed file
    // awaits catalog import, acknowledged before decoding the next set.
    let (batch_tx, batch_rx) = sync_channel(1);
    let is_batch_final = batch.is_some() && !preview;
    #[cfg(target_arch = "wasm32")]
    if is_batch_final {
        return Err("batch HDR requires desktop background workers".into());
    }
    let (j, p, c) = (job.clone(), progress.clone(), cancel.clone());
    let work = move || {
        let update = |f, stage: &str| {
            if let Ok(mut g) = p.lock() {
                *g = (f, stage.to_string());
            }
            !c.load(Ordering::Relaxed)
        };
        if let Some(batch) = batch.as_ref().filter(|_| !preview) {
            let run = || {
                batch
                    .run(&update, &mut |job, out| {
                        let fraction = p.lock().map(|g| g.0).unwrap_or_default();
                        if !update(fraction, "Preparing HDR result") {
                            return Err("cancelled".into());
                        }
                        let prepared = job.prepare_output(out)?;
                        // The prepared file removes itself on drop until catalog import
                        // accepts it, including cancellation before delivery or rejection.
                        if c.load(Ordering::Relaxed) {
                            return Err("cancelled".into());
                        }
                        let (acknowledged, ack_rx) = sync_channel(1);
                        batch_tx
                            .send(BatchEvent::Output { job: job.clone(), prepared: Box::new(prepared), acknowledged })
                            .map_err(|_| "cancelled".to_string())?;
                        loop {
                            if c.load(Ordering::Relaxed) {
                                return Err("cancelled".into());
                            }
                            match ack_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                                Ok(result) => return result,
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Err("cancelled".into()),
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                            }
                        }
                    })
                    .map(|summary| {
                        json!({
                            "groups": summary.groups,
                            "completed": summary.completed,
                            "skipped": summary.skipped,
                            "failed": summary.failed,
                        })
                    })
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err("the merge failed unexpectedly".into()));
            let _ = batch_tx.send(BatchEvent::Finished(result));
        } else {
            let run = || {
                if let Some(batch) = batch.as_ref() { batch_preview(batch, &update) } else { j.run(&update) }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err("the merge failed unexpectedly".into()));
            let _ = tx.send(result);
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("photo-merge".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    Ok(MergeTask {
        preview,
        job,
        options: options.clone(),
        progress,
        cancel,
        rx,
        batch_rx: is_batch_final.then_some(batch_rx),
        batch_results: Vec::new(),
        library_generation: app.session.library_generation,
    })
}

fn batch_preview(batch: &BatchMergeJob, progress: &lightcraft_engine::merge::ProgressFn<'_>) -> Result<MergeOutput, String> {
    let groups = batch.detect(progress)?;
    let mut first = groups.first().cloned().ok_or("no complete HDR bracket sets found")?;
    first.preview = Some(lightcraft_engine::merge::PREVIEW_EDGE);
    let mut out = first.run(progress)?;
    let grouped = groups.iter().map(|job| job.sources.len()).sum::<usize>();
    out.info["bracketSets"] = json!(groups.len());
    out.info["skippedPhotos"] = json!(batch.source.sources.len().saturating_sub(grouped));
    Ok(out)
}

/// Open the merge dialog for the selection.
pub fn open(app: &mut LightcraftApp, command: &str) -> Result<Value, String> {
    let ids = app.session.selected();
    if command == "merge.pixelShift" {
        if !matches!(ids.len(), 1 | 4 | 16) {
            return Err("select one composed ARQ or all 4 or 16 original Pixel Shift shots".into());
        }
    } else if ids.len() < 2 {
        return Err("select at least 2 photos to merge".into());
    }
    app.merge.ids = ids;
    app.merge.error = None;
    app.merge.failed_options = None;
    app.merge.preview = None;
    app.merge.preview_pixels = None;
    if let Some(t) = app.merge.preview_task.take() {
        t.cancel.store(true, Ordering::Relaxed);
    }
    app.ui.dialog = Some(crate::state::Dialog::Merge { opts: MergeDialog::for_command(command) });
    Ok(json!({"photos": app.merge.ids.len()}))
}

/// Start the full-resolution merge (the dialog's Merge button / `ui.dialog.confirm`).
pub fn start_final(app: &mut LightcraftApp, opts: &MergeDialog) -> Result<Value, String> {
    if app.merge.final_task.is_some() {
        return Err("a merge is already running".into());
    }
    if let Some(t) = app.merge.preview_task.take() {
        t.cancel.store(true, Ordering::Relaxed);
    }
    let ids = app.merge.ids.clone();
    let task = spawn(app, opts, &ids, false)?;
    app.merge.final_task = Some(task);
    app.merge.last.insert(opts.command.clone(), opts.clone());
    Ok(json!({"started": true, "photos": ids.len()}))
}

/// Merge the selection without the dialog, with the options last used for `command` (the
/// defaults the first time).
pub fn start_last(app: &mut LightcraftApp, command: &str) -> Result<Value, String> {
    let ids = app.session.selected();
    if ids.len() < 2 {
        return Err("select at least 2 photos to merge".into());
    }
    app.merge.ids = ids;
    let opts = app.merge.last.get(command).cloned().unwrap_or_else(|| MergeDialog::for_command(command));
    start_final(app, &opts)
}

/// Per frame: keep the preview in sync with the dialog's options, collect finished jobs.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    // preview for the open dialog
    let dialog_opts = match &app.ui.dialog {
        Some(crate::state::Dialog::Merge { opts }) => Some(opts.clone()),
        _ => None,
    };
    match &dialog_opts {
        Some(opts) => {
            let current = app.merge.preview_task.as_ref().map(|t| &t.options).or(app.merge.preview.as_ref().map(|p| &p.2));
            let stale = current != Some(opts) && !last_failed_matches(app, opts);
            if stale {
                if let Some(t) = app.merge.preview_task.take() {
                    t.cancel.store(true, Ordering::Relaxed);
                }
                let ids = app.merge.ids.clone();
                match spawn(app, opts, &ids, true) {
                    Ok(t) => {
                        app.merge.error = None;
                        app.merge.preview_task = Some(t);
                    }
                    Err(e) => {
                        app.merge.error = Some(e);
                        app.merge.failed_options = Some(opts.clone());
                    }
                }
            }
        }
        None => {
            if let Some(t) = app.merge.preview_task.take() {
                t.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    if let Some(t) = &app.merge.preview_task {
        match t.rx.try_recv() {
            Ok(Ok(out)) => {
                let Some(t) = app.merge.preview_task.take() else { return };
                if let Some(img) = out.preview {
                    let color = Arc::new(egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.as_bytes()));
                    let tex = ctx.load_texture("merge-preview", color.clone(), egui::TextureOptions::LINEAR);
                    app.merge.preview_pixels = Some(color);
                    app.merge.preview = Some((tex, [img.width, img.height], t.options, out.info));
                }
            }
            Ok(Err(e)) => {
                let Some(t) = app.merge.preview_task.take() else { return };
                if e != "cancelled" {
                    app.merge.error = Some(e);
                    app.merge.failed_options = Some(t.options);
                }
            }
            Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
        }
    }
    // the final merge
    if app.merge.final_task.as_ref().is_some_and(|task| task.batch_rx.is_some()) {
        poll_batch(app, ctx);
        return;
    }
    if let Some(t) = &app.merge.final_task {
        match t.rx.try_recv() {
            Ok(r) => {
                let Some(t) = app.merge.final_task.take() else { return };
                let what = if t.options.command == "merge.pixelShift" {
                    "Pixel Shift"
                } else if t.options.command == "merge.panorama" {
                    "Panorama"
                } else if t.options.command == "merge.hdrPanorama" {
                    "HDR panorama"
                } else {
                    "HDR"
                };
                match r.map_err(lightcraft_engine::EngineError::Other).and_then(|out| app.session.finish_merge(&t.job, out)) {
                    Ok(v) => {
                        app.merge.last_result = Some(v);
                        app.toast(ctx, li_i18n::tr_format!("{what} merge added", what = what));
                    }
                    Err(e) => {
                        app.ui.status = e.to_string();
                        app.toast(ctx, li_i18n::tr_format!("{what} merge failed: {e}", e = e, what = what));
                    }
                }
            }
            Err(_) => {
                let (f, stage) = t.progress.lock().map(|g| g.clone()).unwrap_or_default();
                let now = ctx.input(|i| i.time);
                app.ui.toast = Some((li_i18n::tr_format!("Merging… {stage} {:.0}%", f * 100.0, stage = stage), now + 0.5));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn poll_batch(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = app.merge.final_task.as_ref() else { return };
    if task.library_generation != app.session.library_generation {
        task.cancel.store(true, Ordering::Relaxed);
        app.merge.final_task = None;
        app.toast(ctx, li_i18n::library::t("HDR batch cancelled because the library changed"));
        return;
    }
    let Some(receiver) = task.batch_rx.as_ref() else { return };
    let event = match receiver.try_recv() {
        Ok(event) => Some(event),
        Err(std::sync::mpsc::TryRecvError::Empty) => None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(BatchEvent::Finished(Err("the merge failed unexpectedly".into()))),
    };
    match event {
        Some(BatchEvent::Output { job, prepared, acknowledged }) => {
            if task.cancel.load(Ordering::Relaxed) {
                let _ = acknowledged.send(Err("cancelled".into()));
            } else {
                match app.session.finish_prepared_merge(&job, *prepared) {
                    Ok(result) => {
                        if let Some(task) = app.merge.final_task.as_mut() {
                            task.batch_results.push(result);
                        }
                        let _ = acknowledged.send(Ok(()));
                    }
                    Err(e) => {
                        app.ui.status = e.to_string();
                        let _ = acknowledged.send(Err(e.to_string()));
                    }
                }
            }
        }
        Some(BatchEvent::Finished(result)) => {
            let Some(task) = app.merge.final_task.take() else { return };
            let completed = task.batch_results.len();
            let (mut summary, message) = match result {
                Ok(summary) => {
                    let failed = summary["failed"].as_array().map_or(0, Vec::len);
                    let skipped = summary["skipped"].as_array().map_or(0, Vec::len);
                    let message = format!("HDR batch: {completed} added · {failed} failed · {skipped} ungrouped photos");
                    (summary, message)
                }
                Err(error) => {
                    let message = if error == "cancelled" {
                        format!("HDR batch cancelled: {completed} added")
                    } else {
                        app.ui.status = error.clone();
                        format!("HDR batch failed: {e}", e = error)
                    };
                    (json!({"completed": completed, "error": error}), message)
                }
            };
            summary["results"] = json!(task.batch_results);
            app.merge.last_result = Some(summary);
            app.toast(ctx, message);
        }
        None => {}
    }
    if let Some(task) = app.merge.final_task.as_ref() {
        let t = crate::theme::Tokens::get(ctx);
        egui::Window::new(li_i18n::library::t("HDR Batch Merge"))
            .collapsible(false)
            .resizable(false)
            .default_width(t.panel_w)
            .show(ctx, |ui| batch_progress(ui, task));
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn batch_progress(ui: &mut egui::Ui, task: &MergeTask) {
    let (fraction, stage) = task.progress.lock().map(|g| g.clone()).unwrap_or_default();
    ui.label(li_i18n::tr_format!("{stage}… {:.0}%", fraction * 100.0, stage = stage));
    ui.label(format!("{} HDR merges added", task.batch_results.len()));
    let cancelling = task.cancel.load(Ordering::Relaxed);
    ui.add_enabled_ui(!cancelling, |ui| {
        if crate::widgets::text_button(ui, "merge.batchCancel", if cancelling { "Cancelling…" } else { "Cancel" }, false).clicked() {
            task.cancel.store(true, Ordering::Relaxed);
        }
    });
}

fn last_failed_matches(app: &LightcraftApp, opts: &MergeDialog) -> bool {
    app.merge.failed_options.as_ref() == Some(opts)
}

/// The dialog body (options on the left, preview on the right).
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, opts: &mut MergeDialog) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.horizontal_top(|ui| {
        // preview
        ui.vertical(|ui| {
            let box_size = egui::vec2(520.0, 360.0);
            let (rect, _) = ui.allocate_exact_size(box_size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, t.canvas);
            crate::widgets::register(ui.ctx(), "merge.preview", rect);
            if let Some((tex, size, _, _)) = &app.merge.preview {
                let s = (box_size.x / size[0] as f32).min(box_size.y / size[1] as f32);
                let r = egui::Rect::from_center_size(rect.center(), egui::vec2(size[0] as f32 * s, size[1] as f32 * s));
                ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            if let Some(task) = &app.merge.preview_task {
                let (f, stage) = task.progress.lock().map(|g| g.clone()).unwrap_or_default();
                ui.painter().text(
                    rect.center_bottom() - egui::vec2(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    li_i18n::tr_format!("{stage}… {:.0}%", f * 100.0, stage = stage),
                    t.font(12.0),
                    t.text,
                );
            }
            if let Some(e) = &app.merge.error {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, e, t.font(13.0), t.text);
            }
            let info = app.merge.preview.as_ref().map(|p| p.3.clone());
            if let Some(info) = info {
                let txt = if opts.command == "merge.pixelShift" {
                    format!(
                        "{} · {} {}",
                        li_i18n::tr_format!("{} photos", info["shots"].as_u64().unwrap_or(0)),
                        info["motionPixels"].as_u64().unwrap_or(0),
                        li_i18n::library::t("pixels use motion fallback")
                    )
                } else if opts.is_pano() {
                    format!(
                        "{} photos · {} · {:.0}° × {:.0}°",
                        info["used"].as_array().map_or(0, Vec::len),
                        info["projection"].as_str().unwrap_or(""),
                        info["fov"][0].as_f64().unwrap_or(0.0),
                        info["fov"][1].as_f64().unwrap_or(0.0)
                    )
                } else {
                    let ev: Vec<String> =
                        info["ev"].as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|v| format!("{v:+.1}")).collect()).unwrap_or_default();
                    format!("{} photos · exposures {} EV", ev.len(), ev.join(" / "))
                };
                ui.label(egui::RichText::new(txt).color(t.text_dim));
                if let Some(groups) = info["bracketSets"].as_u64() {
                    let skipped = info["skippedPhotos"].as_u64().unwrap_or(0);
                    ui.label(egui::RichText::new(format!("{groups} bracket sets · {skipped} ungrouped photos")).color(t.text_dim));
                }
            }
        });
        ui.add_space(12.0);
        // options
        ui.vertical(|ui| {
            ui.set_width(if matches!(opts.command.as_str(), "merge.pixelShift" | "merge.hdr") { t.panel_w } else { 220.0 });
            if opts.command == "merge.pixelShift" {
                ui.label(egui::RichText::new(li_i18n::library::t("Measured full colour; 16 shots double width and height.")).color(t.text_label));
                if app.merge.ids.len() == 1 {
                    ui.label(
                        egui::RichText::new(li_i18n::library::t(
                            "Composed ARQ keeps its existing pixels. Motion correction requires the original shots.",
                        ))
                        .color(t.text_dim),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(li_i18n::library::t("Moving areas use the first shot. Capture order comes from Sony metadata."))
                            .color(t.text_dim),
                    );
                    crate::widgets::checkbox(ui, "merge.motionOverlay", &mut opts.show_overlay, "Show Motion Overlay");
                }
            }
            if opts.is_pano() {
                ui.label(egui::RichText::new(li_i18n::library::t("Projection")).color(t.text_label));
                for (v, l) in [("auto", "Auto Select"), ("spherical", "Spherical"), ("cylindrical", "Cylindrical"), ("perspective", "Perspective")] {
                    if ui.radio(opts.projection == v, l).clicked() {
                        opts.projection = v.to_string();
                    }
                }
                ui.add_space(4.0);
                let mut bw = opts.boundary_warp;
                if num(ui, &BOUNDARY_WARP, &mut bw, crate::widgets::numeric_owner(app, (&opts.command, &app.session.selection.ids))) {
                    opts.boundary_warp = bw.round();
                }
                ui.checkbox(&mut opts.fill_edges, li_i18n::library::t("Fill Edges"));
                ui.checkbox(&mut opts.auto_crop, li_i18n::library::t("Auto Crop"));
                ui.add_space(4.0);
            }
            if opts.is_hdr() {
                crate::widgets::checkbox(ui, "merge.align", &mut opts.align, "Auto Align");
                if opts.command == "merge.hdr" {
                    crate::widgets::checkbox(ui, "merge.autoBrackets", &mut opts.auto_brackets, "Automatically Group Brackets");
                    if opts.auto_brackets {
                        ui.label(
                            egui::RichText::new(li_i18n::library::t("Merge each detected set in the background. Preview shows the first set."))
                                .color(t.text_dim),
                        );
                    }
                }
                ui.label(egui::RichText::new(li_i18n::library::t("Deghost Amount")).color(t.text_label));
                ui.horizontal(|ui| {
                    for (i, (v, l)) in [("none", "None"), ("low", "Low"), ("medium", "Med"), ("high", "High")].iter().enumerate() {
                        if crate::widgets::text_button(ui, &format!("mergeDeghost-{i}"), l, opts.deghost == *v).clicked() {
                            opts.deghost = v.to_string();
                        }
                    }
                });
                if opts.command == "merge.hdr" {
                    ui.add_enabled_ui(opts.deghost != "none", |ui| {
                        crate::widgets::checkbox(ui, "merge.ghostOverlay", &mut opts.show_overlay, "Show Deghost Overlay");
                    });
                }
                if opts.command == "merge.hdrPanorama" {
                    let mut b = opts.bracket as f64;
                    if num(ui, &BRACKET, &mut b, crate::widgets::numeric_owner(app, (&opts.command, &app.session.selection.ids))) {
                        opts.bracket = b.round() as u32;
                    }
                }
                ui.add_space(4.0);
            }
            crate::widgets::checkbox(ui, "merge.autoSettings", &mut opts.auto_settings, "Auto Settings");
            crate::widgets::checkbox(ui, "merge.stack", &mut opts.stack, "Create Stack");
            ui.add_space(8.0);
            ui.label(egui::RichText::new(li_i18n::tr_format!("{} photos", app.merge.ids.len())).color(t.text_dim));
        });
    });
}

const BOUNDARY_WARP: lightcraft_develop::ControlSpec = lightcraft_develop::ControlSpec {
    id: "merge.boundaryWarp",
    label: "Boundary Warp",
    section: lightcraft_develop::Section::Light,
    min: 0.0,
    max: 100.0,
    default: 0.0,
    step: 1.0,
    decimals: 0,
    track: lightcraft_develop::Track::Plain,
};
const BRACKET: lightcraft_develop::ControlSpec = lightcraft_develop::ControlSpec {
    id: "merge.bracket",
    label: "Photos per bracket (0 = auto)",
    section: lightcraft_develop::Section::Light,
    min: 0.0,
    max: 9.0,
    default: 0.0,
    step: 1.0,
    decimals: 0,
    track: lightcraft_develop::Track::Plain,
};

fn num(ui: &mut egui::Ui, spec: &lightcraft_develop::ControlSpec, v: &mut f64, owner: egui::Id) -> bool {
    match crate::widgets::numeric_slider(ui, spec, *v, true, None, owner).value {
        Some(n) => {
            *v = n;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests_pixelshift {
    use super::*;
    use crate::headless::HeadlessView;

    #[test]
    fn pixelshift_dialog_clicks_motion_overlay_and_stack_in_both_themes() {
        for dark in [true, false] {
            for panel_width in [250.0, 400.0] {
                let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
                app.ui.settings.gpu = false;
                app.ui.right_width = panel_width;
                app.merge.ids = (1..=4).map(PhotoId).collect();
                let mut tokens = if dark { crate::theme::Tokens::default() } else { crate::tests_develop_c::light() };
                tokens.panel_w = panel_width;
                app.set_host_theme(tokens);
                let mut opts = MergeDialog::for_command("merge.pixelShift");
                assert!(!opts.is_hdr() && !opts.is_pano());
                let mut view = HeadlessView::new();
                app.restyle(&view.ctx);
                let mut frame = 0;
                let mut draw = |events: Vec<egui::Event>, app: &mut LightcraftApp, opts: &mut MergeDialog, view: &mut HeadlessView| {
                    frame += 1;
                    let raw = HeadlessView::raw_input(egui::vec2(1100.0, 500.0), 1.0, frame as f64 / 60.0, events);
                    view.run(raw, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| body(app, ui, opts));
                        app.widgets = crate::widgets::take_registry(ui.ctx());
                    });
                };
                draw(vec![], &mut app, &mut opts, &mut view);
                for id in ["check:merge.motionOverlay", "check:merge.stack"] {
                    let pos = app.widgets.iter().find(|(name, _)| name == id).unwrap().1.center();
                    draw(
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
                        ],
                        &mut app,
                        &mut opts,
                        &mut view,
                    );
                    draw(
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        &mut app,
                        &mut opts,
                        &mut view,
                    );
                }
                assert!(opts.show_overlay && opts.stack);
                let image = view.paint(&std::collections::HashMap::new());
                let rgba = li_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(format!("pixelshift-{}-{panel_width}.png", if dark { "dark" } else { "light" })), bytes).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests_batch {
    use super::*;
    use crate::headless::HeadlessView;

    fn batch_test_task() -> (MergeTask, SyncSender<BatchEvent>) {
        let opts = MergeDialog { auto_brackets: true, ..Default::default() };
        let (kind, finish) = lightcraft_engine::merge::parse(&opts.command, &opts.params()).unwrap();
        let (_, rx) = channel();
        let (tx, batch_rx) = sync_channel(1);
        let task = MergeTask {
            preview: false,
            job: MergeJob { kind, finish, sources: Vec::new(), preview: None, read: Arc::new(|_| Err("test must not read photos".into())) },
            options: opts,
            progress: Arc::new(Mutex::new((0.5, "HDR set 2 of 4".into()))),
            cancel: Arc::new(AtomicBool::new(false)),
            rx,
            batch_rx: Some(batch_rx),
            batch_results: vec![json!({"id": 1})],
            library_generation: 0,
        };
        (task, tx)
    }

    #[test]
    fn hdr_batch_dialog_clicks_grouping_overlay_and_cancel_in_both_themes() {
        for dark in [true, false] {
            for panel_width in [250.0, 400.0] {
                for pixel_ratio in [1.0, 1.5] {
                    let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
                    app.ui.settings.gpu = false;
                    app.merge.ids = (1..=12).map(PhotoId).collect();
                    let mut tokens = if dark { crate::theme::Tokens::default() } else { crate::tests_develop_c::light() };
                    tokens.panel_w = panel_width;
                    app.set_host_theme(tokens);
                    let mut opts = MergeDialog { deghost: "medium".into(), ..Default::default() };
                    let (task, _keep_channel_connected) = batch_test_task();
                    let mut view = HeadlessView::new();
                    app.restyle(&view.ctx);
                    let mut frame = 0;
                    let mut draw = |events, app: &mut LightcraftApp, opts: &mut MergeDialog, view: &mut HeadlessView| {
                        frame += 1;
                        let raw = HeadlessView::raw_input(egui::vec2(1100.0, 650.0), pixel_ratio, frame as f64 / 60.0, events);
                        view.run(raw, |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                body(app, ui, opts);
                                ui.set_max_width(tokens.panel_w);
                                batch_progress(ui, &task);
                            });
                            app.widgets = crate::widgets::take_registry(ctx);
                        });
                    };
                    draw(vec![], &mut app, &mut opts, &mut view);
                    for id in ["check:merge.autoBrackets", "check:merge.ghostOverlay", "button:merge.batchCancel"] {
                        let rect = app.widgets.iter().find(|(name, _)| name == id).unwrap().1;
                        assert!(rect.max.x <= 1100.0 && rect.max.y <= 650.0, "{id} must fit");
                        let pos = rect.center();
                        draw(
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                            &mut app,
                            &mut opts,
                            &mut view,
                        );
                        draw(
                            vec![egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            }],
                            &mut app,
                            &mut opts,
                            &mut view,
                        );
                    }
                    assert!(opts.auto_brackets && opts.show_overlay);
                    assert!(task.cancel.load(Ordering::Relaxed));
                    let image = view.paint(&std::collections::HashMap::new());
                    let rgba = li_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
                    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system");
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(dir.join(format!("hdr-batch-{}-{panel_width}-{pixel_ratio}.png", if dark { "dark" } else { "light" })), bytes)
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn hdr_batch_old_dialog_settings_keep_single_merge_default() {
        let opts: MergeDialog = serde_json::from_value(json!({"command": "merge.hdr", "deghost": "high"})).unwrap();
        assert!(!opts.auto_brackets);
        assert!(opts.align);
        assert_eq!(opts.deghost, "high");
    }

    #[test]
    fn hdr_batch_poll_reports_partial_completion_and_rejects_old_library() {
        let ctx = egui::Context::default();
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
        let (task, tx) = batch_test_task();
        app.merge.final_task = Some(task);
        assert!(tx.send(BatchEvent::Finished(Ok(json!({"groups": 2, "completed": 1, "failed": [[2, "bad frame"]], "skipped": [3]})))).is_ok());
        poll_batch(&mut app, &ctx);
        assert!(app.merge.final_task.is_none());
        let result = app.merge.last_result.as_ref().unwrap().clone();
        assert_eq!(result["completed"], 1);
        assert_eq!(result["results"][0]["id"], 1);
        assert_eq!(result["failed"].as_array().unwrap().len(), 1);
        let (task, _tx) = batch_test_task();
        let cancel = task.cancel.clone();
        app.merge.final_task = Some(task);
        app.session.library_generation += 1;
        poll_batch(&mut app, &ctx);
        assert!(app.merge.final_task.is_none());
        assert!(cancel.load(Ordering::Relaxed));
        assert_eq!(app.merge.last_result.as_ref().unwrap(), &result);
    }

    #[test]
    fn hdr_batch_rejected_prepared_files_are_removed_on_cancel_and_library_change() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/hdr-merge")
            .join(format!("ui-prepared-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for changed_library in [false, true] {
            let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), crate::Services::default());
            let (mut task, tx) = batch_test_task();
            task.job.finish.auto_settings = false;
            task.job.sources = vec![(PhotoId(1), dir.join("source.dng").to_string_lossy().into_owned())];
            let prepared =
                task.job.prepare_output(MergeOutput { dng: b"synthetic queued merge".to_vec(), preview: None, crop: None, info: json!({}) }).unwrap();
            let path = prepared.path().to_string();
            assert!(std::path::Path::new(&path).exists());
            let (acknowledged, ack_rx) = sync_channel(1);
            assert!(tx.send(BatchEvent::Output { job: task.job.clone(), prepared: Box::new(prepared), acknowledged }).is_ok());
            let cancel = task.cancel.clone();
            app.merge.final_task = Some(task);
            if changed_library {
                app.session.library_generation += 1;
            } else {
                cancel.store(true, Ordering::Relaxed);
            }
            let mut view = HeadlessView::new();
            app.restyle(&view.ctx);
            view.run(HeadlessView::raw_input(egui::vec2(1100.0, 650.0), 1.0, 0.0, vec![]), |ui| poll_batch(&mut app, ui.ctx()));
            assert!(!std::path::Path::new(&path).exists(), "rejected prepared merge must not leave an orphan DNG");
            assert!(cancel.load(Ordering::Relaxed));
            if changed_library {
                assert!(app.merge.final_task.is_none());
                assert!(matches!(ack_rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)));
            } else {
                assert_eq!(ack_rx.try_recv().unwrap(), Err("cancelled".into()));
                assert!(tx.send(BatchEvent::Finished(Err("cancelled".into()))).is_ok());
                poll_batch(&mut app, &view.ctx);
                assert!(app.merge.final_task.is_none());
            }
            assert!(app.session.catalog.photo(PhotoId(1)).is_none());
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
