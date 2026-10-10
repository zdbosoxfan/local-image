//! A single resumable Sort → Review → Export workspace. Persisted folder definitions use
//! catalog rules, so export and the optional smart albums always agree with reviewed keywords.
use crate::smart_sort_keys::{self, ReviewAction};
use crate::{LightcraftApp, i18n::tr, state::Dialog, widgets::register};
use egui::{Response, Ui};
use lightcraft_catalog::{Flag, PhotoId};
use lightcraft_engine::smart_sort::{Category, FolderDef, Sensitivity, SortPreset, builtin_presets, classify::Classification, plan};
use lightcraft_engine::smart_sort::{
    bursts::{Burst, BurstExport, BurstStrictness},
    sessions::{self},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SmartSortDialog {
    pub step: usize,
    pub preset: SortPreset,
    pub source: Vec<PhotoId>,
    pub source_name: String,
    pub include_rejected: bool,
    pub rows: Vec<ReviewPhoto>,
    pub overrides: BTreeMap<PhotoId, Vec<String>>,
    pub counts: BTreeMap<String, usize>,
    pub selected: Vec<PhotoId>,
    pub review_folder: String,
    pub least_sure: bool,
    pub inputs: Vec<String>,
    pub dirty: bool,
    pub analysed: usize,
    pub skipped: usize,
    pub videos: usize,
    pub failed: Vec<(PhotoId, String)>,
    pub error: String,
    pub dest: String,
    pub export_preset: String,
    pub export_params: Value,
    pub albums: bool,
    pub save_name: String,
    pub tag_set_name: String,
    pub tag_set_folder: Option<usize>,
    pub tag_set_focus: bool,
    pub manage_sets: bool,
    pub rename_sets: BTreeMap<String, String>,
    pub rules_open: Option<usize>,
    pub settings: Option<Box<Dialog>>,
    pub bursts: Vec<Burst>,
    pub library_examples: Vec<PhotoId>,
    pub focus: Option<PhotoId>,
    pub anchor: Option<PhotoId>,
    pub undo: Vec<ReviewSnapshot>,
    pub redo: Vec<ReviewSnapshot>,
    // People will occupy a second review panel in Phase 3; names/patterns stay in the preset.
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewSnapshot {
    overrides: BTreeMap<PhotoId, Vec<String>>,
    categories: Vec<Category>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewPhoto {
    pub id: PhotoId,
    #[serde(flatten)]
    pub row: Classification,
}
impl Default for SmartSortDialog {
    fn default() -> Self {
        Self {
            step: 0,
            preset: builtin_presets().remove(0),
            source: Vec::new(),
            source_name: String::new(),
            include_rejected: false,
            rows: Vec::new(),
            overrides: BTreeMap::new(),
            counts: BTreeMap::new(),
            selected: Vec::new(),
            review_folder: "Unsorted".into(),
            least_sure: true,
            inputs: Vec::new(),
            dirty: false,
            analysed: 0,
            skipped: 0,
            videos: 0,
            failed: Vec::new(),
            error: String::new(),
            dest: String::new(),
            export_preset: "JPEG (Large)".into(),
            export_params: Value::Null,
            albums: false,
            save_name: String::new(),
            tag_set_name: String::new(),
            tag_set_folder: None,
            tag_set_focus: false,
            manage_sets: false,
            rename_sets: BTreeMap::new(),
            rules_open: None,
            settings: None,
            bursts: Vec::new(),
            library_examples: Vec::new(),
            focus: None,
            anchor: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}
pub fn open(app: &mut LightcraftApp, selected_only: bool) {
    let source = if selected_only || app.session.selection.ids.len() > 1 { app.session.selection.ids.clone() } else { app.session.visible_cloned() };
    let source_name = if selected_only || app.session.selection.ids.len() > 1 {
        tr("Selected photos").to_string()
    } else if let Some(browse) = &app.session.browse {
        browse.path.clone()
    } else if let lightcraft_engine::LibrarySource::Album(id) = app.session.source {
        app.session.catalog.album(id).map(|a| a.name.clone()).unwrap_or_else(|| tr("Current view").to_string())
    } else if let Some(date) = &app.session.filter.date {
        date.clone()
    } else {
        tr("Current view").to_string()
    };
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut preset = app.session.smart.prefs.last.clone().unwrap_or_else(|| builtin_presets().remove(0));
    if preset.event_name.is_empty() {
        preset.event_name = app
            .session
            .browse
            .as_ref()
            .map(|b| std::path::Path::new(&b.path).file_name().unwrap_or_default().to_string_lossy().into_owned())
            .or_else(|| {
                source.iter().find_map(|id| {
                    let lightcraft_catalog::Source::File { path } = &app.session.catalog.photo(*id)?.source else { return None };
                    Some(std::path::Path::new(path).parent()?.file_name()?.to_string_lossy().into_owned())
                })
            })
            .unwrap_or_default();
    }
    let selected = app.session.selection.ids.iter().copied().filter(|id| source.contains(id)).collect();
    app.ui.dialog = Some(Dialog::SmartSort {
        state: Box::new(SmartSortDialog {
            source,
            source_name,
            library_examples: selected,
            preset,
            dest: home.join("Desktop").to_string_lossy().into_owned(),
            ..Default::default()
        }),
    });
}
fn button(ui: &mut Ui, id: impl Into<String>, label: &str) -> Response {
    let r = ui.button(tr(label));
    register(ui.ctx(), id, r.rect);
    r
}
fn text(ui: &mut Ui, id: impl Into<String>, value: &mut String, width: f32) -> Response {
    let r = ui.add(egui::TextEdit::singleline(value).desired_width(width));
    register(ui.ctx(), id, r.rect);
    r
}
fn check(ui: &mut Ui, id: impl Into<String>, value: &mut bool, label: &str) -> Response {
    let r = ui.checkbox(value, tr(label));
    register(ui.ctx(), id, r.rect);
    r
}
fn ids(app: &LightcraftApp, s: &SmartSortDialog) -> Vec<PhotoId> {
    s.source.iter().copied().filter(|id| app.session.catalog.photo(*id).is_some_and(|p| s.include_rejected || p.flag != Flag::Reject)).collect()
}
fn classify(app: &mut LightcraftApp, s: &mut SmartSortDialog) -> Result<(), String> {
    let first_review = s.rows.is_empty();
    let overrides: Vec<_> = s.overrides.iter().map(|(id, categories)| json!({"id":id,"categories":categories})).collect();
    let result = app
        .session
        .execute("smartSort.classify", &json!({"preset":s.preset,"ids":ids(app,s),"overrides":overrides,"includeRejected":s.include_rejected}))
        .map_err(|e| e.to_string())?;
    s.rows = serde_json::from_value(result["photos"].clone()).map_err(|e| e.to_string())?;
    s.counts = serde_json::from_value(result["counts"].clone()).map_err(|e| e.to_string())?;
    if first_review
        && s.counts.get(&s.review_folder).copied().unwrap_or(0) == 0
        && let Some(category) = s.preset.categories.iter().find(|c| s.counts.get(&c.name).copied().unwrap_or(0) > 0)
    {
        s.review_folder = category.name.clone();
    }
    // The command returns its learned exemplars through the last preset.
    if let Some(last) = &app.session.smart.prefs.last {
        s.preset.categories = last.categories.clone();
    }
    refresh_bursts(app, s)?;
    s.dirty = false;
    Ok(())
}
fn advance(app: &mut LightcraftApp, s: &mut SmartSortDialog) -> Result<(), String> {
    if s.step == 0 {
        if s.rows.is_empty() {
            return Err(tr("Analyse photos before review").to_string());
        }
        s.step = 1;
        return Ok(());
    }
    if s.step == 1 {
        if s.dirty {
            classify(app, s)?;
        }
        let assignments: Vec<_> = s.rows.iter().map(|p| json!({"id":p.id,"categories":p.row.assigned})).collect();
        app.session.execute("smartSort.applyKeywords", &json!({"preset":s.preset,"assignments":assignments})).map_err(|e| e.to_string())?;
        if s.preset.folders.is_empty() {
            s.preset.folders = plan::default_folders(&s.preset);
        }
        s.step = 2;
        save_last(app, s)?;
    }
    Ok(())
}
fn save_last(app: &mut LightcraftApp, s: &SmartSortDialog) -> Result<(), String> {
    app.session.smart.prefs.last = Some(s.preset.clone());
    app.session.save_prefs().map_err(|e| e.to_string())
}
fn correct(app: &mut LightcraftApp, ctx: &egui::Context, s: &mut SmartSortDialog, folder: &str, add: bool, remove: bool) {
    if s.selected.is_empty() {
        return;
    }
    s.undo.push(snapshot(s));
    s.redo.clear();
    for id in &s.selected {
        let mut assigned =
            if add || remove { s.rows.iter().find(|p| p.id == *id).map(|p| p.row.assigned.clone()).unwrap_or_default() } else { Vec::new() };
        if remove {
            assigned.retain(|c| c != folder);
        } else if folder != "Unsorted" && !assigned.iter().any(|c| c == folder) {
            assigned.push(folder.into());
        }
        s.overrides.insert(*id, assigned);
    }
    s.dirty = true;
    if let Err(e) = classify(app, s) {
        s.error = e;
    }
    app.toast(ctx, crate::i18n::tr_format!("Learned from {n} corrections", n = s.overrides.len()));
}

pub fn show(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(Dialog::SmartSort { mut state }) = app.ui.dialog.clone() else { return };
    let s = &mut *state;
    if s.dirty
        && app.smart_sort.is_none()
        && let Err(e) = classify(app, s)
    {
        s.error = e;
        s.dirty = false;
    }
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("smart-sort-dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
    });
    let mut close = false;
    let shown = egui::Window::new(tr("Smart Sort & Export…"))
        .id(egui::Id::new("smart-sort-dialog"))
        .collapsible(false)
        .resizable(true)
        .default_size([1050.0, 720.0])
        .min_size([800.0, 540.0])
        .max_size(screen.size() - egui::vec2(40.0, 40.0))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, key, label) in [(0, "sort", "1 Sort"), (1, "review", "2 Review"), (2, "export", "3 Export")] {
                    let r = ui
                        .add_enabled(app.smart_sort.is_none() && (i == 0 || !s.rows.is_empty()), egui::Button::new(tr(label)).selected(s.step == i));
                    register(ui.ctx(), format!("smartSort:step:{key}"), r.rect);
                    if r.clicked() {
                        if i == 2 && s.step < 2 {
                            s.step = 1;
                            if let Err(e) = advance(app, s) {
                                s.error = e;
                            }
                        } else {
                            s.step = i;
                        }
                    }
                }
                ui.label(crate::i18n::tr_format!("{n} photos from {source}", n = ids(app, s).len(), source = &s.source_name));
            });
            ui.separator();
            let body_h = (ui.available_height() - 72.0).max(340.0);
            egui::ScrollArea::vertical().id_salt("smart-sort-body").max_height(body_h).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_min_height(body_h);
                match s.step {
                    0 => sort(app, ui, s),
                    1 => review(app, ui, s),
                    _ => export(app, ui, s),
                }
            });
            if !s.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &s.error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if button(ui, "smartSort:cancel", "Cancel").clicked() {
                    if let Some(task) = &app.smart_sort {
                        task.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        close = true;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if s.step == 0 {
                        let status = app.session.execute("smartSort.status", &json!({})).unwrap_or_default();
                        let r = ui.add_enabled(
                            app.smart_sort.is_none() && status["tagger"]["installed"] == true,
                            egui::Button::new(crate::i18n::tr_format!("Analyse {n} Photos", n = ids(app, s).len())),
                        );
                        register(ui.ctx(), "smartSort:analyze", r.rect);
                        if r.clicked() {
                            s.analysed = 0;
                            s.failed.clear();
                            s.error.clear();
                            if let Err(e) = s.preset.validate().and_then(|()| crate::smart_sort_task::start(app, &ids(app, s))) {
                                s.error = e;
                            }
                        }
                    } else if s.step == 2 {
                        let r = ui.add_enabled(app.export.is_none(), egui::Button::new(tr("Export")));
                        register(ui.ctx(), "smartSort:export", r.rect);
                        if r.clicked() {
                            match start_export(app, s) {
                                Ok(()) => close = true,
                                Err(e) => s.error = e,
                            }
                        }
                    } else if s.step == 1
                        && button(ui, "smartSort:next", "Next").clicked()
                        && let Err(e) = advance(app, s)
                    {
                        s.error = e;
                    }
                    if s.step > 0 && button(ui, "smartSort:back", "Back").clicked() {
                        s.step -= 1;
                    }
                });
            });
        });
    if let Some(w) = shown {
        ctx.move_to_top(w.response.layer_id);
        register(ctx, "dialog:window", w.response.rect);
    }
    if s.step != 1 && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if let Some(task) = &app.smart_sort {
            task.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        } else {
            close = true;
        }
    }
    if close {
        app.session.smart.tagger = None;
        app.ui.dialog = None;
    } else {
        app.ui.dialog = Some(Dialog::SmartSort { state });
    }
}

fn sort(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog) {
    ui.horizontal(|ui| {
        ui.label(tr("Preset"));
        let presets: Vec<_> = builtin_presets().into_iter().chain(app.session.smart.prefs.presets.clone()).collect();
        let response = egui::ComboBox::from_id_salt("smart-sort-preset").selected_text(tr(&s.preset.name)).show_ui(ui, |ui| {
            for (i, preset) in presets.iter().enumerate() {
                let r = ui.selectable_label(s.preset.name == preset.name, tr(&preset.name));
                register(ui.ctx(), format!("smartSort:preset:{i}"), r.rect);
                if r.clicked() {
                    s.preset = preset.clone();
                    s.inputs.clear();
                    s.overrides.clear();
                    s.undo.clear();
                    s.redo.clear();
                    s.dirty = !s.rows.is_empty();
                }
            }
        });
        register(ui.ctx(), "smartSort:preset", response.response.rect);
        text(ui, "smartSort:presetName", &mut s.save_name, 150.0);
        if button(ui, "smartSort:savePreset", "Save Preset…").clicked() {
            let mut preset = s.preset.clone();
            preset.name = s.save_name.trim().into();
            if let Err(e) = app.session.execute("smartSort.savePreset", &json!({"preset":preset})) {
                s.error = e.to_string();
            }
        }
    });
    session_options(app, ui, s);
    burst_options(app, ui, s);
    s.inputs.resize(s.preset.categories.len(), String::new());
    let sets = app.session.execute("smartSort.tagSets", &json!({})).unwrap_or_default();
    let mut delete = None;
    for i in 0..s.preset.categories.len() {
        ui.push_id(i, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let c = &mut s.preset.categories[i];
                    let old_name = c.name.clone();
                    if text(ui, format!("smartSort:folder:{i}"), &mut c.name, 220.0).changed() {
                        for categories in s.overrides.values_mut() {
                            for category in categories {
                                if *category == old_name {
                                    *category = c.name.clone();
                                }
                            }
                        }
                        for folder in &mut s.preset.folders {
                            if !folder.custom && folder.name == old_name {
                                folder.name = c.name.clone();
                            }
                            for tag in &mut folder.tags {
                                if *tag == old_name {
                                    *tag = c.name.clone();
                                }
                            }
                            rename_rule_keyword(
                                &mut folder.rules,
                                &format!("{}|{}", lightcraft_catalog::keywords::clean(&s.preset.keyword_parent), old_name.trim()),
                                &format!("{}|{}", lightcraft_catalog::keywords::clean(&s.preset.keyword_parent), c.name.trim()),
                            );
                        }
                        s.dirty = !s.rows.is_empty();
                    }
                    let r = check(ui, format!("smartSort:matchAll:{i}"), &mut c.match_all, "All tags");
                    if r.changed() {
                        s.dirty = !s.rows.is_empty();
                    }
                    ui.label(tr(if c.match_all { "Match all tags" } else { "Match any tag" }));
                    let menu = ui.menu_button(tr("Tag sets"), |ui| {
                        if button(ui, format!("smartSort:saveTagSet:{i}"), "Save Tags as Tag Set…").clicked() {
                            s.tag_set_folder = Some(i);
                            s.tag_set_focus = true;
                            ui.close();
                        }
                        for (j, set) in sets.as_array().into_iter().flatten().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(set["name"].as_str().unwrap_or_default());
                                for (mode, label) in [(false, "Replace"), (true, "Add")] {
                                    if button(ui, format!("smartSort:applyTagSet:{i}:{j}:{}", if mode { "add" } else { "replace" }), label).clicked()
                                    {
                                        let tags: Vec<String> = serde_json::from_value(set["tags"].clone()).unwrap_or_default();
                                        if !mode {
                                            c.prompts.clear();
                                        }
                                        for tag in tags {
                                            if !c.prompts.contains(&tag) {
                                                c.prompts.push(tag);
                                            }
                                        }
                                        s.dirty = !s.rows.is_empty();
                                        ui.close();
                                    }
                                }
                            });
                        }
                        if button(ui, "smartSort:manageTagSets", "Manage Tag Sets…").clicked() {
                            s.manage_sets = true;
                            ui.close();
                        }
                    });
                    register(ui.ctx(), format!("smartSort:tagSetMenu:{i}"), menu.response.rect);
                    if button(ui, format!("smartSort:deleteFolder:{i}"), "Delete").clicked() {
                        delete = Some(i);
                    }
                });
                let c = &mut s.preset.categories[i];
                let mut remove = None;
                let mut reorder = None;
                let chips = ui.horizontal_wrapped(|ui| {
                    for (j, tag) in c.prompts.iter().enumerate() {
                        egui::Frame::NONE
                            .fill(ui.visuals().widgets.inactive.bg_fill)
                            .corner_radius(7)
                            .inner_margin(egui::Margin::symmetric(5, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let r = ui.add(egui::Label::new(tag).sense(egui::Sense::click_and_drag()));
                                    register(ui.ctx(), format!("smartSort:tag:{i}:{j}"), r.rect);
                                    r.dnd_set_drag_payload((i, j));
                                    if let Some(from) = r.dnd_release_payload::<(usize, usize)>()
                                        && from.0 == i
                                    {
                                        reorder = Some((from.1, j));
                                    }
                                    let r = ui.small_button("×");
                                    register(ui.ctx(), format!("smartSort:removeTag:{i}:{j}"), r.rect);
                                    if r.clicked() {
                                        remove = Some(j);
                                    }
                                });
                            });
                    }
                });
                register(ui.ctx(), format!("smartSort:tags:{i}"), chips.response.rect);
                register(ui.ctx(), format!("smartSort:prompts:{i}"), chips.response.rect);
                if let Some(j) = remove {
                    c.prompts.remove(j);
                    s.dirty = !s.rows.is_empty();
                }
                if let Some((from, to)) = reorder
                    && from != to
                {
                    let tag = c.prompts.remove(from);
                    c.prompts.insert(to, tag);
                    s.dirty = !s.rows.is_empty();
                }
                let r = ui.add(
                    egui::TextEdit::singleline(&mut s.inputs[i]).hint_text(tr("Type tags, separated by commas")).desired_width(ui.available_width()),
                );
                register(ui.ctx(), format!("smartSort:tagInput:{i}"), r.rect);
                let query = s.inputs[i].trim().to_lowercase();
                if !query.is_empty() && !query.contains(',') {
                    let candidates: std::collections::BTreeSet<String> = sets
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|set| set["tags"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .filter(|tag| tag.to_lowercase().contains(&query) && !c.prompts.iter().any(|p| p.eq_ignore_ascii_case(tag)))
                        .map(str::to_string)
                        .collect();
                    ui.horizontal_wrapped(|ui| {
                        for (j, tag) in candidates.iter().take(5).enumerate() {
                            if button(ui, format!("smartSort:tagSuggestion:{i}:{j}"), tag).clicked() {
                                c.prompts.push(tag.clone());
                                s.inputs[i].clear();
                                s.dirty = !s.rows.is_empty();
                            }
                        }
                    });
                }
                let enter = (r.has_focus() || r.lost_focus()) && ui.input(|x| x.key_pressed(egui::Key::Enter));
                if s.inputs[i].contains(',') || enter {
                    let input = std::mem::take(&mut s.inputs[i]);
                    for tag in input.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                        if !c.prompts.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                            c.prompts.push(tag.into());
                        }
                    }
                    s.dirty = !s.rows.is_empty();
                }
                if s.tag_set_folder == Some(i) {
                    ui.horizontal(|ui| {
                        let r = text(ui, "smartSort:tagSetName", &mut s.tag_set_name, 200.0);
                        if s.tag_set_focus {
                            r.request_focus();
                            r.scroll_to_me(None);
                            s.tag_set_focus = false;
                        }
                        if button(ui, "smartSort:confirmTagSet", "Save").clicked() {
                            match app.session.execute("smartSort.saveTagSet", &json!({"name":s.tag_set_name,"tags":c.prompts})) {
                                Ok(_) => s.tag_set_folder = None,
                                Err(e) => s.error = e.to_string(),
                            }
                        }
                        if button(ui, "smartSort:cancelTagSet", "Cancel").clicked() {
                            s.tag_set_folder = None;
                        }
                    });
                }
            });
        });
    }
    if let Some(i) = delete {
        let name = s.preset.categories.remove(i).name;
        s.inputs.remove(i);
        for cats in s.overrides.values_mut() {
            cats.retain(|c| *c != name);
        }
        s.preset.folders.retain(|f| f.custom || f.unsorted || !f.tags.contains(&name));
        for folder in s.preset.folders.iter_mut().filter(|f| f.custom) {
            folder.tags.retain(|t| *t != name);
        }
        s.dirty = !s.rows.is_empty();
    }
    if button(ui, "smartSort:addFolder", "+ Add Folder").clicked() {
        let mut n = s.preset.categories.len() + 1;
        while s.preset.categories.iter().any(|c| c.name == format!("Folder {n}")) {
            n += 1;
        }
        s.preset.categories.push(Category { name: format!("Folder {n}"), ..Default::default() });
        if !s.preset.folders.is_empty()
            && let Some(folder) = plan::keyword_folders(&s.preset).pop()
        {
            let at = s.preset.folders.iter().position(|f| f.unsorted).unwrap_or(s.preset.folders.len());
            s.preset.folders.insert(at, folder);
        }
        s.dirty = !s.rows.is_empty();
    }
    if s.manage_sets {
        manage_sets(app, ui, s, &sets);
    }
    ui.horizontal(|ui| {
        ui.label(tr("Sorting"));
        sensitivity(ui, s);
        if check(ui, "smartSort:multi", &mut s.preset.multi, "A photo can go in more than one folder").changed() {
            s.dirty = !s.rows.is_empty();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Add keywords under:"));
        let old = s.preset.keyword_parent.clone();
        if text(ui, "smartSort:keywordParent", &mut s.preset.keyword_parent, 240.0).changed() {
            for folder in &mut s.preset.folders {
                rename_rule_keyword(
                    &mut folder.rules,
                    &lightcraft_catalog::keywords::clean(&old),
                    &lightcraft_catalog::keywords::clean(&s.preset.keyword_parent),
                );
            }
        }
    });
    if check(ui, "smartSort:includeRejected", &mut s.include_rejected, "Include rejected photos").changed() {
        s.dirty = !s.rows.is_empty();
    }
    let status = app.session.execute("smartSort.status", &json!({})).unwrap_or_default();
    let installed = status["tagger"]["installed"] == true;
    if !installed {
        ui.label(tr("Smart Sort needs a one-time download of 607 MB. It runs on this computer; your photos are never uploaded."));
        if app.services.download_models.is_some() {
            let download = app.services.model_download_status.as_ref().map(|f| f("clip-b32-laion")).unwrap_or_default();
            if download.running {
                ui.add(egui::ProgressBar::new(download.done as f32 / download.total.max(1) as f32));
                if button(ui, "smartSort:downloadCancel", "Cancel download").clicked()
                    && let Some(f) = &app.services.download_models
                {
                    let _ = f("clip-b32-laion", true);
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            } else if button(ui, "smartSort:download", "Download").clicked()
                && let Some(f) = &app.services.download_models
                && let Err(e) = f("clip-b32-laion", false)
            {
                s.error = e;
            }
            if let Some(e) = download.error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
            }
        } else {
            ui.label(tr("Download the Smart Sort model in Compositing ▸ Local AI"));
        }
        ui.hyperlink_to("MIT", "https://huggingface.co/laion/CLIP-ViT-B-32-laion2B-s34B-b79K");
    }
    if let Some(task) = &app.smart_sort {
        let status = task.status();
        let current = status["current"]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok())
            .and_then(|id| app.session.catalog.photo(PhotoId(id)))
            .map(|p| p.file_name.clone())
            .unwrap_or_else(|| tr("Loading model…").to_string());
        ui.add(egui::ProgressBar::new(status["done"].as_u64().unwrap_or(0) as f32 / task.total.max(1) as f32).text(crate::i18n::tr_format!(
            "Analysing {done} of {total} — {file}",
            done = status["done"],
            total = task.total,
            file = current
        )));
        ui.label(crate::i18n::tr_format!("{n} already analysed", n = task.skipped));
    }
    if s.videos > 0 {
        ui.label(crate::i18n::tr_format!("{n} videos skipped", n = s.videos));
    }
    if !s.failed.is_empty() {
        ui.collapsing(tr("Couldn't read"), |ui| {
            for (id, e) in &s.failed {
                let name = app.session.catalog.photo(*id).map(|p| p.file_name.clone()).unwrap_or_default();
                ui.label(format!("{name}: {e}"));
            }
        });
    }
}
fn manage_sets(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog, sets: &Value) {
    ui.group(|ui| {
        ui.label(tr("Manage Tag Sets…"));
        for set in sets.as_array().into_iter().flatten().filter(|s| s["builtin"] == false) {
            let name = set["name"].as_str().unwrap_or_default();
            ui.horizontal(|ui| {
                let value = s.rename_sets.entry(name.into()).or_insert_with(|| name.into());
                text(ui, format!("smartSort:renameTagSet:{name}"), value, 200.0);
                if button(ui, format!("smartSort:renameTagSetSave:{name}"), "Rename").clicked()
                    && let Err(e) = app.session.execute("smartSort.renameTagSet", &json!({"name":name,"to":value}))
                {
                    s.error = e.to_string();
                }
                if button(ui, format!("smartSort:deleteTagSet:{name}"), "Delete").clicked()
                    && let Err(e) = app.session.execute("smartSort.deleteTagSet", &json!({"name":name}))
                {
                    s.error = e.to_string();
                }
            });
        }
        if button(ui, "smartSort:closeTagSets", "Close").clicked() {
            s.manage_sets = false;
        }
    });
}
fn sensitivity(ui: &mut Ui, s: &mut SmartSortDialog) {
    let mut value = match s.preset.sensitivity {
        Sensitivity::Strict => 0,
        Sensitivity::Balanced => 1,
        Sensitivity::Loose => 2,
    };
    let r = ui.add(egui::Slider::new(&mut value, 0..=2).custom_formatter(|v, _| {
        tr(match v as i32 {
            0 => "Strict",
            1 => "Balanced",
            _ => "Loose",
        })
        .to_string()
    }));
    register(ui.ctx(), "smartSort:sensitivity", r.rect);
    if r.changed() {
        s.preset.sensitivity = match value {
            0 => Sensitivity::Strict,
            1 => Sensitivity::Balanced,
            _ => Sensitivity::Loose,
        };
        s.dirty = !s.rows.is_empty();
    }
}

#[derive(Clone)]
struct DragPhotos(Vec<PhotoId>);
fn review(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog) {
    ui.horizontal(|ui| {
        ui.label(tr("Sorting"));
        sensitivity(ui, s);
        check(ui, "smartSort:leastSure", &mut s.least_sure, "Least sure first");
        let r = ui.add_enabled(!s.selected.is_empty() || !s.library_examples.is_empty(), egui::Button::new(tr("+ Folder from Examples…")));
        register(ui.ctx(), "smartSort:folderFromExamples", r.rect);
        if r.clicked() {
            match app.session.execute(
                "smartSort.folderFromExamples",
                &json!({"preset":s.preset,"ids":if s.selected.is_empty() { &s.library_examples } else { &s.selected }}),
            ) {
                Ok(result) => {
                    if let Ok(preset) = serde_json::from_value::<SortPreset>(result["preset"].clone()) {
                        s.preset = preset;
                        if let Some(category) = s.preset.categories.last() {
                            s.review_folder = category.name.clone();
                        }
                        s.undo.clear();
                        s.redo.clear();
                        if let Err(e) = classify(app, s) {
                            s.error = e;
                        }
                    }
                }
                Err(e) => s.error = e.to_string(),
            }
        }
    });
    let names: Vec<_> = s.preset.categories.iter().map(|c| c.name.clone()).chain(["Unsorted".into()]).collect();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(225.0, 400.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            for (index, name) in names.iter().enumerate() {
                let hint = if name == "Unsorted" { "0" } else { smart_sort_keys::folder_hint(index).unwrap_or("") };
                let r = ui.add_sized(
                    [215.0, 28.0],
                    egui::Button::new(format!(
                        "{}  {}  [{}]",
                        if name == "Unsorted" { tr("Unsorted") } else { name.as_str() },
                        s.counts.get(name).copied().unwrap_or(0),
                        hint
                    ))
                    .selected(s.review_folder == *name),
                );
                let r = if hint.is_empty() {
                    r
                } else if name == "Unsorted" {
                    r.on_hover_text(format!("0: {}", tr("Unsorted")))
                } else {
                    r.on_hover_text(format!("{hint}: {}\nAlt+{hint}: {}", tr("Move to"), tr("Also add to")))
                };
                register(ui.ctx(), format!("smartSort:reviewFolder:{name}"), r.rect);
                if r.clicked() {
                    s.review_folder = name.clone();
                    s.focus = None;
                    s.anchor = None;
                }
                if let Some(payload) = r.dnd_release_payload::<DragPhotos>() {
                    s.selected = payload.0.clone();
                    correct(app, ui.ctx(), s, name, false, false);
                }
            }
            if !s.selected.is_empty() {
                ui.separator();
                ui.label(tr("Move to"));
                for name in &names {
                    if button(ui, format!("smartSort:moveTo:{name}"), if name == "Unsorted" { "Unsorted" } else { name }).clicked() {
                        correct(app, ui.ctx(), s, name, false, false);
                    }
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            let mut rows: Vec<_> = s
                .rows
                .iter()
                .filter(|p| if s.review_folder == "Unsorted" { p.row.assigned.is_empty() } else { p.row.assigned.contains(&s.review_folder) })
                .cloned()
                .collect();
            let confidence_folder = s.review_folder.clone();
            let confidence =
                |p: &ReviewPhoto| p.row.scores.get(&confidence_folder).copied().unwrap_or_else(|| p.row.scores.values().copied().fold(0.0, f32::max));
            if s.least_sure {
                rows.sort_by(|a, b| confidence(a).total_cmp(&confidence(b)).then(a.id.cmp(&b.id)));
            } else {
                rows.sort_by_key(|p| {
                    app.session.catalog.photo(p.id).map(|p| p.captured.clone().unwrap_or_else(|| p.imported.clone())).unwrap_or_default()
                });
            }
            if s.preset.bursts.enabled {
                let visible: std::collections::BTreeSet<_> = rows.iter().map(|p| p.id).collect();
                rows.retain(|p| {
                    s.bursts.iter().find(|b| b.photos.contains(&p.id)).is_none_or(|b| {
                        let representative =
                            if visible.contains(&b.best) { b.best } else { b.photos.iter().copied().find(|id| visible.contains(id)).unwrap_or(p.id) };
                        p.id == representative
                    })
                });
            }
            let cols = (ui.available_width() / 148.0).floor().max(1.0) as usize;
            let old_focus = s.focus;
            review_keys(app, ui, s, &rows, cols);
            let mut area = egui::ScrollArea::vertical().id_salt("smart-sort-review-thumbs").max_height(430.0).auto_shrink([false, false]);
            if s.focus != old_focus
                && let Some(index) = rows.iter().position(|p| Some(p.id) == s.focus)
            {
                area = area.vertical_scroll_offset((index / cols) as f32 * 156.0);
            }
            area.show_rows(ui, 156.0, rows.len().div_ceil(cols), |ui, range| {
                egui::Grid::new("smart-sort-review-grid").spacing([10.0, 12.0]).show(ui, |ui| {
                    let start = range.start * cols;
                    let end = (range.end * cols).min(rows.len());
                    for (i, p) in rows.iter().enumerate().take(end).skip(start) {
                        ui.vertical(|ui| {
                            let (rect, r) = ui.allocate_exact_size(egui::vec2(132.0, 106.0), egui::Sense::click_and_drag());
                            register(ui.ctx(), format!("smartSort:thumb:{}", p.id.0), rect);
                            super::grid::request_thumb(app, p.id, 160, 10);
                            let selected = s.selected.contains(&p.id);
                            ui.painter().rect_filled(
                                rect,
                                3.0,
                                if selected { egui::Color32::from_rgb(50, 85, 120) } else { egui::Color32::from_gray(30) },
                            );
                            if let Some(t) = app.renderer.thumb(p.id) {
                                let size = egui::vec2(t.size[0] as f32, t.size[1] as f32);
                                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                                let fit = egui::Rect::from_center_size(rect.center(), size * scale);
                                ui.painter().image(
                                    t.tex.id(),
                                    fit,
                                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                    egui::Color32::WHITE,
                                );
                            }
                            if selected {
                                ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE), egui::StrokeKind::Inside);
                            }
                            if p.row.manual {
                                ui.painter().text(
                                    rect.right_top() + egui::vec2(-5.0, 5.0),
                                    egui::Align2::RIGHT_TOP,
                                    "✓",
                                    egui::FontId::proportional(18.0),
                                    egui::Color32::LIGHT_GREEN,
                                );
                            }
                            if s.preset.bursts.enabled
                                && let Some(burst) = s.bursts.iter().find(|b| b.photos.contains(&p.id) && b.photos.len() > 1)
                            {
                                let badge = egui::Rect::from_min_size(rect.left_top() + egui::vec2(4.0, 4.0), egui::vec2(30.0, 22.0));
                                ui.painter().rect_filled(badge, 5.0, egui::Color32::from_black_alpha(210));
                                ui.painter().text(
                                    badge.center(),
                                    egui::Align2::CENTER_CENTER,
                                    burst.photos.len(),
                                    egui::FontId::proportional(15.0),
                                    egui::Color32::WHITE,
                                );
                                register(ui.ctx(), format!("smartSort:burstBadge:{}", p.id.0), badge);
                            }
                            if r.clicked() || r.secondary_clicked() || r.drag_started() {
                                s.focus = Some(p.id);
                                if ui.input(|i| i.modifiers.shift) && !s.selected.is_empty() {
                                    let from = rows.iter().position(|p| Some(p.id) == s.anchor).unwrap_or(i);
                                    let chosen: Vec<_> = rows[from.min(i)..=from.max(i)].iter().map(|p| p.id).collect();
                                    s.selected = expand_selection(s, &chosen);
                                } else if ui.input(|i| i.modifiers.command) {
                                    if selected {
                                        let members = expand_selection(s, &[p.id]);
                                        s.selected.retain(|id| !members.contains(id));
                                    } else {
                                        s.selected.extend(expand_selection(s, &[p.id]));
                                    }
                                } else if !selected {
                                    s.selected = expand_selection(s, &[p.id]);
                                    s.anchor = Some(p.id);
                                }
                            }
                            r.dnd_set_drag_payload(DragPhotos(s.selected.clone()));
                            r.context_menu(|ui| {
                                for (prefix, label, add) in [("moveTo", "Move to", false), ("alsoAdd", "Also add to", true)] {
                                    let menu = ui.menu_button(tr(label), |ui| {
                                        for name in &names {
                                            if button(ui, format!("smartSort:{prefix}:{name}"), if name == "Unsorted" { "Unsorted" } else { name })
                                                .clicked()
                                            {
                                                correct(app, ui.ctx(), s, name, add, false);
                                                ui.close();
                                            }
                                        }
                                    });
                                    register(ui.ctx(), format!("smartSort:context:{prefix}"), menu.response.rect);
                                }
                                if button(ui, "smartSort:remove", "Remove from folder").clicked() {
                                    let folder = s.review_folder.clone();
                                    correct(app, ui.ctx(), s, &folder, false, true);
                                    ui.close();
                                }
                            });
                            ui.add(egui::ProgressBar::new(confidence(p)).desired_width(132.0).show_percentage());
                            let name = app.session.catalog.photo(p.id).map(|p| p.file_name.clone()).unwrap_or_default();
                            ui.add_sized([132.0, 18.0], egui::Label::new(name).truncate());
                        });
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
        });
    });
}
fn export(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog) {
    for (id, label, value) in [
        ("eventName", "Event name", &mut s.preset.event_name),
        ("folderPattern", "Destination folder pattern", &mut s.preset.folder_pattern),
        ("filePattern", "File name pattern (optional)", &mut s.preset.file_pattern),
    ] {
        ui.horizontal(|ui| {
            ui.label(tr(label));
            text(ui, format!("smartSort:{id}"), value, 450.0);
        });
    }
    ui.label(tr("Folder tokens: {event}, {folder}, {person}, {session}, {date}, {camera}. File names also accept {original} and {seq:4}."));
    let sessions = sessions::split_sessions(&app.session.catalog, &ids(app, s), &s.preset.sessions);
    if s.preset.sessions.enabled {
        check(ui, "smartSort:sessionFolders", &mut s.preset.sessions.export_folders, "Export sessions as folders");
    }
    if s.preset.bursts.enabled {
        ui.horizontal(|ui| {
            for (mode, label, id) in [(BurstExport::BestOnly, "Best of each burst", "bestBursts"), (BurstExport::All, "All photos", "allBursts")] {
                let r = ui.radio_value(&mut s.preset.bursts.export, mode, tr(label));
                register(ui.ctx(), format!("smartSort:{id}"), r.rect);
            }
        });
        check(ui, "smartSort:libraryStacks", &mut s.preset.bursts.also_stack_in_library, "Also stack in the Library");
    }
    ui.horizontal(|ui| {
        ui.label(tr("Destination root"));
        text(ui, "smartSort:dest", &mut s.dest, 560.0);
        if button(ui, "smartSort:chooseDest", "Choose…").clicked()
            && let Some(pick) = &mut app.services.pick_folder
            && let Some(dir) = pick()
        {
            s.dest = dir;
        }
    });
    ui.horizontal(|ui| {
        ui.heading(tr("Folders"));
        text(ui, "smartSort:exportPresetName", &mut s.save_name, 180.0);
        if button(ui, "smartSort:saveLayoutPreset", "Save Preset…").clicked() {
            let mut preset = s.preset.clone();
            preset.name = s.save_name.trim().into();
            if let Err(e) = app.session.execute("smartSort.savePreset", &json!({"preset":preset})) {
                s.error = e.to_string();
            }
        }
    });
    let planned = planned_folders(app, s).unwrap_or_default();
    let mut swap = None;
    for i in 0..s.preset.folders.len() {
        let count_index = if s.preset.folders[i].unsorted {
            planned.len().saturating_sub(1)
        } else {
            s.preset.folders[..i].iter().filter(|f| f.enabled && !f.unsorted).count()
        };
        ui.push_id(("export-folder", i), |ui| {
            let folder = &mut s.preset.folders[i];
            let custom = folder.custom;
            ui.horizontal(|ui| {
                let r = check(ui, format!("smartSort:folders:{}:{i}", if custom { "custom" } else { "default" }), &mut folder.enabled, "");
                register(ui.ctx(), format!("smartSort:exportFolder:{i}"), r.rect);
                text(ui, format!("smartSort:folders:name:{i}"), &mut folder.name, 230.0);
                let count = if folder.enabled { planned.get(count_index).map_or(0, |p| p.ids.len()) } else { 0 };
                ui.label(crate::i18n::tr_format!("{n} photos", n = count));
                if button(ui, format!("smartSort:folders:up:{i}"), "↑").clicked() && i > 0 {
                    swap = Some((i, i - 1));
                }
                if button(ui, format!("smartSort:folders:down:{i}"), "↓").clicked() {
                    swap = Some((i, i + 1));
                }
                if !folder.unsorted && button(ui, format!("smartSort:folders:rules:{i}"), "Advanced Rules…").clicked() {
                    s.rules_open = if s.rules_open == Some(i) { None } else { Some(i) };
                }
                if s.preset.sessions.enabled {
                    ui.label(tr("Only in session"));
                    let label = folder
                        .session
                        .as_ref()
                        .map(|key| sessions.sessions.iter().find(|s| &s.start == key).map_or(tr("No time"), |s| s.name.as_str()))
                        .unwrap_or(tr("All sessions"));
                    let response =
                        egui::ComboBox::from_id_salt(("session-filter", i)).width(140.0).truncate().selected_text(label).show_ui(ui, |ui| {
                            let r = ui.selectable_value(&mut folder.session, None, tr("All sessions"));
                            register(ui.ctx(), format!("smartSort:folderSession:{i}:all"), r.rect);
                            for (j, session) in sessions.sessions.iter().enumerate() {
                                let r = ui.selectable_value(&mut folder.session, Some(session.start.clone()), &session.name);
                                register(ui.ctx(), format!("smartSort:folderSession:{i}:{j}"), r.rect);
                            }
                            let r = ui.selectable_value(&mut folder.session, Some(sessions::NO_TIME.into()), tr("No time"));
                            register(ui.ctx(), format!("smartSort:folderSession:{i}:noTime"), r.rect);
                        });
                    register(ui.ctx(), format!("smartSort:folderSession:{i}"), response.response.rect);
                }
            });
            if custom {
                ui.horizontal(|ui| {
                    ui.label(tr("Include photos tagged:"));
                    let response = ui.menu_button(tr("Tags"), |ui| {
                        for (j, c) in s.preset.categories.iter().enumerate() {
                            let mut on = folder.tags.contains(&c.name);
                            if check(ui, format!("smartSort:folders:tag:{i}:{j}"), &mut on, &c.name).changed() {
                                if on {
                                    folder.tags.push(c.name.clone());
                                } else {
                                    folder.tags.retain(|t| t != &c.name);
                                }
                                folder.rules = plan::tag_rules(&s.preset.keyword_parent, &folder.tags, folder.combine_all);
                            }
                        }
                    });
                    register(ui.ctx(), format!("smartSort:folders:tagPicker:{i}"), response.response.rect);
                    if check(ui, format!("smartSort:folders:combine:{i}"), &mut folder.combine_all, "All selected tags (AND)").changed() {
                        folder.rules = plan::tag_rules(&s.preset.keyword_parent, &folder.tags, folder.combine_all);
                    }
                });
                let mut remove = None;
                ui.horizontal_wrapped(|ui| {
                    for (j, tag) in folder.tags.iter().enumerate() {
                        egui::Frame::NONE
                            .fill(ui.visuals().widgets.inactive.bg_fill)
                            .corner_radius(7)
                            .inner_margin(egui::Margin::symmetric(5, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(tag);
                                    if button(ui, format!("smartSort:folders:removeTag:{i}:{j}"), "×").clicked() {
                                        remove = Some(j);
                                    }
                                });
                            });
                    }
                });
                if let Some(j) = remove {
                    folder.tags.remove(j);
                    folder.rules = plan::tag_rules(&s.preset.keyword_parent, &folder.tags, folder.combine_all);
                }
            } else if folder.unsorted {
                ui.label(tr("Photos matching no enabled folder"));
            } else {
                ui.label(folder.tags.join(", "));
            }
            if s.rules_open == Some(i) {
                super::rules_editor::edit(ui, &mut folder.rules, &format!("smart-sort-rules-{i}"), 0);
            }
        });
    }
    if let Some((a, b)) = swap
        && b < s.preset.folders.len()
    {
        s.preset.folders.swap(a, b);
        s.rules_open = None;
    }
    if button(ui, "smartSort:folders:addCustom", "+ Custom Folder").clicked() {
        s.preset.folders.push(FolderDef {
            name: format!("Custom {}", s.preset.folders.iter().filter(|f| f.custom).count() + 1),
            custom: true,
            rules: plan::tag_rules(&s.preset.keyword_parent, &[], false),
            ..Default::default()
        });
    }
    ui.separator();
    check(ui, "smartSort:firstMatch", &mut s.preset.first_match, "First matching folder only");
    ui.label(tr(if s.preset.first_match { "Only the first matching folder receives a copy" } else { "Copy into every matching folder" }));
    ui.horizontal(|ui| {
        ui.label(tr("Export as:"));
        let presets = app.session.all_export_presets();
        let response = egui::ComboBox::from_id_salt("smart-sort-export-preset")
            .selected_text(tr(if s.export_preset == "Original + Settings" { "Copy originals" } else { &s.export_preset }))
            .show_ui(ui, |ui| {
                for (i, (p, _)) in presets.iter().enumerate() {
                    let r =
                        ui.selectable_label(s.export_preset == p.name, tr(if p.name == "Original + Settings" { "Copy originals" } else { &p.name }));
                    register(ui.ctx(), format!("smartSort:exportPreset:{i}"), r.rect);
                    if r.clicked() {
                        s.export_preset = p.name.clone();
                        s.export_params = p.params.clone();
                    }
                }
            });
        register(ui.ctx(), "smartSort:exportPreset", response.response.rect);
        if button(ui, "smartSort:editSettings", "Edit Settings…").clicked() {
            let mut dlg = super::export_dialog::new_dialog(s.dest.clone());
            let params = export_params(app, s);
            super::export_dialog::apply_export_params(&mut dlg, &params, false);
            s.settings = Some(Box::new(dlg));
        }
    });
    if let Some(mut settings) = s.settings.take() {
        super::export_dialog::body(app, ui, &mut settings);
        if button(ui, "smartSort:settingsDone", "Done").clicked() {
            s.export_params = super::export_dialog::export_dialog_params(&settings);
        } else {
            s.settings = Some(settings);
        }
    }
    check(ui, "smartSort:albums", &mut s.albums, "Also create a smart album for each folder");
    let resolved = plan::resolve_tokens(&app.session.catalog, &planned, &s.preset, &sessions).unwrap_or_default();
    let files: usize = planned.iter().map(|p| p.ids.len()).sum();
    let photos: std::collections::BTreeSet<_> = planned.iter().flat_map(|p| p.ids.iter()).collect();
    ui.label(crate::i18n::tr_format!(
        "{files} files into {folders} folders ({photos} photos)",
        files = files,
        folders = resolved.len(),
        photos = photos.len()
    ));
}
fn export_params(app: &LightcraftApp, s: &SmartSortDialog) -> Value {
    if s.export_params.is_object() {
        s.export_params.clone()
    } else {
        app.session.export_params(&json!({"preset":s.export_preset})).unwrap_or_default()
    }
}
fn start_export(app: &mut LightcraftApp, s: &SmartSortDialog) -> Result<(), String> {
    if app.export.is_some() {
        return Err(tr("An export is already running").to_string());
    }
    let folders = planned_folders(app, s)?;
    let params = export_params(app, s);
    let mut opts = lightcraft_engine::export::ExportOptions::from_json(&params);
    opts.same_folder = false;
    opts.subfolder.clear();
    let sessions = sessions::split_sessions(&app.session.catalog, &ids(app, s), &s.preset.sessions);
    let items = plan::prepare_preset(&mut app.session, &folders, &opts, &s.dest, &s.preset, &sessions)?;
    if items.is_empty() {
        return Err(tr("No photos match the enabled folders").to_string());
    }
    save_last(app, s)?;
    if s.preset.bursts.enabled && s.preset.bursts.also_stack_in_library {
        let ids = ids(app, s);
        app.session.execute("smartSort.stackBursts", &json!({"preset":s.preset,"ids":ids})).map_err(|e| e.to_string())?;
    }
    if s.albums {
        let photos: Vec<_> = ids(app, s)
            .into_iter()
            .filter(|id| app.session.catalog.photo(*id).is_some_and(|p| p.kind != lightcraft_catalog::MediaKind::Video))
            .collect();
        if s.preset.sessions.enabled || s.preset.bursts.enabled {
            for folder in &folders {
                app.session
                    .execute(
                        "album.createSmart",
                        &json!({"name":folder.name,"rules":{"ruleSet":lightcraft_catalog::RuleSet::default(),"only":folder.ids}}),
                    )
                    .map_err(|e| e.to_string())?;
            }
        } else {
            let mut previous = Vec::new();
            let defs = s
                .preset
                .folders
                .iter()
                .filter(|f| f.enabled && !f.unsorted)
                .chain(s.preset.folders.iter().filter(|f| f.enabled && f.unsorted).take(1));
            for (f, planned) in defs.zip(&folders) {
                // Unsorted/first-match albums include exclusions, matching the complete plan.
                let mut rules = f.rules.clone();
                if f.unsorted {
                    rules = lightcraft_catalog::RuleSet {
                        mode: lightcraft_catalog::Match::None,
                        rules: s
                            .preset
                            .folders
                            .iter()
                            .filter(|f| f.enabled && !f.unsorted)
                            .map(|f| lightcraft_catalog::Rule::Group { group: f.rules.clone() })
                            .collect(),
                    };
                }
                if s.preset.first_match && !f.unsorted && !previous.is_empty() {
                    rules = lightcraft_catalog::RuleSet {
                        rules: vec![
                            lightcraft_catalog::Rule::Group { group: rules },
                            lightcraft_catalog::Rule::Group {
                                group: lightcraft_catalog::RuleSet { mode: lightcraft_catalog::Match::None, rules: previous.clone() },
                            },
                        ],
                        ..Default::default()
                    };
                }
                app.session
                    .execute("album.createSmart", &json!({"name":planned.name,"rules":{"ruleSet":rules,"only":photos}}))
                    .map_err(|e| e.to_string())?;
                if !f.unsorted {
                    previous.push(lightcraft_catalog::Rule::Group { group: f.rules.clone() });
                }
            }
        }
    }
    let to = lightcraft_engine::export::Destination { dir: s.dest.clone(), exact: None };
    if app.services.write_shared.is_some() {
        crate::export_task::start(app, items, opts, to, crate::control::AfterExport::from_params(&json!({"afterExport":"showInFolder"})))?;
    } else {
        let writer = app.services.write.as_mut().ok_or_else(|| tr("No export writer is available").to_string())?;
        let files = lightcraft_engine::export::run_batch(items, &opts, &to, writer, &|p| std::path::Path::new(p).exists(), false, &mut |_, _| true)?;
        app.last_export_result = Some(json!({"files":files,"cancelled":false}));
    }
    Ok(())
}

fn rename_rule_keyword(rules: &mut lightcraft_catalog::RuleSet, old: &str, new: &str) {
    for rule in &mut rules.rules {
        match rule {
            lightcraft_catalog::Rule::Group { group } => rename_rule_keyword(group, old, new),
            lightcraft_catalog::Rule::Field { field, value, .. } if field == "keywords" => {
                if let Some(keyword) = value.as_str() {
                    if keyword == old {
                        *value = json!(new);
                    } else if let Some(tail) = keyword.strip_prefix(&format!("{old}|")) {
                        *value = json!(format!("{new}|{tail}"));
                    }
                }
            }
            _ => {}
        }
    }
}

/// The control protocol's primary action follows the same three steps as the visible buttons.
pub fn confirm(app: &mut LightcraftApp, state: &SmartSortDialog) -> Result<Value, String> {
    let mut state = state.clone();
    let exporting = state.step == 2;
    let action = match state.step {
        0 => state.preset.validate().and_then(|()| crate::smart_sort_task::start(app, &ids(app, &state))),
        1 => advance(app, &mut state),
        _ => start_export(app, &state),
    };
    if action.is_ok() && exporting {
        app.ui.dialog = None;
        app.session.smart.tagger = None;
    } else {
        if let Err(e) = &action {
            state.error = e.clone();
        }
        app.ui.dialog = Some(Dialog::SmartSort { state: Box::new(state) });
    }
    action.map(|()| json!({"background":app.smart_sort.is_some()||app.export.is_some()}))
}

fn snapshot(s: &SmartSortDialog) -> ReviewSnapshot {
    ReviewSnapshot { overrides: s.overrides.clone(), categories: s.preset.categories.clone() }
}
fn refresh_bursts(app: &mut LightcraftApp, s: &mut SmartSortDialog) -> Result<(), String> {
    s.bursts = if s.preset.bursts.enabled && !s.rows.is_empty() {
        let ids = s.rows.iter().map(|p| p.id).collect::<Vec<_>>();
        let result = app
            .session
            .execute("smartSort.bursts", &json!({"preset":s.preset,"ids":ids,"includeRejected":s.include_rejected}))
            .map_err(|e| e.to_string())?;
        serde_json::from_value(result).map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    Ok(())
}
fn session_options(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog) {
    ui.horizontal(|ui| {
        check(ui, "smartSort:sessions", &mut s.preset.sessions.enabled, "Split into sessions when there is a gap of more than");
        let r = ui.add(egui::DragValue::new(&mut s.preset.sessions.gap_minutes).range(1..=1440).suffix(tr(" minutes")));
        register(ui.ctx(), "smartSort:sessionGap", r.rect);
    });
    if s.preset.sessions.enabled {
        let sessions = sessions::split_sessions(&app.session.catalog, &ids(app, s), &s.preset.sessions);
        for (index, session) in sessions.sessions.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut name = session.name.clone();
                if text(ui, format!("smartSort:session:{index}"), &mut name, 210.0).changed() {
                    s.preset.sessions.names.insert(session.start.clone(), name);
                }
                ui.label(crate::i18n::tr_format!(
                    "{start}–{end} · {n} photos",
                    start = session.start.get(11..16).unwrap_or(&session.start),
                    end = session.end.get(11..16).unwrap_or(&session.end),
                    n = session.photos.len()
                ));
            });
        }
        if !sessions.no_time.is_empty() {
            ui.label(format!("{} · {} {}", tr("No time"), sessions.no_time.len(), tr("photos")));
        }
    }
}
fn burst_options(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog) {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= check(ui, "smartSort:bursts", &mut s.preset.bursts.enabled, "Group bursts and near-duplicates").changed();
        let r = egui::ComboBox::from_id_salt("burst-strictness")
            .selected_text(tr(match s.preset.bursts.strictness {
                BurstStrictness::Strict => "Strict",
                BurstStrictness::Normal => "Normal",
                BurstStrictness::Loose => "Loose",
            }))
            .show_ui(ui, |ui| {
                for (value, label) in [(BurstStrictness::Strict, "Strict"), (BurstStrictness::Normal, "Normal"), (BurstStrictness::Loose, "Loose")] {
                    let r = ui.selectable_value(&mut s.preset.bursts.strictness, value, tr(label));
                    register(ui.ctx(), format!("smartSort:burstStrictness:{label}"), r.rect);
                    changed |= r.changed();
                }
            });
        register(ui.ctx(), "smartSort:burstStrictness", r.response.rect);
    });
    if changed && let Err(e) = refresh_bursts(app, s) {
        s.error = e;
    }
}
fn planned_folders(app: &mut LightcraftApp, s: &SmartSortDialog) -> Result<Vec<plan::Folder>, String> {
    let ids = ids(app, s);
    let value = app
        .session
        .execute("smartSort.plan", &json!({"sortPreset":s.preset,"ids":ids,"includeRejected":s.include_rejected}))
        .map_err(|e| e.to_string())?;
    serde_json::from_value(value["folders"].clone()).map_err(|e| e.to_string())
}
fn expand_selection(s: &SmartSortDialog, chosen: &[PhotoId]) -> Vec<PhotoId> {
    let mut result = Vec::new();
    for id in chosen {
        let members = s.preset.bursts.enabled.then(|| s.bursts.iter().find(|b| b.photos.contains(id))).flatten();
        let members = members.map_or_else(|| vec![*id], |b| b.photos.clone());
        for member in members {
            if !result.contains(&member) {
                result.push(member);
            }
        }
    }
    result
}
fn review_keys(app: &mut LightcraftApp, ui: &mut Ui, s: &mut SmartSortDialog, rows: &[ReviewPhoto], cols: usize) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let events = ui.input(|i| i.events.clone());
    for event in events {
        let egui::Event::Key { key, modifiers, pressed: true, .. } = event else { continue };
        let Some(action) = smart_sort_keys::review_action(key, modifiers) else { continue };
        ui.input_mut(|i| {
            i.consume_key(modifiers, key);
        });
        let focus = rows.iter().position(|p| Some(p.id) == s.focus).unwrap_or(0);
        let anchor = rows.iter().position(|p| Some(p.id) == s.anchor).unwrap_or(focus);
        match action {
            ReviewAction::Move(dx, dy, extend) => {
                let (focus, anchor, range) = smart_sort_keys::apply_move(focus, anchor, rows.len(), cols, dx, dy, extend);
                if !rows.is_empty() {
                    s.focus = Some(rows[focus].id);
                    s.anchor = Some(rows[anchor].id);
                    s.selected = expand_selection(s, &rows[range].iter().map(|p| p.id).collect::<Vec<_>>());
                }
            }
            ReviewAction::Home(extend) | ReviewAction::End(extend) => {
                if !rows.is_empty() {
                    let end = if matches!(action, ReviewAction::Home(_)) { 0 } else { rows.len() - 1 };
                    let from = if extend { anchor } else { end };
                    s.focus = Some(rows[end].id);
                    s.anchor = Some(rows[from].id);
                    s.selected = expand_selection(s, &rows[from.min(end)..=from.max(end)].iter().map(|p| p.id).collect::<Vec<_>>());
                }
            }
            ReviewAction::SelectAll => s.selected = expand_selection(s, &rows.iter().map(|p| p.id).collect::<Vec<_>>()),
            ReviewAction::MoveTo(index) | ReviewAction::AlsoAdd(index) => {
                if let Some(name) = s.preset.categories.get(index).map(|c| c.name.clone()) {
                    correct(app, ui.ctx(), s, &name, matches!(action, ReviewAction::AlsoAdd(_)), false);
                }
            }
            ReviewAction::ToUnsorted => correct(app, ui.ctx(), s, "Unsorted", false, false),
            ReviewAction::RemoveFromFolder => {
                let folder = s.review_folder.clone();
                correct(app, ui.ctx(), s, &folder, false, true);
            }
            ReviewAction::Undo | ReviewAction::Redo => {
                if let Err(e) = restore_review(app, s, matches!(action, ReviewAction::Redo)) {
                    s.error = e;
                }
            }
            ReviewAction::ClearSelection => {
                s.selected.clear();
                s.focus = None;
                s.anchor = None;
            }
        }
    }
}

fn restore_review(app: &mut LightcraftApp, s: &mut SmartSortDialog, redo: bool) -> Result<(), String> {
    let previous = if redo { s.redo.pop() } else { s.undo.pop() };
    if let Some(previous) = previous {
        let current = snapshot(s);
        if redo {
            s.undo.push(current);
        } else {
            s.redo.push(current);
        }
        s.overrides = previous.overrides;
        s.preset.categories = previous.categories;
        classify(app, s)?;
    }
    Ok(())
}
pub fn review_history_enabled(app: &LightcraftApp, id: &str) -> Option<bool> {
    let Some(Dialog::SmartSort { state }) = &app.ui.dialog else { return None };
    if state.step != 1 {
        return None;
    }
    match id {
        "edit.undo" => Some(!state.undo.is_empty()),
        "edit.redo" => Some(!state.redo.is_empty()),
        _ => None,
    }
}
pub fn review_history_command(app: &mut LightcraftApp, id: &str) -> Option<Result<Value, String>> {
    review_history_enabled(app, id)?;
    let Some(Dialog::SmartSort { mut state }) = app.ui.dialog.clone() else { return None };
    let result = restore_review(app, &mut state, id == "edit.redo").map(|()| Value::Null);
    app.ui.dialog = Some(Dialog::SmartSort { state });
    Some(result)
}
