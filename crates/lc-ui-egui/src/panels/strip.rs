//! The vertical tool strip at the far right (Presets, Edit, Crop, Remove, Masking, Red Eye …).

use egui::vec2;
use serde_json::json;

use crate::LightcraftApp;
use crate::state::RightPanel;
use crate::theme::Tokens;
use crate::widgets::tool_button;

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
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let sz = vec2(t.strip_w, 40.0);
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
                let r = tool_button(ui, "activity", T::Activity.icon(), sz, app.ui.right == RightPanel::Activity, true, "");
                attach(mode, ui, &r, T::Activity);
                if r.clicked() {
                    let _ = app.run("panel.activity", json!({}));
                }
                let r = tool_button(ui, "more", T::More.icon(), sz, false, true, "");
                attach(mode, ui, &r, T::More);
                if r.clicked() {
                    app.ui.dialog = Some(crate::state::Dialog::About);
                }
            });
            // bottom icons
            let bottom = ui.max_rect().bottom();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), bottom - 96.0), ui.max_rect().right_bottom())),
            );
            child.vertical_centered(|ui| {
                let sz = vec2(t.strip_w, 40.0);
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
