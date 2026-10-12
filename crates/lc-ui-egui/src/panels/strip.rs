//! The vertical tool strip at the far right (Presets, Edit, Crop, Remove, Masking, Red Eye …).

use egui::vec2;
use serde_json::json;

use crate::LightcraftApp;
use crate::state::RightPanel;
use crate::theme::Tokens;
use crate::widgets::tool_button;

/// A filter edits one temporary photo. Copy/Paste act on that working copy;
/// Library settings (which can switch libraries), sync and export stay outside.
fn more_action_allowed(app: &LightcraftApp, id: &str) -> bool {
    app.host_session.is_none() || matches!(id, "develop.copy" | "develop.paste" | "dialog.copySettings" | "app.shortcuts")
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("tool_strip")
        .exact_size(t.strip_w)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(t.chrome)
                .inner_margin(egui::Margin { left: 0, right: 0, top: 8, bottom: 8 })
                .stroke(egui::Stroke::new(1.0, t.divider)),
        )
        .show(ui, |ui| {
            let has_photo = app.session.active().is_some();
            // The framed panel's stroke consumes part of its nominal width.
            // Size hit rectangles to that inner allocation, not the outer token.
            let inner = ui.max_rect();
            let control_width = inner.width().min(t.strip_w).max(0.0);
            crate::widgets::register(ui.ctx(), "panel:toolStripInner", inner);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let sz = vec2(control_width, 40.0);
                use crate::panels::tool_tips::{StripTool as T, attach};
                let mode = app.ui.settings.tool_tips;
                let r = tool_button(ui, "presets", T::Presets.icon(), sz, app.ui.presets, has_photo, "");
                attach(mode, ui, &r, T::Presets);
                if r.clicked() {
                    let _ = app.run("panel.presets", json!({}));
                }
                for (tool, panel) in [
                    (T::Edit, RightPanel::Edit),
                    (T::Crop, RightPanel::Crop),
                    (T::Remove, RightPanel::Remove),
                    (T::Masking, RightPanel::Masking),
                    (T::RedEye, RightPanel::RedEye),
                ] {
                    let id = tool.id();
                    // local-image: a Camera Raw Filter session has no Crop
                    if crate::panels::host_session::hides(app, id) {
                        continue;
                    }
                    let on = app.ui.right == panel || (panel == RightPanel::Edit && app.ui.right == RightPanel::Profiles);
                    let r = tool_button(ui, id, tool.icon(), sz, on, has_photo, "");
                    attach(mode, ui, &r, tool);
                    if r.clicked() {
                        let _ = app.run(&format!("panel.{id}"), json!({}));
                    }
                    if id == "edit" {
                        separator(ui, &t);
                    }
                }
                separator(ui, &t);
                let r = tool_button(ui, "versions", T::Versions.icon(), sz, app.ui.right == RightPanel::Versions, has_photo, "");
                attach(mode, ui, &r, T::Versions);
                if r.clicked() {
                    let _ = app.run("panel.versions", json!({}));
                }
                let r = tool_button(ui, "activity", T::Activity.icon(), sz, app.ui.right == RightPanel::Activity, has_photo, "");
                attach(mode, ui, &r, T::Activity);
                if r.clicked() {
                    let _ = app.run("panel.activity", json!({}));
                }
                let r = tool_button(ui, "more", T::More.icon(), sz, false, true, "");
                attach(mode, ui, &r, T::More);
                egui::Popup::menu(&r).show(|ui| {
                    for (id, label) in [
                        ("develop.copy", "Copy Edit Settings"),
                        ("develop.paste", "Paste Edit Settings"),
                        ("dialog.copySettings", "Choose Edit Settings to Copy…"),
                        ("dialog.syncSettings", "Sync Settings…"),
                        ("dialog.export", "Export…"),
                        ("app.shortcuts", "Keyboard Shortcuts"),
                        ("app.settings", "Settings…"),
                    ] {
                        if !more_action_allowed(app, id) {
                            continue;
                        }
                        let enabled = if id == "develop.copy" {
                            has_photo
                        } else if id == "develop.paste" {
                            has_photo && app.session.clipboard.is_some()
                        } else {
                            crate::menus::ui_enabled(app, id)
                        };
                        let mut button = egui::Button::new(crate::i18n::tr(label));
                        if let Some(shortcut) = crate::shortcuts::shortcut_label(id) {
                            button = button.shortcut_text(shortcut);
                        }
                        let response = ui.add_enabled(enabled, button);
                        crate::widgets::register(ui.ctx(), format!("menu:more:{id}"), response.rect);
                        if response.clicked() {
                            if let Err(error) = app.run(id, json!({})) {
                                app.toast_error(ui.ctx(), error);
                            }
                            ui.close();
                        }
                    }
                });
            });
            // bottom icons
            let bottom = ui.max_rect().bottom();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), bottom - 96.0), ui.max_rect().right_bottom())),
            );
            child.vertical_centered(|ui| {
                let sz = vec2(control_width, 40.0);
                use crate::panels::tool_tips::{StripTool as T, attach};
                let r = tool_button(ui, "keywords", T::Keywords.icon(), sz, app.ui.right == RightPanel::Keywords, has_photo, "");
                attach(app.ui.settings.tool_tips, ui, &r, T::Keywords);
                if r.clicked() {
                    let _ = app.run("panel.keywords", json!({}));
                }
                let r = tool_button(ui, "info", T::Info.icon(), sz, app.ui.right == RightPanel::Info, has_photo, "");
                attach(app.ui.settings.tool_tips, ui, &r, T::Info);
                if r.clicked() {
                    let _ = app.run("panel.info", json!({}));
                }
            });
        });
}

fn separator(ui: &mut egui::Ui, t: &Tokens) {
    let (r, _) = ui.allocate_exact_size(vec2(t.strip_w - 16.0, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, t.button_border.gamma_multiply(0.7));
}
