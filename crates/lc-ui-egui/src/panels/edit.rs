//! The Edit panel: histogram, Auto/B&W, profile, and the Light / Color / Effects / Detail / Optics
//! sections, built from the engine's control specs.

use egui::epaint::{Mesh, Vertex};
use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{ControlSpec, DevelopSettings, Section, Track, WbMode, controls};
use lightcraft_geom::Point;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::render::Slot;
use crate::theme::Tokens;
use crate::widgets::{BAND_COLORS, SliderOut, divider, flyout_row, hex, register, section_header, text_button};

/// Commit a slider interaction: begin → live updates → end, so a drag is one undo step.
pub fn apply_slider_out(
    app: &mut LightcraftApp,
    spec: &ControlSpec,
    out: SliderOut,
    mut set: impl FnMut(&mut LightcraftApp, f64) -> Result<Value, String>,
) {
    if out.drag_stopped && !out.drag_started && app.ui.dragging_control.as_deref() != Some(spec.id) {
        // egui also reports drag_stopped when a command stopped dragging (selection/undo).
        // Its former widget must not apply the old pointer position to the new settings.
        return;
    }
    if out.drag_started && !out.reset {
        let _ = app.run("develop.beginInteraction", json!({"label": spec.label, "mask": app.session.active_mask}));
        app.ui.dragging_control = Some(spec.id.to_string());
    }
    if let Some(v) = out.value {
        let _ = set(app, v);
    }
    if out.drag_stopped || out.reset {
        let _ = app.run("develop.endInteraction", json!({}));
        app.ui.dragging_control = None;
    }
}

pub(crate) fn control(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, id: &str, enabled: bool) {
    control_t(app, ui, d, id, enabled, Target::Global);
}

/// What the tool panels edit: the photo's own settings, or the tools of a develop layer (a mask,
/// by id). Layer panels read a view of the layer's tools ([`lightcraft_develop::LayerTools::view`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Global,
    Layer(u32),
}

impl Target {
    /// Set controls by id (`{controlId: value}`).
    pub fn set(self, app: &mut LightcraftApp, values: Value) -> Result<Value, String> {
        match self {
            Target::Global => app.run("develop.set", json!({"values": values})),
            Target::Layer(id) => app.run("mask.setTools", json!({"id": id, "values": values})),
        }
    }

    /// Merge partial settings JSON (for a layer: its sections; `null` drops one).
    pub fn merge(self, app: &mut LightcraftApp, partial: Value, label: &str) -> Result<Value, String> {
        match self {
            Target::Global => app.run("develop.merge", json!({"settings": partial, "label": label})),
            Target::Layer(id) => app.run("mask.setTools", json!({"id": id, "tools": partial, "label": label})),
        }
    }

    pub fn mask_id(self) -> Option<u32> {
        match self {
            Self::Global => None,
            Self::Layer(id) => Some(id),
        }
    }

    pub fn is_layer(self) -> bool {
        matches!(self, Target::Layer(_))
    }

    /// A UI id for this target (layer panels keep their own open/closed state).
    fn key(self, id: &str) -> String {
        match self {
            Target::Global => id.to_string(),
            Target::Layer(_) => format!("layer-{id}"),
        }
    }
}

/// A slider for control `id` of `d`, writing to `target`.
pub(crate) fn control_t(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, id: &str, enabled: bool, target: Target) {
    let Some(spec) = controls::find(id) else { return };
    let v = controls::get(d, id).unwrap_or(spec.default);
    let out = crate::widgets::numeric_slider(ui, spec, v, enabled, None, crate::widgets::numeric_owner(app, (target.mask_id(), id)));
    // a double-clicked photo control returns to the photo's own default (a raw's sharpening,
    // As Shot white balance), not the generic one
    let reset = out.reset && target == Target::Global;
    apply_slider_out(app, spec, out, |app, v| match reset {
        true => app.run("develop.resetControl", json!({"control": id})),
        false => target.set(app, json!({id: v})),
    });
    if id == "detail.sharpenMasking" && matches!(target, Target::Global) {
        let preview = app.ui.dragging_control.as_deref() == Some(id) && ui.input(|i| i.modifiers.alt);
        if app.ui.sharpen_mask_preview != preview {
            app.ui.sharpen_mask_preview = preview;
            ui.ctx().request_repaint();
        }
    }
}

/// The tool sections a develop layer can hold too: Light (and the tone curve), Color (white
/// balance, vibrance / saturation, mixer or B&W mix, Point Color, grading), Effects (and vignette,
/// grain), Detail. `d` is the photo's settings (`Target::Global`) or the layer's view; for a layer,
/// `used_only` hides the tools it doesn't set.
pub fn tool_sections(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId, d: &DevelopSettings, raw: bool, target: Target, used_only: bool) {
    let t = Tokens::get(ui.ctx());
    let tools = match target {
        Target::Global => None,
        Target::Layer(mid) => app.session.develop_of(id).and_then(|dd| dd.masks.iter().find(|m| m.id == mid).map(|m| m.tools.clone())),
    };
    // a part of a section is shown unless the layer filter hides what it doesn't set
    let shows = |key: &str| {
        !(target.is_layer() && ["vignette", "grain", "treatment", "bw_mix"].contains(&key))
            && (!used_only || tools.as_ref().is_none_or(|tl| tl.is_set(key)))
    };
    let tl = tools.as_ref();
    tool_section(app, ui, d, target, tl, used_only, "light", "Basic", &["wb", "light", "curve"], |app, ui, d| {
        if shows("wb") {
            {
                // White balance row
                egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 2 }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(li_i18n::library::t("White Balance")).font(t.font(13.0)).color(t.text_dim));
                        if !target.is_layer() {
                            let presets = app.session.active().map(|id| app.session.source_info(id).camera_wb_presets).unwrap_or_default();
                            let selected = presets
                                .iter()
                                .find(|p| d.wb.mode == WbMode::Custom && (d.wb.temp - p.temp).abs() < 0.01 && (d.wb.tint - p.tint).abs() < 0.01);
                            let label = selected.map(|p| p.name.as_str()).unwrap_or(d.wb.mode.label());
                            let r = crate::widgets::dropdown(ui, "wbMode", li_i18n::library::t(label), t.font(14.0), t.text_label);
                            egui::Popup::menu(&r).show(|ui| {
                                for preset in &presets {
                                    let r = ui.selectable_label(selected.is_some_and(|p| p.name == preset.name), li_i18n::library::t(&preset.name));
                                    register(ui.ctx(), format!("wbPreset:{}", preset.name), r.rect);
                                    if r.clicked() {
                                        let _ = app.run("develop.wb", json!({"preset":preset.name}));
                                    }
                                }
                                if !presets.is_empty() {
                                    ui.separator();
                                }
                                for m in WbMode::ALL {
                                    if m == WbMode::Custom {
                                        continue;
                                    }
                                    let r = ui.selectable_label(d.wb.mode == m, m.label());
                                    register(ui.ctx(), format!("wbGeneric:{}", m.label()), r.rect);
                                    if r.clicked() {
                                        let mode = serde_json::to_value(m).unwrap_or_default();
                                        let _ = app.run("develop.wb", json!({"mode": mode}));
                                    }
                                }
                            });
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let active = app.ui.tool == "wbPicker";
                            let resp = crate::widgets::tool_button(ui, "wbPicker", Icon::WhiteBalance, vec2(28.0, 28.0), active, true, "");
                            super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::WbPicker);
                            if resp.clicked() {
                                app.ui.tool = if active { String::new() } else { "wbPicker".into() };
                            }
                        });
                    });
                });
            }
            if raw {
                control_t(app, ui, d, "wb.temp", true, target);
                control_t(app, ui, d, "wb.tint", true, target);
            } else {
                let out = crate::widgets::numeric_slider(
                    ui,
                    &REL_TEMP,
                    k_to_rel(d.wb.temp),
                    true,
                    None,
                    crate::widgets::numeric_owner(app, (target.mask_id(), "relative-temperature")),
                );
                apply_slider_out(app, &REL_TEMP, out, |app, v| target.set(app, json!({"wb.temp": rel_to_k(v)})));
                let out = crate::widgets::numeric_slider(
                    ui,
                    &REL_TINT,
                    d.wb.tint.clamp(-100.0, 100.0),
                    true,
                    None,
                    crate::widgets::numeric_owner(app, (target.mask_id(), "relative-tint")),
                );
                apply_slider_out(app, &REL_TINT, out, |app, v| target.set(app, json!({"wb.tint": v})));
            }
        }
        if shows("light") {
            sub_title(ui, "Tone");
            for c in ["light.exposure", "light.contrast", "light.highlights", "light.shadows", "light.whites", "light.blacks"] {
                control_t(app, ui, d, c, true, target);
            }
            ui.add_space(6.0);
        }
        if shows("curve") {
            let open = app.ui.flyout_open(&target.key("curve"));
            if narrow_flyout(ui, app.ui.right_width, &target.key("curve"), &curve_title(app, target), Icon::Curve, open).clicked() {
                app.ui.toggle_flyout(&target.key("curve"));
            }
            if open {
                curve_editor(app, ui, id, d, target);
            }
        }
        ui.add_space(8.0);
    });
    tool_section(
        app,
        ui,
        d,
        target,
        tl,
        used_only,
        "color",
        "Color",
        &["color", "mixer", "point_colors", "treatment", "bw_mix", "grading", "skin_tone"],
        |app, ui, d| {
            if shows("color") {
                control_t(app, ui, d, "color.vibrance", true, target);
                control_t(app, ui, d, "color.saturation", true, target);
            }
            if target.is_layer() {
                super::layer_bar::global_only(app, ui, "bw", "B&W");
            }
            ui.add_space(6.0);
            if shows("mixer") || shows("bw_mix") || (crate::is_bw(d) && shows("treatment")) {
                let open = app.ui.flyout_open(&target.key("mixer"));
                if flyout_row(ui, &target.key("mixer"), if crate::is_bw(d) { "B&W Mixer" } else { "Color Mixer" }, Icon::Radial, open).clicked() {
                    app.ui.toggle_flyout(&target.key("mixer"));
                }
                if open {
                    mixer(app, ui, d, target);
                }
            }
            if !crate::is_bw(d) && shows("point_colors") {
                let open = app.ui.flyout_open(&target.key("pointColor"));
                if flyout_row(ui, &target.key("pointColor"), li_i18n::library::t("Point Color"), Icon::Picker, open).clicked() {
                    app.ui.toggle_flyout(&target.key("pointColor"));
                }
                if open {
                    point_color(app, ui, d, target);
                }
            }
            if shows("skin_tone") && !crate::is_bw(d) {
                let key = target.key("skinTone");
                let open = app.ui.flyout_open(&key);
                if narrow_flyout(ui, app.ui.right_width, &key, "Skin Tone", Icon::SkinTone, open).clicked() {
                    app.ui.toggle_flyout(&key);
                }
                if open {
                    skin_tone_section(app, ui, d, target);
                }
            }
            if shows("grading") {
                let open = app.ui.flyout_open(&target.key("grading"));
                if flyout_row(ui, &target.key("grading"), li_i18n::library::t("Color Grading"), Icon::Presets, open).clicked() {
                    app.ui.toggle_flyout(&target.key("grading"));
                }
                if open {
                    grading(app, ui, d, target);
                }
            }
            ui.add_space(8.0);
        },
    );
    tool_section(app, ui, d, target, tl, used_only, "effects", "Effects", &["effects", "vignette", "grain"], |app, ui, d| {
        if shows("effects") {
            for c in ["effects.structure", "effects.texture", "effects.clarity", "effects.dehaze"] {
                control_t(app, ui, d, c, true, target);
            }
        }
        if shows("effects") {
            use lightcraft_develop::ClarityMode;
            let modes = [ClarityMode::Natural, ClarityMode::Punch, ClarityMode::Neutral];
            let selected = modes.iter().position(|m| *m == d.effects.clarity_mode);
            let mut chosen = None;
            // Same inset as the Vignette style control, so the buttons fit a 250 px panel.
            egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, |ui| {
                chosen = crate::widgets::segmented(
                    ui,
                    &target.key("clarityMode"),
                    &[("Natural", "natural"), ("Punch", "punch"), ("Neutral", "neutral")],
                    selected,
                    3,
                );
            });
            if let Some(i) = chosen {
                let _ = target.merge(app, json!({"effects":{"clarity_mode":modes[i]}}), "Clarity Mode");
            }
        }
        if target.is_layer() {
            super::layer_bar::global_only(app, ui, "vignette", "Vignette");
            super::layer_bar::global_only(app, ui, "grain", "Grain");
        }
        if shows("vignette") {
            sub_title(ui, li_i18n::library::t("Vignette"));
            use lightcraft_develop::VignetteStyle as V;
            let styles = [
                (V::HighlightPriority, "Highlight", "highlightPriority"),
                (V::ColorPriority, "Color", "colorPriority"),
                (V::PaintOverlay, "Paint", "paintOverlay"),
            ];
            let items: Vec<(&str, &str)> = styles.iter().map(|(_, l, k)| (*l, *k)).collect();
            let active = styles.iter().position(|(v, _, _)| *v == d.vignette.style);
            let mut chosen = None;
            egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, |ui| {
                chosen = crate::widgets::segmented(ui, &target.key("vignetteStyle"), &items, active, 3);
            });
            if let Some(i) = chosen {
                let _ = target.merge(app, json!({"vignette": {"style": serde_json::to_value(styles[i].0).unwrap_or_default()}}), "Vignette Style");
            }
            for c in ["vignette.amount", "vignette.midpoint", "vignette.feather", "vignette.roundness", "vignette.highlights"] {
                let enabled = c == "vignette.amount" || d.vignette.amount != 0.0;
                control_t(app, ui, d, c, enabled, target);
            }
        }
        if shows("grain") {
            sub_title(ui, li_i18n::library::t("Grain"));
            for c in ["grain.amount", "grain.size", "grain.roughness"] {
                control_t(app, ui, d, c, c == "grain.amount" || d.grain.amount != 0.0, target);
            }
        }
        ui.add_space(8.0);
    });
    tool_section(app, ui, d, target, tl, used_only, "detail", "Detail", &["detail"], |app, ui, d| {
        for c in ["detail.sharpenAmount", "detail.sharpenRadius", "detail.sharpenDetail", "detail.sharpenMasking"] {
            control_t(app, ui, d, c, c == "detail.sharpenAmount" || d.detail.sharpen_amount > 0.0, target);
        }
        if target == Target::Global {
            super::enhance::denoise_controls(app, ui, d);
        }
        sub_title(ui, li_i18n::library::t("Noise Reduction"));
        for c in ["detail.nrLuminance", "detail.nrDetail", "detail.nrContrast", "detail.nrColor", "detail.nrColorDetail", "detail.nrColorSmoothness"]
        {
            control_t(app, ui, d, c, true, target);
        }
        ui.add_space(8.0);
    });
}

/// A tool section: the photo's ([`section`], with its on/off eye) or a layer's (its own open
/// state; a dot when the layer sets any of `keys`; right-click resets them; hidden by the "only
/// tools this layer uses" filter when it sets none).
#[allow(clippy::too_many_arguments)]
fn tool_section(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    d: &DevelopSettings,
    target: Target,
    tools: Option<&lightcraft_develop::LayerTools>,
    used_only: bool,
    id: &str,
    title: &str,
    keys: &[&str],
    body: impl FnOnce(&mut LightcraftApp, &mut egui::Ui, &DevelopSettings),
) {
    let Target::Layer(mid) = target else {
        section(app, ui, d, id, title, body);
        return;
    };
    let set: Vec<&str> =
        keys.iter().copied().filter(|k| !["vignette", "grain", "treatment", "bw_mix"].contains(k) && tools.is_some_and(|t| t.is_set(k))).collect();
    if used_only && set.is_empty() {
        return;
    }
    let key = target.key(id);
    let open = app.ui.section_open(&key);
    let title_with_layer = app.session.layer_label(title, Some(mid));
    let (resp, _) = section_header(ui, &key, &title_with_layer, open, None);
    if !set.is_empty() {
        // the layer sets tools here
        let t = Tokens::get(ui.ctx());
        let galley_w = ui.painter().layout_no_wrap(li_i18n::library::t(title).to_string(), t.semibold(14.0), t.text).size().x;
        ui.painter().circle_filled(pos2(resp.rect.left() + 52.0 + galley_w, resp.rect.center().y), 3.5, t.accent);
        register(ui.ctx(), format!("layerSectionSet:{id}"), resp.rect);
        resp.clone().on_hover_text(format!("{} sets {title}", title_with_layer));
    }
    resp.context_menu(|ui| {
        let r = ui.add_enabled(!set.is_empty(), egui::Button::new(li_i18n::library::t("Reset Section")));
        if r.clicked() {
            let drop: serde_json::Map<String, Value> = set.iter().map(|k| (k.to_string(), Value::Null)).collect();
            let _ = Target::Layer(mid).merge(app, Value::Object(drop), "Reset Layer Section");
            ui.close();
        }
    });
    if resp.clicked() {
        app.ui.toggle_section(&key);
    }
    if open {
        body(app, ui, d);
    }
    divider(ui);
}

/// Relative temperature scale for rendered (non-raw) files: −100..100 ↔ Kelvin via mired shift.
const REL_TEMP: ControlSpec = ControlSpec {
    id: "wb.tempRel",
    label: "Temp",
    section: Section::Color,
    min: -100.0,
    max: 100.0,
    default: 0.0,
    step: 1.0,
    decimals: 0,
    track: Track::Temp,
};
const REL_TINT: ControlSpec = ControlSpec {
    id: "wb.tintRel",
    label: "Tint",
    section: Section::Color,
    min: -100.0,
    max: 100.0,
    default: 0.0,
    step: 1.0,
    decimals: 0,
    track: Track::Tint,
};

fn k_to_rel(k: f64) -> f64 {
    ((1e6 / 6500.0 - 1e6 / k) / 0.8).clamp(-100.0, 100.0)
}
fn rel_to_k(r: f64) -> f64 {
    1e6 / (1e6 / 6500.0 - r * 0.8)
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    super::layer_bar::show(app, ui, id);
    let d = app.session.develop_of(id).unwrap_or_default();
    let target = app.session.active_mask.map(Target::Layer).unwrap_or(Target::Global);
    // a raw shown from its embedded JPEG (preview only) gets the rendered-file white balance scale
    let raw = app.session.catalog.photo(id).is_some_and(|p| p.develops_raw() && !p.relative_wb());
    let mut d = d;
    if d.wb.mode == WbMode::AsShot && app.session.catalog.photo(id).is_some_and(|p| p.relative_wb()) {
        let wb = &mut std::sync::Arc::make_mut(&mut d).wb;
        wb.temp = 6500.0;
        wb.tint = 0.0;
    }
    let preview_only = app.session.catalog.photo(id).and_then(|p| p.preview_only.clone());

    if app.ui.histogram {
        histogram(app, ui, id);
    }
    if app.ui.soft_proof {
        soft_proofing(app, ui, id);
    }
    if let Target::Layer(mid) = target {
        let Some(m) = d.masks.iter().find(|m| m.id == mid) else { return };
        let info = app.session.source_info(id);
        let view = m.tools.view(lightcraft_engine::pipeline::local::effective_wb(&info, &d));
        tool_sections(app, ui, id, &view, raw, target, app.ui.layer_used_only);
        super::masking::local_section(app, ui, m);
        // one collapsed line instead of five repeated "whole image" blocks
        super::layer_bar::global_group(
            app,
            ui,
            &[
                ("profile", "Profile & RAW"),
                ("negative", "Negative"),
                ("toneEq", "Tone Equalizer"),
                ("optics", "Optics"),
                ("calibration", "Calibration"),
            ],
        );
        return;
    }
    // header: Edit + Auto / B&W / HDR
    let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().text(pos2(hr.left() + 24.0, hr.bottom() - 10.0), Align2::LEFT_CENTER, li_i18n::library::t("Edit"), t.semibold(15.0), t.text);
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 8, bottom: 14 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if text_button(ui, "auto", li_i18n::library::t("Auto"), false).clicked() {
                let _ = app.run("develop.auto", json!({}));
                app.toast(ui.ctx(), "Auto settings applied");
            }
            let bw = crate::is_bw(&d);
            if text_button(ui, "bw", li_i18n::library::t("B&W"), bw).clicked() {
                let _ = app.run("develop.treatment", json!({}));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if text_button(ui, "reset", li_i18n::library::t("Reset"), false)
                    .on_hover_text(format!(
                        "{} ({})",
                        li_i18n::library::t("Reset all edits"),
                        crate::shortcuts::label_in(ui.ctx(), "develop.reset").unwrap_or_default()
                    ))
                    .clicked()
                {
                    let _ = app.run("develop.reset", json!({}));
                }
            });
        });
    });
    if let Some(why) = &preview_only {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 0, bottom: 12 }).show(ui, |ui| {
            crate::widgets::preview_only_notice(ui, "edit", why);
        });
    }
    let n = app.session.selection.ids.len();
    if n > 1 && matches!(app.ui.view, crate::state::ViewMode::PhotoGrid | crate::state::ViewMode::SquareGrid) {
        quick_develop(app, ui, n);
    }
    if n > 1 {
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(24, 6)).show(ui, |ui| {
            if text_button(ui, "syncSettings", li_i18n::library::t("Sync Settings…"), false).clicked() {
                let _ = app.run("dialog.syncSettings", json!({}));
            }
        });
    }
    if app.session.auto_sync && n > 1 {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 0, bottom: 10 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(li_i18n::tr_format!("Auto Sync: edits apply to {n} photos", n = n)).color(t.accent).size(12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if text_button(ui, "autoSyncOff", li_i18n::library::t("Turn Off"), false).clicked() {
                        let _ = app.run("develop.autoSync", json!({"on": false}));
                    }
                });
            });
        });
    }
    divider(ui);
    // local-image: a layered document's layers (read-only)
    crate::panels::doc_layers::show(app, ui, id);
    // profile row
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 14, bottom: 14 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(li_i18n::library::t("Profile")).font(t.font(13.0)).color(t.text_dim));
            let name = lightcraft_engine::presets::profile(&d.profile.id).map(|p| p.name).unwrap_or("Color");
            let r = crate::widgets::dropdown(ui, "profile", li_i18n::library::t(name), t.font(15.0), t.text_label);
            egui::Popup::menu(&r).show(|ui| profile_menu(app, ui, &d));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::widgets::icon_button(ui, "profileBrowser", Icon::ProfileGrid, vec2(28.0, 28.0), false, true, "Browse Profiles").clicked() {
                    let _ = app.run("panel.profiles", json!({}));
                }
            });
        });
    });
    crate::panels::profiles::camera_profile_row(app, ui, id, &d);
    if d.profile.id != "lc.color" {
        control(app, ui, &d, "profile.amount", true);
        ui.add_space(6.0);
    }
    if app.session.catalog.photo(id).is_some_and(|p| p.develops_raw()) {
        crate::panels::process::look_row(app, ui, &d);
    }
    divider(ui);
    tool_sections(app, ui, id, &d, raw, Target::Global, false);
    crate::panels::develop_tools::tone_eq_section(app, ui, &d);
    if raw {
        crate::panels::develop_tools::raw_section(app, ui, &d);
    }
    section(app, ui, &d, "optics", "Optics", |app, ui, d| {
        crate::panels::profiles::lens_profile_row(app, ui, id, d);
        let source = app.session.source_info(id);
        let embedded = app.session.catalog.photo(id).and_then(|p| source.embedded_lens(f64::from(p.width), f64::from(p.height)));
        crate::widgets::develop_body(ui, |ui| {
            let mut ca = d.optics.remove_ca;
            if crate::widgets::checkbox(ui, "optics:removeCa", &mut ca, li_i18n::library::t("Remove Chromatic Aberration")).changed() {
                let _ = app.run("develop.merge", json!({"settings": {"optics": {"remove_ca": ca}}, "label": "Remove CA"}));
            }
            let mut lp = d.optics.lens_profile;
            if !crate::panels::host_session::hides(app, "lensProfile") {
                if crate::widgets::checkbox(ui, "optics:lensProfile", &mut lp, li_i18n::library::t("Enable Lens Corrections")).changed() {
                    let settings = if lp && embedded.is_none() && !source.embedded_lens_baked {
                        json!({"optics": {"lens_profile": true}, "lens_db": {"enabled": true}})
                    } else {
                        json!({"optics": {"lens_profile": lp}})
                    };
                    let _ = app.run("develop.merge", json!({"settings": settings, "label": "Lens Corrections"}));
                }
                let status = if source.embedded_lens_baked {
                    "Built-in DNG lens corrections are already applied."
                } else if embedded.is_some() {
                    "Embedded lens corrections available. These take priority over the lens database."
                } else if source.lcp.is_some() && source.lens_db.is_none() {
                    "Imported lens correction profile available."
                } else {
                    "No embedded lens corrections. Use the lens database for a fallback profile."
                };
                crate::widgets::panel_note(ui, li_i18n::library::t(status));
            }
        });
        if d.optics.lens_profile && (embedded.is_some() || source.lcp.is_some()) && !source.embedded_lens_baked {
            let lcp = source.lcp.filter(|_| embedded.is_none() && source.lens_db.is_none());
            let distortion = embedded.is_some_and(|l| l.warp.is_some() || l.radial.is_some_and(|r| r.count > 0))
                || lcp.is_some_and(|l| l.geometric_model().is_some());
            let vignetting = embedded.is_some_and(|l| l.vignette.is_some() || l.radial.is_some_and(|r| r.vig_count > 0))
                || lcp.is_some_and(|l| l.vignette.is_some());
            control(app, ui, d, "optics.profileDistortion", distortion);
            control(app, ui, d, "optics.profileVignetting", vignetting);
        }
        if !crate::panels::host_session::hides(app, "lensProfile") {
            crate::panels::develop_tools::lens_db_rows(app, ui, id, d);
        }
        sub_title(ui, li_i18n::library::t("Manual"));
        for c in ["optics.distortion", "optics.vignetting"] {
            control(app, ui, d, c, true);
        }
        control(app, ui, d, "optics.vignettingMidpoint", d.optics.vignetting != 0.0);
        ui.add_space(6.0);
        let open = app.ui.flyout_open("defringe");
        if flyout_row(ui, "defringe", li_i18n::library::t("Defringe"), Icon::Defringe, open).clicked() {
            app.ui.toggle_flyout("defringe");
        }
        if open {
            control(app, ui, d, "optics.defringePurple", true);
            let on = d.optics.defringe_purple_amount > 0.0;
            control(app, ui, d, "optics.defringePurpleHueLo", on);
            control(app, ui, d, "optics.defringePurpleHueHi", on);
            control(app, ui, d, "optics.defringeGreen", true);
            let on = d.optics.defringe_green_amount > 0.0;
            control(app, ui, d, "optics.defringeGreenHueLo", on);
            control(app, ui, d, "optics.defringeGreenHueHi", on);
            sub_title(ui, li_i18n::library::t("Lateral Chromatic Aberration"));
            control(app, ui, d, "optics.caRed", true);
            control(app, ui, d, "optics.caBlue", true);
        }
        ui.add_space(8.0);
    });
    // Display order does not change the engine's conversion order or saved settings.
    if d.negative.enabled || app.ui.negative_tool_visible {
        negative_section(app, ui, &d);
    }
    crate::widgets::panel_content(ui, |ui| {
        let add = text_button(ui, "addTool", "Add tool", false);
        egui::Popup::menu(&add).show(|ui| {
            if text_button(ui, "addNegative", "Negative", app.ui.negative_tool_visible || d.negative.enabled).clicked() {
                app.ui.negative_tool_visible = true;
                if !app.ui.section_open("negative") {
                    app.ui.toggle_section("negative");
                }
                ui.close();
            }
        });
    });
    crate::panels::develop_tools::color_cal_section(app, ui, &d);
    // Lightroom Classic's Calibration panel (the cloud app hides it): last, like there.
    section(app, ui, &d, "calibration", "Calibration", |app, ui, d| {
        sub_title(ui, li_i18n::library::t("Shadows"));
        control(app, ui, d, "calibration.shadowsTint", true);
        for (title, k) in [("Red Primary", "red"), ("Green Primary", "green"), ("Blue Primary", "blue")] {
            sub_title(ui, title);
            control(app, ui, d, &format!("calibration.{k}Hue"), true);
            control(app, ui, d, &format!("calibration.{k}Sat"), true);
        }
        ui.add_space(8.0);
    });
    ui.add_space(40.0);
}

/// The Negative section (film negative conversion, a port of darktable's negadoctor): film stock,
/// film base colour with its picker, film range, virtual paper and print, colour cast correction.
/// The header's eye button (and the Convert Negative checkbox) is the conversion's on/off switch.
fn negative_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    use lightcraft_develop::FilmStock;
    let t = Tokens::get(ui.ctx());
    let n = d.negative;
    let on = n.enabled && d.section_enabled("negative");
    let set_on = |app: &mut LightcraftApp, on: bool| {
        let _ = app.run("develop.merge", json!({"settings": {"negative": {"enabled": on}}, "label": "Negative"}));
        if on && !app.session.active().and_then(|id| app.session.develop_of(id)).is_some_and(|d| d.section_enabled("negative")) {
            let _ = app.run("develop.sectionEnabled", json!({"section": "negative", "enabled": true}));
        }
    };
    let open = app.ui.section_open("negative");
    // (the eye shows once a conversion is set up: most photos are not negatives, and a crossed-out
    // eye on every one of them would read as a hidden section)
    let (resp, toggled) = section_header(ui, "negative", "Negative", open, n.enabled.then_some(on));
    if let Some(v) = toggled {
        set_on(app, v);
    } else if resp.clicked() {
        app.ui.toggle_section("negative");
    }
    if !open {
        divider(ui);
        return;
    }
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, |ui| {
        let mut v = on;
        let r = crate::widgets::checkbox(ui, "negativeEnabled", &mut v, "Convert Negative");
        register(ui.ctx(), "negative:enabled", r.rect);
        if r.changed() {
            set_on(app, v);
        }
    });
    // film stock
    let stocks: Vec<(&str, &str)> = FilmStock::ALL
        .iter()
        .map(|f| {
            (
                f.label(),
                match f {
                    FilmStock::Color => "color",
                    FilmStock::Bw => "bw",
                    FilmStock::Slide => "slide",
                },
            )
        })
        .collect();
    let mut chosen = None;
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, |ui| {
        ui.label(egui::RichText::new(li_i18n::library::t("Film")).font(t.font(13.0)).color(t.text_dim));
        chosen = crate::widgets::segmented(ui, "negativeFilm", &stocks, FilmStock::ALL.iter().position(|f| *f == n.film), 3);
    });
    if let Some(i) = chosen {
        let _ = app.run("develop.merge", json!({"settings": {"negative": {"film": stocks[i].1}}, "label": "Film"}));
    }
    if n.film == FilmStock::Slide {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 10 }).show(ui, |ui| {
            ui.label(egui::RichText::new(li_i18n::library::t("Slides are positives: the scan is used as it is.")).size(11.0).color(t.text_dim));
        });
        divider(ui);
        return;
    }
    // film base: swatch, picker, components
    sub_title(ui, "Film Base");
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 0, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let base = match n.film {
                FilmStock::Bw => [n.dmin.r; 3],
                _ => n.dmin.to_array(),
            };
            let srgb = li_color::REC2020.to_space(&li_color::SRGB).apply(base);
            let enc = srgb.map(|v| li_color::transfer::encode_srgb8(v.clamp(0.0, 1.0) as f32));
            crate::widgets::color_swatch(ui, "negative:filmBase", Color32::from_rgb(enc[0], enc[1], enc[2]), vec2(40.0, 24.0), None, "Film base");
            let picking = app.ui.tool == "negDmin";
            let (icon_rect, icon_response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            crate::icons::paint_tool(ui, icon_rect, "pipette", Icon::Picker, t.icon, true);
            let resp = text_button(ui, "negativePickBase", "Pick from the Film Rim", picking).union(icon_response);
            super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::FilmBase);
            if resp.clicked() {
                app.ui.tool = if picking { String::new() } else { "negDmin".into() };
            }
        });
    });
    if n.film == FilmStock::Bw {
        if let Some(spec) = controls::find("negative.dminR") {
            let out = crate::widgets::numeric_slider(ui, spec, n.dmin.r, on, Some("Film Base"), crate::widgets::numeric_owner(app, "film-base"));
            apply_slider_out(app, spec, out, |app, v| {
                app.run("develop.set", json!({"values": {"negative.dminR": v, "negative.dminG": v, "negative.dminB": v}}))
            });
        }
    } else {
        for c in ["negative.dminR", "negative.dminG", "negative.dminB"] {
            control(app, ui, d, c, on);
        }
    }
    // the film's density range and the scanner's exposure
    let auto_clicked = |ui: &mut egui::Ui, title: &str, key: &str, tip: &str| -> bool {
        let mut clicked = false;
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 8, bottom: 2 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(li_i18n::library::t(title)).font(t.semibold(13.0)).color(t.text_label));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    clicked = text_button(ui, key, "Auto", false).on_hover_text(li_i18n::library::t(tip)).clicked();
                });
            });
        });
        clicked
    };
    let run_auto = |app: &mut LightcraftApp, ui: &egui::Ui, command: &str| {
        if let Err(e) = app.run(command, json!({})) {
            app.toast(ui.ctx(), e);
        }
    };
    if auto_clicked(ui, "Film Range", "negativeAutoDmax", "Estimate D-max and the scan exposure from the photo (crop away the film rim first)") {
        run_auto(app, ui, "develop.negative.autoDmax");
    }
    for c in ["negative.dMax", "negative.offset"] {
        control(app, ui, d, c, on);
    }
    if auto_clicked(ui, "Virtual Print", "negativeAutoPrint", "Estimate paper black and print exposure so the print spans black to white") {
        run_auto(app, ui, "develop.negative.autoExposure");
    }
    for c in ["negative.black", "negative.gamma", "negative.softClip", "negative.exposure"] {
        control(app, ui, d, c, on);
    }
    // colour cast correction (white balance of the print's shadows and highlights)
    ui.add_space(6.0);
    let fly = app.ui.flyout_open("negativeColor");
    if flyout_row(ui, "negativeColor", "Color Correction", Icon::Picker, fly).clicked() {
        app.ui.toggle_flyout("negativeColor");
    }
    if fly {
        sub_title(ui, "Shadows");
        for c in ["negative.wbLowR", "negative.wbLowG", "negative.wbLowB"] {
            control(app, ui, d, c, on);
        }
        sub_title(ui, "Highlights");
        for c in ["negative.wbHighR", "negative.wbHighG", "negative.wbHighB"] {
            control(app, ui, d, c, on);
        }
    }
    ui.add_space(8.0);
    divider(ui);
}

/// The profile dropdown: Favorites, Recent, one submenu per group, then favourite toggle and
/// Browse….
fn profile_menu(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    use lightcraft_engine::presets::{PROFILES, profile, profile_groups};
    let t = Tokens::get(ui.ctx());
    ui.set_min_width(200.0);
    let cur = d.profile.id.as_str();
    // (clicked, hovered)
    let mut pick: (Option<&'static str>, Option<&'static str>) = (None, None);
    let item = |ui: &mut egui::Ui,
                key: &str,
                p: &'static lightcraft_engine::presets::ProfileInfo,
                pick: &mut (Option<&'static str>, Option<&'static str>)| {
        let r = ui.selectable_label(p.id == cur, li_i18n::library::t(p.name));
        register(ui.ctx(), format!("profileMenu:{key}:{}", p.id), r.rect);
        if r.clicked() {
            pick.0 = Some(p.id);
        }
        if r.hovered() {
            pick.1 = Some(p.id);
        }
    };
    let heading = |ui: &mut egui::Ui, s: &str| {
        ui.label(egui::RichText::new(li_i18n::library::t(s)).font(t.semibold(11.5)).color(t.text_dim));
    };
    for (title, key, ids) in [("Favorites", "fav", app.session.profile_favorites.clone()), ("Recent", "recent", app.session.profile_recent.clone())] {
        let list: Vec<_> = ids.iter().filter_map(|id| profile(id)).collect();
        if list.is_empty() {
            continue;
        }
        heading(ui, title);
        for p in list {
            item(ui, key, p, &mut pick);
        }
        ui.separator();
    }
    for g in profile_groups() {
        let r = ui.menu_button(li_i18n::library::t(g), |ui| {
            ui.set_min_width(170.0);
            for p in PROFILES.iter().filter(|p| p.group == g) {
                item(ui, "group", p, &mut pick);
            }
        });
        register(ui.ctx(), format!("profileMenu:groupMenu:{g}"), r.response.rect);
    }
    ui.separator();
    if let Some(p) = profile(cur) {
        let fav = app.session.profile_favorites.iter().any(|f| f == p.id);
        let label = if fav {
            li_i18n::tr_format!("Remove “{}” from Favorites", li_i18n::library::t(p.name))
        } else {
            li_i18n::tr_format!("Add “{}” to Favorites", li_i18n::library::t(p.name))
        };
        let r = ui.button(label);
        register(ui.ctx(), "profileMenu:toggleFavorite", r.rect);
        if r.clicked() {
            let _ = app.run("profile.favorite", json!({"id": p.id}));
        }
    }
    let r = ui.button(li_i18n::library::t("Browse…"));
    register(ui.ctx(), "profileMenu:browse", r.rect);
    if r.clicked() {
        let _ = app.run("panel.profiles", json!({}));
    }
    // resting on a profile previews it in the loupe
    if let Some(p) = pick.1.and_then(profile)
        && p.id != cur
    {
        let mut s = d.clone();
        s.profile.id = p.id.to_string();
        s.profile.amount = 100.0;
        app.hover_preview = Some(crate::HoverPreview { label: li_i18n::tr_format!("Profile: {}", li_i18n::library::t(p.name)), settings: s });
    }
    if let Some(id) = pick.0 {
        let _ = app.run("develop.profile", json!({"id": id}));
    }
}

pub fn sub_title(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 24.0, r.center().y + 4.0), Align2::LEFT_CENTER, li_i18n::library::t(title), t.semibold(13.0), t.text_label);
}

fn section(
    app: &mut LightcraftApp,
    ui: &mut egui::Ui,
    d: &DevelopSettings,
    id: &str,
    title: &str,
    body: impl FnOnce(&mut LightcraftApp, &mut egui::Ui, &DevelopSettings),
) {
    // local-image: tools a Camera Raw Filter session can't use
    if crate::panels::host_session::hides(app, id) {
        return;
    }
    let open = app.ui.section_open(id);
    let (resp, toggled) = section_header(ui, id, title, open, Some(d.section_enabled(id)));
    if let Some(on) = toggled {
        let _ = app.run("develop.sectionEnabled", json!({"section": id, "enabled": on}));
    } else if resp.clicked() {
        app.ui.toggle_section(id);
    }
    if open {
        body(app, ui, d);
    }
    divider(ui);
}

// histogram

fn histogram(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    // Borrow cached bins: no image-sized work or per-frame histogram/smoothing allocation.
    let hist = app
        .renderer
        .textures
        .get(&Slot::Main)
        .filter(|t| t.photo == id)
        .and_then(|t| t.histogram.as_ref())
        .or_else(|| app.renderer.textures.get(&Slot::Thumb(id)).and_then(|t| t.histogram.as_ref()));
    let view = hist.map(|h| li_ui_kit::histogram::Histogram::u32([&h.r, &h.g, &h.b], Some(&h.luma)));
    let resp =
        li_ui_kit::histogram::library(ui, view, hist.map_or((0.0, 0.0), |h| h.clipping()), app.ui.show_clipping, li_i18n::library::t("Histogram"));
    let r = resp.rect;
    register(ui.ctx(), "histogram", r);
    if resp.clicked() {
        app.ui.show_clipping = !app.ui.show_clipping;
    }
    let p = ui.painter();
    // EXIF strip
    if let Some(ph) = app.session.catalog.photo(id) {
        let m = &ph.meta;
        let items = [
            m.iso.map(|i| format!("ISO {i}")).unwrap_or_default(),
            m.focal_mm.map(|f| format!("{f:.0}mm")).unwrap_or_default(),
            m.aperture.map(|a| format!("f/{a:.1}")).unwrap_or_default(),
            m.shutter.clone(),
        ];
        let y = r.bottom() - 13.0;
        for (i, s) in items.iter().enumerate() {
            let x = r.left() + 18.0 + (r.width() - 36.0) * i as f32 / 3.0;
            let align = match i {
                0 => Align2::LEFT_CENTER,
                3 => Align2::RIGHT_CENTER,
                _ => Align2::CENTER_CENTER,
            };
            p.text(pos2(x, y), align, s, t.font(12.5), t.text_label);
        }
    }
}

// soft proofing

/// The Soft Proofing strip under the histogram: proof profile, gamut warnings, Create Proof Copy.
fn soft_proofing(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    use crate::widgets::{checkbox, choice, develop_body, field_with_action, panel_note};
    use lightcraft_engine::pipeline::{OutputSpace, ProofIntent};
    develop_body(ui, |ui| {
        panel_note(ui, "Soft Proofing");
        panel_note(ui, app.ui.proof.label());
        let mut space = app.ui.proof.profile.is_none().then_some(app.ui.proof.space);
        let mut options: Vec<_> = OutputSpace::ALL.into_iter().map(|s| (Some(s), s.label())).collect();
        if app.ui.proof.profile.is_some() {
            options.push((None, app.ui.proof.label()));
        }
        if choice(ui, "proof-space", &mut space, &options)
            && let Some(space) = space
        {
            let _ = app.run("view.softProof", json!({"space":space}));
        }
        let path_id = ui.id().with("proof-profile-path");
        let mut path = ui.ctx().data_mut(|d| d.get_temp::<String>(path_id).unwrap_or_default());
        if field_with_action(ui, "proof-profile", &mut path, "ICC profile path", "Load ICC Profile", true)
            && let Err(error) = app.run("view.softProof", json!({"profilePath":path}))
        {
            app.toast(ui.ctx(), error);
        }
        ui.ctx().data_mut(|d| d.insert_temp(path_id, path));
        let mut intent = app.ui.proof.intent;
        if choice(
            ui,
            "proof-intent",
            &mut intent,
            &[
                (ProofIntent::RelativeColorimetric, "Relative Colorimetric"),
                (ProofIntent::Perceptual, "Perceptual"),
                (ProofIntent::Saturation, "Saturation"),
                (ProofIntent::AbsoluteColorimetric, "Absolute Colorimetric"),
            ],
        ) {
            let _ = app.run("view.softProof", json!({"intent":intent}));
        }
        let mut bpc = app.ui.proof.bpc;
        if checkbox(ui, "proofBpc", &mut bpc, "Black Point Compensation").changed() {
            let _ = app.run("view.softProof", json!({"bpc":bpc}));
        }
        let mut paper = app.ui.proof.simulate_paper;
        if checkbox(ui, "proofPaper", &mut paper, "Simulate Paper & Ink").changed() {
            let _ = app.run("view.softProof", json!({"simulatePaper":paper}));
        }
        let mut dest = app.ui.proof.dest_warning;
        if checkbox(ui, "proofDestWarning", &mut dest, "Destination Gamut Warning").changed() {
            let _ = app.run("view.softProof", json!({"destWarning":dest}));
        }
        let mut display = app.ui.proof.display_warning;
        if checkbox(ui, "proofDisplayWarning", &mut display, "Monitor Gamut Warning").changed() {
            let _ = app.run("view.softProof", json!({"displayWarning":display}));
        }
        if text_button(ui, "createProofCopy", "Create Proof Copy", false).clicked() {
            let name = li_i18n::tr_format!("Proof Copy ({})", app.ui.proof.label());
            if app.run("photo.virtualCopy", json!({"ids": [id.0], "name": name})).is_ok() {
                app.toast(ui.ctx(), "Proof copy created");
            }
        }
    });
    divider(ui);
}

/// Skin selection shown in the same UCS22 coordinates and ranges as the tool.
fn skin_color(reference: [f64; 3]) -> Color32 {
    use lightcraft_pipeline::ucs;
    let c = [reference[0] as f32, reference[1] as f32, (reference[2] as f32).to_radians()];
    let to_rgb = |c| {
        let xyz = ucs::xyy_to_xyz(ucs::jch_to_xyy(c, ucs::y_to_l_star(1.0)));
        li_color::SRGB.from_xyz().apply_f32(xyz)
    };
    let mut rgb = to_rgb(c);
    // Range strips deliberately include out-of-display-gamut samples. Reduce chroma at the
    // same J/h for their UI swatches rather than clipping channels into a different hue.
    if !rgb.iter().all(|v| (0.0..=1.0).contains(v)) {
        let (mut lo, mut hi) = (0.0, c[1]);
        for _ in 0..10 {
            let mid = (lo + hi) * 0.5;
            let candidate = to_rgb([c[0], mid, c[2]]);
            if candidate.iter().all(|v| (0.0..=1.0).contains(v)) {
                lo = mid;
                rgb = candidate;
            } else {
                hi = mid;
            }
        }
    }
    let c = rgb.map(|v| (li_color::transfer::linear_to_srgb(v.max(0.0)).clamp(0.0, 1.0) * 255.0).round() as u8);
    Color32::from_rgb(c[0], c[1], c[2])
}
fn skin_tone_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, target: Target) {
    let t = Tokens::get(ui.ctx());
    // the panel's row inset, as Point Color and every slider row
    let inset = egui::Margin { left: 24, right: 22, top: 4, bottom: 4 };
    ui.scope(|ui| {
        ui.set_width(ui.available_width().min(app.ui.right_width - 2.0));
        let picking = app.ui.tool == "skinTone";
        egui::Frame::NONE.inner_margin(inset).show(ui, |ui| {
            ui.horizontal(|ui| {
                let pick = crate::widgets::tool_button(
                    ui,
                    "skinTone",
                    Icon::SkinTone,
                    vec2(28.0, 28.0),
                    picking,
                    true,
                    "Pick a skin reference on the photo",
                );
                // Preserve the control-channel id used before this button gained its glyph.
                register(ui.ctx(), format!("button:{}", target.key("skinTonePick")), pick.rect);
                if pick.clicked() {
                    app.ui.tool = if picking { String::new() } else { "skinTone".into() };
                    app.ui.point_color_mask = target.mask_id();
                }
                ui.label("Pick Skin Reference");
            });
            if let Some(reference) = d.skin_tone.reference {
                ui.horizontal(|ui| {
                    crate::widgets::color_swatch(ui, &target.key("skinToneSwatch"), skin_color(reference), vec2(32.0, 32.0), None, "Skin reference");
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!("J {:.0} · C {:.0} · h {:.0}°", reference[0] * 100.0, reference[1] * 100.0, reference[2]))
                                .font(t.font(11.0))
                                .color(t.text_label),
                        );
                        if text_button(ui, &target.key("skinToneClear"), "Clear", false).clicked() {
                            let _ = target.merge(app, json!({"skin_tone":{"reference":null}}), "Clear Skin Tone");
                            app.ui.skin_tone_visualize = false;
                        }
                    });
                });
            } else {
                ui.label(RichText::new("Pick a skin tone on the photo to enable").font(t.font(11.0)).color(t.text_label));
            }
        });
        for id in ["skinTone.uniformity", "skinTone.lightness", "skinTone.hueRange", "skinTone.chromaRange", "skinTone.lightnessRange"] {
            control_t(app, ui, d, id, d.skin_tone.reference.is_some(), target);
        }
        if let Some(reference) = d.skin_tone.reference {
            for (axis, label, extent, domain) in [
                (2, "Hue", d.skin_tone.hue_range, 360.0),
                (1, "Saturation", 0.04 + d.skin_tone.chroma_range / 100.0 * 0.4, 1.0),
                (0, "Lightness", 0.05 + d.skin_tone.lightness_range / 100.0 * 0.5, 1.0),
            ] {
                // The selected window, wrapping hue at 0/360.
                let shifts: &[f64] = if axis == 2 { &[-domain, 0.0, domain] } else { &[0.0] };
                let windows: Vec<(f32, f32)> = shifts
                    .iter()
                    .map(|shift| {
                        let lo = (reference[axis] - extent + shift).clamp(0.0, domain) / domain;
                        let hi = (reference[axis] + extent + shift).clamp(0.0, domain) / domain;
                        (lo as f32, hi as f32)
                    })
                    .collect();
                egui::Frame::NONE.inner_margin(egui::Margin { top: 2, bottom: 2, ..inset }).show(ui, |ui| {
                    let sample = |f: f32| {
                        let mut c = reference;
                        c[axis] = f as f64 * domain;
                        skin_color(c)
                    };
                    crate::widgets::range_strip(ui, &target.key(&format!("skinToneRange:{label}")), label, sample, &windows);
                });
            }
        }
        egui::Frame::NONE.inner_margin(inset).show(ui, |ui| {
            ui.add_enabled_ui(d.skin_tone.reference.is_some(), |ui| {
                let mut visible = app.ui.skin_tone_visualize && app.ui.skin_tone_mask == target.mask_id();
                let r = ui.checkbox(&mut visible, "View selected range");
                register(ui.ctx(), target.key("skinToneViewRange"), r.rect);
                if r.changed() {
                    app.ui.skin_tone_visualize = visible;
                    app.ui.skin_tone_mask = target.mask_id();
                }
                let mut protect = d.skin_tone.protect_lips;
                let r = ui.checkbox(&mut protect, "Protect Lips");
                register(ui.ctx(), target.key("skinProtectLips"), r.rect);
                if r.changed() {
                    let _ = target.merge(app, json!({"skin_tone":{"protect_lips":protect}}), "Skin Tone");
                }
            });
        });
    });
}

// tone curve

/// Reset tone curve channel `ch` (`parametric`, `master`, `red`, `green`, `blue` or `all`) of
/// `target`.
fn reset_curve(app: &mut LightcraftApp, target: Target, ch: &str) {
    if !target.is_layer() {
        let _ = app.run("curve.reset", json!({"channel": ch}));
        return;
    }
    let d = lightcraft_develop::ToneCurve::default();
    let partial = match ch {
        "all" => json!({"curve": null}),
        "parametric" => json!({"curve": {"highlights": d.highlights, "lights": d.lights, "darks": d.darks, "shadows": d.shadows,
            "split_shadows": d.split_shadows, "split_mid": d.split_mid, "split_highlights": d.split_highlights}}),
        ch => json!({"curve": {ch: []}}),
    };
    let _ = target.merge(app, partial, "Reset Tone Curve");
}

// Some existing RAW controls request more space than the narrow panel. These tools retain
// the user's panel width even if a preceding row has grown egui's available rectangle.
fn narrow_flyout(ui: &mut egui::Ui, width: f32, id: &str, title: &str, icon: Icon, open: bool) -> egui::Response {
    ui.scope(|ui| {
        ui.set_width(ui.available_width().min(width - 2.0));
        flyout_row(ui, id, title, icon, open)
    })
    .inner
}

fn curve_title(app: &LightcraftApp, target: Target) -> String {
    if let Target::Layer(mid) = target
        && let Some(d) = app.session.active().and_then(|id| app.session.develop_of(id))
        && let Some(m) = d.masks.iter().find(|m| m.id == mid)
    {
        return format!("Curve - {}", m.name);
    }
    li_i18n::library::t("Curve").to_string()
}

fn curve_line(p: &egui::Painter, line: Vec<Pos2>, color: Color32, t: Tokens) {
    p.add(egui::Shape::line(line.clone(), Stroke::new(5.0, t.grid_bg)));
    p.add(egui::Shape::line(line, Stroke::new(2.75, color)));
}

fn curve_editor(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId, d: &DevelopSettings, target: Target) {
    let t = Tokens::get(ui.ctx());
    ui.scope(|ui| {
        ui.set_width(ui.available_width().min(app.ui.right_width - 2.0));
        // channel selector
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 6, bottom: 4 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (ch, c) in [
                    ("parametric", t.text),
                    ("master", t.curve_color("master")),
                    ("red", t.curve_color("red")),
                    ("green", t.curve_color("green")),
                    ("blue", t.curve_color("blue")),
                ] {
                    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    register(ui.ctx(), format!("curveChannel:{ch}"), r);
                    let sel = app.ui.curve_channel == ch;
                    if sel {
                        ui.painter().rect_filled(r, 3.0, t.tool_active);
                    }
                    if ch == "parametric" {
                        paint(ui.painter(), r.shrink(4.0), Icon::Curve, if sel { t.text } else { t.icon });
                    } else {
                        ui.painter().circle_stroke(r.center(), 7.0, Stroke::new(2.0, c));
                        if sel {
                            ui.painter().circle_filled(r.center(), 3.5, c);
                        }
                    }
                    if sel {
                        ui.painter().line_segment([r.left_bottom() + vec2(4.0, 1.0), r.right_bottom() + vec2(-4.0, 1.0)], Stroke::new(2.0, t.text));
                    }
                    let name = match ch {
                        "parametric" => "Parametric curve",
                        "master" => "Point curve",
                        "red" => "Red channel",
                        "green" => "Green channel",
                        _ => "Blue channel",
                    };
                    let resp = resp.on_hover_text(li_i18n::tr_format!("{name} — double-click to reset it", name = name));
                    if resp.double_clicked() {
                        app.ui.curve_channel = ch.into();
                        reset_curve(app, target, ch);
                    } else if resp.clicked() {
                        app.ui.curve_channel = ch.into();
                    }
                }
                if !target.is_layer() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        tat_button(app, ui, "tat:curve", "Targeted adjustment: drag up/down on the photo to adjust the curve there");
                    });
                }
            });
        });
        let ch = app.ui.curve_channel.clone();
        let w = ui.available_width();
        // Keep the preferred 240 pt plot where it fits; narrow panels reduce margins first.
        let side = (w - 48.0).max(240.0).min((w - 16.0).max(80.0));
        let (outer, _) = ui.allocate_exact_size(vec2(w, side + 12.0), Sense::hover());
        let r = Rect::from_min_size(pos2(outer.left() + (w - side) / 2.0, outer.top() + 6.0), vec2(side, side));
        let resp = ui.interact(r, egui::Id::new(("curve", &ch)), Sense::click_and_drag());
        register(ui.ctx(), "curve", r);
        resp.context_menu(|ui| curve_reset_menu(app, ui, &ch, target));
        let p = ui.painter();
        p.rect_filled(r, 2.0, t.grid_bg);
        p.rect_stroke(r, 2.0, Stroke::new(1.0, t.button_border), egui::StrokeKind::Inside);
        if let Some(h) = app
            .renderer
            .textures
            .get(&Slot::Main)
            .filter(|t| t.photo == id)
            .and_then(|t| t.histogram.as_ref())
            .or_else(|| app.renderer.textures.get(&Slot::Thumb(id)).and_then(|t| t.histogram.as_ref()))
        {
            let values = match ch.as_str() {
                "red" => &h.r,
                "green" => &h.g,
                "blue" => &h.b,
                _ => &h.luma,
            };
            let peak = values.iter().skip(2).take(252).copied().max().unwrap_or(1).max(1) as f32;
            let color = t.text_dim.gamma_multiply(0.18);
            for (i, value) in values.iter().enumerate() {
                let x = r.left() + r.width() * i as f32 / 255.0;
                let y = r.bottom() - r.height() * (*value as f32 / peak).sqrt().min(1.0);
                p.line_segment([pos2(x, r.bottom()), pos2(x, y)], Stroke::new(r.width() / 255.0 + 0.2, color));
            }
        }
        for i in 1..4 {
            let f = i as f32 / 4.0;
            p.line_segment([pos2(r.left() + r.width() * f, r.top()), pos2(r.left() + r.width() * f, r.bottom())], Stroke::new(1.0, t.field_border));
            p.line_segment([pos2(r.left(), r.top() + r.height() * f), pos2(r.right(), r.top() + r.height() * f)], Stroke::new(1.0, t.field_border));
        }
        p.add(egui::Shape::dashed_line(&[r.left_bottom(), r.right_top()], Stroke::new(1.0, t.text_dim.gamma_multiply(0.35)), 5.0, 4.0));
        let to_screen = |x: f64, y: f64| pos2(r.left() + x as f32 * r.width(), r.bottom() - y as f32 * r.height());
        let from_screen = |q: Pos2| (((q.x - r.left()) / r.width()).clamp(0.0, 1.0) as f64, ((r.bottom() - q.y) / r.height()).clamp(0.0, 1.0) as f64);
        if ch == "parametric" {
            let tables = lightcraft_pipeline::finish::curve_luts(&d.curve);
            let line = (0..=256)
                .map(|i| {
                    let x = i as f32 / 256.0;
                    to_screen(x as f64, tables.as_ref().map_or(x, |c| c.tables[0].eval(x)) as f64)
                })
                .collect();
            // Region dividers use the actual pipeline split positions.
            let splits = [d.curve.split_shadows, d.curve.split_mid, d.curve.split_highlights];
            for (i, split) in splits.into_iter().enumerate() {
                let x = r.left() + r.width() * split as f32 / 100.0;
                p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.0, t.text_dim.gamma_multiply(0.5)));
                let handle = Rect::from_center_size(pos2(x, r.bottom() - 5.0), vec2(12.0, 12.0));
                let response = ui.interact(handle, egui::Id::new(("curve-split", target.key("curve"), i)), Sense::drag());
                register(ui.ctx(), format!("curveSplit:{i}"), handle);
                p.circle_filled(handle.center(), if response.hovered() { 5.0 } else { 4.0 }, t.text_label);
                p.circle_stroke(handle.center(), 4.0, Stroke::new(1.0, t.grid_bg));
                if response.drag_started() {
                    let _ = app.run("develop.beginInteraction", json!({"label":"Curve Regions","mask":target.mask_id()}));
                }
                if response.dragged()
                    && let Some(q) = response.interact_pointer_pos()
                {
                    let lo = if i == 0 { 1.0 } else { splits[i - 1] + 1.0 };
                    let hi = if i == 2 { 99.0 } else { splits[i + 1] - 1.0 };
                    let value = ((q.x - r.left()) / r.width() * 100.0) as f64;
                    let key = ["split_shadows", "split_mid", "split_highlights"][i];
                    let _ = target.merge(app, json!({"curve":{key:value.clamp(lo,hi)}}), "Curve Regions");
                }
                if response.drag_stopped() {
                    let _ = app.run("develop.endInteraction", json!({}));
                }
            }
            curve_line(p, line, t.curve_color("master"), t);
            ui.horizontal(|ui| {
                let mut mode = d.curve.mode;
                ui.selectable_value(&mut mode, lightcraft_develop::CurveMode::Luminance, li_i18n::library::t("Luminance"));
                ui.selectable_value(&mut mode, lightcraft_develop::CurveMode::Rgb, "RGB");
                if mode != d.curve.mode {
                    let _ = target.merge(app, json!({"curve":{"mode":mode}}), "Curve Mode");
                }
            });
            curve_footer(app, ui, d, target);
            for c in ["curve.highlights", "curve.lights", "curve.darks", "curve.shadows"] {
                control_t(app, ui, d, c, true, target);
            }
            control_t(app, ui, d, "curve.refineSaturation", true, target);
            return;
        }
        let pts_of = |d: &DevelopSettings| -> Vec<Point> {
            let v = match ch.as_str() {
                "red" => &d.curve.red,
                "green" => &d.curve.green,
                "blue" => &d.curve.blue,
                _ => &d.curve.master,
            };
            // in the order the curve is drawn (points from a sidecar, a preset or an agent can
            // come unsorted or not finite): the drag below keeps each point between its neighbours
            let mut v: Vec<Point> = v.iter().copied().filter(|q| q.x.is_finite() && q.y.is_finite()).collect();
            v.sort_by(|a, b| a.x.total_cmp(&b.x));
            if v.len() < 2 { vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)] } else { v }
        };
        let pts = pts_of(d);
        let curve = li_color::spline::MonotoneCurve::new(&pts.iter().map(|q| (q.x, q.y)).collect::<Vec<_>>());
        let color = t.curve_color(&ch);
        let line: Vec<Pos2> = (0..=256).map(|i| to_screen(i as f64 / 256.0, curve.eval(i as f64 / 256.0).clamp(0.0, 1.0))).collect();
        curve_line(p, line, color, t);
        // interaction: drag a point (both axes, between its neighbours), drag empty space to add and
        // drag a new point, click empty space to add, double-click a point to delete it
        let drag_id = egui::Id::new(("curve-drag", &ch));
        // the point being dragged (stored as `Option<usize>` so a stale value can be cleared)
        let mut dragging: Option<usize> = ui.data(|dd| dd.get_temp::<Option<usize>>(drag_id)).flatten();
        let nearest = |q: Pos2| pts.iter().enumerate().map(|(i, p)| (i, to_screen(p.x, p.y).distance(q))).min_by(|a, b| a.1.total_cmp(&b.1));
        let hovered = resp.hover_pos().and_then(nearest).filter(|(_, dist)| *dist < 10.0).map(|(i, _)| i);
        if dragging.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        for (i, q) in pts.iter().enumerate() {
            let c = to_screen(q.x, q.y);
            let radius = if dragging == Some(i) || (dragging.is_none() && hovered == Some(i)) { 8.0 } else { 6.0 };
            p.circle_filled(c, radius, color);
            p.circle_stroke(c, radius, Stroke::new(1.5, t.grid_bg));
            if i == 0 || i + 1 == pts.len() {
                p.circle_filled(c, 2.5, t.grid_bg);
            }
        }
        // input / output readout of the point being dragged (0–255 like the histogram)
        if let Some(q) = dragging.and_then(|i| pts.get(i)) {
            p.text(
                r.left_top() + vec2(6.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!("{} / {}", (q.x * 255.0).round(), (q.y * 255.0).round()),
                t.font(11.0),
                t.text_dim,
            );
        }
        let mut new_pts = None;
        if resp.double_clicked()
            && let Some(q) = resp.interact_pointer_pos()
            && let Some((i, dist)) = nearest(q)
            && dist < 10.0
            && i != 0
            && i != pts.len() - 1
        {
            let mut v = pts.clone();
            v.remove(i);
            new_pts = Some(v);
        } else if resp.drag_started()
            && let Some(q) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
        {
            // pick the point under the press, not where the pointer is once the drag threshold passed
            let _ = app.run("develop.beginInteraction", json!({"label": "Tone Curve","mask":target.mask_id()}));
            match nearest(q) {
                Some((i, dist)) if dist < 10.0 => dragging = Some(i),
                _ => {
                    let (x, y) = from_screen(q);
                    let mut v = pts.clone();
                    let i = v.iter().position(|p| p.x > x).unwrap_or(v.len());
                    v.insert(i, Point::new(x, y));
                    dragging = Some(i);
                    new_pts = Some(v);
                }
            }
        } else if resp.dragged()
            && let (Some(i), Some(q)) = (dragging, resp.interact_pointer_pos())
        {
            let (mut x, y) = from_screen(q);
            let mut v = pts.clone();
            if i < v.len() {
                let lo = if i == 0 { 0.0 } else { v[i - 1].x + 0.01 };
                let hi = if i + 1 >= v.len() { 1.0 } else { v[i + 1].x - 0.01 };
                if i == 0 {
                    x = x.min(hi);
                } else if i + 1 == v.len() {
                    x = x.max(lo);
                } else if lo <= hi {
                    x = x.clamp(lo, hi);
                } else {
                    // neighbours closer than the 0.02 spacing leave no room: the point stays
                    // midway (`clamp` would panic: min above max)
                    x = (v[i - 1].x + v[i + 1].x) / 2.0;
                }
                v[i] = Point::new(x, y);
                new_pts = Some(v);
            }
        } else if resp.clicked()
            && let Some(q) = resp.interact_pointer_pos()
            && nearest(q).is_none_or(|(_, dist)| dist >= 10.0)
        {
            let (x, y) = from_screen(q);
            let mut v = pts.clone();
            let i = v.iter().position(|p| p.x > x).unwrap_or(v.len());
            v.insert(i, Point::new(x, y));
            new_pts = Some(v);
        }
        if let Some(v) = new_pts {
            match target {
                Target::Global => {
                    let arr: Vec<[f64; 2]> = v.iter().map(|p| [p.x, p.y]).collect();
                    let _ = app.run("develop.curve", json!({"channel": ch, "points": arr}));
                }
                Target::Layer(_) => {
                    let key = if ["red", "green", "blue"].contains(&ch.as_str()) { ch.as_str() } else { "master" };
                    let _ = target.merge(app, json!({"curve": {key: v}}), "Tone Curve");
                }
            }
        }
        if resp.drag_stopped() {
            dragging = None;
            let _ = app.run("develop.endInteraction", json!({}));
        }
        ui.data_mut(|dd| dd.insert_temp(drag_id, dragging));
        curve_footer(app, ui, d, target);
        control_t(app, ui, d, "curve.refineSaturation", true, target);
        let _ = id;
    });
}

/// The row under the curve graph: point-curve presets and reset every curve.
fn curve_footer(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, target: Target) {
    use lightcraft_engine::cmd::curves::{all_presets, matching_preset};
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(li_i18n::library::t("Point Curve")).font(t.font(12.0)).color(t.text_label));
            let current = matching_preset(&app.session, &d.curve);
            let builtin = current.as_ref().is_none_or(|name| all_presets(&app.session).iter().any(|p| p.builtin && &p.name == name));
            let label = li_i18n::library::builtin_label(current.as_deref().unwrap_or("Custom"), builtin);
            let r = crate::widgets::dropdown(ui, "curvePreset", label, t.font(12.0), t.text);
            egui::Popup::menu(&r).show(|ui| {
                ui.set_min_width(180.0);
                let presets = all_presets(&app.session);
                let mut user_seen = false;
                for p in &presets {
                    if !p.builtin && !user_seen {
                        user_seen = true;
                        ui.separator();
                    }
                    let r = ui.selectable_label(current.as_deref() == Some(p.name.as_str()), li_i18n::library::builtin_label(&p.name, p.builtin));
                    register(ui.ctx(), format!("curvePreset:{}", p.name), r.rect);
                    if r.clicked() {
                        let _ = app.run("curve.applyPreset", json!({"name": p.name, "mask":target.mask_id()}));
                        ui.close();
                    }
                    if !p.builtin {
                        r.context_menu(|ui| {
                            if ui.button(li_i18n::library::t("Delete Preset")).clicked() {
                                let _ = app.run("curve.deletePreset", json!({"name": p.name}));
                                ui.close();
                            }
                        });
                    }
                }
                ui.separator();
                let shaped = current.as_deref() != Some("Linear");
                let r = ui.add_enabled(shaped, egui::Button::new(li_i18n::library::t("Save Point Curve…")));
                register(ui.ctx(), "curvePresetMenu:save", r.rect);
                if r.clicked() {
                    crate::panels::dialogs::prompt(
                        app,
                        "Save Point Curve Preset",
                        "Preset name",
                        "",
                        "curve.savePreset",
                        json!({"mask":target.mask_id()}),
                        "name",
                    );
                    ui.close();
                }
                if ui.add_enabled(app.services.file_dialog.is_some(), egui::Button::new(li_i18n::library::t("Import Presets…"))).clicked() {
                    let _ = app.run("file.importCurvePresets", json!({}));
                    ui.close();
                }
                let can_export = app.services.file_dialog.is_some() && !app.session.curve_presets.is_empty();
                if ui.add_enabled(can_export, egui::Button::new(li_i18n::library::t("Export Presets…"))).clicked() {
                    let _ = app.run("file.exportCurvePresets", json!({}));
                    ui.close();
                }
                if presets.iter().any(|p| !p.builtin) {
                    ui.label(RichText::new(li_i18n::library::t("Right-click a preset to delete it")).font(t.font(11.0)).color(t.text_dim));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = text_button(ui, "curveReset", li_i18n::library::t("Reset"), false).on_hover_text(li_i18n::library::t(
                    "Reset all curves: point curves (all channels) and parametric (double-click a channel to reset only that one)",
                ));
                if r.clicked() {
                    reset_curve(app, target, "all");
                }
            });
        });
    });
}

/// Right-click menu of the curve graph.
fn curve_reset_menu(app: &mut LightcraftApp, ui: &mut egui::Ui, ch: &str, target: Target) {
    let label = if ch == "parametric" { "Reset Parametric Curve".to_string() } else { li_i18n::tr_format!("Reset {} Channel", channel_label(ch)) };
    let r = ui.button(label);
    register(ui.ctx(), "curveMenu:resetChannel", r.rect);
    if r.clicked() {
        reset_curve(app, target, ch);
        ui.close();
    }
    let r = ui.button(li_i18n::library::t("Reset All Curves"));
    register(ui.ctx(), "curveMenu:resetAll", r.rect);
    if r.clicked() {
        reset_curve(app, target, "all");
        ui.close();
    }
}

fn channel_label(ch: &str) -> &'static str {
    match ch {
        "red" => "Red",
        "green" => "Green",
        "blue" => "Blue",
        _ => "RGB",
    }
}

/// Toggle for a targeted-adjustment tool (`tool` = `tat:<target>`).
fn tat_button(app: &mut LightcraftApp, ui: &mut egui::Ui, tool: &str, _tip: &str) {
    let active = app.ui.tool == tool;
    let id = tool.replace(':', "-");
    let resp = crate::widgets::tool_button(ui, &id, Icon::Target, vec2(26.0, 26.0), active, true, "");
    super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::Target);
    if resp.clicked() {
        app.ui.tool = if active { String::new() } else { tool.to_string() };
    }
}

// colour mixer

fn mixer(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, target: Target) {
    let t = Tokens::get(ui.ctx());
    let bands = lightcraft_develop::MIXER_BANDS;
    let sel = bands.iter().position(|b| *b == app.ui.mixer_mode).unwrap_or(0);
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 8, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            for (i, b) in bands.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                register(ui.ctx(), format!("mixerBand:{b}"), r);
                ui.painter().circle_filled(r.center(), 9.0, hex(BAND_COLORS[i]));
                if i == sel {
                    ui.painter().circle_stroke(r.center(), 10.5, Stroke::new(1.5, t.text));
                    ui.painter().circle_filled(r.center(), 2.0, Color32::WHITE);
                }
                if resp.clicked() {
                    app.ui.mixer_mode = b.to_string();
                }
            }
        });
    });
    // targeted adjustment: pick the attribute, then drag on the photo (the photo's mixer only)
    let tat = !target.is_layer();
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 2 }).show(ui, |ui| {
        if !tat {
            return;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            paint(ui.painter(), r, Icon::Target, t.icon);
            let items: &[(&str, &str)] =
                if crate::is_bw(d) { &[("lum", "B&W Mix")] } else { &[("hue", "Hue"), ("sat", "Saturation"), ("lum", "Luminance")] };
            for (k, label) in items {
                let tool = format!("tat:{k}");
                let active = app.ui.tool == tool;
                if text_button(ui, &format!("tatMixer-{k}"), label, active)
                    .on_hover_text(li_i18n::library::t("Targeted adjustment: drag up/down on the photo to adjust the colours there"))
                    .clicked()
                {
                    app.ui.tool = if active { String::new() } else { tool };
                }
            }
        });
    });
    let b = bands[sel];
    if crate::is_bw(d) {
        if tat {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                if crate::widgets::text_button(ui, "bwAuto", li_i18n::library::t("Auto"), false)
                    .on_hover_text(li_i18n::library::t("Set the mix from the photo's colours"))
                    .clicked()
                {
                    let _ = app.run("develop.autoBwMix", json!({}));
                }
            });
        }
        control_t(app, ui, d, &format!("bw.{b}"), true, target);
    } else {
        for k in ["hue", "sat", "lum"] {
            control_t(app, ui, d, &format!("mixer.{b}.{k}"), true, target);
        }
    }
}

// point color

/// Display colour of an OkLCh sample (lightness, chroma, hue in degrees).
fn oklch_color(l: f64, c: f64, h_deg: f64) -> Color32 {
    use li_color::perceptual::{lch_to_lab, oklab_to_2020};
    let lin = oklab_to_2020(lch_to_lab([l as f32, c as f32, (h_deg as f32).to_radians()]));
    let s = li_color::REC2020.to_space(&li_color::SRGB).apply_f32(lin);
    let e = s.map(|v| (li_color::transfer::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8);
    Color32::from_rgb(e[0], e[1], e[2])
}

/// Point Color: swatches of the samples (+ the eyedropper), the selected sample's shifts and range,
/// and "Visualize range".
fn point_color(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, target: Target) {
    let layer = match target {
        Target::Layer(m) => Some(m),
        Target::Global => None,
    };
    let t = Tokens::get(ui.ctx());
    let n = d.point_colors.len();
    if app.ui.point_color >= n && n > 0 {
        app.ui.point_color = n - 1;
    }
    let sel = app.ui.point_color;
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 8, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let active = app.ui.tool == "pointColor" && app.ui.point_color_mask == layer;
            let full = n >= lightcraft_develop::MAX_POINT_COLORS;
            let resp = crate::widgets::tool_button(ui, "pointColorPicker", Icon::Picker, vec2(26.0, 26.0), active, !full, "");
            super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::PointColor);
            if resp.clicked() {
                app.ui.tool = if active { String::new() } else { "pointColor".into() };
                app.ui.point_color_mask = layer;
            }
            for (i, p) in d.point_colors.iter().enumerate() {
                let label = format!("Point color sample {}", i + 1);
                let resp = crate::widgets::color_swatch(
                    ui,
                    &format!("pointColor:{i}"),
                    oklch_color(p.lum, p.chroma, p.hue),
                    vec2(22.0, 22.0),
                    Some(i == sel),
                    &label,
                );
                if resp.clicked() {
                    app.ui.point_color = i;
                }
            }
        });
    });
    if n == 0 {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 2, bottom: 8 }).show(ui, |ui| {
            ui.label(egui::RichText::new(li_i18n::library::t("Pick a colour on the photo with the eyedropper.")).size(12.0).color(t.text_dim));
        });
        return;
    }
    for k in ["hueShift", "satShift", "lumShift", "variance"] {
        control_t(app, ui, d, &format!("pointColor.{sel}.{k}"), true, target);
    }
    sub_title(ui, li_i18n::library::t("Range"));
    for k in ["range", "hueRange", "satRange", "lumRange"] {
        control_t(app, ui, d, &format!("pointColor.{sel}.{k}"), true, target);
    }
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 8 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            if layer.is_none() {
                let mut v = app.ui.point_color_visualize;
                if ui.checkbox(&mut v, li_i18n::library::t("Visualize range")).changed() {
                    app.ui.point_color_visualize = v;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if text_button(ui, "pointColorDelete", li_i18n::library::t("Delete"), false).clicked() {
                    let mut q = json!({"index": sel});
                    if let Some(m) = layer {
                        q["mask"] = json!(m);
                    }
                    let _ = app.run("pointColor.delete", q);
                }
            });
        });
    });
}

// colour grading

fn grading(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, target: Target) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    let (area, _) = ui.allocate_exact_size(vec2(w, 250.0), Sense::hover());
    let big = 44.0;
    let small = 38.0;
    let wheels = [
        ("midtones", "Midtones", pos2(area.center().x, area.top() + 60.0), big),
        ("shadows", "Shadows", pos2(area.left() + w * 0.28, area.top() + 170.0), small),
        ("highlights", "Highlights", pos2(area.left() + w * 0.72, area.top() + 170.0), small),
    ];
    for (key, label, c, rad) in wheels {
        let wh = match key {
            "shadows" => d.grading.shadows,
            "midtones" => d.grading.midtones,
            _ => d.grading.highlights,
        };
        let r = Rect::from_center_size(c, vec2(rad * 2.0, rad * 2.0));
        let resp = ui.interact(r, egui::Id::new(("wheel", key)), Sense::click_and_drag());
        register(ui.ctx(), format!("wheel:{key}"), r);
        paint_wheel(ui.painter(), c, rad);
        let a = (wh.hue as f32).to_radians();
        let dot = c + vec2(a.cos(), -a.sin()) * rad * (wh.sat as f32 / 100.0);
        ui.painter().circle_filled(dot, 5.0, Color32::WHITE);
        ui.painter().circle_stroke(dot, 5.0, Stroke::new(1.0, Color32::BLACK));
        ui.painter().text(pos2(c.x, c.y + rad + 12.0), Align2::CENTER_CENTER, li_i18n::library::t(label), t.font(12.0), t.text_label);
        if resp.drag_started() {
            let _ =
                app.run("develop.beginInteraction", json!({"label": li_i18n::tr_format!("{label} Grading", label = label),"mask":target.mask_id()}));
        }
        if (resp.dragged() || resp.clicked())
            && let Some(q) = resp.interact_pointer_pos()
        {
            let v = q - c;
            let hue = ((-v.y).atan2(v.x).to_degrees() + 360.0) % 360.0;
            let sat = (v.length() / rad * 100.0).min(100.0);
            let _ = target.set(app, json!({format!("grading.{key}.hue"): hue.round(), format!("grading.{key}.sat"): sat.round()}));
        }
        if resp.drag_stopped() || resp.clicked() {
            let _ = app.run("develop.endInteraction", json!({}));
        }
        if resp.double_clicked() {
            let _ = target.set(app, json!({format!("grading.{key}.hue"): 0, format!("grading.{key}.sat"): 0}));
        }
    }
    for c in ["grading.shadows.lum", "grading.midtones.lum", "grading.highlights.lum", "grading.blending", "grading.balance"] {
        control_t(app, ui, d, c, true, target);
    }
}

fn paint_wheel(p: &egui::Painter, c: Pos2, rad: f32) {
    let mut mesh = Mesh::default();
    let n = 48;
    mesh.vertices.push(Vertex { pos: c, uv: Pos2::ZERO, color: Color32::from_gray(128) });
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let rgb = li_color::perceptual::hsv_to_rgb(a.to_degrees(), 0.75, 0.8);
        let col = Color32::from_rgb((rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8);
        mesh.vertices.push(Vertex { pos: c + vec2(a.cos(), -a.sin()) * rad, uv: Pos2::ZERO, color: col });
        if i > 0 {
            mesh.indices.extend([0, i as u32, i as u32 + 1]);
        }
    }
    p.add(mesh);
    p.circle_stroke(c, rad, Stroke::new(1.0, Color32::from_gray(40)));
}

/// Quick Develop (grid with several photos selected): relative steps applied to every selected
/// photo from its own value.
fn quick_develop(app: &mut LightcraftApp, ui: &mut egui::Ui, n: usize) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 0, bottom: 10 }).show(ui, |ui| {
        ui.label(egui::RichText::new(li_i18n::tr_format!("Quick Develop · {n} photos", n = n)).color(t.text_label).size(12.5));
        ui.add_space(4.0);
        let rows: [(&str, &str, f64, f64); 9] = [
            ("Exposure", "light.exposure", 1.0 / 3.0, 1.0),
            ("Contrast", "light.contrast", 5.0, 20.0),
            ("Highlights", "light.highlights", 5.0, 20.0),
            ("Shadows", "light.shadows", 5.0, 20.0),
            ("Whites", "light.whites", 5.0, 20.0),
            ("Blacks", "light.blacks", 5.0, 20.0),
            ("Clarity", "effects.clarity", 5.0, 20.0),
            ("Vibrance", "color.vibrance", 5.0, 20.0),
            ("Temp", "wb.temp", 100.0, 500.0),
        ];
        egui::Grid::new("quick-develop").num_columns(5).spacing([4.0, 3.0]).show(ui, |ui| {
            for (label, ctl, small, big) in rows {
                ui.label(egui::RichText::new(li_i18n::library::t(label)).size(11.5).color(t.text_dim));
                for (txt, d) in [("◀◀", -big), ("◀", -small), ("▶", small), ("▶▶", big)] {
                    let r = ui.add(egui::Button::new(egui::RichText::new(txt).size(10.0)).min_size(egui::vec2(28.0, 18.0)));
                    crate::widgets::register(ui.ctx(), format!("button:quick-{ctl}-{txt}"), r.rect);
                    if r.on_hover_text(li_i18n::tr_format!("{label} {d:+} on every selected photo", d = d, label = label)).clicked() {
                        let _ = app.run("develop.quickAdjust", json!({"control": ctl, "delta": d}));
                    }
                }
                ui.end_row();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Services,
        headless::Headless,
        state::{RightPanel, ViewMode},
    };
    use std::time::{Duration, Instant};
    const WAIT: Duration = Duration::from_secs(40);
    fn click(h: &mut Headless, id: &str) {
        let r = h.request("ui.clickWidget", json!({"id":id}), WAIT);
        assert_eq!(r["ok"], true, "{id}: {r}");
        h.step();
    }
    #[test]
    fn proof_controls_click_load_icc_and_fit_all_themes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cmyk-proof");
        std::fs::create_dir_all(&dir).unwrap();
        let profile_path = dir.join("test-printer.icc");
        std::fs::write(&profile_path, include_bytes!("../../../pc-cms/profiles/photocraft-coated-cmyk.icc")).unwrap();
        for dark in [true, false] {
            for width in [250.0, 400.0] {
                for scale in [1.0, 1.5] {
                    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
                    app.ui.view = ViewMode::Detail;
                    app.ui.right = RightPanel::Edit;
                    app.ui.soft_proof = true;
                    app.ui.histogram = false;
                    app.ui.left_panel = false;
                    app.ui.filmstrip = false;
                    app.ui.right_width = width;
                    app.set_host_theme(if dark { Tokens::default() } else { crate::tests_develop_c::light() });
                    let mut h = Headless::new(app, [1200.0, 1100.0], scale);
                    h.step();
                    h.step();
                    click(&mut h, "dropdown:proof-intent");
                    click(&mut h, "button:proof-intent-1");
                    assert_eq!(h.app.ui.proof.intent, lightcraft_pipeline::ProofIntent::Perceptual);
                    click(&mut h, "check:proofBpc");
                    assert!(!h.app.ui.proof.bpc);
                    click(&mut h, "check:proofPaper");
                    assert!(h.app.ui.proof.simulate_paper);
                    click(&mut h, "check:proofDestWarning");
                    click(&mut h, "check:proofDisplayWarning");
                    assert!(h.app.ui.proof.dest_warning && h.app.ui.proof.display_warning);
                    click(&mut h, "field:proof-profile");
                    let r = h.request("ui.text", json!({"text":profile_path.to_str().unwrap()}), WAIT);
                    assert_eq!(r["ok"], true, "{r}");
                    h.step();
                    click(&mut h, "button:proof-profile");
                    assert!(h.app.ui.proof.profile.is_some());
                    let n = h.app.session.catalog.photos().count();
                    click(&mut h, "button:createProofCopy");
                    assert_eq!(h.app.session.catalog.photos().count(), n + 1);
                    let copy = h.app.session.catalog.photos().max_by_key(|p| p.id.0).unwrap();
                    assert_eq!(copy.copy_name.as_deref(), Some(format!("Proof Copy ({})", h.app.ui.proof.label()).as_str()));
                    let before = h.app.ui.proof.clone();
                    let r = h.request(
                        "engine.execute",
                        json!({"command":"view.softProof", "params":{"profilePath":dir.join("absent.icc"),"bpc":true}}),
                        WAIT,
                    );
                    assert_ne!(r["ok"], true);
                    assert_eq!(before, h.app.ui.proof);
                    // The created copy retains the active proof view; ensure every control stays
                    // within the actual narrow dock at both display scales.
                    h.step();
                    h.step();
                    let panel = h.app.widgets.iter().find(|(id, _)| id == "panel:right_panel").unwrap().1;
                    for (id, r) in &h.app.widgets {
                        if id.contains("proof") || id == "button:createProofCopy" {
                            assert!(
                                r.left() >= panel.left() - 1.0 && r.right() <= panel.right() + 1.0,
                                "{id} at width {width}: {r:?} outside {panel:?}"
                            );
                        }
                    }
                    assert!(h.settle(WAIT));
                    let image = h.paint();
                    let x0 = (panel.left() * scale) as usize;
                    let x1 = (panel.right() * scale) as usize;
                    let y1 = ((panel.top() + 500.0) * scale) as usize;
                    let crop = li_raster::Rgba8::from_fn(x1 - x0, y1, |x, y| image.pixels[y * image.size[0] + x + x0].to_array());
                    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&crop), &Default::default()).unwrap();
                    std::fs::write(dir.join(format!("proof-{}-{}-{scale}.png", if dark { "dark" } else { "light" }, width as u32)), png).unwrap();
                }
            }
        }
    }

    #[test]
    fn proof_interactive_render_job_small_sample() {
        let mut session = lightcraft_engine::Session::with_demo();
        let id = session.active().unwrap_or(lightcraft_catalog::PhotoId(1));
        session.selection = lightcraft_engine::Selection::single(id);
        let stages = std::sync::Arc::new(lightcraft_pipeline::StageCache::default());
        let start = Instant::now();
        let ordinary = session.render_job(id, 192, 128, false, true).unwrap().with_stages(stages.clone()).run();
        ordinary.rendered.as_ref().unwrap();
        let ordinary_ms = start.elapsed().as_secs_f64() * 1000.0;
        session.accept(&ordinary);
        let proof = lightcraft_pipeline::Proof {
            profile: Some(
                lightcraft_pipeline::ProofProfile::try_from(std::sync::Arc::new(
                    include_bytes!("../../../pc-cms/profiles/photocraft-coated-cmyk.icc").to_vec(),
                ))
                .unwrap(),
            ),
            ..Default::default()
        };
        let cold_start = Instant::now();
        let cold = session.render_job(id, 192, 128, false, true).unwrap().with_stages(stages.clone()).with_proof(Some(proof.clone())).run();
        cold.rendered.as_ref().unwrap();
        let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
        let warm_start = Instant::now();
        let warm = session.render_job(id, 192, 128, false, true).unwrap().with_stages(stages.clone()).with_proof(Some(proof.clone())).run();
        let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(cold.rendered.unwrap().image, warm.rendered.unwrap().image);
        let edit_start = Instant::now();
        session.execute("develop.set", &json!({"control":"light.exposure", "value":0.2})).unwrap();
        let edited = session.render_job(id, 192, 128, false, true).unwrap().with_stages(stages).with_proof(Some(proof)).draft().run();
        edited.rendered.unwrap();
        let edit_ms = edit_start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "real RenderJob (192x128 fit): ordinary preview {ordinary_ms:.3} ms; CPU CMYK proof cold {cold_ms:.3} ms; cached proof {warm_ms:.3} ms; exposure edit + draft {edit_ms:.3} ms; GPU adapter {:?}",
            lightcraft_engine::gpu::adapter_name()
        );
    }
}
