//! Library/Develop Preferences pages (⌘,), hosted in the editor shell; standalone fallback: General, Import, Performance, Interface, Attributions.
//!
//! Changes apply immediately (no OK/Cancel). Where they are stored:
//! - **app settings** ([`crate::state::AppSettings`]: startup view, delete confirmation, GPU,
//!   preview size, filmstrip/grid badges, last library) and Auto Advance live in the UI state,
//!   saved by the host in the shared application Preferences store;
//! - **library settings** (import defaults, XMP sidecars, thumbnail cache size) go through the
//!   `library.preferences` / `library.xmpPreferences` commands into the library's `prefs.json`,
//!   so they travel with the library.

use egui::RichText;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::state::{GridBadges, PREVIEW_EDGES, StartupView};
use crate::theme::Tokens;
use crate::widgets::register;

/// Hosted application preferences are edited as a draft by the one Preferences shell.
/// Catalog actions (import defaults, XMP, cache/preview maintenance) remain immediate and travel
/// with their catalog; they are not another application settings store.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PreferencesDraft {
    settings: crate::state::AppSettings,
    auto_advance: bool,
    show_filenames: bool,
    navigator: bool,
    info_overlay: crate::state::InfoOverlay,
}
impl Default for PreferencesDraft {
    fn default() -> Self {
        Self::from_ui(&crate::UiState::default())
    }
}
impl PreferencesDraft {
    pub fn from_ui(ui: &crate::UiState) -> Self {
        Self {
            settings: ui.settings.clone(),
            auto_advance: ui.auto_advance,
            show_filenames: ui.show_filenames,
            navigator: ui.navigator,
            info_overlay: ui.info_overlay,
        }
    }
    pub fn install(&self, ui: &mut crate::UiState) {
        // Opening a catalog is an immediate action, not an editable preference field.
        let path = ui.settings.library_path.clone();
        ui.settings = self.settings.clone();
        ui.settings.library_path = path;
        ui.auto_advance = self.auto_advance;
        ui.show_filenames = self.show_filenames;
        ui.navigator = self.navigator;
        ui.info_overlay = self.info_overlay;
    }
}

/// Render a Library/Develop page inside the host's Preferences, reusing the standalone controls
/// and their legacy automation ids. Application edits commit on Apply/OK; catalog actions are
/// labelled as immediate. The host supplies the current theme tokens through the context.
pub fn shared_page(app: &mut LightcraftApp, ui: &mut egui::Ui, tab: &str, draft: &mut Value) {
    let Ok(working) = serde_json::from_value::<PreferencesDraft>(draft.clone()) else { return };
    let original = PreferencesDraft::from_ui(&app.ui);
    working.install(&mut app.ui);
    let t = Tokens::get(ui.ctx());
    match tab {
        "import" => {
            hint(ui, &t, "Catalog settings and maintenance actions apply immediately.");
            import_tab(app, ui, &t);
        }
        "performance" => {
            hint(ui, &t, "Catalog settings and maintenance actions apply immediately.");
            performance_tab(app, ui, &t);
        }
        "interface" => interface_tab(app, ui, &t),
        _ => general_tab(app, ui, &t),
    }
    *draft = serde_json::to_value(PreferencesDraft::from_ui(&app.ui)).unwrap_or_default();
    original.install(&mut app.ui);
}

/// (id, label) of the tabs, in order.
pub const TABS: &[(&str, &str)] =
    &[("general", "General"), ("import", "Import"), ("performance", "Performance"), ("interface", "Interface"), ("attributions", "Attributions")];

/// Thumbnail cache sizes offered (MB).
const CACHE_SIZES: [u32; 5] = [512, 1024, 2048, 4096, 8192];

/// The dialog body for `tab` (the tab bar switches `tab`).
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, tab: &mut String) {
    let t = Tokens::get(ui.ctx());
    ui.set_width(crate::theme::PREFERENCES_BODY_WIDTH);
    ui.set_min_height(330.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (id, label) in TABS {
            if crate::widgets::text_button(ui, &format!("settingsTab-{id}"), label, tab == id).clicked() {
                *tab = id.to_string();
            }
        }
        if app.hosted_menus && crate::widgets::text_button(ui, "settingsAI", "AI…", false).clicked() {
            app.open_shared_ai = true;
        }
    });
    ui.separator();
    ui.add_space(2.0);
    let height = ui.ctx().content_rect().height() - t.top_bar_h - t.bottom_bar_h - 3.0 * ui.spacing().interact_size.y;
    egui::ScrollArea::vertical().max_height(height).id_salt(("library-preferences-body", tab.as_str())).show(ui, |ui| match tab.as_str() {
        "import" => import_tab(app, ui, &t),
        "performance" => performance_tab(app, ui, &t),
        "interface" => interface_tab(app, ui, &t),
        "attributions" => crate::panels::attributions::body(app, ui, crate::panels::attributions::SETTINGS),
        _ => general_tab(app, ui, &t),
    });
}

fn heading(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(li_i18n::library::t(text)).font(t.semibold(12.5)).color(t.text));
}

fn row<R>(ui: &mut egui::Ui, t: &Tokens, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width().min(crate::theme::PREFERENCES_BODY_WIDTH);
    ui.allocate_ui_with_layout(egui::vec2(width, ui.spacing().interact_size.y), egui::Layout::top_down(egui::Align::Min), |ui| {
        if !label.is_empty() {
            ui.label(RichText::new(li_i18n::library::t(label)).color(t.text_label));
        }
        add(ui)
    })
    .inner
}

fn hint(ui: &mut egui::Ui, t: &Tokens, text: &str) {
    ui.label(RichText::new(li_i18n::library::t(text)).size(11.0).color(t.text_dim));
}

/// A checkbox addressable as `check:{id}`; true when toggled.
fn check(ui: &mut egui::Ui, id: &str, value: &mut bool, label: &str) -> bool {
    let r = crate::widgets::checkbox(ui, id, value, label);
    r.changed()
}

/// Mutually exclusive buttons (`button:{id}-{index}`).
fn choices<V: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, options: &[(V, &str)], value: &mut V) -> bool {
    let t = Tokens::get(ui.ctx());
    let min_width = options
        .iter()
        .map(|(_, label)| {
            ui.painter().layout_no_wrap(li_i18n::library::t(label).into(), t.semibold(crate::theme::BUTTON_FONT_SIZE), t.text).size().x
                + 2.0 * ui.spacing().button_padding.x
        })
        .fold(0.0, f32::max);
    let per_row = ((ui.available_width() / min_width.max(1.0)).floor() as usize).clamp(1, options.len().max(1));
    let keys: Vec<_> = (0..options.len()).map(|index| index.to_string()).collect();
    let items: Vec<_> = options.iter().zip(&keys).map(|((_, label), key)| (*label, key.as_str())).collect();
    if let Some(index) = crate::widgets::segmented(ui, id, &items, options.iter().position(|(v, _)| v == value), per_row)
        && *value != options[index].0
    {
        *value = options[index].0;
        return true;
    }
    false
}

// ------------------------------------------------------------------------------------- General

fn general_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    if !app.hosted_menus {
        row(ui, t, li_i18n::library::t("Language"), |ui| {
            let languages: Vec<_> = li_i18n::Lang::ALL.iter().map(|language| (*language, language.name())).collect();
            choices(ui, "settingsLanguage", &languages, &mut app.ui.language);
            li_i18n::set_current(app.ui.language);
        });
    }
    heading(ui, t, li_i18n::library::t("Library"));
    let location = match &app.session.library {
        Some(l) => l.dir.display().to_string(),
        None => li_i18n::library::t("In memory — nothing is saved").to_string(),
    };
    row(ui, t, li_i18n::library::t("Location"), |ui| {
        ui.label(RichText::new(location).color(t.text));
    });
    row(ui, t, "", |ui| {
        let can = app.services.has_folder_picker();
        let r = ui.add_enabled_ui(can, |ui| crate::widgets::text_button(ui, "settingsOpenLibrary", "Open Library…", false)).inner;
        register(ui.ctx(), "button:settingsOpenLibrary", r.rect);
        if r.clicked() {
            let _ = app.run("app.openLibrary", json!({}));
        }
        if !can {
            hint(ui, t, li_i18n::library::t("not available here"));
        }
    });
    if app.hosted_menus && crate::widgets::text_button(ui, "settingsAI", "AI…", false).clicked() {
        app.open_shared_ai = true;
    }
    crate::library_safety_ui::settings(app, ui);
    heading(ui, t, "Face recognition");
    hint(ui, t, "Face data stays in this library. Clearing it does not load face models.");
    if crate::widgets::text_button(ui, "settingsClearFaces", li_i18n::library::t("Clear face data…"), false).clicked() {
        app.ui.people.clear = true;
    }
    if app.ui.people.clear {
        check(ui, "removeFaceNames", &mut app.ui.people.remove_names, "Also remove names Smart Sort added");
        hint(ui, t, "Delete stored faces, headshot embeddings and person matches?");
        let r = ui
            .add_enabled_ui(app.people_task.is_none() && app.smart_sort.is_none(), |ui| {
                crate::widgets::text_button(ui, "settingsConfirmClearFaces", "Clear face data", false)
            })
            .inner;
        register(ui.ctx(), "smartSort:clearFaceData", r.rect);
        if r.clicked() {
            match app.session.execute("smartSort.clearFaceData", &json!({"removeNames":app.ui.people.remove_names})) {
                Ok(_) => {
                    app.ui.people = Default::default();
                    app.people_images = Default::default();
                    app.session.smart.people.tagger = None;
                }
                Err(e) => app.ui.people.error = e.to_string(),
            }
        }
        if !app.ui.people.error.is_empty() {
            ui.colored_label(t.caution, &app.ui.people.error);
        }
        if crate::widgets::text_button(ui, "settingsClearFacesCancel", li_i18n::library::t("Cancel"), false).clicked() {
            app.ui.people.clear = false;
        }
    }
    heading(ui, t, li_i18n::library::t("Startup"));
    row(ui, t, li_i18n::library::t("Open in"), |ui| {
        choices(
            ui,
            "settingsStartup",
            &[(StartupView::Last, "Last view"), (StartupView::Grid, "Photo Grid"), (StartupView::Detail, "Loupe")],
            &mut app.ui.settings.startup_view,
        );
    });
    heading(ui, t, li_i18n::library::t("Culling"));
    check(ui, "settings.autoAdvance", &mut app.ui.auto_advance, "Auto Advance: move to the next photo after rating or flagging");
    ui.label(li_i18n::library::t("Deleting photos always asks: Remove from Library, Delete from Disk, or Cancel."));
    check(ui, "settings.confirmSaveOver", &mut app.ui.settings.confirm_save_over, "Ask before File ▸ Save Over Original replaces a file");
    heading(ui, t, li_i18n::library::t("External Editor"));
    row(ui, t, li_i18n::library::t("Application"), |ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.ui.settings.external_editor)
                .hint_text(li_i18n::library::t("System default"))
                .desired_width(220.0_f32.min(ui.available_width())),
        );
        register(ui.ctx(), "field:externalEditor", r.rect);
    });
    hint(
        ui,
        t,
        li_i18n::library::t(
            "Photo ▸ Edit in External Editor (⇧⌘E) renders a 16-bit TIFF copy, stacks it with the original and opens it here (an app name on macOS, a program path elsewhere).",
        ),
    );
}

// -------------------------------------------------------------------------------------- Import

/// A preset picker: `None` = `none_label`. Returns the new choice when it changed.
fn preset_combo(app: &LightcraftApp, ui: &mut egui::Ui, id: &str, current: Option<&str>, none_label: &str) -> Option<Option<String>> {
    let name = |pid: &str| app.session.presets.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_else(|| format!("{pid} (missing)"));
    let text = current.map(name).unwrap_or_else(|| none_label.to_string());
    let mut out = None;
    let r = crate::widgets::choice_menu(ui, id, &text, |ui| {
        if ui.selectable_label(current.is_none(), none_label).clicked() {
            out = Some(None);
        }
        let mut group = "";
        for p in &app.session.presets {
            if p.group != group {
                group = &p.group;
                ui.label(RichText::new(group).size(10.5).weak());
            }
            if ui.selectable_label(current == Some(p.id.as_str()), &p.name).clicked() {
                out = Some(Some(p.id.clone()));
            }
        }
    });
    register(ui.ctx(), format!("combo:{id}"), r.rect);
    out.filter(|v| v.as_deref() != current)
}

/// Cameras of the raws in the library plus those with a stored default, sorted.
fn cameras(app: &LightcraftApp) -> Vec<String> {
    let mut v: Vec<String> = app
        .session
        .catalog
        .photos()
        .filter(|p| p.kind == lightcraft_catalog::MediaKind::Raw && !p.meta.camera.is_empty())
        .map(|p| p.meta.camera.clone())
        .chain(app.session.import_defaults.cameras.iter().map(|c| c.camera.clone()))
        .collect();
    v.sort_by_key(|c| c.to_lowercase());
    v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    v
}

fn import_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let d = app.session.import_defaults.clone();
    heading(ui, t, li_i18n::library::t("Raw defaults"));
    hint(ui, t, li_i18n::library::t("Settings new raw photos start from. Changing them doesn't touch photos already in the library."));
    row(ui, t, li_i18n::library::t("Raw photos"), |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsRawPreset", d.raw_preset.as_deref(), "Local Image Default") {
            let _ = app.run("library.preferences", json!({"import": {"rawPreset": v}}));
        }
    });
    row(ui, t, li_i18n::library::t("Default look for new photos"), |ui| {
        crate::panels::process::default_look_combo(app, ui);
    });
    hint(ui, t, li_i18n::library::t("New raw photos use this look; each photo can change it in Develop (Profile › Look)."));
    row(ui, t, li_i18n::library::t("Camera profiles folder"), |ui| {
        let edit_id = ui.id().with("cameraProfilesFolderEdit");
        let mut folder = ui.data(|data| data.get_temp::<String>(edit_id)).unwrap_or_else(|| d.camera_profiles_folder.clone());
        let response = ui.text_edit_singleline(&mut folder);
        crate::widgets::register(ui.ctx(), "settings:cameraFolder", response.rect);
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(edit_id, folder.clone()));
        } else {
            ui.data_mut(|data| data.remove::<String>(edit_id));
        }
        let (choose, choosing) = if app.services.has_folder_picker() {
            let r = crate::widgets::text_button(ui, "settingsCameraFolderChoose", "Choose…", false);
            crate::widgets::register(ui.ctx(), "settings:cameraFolderChoose", r.rect);
            (r.clicked(), r.is_pointer_button_down_on())
        } else {
            (false, false)
        };
        if choose {
            if let Some(folder) = app.services.choose_folder("Choose Camera Profiles Folder") {
                let _ = app.run("library.preferences", json!({"import":{"cameraProfilesFolder":folder}}));
            }
            ui.data_mut(|data| data.remove::<String>(edit_id));
        } else if !choosing && response.lost_focus() && folder != d.camera_profiles_folder {
            let _ = app.run("library.preferences", json!({"import":{"cameraProfilesFolder":folder}}));
        }
    });
    hint(
        ui,
        t,
        li_i18n::library::t(
            "Read your DCP camera and LCP lens profiles here, including subfolders. Adobe install folders are detected automatically.",
        ),
    );
    if crate::widgets::text_button(ui, "profilesReload", "Reload Profiles", false).clicked() {
        let _ = app.run("library.preferences", json!({"reloadProfiles":true}));
    }
    let (camera, lens, skipped) = lightcraft_engine::dcp_profiles::counts();
    hint(
        ui,
        t,
        &format!(
            "{}: {camera}; {}: {lens}; {}: {skipped}",
            li_i18n::library::t("Camera Profiles"),
            li_i18n::library::t("Lens Profiles"),
            li_i18n::library::t("Skipped")
        ),
    );
    let mut per = d.per_camera;
    row(ui, t, "", |ui| {
        if check(ui, "settings.perCamera", &mut per, "Use camera-specific defaults") {
            let _ = app.run("library.preferences", json!({"import": {"perCamera": per}}));
        }
    });
    if per {
        let cams = cameras(app);
        if cams.is_empty() {
            hint(ui, t, li_i18n::library::t("No raw photos yet: cameras appear here once their photos are in the library."));
        }
        egui::ScrollArea::vertical().max_height(150.0).id_salt("settingsCameras").show(ui, |ui| {
            for (i, cam) in cams.iter().enumerate() {
                let entry = d.cameras.iter().find(|c| c.camera.eq_ignore_ascii_case(cam));
                row(ui, t, cam, |ui| {
                    // "Raw default" = no entry; otherwise the entry's preset (None = Local Image Default)
                    const RAW_DEFAULT: &str = "\u{1}raw";
                    let current = match entry {
                        None => Some(RAW_DEFAULT),
                        Some(e) => e.preset.as_deref(),
                    };
                    let mut pick = None;
                    let label = match current {
                        Some(RAW_DEFAULT) => "Same as raw default".to_string(),
                        None => "Local Image Default".to_string(),
                        Some(pid) => app.session.presets.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_else(|| pid.to_string()),
                    };
                    let id = format!("settingsCamera-{i}");
                    let r = crate::widgets::choice_menu(ui, &id, &label, |ui| {
                        if ui.selectable_label(current == Some(RAW_DEFAULT), li_i18n::library::t("Same as raw default")).clicked() {
                            pick = Some(json!({"camera": cam, "remove": true}));
                        }
                        if ui.selectable_label(current.is_none(), li_i18n::library::t("Local Image Default")).clicked() {
                            pick = Some(json!({"camera": cam, "preset": null}));
                        }
                        for p in &app.session.presets {
                            if ui.selectable_label(current == Some(p.id.as_str()), &p.name).clicked() {
                                pick = Some(json!({"camera": cam, "preset": p.id}));
                            }
                        }
                    });
                    register(ui.ctx(), format!("combo:{id}"), r.rect);
                    if let Some(c) = pick {
                        let _ = app.run("library.preferences", json!({"camera": c}));
                    }
                });
            }
        });
    }
    heading(ui, t, li_i18n::library::t("Other images (JPEG, PNG, TIFF, HEIC…)"));
    row(ui, t, li_i18n::library::t("Non-raw photos"), |ui| {
        if let Some(v) = preset_combo(app, ui, "settingsOtherPreset", d.other_preset.as_deref(), "None") {
            let _ = app.run("library.preferences", json!({"import": {"otherPreset": v}}));
        }
    });
    heading(ui, t, li_i18n::library::t("Metadata"));
    hint(ui, t, li_i18n::library::t("Added to photos you import that don't already have it."));
    for (key, label, hint_text, value) in
        [("copyright", "Copyright", "© 2026 Your Name", d.copyright.clone()), ("creator", "Creator", "Your Name", d.creator.clone())]
    {
        row(ui, t, label, |ui| {
            let id = egui::Id::new(("settingsMeta", key));
            let mut text: String = ui.data(|m| m.get_temp(id)).unwrap_or(value.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint_text).desired_width(240.0_f32.min(ui.available_width())));
            register(ui.ctx(), format!("field:settings.{key}"), r.rect);
            if r.lost_focus() && text.trim() != value {
                let _ = app.run("library.preferences", json!({"import": {key: text.trim()}}));
            }
            if r.has_focus() {
                ui.data_mut(|m| m.insert_temp(id, text));
            } else {
                ui.data_mut(|m| m.remove::<String>(id));
            }
        });
    }
    if !app.session.metadata_presets.is_empty() {
        row(ui, t, li_i18n::library::t("Metadata preset"), |ui| {
            let cur = d.metadata_preset.clone();
            let mut pick = None;
            let r = crate::widgets::choice_menu(ui, "settingsMetaPreset", &cur.clone().unwrap_or_else(|| "None".into()), |ui| {
                if ui.selectable_label(cur.is_none(), li_i18n::library::t("None")).clicked() {
                    pick = Some(String::new());
                }
                for m in &app.session.metadata_presets {
                    if ui.selectable_label(cur.as_deref() == Some(m.name.as_str()), &m.name).clicked() {
                        pick = Some(m.name.clone());
                    }
                }
            });
            register(ui.ctx(), "combo:settingsMetaPreset", r.rect);
            if let Some(n) = pick {
                let _ = app.run("library.preferences", json!({"import": {"metadataPreset": n}}));
            }
        });
    }
    heading(ui, t, li_i18n::library::t("XMP sidecars"));
    let mut xmp = app.session.xmp;
    if check(ui, "settings.autoWriteXmp", &mut xmp.auto_write, "Automatically write changes into XMP sidecars") {
        let _ = app.run("library.xmpPreferences", json!({"autoWrite": xmp.auto_write}));
    }
    row(ui, t, li_i18n::library::t("Sidecar names"), |ui| {
        use lightcraft_engine::sidecar::SidecarNaming as N;
        let mut n = xmp.naming;
        if choices(ui, "settingsXmpNaming", &[(N::Stem, "IMG_1.xmp"), (N::Full, "IMG_1.CR3.xmp")], &mut n) {
            let naming = if n == N::Full { "full" } else { "stem" };
            let _ = app.run("library.xmpPreferences", json!({"naming": naming}));
        }
    });
    if !cfg!(target_arch = "wasm32") {
        heading(ui, t, li_i18n::library::t("Auto Import"));
        hint(
            ui,
            t,
            li_i18n::library::t("Photos that arrive in this folder (tethering, a scanner, a sync app) are added as soon as they're complete."),
        );
        row(ui, t, li_i18n::library::t("Watched folder"), |ui| {
            ui.label(RichText::new(d.auto_folder.clone().unwrap_or_else(|| "Off".into())).color(t.text));
            let can = app.services.has_folder_picker();
            let r = ui.add_enabled_ui(can, |ui| crate::widgets::text_button(ui, "settingsAutoFolder", "Choose…", false)).inner;
            register(ui.ctx(), "button:settingsAutoFolder", r.rect);
            if r.clicked()
                && let Some(f) = app.services.choose_folder("Choose Auto Import Folder")
                && let Err(e) = app.run("library.autoImport", json!({"folder": f}))
            {
                app.toast(ui.ctx(), e);
            }
            if d.auto_folder.is_some() && crate::widgets::text_button(ui, "settingsAutoFolderOff", "Turn Off", false).clicked() {
                let _ = app.run("library.autoImport", json!({"folder": null}));
            }
        });
        if d.auto_folder.is_some() {
            row(ui, t, "", |ui| {
                let mut copy = d.auto_copy;
                if crate::widgets::checkbox(ui, "copyIntoLibrary", &mut copy, "Copy into the library (else use the files where they are)").changed() {
                    let _ = app.run("library.autoImport", json!({"copy": copy}));
                }
            });
            row(ui, t, li_i18n::library::t("Collection"), |ui| {
                let id = egui::Id::new("auto-album");
                let mut name = ui.data(|m| m.get_temp::<String>(id)).unwrap_or_else(|| d.auto_album.clone().unwrap_or_default());
                let r = ui.add(
                    egui::TextEdit::singleline(&mut name).hint_text(li_i18n::library::t("None")).desired_width(180.0_f32.min(ui.available_width())),
                );
                if r.lost_focus() {
                    let _ = app.run("library.autoImport", json!({"album": name.trim()}));
                }
                ui.data_mut(|m| m.insert_temp(id, name));
            });
        }
    }
    if app.session.library.is_none() {
        hint(ui, t, li_i18n::library::t("In-memory session: these settings last until Local Image quits."));
    }
}

// --------------------------------------------------------------------------------- Performance

fn performance_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use lightcraft_engine::gpu;
    heading(ui, t, li_i18n::library::t("Rendering"));
    check(ui, "settings.gpu", &mut app.ui.settings.gpu, "Use the GPU for rendering");
    let status = if !gpu::available() {
        match gpu::unavailable_reason() {
            Some(why) => li_i18n::tr_format!("Rendering on the CPU: {why}", why = why),
            None => "No usable GPU found: rendering on the CPU".to_string(),
        }
    } else {
        format!("GPU: {}", gpu::adapter_name().unwrap_or_else(|| "starting…".into()))
    };
    hint(ui, t, &status);
    row(ui, t, li_i18n::library::t("Preview size"), |ui| {
        let opts: Vec<(u32, String)> = PREVIEW_EDGES.iter().map(|e| (*e, format!("{e} px"))).collect();
        let opts: Vec<(u32, &str)> = opts.iter().map(|(e, l)| (*e, l.as_str())).collect();
        choices(ui, "settingsPreview", &opts, &mut app.ui.settings.preview_edge);
    });
    hint(ui, t, li_i18n::library::t("Largest long edge the Loupe view renders at; larger is sharper on big displays but slower."));
    row(ui, t, li_i18n::library::t("Memory for caches"), |ui| {
        let auto = li_i18n::tr_format!("Automatic ({} MB)", lightcraft_engine::memory::default_budget() >> 20);
        let opts = [(0u32, auto.as_str()), (512, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB")];
        choices(ui, "settingsMemory", &opts, &mut app.ui.settings.memory_mb);
    });
    heading(ui, t, li_i18n::library::t("Thumbnail cache"));
    let cur = (app.session.cache_bytes() >> 20) as u32;
    row(ui, t, li_i18n::library::t("Size limit"), |ui| {
        let opts = [(512u32, "512 MB"), (1024, "1 GB"), (2048, "2 GB"), (4096, "4 GB"), (8192, "8 GB")];
        let mut v = if CACHE_SIZES.contains(&cur) { cur } else { 2048 };
        if choices(ui, "settingsCache", &opts, &mut v) {
            let _ = app.run("library.preferences", json!({"cacheMb": v}));
        }
    });
    let used = app.session.media.rendered.disk().map(|d| d.size());
    row(ui, t, li_i18n::library::t("In use"), |ui| {
        ui.label(RichText::new(used.map(|b| format!("{:.1} MB", b as f64 / 1048576.0)).unwrap_or_else(|| "memory only".into())).color(t.text));
        let r = crate::widgets::text_button(ui, "settingsClearCache", "Clear Cache", false);
        register(ui.ctx(), "button:settingsClearCache", r.rect);
        if r.clicked() {
            let _ = app.run("library.clearPreviews", json!({}));
        }
    });
    heading(ui, t, li_i18n::library::t("Local folders"));
    row(ui, t, li_i18n::library::t("Forget unchanged photos"), |ui| {
        let opts = [(0u32, "Never"), (7, "After 7 days"), (30, "After 30 days"), (90, "After 90 days"), (365, "After a year")];
        let mut v = app.session.forget_local_days;
        if choices(ui, "settingsForgetLocal", &opts, &mut v) {
            let _ = app.run("library.preferences", json!({"forgetLocalDays": v}));
        }
    });
    hint(
        ui,
        t,
        li_i18n::library::t(
            "Photos seen in Local but never added or changed leave the catalog when their folder hasn't been browsed for this long (checked when the library opens). Files stay on disk; browsing the folder shows them again.",
        ),
    );
    #[cfg(not(target_arch = "wasm32"))]
    smart_previews(app, ui, t);
}

/// Where this library keeps its smart previews (the offline-editing proxies, which can be large):
/// the effective folder, what is in it, and choosing another one.
#[cfg(not(target_arch = "wasm32"))]
fn smart_previews(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    // listing a big folder every frame would be slow: refresh every 2 s and after a change
    let cache = egui::Id::new("smart-location");
    let now = ui.input(|i| i.time);
    let loc: Value = match ui.data(|d| d.get_temp::<(f64, Value)>(cache)) {
        Some((at, v)) if now - at < 2.0 => v,
        _ => {
            let v = app.run("library.smartPreviewsLocation", json!({})).unwrap_or(Value::Null);
            ui.data_mut(|d| d.insert_temp(cache, (now, v.clone())));
            v
        }
    };
    if loc.is_null() {
        return;
    }
    heading(ui, t, li_i18n::library::t("Smart previews"));
    let path = loc["path"].as_str().unwrap_or_default().to_string();
    let available = loc["available"].as_bool().unwrap_or(true);
    let custom = loc["custom"].as_bool().unwrap_or(false);
    row(ui, t, li_i18n::library::t("Folder"), |ui| {
        let text = format!("{path}{}", if custom { "" } else { "  (library default)" });
        ui.add(egui::Label::new(RichText::new(text.clone()).color(if available { t.text } else { t.reject })).truncate()).on_hover_text(text);
    });
    let (count, bytes) = (loc["count"].as_u64().unwrap_or(0), loc["bytes"].as_u64().unwrap_or(0));
    if available {
        hint(
            ui,
            t,
            &li_i18n::tr_format!("{count} smart preview{} · {:.1} MB", if count == 1 { "" } else { "s" }, bytes as f64 / 1048576.0, count = count),
        );
    } else {
        hint(
            ui,
            t,
            li_i18n::library::t(
                "This folder is not available (drive disconnected?). Smart previews are not built or used until it is back or you choose another folder.",
            ),
        );
    }
    // what happens to the previews already built when the folder changes
    let mode_id = egui::Id::new("smart-existing");
    let mut mode: u8 = ui.data(|d| d.get_temp(mode_id)).unwrap_or(0);
    if count > 0 {
        row(ui, t, li_i18n::library::t("Existing previews"), |ui| {
            if choices(ui, "settingsSmartExisting", &[(0u8, "Move them"), (1, "Leave them"), (2, "Delete them")], &mut mode) {
                ui.data_mut(|d| d.insert_temp(mode_id, mode));
            }
        });
    }
    let existing = ["move", "leave", "discard"][mode.min(2) as usize];
    let mut result = None;
    ui.horizontal_wrapped(|ui| {
        if app.services.has_folder_picker() {
            let r = crate::widgets::text_button(ui, "settingsSmartChoose", "Choose Folder…", false);
            register(ui.ctx(), "button:settingsSmartChoose", r.rect);
            if r.clicked()
                && let Some(dir) = app.services.choose_folder("Choose Smart Preview Folder")
            {
                result = Some(app.run("library.smartPreviewsLocation", json!({"path": dir, "existing": existing})));
            }
        }
        let r = ui.add_enabled_ui(custom, |ui| crate::widgets::text_button(ui, "settingsSmartReset", "Use Library Folder", false)).inner;
        register(ui.ctx(), "button:settingsSmartReset", r.rect);
        if r.clicked() {
            result = Some(app.run("library.smartPreviewsLocation", json!({"reset": true, "existing": existing})));
        }
    });
    if let Some(r) = result {
        ui.data_mut(|d| d.remove::<(f64, Value)>(cache));
        match r {
            Ok(v) => {
                let failed = v["failed"].as_array().map_or(0, Vec::len);
                let msg = match (existing, v["handled"].as_u64().unwrap_or(0)) {
                    (_, 0) => "Smart previews folder changed".to_string(),
                    ("move", n) => format!("Smart previews folder changed; moved {n}"),
                    ("discard", n) => format!("Smart previews folder changed; deleted {n}"),
                    _ => "Smart previews folder changed".to_string(),
                };
                app.toast(ui.ctx(), if failed > 0 { li_i18n::tr_format!("{msg} ({failed} failed)", failed = failed, msg = msg) } else { msg });
            }
            Err(e) => app.toast(ui.ctx(), e),
        }
    }
    hint(
        ui,
        t,
        li_i18n::library::t("Keep it on a drive with room: smart previews are about 1 MB per photo. The thumbnail cache stays in the library."),
    );
}

// ---------------------------------------------------------------------------------- Interface

fn interface_tab(app: &mut LightcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    heading(ui, t, li_i18n::library::t("Tools"));
    row(ui, t, li_i18n::library::t("Tool tips"), |ui| {
        use crate::panels::tool_tips::ToolTipMode as M;
        choices(
            ui,
            "settingsToolTips",
            &[
                (M::Rich, li_i18n::library::t("Rich")),
                (M::Simple, li_i18n::library::t("Simple (name and shortcut)")),
                (M::Off, li_i18n::library::t("Off")),
            ],
            &mut app.ui.settings.tool_tips,
        );
    });
    if !app.hosted_menus {
        row(ui, t, "Tool icons", |ui| {
            choices(ui, "settingsToolIcons", &[(false, "Monochrome (default)"), (true, "Colour")], &mut app.ui.settings.color_tool_icons);
        });
    }
    heading(ui, t, li_i18n::library::t("Filmstrip"));
    check(ui, "settings.filmNames", &mut app.ui.settings.film_names, "Show file names");
    check(ui, "settings.filmBadges", &mut app.ui.settings.film_badges, "Show ratings, flags and edit badges");
    heading(ui, t, li_i18n::library::t("Grid"));
    row(ui, t, li_i18n::library::t("Ratings & flags"), |ui| {
        choices(
            ui,
            "settingsGridBadges",
            &[(GridBadges::Auto, "When rated or hovered"), (GridBadges::Always, "Always"), (GridBadges::Never, "Never")],
            &mut app.ui.settings.grid_badges,
        );
    });
    check(ui, "settings.showFilenames", &mut app.ui.show_filenames, "Square Grid: show file names and formats");
    heading(ui, t, li_i18n::library::t("Loupe"));
    check(ui, "settings.navigator", &mut app.ui.navigator, "Show the Navigator while zoomed in");
    row(ui, t, li_i18n::library::t("Info overlay"), |ui| {
        use crate::state::InfoOverlay as I;
        choices(ui, "settingsInfo", &[(I::Off, "Off"), (I::Basic, "File & date"), (I::Exposure, "Exposure")], &mut app.ui.info_overlay);
    });
}

// ------------------------------------------------------------------------------- Open Library…

/// `app.openLibrary {path?}`: close the current library and open (or create) the one at `path`,
/// or a folder chosen in a dialog. Remembered as the library to open at launch.
pub fn open_library(app: &mut LightcraftApp, p: &Value) -> Result<Value, String> {
    if app.import.is_some() || app.export.is_some() {
        return Err("Finish or cancel the current import or export before switching libraries".into());
    }
    let path = match p.get("path").and_then(Value::as_str) {
        Some(x) => x.to_string(),
        None => match app.services.has_folder_picker() {
            true => match app.services.choose_folder("Choose Library Folder") {
                Some(x) => x,
                None => return Ok(Value::Null),
            },
            false => return Err("no folder dialog on this platform".into()),
        },
    };
    app.session.close_library().map_err(|e| e.to_string())?;
    app.session.open_library(&path, false).map_err(|e| e.to_string())?;
    app.renderer.forget_all();
    // A review from the old catalog contains its duplicate/album identities.
    app.scan = None;
    if matches!(app.ui.dialog, Some(crate::state::Dialog::Import { .. })) {
        app.ui.dialog = None;
    }
    app.ui.external_edits.clear();
    app.people_task = None;
    app.smart_sort = None;
    app.people_images = Default::default();
    app.ui.people = Default::default();
    app.ui.compare = None;
    app.ui.settings.library_path = path.clone();
    Ok(json!({"path": path, "photos": app.session.catalog.len()}))
}
