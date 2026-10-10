//! Shared Remove Background settings and dispatch for all three editor surfaces.

use egui::vec2;
use li_ai::catalog::ModelId;
use serde_json::{Value, json};

use crate::{PhotocraftApp, widgets};

const MODELS: [(&str, &str); 3] = [("quick", "Standard (on-device)"), ("qwen-int8", "Qwen AI · Compact"), ("qwen-bf16", "Qwen AI · Full")];
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
    let standard = ai.cutout_engine == "quick";
    let mut p = json!({
        "engine": if standard { "learned" } else { &ai.cutout_engine },
        "output": ai.cutout_output,
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
    let (cmd, p) = request(&app.ui.ai, layer);
    #[cfg(test)]
    if let Some(result) = tests::injected_result(app, cmd, &p) {
        return result;
    }
    app.run(cmd, p)
}

fn model_ready(ai: &crate::ai_ui::AiOptions) -> bool {
    if ai.cutout_engine == "quick" {
        photocraft_engine::seg::installed().is_some()
    } else {
        crate::ai_ui::status().ready(ModelId::Qwen, if ai.cutout_engine == "qwen-bf16" { "bf16" } else { "int8" }).is_ok()
    }
}

pub(crate) fn can_remove(app: &PhotocraftApp) -> bool {
    let cmd = if app.ui.ai.cutout_engine == "quick" { "layer.removeBackground" } else { "ai.removeBackground" };
    model_ready(&app.ui.ai)
        && app.session.is_enabled(cmd)
        && app.session.active().and_then(|d| d.active_layer.map(|id| d.doc.effective_locks(id))).is_some_and(|locks| !locks.pixels && !locks.all)
}

pub(crate) fn readiness(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if app.ui.ai.cutout_engine == "quick" {
        match photocraft_engine::seg::installed() {
            Some(li_seg::InUse::Custom(c)) => {
                ui.label(crate::i18n::fmt(tl!("Subject & Background model: {model}"), &[("model", &c.name)]));
            }
            Some(_) => {
                ui.label(tl!("IS-Net · CPU"));
            }
            None => crate::ai_ui::readiness_warning(app, ui, tl!("Install a Subject & Background model to use Standard."), true),
        }
    } else {
        let variant = if app.ui.ai.cutout_engine == "qwen-bf16" { "bf16" } else { "int8" };
        crate::ai_ui::readiness(app, ui, ModelId::Qwen, variant);
    }
}

/// Inline controls for the options bar; field rows for Properties and the task-bar popup.
pub(crate) fn settings(app: &mut PhotocraftApp, ui: &mut egui::Ui, inline: bool) {
    let models: Vec<_> = MODELS.iter().map(|(k, l)| ((*k).to_owned(), *l)).collect();
    let outputs: Vec<_> = OUTPUTS.iter().map(|(k, l)| ((*k).to_owned(), *l)).collect();
    let mut fields = |ui: &mut egui::Ui| {
        row(ui, inline, tl!("Model"), |ui, width| {
            widgets::dropdown(ui, "cutout-model", &mut app.ui.ai.cutout_engine, &models, width);
        });
        if app.ui.ai.cutout_engine != "quick" {
            row(ui, inline, tl!("Keep…"), |ui, width| {
                let r = ui.add(egui::TextEdit::singleline(&mut app.ui.ai.cutout_hint).hint_text(tl!("Keep… (optional)")).desired_width(width));
                r.widget_info(|| {
                    let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, ui.is_enabled(), tl!("Keep… (optional)"));
                    info.current_text_value = Some(app.ui.ai.cutout_hint.clone());
                    info
                });
            });
        }
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
        if ui.selectable_label(app.ui.ai.cutout_engine == key, tl!(label)).clicked() {
            app.ui.ai.cutout_engine = key.into();
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
