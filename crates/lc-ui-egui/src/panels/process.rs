//! Per-photo looks and default look preferences.

use lightcraft_develop::{DevelopSettings, Look};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::{register};

/// The Look picker (Soft Film · Sigmoid · Camera) for a raw photo.
pub fn look_row(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
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
        let mut base=d.look_options.base;
        egui::ComboBox::from_id_salt("lookBase").selected_text(base.label()).show_ui(ui,|ui| {
            for b in lightcraft_develop::ToneBase::ALL { ui.selectable_value(&mut base,b,b.label()); }
        });
        if base!=d.look_options.base {
            let _=app.run("develop.merge",json!({"settings":{"look_options":{"base":base}},"label":"Base Curve"}));
        }
        let mut hue=d.look_options.hue_preservation;
        if ui.add(egui::Slider::new(&mut hue,0.0..=100.0).text(crate::i18n::tr("Hue Preservation"))).changed() {
            let _=app.run("develop.merge",json!({"settings":{"look_options":{"hue_preservation":hue}},"label":"Hue Preservation"}));
        }

    });
}

/// What each look does (tooltips, the Settings hint).
pub fn look_hint(l: Look) -> &'static str {
    match l {
        Look::Adobe => "Bright midtones, a soft toe and a long highlight shoulder",
        Look::Sigmoid => "darktable's neutral scene-referred sigmoid curve, hue preserving",
        Look::Camera => "Matches the camera's own JPEG (or a maker-style curve when the file has no preview)",
    }
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
