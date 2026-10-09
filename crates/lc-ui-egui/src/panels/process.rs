//! Process version and look controls (Process 2026): the Look picker under the profile, the
//! Process picker heading the Calibration section, and "Update to Process 2026".

use lightcraft_develop::{DevelopSettings, Look, ProcessVersion};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::{register, text_button};

/// The Look picker (Adobe-like · Sigmoid · Match Camera) for a raw photo under Process 2026.
pub fn look_row(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    if !d.v2026() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 0, bottom: 12 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::i18n::tr("Look")).font(t.font(13.0)).color(t.text_dim));
            let mut pick = None;
            let r = egui::ComboBox::from_id_salt("developLook").width(170.0).selected_text(crate::i18n::tr(d.look.label())).show_ui(ui, |ui| {
                for l in Look::ALL {
                    if ui.selectable_label(d.look == l, crate::i18n::tr(l.label())).on_hover_text(crate::i18n::tr(look_hint(l))).clicked() {
                        pick = Some(l);
                    }
                }
            });
            register(ui.ctx(), "combo:developLook", r.response.rect);
            if let Some(l) = pick.filter(|l| *l != d.look) {
                let _ = app.run("develop.look", json!({"look": l.id()}));
            }
        });
    });
}

/// What each look does (tooltips, the Settings hint).
pub fn look_hint(l: Look) -> &'static str {
    match l {
        Look::Adobe => "Bright midtones, a soft toe and a long highlight shoulder (Lightroom-like)",
        Look::Sigmoid => "darktable's neutral scene-referred sigmoid curve, hue preserving",
        Look::Camera => "Matches the camera's own JPEG (or a maker-style curve when the file has no preview)",
    }
}

/// The Process row at the top of the Calibration section: 2026 / Legacy, and "Update to
/// Process 2026" for a photo still on Legacy.
pub fn process_row(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 6 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::i18n::tr("Process")).font(t.font(13.0)).color(t.text_dim));
            let mut pick = None;
            let r = egui::ComboBox::from_id_salt("developProcess").width(110.0).selected_text(crate::i18n::tr(d.process.label())).show_ui(ui, |ui| {
                for pv in ProcessVersion::ALL {
                    if ui.selectable_label(d.process == pv, crate::i18n::tr(pv.label())).clicked() {
                        pick = Some(pv);
                    }
                }
            });
            register(ui.ctx(), "combo:developProcess", r.response.rect);
            if let Some(pv) = pick.filter(|pv| *pv != d.process) {
                set_process(app, pv, false);
            }
        });
        if !d.v2026() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(crate::i18n::tr("This photo renders with the Legacy process. Process 2026 adds looks, hue-preserving tone and curves, camera colour per white balance and smoother gamut mapping.")).size(11.0).color(t.text_dim));
            ui.add_space(2.0);
            if text_button(ui, "updateProcess", "Update to Process 2026", false).clicked() {
                set_process(app, ProcessVersion::V2026, true);
            }
        }
    });
}

/// Set the process version of the active photo, or (`selection`) of every selected photo.
pub fn set_process(app: &mut LightcraftApp, pv: ProcessVersion, selection: bool) {
    let process = serde_json::to_value(pv).unwrap_or(json!("v2026"));
    let p = if selection { json!({"process": process}) } else { json!({"process": process, "ids": app.session.active().map(|i| vec![i.0]).unwrap_or_default()}) };
    let _ = app.run("develop.setProcess", p);
}

/// Settings → Import: the look new raw photos start with.
pub fn default_look_combo(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let cur = app.session.import_defaults.look;
    let mut pick = None;
    let r = egui::ComboBox::from_id_salt("settingsDefaultLook").width(240.0).selected_text(crate::i18n::tr(cur.label())).show_ui(ui, |ui| {
        for l in Look::ALL {
            if ui.selectable_label(cur == l, crate::i18n::tr(l.label())).on_hover_text(crate::i18n::tr(look_hint(l))).clicked() {
                pick = Some(l);
            }
        }
    });
    register(ui.ctx(), "combo:settingsDefaultLook", r.response.rect);
    if let Some(l) = pick.filter(|l| *l != cur) {
        let _ = app.run("library.preferences", json!({"import": {"look": l.id()}}));
    }
}
