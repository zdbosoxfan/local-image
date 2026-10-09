//! Modal dialogs (new album, rename, create preset, choose settings to copy, export, save over
//! original, about, shortcuts). The Export dialog's body is [`crate::panels::export_dialog`].

use lightcraft_develop::SettingsGroup;
use serde_json::json;

use crate::LightcraftApp;
use crate::state::Dialog;
use crate::theme::Tokens;

/// Rows the Rename dialog previews.
const RENAME_PREVIEW_ROWS: usize = 6;

/// The Rename dialog's preview: its first rows (`None` while being planned) and how many files
/// are renamed. Which names are taken is checked on disk, which can block on a slow drive, so the
/// rows are planned on a worker thread, and only when the template, start number, photos or
/// catalog change — never once per frame.
pub(crate) fn rename_preview(
    app: &mut LightcraftApp,
    ctx: &egui::Context,
    template: &str,
    start: usize,
) -> (Option<Vec<lightcraft_engine::rename::RenamePlan>>, usize) {
    use std::hash::{Hash, Hasher};
    type Rows = std::sync::Arc<std::sync::Mutex<Option<Vec<lightcraft_engine::rename::RenamePlan>>>>;
    let ids = app.session.targets(&json!({}));
    let photos = lightcraft_engine::rename::rename_photos(&app.session.catalog, &ids);
    let total = photos.len();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (template, start, app.session.catalog.revision, &ids).hash(&mut h);
    let key = h.finish();
    let id = egui::Id::new("rename-preview-rows");
    let read = |rows: &Rows| rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    if let Some((k, rows)) = ctx.data(|d| d.get_temp::<(u64, Rows)>(id))
        && k == key
    {
        return (read(&rows), total);
    }
    let rows: Rows = Default::default();
    let first: Vec<_> = photos.into_iter().take(RENAME_PREVIEW_ROWS).collect();
    let (out, template, repaint) = (rows.clone(), template.to_string(), ctx.clone());
    let exists = app.session.media.availability.probe();
    let work = move || {
        let plans = lightcraft_engine::rename::plan_rename_photos(&first, &template, start, &|f| exists(f));
        *out.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(plans);
        repaint.request_repaint();
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(e) = std::thread::Builder::new().name("lc-rename-preview".into()).spawn(work) {
        log::warn!("rename preview: {e}");
    }
    #[cfg(target_arch = "wasm32")]
    work();
    ctx.data_mut(|d| d.insert_temp(id, (key, rows.clone())));
    (read(&rows), total)
}

/// About dialog tabs: (widget id suffix, label). The credits come from `crate::credits`.
pub const ABOUT_TABS: &[(&str, &str)] = &[("about", "About"), ("contributors", "Contributors"), ("models", "Models")];

/// Help ▸ What's New (docs/whats-new.md).
pub const WHATS_NEW: &str = include_str!("../../../../docs/whats-new.md");

pub fn show(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(mut dlg) = app.ui.dialog.clone() else { return };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    // The backdrop is an area below the dialog window (a bare `Middle` layer painter would be
    // painted after every area — i.e. over the dialog too).
    egui::Area::new(egui::Id::new("dialog-dim")).order(egui::Order::Middle).fixed_pos(screen.min).interactable(false).show(ctx, |ui| {
        ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
    });
    let mut close = false;
    let mut confirm = false;
    let mut save_beside = false;
    let title: String = match &dlg {
        Dialog::TextPrompt { title, .. } => title.as_str(),
        Dialog::NewAlbum { folder: true, .. } => "Create Folder",
        Dialog::NewAlbum { .. } => "Create Album",
        Dialog::RenameAlbum { .. } => "Rename Album",
        Dialog::Rename { .. } => "Rename Photos",
        Dialog::Import { .. } => "Import Photos",
        Dialog::LabelNames { .. } => "Edit Color Label Names",
        Dialog::CaptureTime { .. } => "Edit Capture Time",
        Dialog::RenameKeyword { .. } => "Rename Keyword",
        Dialog::MergeKeywords { .. } => "Merge Keywords",
        Dialog::NewSmartAlbum { .. } => "Create Smart Album",
        Dialog::AllMetadata { .. } => "All Metadata",
        Dialog::SystemInfo { .. } => "System Info",
        Dialog::WhatsNew => "What's New",
        Dialog::Cull { .. } => "Assisted Culling",
        Dialog::SmartRules { id: None, .. } => "New Smart Album",
        Dialog::SmartRules { .. } => "Edit Smart Album",
        Dialog::AutoStack { .. } => "Auto-Stack by Capture Time",
        Dialog::CreatePreset { .. } => "Create Preset",
        Dialog::CopySettings { .. } => "Choose Edit Settings to Copy",
        Dialog::PasteSettings { .. } => "Paste Selected Settings",
        Dialog::Export { .. } => "Export",
        Dialog::SaveOverOriginal { plan, .. } if plan["mode"] == "beside" => "Save JPEG Beside Original",
        Dialog::SaveOverOriginal { .. } => "Save Over Original",
        Dialog::Merge { opts } => opts.title(),
        Dialog::Settings { .. } => "Settings",
        Dialog::ConfirmDelete { .. } => "Delete Photos",
        Dialog::SamModel { .. } => "Download the SAM 3 Model?",
        Dialog::About => "About Local Image",
        Dialog::Shortcuts => "Keyboard Shortcuts",
    }
    .to_string();
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::symmetric(16, 12));
    let shown = egui::Window::new(crate::i18n::tr(&title)).id(egui::Id::new("lightcraft-dialog"))
        .collapsible(false)
        // the Export dialog is wide (presets beside the sections) and can be resized
        .resizable(matches!(dlg, Dialog::Export { .. }))
        .frame(frame)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(match dlg {
            Dialog::Export { .. } => 900.0,
            Dialog::SaveOverOriginal { .. } => 480.0,
            Dialog::Import { .. } => 760.0,
            Dialog::SmartRules { .. } => 680.0,
            Dialog::AllMetadata { .. } => 620.0,
            _ => 380.0,
        })
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            match &mut dlg {
                Dialog::AutoStack { gap } => {
                    ui.label(egui::RichText::new(crate::i18n::tr("Stack photos taken within this time of each other:")).color(t.text_label));
                    ui.add(
                        egui::Slider::new(gap, 0.0..=86400.0)
                            .logarithmic(true)
                            .smallest_positive(1.0)
                            .custom_formatter(|v, _| crate::panels::dialogs::fmt_gap(v))
                            .custom_parser(|s| s.trim().trim_end_matches('s').parse().ok()),
                    );
                    let preview = app.session.execute("stack.auto", &json!({"gap": *gap, "preview": true})).unwrap_or_default();
                    let scope = if app.session.selection.ids.len() > 1 { "the selected photos" } else { "the photos in view" };
                    ui.label(
                        egui::RichText::new(format!("Creates {} stacks from {} of {scope}", preview["stacks"], preview["photos"])).color(t.text_dim),
                    );
                }
                Dialog::Cull { reject_below, pick_best } => {
                    let n = app.session.selection.ids.len();
                    let scope = if n > 1 { crate::i18n::tr_format!("the {n} selected photos", n = n) } else { crate::i18n::tr_format!("the {} photos in view", app.session.visible_cloned().len()) };
                    ui.label(egui::RichText::new(format!("Scores {scope} for focus and exposure and finds similar shots taken within seconds of each other (bursts).")).color(t.text_label));
                    ui.add_space(6.0);
                    let r = ui.add(egui::Slider::new(reject_below, 0.0..=80.0).text("Reject below focus").step_by(1.0));
                    crate::widgets::register(ui.ctx(), "field:cullReject", r.rect);
                    ui.label(egui::RichText::new(if *reject_below > 0.0 { "Blurry photos below the score are flagged as rejects." } else { "0: nothing is rejected." }).color(t.text_dim));
                    let r = ui.checkbox(pick_best, crate::i18n::tr("Pick the sharpest photo of each burst"));
                    crate::widgets::register(ui.ctx(), "check:cullPick", r.rect);
                    ui.label(egui::RichText::new(crate::i18n::tr("Scores stay on the photos: filter or make smart albums with Focus and Best of Similar Shots.")).color(t.text_dim));
                }
                Dialog::WhatsNew => {
                    egui::ScrollArea::vertical().max_height(460.0).auto_shrink([false, true]).show(ui, |ui| {
                        for line in WHATS_NEW.lines() {
                            let l = line.trim_end();
                            if let Some(h) = l.strip_prefix("### ") {
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new(h).font(t.semibold(12.5)).color(t.text));
                            } else if let Some(h) = l.strip_prefix("## ") {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new(h).font(t.semibold(14.0)).color(t.text));
                            } else if l.starts_with("# ") || l.is_empty() {
                            } else if let Some(b) = l.strip_prefix("- ") {
                                ui.label(egui::RichText::new(format!("•  {}", b.replace('`', ""))).color(t.text_label));
                            } else {
                                ui.label(egui::RichText::new(l.trim().replace('`', "")).color(t.text_label));
                            }
                        }
                    });
                }
                Dialog::SystemInfo { rows } => {
                    egui::Grid::new("sysinfo").num_columns(2).spacing([16.0, 4.0]).striped(true).show(ui, |ui| {
                        for (k, v) in rows.iter() {
                            ui.label(egui::RichText::new(k).color(t.text_dim));
                            ui.add(egui::Label::new(egui::RichText::new(v).color(t.text_label)).wrap());
                            ui.end_row();
                        }
                    });
                    if ui.button(crate::i18n::tr("Copy to Clipboard")).clicked() {
                        let text: String = rows.iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
                        ui.ctx().copy_text(text);
                    }
                }
                Dialog::AllMetadata { title, rows, search } => {
                    ui.label(egui::RichText::new(title.as_str()).color(t.text_label));
                    let r = ui.add(egui::TextEdit::singleline(search).hint_text(crate::i18n::tr("Filter fields")).desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:metadataSearch", r.rect);
                    let q = search.trim().to_lowercase();
                    let keep = |n: &str, v: &str| q.is_empty() || n.to_lowercase().contains(&q) || v.to_lowercase().contains(&q);
                    let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
                    for row in rows["exif"].as_array().into_iter().flatten() {
                        let (g, n, v) = (row["group"].as_str().unwrap_or(""), row["name"].as_str().unwrap_or(""), row["value"].as_str().unwrap_or(""));
                        if !keep(n, v) {
                            continue;
                        }
                        match groups.iter_mut().find(|(x, _)| x == g) {
                            Some((_, list)) => list.push((n.into(), v.into())),
                            None => groups.push((g.into(), vec![(n.into(), v.into())])),
                        }
                    }
                    let xmp: Vec<(String, String)> = rows["xmp"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|r| (r["name"].as_str().unwrap_or("").to_string(), r["value"].as_str().unwrap_or("").to_string()))
                        .filter(|(n, v)| keep(n, v))
                        .collect();
                    if !xmp.is_empty() {
                        groups.push(("XMP".into(), xmp));
                    }
                    egui::ScrollArea::vertical().max_height(440.0).auto_shrink([false, true]).show(ui, |ui| {
                        if groups.is_empty() {
                            ui.label(egui::RichText::new(rows["note"].as_str().unwrap_or("No metadata found")).color(t.text_dim));
                        }
                        for (g, list) in &groups {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(g).font(t.semibold(12.5)).color(t.text));
                            egui::Grid::new(format!("meta-{g}")).num_columns(2).spacing([16.0, 3.0]).striped(true).show(ui, |ui| {
                                for (n, v) in list {
                                    ui.label(egui::RichText::new(n).color(t.text_dim));
                                    ui.add(egui::Label::new(egui::RichText::new(v).color(t.text_label)).wrap());
                                    ui.end_row();
                                }
                            });
                        }
                    });
                }
                Dialog::SmartRules { name, rules, .. } => {
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text(crate::i18n::tr("Name")).desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:smartName", r.rect);
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                        crate::panels::rules_editor::edit(ui, rules, "rules", 0);
                    });
                    let problems = rules.problems();
                    let f = lightcraft_catalog::Filter { rule_set: Some(rules.clone()), ..Default::default() };
                    let n = if problems.is_empty() { app.session.catalog.query(&f, &Default::default()).len() } else { 0 };
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(match problems.first() {
                            Some(p) => p.clone(),
                            None => crate::i18n::tr_format!("{n} photo{} match · updates automatically as photos change", if n == 1 { "" } else { "s" }, n = n),
                        })
                        .color(t.text_dim),
                    );
                }
                Dialog::NewSmartAlbum { name } => {
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text(crate::i18n::tr("Name")).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    let rules = app.session.view_rules();
                    let n = app.session.catalog.query(&rules, &Default::default()).len();
                    ui.label(egui::RichText::new(crate::i18n::tr_format!("Matches: {}", rules.describe())).color(t.text_label));
                    ui.label(
                        egui::RichText::new(crate::i18n::tr_format!("{n} photo{} now · updates automatically as photos change", if n == 1 { "" } else { "s" }, n = n))
                            .color(t.text_dim),
                    );
                }
                Dialog::CaptureTime { mode, time, days, hours, minutes, zone } => {
                    let n = app.session.targets(&json!({})).len();
                    field(ui, "Change", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, (label, m)) in [("Set date & time", "set"), ("Shift by", "shift"), ("Time zone", "zone")].iter().enumerate() {
                            if crate::widgets::text_button(ui, &format!("captureMode-{i}"), label, mode == m).clicked() {
                                *mode = m.to_string();
                            }
                        }
                    });
                    let params = match mode.as_str() {
                        "set" => {
                            field(ui, "New time", |ui| {
                                let r = ui.add(egui::TextEdit::singleline(time).hint_text("2026-09-30 14:05:00").desired_width(f32::INFINITY));
                                crate::widgets::register(ui.ctx(), "field:captureTime", r.rect);
                            });
                            json!({"time": time})
                        }
                        "shift" => {
                            field(ui, "Shift", |ui| {
                                ui.add(egui::DragValue::new(days).suffix(" d"));
                                ui.add(egui::DragValue::new(hours).suffix(" h"));
                                ui.add(egui::DragValue::new(minutes).suffix(" min"));
                            });
                            json!({"shift": *days as i64 * 86_400 + *hours as i64 * 3600 + *minutes as i64 * 60})
                        }
                        _ => {
                            field(ui, "Hours", |ui| ui.add(egui::DragValue::new(zone).range(-26.0..=26.0).speed(0.25).fixed_decimals(2)));
                            json!({"hours": zone})
                        }
                    };
                    // preview on the active photo (the others move by the same amount)
                    let active = app.session.active().and_then(|id| app.session.catalog.photo(id)).map(|p| (p.file_name.clone(), p.date().to_string()));
                    if let Some((name, cur)) = active {
                        let delta = match mode.as_str() {
                            "set" => lightcraft_catalog::dates::normalize_iso(time)
                                .and_then(|t| Some(lightcraft_catalog::dates::iso_seconds(&t)? - lightcraft_catalog::dates::iso_seconds(&cur)?)),
                            "shift" => params["shift"].as_i64(),
                            _ => Some((*zone as f64 * 3600.0).round() as i64),
                        };
                        let after = delta.and_then(|d| lightcraft_catalog::dates::shift_iso(&cur, d));
                        ui.label(
                            egui::RichText::new(match after {
                                Some(a) => format!("{name}: {} → {}", cur.replace('T', " "), a.replace('T', " ")),
                                None => "Enter a date as YYYY-MM-DD HH:MM:SS".into(),
                            })
                            .color(t.text_label),
                        );
                    }
                    if n > 1 {
                        ui.label(egui::RichText::new(crate::i18n::tr_format!("All {n} selected photos move by the same amount.", n = n)).color(t.text_dim));
                    }
                }
                Dialog::LabelNames { names, save_as } => {
                    names.resize(5, String::new());
                    // start from a set
                    let sets = lightcraft_engine::cmd::manage::label_sets_json(&app.session);
                    field(ui, "Set", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for set in sets["sets"].as_array().into_iter().flatten() {
                            let name = set["name"].as_str().unwrap_or_default();
                            if crate::widgets::text_button(ui, &format!("labelSet-{name}"), name, false).clicked() {
                                *names = set["names"].as_array().into_iter().flatten().map(|n| n.as_str().unwrap_or_default().to_string()).collect();
                            }
                        }
                    });
                    for (i, l) in lightcraft_catalog::ColorLabel::ALL.iter().enumerate() {
                        let colour = format!("{l:?}");
                        field(ui, &colour, |ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 6.0, crate::panels::grid::label_color(*l));
                            let te = ui.add(egui::TextEdit::singleline(&mut names[i]).hint_text(colour.as_str()).desired_width(f32::INFINITY));
                            crate::widgets::register(ui.ctx(), format!("field:labelName-{}", colour.to_lowercase()), te.rect);
                        });
                    }
                    field(ui, "Save as set", |ui| {
                        let te = ui.add(egui::TextEdit::singleline(save_as).hint_text(crate::i18n::tr("Optional name")).desired_width(f32::INFINITY));
                        crate::widgets::register(ui.ctx(), "field:labelSetName", te.rect);
                    });
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("Names appear in the label menu, the filter bar and the Info panel, and are written to XMP. Empty = the colour's name."))
                            .color(t.text_dim),
                    );
                }
                Dialog::Rename { template, start } => {
                    let n = app.session.targets(&json!({})).len();
                    ui.label(egui::RichText::new(crate::i18n::tr_format!("{n} photo{}", if n == 1 { "" } else { "s" }, n = n)).color(t.text_dim));
                    let template_id = egui::Id::new("rename-template");
                    let tags_open = field(ui, "Template", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let w = (ui.available_width() - 50.0).max(80.0);
                        let r = ui.add(egui::TextEdit::singleline(template).id(template_id).hint_text("{name}").desired_width(w));
                        crate::widgets::register(ui.ctx(), "field:renameTemplate", r.rect);
                        crate::import::tag_toggle(ui, "renameTemplate")
                    });
                    if tags_open {
                        crate::import::tag_help(ui, "renameTemplate", template, template_id);
                    }
                    crate::import::unknown_tags_warning(ui, template);
                    field(ui, "Presets", |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, (label, tpl)) in
                            [("Name", "{name}"), ("Date–Name", "{date}_{name}"), ("Title–Seq", "{title}-{seq:3}"), ("Custom–Seq", "Photo-{seq:3}")].iter().enumerate()
                        {
                            if crate::widgets::text_button(ui, &format!("renamePreset-{i}"), label, template == tpl).clicked() {
                                *template = tpl.to_string();
                            }
                        }
                    });
                    field(ui, "Start at", |ui| ui.add(egui::DragValue::new(start).range(0..=999_999)));
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("Tags: see Tags beside the template. Files are renamed on disk (with their XMP sidecars); existing names get -1, -2…"))
                            .color(t.text_dim),
                    );
                    let (preview, total) = rename_preview(app, ui.ctx(), template, *start as usize);
                    match &preview {
                        Some(rows) => {
                            egui::Grid::new("rename-preview").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                                for pl in rows {
                                    ui.label(egui::RichText::new(&pl.from).color(t.text_dim));
                                    ui.label(egui::RichText::new("→").color(t.text_dim));
                                    ui.label(egui::RichText::new(&pl.to).color(t.text));
                                    ui.end_row();
                                }
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("Checking names…").color(t.text_dim));
                        }
                    }
                    let more = total.saturating_sub(RENAME_PREVIEW_ROWS);
                    if more > 0 {
                        ui.label(egui::RichText::new(crate::i18n::tr_format!("… and {more} more", more = more)).color(t.text_dim));
                    }
                }
                Dialog::RenameKeyword { from, to } => {
                    let n = app.session.catalog.photos().filter(|p| p.meta.keywords.iter().any(|k| lightcraft_catalog::keywords::is_under(k, from))).count();
                    let r = ui.add(egui::TextEdit::singleline(to).hint_text(crate::i18n::tr("New name")).desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:keywordName", r.rect);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Renames “{from}” on {n} photo{} (keywords below it too). Use | for levels, e.g. Travel|Italy. An existing name merges the two.",
                            if n == 1 { "" } else { "s" }
                        ))
                        .color(t.text_dim),
                    );
                }
                Dialog::MergeKeywords { from, into } => {
                    ui.label(egui::RichText::new(format!("Replace {} with:", from.iter().map(|f| format!("“{f}”")).collect::<Vec<_>>().join(", "))).color(t.text_label));
                    let r = ui.add(egui::TextEdit::singleline(into).hint_text(crate::i18n::tr("Keyword")).desired_width(f32::INFINITY));
                    crate::widgets::register(ui.ctx(), "field:keywordInto", r.rect);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    let options: Vec<String> = app
                        .session
                        .catalog
                        .keyword_suggestions(from, into, 12)
                        .into_iter()
                        .filter(|k| !from.iter().any(|f| lightcraft_catalog::keywords::is_under(k, f)))
                        .collect();
                    ui.horizontal_wrapped(|ui| {
                        for k in options {
                            if ui.add(egui::Button::new(egui::RichText::new(&k).color(t.text_label)).corner_radius(10.0)).clicked() {
                                *into = k;
                            }
                        }
                    });
                }
                Dialog::TextPrompt { value, hint, .. } => {
                    let r = ui.add(egui::TextEdit::singleline(value).hint_text(hint.as_str()).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                }
                Dialog::NewAlbum { name, .. } | Dialog::RenameAlbum { name, .. } => {
                    let r = ui.add(egui::TextEdit::singleline(name).hint_text(crate::i18n::tr("Name")).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                }
                Dialog::CreatePreset { name, group, groups } => {
                    ui.add(egui::TextEdit::singleline(name).hint_text(crate::i18n::tr("Preset name")).desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(group).hint_text(crate::i18n::tr("Group")).desired_width(f32::INFINITY));
                    ui.label(egui::RichText::new(crate::i18n::tr("Settings to include")).color(t.text_dim));
                    group_checklist(ui, "presetInclude", groups);
                }
                Dialog::CopySettings { groups } => group_checklist(ui, "copyGroup", groups),
                Dialog::PasteSettings { groups } => {
                    let n = app.session.targets(&json!({})).len();
                    ui.label(
                        egui::RichText::new(crate::i18n::tr_format!("Paste into {n} photo{} — only settings that were copied are pasted", if n == 1 { "" } else { "s" }, n = n))
                            .color(t.text_dim),
                    );
                    group_checklist(ui, "pasteGroup", groups);
                }
                d @ Dialog::Export { .. } => {
                    if crate::panels::export_dialog::body(app, ui, d) {
                        confirm = true;
                    }
                }
                Dialog::SaveOverOriginal { plan, dont_ask, .. } => save_over_body(ui, plan, dont_ask),
                Dialog::Merge { opts } => crate::merge::body(app, ui, opts),
                Dialog::Import { opts } => crate::import::body(app, ui, opts),
                Dialog::Settings { tab } => crate::panels::settings::body(app, ui, tab),
                Dialog::SamModel { error, .. } => sam_model_body(app, ui, error.as_deref()),
                Dialog::ConfirmDelete { count } => {
                    let what = if *count == 1 { crate::i18n::tr("this photo").to_string() } else { crate::i18n::tr_format!("these {count} photos", count = count) };
                    ui.label(crate::i18n::tr_format!("Move {what} to Recently Deleted?", what = what));
                    ui.label(egui::RichText::new(crate::i18n::tr("They can be restored from Recently Deleted until it is emptied.")).color(t.text_dim));
                }
                Dialog::About => {
                    ui.set_min_width(680.0);
                    let tab_id = egui::Id::new("about_tab");
                    let mut tab = ui.data_mut(|d| d.get_temp::<u8>(tab_id)).unwrap_or(0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, (id, label)) in ABOUT_TABS.iter().enumerate() {
                            if crate::widgets::text_button(ui, &format!("aboutTab-{id}"), label, usize::from(tab) == i).clicked() {
                                tab = u8::try_from(i).unwrap_or(0);
                            }
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(tab_id, tab));
                    ui.separator();
                    match tab {
                        1 => crate::credits::contributors_ui(app, ui),
                        2 => crate::credits::models_ui(ui),
                        _ => {
                            ui.label(egui::RichText::new("Local Image").font(t.semibold(20.0)).color(t.text));
                            ui.label(crate::i18n::tr_format!("Version {} — a clean-room, pure-Rust photo library and raw developer.", env!("CARGO_PKG_VERSION")));
                            ui.label(format!("GPL-3.0-or-later (includes PhotoCraft and LightCraft, MIT OR Apache-2.0). Fonts: {} (OFL). Icons: original.", crate::theme::font_credits()));
                            ui.label(crate::i18n::tr("Lens data: LensFun database (CC BY-SA 3.0, lensfun.github.io) via the lensfun crate (LGPL-3.0)."));
                            ui.add_space(10.0);
                            for (label, url) in [
                                ("Local Image website", crate::links::APP_PAGE),
                                ("Source code on GitHub", crate::links::GITHUB),
                                ("ArtCraft — more creative apps", crate::links::WEBSITE),
                            ] {
                                let r = ui.link(crate::i18n::tr(label)).on_hover_text(url);
                                if r.clicked() {
                                    let _ = crate::links::open(app, url);
                                }
                            }
                        }
                    }
                }
                Dialog::Shortcuts => {
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                            for (id, label, sc, _) in crate::menus::ui_commands() {
                                if let Some(sc) = sc {
                                    ui.label(crate::i18n::tr(label));
                                    ui.label(*sc);
                                    ui.label(egui::RichText::new(*id).color(t.text_dim));
                                    ui.end_row();
                                }
                            }
                            for c in lightcraft_engine::command_specs() {
                                if let Some(sc) = c.shortcut {
                                    ui.label(crate::i18n::tr(c.label));
                                    ui.label(sc);
                                    ui.label(egui::RichText::new(c.id).color(t.text_dim));
                                    ui.end_row();
                                }
                            }
                            for (sc, id, _) in crate::shortcuts::ALIASES {
                                let label = crate::menus::ui_commands()
                                    .find(|c| c.0 == *id)
                                    .map(|c| c.1)
                                    .or_else(|| lightcraft_engine::find_command(id).map(|c| c.label))
                                    .unwrap_or(id);
                                ui.label(crate::i18n::tr(label));
                                ui.label(*sc);
                                ui.label(egui::RichText::new(*id).color(t.text_dim));
                                ui.end_row();
                            }
                        });
                    });
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let informational = matches!(dlg, Dialog::About | Dialog::Shortcuts | Dialog::Settings { .. });
                let sam = &app.session.segmenter;
                let (sam_installed, sam_running, sam_failed) = (sam.installed(), sam.download_status().running, sam.download_status().error.is_some());
                // no download location in this build: nothing to offer but the manual install
                let sam_nowhere = !sam_installed && !sam_running && sam.mirrors().is_empty();
                let cancel = match &dlg {
                    Dialog::SamModel { .. } if sam_running || sam_installed || sam_nowhere => "Close",
                    Dialog::SamModel { .. } => "Not Now",
                    _ => "Cancel",
                };
                if !informational {
                    let r = ui.button(crate::i18n::tr(cancel));
                    crate::widgets::register(ui.ctx(), "button:dialogCancel", r.rect);
                    if r.clicked() {
                        close = true;
                    }
                }
                let add_label;
                // Save Over Original: Cancel / Save Copy Beside / Overwrite
                if let Dialog::SaveOverOriginal { plan, .. } = &dlg
                    && plan["mode"] == "overwrite"
                {
                    let r = ui.button(crate::i18n::tr("Save Copy Beside"));
                    crate::widgets::register(ui.ctx(), "button:saveOverBeside", r.rect);
                    if r.clicked() {
                        save_beside = true;
                        confirm = true;
                    }
                }
                let ok = match &dlg {
                    Dialog::Import { opts } => {
                        let n = opts.selected_paths().len();
                        let verb = if opts.copy && opts.move_files { "Move" } else { "Import" };
                        add_label = crate::i18n::tr_format!("{verb} {n} Photo{}", if n == 1 { "" } else { "s" }, verb = crate::i18n::tr(verb), n = n);
                        add_label.as_str()
                    }
                    Dialog::Merge { .. } => "Merge",
                    Dialog::Export { .. } => {
                        let n = crate::control::export_targets(app).len();
                        add_label = format!("{} {n} {}", crate::i18n::tr("Export"), crate::i18n::tr(if n == 1 { "Photo" } else { "Photos" }));
                        add_label.as_str()
                    }
                    Dialog::SaveOverOriginal { plan, .. } if plan["mode"] == "beside" => "Save JPEG Beside",
                    Dialog::SaveOverOriginal { .. } => "Overwrite",
                    Dialog::ConfirmDelete { .. } => "Delete",
                    Dialog::SamModel { then: Some(_), .. } if sam_installed => "Continue",
                    Dialog::SamModel { .. } if sam_installed || sam_running || sam_nowhere => "",
                    Dialog::SamModel { error, .. } if error.is_some() || sam_failed => "Try Again",
                    Dialog::SamModel { .. } => "Download",
                    _ if informational => "Close",
                    _ => "OK",
                };
                let r = (!ok.is_empty()).then(|| ui.button(crate::i18n::tr(ok)));
                if let Some(r) = &r {
                    crate::widgets::register(ui.ctx(), "button:dialogOk", r.rect);
                }
                if r.is_some_and(|r| r.clicked()) {
                    if informational {
                        close = true;
                    } else {
                        confirm = true;
                    }
                }
            });
        });
    if let Some(w) = shown {
        ctx.move_to_top(w.response.layer_id);
        crate::widgets::register(ctx, "dialog:window", w.response.rect);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if save_beside && let Dialog::SaveOverOriginal { beside, .. } = &mut dlg {
        *beside = true;
    }
    // Existing Files ▸ Ask what to do: ask before the export starts when files are in the way
    if confirm
        && let Dialog::Export { ask_existing: true, existing: None, .. } = &dlg
    {
        let n = crate::panels::export_dialog::existing_outputs(app, &dlg);
        if let Dialog::Export { existing, .. } = &mut dlg {
            *existing = Some(n);
        }
        if n > 0 {
            confirm = false;
        }
    }
    if confirm {
        match confirm_dialog(app, &dlg) {
            // the import review stays open on an error (e.g. an unusable folder template), and so
            // does Export (e.g. no folder chosen) so the choices aren't lost
            Err(e) if matches!(dlg, Dialog::Import { .. } | Dialog::Export { .. }) => app.toast(ctx, e),
            // the SAM 3 dialog stays open to show the download (or why it can't start)
            Err(e) if matches!(dlg, Dialog::SamModel { .. }) => {
                if let Dialog::SamModel { error, .. } = &mut dlg {
                    *error = Some(e);
                }
            }
            // asked again next time
            Ok(_) if keeps_open(app, &dlg) => {
                if let Dialog::SamModel { error, .. } = &mut dlg {
                    *error = None;
                }
            }
            _ => close = true,
        }
    }
    app.ui.dialog = if close { None } else { Some(dlg) };
}

/// Apply a dialog's action (also used by `ui.dialog.confirm`).
/// `90` → `1 min 30 s`.
pub fn fmt_gap(v: f64) -> String {
    let v = v.round() as u64;
    match v {
        0..60 => format!("{v} s"),
        60..3600 if v.is_multiple_of(60) => format!("{} min", v / 60),
        60..3600 => format!("{} min {} s", v / 60, v % 60),
        3600..86400 if v.is_multiple_of(3600) => format!("{} h", v / 3600),
        3600..86400 => format!("{} h {} min", v / 3600, v % 3600 / 60),
        _ => "1 day".into(),
    }
}

/// File ▸ Save Over Original: what will happen, the backup, and Don't ask again.
fn save_over_body(ui: &mut egui::Ui, plan: &serde_json::Value, dont_ask: &mut bool) {
    let t = Tokens::get(ui.ctx());
    let s = |k: &str| plan[k].as_str().unwrap_or_default().to_string();
    let wrap = |ui: &mut egui::Ui, text: String, color: egui::Color32| {
        ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
    };
    if plan["mode"] == "overwrite" {
        wrap(ui, format!("{} {}?", crate::i18n::tr("Replace the original file"), s("fileName")), t.text);
        wrap(
            ui,
            format!(
                "{} {} {}",
                crate::i18n::tr("The edited photo is saved over it at full size as"),
                s("format"),
                crate::i18n::tr("(same color space and bit depth, with its metadata). The edits are then part of the pixels: the photo's settings, history, masks and AI removals are reset."),
            ),
            t.text_label,
        );
        wrap(
            ui,
            format!(
                "{} {}. {}",
                crate::i18n::tr("A copy of the original is kept in"),
                s("backupDir"),
                crate::i18n::tr("This can't be undone: the backup is the way back.")
            ),
            t.text_dim,
        );
    } else {
        wrap(ui, format!("{} {}", s("fileName"), crate::i18n::tr("is a raw file (or a format Local Image can't write), so it is never replaced.")), t.text);
        wrap(
            ui,
            format!(
                "{} {} {}",
                crate::i18n::tr("A JPEG of the edited photo is saved beside it as"),
                s("besideName"),
                crate::i18n::tr("(or the next free name), added to the library and stacked on top of the original.")
            ),
            t.text_label,
        );
    }
    for w in plan["warnings"].as_array().into_iter().flatten() {
        wrap(ui, w.as_str().unwrap_or_default().to_string(), t.caution);
    }
    ui.add_space(2.0);
    let r = ui.checkbox(dont_ask, crate::i18n::tr("Don't ask again"));
    crate::widgets::register(ui.ctx(), "check:saveOverDontAsk", r.rect);
    if *dont_ask {
        ui.label(egui::RichText::new(crate::i18n::tr("Turn asking back on in Settings ▸ General.")).size(11.5).color(t.text_dim));
    }
}

/// Whether a dialog stays open after its action succeeded (the SAM 3 dialog while the model
/// downloads).
pub fn keeps_open(app: &LightcraftApp, dlg: &Dialog) -> bool {
    matches!(dlg, Dialog::SamModel { .. }) && !app.session.segmenter.installed()
}

/// The SAM 3 dialog: what the model is, its size and licence, and the download's progress.
fn sam_model_body(app: &mut LightcraftApp, ui: &mut egui::Ui, error: Option<&str>) {
    use lightcraft_engine::segment::{LICENSE_NAME, LICENSE_URL, MODEL_BYTES};
    let t = Tokens::get(ui.ctx());
    let seg = &app.session.segmenter;
    let d = seg.download_status();
    if seg.installed() {
        ui.label(crate::i18n::tr("The SAM 3 model is installed: Object and Describe masks are ready."));
        return;
    }
    let gb = |b: u64| b as f64 / 1e9;
    ui.label(crate::i18n::tr(
        "Object and Describe masks use SAM 3, Meta's segmentation model. It isn't part of Local Image, and everything else works without it.",
    ));
    let dir = seg.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
    ui.label(format!("{} {:.1} GB, {} {dir}", crate::i18n::tr("A one-time download of about"), gb(MODEL_BYTES), crate::i18n::tr("saved in")));
    ui.label(
        egui::RichText::new(format!(
            "{} {LICENSE_NAME} — {}",
            crate::i18n::tr("Licence:"),
            crate::i18n::tr("Meta's terms, not Local Image's. Downloading it means accepting them.")
        ))
        .color(t.text_label),
    );
    let r = ui.link(crate::i18n::tr("Read the SAM License")).on_hover_text(LICENSE_URL);
    crate::widgets::register(ui.ctx(), "link:samLicense", r.rect);
    if r.clicked() {
        let _ = crate::links::open(app, LICENSE_URL);
    }
    let seg = &app.session.segmenter;
    if d.running {
        ui.add_space(4.0);
        let frac = if d.total > 0 { d.done as f64 / d.total as f64 } else { 0.0 };
        let text = format!("{:.2} / {:.2} GB · {}", gb(d.done), gb(d.total), d.file);
        let r = ui.add(egui::ProgressBar::new(frac as f32).text(text));
        crate::widgets::register(ui.ctx(), "progress:samDownload", r.rect);
        let r = ui.button(crate::i18n::tr("Cancel Download"));
        crate::widgets::register(ui.ctx(), "button:samCancel", r.rect);
        if r.clicked() {
            app.session.segmenter.cancel_download();
        }
        ui.label(egui::RichText::new(crate::i18n::tr("You can close this: the download continues, and resumes if interrupted.")).color(t.text_dim));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        return;
    }
    if seg.mirrors().is_empty() {
        ui.label(
            egui::RichText::new(crate::i18n::tr(
                "This build has no download location for the model yet: put the files in the folder above yourself (see docs/ai-masks.md).",
            ))
            .color(t.text_dim),
        );
    }
    if let Some(e) = error.map(str::to_string).or(d.error) {
        ui.label(egui::RichText::new(format!("{} {e}", crate::i18n::tr("The download didn't work:"))).color(egui::Color32::from_rgb(230, 90, 80)));
    }
}

pub fn confirm_dialog(app: &mut LightcraftApp, dlg: &Dialog) -> Result<serde_json::Value, String> {
    match dlg {
        Dialog::SamModel { then, .. } => {
            if app.session.segmenter.installed() {
                // installed: start what the user was doing
                if let Some((kind, op)) = then {
                    crate::panels::masking::begin_ai(app, kind, op)?;
                }
                return Ok(serde_json::Value::Null);
            }
            let r = app.run("segment.model.download", json!({"acknowledged": true}));
            if r.is_ok() {
                app.ui.sam_downloading = true;
            }
            r
        }
        Dialog::NewAlbum { name, folder } => app.run("album.create", json!({"name": name, "folder": folder, "addSelected": !folder})),
        Dialog::RenameAlbum { id, name } => app.run("album.rename", json!({"id": id, "name": name})),
        Dialog::TextPrompt { value, command, params, key, .. } => {
            let mut p = params.clone();
            p[key.as_str()] = json!(value.trim());
            app.run(command, p)
        }
        Dialog::CaptureTime { mode, time, days, hours, minutes, zone } => app.run(
            "photo.setCaptureTime",
            match mode.as_str() {
                "set" => json!({"time": time}),
                "shift" => json!({"shift": *days as i64 * 86_400 + *hours as i64 * 3600 + *minutes as i64 * 60}),
                _ => json!({"hours": zone}),
            },
        ),
        Dialog::LabelNames { names, save_as } => {
            let mut m = serde_json::Map::new();
            for (l, n) in lightcraft_catalog::ColorLabel::ALL.iter().zip(names) {
                m.insert(format!("{l:?}").to_lowercase(), if n.trim().is_empty() { serde_json::Value::Null } else { json!(n.trim()) });
            }
            let r = app.run("label.setNames", json!({"names": m}));
            if r.is_ok() && !save_as.trim().is_empty() {
                return app.run("label.saveSet", json!({"name": save_as.trim()}));
            }
            r
        }
        Dialog::Rename { template, start } => app.run("photo.rename", json!({"template": template, "start": start})),
        Dialog::RenameKeyword { from, to } => app.run("keyword.rename", json!({"from": from, "to": to})),
        Dialog::MergeKeywords { from, into } => app.run("keyword.merge", json!({"from": from, "into": into})),
        Dialog::AutoStack { gap } => app.run("stack.auto", json!({"gap": gap})),
        Dialog::AllMetadata { .. } | Dialog::SystemInfo { .. } | Dialog::WhatsNew => Ok(serde_json::Value::Null),
        Dialog::Cull { reject_below, pick_best } => {
            let mut p = json!({"pickBest": pick_best});
            if *reject_below > 0.0 {
                p["rejectBelow"] = json!(reject_below);
            }
            app.run("photo.analyze", p)
        }
        Dialog::SmartRules { id, name, rules } => {
            let name = if name.trim().is_empty() { "Smart Album".to_string() } else { name.trim().to_string() };
            match id {
                Some(id) => {
                    if app.session.catalog.album(lightcraft_catalog::AlbumId(*id)).is_some_and(|a| a.name != name) {
                        app.run("album.rename", json!({"id": id, "name": name}))?;
                    }
                    app.run("album.setRules", json!({"id": id, "replace": true, "rules": {"ruleSet": rules}}))
                }
                None => app.run("album.createSmart", json!({"name": name, "rules": {"ruleSet": rules}})),
            }
        }
        Dialog::NewSmartAlbum { name } => app.run("album.createSmart", json!({"name": if name.trim().is_empty() { "Smart Album" } else { name }})),
        Dialog::CreatePreset { name, group, groups } => app.run(
            "preset.create",
            json!({
                "name": if name.trim().is_empty() { "My Preset" } else { name },
                "group": if group.trim().is_empty() { "User Presets" } else { group },
                "groups": groups,
            }),
        ),
        Dialog::CopySettings { groups } => app.run("develop.copy", json!({"groups": groups})),
        Dialog::PasteSettings { groups } => app.run("develop.paste", json!({"groups": groups})),
        Dialog::Export { dir, .. } => {
            let mut p = crate::panels::export_dialog::export_dialog_params(dlg);
            p["dir"] = json!(dir);
            p["background"] = json!(true);
            app.run("app.export", p)
        }
        Dialog::SaveOverOriginal { plan, dont_ask, beside } => {
            if *dont_ask {
                app.ui.settings.confirm_save_over = false;
            }
            crate::menus::run_save_over(app, &egui::Context::default(), plan, *beside)
        }
        Dialog::Merge { opts } => crate::merge::start_final(app, opts),
        Dialog::Import { opts } => crate::import::start(app, opts),
        Dialog::ConfirmDelete { .. } => app.run("photo.delete", json!({})),
        Dialog::About | Dialog::Shortcuts | Dialog::Settings { .. } => Ok(serde_json::Value::Null),
    }
}

/// Open a one-field dialog that runs `command` with `params` + `{key: typed value}`.
pub fn prompt(app: &mut LightcraftApp, title: &str, hint: &str, value: &str, command: &str, params: serde_json::Value, key: &str) {
    app.ui.dialog =
        Some(Dialog::TextPrompt { title: title.into(), hint: hint.into(), value: value.into(), command: command.into(), params, key: key.into() });
}

// ------------------------------------------------------------------------------------------ dialog widgets

/// Two columns of settings-group checkboxes (Copy Settings, Create Preset) plus All / None.
fn group_checklist(ui: &mut egui::Ui, tag: &str, groups: &mut Vec<String>) {
    let key_of = |g: &SettingsGroup| serde_json::to_value(g).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    ui.columns(2, |cols| {
        for (i, g) in SettingsGroup::ALL.iter().enumerate() {
            let key = key_of(g);
            let mut on = groups.contains(&key);
            let r = cols[i % 2].checkbox(&mut on, g.label());
            crate::widgets::register(&r.ctx, format!("{tag}:{key}"), r.rect);
            if r.changed() {
                if on {
                    groups.push(key);
                } else {
                    groups.retain(|x| *x != key);
                }
            }
        }
    });
    ui.horizontal(|ui| {
        let all = ui.small_button("All");
        crate::widgets::register(ui.ctx(), format!("{tag}:all"), all.rect);
        if all.clicked() {
            *groups = SettingsGroup::ALL.iter().map(key_of).collect();
        }
        let none = ui.small_button("None");
        crate::widgets::register(ui.ctx(), format!("{tag}:none"), none.rect);
        if none.clicked() {
            groups.clear();
        }
    });
}

/// Width of the label column in dialogs.
const LABEL_W: f32 = 78.0;

/// A labelled row (fixed label column).
fn field<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            ui.label(egui::RichText::new(crate::i18n::tr(label)).color(t.text_label));
        });
        add(ui)
    })
    .inner
}
