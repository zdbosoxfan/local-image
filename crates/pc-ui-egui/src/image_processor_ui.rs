//! File Type section of the Image Processor dialog: which of JPEG, PSD and TIFF one run writes,
//! each with its own settings (the engine's per-format mode).

use std::cell::Cell;

use serde_json::{Map, Value, json};

use crate::state::{DialogKind, UiState};

type Fields = Map<String, Value>;

const COMMAND: &str = "file.scripts.imageProcessor";
const FORMATS: [&str; 3] = ["jpeg", "psd", "tiff"];
const NO_FORMAT: &str = "You must save to at least one file type.";
/// Height kept free for the title bar and the buttons when the body scrolls.
const DIALOG_CHROME: f32 = 150.0;

pub(crate) fn owns(fields: &Fields) -> bool {
    fields.contains_key("__form") && fields.get("__command").and_then(Value::as_str) == Some(COMMAND) && !fields.contains_key("format")
}

/// Run with no file type checked shows the alert and keeps the dialog open.
pub(crate) fn preflight(ui: &mut UiState, id: u64) -> Result<(), String> {
    if let Some(dialog) = ui.dialogs.iter().find(|dialog| dialog.id == id && owns(&dialog.fields))
        && !FORMATS.iter().any(|key| dialog.fields.get(*key).is_some_and(|options| options.is_object() && options.get("enabled") != Some(&Value::Bool(false))))
    {
        alert(ui, NO_FORMAT);
        return Err(tl!(NO_FORMAT).into());
    }
    Ok(())
}

/// A number typed out of range shows its alert, and the Run or Enter of the same frame waits.
pub(crate) fn finish(ui: &mut UiState, interactive: bool, alert_text: Option<&str>, outcome: Option<bool>) -> Option<bool> {
    if interactive
        && outcome != Some(false)
        && let Some(text) = alert_text
    {
        alert(ui, text);
        return None;
    }
    outcome
}

/// Opens `text` as an alert unless the same one is already showing.
fn alert(ui: &mut UiState, text: &str) {
    let message = tl!(text).to_owned();
    let shown =
        ui.dialogs.iter().any(|dialog| dialog.kind == DialogKind::Error && dialog.fields.get("message").and_then(Value::as_str) == Some(message.as_str()));
    if !shown {
        let mut fields = Fields::new();
        fields.insert("message".into(), json!(message));
        ui.open_dialog(DialogKind::Error, fields);
    }
}

/// The dialog's folders, then the File Type section. Returns the alert a number entry raised.
pub(crate) fn body(ui: &mut egui::Ui, fields: &mut Fields) -> Option<&'static str> {
    let height = (ui.ctx().content_rect().height() - DIALOG_CHROME).max(120.0);
    egui::ScrollArea::vertical()
        .id_salt("image-processor")
        .max_height(height)
        .show(ui, |ui| {
            crate::view_cmds::form_body(ui, fields);
            formats(ui, fields)
        })
        .inner
}

fn formats(ui: &mut egui::Ui, fields: &mut Fields) -> Option<&'static str> {
    ui.add_space(8.0);
    crate::widgets::hairline(ui);
    ui.label(egui::RichText::new(tl!("File Type")).strong());
    let mut alert = None;
    for (key, label) in FORMATS.into_iter().zip([tl!("Save as JPEG"), tl!("Save as PSD"), tl!("Save as TIFF")]) {
        let Some(options) = fields.get_mut(key).and_then(Value::as_object_mut) else { continue };
        ui.push_id(key, |ui| {
            let enabled = checkbox(ui, options, "enabled", label);
            ui.indent("format-options", |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    // PSD has no Maximize Compatibility here: PSD export has no such switch.
                    match key {
                        "jpeg" => {
                            ui.horizontal(|ui| number(ui, options, "quality", tl!("Quality"), 5.0, 12.0, &mut alert));
                            checkbox(ui, options, "convertToSrgb", tl!("Convert Profile to sRGB"));
                        }
                        "tiff" => {
                            checkbox(ui, options, "lzw", tl!("LZW Compression"));
                        }
                        _ => {}
                    }
                    let resize = checkbox(ui, options, "resize", tl!("Resize to Fit"));
                    ui.add_enabled_ui(resize, |ui| {
                        ui.horizontal(|ui| {
                            number(ui, options, "width", tl!("W"), 1.0, 30_000.0, &mut alert);
                            number(ui, options, "height", tl!("H"), 1.0, 30_000.0, &mut alert);
                            ui.label("px");
                        });
                    });
                });
            });
        });
        ui.add_space(4.0);
    }
    alert
}

fn checkbox(ui: &mut egui::Ui, options: &mut Fields, key: &str, label: &str) -> bool {
    let mut on = options.get(key).and_then(Value::as_bool).unwrap_or(false);
    if crate::widgets::checkbox(ui, &mut on, label).changed() {
        options.insert(key.into(), json!(on));
    }
    on
}

/// A number field from 1 to `max`. A typed entry that is not a number keeps the previous value,
/// one out of range is clamped; either raises the matching alert.
fn number(ui: &mut egui::Ui, options: &mut Fields, key: &str, label: &str, default: f64, max: f64, alert: &mut Option<&'static str>) {
    let mut value = options.get(key).and_then(Value::as_f64).unwrap_or(default);
    let warning = Cell::new(None);
    let whole = key != "quality";
    let name = ui.label(label);
    ui.add(egui::DragValue::new(&mut value).range(1.0..=max).max_decimals(if whole { 0 } else { 8 }).update_while_editing(false).custom_parser(|text| {
        let parsed = crate::widgets::parse_num(text);
        warning.set(match parsed {
            None => Some(invalid_entry(key)),
            Some(value) if !(1.0..=max).contains(&value) => Some(closest_value(key)),
            Some(_) => None,
        });
        parsed.map(|n| if whole { n.trunc() } else { n })
    }))
    .labelled_by(name.id);
    options.insert(key.into(), json!(value));
    *alert = warning.get().or(*alert);
}

fn invalid_entry(key: &str) -> &'static str {
    if key == "quality" {
        "Invalid numeric entry. A value between 1 and 12 is required. Previous value inserted."
    } else {
        "Invalid numeric entry. A value between 1 and 30000 is required. Previous value inserted."
    }
}

fn closest_value(key: &str) -> &'static str {
    if key == "quality" {
        "A value between 1 and 12 is required. Closest value inserted."
    } else {
        "A value between 1 and 30000 is required. Closest value inserted."
    }
}

#[cfg(test)]
mod tests;
