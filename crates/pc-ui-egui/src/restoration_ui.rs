//! Restoration preview: an immutable composite snapshot, a worker, and one new-layer edit.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, TextureHandle, vec2};
use image::RgbaImage;
use li_ai::restoration::Task;
use li_ai::restoration::byo;

use crate::{PhotocraftApp, widgets};

type ImageResult = Arc<Mutex<Option<Result<RgbaImage, String>>>>;
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
    task: Task,
    key: Option<SourceKey>,
    source: Option<Arc<RgbaImage>>,
    result: Option<RgbaImage>,
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
}

pub(crate) fn open(app: &mut PhotocraftApp) {
    app.restoration.open = true;
    app.restoration.split = 0.5;
    app.restoration.acks = byo::Acknowledgements::load(&ack_path());
    sync_source(app);
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
    app.restoration.after = None;
    app.restoration.before = None;
    app.restoration.message.clear();
    app.restoration.elapsed = None;
    app.restoration.source = app.session.active().map(|d| Arc::new(photocraft_engine::ai_cmds::flatten_rgba(&d.doc)));
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
    let task = r.task;
    let byo_model = r.selected_byo.clone();
    let acks = r.acks.clone();
    let checkpoint = r.checkpoint.clone();
    let settings = li_ai::AiSettings::load();
    #[cfg(test)]
    let settings = TEST_SETTINGS.with(|s| s.borrow().clone()).unwrap_or(settings);
    let client = li_ai::service().client;
    let ctl = li_ai::JobControl::new();
    let pending: ImageResult = Arc::new(Mutex::new(None));
    let out = pending.clone();
    let control = ctl.clone();
    let ctx = ctx.clone();
    let worker = std::thread::Builder::new().name("photo-restoration".into()).spawn(move || {
        let result = if let Some(model) = byo_model {
            byo::restore(&client, &settings, &model, &acks, &source, &checkpoint, &control)
        } else {
            li_ai::restoration::restore(&client, &settings, task, &source, &control)
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
            Ok(image) => r.result = Some(image),
            Err(error) => r.message = error,
        }
    }
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
    let label = app.restoration.selected_byo.as_ref().map(|m| m.kind.label()).unwrap_or(app.restoration.task.label());
    let name = format!("{}: {}", tl!("Restoration"), tl!(label));
    app.session
        .edit(&name, |doc, active| {
            let layer = photocraft_engine::ai_cmds::layer_from_rgba(doc, &name, image, 0, 0);
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
    poll(&mut app.restoration);
    poll_discovery(&mut app.restoration);
    let mut open = true;
    let mut run = false;
    let mut apply_result = false;
    let mut install = false;
    let mut cancel = false;
    egui::Window::new(tl!("Restore Photo"))
        .id(egui::Id::new("restoration-dialog"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .vscroll(true)
        .max_height((ctx.content_rect().height() - 80.0).max(400.0))
        .default_width(680.0)
        .default_height(700.0)
        .default_pos(egui::pos2(220.0, 50.0))
        .show(ctx, |ui| {
            let r = &mut app.restoration;
            ui.label(tl!("Permissive models"));
            ui.label(tl!("Task"));
            ui.add_enabled_ui(r.pending.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for task in Task::ALL {
                        if ui.selectable_label(r.selected_byo.is_none() && r.task == task, tl!(task.label())).clicked() {
                            r.task = task;
                            r.selected_byo = None;
                            r.result = None;
                            r.after = None;
                            r.message.clear();
                            r.elapsed = None;
                        }
                    }
                });
            });
            if r.selected_byo.is_none()
                && let Ok(model) = r.task.model()
            {
                ui.horizontal(|ui| {
                    ui.label(&model.label);
                    if let Some(url) = &model.license_url {
                        ui.hyperlink_to(model.license.as_deref().unwrap_or(""), url);
                    }
                });
                ui.label(tl!("Original resolution. Apply creates a new layer and hides the source layers."));
            } else if r.selected_byo.is_none() {
                ui.label(if r.task == Task::Colourise {
                    tl!(li_ai::restoration::COLOURISE_UNAVAILABLE)
                } else {
                    tl!(li_ai::restoration::OLD_PHOTO_UNAVAILABLE)
                });
            }
            ui.horizontal(|ui| {
                let ready = r.selected_byo.as_ref().map_or_else(|| r.task.file().is_ok(), |m| m.adapter.is_some() && r.acks.contains(m, &r.byo_host));
                run = ui.add_enabled(r.source.is_some() && ready && r.pending.is_none(), egui::Button::new(tl!("Preview restoration"))).clicked();
                apply_result = ui.add_enabled(r.result.is_some() && r.pending.is_none(), egui::Button::new(tl!("Apply as new layer"))).clicked();
                install = ui.button(tl!("Install restoration models…")).clicked();
                cancel = ui.button(tl!("Cancel")).clicked();
            });
            if r.selected_byo.is_none() && r.task == Task::Colourise {
                ui.label(tl!(li_ai::restoration::COLOURISE_UNAVAILABLE));
            }
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
        });
    if run {
        start(app, ctx);
    }
    if apply_result && let Err(e) = apply(app) {
        app.restoration.message = e;
    }
    if install {
        crate::model_browser::open_restoration();
    }
    if !open || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        stop(&mut app.restoration);
        app.restoration.result = None;
        app.restoration.after = None;
        app.restoration.open = false;
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
        let settings = li_ai::AiSettings::load();
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
    static TEST_RESULT: std::cell::RefCell<Option<Result<RgbaImage, String>>> = const { std::cell::RefCell::new(None) };
    static TEST_SETTINGS: std::cell::RefCell<Option<li_ai::AiSettings>> = const { std::cell::RefCell::new(None) };
    static TEST_ACK_PATH: std::path::PathBuf = std::env::temp_dir().join(format!("li-restoration-acks-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
}

#[cfg(test)]
mod tests;
