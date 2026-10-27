//! File › New: Photoshop's New Document dialog. Category tabs with blank-document presets on the
//! left, Preset Details on the right. Values live in the dialog fields (`width`/`height` in pixels,
//! `resolution` in ppi, `mode`, `depth`, `background`, `name`), so `ui.dialog.set` drives it and
//! Create runs `file.new` with them.

use egui::{Align2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use serde_json::{Map, Value, json};

use crate::theme::Tokens;
use crate::{icons, widgets};

const REMEMBERED: &str = "file.new";
const RECENT_LIMIT: usize = 8;

/// Only durable document choices, never names, paths or dialog runtime state.
fn choices(fields: &Map<String, Value>) -> Map<String, Value> {
    fields
        .iter()
        .filter(|(key, value)| match key.as_str() {
            "width" | "height" => value.as_u64().is_some_and(|n| (1..=300_000).contains(&n)),
            "resolution" => value.as_f64().is_some_and(|n| n.is_finite() && (1.0..=30_000.0).contains(&n)),
            "mode" => value.as_str().is_some_and(|s| ["rgb", "gray", "cmyk", "lab"].contains(&s)),
            "depth" => value.as_u64().is_some_and(|n| [8, 16, 32].contains(&n)),
            "background" => value.as_str().is_some_and(|s| ["white", "black", "backgroundColor", "transparent"].contains(&s)),
            "designUnits" => serde_json::from_value::<photocraft_doc::vector_design::structure::DesignUnits>((*value).clone()).is_ok(),
            _ => false,
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn recent(saved: Option<&Value>) -> Vec<Value> {
    saved
        .and_then(|v| v.get("recent"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let values = choices(v.as_object()?);
            ["width", "height", "resolution", "mode", "depth"].iter().all(|key| values.contains_key(*key)).then_some(Value::Object(values))
        })
        .take(RECENT_LIMIT)
        .collect()
}

/// Shared by the menu, home button and automation dialog opener. No counter is
/// consumed while a dialog is merely opened/cancelled.
pub fn fields(app: &crate::PhotocraftApp) -> Map<String, Value> {
    let mut fields = crate::state::UiState::new_document_fields();
    let saved = app.session.prefs().dialogs.get(REMEMBERED);
    if let Some(last) = saved.and_then(|v| v.get("last")).and_then(Value::as_object) {
        fields.extend(choices(last));
    }
    let name = photocraft_engine::file_cmds::untitled_name("Untitled", |name| {
        app.session.documents().iter().any(|d| d.doc.name == name)
            || app.ui.dialogs.iter().any(|d| d.kind == crate::state::DialogKind::NewDocument && d.fields.get("name").and_then(Value::as_str) == Some(name))
    });
    fields.insert("name".into(), json!(name));
    fields.insert("__generatedName".into(), json!(name));
    fields.insert("__recent".into(), json!(recent(saved)));
    fields
}

pub fn confirm(app: &mut crate::PhotocraftApp, submitted: &Map<String, Value>) -> Result<Value, String> {
    let mut fields = submitted.clone();
    if fields.get("name") == fields.get("__generatedName") {
        let name = fields.get("name").and_then(Value::as_str).unwrap_or("");
        if app.session.documents().iter().any(|d| d.doc.name == name) {
            fields.insert(
                "name".into(),
                json!(photocraft_engine::file_cmds::untitled_name("Untitled", |name| app.session.documents().iter().any(|d| d.doc.name == name))),
            );
        }
    }
    let result = app.run("file.new", command_params(&fields))?;
    // New is synchronous. Persist actual accepted geometry, not invalid input that
    // the command may have clamped, and only after document creation succeeded.
    if let Some(doc) = result.get("document").and_then(Value::as_u64).and_then(|i| app.session.documents().get(i as usize)).map(|s| &s.doc) {
        let mut accepted = choices(&fields);
        accepted.insert("width".into(), json!(doc.size.width));
        accepted.insert("height".into(), json!(doc.size.height));
        accepted.insert("resolution".into(), json!(doc.resolution_dpi));
        accepted.insert("depth".into(), json!(doc.depth.bits()));
        accepted.insert(
            "mode".into(),
            json!(match doc.mode {
                photocraft_color::ColorMode::Grayscale => "gray",
                photocraft_color::ColorMode::Cmyk => "cmyk",
                photocraft_color::ColorMode::Lab => "lab",
                _ => "rgb",
            }),
        );
        accepted.insert("designUnits".into(), json!(doc.design_units));
        let value = Value::Object(accepted);
        let mut entries = recent(app.session.prefs().dialogs.get(REMEMBERED));
        entries.retain(|entry| entry != &value);
        entries.insert(0, value.clone());
        entries.truncate(RECENT_LIMIT);
        app.session.edit_prefs(|prefs| prefs.dialogs.insert(REMEMBERED.into(), json!({"last": value, "recent": entries})));
    }
    if let Some(i) = app.session.active_index() {
        app.ui.views[i].fit_pending = true;
    }
    Ok(result)
}

/// A blank-document preset: (name, width px, height px, ppi).
pub type Preset = (&'static str, u32, u32, f32);

/// Photoshop's New Document categories and their blank-document presets.
pub const CATEGORIES: &[(&str, &[Preset])] = &[
    ("Recent", &[]),
    (
        "Photo",
        &[
            ("Landscape, 6 x 4", 1800, 1200, 300.0),
            ("Landscape, 7 x 5", 2100, 1500, 300.0),
            ("Landscape, 10 x 8", 3000, 2400, 300.0),
            ("Portrait, 4 x 6", 1200, 1800, 300.0),
            ("Portrait, 5 x 7", 1500, 2100, 300.0),
            ("Square, 5 x 5", 1500, 1500, 300.0),
        ],
    ),
    (
        "Print",
        &[
            ("Letter", 2550, 3300, 300.0),
            ("Legal", 2550, 4200, 300.0),
            ("Tabloid", 3300, 5100, 300.0),
            ("A4", 2480, 3508, 300.0),
            ("A3", 3508, 4961, 300.0),
            ("A5", 1748, 2480, 300.0),
        ],
    ),
    (
        "Art & Illustration",
        &[("Poster", 5400, 7200, 300.0), ("Postcard", 1800, 1200, 300.0), ("Comic Book", 1988, 3075, 300.0), ("Square, 12 x 12", 3600, 3600, 300.0)],
    ),
    (
        "Web",
        &[
            ("Web Most Common", 1366, 768, 72.0),
            ("Web Minimum", 1024, 768, 72.0),
            ("Web Large", 1920, 1080, 72.0),
            ("MacBook Pro 16\"", 3456, 2234, 72.0),
            ("iMac 24\"", 4480, 2520, 72.0),
        ],
    ),
    (
        "Mobile",
        &[
            ("iPhone 16", 1179, 2556, 72.0),
            ("iPhone 16 Pro Max", 1320, 2868, 72.0),
            ("iPad Pro 13\"", 2064, 2752, 72.0),
            ("Android 1080p", 1080, 1920, 72.0),
            ("Apple Watch 45mm", 396, 484, 72.0),
        ],
    ),
    (
        "Film & Video",
        &[
            ("HDTV 1080p", 1920, 1080, 72.0),
            ("HDTV 720p", 1280, 720, 72.0),
            ("UHD 4K", 3840, 2160, 72.0),
            ("DCI 4K", 4096, 2160, 72.0),
            ("UHD 8K", 7680, 4320, 72.0),
        ],
    ),
];

const DEPTH_OPTIONS: &[(u64, &str, &str)] = &[(8, "8 bit", "Integer"), (16, "16 bit", "Integer"), (32, "32 bit (float)", "Floating point")];

/// Width/Height units: (key, label, units per inch; 0 = pixels).
pub const UNITS: &[(&str, &str, f32)] =
    &[("px", "Pixels", 0.0), ("in", "Inches", 1.0), ("cm", "Centimeters", 2.54), ("mm", "Millimeters", 25.4), ("pt", "Points", 72.0), ("pica", "Picas", 6.0)];

/// Pixels to the display unit at `ppi`.
pub fn to_unit(px: f32, unit: &str, ppi: f32) -> f32 {
    match UNITS.iter().find(|u| u.0 == unit) {
        Some((_, _, per_in)) if *per_in > 0.0 => px / ppi.max(1.0) * per_in,
        _ => px,
    }
}

/// Display unit back to pixels at `ppi`.
pub fn from_unit(v: f32, unit: &str, ppi: f32) -> f32 {
    match UNITS.iter().find(|u| u.0 == unit) {
        Some((_, _, per_in)) if *per_in > 0.0 => (v / per_in * ppi.max(1.0)).round(),
        _ => v.round(),
    }
}

/// A typed size as the whole pixel count `file.new` takes (#254: a float like `512.0` isn't one, so
/// the command fell back to its 1920 x 1080 default). Clamped to the command's 1–300000 range.
pub fn px_value(px: f32) -> Value {
    json!(if px.is_finite() { px.round().clamp(1.0, 300_000.0) as u32 } else { 1 })
}

/// Apply a preset to the dialog fields.
pub fn apply_preset(f: &mut Map<String, Value>, p: &Preset) {
    f.insert("width".into(), json!(p.1));
    f.insert("height".into(), json!(p.2));
    f.insert("resolution".into(), json!(p.3));
    f.insert("__preset".into(), json!(p.0));
    let category = f.get("__category").and_then(Value::as_str).unwrap_or("Photo");
    let units = photocraft_engine::vector_packages::layout::preset_units(category);
    f.insert("designUnits".into(), json!(units));
    use photocraft_doc::vector_design::structure::DesignUnits;
    f.insert(
        "__unit".into(),
        json!(match units {
            DesignUnits::Millimeters => "mm",
            DesignUnits::Points => "pt",
            _ =>
                if p.3 >= 300. {
                    "in"
                } else {
                    "px"
                },
        }),
    );
}

/// Fields `file.new` takes (drops the dialog's `__` UI keys).
pub fn command_params(f: &Map<String, Value>) -> Value {
    Value::Object(f.iter().filter(|(k, _)| !k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Set the resolution (pixels/inch) the way Photoshop's New Document does (#758): with Width/Height
/// in a physical unit the physical size is kept and the pixel count changes; in pixels the pixels
/// are kept.
pub fn set_resolution(f: &mut Map<String, Value>, new_ppi: f32) {
    let old_ppi = get_f(f, "resolution", 72.0);
    f.insert("resolution".into(), json!(new_ppi));
    if get_s(f, "__unit", "px") == "px" || !(old_ppi > 0.0 && new_ppi > 0.0) || old_ppi == new_ppi {
        return;
    }
    let scale = new_ppi / old_ppi;
    let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
    f.insert("width".into(), px_value(w * scale));
    f.insert("height".into(), px_value(h * scale));
    f.remove("__preset");
}

fn get_f(f: &Map<String, Value>, k: &str, d: f32) -> f32 {
    f.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32)
}

fn get_s(f: &Map<String, Value>, k: &str, d: &str) -> String {
    f.get(k).and_then(Value::as_str).unwrap_or(d).to_string()
}

fn small_label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).size(11.5).color(t.text_dim));
}

/// Paint a page thumbnail with the preset's aspect ratio.
fn page_icon(ui: &egui::Ui, r: Rect, w: u32, h: u32, t: &Tokens) {
    let s = 30.0 / (w.max(h) as f32);
    let page = Rect::from_center_size(r.center(), vec2(w as f32 * s, h as f32 * s));
    ui.painter().rect_stroke(page, 1.0, Stroke::new(1.2, t.text_dim), StrokeKind::Inside);
}

pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let cat = get_s(f, "__category", "Recent");
    // Category tabs.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        for (name, _) in CATEGORIES {
            let on = cat == *name;
            let r = ui.add(egui::Label::new(RichText::new(tl!(name)).size(13.0).color(if on { t.text } else { t.text_dim })).sense(Sense::click()));
            if on {
                ui.painter().line_segment([r.rect.left_bottom() + vec2(0.0, 3.0), r.rect.right_bottom() + vec2(0.0, 3.0)], Stroke::new(2.0, t.text));
            }
            if r.clicked() {
                f.insert("__category".into(), json!(name));
            }
        }
    });
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(8.0);
    let presets = CATEGORIES.iter().find(|c| c.0 == cat).map_or(CATEGORIES[0].1, |c| c.1);
    let chosen = get_s(f, "__preset", "");
    ui.horizontal_top(|ui| {
        // Left: preset grid.
        ui.vertical(|ui| {
            ui.set_width(520.0);
            let count =
                if cat == "Recent" { f.get("__recent").and_then(Value::as_array).map_or(0, |entries| entries.len().min(RECENT_LIMIT)) } else { presets.len() };
            ui.label(RichText::new(li_i18n::fmt(tl!("BLANK DOCUMENT PRESETS ({n})"), &[("n", &count.to_string())])).size(11.0).color(t.text_faint));
            ui.add_space(6.0);
            if cat == "Recent" {
                let entries = f.get("__recent").and_then(Value::as_array).cloned().unwrap_or_default();
                if entries.is_empty() {
                    ui.label(tl!("No recent documents yet."));
                }
                for entry in entries.into_iter().take(RECENT_LIMIT) {
                    let Some(values) = entry.as_object() else { continue };
                    let values = choices(values);
                    if !["width", "height", "resolution", "mode", "depth"].iter().all(|key| values.contains_key(*key)) {
                        continue;
                    }
                    let label = format!(
                        "{} × {} px · {} ppi · {} · {} bit",
                        values["width"],
                        values["height"],
                        widgets::fmt_num(values["resolution"].as_f64().unwrap_or(72.)),
                        values["mode"].as_str().unwrap_or("rgb").to_uppercase(),
                        values["depth"]
                    );
                    if ui.button(label).clicked() {
                        f.extend(choices(&values));
                        f.insert("__unit".into(), json!("px"));
                        f.remove("__preset");
                    }
                }
            }
            let card = vec2(164.0, 112.0);
            for row in presets.chunks(3) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for p in row {
                        let (r, resp) = ui.allocate_exact_size(card, Sense::click());
                        let on = chosen == p.0;
                        ui.painter().rect_filled(
                            r,
                            t.radius,
                            if on {
                                t.row_selected
                            } else if resp.hovered() {
                                t.hover
                            } else {
                                t.field
                            },
                        );
                        if on {
                            ui.painter().rect_stroke(r, t.radius, Stroke::new(1.5, t.accent), StrokeKind::Inside);
                        }
                        page_icon(ui, Rect::from_center_size(pos2(r.center().x, r.top() + 34.0), vec2(40.0, 40.0)), p.1, p.2, &t);
                        ui.painter().text(pos2(r.center().x, r.top() + 72.0), Align2::CENTER_CENTER, tl!(p.0), egui::FontId::proportional(12.0), t.text);
                        let unit = if p.3 >= 300.0 { "in" } else { "px" };
                        let size = if unit == "in" {
                            format!(
                                "{} x {} in @ {} ppi",
                                widgets::fmt_num(to_unit(p.1 as f32, "in", p.3) as f64),
                                widgets::fmt_num(to_unit(p.2 as f32, "in", p.3) as f64),
                                p.3
                            )
                        } else {
                            format!("{} x {} px @ {} ppi", p.1, p.2, p.3)
                        };
                        ui.painter().text(
                            pos2(r.center().x, r.top() + 90.0),
                            Align2::CENTER_CENTER,
                            size,
                            egui::FontId::proportional(10.5),
                            if on || resp.hovered() { t.text_secondary_on_fill } else { t.text_faint },
                        );
                        if resp.clicked() {
                            apply_preset(f, p);
                        }
                    }
                });
                ui.add_space(8.0);
            }
        });
        ui.add_space(10.0);
        // Right: Preset Details.
        ui.vertical(|ui| {
            ui.set_width(260.0);
            ui.label(RichText::new(tl!("PRESET DETAILS")).size(11.0).color(t.text_faint));
            ui.add_space(4.0);
            let mut name = get_s(f, "name", tl!("Untitled-1"));
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(250.0).font(egui::FontId::proportional(15.0))).changed() {
                f.insert("name".into(), json!(name));
            }
            ui.add_space(8.0);
            let ppi = get_f(f, "resolution", 72.0);
            let mut unit = get_s(f, "__unit", "px");
            small_label(ui, tl!("Width"));
            ui.horizontal(|ui| {
                let mut w = to_unit(get_f(f, "width", 1920.0), &unit, ppi);
                if widgets::value_field(ui, &mut w, 0.0..=300_000.0, "", 110.0).changed() {
                    f.insert("width".into(), px_value(from_unit(w, &unit, ppi)));
                    f.remove("__preset");
                }
                let opts: Vec<(String, &str)> = UNITS.iter().map(|u| (u.0.to_string(), u.1)).collect();
                if widgets::dropdown(ui, "nd-unit", &mut unit, &opts, 120.0) {
                    f.insert("__unit".into(), json!(unit));
                }
            });
            small_label(ui, tl!("Height"));
            ui.horizontal(|ui| {
                let mut h = to_unit(get_f(f, "height", 1080.0), &unit, ppi);
                if widgets::value_field(ui, &mut h, 0.0..=300_000.0, "", 110.0).changed() {
                    f.insert("height".into(), px_value(from_unit(h, &unit, ppi)));
                    f.remove("__preset");
                }
                ui.add_space(6.0);
                small_label(ui, tl!("Orientation"));
                let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
                for (icon, portrait) in [("rectangle-vertical", true), ("rectangle-horizontal", false)] {
                    if icons::button(ui, icon, 24.0, (h > w) == portrait, if portrait { "Portrait" } else { "Landscape" }).clicked() && (h > w) != portrait {
                        f.insert("width".into(), px_value(h));
                        f.insert("height".into(), px_value(w));
                    }
                }
            });
            ui.add_space(4.0);
            small_label(ui, tl!("Resolution"));
            ui.horizontal(|ui| {
                let per_cm = get_s(f, "__resUnit", "in") == "cm";
                let mut r = if per_cm { ppi / 2.54 } else { ppi };
                if widgets::value_field(ui, &mut r, 1.0..=30_000.0, "", 110.0).changed() {
                    set_resolution(f, if per_cm { r * 2.54 } else { r });
                }
                let mut ru = get_s(f, "__resUnit", "in");
                if widgets::dropdown(ui, "nd-resunit", &mut ru, &[("in".to_string(), tl!("Pixels/Inch")), ("cm".to_string(), tl!("Pixels/Centimeter"))], 120.0)
                {
                    f.insert("__resUnit".into(), json!(ru));
                }
            });
            ui.add_space(4.0);
            small_label(ui, tl!("Color Mode"));
            ui.horizontal(|ui| {
                let mut mode = get_s(f, "mode", "rgb");
                if widgets::dropdown(
                    ui,
                    "nd-mode",
                    &mut mode,
                    &[
                        ("gray".to_string(), tl!("Grayscale")),
                        ("rgb".to_string(), tl!("RGB Color")),
                        ("cmyk".to_string(), tl!("CMYK Color")),
                        ("lab".to_string(), tl!("Lab Color")),
                    ],
                    110.0,
                ) {
                    f.insert("mode".into(), json!(mode));
                }
                let mut depth = f.get("depth").and_then(Value::as_u64).unwrap_or(8);
                let depth_options: Vec<(u64, &str, &str)> = DEPTH_OPTIONS.iter().map(|(bits, label, tooltip)| (*bits, *label, *tooltip)).collect();
                if widgets::dropdown_with_tooltips(ui, "nd-depth", &mut depth, &depth_options, 120.0) {
                    f.insert("depth".into(), json!(depth));
                }
            });
            ui.add_space(4.0);
            small_label(ui, tl!("Background Contents"));
            let mut bg = get_s(f, "background", "white");
            let opts = [
                ("white".to_string(), tl!("White")),
                ("black".to_string(), tl!("Black")),
                ("backgroundColor".to_string(), tl!("Background Color")),
                ("transparent".to_string(), tl!("Transparent")),
            ];
            if widgets::dropdown(ui, "nd-bg", &mut bg, &opts, 240.0) {
                f.insert("background".into(), json!(bg));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_round_trip_through_pixels() {
        assert_eq!(to_unit(2100.0, "in", 300.0), 7.0);
        assert_eq!(from_unit(7.0, "in", 300.0), 2100.0);
        assert_eq!(from_unit(2.54, "cm", 300.0), 300.0);
        assert_eq!(to_unit(640.0, "px", 72.0), 640.0);
    }

    #[test]
    fn resolution_keeps_physical_size_in_physical_units_and_pixels_in_px() {
        let a4 = CATEGORIES.iter().find(|c| c.0 == "Print").unwrap().1.iter().find(|p| p.0 == "A4").unwrap();
        let mut f = crate::state::UiState::new_document_fields();
        apply_preset(&mut f, a4);
        f.insert("__unit".into(), json!("in"));
        set_resolution(&mut f, 150.0);
        let p = command_params(&f);
        assert_eq!((p["width"].clone(), p["height"].clone(), p["resolution"].clone()), (json!(1240), json!(1754), json!(150.0)));
        assert!(!f.contains_key("__preset"));

        let mut f = crate::state::UiState::new_document_fields();
        apply_preset(&mut f, a4);
        f.insert("__unit".into(), json!("px"));
        set_resolution(&mut f, 150.0);
        let p = command_params(&f);
        assert_eq!((p["width"].clone(), p["height"].clone(), p["resolution"].clone()), (json!(2480), json!(3508), json!(150.0)));
    }

    #[test]
    fn new_document_depth_labels_and_tooltips_are_translated() {
        for lang in li_i18n::Lang::ALL.into_iter().filter(|lang| lang.code() != "en") {
            for (_, label, tooltip) in DEPTH_OPTIONS {
                assert_ne!(li_i18n::tr(lang, label), *label, "{}: {label}", lang.code());
                assert_ne!(li_i18n::tr(lang, tooltip), *tooltip, "{}: {tooltip}", lang.code());
            }
        }
    }

    #[test]
    fn preset_sets_size_resolution_and_create_params_drop_ui_keys() {
        let mut f = crate::state::UiState::new_document_fields();
        let a4 = CATEGORIES.iter().find(|c| c.0 == "Print").unwrap().1.iter().find(|p| p.0 == "A4").unwrap();
        apply_preset(&mut f, a4);
        let p = command_params(&f);
        assert_eq!(p["width"], 2480);
        assert_eq!(p["height"], 3508);
        assert_eq!(p["resolution"], 300.0);
        assert!(p.get("__preset").is_none() && p.get("__unit").is_none());
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", p).unwrap();
        let d = &s.active().unwrap().doc;
        assert_eq!((d.size.width, d.size.height, d.resolution_dpi), (2480, 3508, 300.0));
    }
    /// The real dialog (#254): a typed size must reach `file.new`, however it is confirmed.
    mod dialog {
        use super::super::{CATEGORIES, apply_preset};
        use crate::PhotocraftApp;
        use crate::state::{DialogKind, UiState};
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        fn harness() -> Harness<'static, PhotocraftApp> {
            let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(|ui, app| crate::dialogs::show(app, ui.ctx()), app);
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
            h.state_mut().ui.open_dialog(DialogKind::NewDocument, UiState::new_document_fields());
            h.run_steps(3);
            h
        }

        fn click_at(h: &mut Harness<'static, PhotocraftApp>, at: egui::Pos2) {
            h.hover_at(at);
            h.run_steps(1);
            h.drag_at(at);
            h.run_steps(1);
            h.drop_at(at);
            h.run_steps(2);
        }

        /// The Width (0), Height (1) and Resolution (2) fields.
        fn field(h: &Harness<'static, PhotocraftApp>, i: usize) -> egui::Rect {
            h.query_all_by_role(Role::SpinButton).nth(i).map(|n| n.rect()).expect("a size field")
        }

        /// Click into field `i` (which selects its text) and type `text`, as a user does.
        fn type_into(h: &mut Harness<'static, PhotocraftApp>, i: usize, text: &str) {
            let r = field(h, i);
            click_at(h, r.center());
            for c in text.chars() {
                h.event(egui::Event::Text(c.to_string()));
                h.run_steps(1);
            }
        }

        fn fields(h: &Harness<'static, PhotocraftApp>) -> serde_json::Map<String, serde_json::Value> {
            h.state().ui.dialogs.first().map(|d| d.fields.clone()).expect("the dialog is open")
        }

        fn set_fields(h: &mut Harness<'static, PhotocraftApp>, f: serde_json::Map<String, serde_json::Value>) {
            h.state_mut().ui.dialogs[0].fields = f;
            h.run_steps(2);
        }

        fn enter(h: &mut Harness<'static, PhotocraftApp>) {
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
        }

        fn created(h: &Harness<'static, PhotocraftApp>) -> (u32, u32, f32) {
            assert!(h.state().ui.dialogs.is_empty(), "the dialog closed");
            let d = &h.state().session.active().expect("a new document").doc;
            (d.size.width, d.size.height, d.resolution_dpi)
        }

        #[test]
        fn typed_size_then_enter_creates_that_size() {
            let mut h = harness();
            type_into(&mut h, 0, "512");
            type_into(&mut h, 1, "512");
            let f = fields(&h);
            assert_eq!((f["width"].as_u64(), f["height"].as_u64()), (Some(512), Some(512)), "whole pixels: {f:?}");
            enter(&mut h);
            assert_eq!(created(&h), (512, 512, 72.0));
        }

        #[test]
        fn typed_size_then_create_without_leaving_the_field_creates_that_size() {
            let mut h = harness();
            type_into(&mut h, 0, "512");
            type_into(&mut h, 1, "300");
            // Still editing Height: click Create straight away.
            let create = h.get_by_label("Create").rect();
            click_at(&mut h, create.center());
            assert_eq!(created(&h), (512, 300, 72.0));
        }

        #[test]
        fn typing_over_a_preset_wins() {
            let mut h = harness();
            let mut f = fields(&h);
            let web = CATEGORIES.iter().find(|c| c.0 == "Web").unwrap().1.iter().find(|p| p.0 == "Web Minimum").unwrap();
            apply_preset(&mut f, web);
            set_fields(&mut h, f);
            type_into(&mut h, 0, "512");
            type_into(&mut h, 1, "512");
            assert!(fields(&h).get("__preset").is_none(), "typing deselects the preset");
            enter(&mut h);
            assert_eq!(created(&h), (512, 512, 72.0));
        }

        #[test]
        fn clicking_a_preset_card_after_typing_sets_its_size() {
            let mut h = harness();
            type_into(&mut h, 0, "512");
            h.get_by_label("Photo").click();
            h.run_steps(2);
            // The first card ("Landscape, 6 x 4") sits under the presets heading.
            let heading = h.get_by_label_contains("BLANK DOCUMENT PRESETS").rect();
            click_at(&mut h, heading.left_bottom() + egui::vec2(80.0, 60.0));
            assert_eq!(fields(&h).get("__preset").and_then(|v| v.as_str()), Some("Landscape, 6 x 4"));
            enter(&mut h);
            assert_eq!(created(&h), (1800, 1200, 300.0));
        }

        #[test]
        fn typed_size_in_inches_converts_at_the_resolution() {
            let mut h = harness();
            type_into(&mut h, 2, "300");
            let mut f = fields(&h);
            f.insert("__unit".into(), serde_json::json!("in"));
            set_fields(&mut h, f);
            type_into(&mut h, 0, "2");
            type_into(&mut h, 1, "1.5");
            enter(&mut h);
            assert_eq!(created(&h), (600, 450, 300.0));
        }

        #[test]
        fn changing_units_keeps_the_typed_pixel_size() {
            let mut h = harness();
            type_into(&mut h, 0, "512");
            type_into(&mut h, 1, "512");
            for unit in ["in", "cm", "mm", "pt", "pica", "px"] {
                let mut f = fields(&h);
                f.insert("__unit".into(), serde_json::json!(unit));
                set_fields(&mut h, f);
            }
            enter(&mut h);
            assert_eq!(created(&h), (512, 512, 72.0));
        }

        #[test]
        fn tiny_size_in_inches_is_not_changed_by_showing_it() {
            // 1 px at 300 ppi is 0.0033 in: a field floored at 0.01 in turned it into 3 px unasked.
            let mut h = harness();
            let mut f = fields(&h);
            for (k, v) in [
                ("width", serde_json::json!(1)),
                ("height", serde_json::json!(1)),
                ("resolution", serde_json::json!(300.0)),
                ("__unit", serde_json::json!("in")),
            ] {
                f.insert(k.into(), v);
            }
            set_fields(&mut h, f);
            h.run_steps(2);
            let p = super::super::command_params(&fields(&h));
            assert_eq!((p["width"].as_u64(), p["height"].as_u64()), (Some(1), Some(1)));
        }

        #[test]
        fn orientation_swap_keeps_whole_pixels() {
            let mut h = harness();
            type_into(&mut h, 0, "512");
            type_into(&mut h, 1, "256");
            // The Portrait icon button follows the "Orientation" label (icons have tooltips only).
            let label = h.get_by_label("Orientation").rect();
            let gap = h.ctx.global_style().spacing.item_spacing.x;
            click_at(&mut h, egui::pos2(label.right() + gap + 12.0, label.center().y));
            enter(&mut h);
            assert_eq!(created(&h), (256, 512, 72.0));
        }
    }
}

#[cfg(test)]
mod remembered_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::{PhotocraftApp, Services};
    use egui_kittest::kittest::Queryable;

    fn open(app: &mut PhotocraftApp, ctx: &egui::Context) -> u64 {
        crate::menus::invoke(app, ctx, "file.new", json!({})).unwrap()["dialog"].as_u64().unwrap()
    }

    #[test]
    fn remembered_new_success_restart_cancel_failure_and_unique_names() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
        let ctx = egui::Context::default();
        let first = open(&mut app, &ctx);
        assert_eq!(app.ui.dialog_mut(first).unwrap().fields["name"], "Untitled-1");
        crate::dialogs::cancel(&mut app, first).unwrap();
        assert!(!app.session.prefs().dialogs.contains_key(REMEMBERED));
        let first = open(&mut app, &ctx);
        assert_eq!(app.ui.dialog_mut(first).unwrap().fields["name"], "Untitled-1");
        let f = &mut app.ui.dialog_mut(first).unwrap().fields;
        f.extend(json!({"width":24,"height":18,"resolution":150,"mode":"gray","depth":16,"background":"transparent"}).as_object().unwrap().clone());
        crate::dialogs::confirm(&mut app, first).unwrap();
        assert_eq!(app.session.active().unwrap().doc.name, "Untitled-1");
        let saved = app.session.prefs().dialogs[REMEMBERED].clone();
        let second = open(&mut app, &ctx);
        let f = &mut app.ui.dialog_mut(second).unwrap().fields;
        assert_eq!(f["name"], "Untitled-2");
        for key in ["width", "height", "resolution", "mode", "depth"] {
            assert_eq!(f[key], saved["last"][key]);
        }
        f.insert("width".into(), json!(30));
        crate::dialogs::cancel(&mut app, second).unwrap();
        assert_eq!(app.session.prefs().dialogs[REMEMBERED], saved);
        let failed = open(&mut app, &ctx);
        app.ui.dialog_mut(failed).unwrap().fields.insert("designUnits".into(), json!("invalid"));
        assert!(crate::dialogs::confirm(&mut app, failed).is_err());
        assert_eq!(app.session.documents().len(), 1);
        assert_eq!(app.session.prefs().dialogs[REMEMBERED], saved);
        let second = open(&mut app, &ctx);
        assert_eq!(app.ui.dialog_mut(second).unwrap().fields["name"], "Untitled-2");
        crate::dialogs::confirm(&mut app, second).unwrap();
        assert_eq!(app.session.active().unwrap().doc.name, "Untitled-2");
        assert_eq!(app.session.prefs().dialogs[REMEMBERED]["recent"].as_array().unwrap().len(), 1, "identical successful choices deduplicate");
        let serialized = serde_json::to_string(&app.session.prefs().to_json()).unwrap();
        let mut session = photocraft_engine::Session::new();
        session.load_prefs_json(&serialized).unwrap();
        let restarted = PhotocraftApp::new(session, Services::default());
        let f = fields(&restarted);
        assert_eq!(f["width"], 24);
        assert_eq!(f["height"], 18);
        assert_eq!(f["resolution"], 150.0);
        assert_eq!(f["mode"], "gray");
        assert_eq!(f["depth"], 16);
        assert!(!saved["last"].as_object().unwrap().contains_key("name"));
    }

    #[test]
    fn remembered_new_recent_uses_real_create_close_and_recall_buttons() {
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1100., 760.)).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(photocraft_engine::Session::new(), Services::default())
        });
        let ctx = h.ctx.clone();
        for (width, height) in [(24, 18), (32, 20)] {
            let id = open(h.state_mut(), &ctx);
            h.state_mut().ui.dialog_mut(id).unwrap().fields.extend(
                json!({"width":width,"height":height,"resolution":150,"mode":"gray","depth":16,"background":"transparent"}).as_object().unwrap().clone(),
            );
            h.run_steps(3);
            h.get_by_label("Create").click();
            h.run_steps(3);
            assert_eq!(h.state().session.active().unwrap().doc.size.width, width);
        }
        let id = open(h.state_mut(), &ctx);
        h.run_steps(3);
        let recent = h.get_by_label("24 × 18 px · 150 ppi · GRAY · 16 bit");
        assert!(ctx.content_rect().contains_rect(recent.rect()));
        recent.click();
        h.run_steps(3);
        assert_eq!(h.state_mut().ui.dialog_mut(id).unwrap().fields["width"], 24);
        let saved = h.state().session.prefs().dialogs[REMEMBERED].clone();
        assert_eq!(saved["recent"].as_array().unwrap().len(), 2);
        // The dialog's own Close (beside Create), not the window title bar's Close.
        let create = h.get_by_label("Create").rect();
        h.query_all_by_label("Close")
            .min_by(|a, b| a.rect().center().distance(create.center()).total_cmp(&b.rect().center().distance(create.center())))
            .unwrap()
            .click();
        h.run_steps(3);
        assert_eq!(h.state().session.documents().len(), 2);
        assert_eq!(h.state().session.prefs().dialogs[REMEMBERED], saved);
        let id = open(h.state_mut(), &ctx);
        assert_eq!(h.state_mut().ui.dialog_mut(id).unwrap().fields["name"], "Untitled-3");
        assert_eq!(h.state_mut().ui.dialog_mut(id).unwrap().fields["width"], 32, "cancelled recall did not replace successful defaults");
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().ui.dialogs.is_empty());
        assert_eq!(h.state().session.prefs().dialogs[REMEMBERED], saved);
        let id = open(h.state_mut(), &ctx);
        assert_eq!(h.state_mut().ui.dialog_mut(id).unwrap().fields["name"], "Untitled-3");
    }

    #[test]
    fn remembered_new_rejects_malformed_saved_fields_and_bounds_recent_list() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
        app.session.edit_prefs(|p| {
            p.dialogs.insert(
                REMEMBERED.into(),
                json!({"last":{"width":0,"height":"huge","resolution":-1,"mode":"broken","depth":64,"name":"old path","__preview":true},"recent":[{},false]}),
            )
        });
        let f = fields(&app);
        assert_eq!(f["width"], 1920);
        assert_eq!(f["height"], 1080);
        assert_eq!(f["resolution"], 72);
        assert_eq!(f["mode"], "rgb");
        assert_eq!(f["depth"], 8);
        assert_eq!(f["name"], "Untitled-1");
        assert!(f["__recent"].as_array().unwrap().is_empty());
        assert!(!f.contains_key("__preview"));
        for width in 1..=RECENT_LIMIT + 2 {
            let mut f = fields(&app);
            f.extend(json!({"width":width,"height":1,"background":"transparent"}).as_object().unwrap().clone());
            confirm(&mut app, &f).unwrap();
        }
        let recent = &app.session.prefs().dialogs[REMEMBERED]["recent"];
        assert_eq!(recent.as_array().unwrap().len(), RECENT_LIMIT);
        assert_eq!(recent[0]["width"], RECENT_LIMIT + 2);
    }
}
