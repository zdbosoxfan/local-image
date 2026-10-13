//! Restoration preview: an immutable composite snapshot, a worker, and one new-layer edit.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, TextureHandle, vec2};
use image::{GrayImage, RgbaImage};
use li_ai::restoration::Task;
use li_ai::restoration::byo;

use crate::{PhotocraftApp, widgets};

enum Preview {
    Image(RgbaImage),
    Mask(GrayImage),
    Installed,
}
type ImageResult = Arc<Mutex<Option<Result<Preview, String>>>>;
const TASKS: [Task; 4] = [Task::Jpeg, Task::Scan, Task::Colourise, Task::OldPhoto];
fn caption(task: Task) -> &'static str {
    match task {
        Task::Jpeg => "Fix JPEG artefacts",
        Task::Scan => "Clean up scan",
        Task::Colourise => "Colourise",
        Task::OldPhoto => "Repair old photo",
        Task::Denoise => "Denoise",
    }
}
fn settings() -> li_ai::AiSettings {
    #[cfg(test)]
    if let Some(settings) = TEST_SETTINGS.with(|s| s.borrow().clone()) {
        return settings;
    }
    li_ai::AiSettings::load()
}
fn local_pin(task: Task) -> Option<li_seg::restoration::Pin> {
    match task {
        Task::Colourise => Some(li_seg::restoration::DD_COLOR),
        Task::OldPhoto => Some(li_seg::restoration::SCRATCHES),
        _ => None,
    }
}
type SourceKey = (u64, u64, Option<photocraft_doc::LayerId>);
struct Discovered {
    models: Vec<byo::Model>,
    checkpoints: Vec<String>,
    host: String,
}
type Discovery = Arc<Mutex<Option<Result<Discovered, String>>>>;

#[derive(Default)]
pub(crate) struct Runtime {
    open: bool,
    browser_child: bool,
    task: Task,
    key: Option<SourceKey>,
    source: Option<Arc<RgbaImage>>,
    result: Option<RgbaImage>,
    mask: Option<GrayImage>,
    colourise_after: bool,
    auto_preview: bool,
    repair: Option<(photocraft_engine::jobs::JobId, u64)>,
    before: Option<TextureHandle>,
    after: Option<TextureHandle>,
    pending: Option<ImageResult>,
    ctl: Option<li_ai::JobControl>,
    started: Option<Instant>,
    elapsed: Option<Duration>,
    message: String,
    split: f32,
    discovery: Option<Discovery>,
    byo_models: Vec<byo::Model>,
    byo_host: String,
    selected_byo: Option<byo::Model>,
    acks: byo::Acknowledgements,
    checkpoints: Vec<String>,
    checkpoint: String,
    model_dir: std::path::PathBuf,
    local_ready: Option<(Task, bool, Instant)>,
}

pub(crate) fn open(app: &mut PhotocraftApp) {
    app.restoration.open = true;
    app.restoration.browser_child = false;
    app.restoration.split = 0.5;
    app.restoration.model_dir = settings().model_dir();
    app.restoration.local_ready = None;
    app.restoration.acks = byo::Acknowledgements::load(&ack_path());
    sync_source(app);
}

pub(crate) fn is_open(app: &PhotocraftApp) -> bool {
    app.restoration.open
}

pub(crate) fn model_browser_is_modal(app: &PhotocraftApp) -> bool {
    app.restoration.open && app.restoration.browser_child
}

fn close(r: &mut Runtime) {
    stop(r);
    r.result = None;
    r.mask = None;
    r.after = None;
    r.open = false;
    r.auto_preview = false;
    r.browser_child = false;
}

/// Resolve Escape before canvas/vector or generic-job handlers can consume it.
pub(crate) fn handle_keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !is_open(app) || egui::Popup::is_any_open(ctx) {
        return;
    }
    let child = model_browser_is_modal(app) && crate::model_browser::is_open();
    let owner = egui::Id::new(if child { "restoration-model-browser" } else { "restoration-dialog" });
    if ctx.memory(|m| m.top_modal_layer().is_none_or(|layer| layer.id == owner)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        if child {
            crate::model_browser::close();
            app.restoration.browser_child = false;
        } else {
            close(&mut app.restoration);
        }
    }
}

fn key(app: &PhotocraftApp) -> Option<SourceKey> {
    app.session.active().map(|d| (d.doc.id.0, d.revision, d.active_layer))
}

fn sync_source(app: &mut PhotocraftApp) {
    let key = key(app);
    if key == app.restoration.key {
        return;
    }
    stop(&mut app.restoration);
    app.restoration.result = None;
    app.restoration.mask = None;
    app.restoration.after = None;
    app.restoration.before = None;
    app.restoration.message.clear();
    app.restoration.elapsed = None;
    app.restoration.source = match app.session.active().map(|d| photocraft_engine::ai_cmds::flatten_rgba(&d.doc)).transpose() {
        Ok(source) => source.map(Arc::new),
        Err(e) => {
            app.restoration.message = e.to_string();
            None
        }
    };
    app.restoration.key = key;
}

fn stop(r: &mut Runtime) {
    if let Some(ctl) = r.ctl.take() {
        ctl.cancel();
    }
    r.pending = None;
    r.started = None;
}

fn start(app: &mut PhotocraftApp, ctx: &egui::Context) {
    sync_source(app);
    let r = &mut app.restoration;
    let Some(source) = r.source.clone() else { return };
    r.message.clear();
    r.result = None;
    r.mask = None;
    r.after = None;
    r.elapsed = None;
    #[cfg(test)]
    if let Some(result) = TEST_RESULT.with(|r| r.borrow_mut().take()) {
        match result {
            Ok(image) => r.result = Some(image),
            Err(error) => r.message = error,
        }
        return;
    }
    #[cfg(test)]
    if let Some(mask) = TEST_MASK.with(|r| r.borrow_mut().take()) {
        receive(r, Preview::Mask(mask));
        return;
    }
    let task = r.task;
    let byo_model = r.selected_byo.clone();
    let acks = r.acks.clone();
    let checkpoint = r.checkpoint.clone();
    let settings = settings();
    let client = li_ai::service().client;
    let ctl = li_ai::JobControl::new();
    let pending: ImageResult = Arc::new(Mutex::new(None));
    let out = pending.clone();
    let control = ctl.clone();
    let ctx = ctx.clone();
    let worker = std::thread::Builder::new().name("photo-restoration".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let result = if let Some(model) = byo_model {
            byo::restore(&client, &settings, &model, &acks, &source, &checkpoint, &control).map(Preview::Image)
        } else if task == Task::Colourise {
            control.set_stage(li_ai::comfy::Stage::Running);
            li_seg::restoration::colourise(&settings.model_dir(), &source, &|| !control.is_cancelled()).map(Preview::Image)
        } else if task == Task::OldPhoto {
            control.set_stage(li_ai::comfy::Stage::Running);
            li_seg::restoration::scratches(&settings.model_dir(), &source, &|| !control.is_cancelled()).map(Preview::Mask)
        } else {
            li_ai::restoration::restore(&client, &settings, task, &source, &control).map(Preview::Image)
        }
        .map_err(|e| format!("{e:#}"));
        if let Ok(mut slot) = out.lock() {
            *slot = Some(result);
        }
        ctx.request_repaint();
    });
    match worker {
        Ok(_) => {
            r.pending = Some(pending);
            r.ctl = Some(ctl);
            r.started = Some(Instant::now());
        }
        Err(e) => r.message = e.to_string(),
    }
}

fn poll(r: &mut Runtime) {
    let result = r.pending.as_ref().and_then(|p| p.lock().ok().and_then(|mut p| p.take()));
    if let Some(result) = result {
        r.pending = None;
        r.ctl = None;
        r.elapsed = r.started.take().map(|s| s.elapsed());
        match result {
            Ok(preview) => receive(r, preview),
            Err(error) => r.message = error,
        }
    }
}

fn receive(r: &mut Runtime, preview: Preview) {
    match preview {
        Preview::Image(image) => r.result = Some(image),
        Preview::Mask(mask) => {
            if !mask.pixels().any(|p| p[0] > 0) {
                r.message = tl!("No scratches detected.").into();
                return;
            }
            if let Some(source) = &r.source {
                let mut overlay = source.as_ref().clone();
                for (p, m) in overlay.pixels_mut().zip(mask.pixels()) {
                    if m[0] > 0 {
                        p[0] = (u16::from(p[0]) / 2 + 127) as u8;
                        p[1] /= 2;
                        p[2] /= 2;
                    }
                }
                r.result = Some(overlay);
            }
            r.mask = Some(mask);
        }
        Preview::Installed => {
            r.message = tl!("Restore model installed.").into();
            r.local_ready = None;
        }
    }
}

fn install_local(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(pin) = local_pin(app.restoration.task) else { return };
    if let Some(path) = crate::ai_ui::pick_onnx() {
        let pending: ImageResult = Arc::new(Mutex::new(None));
        let output = pending.clone();
        let dir = settings().model_dir();
        let ctl = li_ai::JobControl::new();
        let control = ctl.clone();
        let ctx = ctx.clone();
        match std::thread::Builder::new().name("restore-model-import".into()).spawn(move || {
            let result = pin.install_checked(&path, &dir, &|| !control.is_cancelled()).map(|()| Preview::Installed).map_err(|e| format!("{e:#}"));
            if let Ok(mut slot) = output.lock() {
                *slot = Some(result);
            }
            ctx.request_repaint();
        }) {
            Ok(_) => {
                app.restoration.pending = Some(pending);
                app.restoration.ctl = Some(ctl);
                app.restoration.started = Some(Instant::now());
            }
            Err(error) => app.restoration.message = error.to_string(),
        }
    }
}

/// Called only after the existing Remove job actually completes successfully. Optional
/// Colourise opens a fresh preview of that repaired document; it never applies automatically.
pub(crate) fn repair_finished(app: &mut PhotocraftApp, event: &photocraft_engine::jobs::JobEvent) {
    if app.restoration.repair.is_some_and(|(job, _)| job == event.id) {
        let Some((_, doc)) = app.restoration.repair.take() else { return };
        if matches!(event.outcome, photocraft_engine::jobs::JobOutcome::Done(_)) && app.session.active().is_some_and(|d| d.doc.id.0 == doc) {
            open_colourise(app);
        }
    }
}
pub(crate) fn after_repair(app: &mut PhotocraftApp, result: &serde_json::Value, doc: u64) {
    if let Some(job) = result["job"].as_u64() {
        app.restoration.repair = Some((photocraft_engine::jobs::JobId(job), doc));
    } else {
        open_colourise(app);
    }
}
fn open_colourise(app: &mut PhotocraftApp) {
    app.restoration.task = Task::Colourise;
    app.restoration.selected_byo = None;
    open(app);
    app.restoration.auto_preview = true;
}

/// Uses the engine's existing pixel-format conversion and Session edit/undo transaction.
fn apply(app: &mut PhotocraftApp) -> Result<(), String> {
    if key(app) != app.restoration.key {
        return Err(tl!("The image changed. Preview restoration again.").into());
    }
    let image = app.restoration.result.as_ref().ok_or_else(|| tl!("Preview restoration first.").to_owned())?;
    if Some(image.dimensions()) != app.restoration.source.as_ref().map(|s| s.dimensions()) {
        return Err(tl!("Restoration returned unexpected dimensions.").into());
    }
    let label = app.restoration.selected_byo.as_ref().map(|m| m.kind.label()).unwrap_or(caption(app.restoration.task));
    let name = format!("{}: {}", tl!("Restoration"), tl!(label));
    app.session
        .edit(&name, |doc, active| {
            let layer = photocraft_engine::ai_cmds::layer_from_rgba(doc, &name, image, 0, 0)?;
            // The input is the visible composite, so the result belongs at the root top;
            // nesting it inside the selected group would apply that group's effects twice.
            // Hide the original composite to preserve partially transparent pixels exactly.
            // Their pixels, hierarchy and previous visibility all remain in the undo snapshot.
            for original in &mut doc.layers {
                original.visible = false;
            }
            let id = doc.insert_above(None, layer);
            *active = Some(id);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    app.restoration.open = false;
    Ok(())
}

fn texture(ctx: &egui::Context, image: &RgbaImage, slot: &mut Option<TextureHandle>, name: &str) -> egui::TextureId {
    slot.get_or_insert_with(|| {
        let thumb = image::imageops::thumbnail(image, 1400, 1000);
        let color = egui::ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw());
        ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
    })
    .id()
}

fn preview(r: &mut Runtime, ui: &mut egui::Ui) {
    let Some(source) = r.source.as_ref() else {
        ui.label(tl!("Open an image to restore."));
        return;
    };
    let before = texture(ui.ctx(), source, &mut r.before, "restoration-before");
    let after = r.result.as_ref().map(|im| texture(ui.ctx(), im, &mut r.after, "restoration-after"));
    ui.horizontal(|ui| {
        ui.label(tl!("Before"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(tl!("After"));
        });
    });
    let size = vec2(ui.available_width(), 330.0);
    let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let aspect = source.width() as f32 / source.height() as f32;
    let width = area.width().min(area.height() * aspect);
    let rect = egui::Rect::from_center_size(area.center(), vec2(width, width / aspect));
    widgets::checker(ui.painter(), rect, 8.0);
    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    ui.painter().image(before, rect, uv, Color32::WHITE);
    if let Some(after) = after {
        let divider = rect.left() + rect.width() * r.split;
        let clip = egui::Rect::from_min_max(egui::pos2(divider, rect.top()), rect.max);
        ui.painter().with_clip_rect(clip.intersect(ui.clip_rect())).image(after, rect, uv, Color32::WHITE);
        ui.painter().vline(divider, rect.y_range(), egui::Stroke::new(2.0, Color32::WHITE));
        ui.add(egui::Slider::new(&mut r.split, 0.0..=1.0).text(tl!("Before / after split")).show_value(false));
    }
}

pub(crate) fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.restoration.open {
        return;
    }
    sync_source(app);
    if std::mem::take(&mut app.restoration.auto_preview) {
        start(app, ctx);
    }
    poll(&mut app.restoration);
    poll_discovery(&mut app.restoration);
    if app.restoration.browser_child {
        if crate::model_browser::is_open() {
            return;
        }
        app.restoration.browser_child = false;
    }
    let mut run = false;
    let mut apply_result = false;
    let mut install = false;
    let mut cancel = false;
    let screen = ctx.content_rect().size();
    let id = egui::Id::new("restoration-dialog");
    // Keep controls under the pointer while preview/discovery content changes height.
    // A centered, auto-shrinking area recenters between hover, press and release.
    let area = egui::Modal::default_area(id).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 24.0));
    let content_height = (screen.y - 160.0).clamp(100.0, 540.0);
    let modal = egui::Modal::new(id).area(area).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
        ui.set_width((screen.x - 48.0).clamp(280.0, 680.0));
        ui.heading(tl!("Restore Photo"));
        widgets::hairline(ui);
        let r = &mut app.restoration;
        let (pin, local_ready) = egui::ScrollArea::vertical()
            .id_salt("restoration-content")
            .min_scrolled_height(content_height)
            .max_height(content_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(tl!("Permissive models"));
                ui.label(tl!("Task"));
                ui.add_enabled_ui(r.pending.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for task in TASKS {
                            if ui.selectable_label(r.selected_byo.is_none() && r.task == task, tl!(caption(task))).clicked() {
                                r.task = task;
                                r.selected_byo = None;
                                r.result = None;
                                r.mask = None;
                                r.after = None;
                                r.message.clear();
                                r.elapsed = None;
                            }
                        }
                    });
                });
                let pin = r.selected_byo.is_none().then(|| local_pin(r.task)).flatten();
                if let Some(pin) = pin {
                    ui.hyperlink_to(pin.licence, pin.source);
                    ui.label(tl!("Runs on this computer with the installed Restore model."));
                    ui.label(tl!("Choose the approved ONNX model to install locally."));
                } else if r.selected_byo.is_none()
                    && let Ok(model) = r.task.model()
                {
                    ui.horizontal(|ui| {
                        ui.label(&model.label);
                        if let Some(url) = &model.license_url {
                            ui.hyperlink_to(model.license.as_deref().unwrap_or(""), url);
                        }
                    });
                }
                if r.selected_byo.is_none() && r.task == Task::OldPhoto {
                    ui.label(tl!("Preview marks detected scratches in red. Review and adjust the mask in AI Remove, then click Remove to fill it."));
                    ui.checkbox(&mut r.colourise_after, tl!("Colourise after repair"));
                } else {
                    ui.label(tl!("Original resolution. Apply creates a new layer and hides the source layers."));
                }
                let local_ready = if let Some(pin) = pin {
                    if r.local_ready.is_none_or(|(task, _, at)| task != r.task || at.elapsed() >= Duration::from_secs(1)) {
                        r.local_ready = Some((r.task, pin.installed(&r.model_dir), Instant::now()));
                    }
                    r.local_ready.is_some_and(|(_, ready, _)| ready)
                } else {
                    r.task.file().is_ok()
                };
                byo_ui(r, ui);
                preview(r, ui);
                if !r.message.is_empty() {
                    ui.label(RichText::new(&r.message).color(crate::theme::Tokens::get(ui.ctx()).danger));
                }
                if let Some(elapsed) = r.elapsed {
                    ui.label(format!("{}: {:.2} s", tl!("Preview time"), elapsed.as_secs_f64()));
                }
                if let Some(ctl) = &r.ctl {
                    let p = ctl.progress();
                    ui.label(format!("{} · {}", tl!("Running…"), p.stage.label()));
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                }
                (pin, local_ready)
            })
            .inner;
        widgets::hairline(ui);
        ui.horizontal_wrapped(|ui| {
            let ready = r.selected_byo.as_ref().map_or_else(|| local_ready, |m| m.adapter.is_some() && r.acks.contains(m, &r.byo_host));
            #[cfg(test)]
            let ready = ready || TEST_RESULT.with(|s| s.borrow().is_some()) || TEST_MASK.with(|s| s.borrow().is_some());
            run = ui.add_enabled(r.source.is_some() && ready && r.pending.is_none(), egui::Button::new(tl!("Preview restoration"))).clicked();
            let label = if r.mask.is_some() { tl!("Review scratches in AI Remove") } else { tl!("Apply as new layer") };
            apply_result = ui.add_enabled(r.result.is_some() && r.pending.is_none(), egui::Button::new(label)).clicked();
            install = ui
                .add_enabled(
                    r.pending.is_none(),
                    egui::Button::new(if pin.is_some() { tl!("Import Restore model…") } else { tl!("Install restoration models…") }),
                )
                .clicked();
            cancel = ui.button(tl!("Cancel")).clicked();
        });
    });
    #[cfg(test)]
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("restoration-modal-rect"), modal.response.rect));
    let escape = modal.is_top_modal && !modal.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if run {
        start(app, ctx);
    }
    if apply_result {
        if key(app) != app.restoration.key {
            app.restoration.message = tl!("The image changed. Preview restoration again.").into();
        } else if let Some(mask) = app.restoration.mask.take() {
            let colourise = app.restoration.colourise_after;
            match crate::ai_remove_ui::detected(app, mask, colourise) {
                Ok(()) => {
                    app.restoration.open = false;
                    app.restoration.result = None;
                    app.restoration.after = None;
                }
                Err(error) => app.restoration.message = error,
            }
        } else if let Err(e) = apply(app) {
            app.restoration.message = e;
        }
    }
    if install {
        if app.restoration.selected_byo.is_none() && local_pin(app.restoration.task).is_some() {
            install_local(app, ctx);
        } else {
            app.restoration.browser_child = true;
            crate::model_browser::open_restoration();
        }
    }
    // A click on the backdrop is inert; only explicit Cancel or owned Escape closes.
    if cancel || escape {
        close(&mut app.restoration);
    }
}

fn ack_path() -> std::path::PathBuf {
    #[cfg(test)]
    {
        TEST_ACK_PATH.with(|p| p.clone())
    }
    #[cfg(not(test))]
    {
        byo::Acknowledgements::path()
    }
}

fn discover(r: &mut Runtime, ctx: &egui::Context) {
    let slot: Discovery = Arc::new(Mutex::new(None));
    let output = slot.clone();
    let ctx = ctx.clone();
    let worker = std::thread::Builder::new().name("restoration-byo-discovery".into()).spawn(move || {
        let settings = settings();
        let client = li_ai::service().client;
        let result = client
            .object_info()
            .map(|info| {
                let files = byo::inventory(&client, &settings);
                let models = byo::detect(&info, &files);
                let checkpoints = info.choices("CheckpointLoaderSimple", "ckpt_name").into_iter().filter(|s| !byo::excluded(s)).collect();
                Discovered { models, checkpoints, host: client.host().into() }
            })
            .map_err(|e| format!("{e:#}"));
        if let Ok(mut o) = output.lock() {
            *o = Some(result);
        }
        ctx.request_repaint();
    });
    match worker {
        Ok(_) => r.discovery = Some(slot),
        Err(e) => r.message = e.to_string(),
    }
}

fn poll_discovery(r: &mut Runtime) {
    if let Some(result) = r.discovery.as_ref().and_then(|d| d.lock().ok().and_then(|mut o| o.take())) {
        r.discovery = None;
        match result {
            Ok(Discovered { models, checkpoints, host }) => {
                r.byo_models = models;
                r.checkpoints = checkpoints;
                r.byo_host = host;
                if r.selected_byo.as_ref().is_some_and(|m| !r.byo_models.contains(m)) {
                    r.selected_byo = None;
                    r.result = None;
                    r.mask = None;
                    r.after = None;
                }
            }
            Err(e) => r.message = e,
        }
    }
}

fn byo_ui(r: &mut Runtime, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(tl!("Bring your own model")).id_salt("restoration-byo").show(ui, |ui| {
        ui.label(tl!("Use models you installed in ComfyUI. Local Image does not download these models or install their nodes."));
        if ui.add_enabled(r.discovery.is_none() && r.pending.is_none(), egui::Button::new(tl!("Detect installed models"))).clicked() {
            discover(r, ui.ctx());
        }
        if r.discovery.is_some() {
            ui.spinner();
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if r.byo_models.is_empty() {
            ui.label(tl!("No supported user-installed models detected."));
        }
        egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
            for model in &r.byo_models {
                if ui.add_enabled(r.pending.is_none(), egui::Button::selectable(r.selected_byo.as_ref() == Some(model), model.label())).clicked() {
                    r.selected_byo = Some(model.clone());
                    r.result = None;
                    r.mask = None;
                    r.after = None;
                    r.message.clear();
                    r.elapsed = None;
                }
                let (name, url) = model.kind.licence();
                ui.hyperlink_to(name, url);
                ui.label(tl!(byo::NOTICE));
                if model.kind == byo::Kind::RestoreFormerPlus {
                    ui.label(tl!(byo::DATA_NOTICE));
                }
                if !model.reason.is_empty() {
                    ui.label(&model.reason);
                }
                ui.separator();
            }
        });
        if let Some(model) = &r.selected_byo {
            let mut ack = r.acks.contains(model, &r.byo_host);
            if ui.add_enabled(!ack, egui::Checkbox::new(&mut ack, tl!("I have read the licence and understand the commercial-use restrictions"))).changed()
                && ack
                && let Err(e) = r.acks.acknowledge(model, &r.byo_host, &ack_path())
            {
                r.message = e.to_string();
            }
            if model.kind == byo::Kind::StableSr {
                ui.label(tl!("Choose your installed StableSR-compatible diffusion checkpoint"));
                egui::ComboBox::from_id_salt("restoration-stablesr-checkpoint").selected_text(&r.checkpoint).show_ui(ui, |ui| {
                    for ckpt in &r.checkpoints {
                        if ui.selectable_value(&mut r.checkpoint, ckpt.clone(), ckpt).changed() {
                            r.result = None;
                            r.mask = None;
                            r.after = None;
                        }
                    }
                });
            }
        }
    });
}

#[cfg(test)]
thread_local! {
    static TEST_MASK: std::cell::RefCell<Option<GrayImage>> = const { std::cell::RefCell::new(None) };
    static TEST_RESULT: std::cell::RefCell<Option<Result<RgbaImage, String>>> = const { std::cell::RefCell::new(None) };
    static TEST_SETTINGS: std::cell::RefCell<Option<li_ai::AiSettings>> = const { std::cell::RefCell::new(None) };
    static TEST_ACK_PATH: std::path::PathBuf = std::env::temp_dir().join(format!("li-restoration-acks-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
}

#[cfg(test)]
mod tests;
