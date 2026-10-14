//! Per-photo looks and default look preferences.

use lightcraft_develop::{ControlSpec, DevelopSettings, Look, Section, Track};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::register;

/// The Look picker (Soft Film · Sigmoid · Camera) for a raw photo.
pub fn look_row(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let t = Tokens::get(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    crate::widgets::develop_body(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::i18n::tr("Look")).text_style(egui::TextStyle::Body).color(t.text_dim));
            let mut pick = None;
            let r = crate::widgets::dropdown(ui, "developLook", crate::i18n::tr(d.look.label()), font.clone(), t.text_label);
            register(ui.ctx(), "combo:developLook", r.rect);
            egui::Popup::menu(&r).show(|ui| {
                for l in Look::ALL {
                    let choice = ui.selectable_label(d.look == l, crate::i18n::tr(l.label())).on_hover_text(crate::i18n::tr(look_hint(l)));
                    register(ui.ctx(), format!("look:{}", l.id()), choice.rect);
                    if choice.clicked() {
                        pick = Some(l);
                        ui.close();
                    }
                }
            });
            if let Some(l) = pick.filter(|l| *l != d.look) {
                let _ = app.run("develop.look", json!({"look": l.id()}));
            }
        });
        let mut base = d.look_options.base;
        let r = crate::widgets::dropdown(ui, "lookBase", crate::i18n::tr(base.label()), font, t.text_label);
        egui::Popup::menu(&r).show(|ui| {
            for b in lightcraft_develop::ToneBase::ALL {
                let choice = ui.selectable_label(base == b, crate::i18n::tr(b.label()));
                register(ui.ctx(), format!("lookBase:{}", b.label()), choice.rect);
                if choice.clicked() {
                    base = b;
                    ui.close();
                }
            }
        });
        if base != d.look_options.base {
            let _ = app.run("develop.merge", json!({"settings":{"look_options":{"base":base}},"label":"Base Curve"}));
        }
    });
    let hue_spec = ControlSpec {
        id: "look.huePreservation",
        label: "Hue Preservation",
        section: Section::Profile,
        min: 0.0,
        max: 100.0,
        default: lightcraft_develop::LookOptions::default().hue_preservation,
        step: 1.0,
        decimals: 0,
        track: Track::Plain,
    };
    if let Some(hue) = crate::widgets::slider(ui, &hue_spec, d.look_options.hue_preservation, true, None).value {
        let _ = app.run("develop.merge", json!({"settings":{"look_options":{"hue_preservation":hue}},"label":"Hue Preservation"}));
    }
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
