//! Photo Merge in the UI: the HDR / Panorama / HDR Panorama dialog (options + live preview) and
//! the background merge jobs.
//!
//! Previews and the final merge run [`lightcraft_engine::merge::MergeJob`]s on worker threads with
//! progress and cancellation; the window stays responsive. Changing an option cancels the running
//! preview and starts a new one. The final merge keeps running after the dialog closes (progress
//! in a toast); when it finishes, the result is written, imported and selected
//! ([`lightcraft_engine::Session::finish_merge`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use lightcraft_catalog::PhotoId;
use lightcraft_engine::merge::{MergeJob, MergeOutput};
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
    /// The last finished merge (`{id, path, …}`).
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
    let job = app.session.plan_merge(kind, finish, ids, preview).map_err(|e| e.to_string())?;
    let progress = Arc::new(Mutex::new((0.0, "Starting".to_string())));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let (j, p, c) = (job.clone(), progress.clone(), cancel.clone());
    let work = move || {
        let run = || {
            j.run(&|f, stage| {
                if let Ok(mut g) = p.lock() {
                    *g = (f, stage.to_string());
                }
                !c.load(Ordering::Relaxed)
            })
        };
        // a panicking merge reports an error instead of leaving "Merging…" up forever
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err("the merge failed unexpectedly".into()));
        let _ = tx.send(r);
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("photo-merge".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    Ok(MergeTask { preview, job, options: options.clone(), progress, cancel, rx })
}

/// Open the merge dialog for the selection.
pub fn open(app: &mut LightcraftApp, command: &str) -> Result<Value, String> {
    let ids = app.session.targets(&json!({}));
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
    let ids = app.session.targets(&json!({}));
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
                        app.toast(ctx, crate::i18n::tr_format!("{what} merge added", what = what));
                    }
                    Err(e) => {
                        app.ui.status = e.to_string();
                        app.toast(ctx, crate::i18n::tr_format!("{what} merge failed: {e}", e = e, what = what));
                    }
                }
            }
            Err(_) => {
                let (f, stage) = t.progress.lock().map(|g| g.clone()).unwrap_or_default();
                let now = ctx.input(|i| i.time);
                app.ui.toast = Some((crate::i18n::tr_format!("Merging… {stage} {:.0}%", f * 100.0, stage = stage), now + 0.5));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
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
                    crate::i18n::tr_format!("{stage}… {:.0}%", f * 100.0, stage = stage),
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
                        crate::i18n::tr_format!("{} photos", info["shots"].as_u64().unwrap_or(0)),
                        info["motionPixels"].as_u64().unwrap_or(0),
                        crate::i18n::tr("pixels use motion fallback")
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
            }
        });
        ui.add_space(12.0);
        // options
        ui.vertical(|ui| {
            ui.set_width(if opts.command == "merge.pixelShift" { t.panel_w } else { 220.0 });
            if opts.command == "merge.pixelShift" {
                ui.label(egui::RichText::new(crate::i18n::tr("Measured full colour; 16 shots double width and height.")).color(t.text_label));
                if app.merge.ids.len() == 1 {
                    ui.label(
                        egui::RichText::new(crate::i18n::tr(
                            "Composed ARQ keeps its existing pixels. Motion correction requires the original shots.",
                        ))
                        .color(t.text_dim),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("Moving areas use the first shot. Capture order comes from Sony metadata."))
                            .color(t.text_dim),
                    );
                    crate::widgets::checkbox(ui, "merge.motionOverlay", &mut opts.show_overlay, "Show Motion Overlay");
                }
            }
            if opts.is_pano() {
                ui.label(egui::RichText::new(crate::i18n::tr("Projection")).color(t.text_label));
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
                ui.checkbox(&mut opts.fill_edges, crate::i18n::tr("Fill Edges"));
                ui.checkbox(&mut opts.auto_crop, crate::i18n::tr("Auto Crop"));
                ui.add_space(4.0);
            }
            if opts.is_hdr() {
                ui.checkbox(&mut opts.align, crate::i18n::tr("Auto Align"));
                ui.label(egui::RichText::new(crate::i18n::tr("Deghost Amount")).color(t.text_label));
                ui.horizontal(|ui| {
                    for (i, (v, l)) in [("none", "None"), ("low", "Low"), ("medium", "Med"), ("high", "High")].iter().enumerate() {
                        if crate::widgets::text_button(ui, &format!("mergeDeghost-{i}"), l, opts.deghost == *v).clicked() {
                            opts.deghost = v.to_string();
                        }
                    }
                });
                if opts.command == "merge.hdr" {
                    ui.add_enabled_ui(opts.deghost != "none", |ui| ui.checkbox(&mut opts.show_overlay, crate::i18n::tr("Show Deghost Overlay")));
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
            ui.label(egui::RichText::new(crate::i18n::tr_format!("{} photos", app.merge.ids.len())).color(t.text_dim));
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
                let rgba = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(format!("pixelshift-{}-{panel_width}.png", if dark { "dark" } else { "light" })), bytes).unwrap();
            }
        }
    }
}
