//! File ▸ Export…: the Export dialog in Lightroom Classic's layout, the one photographers know.
//! Presets on the left (built-in and user presets; Add, Remove, Update with Current Settings),
//! collapsible sections on the right in Lightroom's order — Export Location, File Naming, File
//! Settings, Image Sizing, Output Sharpening, Metadata, Watermarking, Post-Processing — each
//! showing a one-line summary while collapsed. Option names follow Lightroom (darktable's export
//! module where Lightroom has no equivalent, e.g. bit depth and compression).
//!
//! The choices are `app.export` params ([`export_dialog_params`]), so presets, Export with
//! Previous, the control channel and MCP all share them; the dialog-only ones (Add to This
//! Catalog, Ask what to do, After Export) ride along as extra params.

use egui::{Align, Layout, RichText, Sense, Ui, vec2};
use lightcraft_engine::export::{Anchor, Conflict, ExportFormat as F, ExportOptions, MetadataPolicy as M, OutputSpace, Resize, ResizeMode as R, SharpenAmount, SharpenFor};
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::widgets::register;

/// The right-hand sections: (key, title, open by default).
pub const SECTIONS: &[(&str, &str, bool)] = &[
    ("location", "Export Location", true),
    ("naming", "File Naming", true),
    ("file", "File Settings", true),
    ("sizing", "Image Sizing", true),
    ("sharpening", "Output Sharpening", false),
    ("metadata", "Metadata", false),
    ("watermark", "Watermarking", false),
    ("post", "Post-Processing", false),
];

/// File Naming ▸ Rename To templates (label, template); anything else is "Custom Settings".
pub const NAMING_TEMPLATES: &[(&str, &str)] = &[
    ("Filename", "{name}"),
    ("Filename - Sequence", "{name}-{seq}"),
    ("Date - Filename", "{date}-{name}"),
    ("Custom Name - Sequence", "Photo-{seq}"),
    ("Title - Sequence", "{title}-{seq}"),
    ("Date - Sequence", "{date}-{seq}"),
];

/// Image Sizing ▸ Resize to Fit (Lightroom's choices first).
const RESIZE_MODES: [(R, &str); 8] = [
    (R::WidthHeight, "Width & Height"),
    (R::Dimensions, "Dimensions"),
    (R::LongEdge, "Long Edge"),
    (R::ShortEdge, "Short Edge"),
    (R::Megapixels, "Megapixels"),
    (R::Percent, "Percentage"),
    (R::Width, "Width"),
    (R::Height, "Height"),
];

const FORMATS: [(F, &str); 7] =
    [(F::Jpeg, "JPEG"), (F::Png, "PNG"), (F::Tiff, "TIFF"), (F::Webp, "WebP"), (F::Avif, "AVIF"), (F::Dng, "DNG"), (F::Original, "Original")];

const SPACES: [(OutputSpace, &str); 5] = [
    (OutputSpace::Srgb, "sRGB"),
    (OutputSpace::DisplayP3, "Display P3"),
    (OutputSpace::AdobeRgb, "Adobe RGB (1998)"),
    (OutputSpace::ProPhoto, "ProPhoto RGB"),
    (OutputSpace::Rec2020, "Rec. 2020"),
];

const METADATA: [(M, &str); 4] =
    [(M::All, "All Metadata"), (M::AllExceptCamera, "All Except Camera Info"), (M::Copyright, "Copyright Only"), (M::None, "None")];

const SHARPEN_FOR: [(SharpenFor, &str); 3] = [(SharpenFor::Screen, "Screen"), (SharpenFor::Matte, "Matte Paper"), (SharpenFor::Glossy, "Glossy Paper")];

const AMOUNTS: [(SharpenAmount, &str); 3] = [(SharpenAmount::Low, "Low"), (SharpenAmount::Standard, "Standard"), (SharpenAmount::High, "High")];

/// Existing Files choices; index 0 is "Ask what to do".
const EXISTING: [&str; 4] = ["Ask what to do", "Choose a new name for the exported file", "Overwrite WITHOUT WARNING", "Skip"];

/// Post-Processing ▸ After Export (param value, label).
pub const AFTER_EXPORT: [(&str, &str); 3] =
    [("nothing", "Do nothing"), ("showInFolder", "Show in File Manager"), ("openIn", "Open in Other Application…")];

/// The 3 × 3 watermark positions, row by row.
const ANCHORS: [Anchor; 9] = [
    Anchor::TopLeft,
    Anchor::Top,
    Anchor::TopRight,
    Anchor::Left,
    Anchor::Center,
    Anchor::Right,
    Anchor::BottomLeft,
    Anchor::Bottom,
    Anchor::BottomRight,
];

/// Width of the preset list.
const PRESETS_W: f32 = 210.0;
/// Width of the label column in the sections.
const LABEL_W: f32 = 132.0;

/// A new Export dialog with default choices (see [`apply_export_params`]).
pub fn new_dialog(dir: String) -> Dialog {
    Dialog::Export {
        opts: Default::default(),
        full_size: false,
        resize: Default::default(),
        preset_name: String::new(),
        limit_kb: 0,
        dir,
        preset: String::new(),
        add_to_library: false,
        add_to_stack: false,
        ask_existing: false,
        existing: None,
        after: String::new(),
        after_app: String::new(),
    }
}

/// Load `app.export` params `p` (the last export's, or a preset's) into the dialog. The folder is
/// kept (presets don't carry one). `from_preset`: a preset without a size means full size; for
/// the last export, no size at all (nothing exported yet) means the 2048 px default.
pub fn apply_export_params(dlg: &mut Dialog, p: &Value, from_preset: bool) {
    let Dialog::Export { opts, full_size, resize, limit_kb, add_to_library, add_to_stack, ask_existing, existing, after, after_app, .. } = dlg
    else {
        return;
    };
    let o = ExportOptions::from_json(p);
    *full_size = o.resize.is_none() && (from_preset || ExportOptions::has_size_param(p));
    *resize = o.resize.unwrap_or_default();
    *limit_kb = o.limit_kb.unwrap_or(0);
    *opts = o;
    let b = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    *add_to_library = b("addToLibrary");
    *add_to_stack = b("addToStack");
    *ask_existing = b("askExisting");
    *existing = None;
    *after = p.get("afterExport").and_then(Value::as_str).unwrap_or("nothing").to_string();
    *after_app = p.get("afterExportApp").and_then(Value::as_str).unwrap_or_default().to_string();
}

/// The dialog's choices as `app.export` params (without the folder).
pub fn export_dialog_params(dlg: &Dialog) -> Value {
    let Dialog::Export { opts, full_size, resize, limit_kb, add_to_library, add_to_stack, ask_existing, after, after_app, .. } = dlg else {
        return json!({});
    };
    let o = ExportOptions { resize: (!full_size).then_some(*resize), limit_kb: (*limit_kb > 0).then_some(*limit_kb), ..opts.clone() };
    let mut p = o.to_json();
    p["addToLibrary"] = json!(add_to_library);
    p["addToStack"] = json!(*add_to_library && *add_to_stack);
    p["askExisting"] = json!(ask_existing);
    p["afterExport"] = json!(if after.is_empty() { "nothing" } else { after.as_str() });
    if !after_app.trim().is_empty() {
        p["afterExportApp"] = json!(after_app.trim());
    }
    p
}

/// The options the export will run with (the size and file-size limit applied).
fn effective(dlg: &Dialog) -> Option<ExportOptions> {
    let Dialog::Export { opts, full_size, resize, limit_kb, .. } = dlg else { return None };
    Some(ExportOptions { resize: (!full_size).then_some(*resize), limit_kb: (*limit_kb > 0).then_some(*limit_kb), ..opts.clone() })
}

/// File Naming ▸ Example: the first photo's exported file name.
pub fn example_name(app: &mut LightcraftApp, dlg: &Dialog) -> Option<String> {
    let o = effective(dlg)?;
    let id = *crate::control::export_targets(app).first()?;
    Some(o.file_name_for(app.session.catalog.photo(id)?, 1))
}

/// How many of the export's files already exist (Existing Files ▸ Ask what to do).
pub fn existing_outputs(app: &mut LightcraftApp, dlg: &Dialog) -> usize {
    let (Some(o), Dialog::Export { dir, .. }) = (effective(dlg), dlg) else { return 0 };
    let ids = crate::control::export_targets(app);
    let to = lightcraft_engine::export::Destination { dir: dir.trim().to_string(), exact: None };
    lightcraft_engine::export::planned_paths(&app.session.catalog, &ids, &o, &to).into_iter().flatten().filter(|p| std::path::Path::new(p).exists()).count()
}

/// The section's one-line summary (shown in its header).
pub fn summary(app: &mut LightcraftApp, dlg: &Dialog, key: &str) -> String {
    let Dialog::Export { opts, full_size, resize, dir, limit_kb, add_to_library, after, after_app, .. } = dlg else { return String::new() };
    let rendered = opts.format.is_rendered();
    match key {
        "location" => {
            let base = if opts.same_folder { crate::i18n::tr("Same folder as original photo").to_string() } else { short_path(dir) };
            let mut s = format!("{} {base}", crate::i18n::tr("Export to:"));
            if !opts.subfolder.is_empty() {
                s += &format!(" / {}", opts.subfolder);
            }
            if *add_to_library {
                s += &format!(" · {}", crate::i18n::tr("added to the library"));
            }
            s
        }
        "naming" => example_name(app, dlg).unwrap_or_else(|| opts.naming.clone()),
        "file" => {
            let f = label_of(&FORMATS, opts.format);
            if !rendered {
                return f.to_string();
            }
            let mut parts = vec![label_of(&SPACES, opts.effective_space()).to_string()];
            let depths = ExportOptions::bit_depths(opts.format);
            if depths.len() > 1 {
                let d = opts.bit_depth.filter(|b| depths.iter().any(|x| x.0 == *b)).unwrap_or(depths[0].0);
                parts.push(format!("{d}-bit"));
            }
            if matches!(opts.format, F::Jpeg | F::Avif) {
                parts.push(format!("{} {}", crate::i18n::tr("Quality"), opts.quality));
            }
            if opts.format == F::Jpeg && *limit_kb > 0 {
                parts.push(format!("≤ {limit_kb} KB"));
            }
            format!("{f} ({})", parts.join(", "))
        }
        "sizing" if !rendered => crate::i18n::tr("Original size").to_string(),
        "sizing" => {
            let size = if *full_size {
                crate::i18n::tr("Full size").to_string()
            } else {
                let v = resize.value;
                let what = match resize.mode {
                    R::WidthHeight | R::Dimensions => format!("{} × {} px", v.round(), resize.height),
                    R::Megapixels => format!("{v:.1} MP"),
                    R::Percent => format!("{}%", v.round()),
                    _ => format!("{} px", v.round()),
                };
                format!("{}: {what}", crate::i18n::tr(label_of(&RESIZE_MODES, resize.mode)))
            };
            format!("{size} · {} ppi", opts.ppi)
        }
        "sharpening" if !rendered || opts.sharpen == SharpenFor::None => crate::i18n::tr("Sharpening Off").to_string(),
        "sharpening" => format!(
            "{} {} ({})",
            crate::i18n::tr("Sharpening for"),
            crate::i18n::tr(label_of(&SHARPEN_FOR, opts.sharpen)),
            crate::i18n::tr(label_of(&AMOUNTS, opts.sharpen_amount))
        ),
        "metadata" if !rendered => crate::i18n::tr("Kept in the file").to_string(),
        "metadata" => {
            let mut s = crate::i18n::tr(label_of(&METADATA, opts.metadata)).to_string();
            if opts.remove_location && !matches!(opts.metadata, M::None | M::Copyright) {
                s += &format!(", {}", crate::i18n::tr("Remove Location Info"));
            }
            s
        }
        "watermark" => match &opts.watermark {
            Some(w) if rendered && !w.image.trim().is_empty() => format!("{} {}", crate::i18n::tr("Graphic watermark:"), short_path(&w.image)),
            Some(w) if rendered => format!("{} “{}”", crate::i18n::tr("Watermark:"), w.text.lines().next().unwrap_or_default()),
            _ => crate::i18n::tr("No watermark").to_string(),
        },
        "post" => match after.as_str() {
            "openIn" if !after_app.trim().is_empty() => format!("{} {}", crate::i18n::tr("Open in"), short_path(after_app)),
            a => crate::i18n::tr(AFTER_EXPORT.iter().find(|x| x.0 == a).map_or("Do nothing", |x| x.1)).to_string(),
        },
        _ => String::new(),
    }
}

fn label_of<V: PartialEq + Copy>(options: &[(V, &'static str)], v: V) -> &'static str {
    options.iter().find(|o| o.0 == v).map_or("", |o| o.1)
}

/// The last two components of a path (`…/Pictures/Exports`).
fn short_path(p: &str) -> String {
    let parts: Vec<&str> = p.trim().split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => crate::i18n::tr("(no folder)").to_string(),
        1 | 2 => p.trim().to_string(),
        n => format!("…/{}/{}", parts[n - 2], parts[n - 1]),
    }
}

/// The dialog body. Returns true when the export should start now (an Existing Files choice).
pub fn body(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    let n = crate::control::export_targets(app).len();
    let body_h = (ui.ctx().content_rect().height() * 0.66 - 90.0).clamp(300.0, 620.0);
    // header: where to (Lightroom's "Export To: Hard Drive") and how many
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::tr("Export To:")).color(t.text_label));
        ui.label(RichText::new(crate::i18n::tr("Hard Drive")).font(t.semibold(12.5)).color(t.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{} {n} {}", crate::i18n::tr("Export"), if n == 1 { "File" } else { "Files" })).color(t.text_dim));
        });
    });
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(PRESETS_W, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_width(PRESETS_W);
            ui.set_height(body_h);
            presets(app, ui, dlg, body_h);
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical().id_salt("export-sections").max_height(body_h).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for (key, title, open) in SECTIONS {
                    let sum = summary(app, dlg, key);
                    section(ui, key, title, &sum, *open, |ui| match *key {
                        "location" => location(app, ui, dlg),
                        "naming" => naming(app, ui, dlg),
                        "file" => file_settings(ui, dlg),
                        "sizing" => sizing(ui, dlg),
                        "sharpening" => sharpening(ui, dlg),
                        "metadata" => metadata(ui, dlg),
                        "watermark" => watermark(app, ui, dlg),
                        _ => post(app, ui, dlg),
                    });
                }
            });
        });
    });
    existing_prompt(ui, dlg)
}

/// Existing Files ▸ Ask what to do: files are already there; choose, then the export starts.
fn existing_prompt(ui: &mut Ui, dlg: &mut Dialog) -> bool {
    let Dialog::Export { opts, existing, .. } = dlg else { return false };
    let Some(count) = existing.filter(|c| *c > 0) else { return false };
    let t = Tokens::get(ui.ctx());
    let mut go = false;
    egui::Frame::new().fill(t.inset).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
        let what = if count == 1 { "1 file already exists".to_string() } else { format!("{count} files already exist") };
        ui.label(RichText::new(format!("{what} {}", crate::i18n::tr("in the destination. What should the export do?"))).color(t.caution));
        ui.horizontal(|ui| {
            for (i, (label, c)) in [("Choose New Names", Conflict::Unique), ("Overwrite", Conflict::Overwrite), ("Skip", Conflict::Skip)].into_iter().enumerate() {
                if crate::widgets::text_button(ui, &format!("exportExisting-{i}"), label, false).clicked() {
                    opts.conflict = c;
                    go = true;
                }
            }
        });
    });
    if go {
        // answered: the export runs with this choice (and asks again next time)
        *existing = Some(0);
    }
    go
}

/// A collapsible section: a header row (▸/▾, title, summary) and, when open, its rows.
fn section(ui: &mut Ui, key: &str, title: &str, summary: &str, default_open: bool, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    let id = egui::Id::new(("export-section", key));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, default_open));
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    register(ui.ctx(), format!("exportSection:{key}"), rect);
    let p = ui.painter();
    p.rect_filled(rect, 3.0, if resp.hovered() { t.hover } else { t.button });
    let mid = rect.center().y;
    p.text(egui::pos2(rect.left() + 8.0, mid), egui::Align2::LEFT_CENTER, if open { "▾" } else { "▸" }, t.semibold(12.0), t.text_label);
    let title_g = p.layout_no_wrap(crate::i18n::tr(title).to_string(), t.semibold(12.5), t.text);
    let title_w = title_g.size().x;
    p.galley(egui::pos2(rect.left() + 24.0, mid - title_g.size().y / 2.0), title_g, t.text);
    if !open {
        // the summary, clipped to the room right of the title
        let room = egui::Rect::from_min_max(egui::pos2(rect.left() + 40.0 + title_w, rect.top()), egui::pos2(rect.right() - 8.0, rect.bottom()));
        if room.width() > 20.0 {
            let font = egui::FontId::proportional(11.5);
            let mut job = egui::text::LayoutJob::simple_singleline(summary.to_string(), font, t.text_dim);
            job.wrap = egui::text::TextWrapping::truncate_at_width(room.width());
            let g = ui.painter().layout_job(job);
            ui.painter().with_clip_rect(room).galley(egui::pos2(room.right() - g.size().x, mid - g.size().y / 2.0), g, t.text_dim);
        }
    }
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    if open {
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 6, top: 2, bottom: 6 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui);
        });
    }
}

/// A labelled row: the label right-aligned in a fixed column, like Lightroom's.
fn row<Rv>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> Rv) -> Rv {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(LABEL_W, 22.0), Layout::right_to_left(Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            if !label.is_empty() {
                ui.label(RichText::new(crate::i18n::tr(label)).color(t.text_label));
            }
        });
        add(ui)
    })
    .inner
}

/// A drop-down of `options` (registered as `combo:{id}`); true when the value changed.
fn combo<V: PartialEq + Copy>(ui: &mut Ui, id: &str, width: f32, options: &[(V, &str)], value: &mut V) -> bool {
    let before = *value;
    let cur = options.iter().find(|o| o.0 == *value).map_or("", |o| o.1);
    let c = egui::ComboBox::from_id_salt(id).width(width).selected_text(crate::i18n::tr(cur)).show_ui(ui, |ui| {
        for (v, l) in options {
            ui.selectable_value(value, *v, crate::i18n::tr(l));
        }
    });
    register(ui.ctx(), format!("combo:{id}"), c.response.rect);
    *value != before
}

/// A checkbox registered as `check:{id}`.
fn check(ui: &mut Ui, id: &str, value: &mut bool, label: &str) -> egui::Response {
    let r = ui.checkbox(value, crate::i18n::tr(label));
    register(ui.ctx(), format!("check:{id}"), r.rect);
    r
}

fn dim(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
        ui.add(egui::Label::new(RichText::new(crate::i18n::tr(text)).size(11.5).color(t.text_dim)).wrap());
    });
}

// ------------------------------------------------------------------------------------------ presets

fn presets(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog, body_h: f32) {
    let t = Tokens::get(ui.ctx());
    let all = app.session.all_export_presets();
    let selected = match dlg {
        Dialog::Export { preset, .. } => preset.clone(),
        _ => String::new(),
    };
    let mut chosen = None;
    ui.label(RichText::new(crate::i18n::tr("Preset:")).color(t.text_label));
    egui::Frame::new().fill(t.inset).corner_radius(3.0).inner_margin(4.0).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("export-presets").max_height(body_h - 150.0).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (group, builtin) in [("Local Image Presets", true), ("User Presets", false)] {
                ui.label(RichText::new(crate::i18n::tr(group)).font(t.semibold(11.5)).color(t.text_label));
                let mut any = false;
                for (p, _) in all.iter().filter(|(_, b)| *b == builtin) {
                    any = true;
                    let label = crate::i18n::builtin_label(&p.name, builtin);
                    let r = ui.selectable_label(selected.eq_ignore_ascii_case(&p.name), format!("   {label}"));
                    register(ui.ctx(), format!("exportPreset:{}", p.name), r.rect);
                    if r.clicked() {
                        chosen = Some(p.name.clone());
                    }
                }
                if !any {
                    ui.label(RichText::new(format!("   {}", crate::i18n::tr("None yet"))).size(11.5).color(t.text_dim));
                }
                ui.add_space(4.0);
            }
        });
    });
    if let Some(name) = chosen
        && let Ok(params) = app.session.export_params(&json!({"preset": name}))
    {
        apply_export_params(dlg, &params, true);
        if let Dialog::Export { preset, .. } = dlg {
            *preset = name;
        }
    }
    let Dialog::Export { preset, preset_name, .. } = dlg else { return };
    let user = all.iter().any(|(p, b)| !b && p.name.eq_ignore_ascii_case(preset));
    // Add (the current settings under a new name), Remove, Update with Current Settings
    let r = ui.add(egui::TextEdit::singleline(preset_name).hint_text(crate::i18n::tr("New preset name")).desired_width(f32::INFINITY));
    register(ui.ctx(), "field:exportPresetName", r.rect);
    let mut action = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if crate::widgets::text_button(ui, "exportPresetAdd", "Add", false).clicked() {
            action = Some("add");
        }
        let r = ui.add_enabled(user, egui::Button::new(crate::i18n::tr("Remove")));
        register(ui.ctx(), "button:exportPresetRemove", r.rect);
        if r.clicked() {
            action = Some("remove");
        }
    });
    let r = ui.add_enabled(user, egui::Button::new(crate::i18n::tr("Update with Current Settings")));
    register(ui.ctx(), "button:exportPresetUpdate", r.rect);
    if r.clicked() {
        action = Some("update");
    }
    let params = export_dialog_params(dlg);
    let Dialog::Export { preset, preset_name, .. } = dlg else { return };
    let r = match action {
        Some("add") if preset_name.trim().is_empty() => Err("Type a name for the new preset first".to_string()),
        Some("add") => app.run("export.savePreset", json!({"name": preset_name.trim(), "params": params})).map(|_| {
            *preset = preset_name.trim().to_string();
            preset_name.clear();
            format!("{} “{}”", crate::i18n::tr("Saved export preset"), preset)
        }),
        Some("update") => app
            .run("export.savePreset", json!({"name": preset.clone(), "params": params}))
            .map(|_| format!("{} “{}”", crate::i18n::tr("Updated export preset"), preset)),
        Some(_) => app.run("export.deletePreset", json!({"name": preset.clone()})).map(|_| {
            let msg = format!("{} “{}”", crate::i18n::tr("Removed export preset"), preset);
            preset.clear();
            msg
        }),
        None => return,
    };
    match r {
        Ok(msg) | Err(msg) => app.toast(ui.ctx(), msg),
    }
}

// ------------------------------------------------------------------------------------------ sections

fn location(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, dir, add_to_library, add_to_stack, ask_existing, .. } = dlg else { return };
    row(ui, "Export To:", |ui| {
        combo(ui, "exportTo", 240.0, &[(false, "Specific folder"), (true, "Same folder as original photo")], &mut opts.same_folder);
    });
    if !opts.same_folder {
        row(ui, "Folder:", |ui| {
            let h = ui.spacing().interact_size.y.max(22.0);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), h), Layout::right_to_left(Align::Center), |ui| {
                if app.services.pick_folder.is_some()
                    && crate::widgets::text_button(ui, "exportChooseFolder", "Choose…", false).clicked()
                    && let Some(pick) = app.services.pick_folder.as_mut()
                    && let Some(d) = pick()
                {
                    *dir = d;
                }
                let r = ui.add(egui::TextEdit::singleline(dir).desired_width(ui.available_width()));
                register(ui.ctx(), "field:exportFolder", r.rect);
            });
        });
    }
    // Put in Subfolder: the name typed is kept while the box is off
    let sub_id = egui::Id::new("export-subfolder-name");
    let mut sub_on = !opts.subfolder.is_empty() || ui.data(|d| d.get_temp::<bool>(sub_id.with("on"))).unwrap_or(false);
    row(ui, "", |ui| {
        let before = sub_on;
        check(ui, "exportSubfolder", &mut sub_on, "Put in Subfolder:");
        if sub_on != before {
            ui.data_mut(|d| d.insert_temp(sub_id.with("on"), sub_on));
            if sub_on {
                opts.subfolder = ui.data(|d| d.get_temp::<String>(sub_id)).unwrap_or_default();
            } else {
                ui.data_mut(|d| d.insert_temp(sub_id, opts.subfolder.clone()));
                opts.subfolder.clear();
            }
        }
        ui.add_enabled_ui(sub_on, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut opts.subfolder).hint_text(crate::i18n::tr("Subfolder name")).desired_width(ui.available_width()));
            register(ui.ctx(), "field:exportSubfolder", r.rect);
        });
    });
    row(ui, "", |ui| {
        check(ui, "exportAddToLibrary", add_to_library, "Add to This Catalog");
        ui.add_enabled_ui(*add_to_library, |ui| {
            check(ui, "exportAddToStack", add_to_stack, "Add to Stack");
        });
    });
    // Existing Files: Ask / Choose a new name / Overwrite WITHOUT WARNING / Skip
    let mut choice = match (*ask_existing, opts.conflict) {
        (true, _) => 0usize,
        (false, Conflict::Unique) => 1,
        (false, Conflict::Overwrite) => 2,
        (false, Conflict::Skip) => 3,
    };
    row(ui, "Existing Files:", |ui| {
        let options: Vec<(usize, &str)> = EXISTING.iter().copied().enumerate().collect();
        if combo(ui, "exportExisting", 300.0, &options, &mut choice) {
            *ask_existing = choice == 0;
            opts.conflict = match choice {
                2 => Conflict::Overwrite,
                3 => Conflict::Skip,
                _ => Conflict::Unique,
            };
        }
    });
    if choice == 2 {
        dim(ui, "Files of the same name are replaced (a photo's original never is).");
    }
}

fn naming(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog) {
    let example = example_name(app, dlg);
    let Dialog::Export { opts, .. } = dlg else { return };
    let mut tpl = NAMING_TEMPLATES.iter().position(|(_, x)| *x == opts.naming).unwrap_or(usize::MAX);
    row(ui, "Rename To:", |ui| {
        let mut options: Vec<(usize, &str)> = NAMING_TEMPLATES.iter().map(|(l, _)| *l).enumerate().collect();
        options.push((usize::MAX, "Custom Settings"));
        if combo(ui, "exportNamingTemplate", 240.0, &options, &mut tpl)
            && let Some((_, x)) = NAMING_TEMPLATES.get(tpl)
        {
            opts.naming = x.to_string();
        }
    });
    let naming_id = egui::Id::new("export-naming");
    let tags_open = row(ui, "Template:", |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let w = (ui.available_width() - 50.0).max(80.0);
        let r = ui.add(egui::TextEdit::singleline(&mut opts.naming).id(naming_id).hint_text("{name}-{seq}  ·  {date}  ·  {title}").desired_width(w));
        register(ui.ctx(), "field:exportNaming", r.rect);
        crate::import::tag_toggle(ui, "exportNaming")
    });
    if tags_open {
        crate::import::tag_help(ui, "exportNaming", &mut opts.naming, naming_id);
    }
    crate::import::unknown_tags_warning(ui, &opts.naming);
    if opts.naming.contains("{seq") {
        row(ui, "Start Number:", |ui| {
            let r = ui.add(egui::DragValue::new(&mut opts.start_number).range(1..=999_999));
            register(ui.ctx(), "field:exportStartNumber", r.rect);
        });
    }
    let t = Tokens::get(ui.ctx());
    row(ui, "Example:", |ui| {
        let r = ui.label(RichText::new(example.unwrap_or_default()).color(t.text));
        register(ui.ctx(), "label:exportExample", r.rect);
    });
}

fn file_settings(ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, limit_kb, .. } = dlg else { return };
    row(ui, "Image Format:", |ui| {
        if combo(ui, "exportFormat", 180.0, &FORMATS, &mut opts.format) {
            // each format starts at its own default depth (TIFF 16-bit, others 8-bit)
            opts.bit_depth = None;
        }
    });
    match opts.format {
        F::Dng => dim(ui, "Raw photos as DNG, with the edits embedded. Size, color and output options don't apply."),
        F::Original => dim(ui, "The original files, unchanged, each with an XMP sidecar holding its edits."),
        _ => {}
    }
    if matches!(opts.format, F::Jpeg | F::Avif) {
        row(ui, "Quality:", |ui| {
            let r = ui.add(egui::Slider::new(&mut opts.quality, 1..=100));
            register(ui.ctx(), "field:exportQuality", r.rect);
        });
    }
    if opts.format == F::Jpeg {
        row(ui, "", |ui| {
            let mut on = *limit_kb > 0;
            if check(ui, "exportLimitSize", &mut on, "Limit File Size To:").changed() {
                *limit_kb = if on { 500 } else { 0 };
            }
            ui.add_enabled_ui(on, |ui| {
                let r = ui.add(egui::DragValue::new(limit_kb).range(1..=100_000).suffix(" K"));
                register(ui.ctx(), "field:exportLimitKb", r.rect);
            });
        });
    }
    if opts.format.is_rendered() {
        if opts.format == F::Avif {
            row(ui, "Color Space:", |ui| ui.label(crate::i18n::tr("sRGB (AVIF)")));
        } else {
            row(ui, "Color Space:", |ui| {
                combo(ui, "exportColorSpace", 180.0, &SPACES, &mut opts.color_space);
            });
        }
        let depths = ExportOptions::bit_depths(opts.format);
        if depths.len() > 1 {
            let mut bd = opts.bit_depth.filter(|b| depths.iter().any(|d| d.0 == *b)).unwrap_or(depths[0].0);
            row(ui, "Bit Depth:", |ui| {
                combo(ui, "exportBitDepth", 180.0, depths, &mut bd);
            });
            opts.bit_depth = Some(bd);
        }
    }
    if opts.format == F::Tiff {
        use lightcraft_engine::export::TiffCompression as Z;
        row(ui, "Compression:", |ui| {
            combo(ui, "exportTiffCompression", 180.0, &[(Z::None, "None"), (Z::Lzw, "LZW"), (Z::Deflate, "ZIP")], &mut opts.tiff_compression);
        });
    }
    if opts.format == F::Dng {
        use lightcraft_engine::export::DngCompression as Z;
        row(ui, "Compression:", |ui| {
            combo(ui, "exportDngCompression", 180.0, &[(Z::Lossless, "Lossless"), (Z::Deflate, "ZIP"), (Z::Uncompressed, "None")], &mut opts.dng_compression);
        });
    }
}

fn sizing(ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, full_size, resize, .. } = dlg else { return };
    if !opts.format.is_rendered() {
        dim(ui, "Not available: the file is exported at its original size.");
        return;
    }
    let mut fit = !*full_size;
    row(ui, "", |ui| {
        if check(ui, "exportResize", &mut fit, "Resize to Fit:").changed() {
            *full_size = !fit;
        }
        ui.add_enabled_ui(fit, |ui| {
            let before = resize.mode;
            if combo(ui, "exportResizeMode", 160.0, &RESIZE_MODES, &mut resize.mode) {
                // a sensible value for the new unit
                resize.value = match resize.mode {
                    R::Megapixels => 12.0,
                    R::Percent => 50.0,
                    _ if matches!(before, R::Megapixels | R::Percent) => 2048.0,
                    _ => resize.value,
                };
                if matches!(resize.mode, R::Dimensions | R::WidthHeight) && resize.height == 0 {
                    resize.height = resize.value as u32;
                }
            }
            check(ui, "exportDontEnlarge", &mut resize.dont_enlarge, "Don't Enlarge");
        });
    });
    if fit {
        size_values(ui, resize);
    }
    row(ui, "Resolution:", |ui| {
        let r = ui.add(egui::DragValue::new(&mut opts.ppi).range(1..=1200));
        register(ui.ctx(), "field:exportPpi", r.rect);
        ui.label(crate::i18n::tr("pixels per inch"));
    });
}

/// The value field(s) of the resize mode.
fn size_values(ui: &mut Ui, r: &mut Resize) {
    let px = |ui: &mut Ui, id: &str, v: &mut f32| {
        let mut x = v.round() as u32;
        let resp = ui.add(egui::DragValue::new(&mut x).range(16..=65_535).speed(4.0).suffix(" px"));
        register(ui.ctx(), format!("field:{id}"), resp.rect);
        if resp.changed() {
            *v = x as f32;
        }
    };
    match r.mode {
        R::WidthHeight | R::Dimensions => {
            row(ui, "", |ui| {
                ui.label(crate::i18n::tr("W:"));
                px(ui, "exportSizeW", &mut r.value);
                ui.label(crate::i18n::tr("H:"));
                let mut h = r.height as f32;
                px(ui, "exportSizeH", &mut h);
                r.height = h as u32;
            });
            if r.mode == R::Dimensions {
                dim(ui, "Fits either way round: the long edge within the larger number.");
            }
        }
        R::Megapixels => {
            row(ui, "", |ui| {
                let resp = ui.add(egui::DragValue::new(&mut r.value).range(0.1..=500.0).speed(0.1).fixed_decimals(1).suffix(" MP"));
                register(ui.ctx(), "field:exportSizeMp", resp.rect);
            });
        }
        R::Percent => {
            row(ui, "", |ui| {
                let resp = ui.add(egui::DragValue::new(&mut r.value).range(1.0..=400.0).speed(1.0).fixed_decimals(0).suffix(" %"));
                register(ui.ctx(), "field:exportSizePercent", resp.rect);
            });
        }
        _ => {
            row(ui, "", |ui| px(ui, "exportSizePx", &mut r.value));
        }
    }
}

fn sharpening(ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, .. } = dlg else { return };
    if !opts.format.is_rendered() {
        dim(ui, "Not available for this format.");
        return;
    }
    let mut on = opts.sharpen != SharpenFor::None;
    row(ui, "", |ui| {
        if check(ui, "exportSharpen", &mut on, "Sharpen For:").changed() {
            opts.sharpen = if on { SharpenFor::Screen } else { SharpenFor::None };
        }
        ui.add_enabled_ui(on, |ui| {
            let mut target = if on { opts.sharpen } else { SharpenFor::Screen };
            if combo(ui, "exportSharpenFor", 150.0, &SHARPEN_FOR, &mut target) {
                opts.sharpen = target;
            }
            ui.label(crate::i18n::tr("Amount:"));
            combo(ui, "exportSharpenAmount", 110.0, &AMOUNTS, &mut opts.sharpen_amount);
        });
    });
}

fn metadata(ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, .. } = dlg else { return };
    if !opts.format.is_rendered() {
        dim(ui, "The file's own metadata is kept; the edits travel as XMP.");
        return;
    }
    row(ui, "Include:", |ui| {
        combo(ui, "exportMetadata", 240.0, &METADATA, &mut opts.metadata);
    });
    if !matches!(opts.metadata, M::None | M::Copyright) {
        row(ui, "", |ui| {
            check(ui, "exportRemoveLocation", &mut opts.remove_location, "Remove Location Info");
        });
    }
}

fn watermark(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { opts, .. } = dlg else { return };
    if !opts.format.is_rendered() {
        dim(ui, "Not available for this format.");
        return;
    }
    let mut on = opts.watermark.is_some();
    row(ui, "", |ui| {
        if check(ui, "exportWatermark", &mut on, "Watermark").changed() {
            opts.watermark = on.then(|| lightcraft_engine::export::Watermark { text: "© ".into(), ..Default::default() });
        }
    });
    let Some(wm) = &mut opts.watermark else { return };
    // text, or a graphic (a logo with transparency)
    let style_id = egui::Id::new("wm-graphic");
    let mut graphic = !wm.image.is_empty() || ui.data(|d| d.get_temp::<bool>(style_id)).unwrap_or(false);
    row(ui, "Style:", |ui| {
        let before = graphic;
        combo(ui, "exportWmStyle", 140.0, &[(false, "Text"), (true, "Graphic")], &mut graphic);
        if graphic != before {
            ui.data_mut(|d| d.insert_temp(style_id, graphic));
            if !graphic {
                wm.image.clear();
            }
        }
    });
    if graphic {
        row(ui, "Graphic:", |ui| {
            let h = ui.spacing().interact_size.y.max(22.0);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), h), Layout::right_to_left(Align::Center), |ui| {
                if app.services.pick_files.is_some()
                    && crate::widgets::text_button(ui, "exportWmChoose", "Choose…", false).clicked()
                    && let Some(f) = app.services.pick_files.as_mut().and_then(|pick| pick().into_iter().next())
                {
                    wm.image = f;
                }
                ui.add(egui::TextEdit::singleline(&mut wm.image).hint_text("logo.png").desired_width(ui.available_width()));
            });
        });
        row(ui, "Width:", |ui| {
            let mut w = wm.image_width * 100.0;
            if ui.add(egui::DragValue::new(&mut w).range(2.0..=100.0).speed(0.5).fixed_decimals(0).suffix(" % of photo")).changed() {
                wm.image_width = w / 100.0;
            }
        });
    } else {
        row(ui, "Text:", |ui| {
            let r = ui.add(egui::TextEdit::multiline(&mut wm.text).desired_rows(2).hint_text("© Your Name").desired_width(f32::INFINITY));
            register(ui.ctx(), "field:exportWmText", r.rect);
        });
        row(ui, "Size:", |ui| {
            let mut size = wm.size * 100.0;
            if ui.add(egui::DragValue::new(&mut size).range(1.0..=15.0).speed(0.1).fixed_decimals(1).suffix(" % of short edge")).changed() {
                wm.size = size / 100.0;
            }
        });
        row(ui, "Color:", |ui| {
            ui.color_edit_button_srgb(&mut wm.color);
            ui.checkbox(&mut wm.shadow, crate::i18n::tr("Shadow"));
            ui.checkbox(&mut wm.vertical, app.ui.language.tr("Vertical text"));
        });
    }
    row(ui, "Opacity:", |ui| {
        let mut op = wm.opacity * 100.0;
        if ui.add(egui::DragValue::new(&mut op).range(5.0..=100.0).speed(0.5).fixed_decimals(0).suffix(" %")).changed() {
            wm.opacity = op / 100.0;
        }
    });
    row(ui, "Inset:", |ui| {
        let mut inset = wm.inset * 100.0;
        if ui.add(egui::DragValue::new(&mut inset).range(0.0..=40.0).speed(0.1).fixed_decimals(1).suffix(" % of short edge")).changed() {
            wm.inset = inset / 100.0;
        }
    });
    // Anchor: the nine positions as a 3 × 3 grid
    row(ui, "Anchor:", |ui| {
        egui::Grid::new("export-wm-anchor").spacing([2.0, 2.0]).show(ui, |ui| {
            for (i, a) in ANCHORS.iter().enumerate() {
                let on = wm.anchor == *a;
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
                register(ui.ctx(), format!("button:exportWmAnchor-{i}"), r);
                let t = Tokens::get(ui.ctx());
                ui.painter().rect_filled(r, 2.0, if on { t.accent } else if resp.hovered() { t.hover } else { t.button });
                ui.painter().circle_filled(r.center(), 2.5, if on { t.text } else { t.text_dim });
                if resp.clicked() {
                    wm.anchor = *a;
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
    });
}

fn post(app: &mut LightcraftApp, ui: &mut Ui, dlg: &mut Dialog) {
    let Dialog::Export { after, after_app, .. } = dlg else { return };
    if after.is_empty() {
        *after = "nothing".into();
    }
    let mut choice = AFTER_EXPORT.iter().position(|a| a.0 == after.as_str()).unwrap_or(0);
    row(ui, "After Export:", |ui| {
        let options: Vec<(usize, &str)> = AFTER_EXPORT.iter().map(|a| a.1).enumerate().collect();
        if combo(ui, "exportAfter", 260.0, &options, &mut choice) {
            *after = AFTER_EXPORT[choice].0.to_string();
        }
    });
    match after.as_str() {
        "openIn" => {
            row(ui, "Application:", |ui| {
                let r = ui.add(egui::TextEdit::singleline(after_app).hint_text(crate::i18n::tr("System default")).desired_width(f32::INFINITY));
                register(ui.ctx(), "field:exportAfterApp", r.rect);
            });
            if app.services.open_with.is_none() {
                dim(ui, "Not available here.");
            }
        }
        "showInFolder" if app.services.reveal.is_none() => dim(ui, "Not available here."),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_round_trip_through_the_dialog() {
        let mut d = new_dialog("/out".into());
        let p = json!({"format": "tiff", "bitDepth": 8, "sameFolder": true, "subfolder": "Edited", "addToLibrary": true, "addToStack": true,
            "askExisting": true, "afterExport": "openIn", "afterExportApp": "gimp", "longEdge": 1500, "naming": "{name}-{seq}"});
        apply_export_params(&mut d, &p, true);
        let back = export_dialog_params(&d);
        for k in ["format", "bitDepth", "sameFolder", "subfolder", "addToLibrary", "addToStack", "askExisting", "afterExport", "afterExportApp", "naming"] {
            assert_eq!(back[k], p[k], "{k}: {back}");
        }
        assert_eq!(ExportOptions::from_json(&back).resize.map(|r| r.value), Some(1500.0));
        // Add to Stack needs Add to This Catalog
        if let Dialog::Export { add_to_library, .. } = &mut d {
            *add_to_library = false;
        }
        assert_eq!(export_dialog_params(&d)["addToStack"], false);
        // an old saved export (no dialog-only params) still loads
        apply_export_params(&mut d, &json!({"format": "jpeg", "quality": 80}), false);
        let Dialog::Export { opts, full_size, after, .. } = &d else { unreachable!() };
        assert_eq!((opts.quality, opts.same_folder, *full_size, after.as_str()), (80, false, false, "nothing"));
    }

    #[test]
    fn short_paths_keep_the_last_two_folders() {
        assert_eq!(short_path("/home/me/Pictures/Exports"), "…/Pictures/Exports");
        assert_eq!(short_path("/out"), "/out");
    }
}
