//! Edit panel sections of the toolset upgrades: Tone Equalizer, Raw Processing (demosaic,
//! highlight reconstruction, capture sharpening), the lens database rows of Optics, and Color
//! Calibration. Each tool is off until switched on; the section headers' eye switches it off
//! again without losing its settings.

use lightcraft_catalog::PhotoId;
use lightcraft_develop::{Adaptation, Demosaic, DevelopSettings, HighlightMode, Illuminant};
use serde_json::{Value, json};

use super::edit::{control, sub_title};
use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::{divider, register, section_header};

fn pad(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, add);
}

fn merge(app: &mut LightcraftApp, settings: Value, label: &str) {
    let _ = app.run("develop.merge", json!({"settings": settings, "label": label}));
}

/// A tool with an on/off flag (`key.enabled`) in a section `id`: the header (its eye shows once
/// the tool is on), an "on" checkbox, then `body` while it is on. Hidden in sessions that can't
/// use it (Camera Raw Filter).
#[allow(clippy::too_many_arguments)]
fn tool(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    d: &DevelopSettings,
    id: &str,
    key: &str,
    title: &str,
    check: &str,
    enabled: bool,
    body: impl FnOnce(&mut LightcraftApp, &mut egui::Ui, &DevelopSettings),
) {
    if crate::panels::host_session::hides(app, id) {
        return;
    }
    let on = enabled && d.section_enabled(id);
    let set_on = |app: &mut LightcraftApp, v: bool| {
        merge(app, json!({key: {"enabled": v}}), title);
        if v && !d.section_enabled(id) {
            let _ = app.run("develop.sectionEnabled", json!({"section": id, "enabled": true}));
        }
    };
    let open = app.ui.section_open(id);
    let (resp, toggled) = section_header(ui, id, title, open, enabled.then_some(on));
    if let Some(v) = toggled {
        set_on(app, v);
    } else if resp.clicked() {
        app.ui.toggle_section(id);
    }
    if open {
        pad(ui, |ui| {
            let mut v = on;
            let r = ui.checkbox(&mut v, crate::i18n::tr(check));
            register(ui.ctx(), format!("{id}:enabled"), r.rect);
            if r.changed() {
                set_on(app, v);
            }
        });
        if on {
            body(app, ui, d);
        }
        ui.add_space(8.0);
    }
    divider(ui);
}

/// A row of choices (segmented), merged as `{key: {field: value}}`.
fn choice(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    items: &[(&str, &str)],
    active: Option<usize>,
    per_row: usize,
    set: impl Fn(&str) -> Value,
) {
    let t = Tokens::get(ui.ctx());
    let mut chosen = None;
    pad(ui, |ui| {
        ui.label(egui::RichText::new(crate::i18n::tr(label)).font(t.font(13.0)).color(t.text_dim));
        chosen = crate::widgets::segmented(ui, id, items, active, per_row);
    });
    if let Some(i) = chosen {
        merge(app, set(items[i].1), label);
    }
}

fn key_of<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Tone Equalizer: nine zone sliders, the mask's size / edges / compensation, mask preview.
pub fn tone_eq_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let te = d.tone_eq;
    tool(app, ui, d, "toneEq", "tone_eq", "Tone Equalizer", "Equalize Tones", te.enabled, |app, ui, d| {
        pad(ui, |ui| {
            let mut show = app.ui.flyout_open("toneEqMask");
            let r = ui.checkbox(&mut show, crate::i18n::tr("Show Mask"));
            register(ui.ctx(), "toneEq:mask", r.rect);
            if r.changed() {
                app.ui.toggle_flyout("toneEqMask");
            }
        });
        sub_title(ui, "Exposure by Zone");
        for k in (0..=8).rev() {
            control(app, ui, d, &format!("toneEq.ev{k}"), true);
        }
        sub_title(ui, "Mask");
        for c in ["toneEq.size", "toneEq.refine", "toneEq.maskExposure", "toneEq.maskContrast", "toneEq.smoothing"] {
            control(app, ui, d, c, true);
        }
    });
}

/// Raw Processing (raw files only): demosaic, highlight reconstruction, capture sharpening.
pub fn raw_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let id = "raw";
    if crate::panels::host_session::hides(app, id) {
        return;
    }
    let open = app.ui.section_open(id);
    let changed = !d.raw.is_default();
    let (resp, toggled) = section_header(ui, id, "Raw Processing", open, changed.then_some(d.section_enabled(id)));
    if let Some(on) = toggled {
        let _ = app.run("develop.sectionEnabled", json!({"section": id, "enabled": on}));
    } else if resp.clicked() {
        app.ui.toggle_section(id);
    }
    if !open {
        divider(ui);
        return;
    }
    let r = d.raw;
    let t = Tokens::get(ui.ctx());
    let mut available = Demosaic::ALL.to_vec();
    if r.demosaic == Demosaic::DualRcd {
        available.push(Demosaic::DualRcd);
    }
    let methods: Vec<(&str, String)> = available.iter().map(|m| (m.label(), key_of(*m))).collect();
    let items: Vec<(&str, &str)> = methods.iter().map(|(l, k)| (*l, k.as_str())).collect();
    choice(app, ui, "rawDemosaic", "Demosaic", &items, available.iter().position(|m| *m == r.demosaic), 3, |k| json!({"raw": {"demosaic": k}}));
    if r.demosaic.is_dual() {
        control(app, ui, d, "raw.dualThreshold", true);
    }
    pad(ui, |ui| {
        ui.label(
            egui::RichText::new(crate::i18n::tr("Visible at 1:1 and in exports; previews are binned from the sensor.")).size(11.0).color(t.text_dim),
        );
    });
    let modes: Vec<(&str, String)> = HighlightMode::ALL.iter().map(|m| (m.label(), key_of(*m))).collect();
    let items: Vec<(&str, &str)> = modes.iter().map(|(l, k)| (*l, k.as_str())).collect();
    choice(
        app,
        ui,
        "rawHighlights",
        "Highlight Reconstruction",
        &items,
        HighlightMode::ALL.iter().position(|m| *m == r.highlights),
        3,
        |k| json!({"raw": {"highlights": k}}),
    );
    sub_title(ui, "Capture Sharpening");
    pad(ui, |ui| {
        let mut on = r.capture.enabled;
        let c = ui.checkbox(&mut on, crate::i18n::tr("Sharpen the Sensor's Blur"));
        register(ui.ctx(), "raw:capture", c.rect);
        if c.changed() {
            merge(app, json!({"raw": {"capture": {"enabled": on}}}), "Capture Sharpening");
        }
    });
    if r.capture.enabled {
        for c in ["raw.captureRadius", "raw.captureThreshold", "raw.captureCornerBoost", "raw.captureIterations"] {
            control(app, ui, d, c, true);
        }
        pad(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr("Radius and Contrast Threshold 0: measured from the raw data and the ISO."))
                    .size(11.0)
                    .color(t.text_dim),
            );
        });
    }
    ui.add_space(8.0);
    divider(ui);
}

/// The lens database rows of the Optics section: on/off, the detected or chosen lens, a picker,
/// and the correction strengths.
pub fn lens_db_rows(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId, d: &DevelopSettings) {
    let t = Tokens::get(ui.ctx());
    let l = d.lens_db.clone();
    pad(ui, |ui| {
        let mut on = l.enabled;
        let r = ui.checkbox(&mut on, crate::i18n::tr("Use Lens Database"));
        register(ui.ctx(), "lensDb:enabled", r.rect);
        if r.changed() {
            merge(app, json!({"lens_db": {"enabled": on}}), "Lens Database");
        }
    });
    if !l.enabled {
        return;
    }
    let Some(photo) = app.session.catalog.photo(id).cloned() else { return };
    // detection runs once per photo (fuzzy matching over the whole database)
    let cache = egui::Id::new(("lensDbDetect", id.0));
    let detected: Option<(Option<String>, Option<lightcraft_engine::lens_db::Named>)> = ui.ctx().data(|m| m.get_temp(cache));
    let (camera_label, auto_lens) = detected.unwrap_or_else(|| {
        let det = lightcraft_engine::lens_db::detect(&photo.meta.camera, &photo.meta.lens);
        let v = (det.camera.as_ref().map(|c| c.label()), det.lens.clone());
        ui.ctx().data_mut(|m| m.insert_temp(cache, v.clone()));
        v
    });
    let chosen = l.lens.as_ref().map(|n| lightcraft_engine::lens_db::Named { maker: n.maker.clone(), model: n.model.clone() });
    let found = lightcraft_engine::lens_db::for_photo(&photo, d).is_some();
    pad(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::i18n::tr("Lens")).font(t.font(13.0)).color(t.text_dim));
            let text = match (&chosen, &auto_lens) {
                (Some(n), _) => n.label(),
                (None, Some(n)) => crate::i18n::tr_format!("Auto: {lens}", lens = n.label()),
                (None, None) => crate::i18n::tr("Choose…").to_string(),
            };
            let r = crate::widgets::dropdown(ui, "lensDbLens", &text, t.font(13.0), t.text_label);
            egui::Popup::menu(&r).show(|ui| lens_picker(app, ui, camera_label.is_some().then(|| camera_of(&photo)).flatten()));
        });
        if let Some(c) = &camera_label {
            ui.label(egui::RichText::new(crate::i18n::tr_format!("Camera: {camera}", camera = c)).size(11.0).color(t.text_dim));
        }
        if !found {
            ui.label(
                egui::RichText::new(crate::i18n::tr(
                    "No profile for this lens and focal length: choose the lens, or check the photo's focal length.",
                ))
                .size(11.0)
                .color(t.text_dim),
            );
        }
    });
    for c in ["lensDb.distortion", "lensDb.tca", "lensDb.vignetting"] {
        control(app, ui, d, c, found);
    }
}

fn camera_of(p: &lightcraft_catalog::Photo) -> Option<lightcraft_engine::lens_db::Named> {
    lightcraft_engine::lens_db::detect(&p.meta.camera, "").camera
}

/// The lens list: a search field, "Automatic", then the matching lenses for the camera's mount.
fn lens_picker(app: &mut LightcraftApp, ui: &mut egui::Ui, camera: Option<lightcraft_engine::lens_db::Named>) {
    let qid = egui::Id::new("lensDbQuery");
    let mut q: String = ui.ctx().data(|m| m.get_temp(qid)).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(crate::i18n::tr("Search lenses")).desired_width(240.0));
    if r.changed() {
        ui.ctx().data_mut(|m| m.insert_temp(qid, q.clone()));
    }
    if ui.button(crate::i18n::tr("Automatic (from the photo)")).clicked() {
        merge(app, json!({"lens_db": {"lens": null}}), "Lens");
        ui.close();
    }
    ui.separator();
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for n in lightcraft_engine::lens_db::search(camera.as_ref(), &q, 200) {
            if ui.button(n.label()).clicked() {
                merge(app, json!({"lens_db": {"lens": {"maker": n.maker, "model": n.model}}}), "Lens");
                ui.close();
            }
        }
    });
}

/// Color Calibration: chromatic adaptation from a chosen illuminant, gamut compression.
pub fn color_cal_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let cc = d.color_cal;
    tool(app, ui, d, "colorCal", "color_cal", "Color Calibration", "Calibrate Colors", cc.enabled, |app, ui, d| {
        let t = Tokens::get(ui.ctx());
        let cats: Vec<(&str, String)> = Adaptation::ALL.iter().map(|a| (a.label(), key_of(*a))).collect();
        let items: Vec<(&str, &str)> = cats.iter().map(|(l, k)| (*l, k.as_str())).collect();
        choice(
            app,
            ui,
            "colorCalCat",
            "Adaptation",
            &items,
            Adaptation::ALL.iter().position(|a| *a == cc.adaptation),
            2,
            |k| json!({"color_cal": {"adaptation": k}}),
        );
        pad(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::tr("Illuminant")).font(t.font(13.0)).color(t.text_dim));
                let r = crate::widgets::dropdown(ui, "colorCalIlluminant", crate::i18n::tr(cc.illuminant.label()), t.font(13.0), t.text_label);
                egui::Popup::menu(&r).show(|ui| {
                    for il in Illuminant::ALL {
                        if ui.selectable_label(il == cc.illuminant, crate::i18n::tr(il.label())).clicked() {
                            merge(app, json!({"color_cal": {"illuminant": key_of(il)}}), "Illuminant");
                            ui.close();
                        }
                    }
                });
            });
        });
        if cc.illuminant == Illuminant::Custom {
            control(app, ui, d, "colorCal.x", true);
            control(app, ui, d, "colorCal.y", true);
        }
        control(app, ui, d, "colorCal.gamut", true);
        pad(ui, |ui| {
            let mut clip = cc.clip;
            if ui.checkbox(&mut clip, crate::i18n::tr("Clip Negative RGB")).changed() {
                merge(app, json!({"color_cal": {"clip": clip}}), "Clip Negative RGB");
            }
        });
    });
}
