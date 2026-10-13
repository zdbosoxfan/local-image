//! Shared Remove Background settings and dispatch for all three editor surfaces.

use egui::vec2;
use li_ai::catalog::ModelId;
use serde_json::{Value, json};

use crate::{PhotocraftApp, widgets};

pub(crate) const MODELS: [(&str, &str); 4] =
    [("fast", "Fast"), ("better", "Better Quality"), ("qwen-int8", "Qwen AI · Compact"), ("qwen-bf16", "Qwen AI · Full")];

pub(crate) fn deserialize_engine<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(d)?;
    Ok(if value == "quick" { "fast".into() } else { value })
}

fn cpu(engine: &str) -> bool {
    matches!(engine, "fast" | "better" | "quick")
}

fn tier(ai: &crate::ai_ui::AiOptions) -> Option<&'static li_seg::ModelSpec> {
    if cpu(&ai.cutout_engine) { li_seg::spec(if ai.cutout_engine == "better" { "inspyrenet" } else { "rmbg-1.4" }) } else { None }
}

fn tooltip(engine: &str) -> String {
    if cpu(engine) {
        let better = engine == "better";
        let seconds = li_ai::ops::cutout_seconds(better).unwrap_or(if better { 15.0 } else { 3.0 });
        format!("About {seconds:.1} s per photo on this computer. Runs locally on CPU, including on Apple Silicon.")
    } else {
        "Needs a capable NVIDIA GPU and ComfyUI; does not run on a Mac. About 30–120 s per photo, depending on GPU and image size.".into()
    }
}
// Shared option metadata follows the model catalogue's dynamic help-text pattern.
// Reuse translated field names; new explanatory prose keeps the English fallback
// while the repository's language work is paused.
const EDGE_REFINEMENT: (&str, &str) = ("Refine", "Resolve soft hair and fur edges against the original photo. Adds processing time.");
const COLOUR_CLEANUP: (&str, &str) = ("Color Decontaminate", "Replace background colour spill along the subject edge with nearby foreground colours.");

const OUTPUTS: [(&str, &str); 6] = [
    ("mask", "Layer mask"),
    ("transparent", "Transparent"),
    ("white", "White background"),
    ("color", "Colour background"),
    ("blur", "Blur background"),
    ("newLayer", "Cutout on a new layer"),
];

/// Build the same command request regardless of which surface initiated removal.
pub(crate) fn request(ai: &crate::ai_ui::AiOptions, layer: Option<u64>) -> (&'static str, Value) {
    let standard = cpu(&ai.cutout_engine);
    let mut p = json!({
        "engine": if standard { "learned" } else { &ai.cutout_engine },
        "output": ai.cutout_output,
        "refine": ai.cutout_refine,
        "refineEdges": ai.cutout_refine,
        "decontaminate": ai.cutout_decontaminate,
        "tier": ai.cutout_engine,
    });
    if let Some(layer) = layer {
        p["layer"] = json!(layer);
    }
    if !standard {
        p["hint"] = json!(ai.cutout_hint);
    }
    match ai.cutout_output.as_str() {
        "color" => p["color"] = json!(ai.cutout_color),
        "blur" => p["amount"] = json!(ai.cutout_blur),
        _ => {}
    }
    (if standard { "layer.removeBackground" } else { "ai.removeBackground" }, p)
}

pub(crate) fn run(app: &mut PhotocraftApp, layer: Option<u64>) -> Result<Value, String> {
    #[cfg(test)]
    {
        let (cmd, p) = request(&app.ui.ai, layer);
        if let Some(result) = tests::injected_result(app, cmd, &p) {
            return result;
        }
    }
    if !can_remove(app) {
        return Err("Select an unlocked pixel layer and install the selected model first.".into());
    }
    if cpu(&app.ui.ai.cutout_engine) {
        li_seg::set_subject_quality(&photocraft_engine::seg::models_dir(), app.ui.ai.cutout_engine == "better");
    }
    let options = app.ui.ai.clone();
    let state = app.session.active().ok_or("No document open")?;
    let id = layer.map(photocraft_doc::LayerId).or(state.active_layer).ok_or("No layer selected")?;
    photocraft_engine::jobs::edit_job(
        &mut app.session,
        "Remove Background",
        move |doc, active, ctx| {
            let result = apply_cutout(doc, id, &options, ctx)?;
            *active = Some(result);
            Ok(result)
        },
        |id| json!({"layer": id.0}),
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn model_ready(ai: &crate::ai_ui::AiOptions) -> bool {
    if let Some(spec) = tier(ai) {
        li_seg::installed_bytes(&photocraft_engine::seg::models_dir(), spec) == Some(spec.bytes)
    } else {
        crate::ai_ui::status().ready(ModelId::Qwen, if ai.cutout_engine == "qwen-bf16" { "bf16" } else { "int8" }).is_ok()
    }
}

pub(crate) fn can_remove(app: &PhotocraftApp) -> bool {
    let cmd = if cpu(&app.ui.ai.cutout_engine) { "layer.removeBackground" } else { "ai.removeBackground" };
    model_ready(&app.ui.ai)
        && app.session.is_enabled(cmd)
        && app.session.active().and_then(|d| d.active_layer.map(|id| d.doc.effective_locks(id))).is_some_and(|locks| !locks.pixels && !locks.all)
}

/// The existing verified download flow, inline at first use and in Local AI.
pub(crate) fn tier_download(ui: &mut egui::Ui, spec: &'static li_seg::ModelSpec) {
    // Styled as Local AI's model rows (`ai_ui::model_function_row`).
    let t = crate::theme::Tokens::get(ui.ctx());
    let dir = photocraft_engine::seg::models_dir();
    ui.label(
        egui::RichText::new(format!("{} · CPU · {} · {}", spec.label, li_ai::download::human_bytes(spec.bytes), spec.licence)).color(t.text_faint).size(11.0),
    );
    if li_seg::installed_bytes(&dir, spec) == Some(spec.bytes) {
        return;
    }
    let downloads = crate::ai_ui::downloads();
    if let Some(d) = downloads.get(&format!("seg:{}", spec.id)) {
        if !d.finished {
            ui.horizontal(|ui| {
                let frac = d.done as f32 / d.total.max(1) as f32;
                ui.add(egui::ProgressBar::new(frac).desired_width(140.0).text(format!("{:.0}%", frac * 100.0)));
                if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                    d.ctl.cancel();
                }
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        if let Some(error) = &d.error {
            ui.label(egui::RichText::new(error).color(t.danger).size(11.5));
        }
    }
    if widgets::secondary_button(ui, &format!("Download {} ({})", spec.label, li_ai::download::human_bytes(spec.bytes)), 0.0).clicked() {
        crate::ai_ui::start_seg_download(spec);
    }
}

pub(crate) fn readiness(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if let Some(spec) = tier(&app.ui.ai) {
        tier_download(ui, spec);
    } else {
        let variant = if app.ui.ai.cutout_engine == "qwen-bf16" { "bf16" } else { "int8" };
        crate::ai_ui::readiness(app, ui, ModelId::Qwen, variant);
    }
}

/// Inline controls for the options bar; field rows for Properties and the task-bar popup.
pub(crate) fn settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, inline: bool) {
    if app.ui.ai.cutout_engine == "quick" {
        app.ui.ai.cutout_engine = "fast".into();
    }
    let tips: Vec<_> = MODELS.iter().map(|(k, _)| tooltip(k)).collect();
    let models: Vec<_> = MODELS.iter().zip(&tips).map(|((k, l), tip)| ((*k).to_owned(), *l, tip.as_str())).collect();
    let outputs: Vec<_> = OUTPUTS.iter().map(|(k, l)| ((*k).to_owned(), *l)).collect();
    let mut fields = |ui: &mut egui::Ui| {
        row(ui, inline, tl!("Model"), |ui, width| {
            widgets::dropdown_with_tooltips(ui, "cutout-model", &mut app.ui.ai.cutout_engine, &models, width);
            if cpu(&app.ui.ai.cutout_engine) {
                li_seg::set_subject_quality(&photocraft_engine::seg::models_dir(), app.ui.ai.cutout_engine == "better");
            }
        });
        if !cpu(&app.ui.ai.cutout_engine) {
            row(ui, inline, tl!("Keep…"), |ui, width| {
                let r = ui.add(egui::TextEdit::singleline(&mut app.ui.ai.cutout_hint).hint_text(tl!("Keep… (optional)")).desired_width(width));
                r.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, ui.is_enabled(), tl!("Keep… (optional)"));
                    info.current_text_value = Some(app.ui.ai.cutout_hint.clone());
                    info
                });
            });
        }
        widgets::checkbox(ui, &mut app.ui.ai.cutout_refine, EDGE_REFINEMENT.0).on_hover_text(tl!(EDGE_REFINEMENT.1));
        widgets::checkbox(ui, &mut app.ui.ai.cutout_decontaminate, COLOUR_CLEANUP.0).on_hover_text(tl!(COLOUR_CLEANUP.1));
        row(ui, inline, tl!("Output"), |ui, width| {
            widgets::dropdown(ui, "cutout-output", &mut app.ui.ai.cutout_output, &outputs, width);
        });
        match app.ui.ai.cutout_output.as_str() {
            "color" => row(ui, inline, tl!("Colour"), |ui, _| {
                let r = ui.color_edit_button_rgb(&mut app.ui.ai.cutout_color).on_hover_text(tl!("Background colour"));
                r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, true, tl!("Background colour")));
            }),
            "blur" => row(ui, inline, tl!("Amount"), |ui, width| {
                let r = widgets::value_field(ui, &mut app.ui.ai.cutout_blur, 0.0..=1000.0, "px", width);
                r.widget_info(|| {
                    let mut info = egui::WidgetInfo::drag_value(ui.is_enabled(), app.ui.ai.cutout_blur as f64);
                    info.label = Some(tl!("Background blur amount").to_owned());
                    info
                });
            }),
            _ => {}
        }
    };
    if inline {
        fields(ui);
    } else {
        ui.push_id("background-settings", fields);
    }
}

/// The task-bar menu uses direct choices so it never nests a ComboBox inside another popup.
pub(crate) fn menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    widgets::section_label(ui, tl!("Model"));
    for (key, label) in MODELS {
        if ui.selectable_label(app.ui.ai.cutout_engine == key, tl!(label)).on_hover_text(tooltip(key)).clicked() {
            app.ui.ai.cutout_engine = key.into();
            if cpu(key) {
                li_seg::set_subject_quality(&photocraft_engine::seg::models_dir(), key == "better");
            }
            ui.close();
        }
    }
    widgets::hairline(ui);
    widgets::section_label(ui, tl!("Output"));
    for (key, label) in OUTPUTS {
        if ui.selectable_label(app.ui.ai.cutout_output == key, tl!(label)).clicked() {
            app.ui.ai.cutout_output = key.into();
            ui.close();
        }
    }
}

fn row(ui: &mut egui::Ui, inline: bool, label: &str, add: impl FnOnce(&mut egui::Ui, f32)) {
    if inline {
        ui.label(label);
        add(ui, 150.0);
    } else {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(48.0, 22.0), egui::Label::new(label));
            let width = (ui.available_width() - 4.0).max(80.0);
            add(ui, width);
        });
        ui.add_space(crate::theme::ROW_GAP);
    }
}

pub(crate) fn remove_button(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: Option<u64>, width: f32) {
    let clicked = ui.add_enabled_ui(can_remove(app), |ui| widgets::primary_button(ui, tl!("Remove Background"), width)).inner.clicked();
    if clicked && let Err(e) = run(app, layer) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

pub(crate) fn section(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: u64) {
    if crate::props_layout::section(ui, "remove-background", tl!("Remove Background")) {
        properties(app, ui, layer);
    }
}

pub(crate) fn properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: u64) {
    settings(app, ui, false);
    readiness(app, ui);
    ui.add_space(crate::theme::ROW_GAP);
    remove_button(app, ui, Some(layer), ui.available_width());
}

#[cfg(test)]
#[path = "background_ui_tests.rs"]
mod tests;

/// Shared single/batch processing. Uses the editor's profile-aware sampler, closed-form
/// matting and output implementation, keeping document changes in one undoable job.
pub(crate) fn apply_cutout(
    doc: &mut photocraft_doc::Document,
    id: photocraft_doc::LayerId,
    options: &crate::ai_ui::AiOptions,
    ctx: &photocraft_engine::jobs::JobCtx,
) -> photocraft_engine::Result<photocraft_doc::LayerId> {
    use photocraft_engine::{EngineError, ai_cmds, cutout_cmds};
    let bounds = doc.bounds();
    let mut layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
    let locks = doc.effective_locks(id);
    if locks.pixels || locks.all || layer.surface().is_none() {
        return Err(EngineError::Other("Select an unlocked pixel layer".into()));
    }
    layer.mask = None;
    layer.visible = true;
    layer.opacity = 1.0;
    let img = ai_cmds::selected_layer_rgba(doc, &layer, bounds)?;
    let variant = match options.cutout_engine.as_str() {
        "qwen-bf16" => "bf16",
        "qwen-int8" => "int8",
        "better" => "better",
        _ => "fast",
    };
    let hint = options.cutout_hint.chars().take(500).collect::<String>();
    let mut alpha = ai_cmds::bridged(ctx, |ctl| li_ai::service().cutout(&img, variant, &hint, li_ai::ops::new_seed(), ctl))?;
    ctx.check()?;
    if options.cutout_refine {
        ctx.progress(0.9, "Refining edges");
        alpha = refine_matte(&img, &alpha);
        ctx.check()?;
    }
    let values: Vec<f32> = alpha.pixels().map(|p| p[0] as f32 / 255.0).collect();
    let mut mask = photocraft_raster::Surface::new(photocraft_color::PixelFormat::GRAY8);
    mask.write_region(bounds, &values);
    let pixel_error = || EngineError::Other("Remove Background needs a pixel layer".into());
    let original_surface = if options.cutout_decontaminate && options.cutout_output == "newLayer" {
        Some(doc.layer(id).ok_or(EngineError::NoLayer(id))?.surface().ok_or_else(pixel_error)?.clone())
    } else {
        None
    };
    if options.cutout_decontaminate {
        let region = photocraft_algo::selection::Region { bbox: bounds, mask: alpha.into_raw() };
        let source = doc.layer(id).ok_or(EngineError::NoLayer(id))?.surface().ok_or_else(pixel_error)?;
        let clean = photocraft_algo::matting::decontaminate(source, &region, 3.0, 100.0);
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.surface_mut().ok_or_else(pixel_error)? = clean;
        ctx.check()?;
    }
    let (_, params) = request(options, Some(id.0));
    let output = cutout_cmds::BackgroundOutput::from_params(&params, "layer.removeBackground")?;
    let result = cutout_cmds::apply_output(doc, id, mask, &output, ctx)?;
    if let Some(source) = original_surface {
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.surface_mut().ok_or_else(pixel_error)? = source;
    }
    Ok(result)
}

fn refine_matte(img: &image::RgbaImage, alpha: &image::GrayImage) -> image::GrayImage {
    use photocraft_algo::cf_matting::{MattingParams, closed_form_matting_tiled, trimap_from_mask};
    let (w, h) = (img.width() as usize, img.height() as usize);
    let m: Vec<f32> = alpha.pixels().map(|p| p[0] as f32 / 255.0).collect();
    let band = ((w.min(h) as f32 / 160.0).round() as usize).clamp(3, 24);
    let mut tri = trimap_from_mask(&m, w, h, band);
    for (t, v) in tri.iter_mut().zip(&m) {
        if (0.1..0.9).contains(v) {
            *t = 0.5;
        }
    }
    let rgb = photocraft_algo::segment::RgbImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x as u32, y as u32);
        [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
    });
    let a = closed_form_matting_tiled(&rgb, &tri, &MattingParams::default(), 384, 24);
    image::GrayImage::from_fn(w as u32, h as u32, |x, y| image::Luma([(a[y as usize * w + x as usize] * 255.0).round() as u8]))
}
