//! Preset Manager and Export/Import Presets dialog bodies.
use super::*;

pub(super) fn presets_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut kind = f.get("kind").and_then(Value::as_str).unwrap_or("brushes").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Preset Type")).color(t.text_dim));
        let opts = [("brushes".to_string(), tl!("Brushes")), ("customShapes".to_string(), tl!("Custom Shapes")), ("patterns".to_string(), tl!("Patterns"))];
        crate::widgets::dropdown(ui, "preset-kind", &mut kind, &opts, 180.0);
    });
    let list = app
        .session
        .execute("edit.presets.presetManager", json!({"kind": kind}))
        .ok()
        .and_then(|v| v.get(&kind).cloned())
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default();
    let mut selected = f.get("selected").and_then(Value::as_u64).unwrap_or(0) as usize;
    egui::ScrollArea::vertical().max_height(260.0).id_salt("preset-list").show(ui, |ui| {
        for (i, name) in list.iter().enumerate() {
            if ui.selectable_label(i == selected, name).clicked() {
                selected = i;
                f.insert("newName".into(), json!(name));
            }
        }
        if list.is_empty() {
            ui.label(RichText::new(tl!("No presets of this type.")).color(t.text_faint));
        }
    });
    let mut new_name = f.get("newName").and_then(Value::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut new_name).desired_width(200.0));
        if ui.add_enabled(selected < list.len() && !new_name.trim().is_empty(), egui::Button::new(tl!("Rename"))).clicked() {
            let _ = app.run("edit.presets.presetManager", json!({"action": "rename", "kind": kind, "index": selected, "newName": new_name}));
        }
        if ui.add_enabled(selected < list.len(), egui::Button::new(tl!("Delete"))).clicked() {
            let _ = app.run("edit.presets.presetManager", json!({"action": "delete", "kind": kind, "index": selected}));
        }
        // Load Photoshop brushes (.abr) into the library.
        if kind == "brushes" && ui.button(tl!("Load…")).on_hover_text(tl!("Import Photoshop brushes (.abr)")).clicked() {
            app.open_dialog_file();
        }
    });
    f.insert("kind".into(), json!(kind));
    f.insert("selected".into(), json!(selected));
    f.insert("newName".into(), json!(new_name));
}

pub(super) fn presets_io_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut action = f.get("action").and_then(Value::as_str).unwrap_or("export").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Action")).color(t.text_dim));
        crate::widgets::dropdown(
            ui,
            "presets-io",
            &mut action,
            &[("export".to_string(), tl!("Export Presets")), ("import".to_string(), tl!("Import Presets"))],
            180.0,
        );
    });
    for (k, label) in [("brushes", tl!("Brushes")), ("customShapes", tl!("Custom Shapes"))] {
        let mut on = f.get(k).and_then(Value::as_bool).unwrap_or(true);
        crate::widgets::checkbox(ui, &mut on, label);
        f.insert(k.into(), json!(on));
    }
    f.insert("action".into(), json!(action));
}
