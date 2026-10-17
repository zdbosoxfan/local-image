//! Edit › Preferences, Keyboard Shortcuts and Menus, Color Settings and the rest of the Edit
//! menu's dialogs, plus the shell side of preferences: loading and saving them through
//! [`crate::Services`], applying the interface theme, autosave and crash recovery, and the
//! history log.
//!
//! The values live in the engine ([`photocraft_engine::prefs::Preferences`], so agents read and
//! change them with `prefs.get` / `prefs.set`); this module only edits a working copy in a dialog
//! and commits it with those commands on Apply or OK.

use li_keys::input::{is_modifier_key, shortcut_text};
use std::collections::{BTreeMap, HashMap};

use egui::{Color32, RichText, vec2};
use photocraft_doc::DocId;
use photocraft_engine::prefs::{self, SECTIONS, Theme};
use photocraft_engine::snap::{SnapLine, SnapTargets};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;
use crate::theme::{ThemeKind, Tokens};

mod lifecycle;
mod mismatch;
mod preferences_page;
mod presets;
mod shortcut_dialog;

pub use self::lifecycle::*;
pub(crate) use self::mismatch::*;
use self::preferences_page::*;
use self::presets::*;
pub use self::shortcut_dialog::*;

/// Extra preference pages supplied by the app host. The editor provides the only modal shell,
/// navigation and Apply/OK/Cancel actions. A host keeps the draft as JSON and owns its validation.
/// Catalog maintenance actions can remain immediate, with an explicit notice in their page.
pub trait HostPreferencesPages {
    fn pages(&self) -> &'static [(&'static str, &'static str)];
    fn values(&self) -> Value;
    fn body(&mut self, ui: &mut egui::Ui, section: &str, draft: &mut Value);
    /// Validate and reserve the host draft without changing live settings. Publication must be
    /// infallible: the shell calls it only after every fallible part of the transaction succeeds.
    fn prepare(&self, draft: &Value) -> Result<Box<dyn FnOnce() + '_>, String>;
}

/// Shell runtime state for preferences, autosave and snapping (not serialised).
#[derive(Default)]
pub struct Runtime {
    loaded: bool,
    /// Preferences revision last written to storage (or loaded from it).
    saved_rev: u64,
    ai_revision: u64,
    /// This instance's preferences as of `saved_rev`: the base of the three-way merge on save.
    saved_value: Option<Value>,
    /// Defaults/current state before an initial I/O failure; local edits are rebased
    /// onto the first readable original, never saved over it as a default snapshot.
    unreadable_base: Option<Value>,
    unreadable_notice: Option<u64>,
    save_retry: SaveRetry,
    theme_pref: Option<Theme>,
    next_autosave_ms: f64,
    // Resource preservation can replace a snapshot without advancing its edit revision.
    // Weak identity detects those changes without retaining another large document.
    autosaved: HashMap<DocId, (u64, std::sync::Weak<photocraft_doc::Document>)>,
    /// Autosaves queued on a background writer that hasn't reported yet. A queued write is not a
    /// durable snapshot: it moves to `autosaved` only once the write succeeded (upstream
    /// PhotoCraft 83d5c543).
    autosave_pending: HashMap<DocId, (u64, std::sync::Weak<photocraft_doc::Document>)>,
    log_len: usize,
    /// Snapping state of the drag in progress (see `snap_ui`).
    pub(crate) snap: Option<crate::snap_ui::ActiveSnap>,
    pub(crate) snap_lines: Vec<SnapLine>,
    pub(crate) guide_targets: Option<((DocId, u64), SnapTargets)>,
    /// Checkerboard colours the cached CPU checker texture was built with.
    pub(crate) checker_key: Option<[[u8; 3]; 2]>,
    /// Style last sent to the GPU canvas.
    pub(crate) gpu_style: Option<crate::gpu_canvas::CanvasStyle>,
}

/// The GPU canvas colours from Preferences › Transparency & Gamut.
pub fn canvas_style(app: &PhotocraftApp) -> crate::gpu_canvas::CanvasStyle {
    let t = &app.session.prefs().transparency_and_gamut;
    let [l, d] = t.colors();
    let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    let g = prefs::parse_hex(&t.gamut_warning_color).unwrap_or([128; 3]);
    crate::gpu_canvas::CanvasStyle {
        checker_square: t.square().unwrap_or(0.0),
        checker_light: f(l),
        checker_dark: f(d),
        gamut_color: f(g),
        gamut_opacity: t.gamut_warning_opacity as f32 / 100.0,
    }
}

/// Pasteboard colour from Preferences › Interface (`None` = the theme's default canvas).
pub fn pasteboard_color(app: &PhotocraftApp) -> Option<Color32> {
    use prefs::CanvasColor;
    let i = &app.session.prefs().interface;
    Some(match i.canvas_color {
        CanvasColor::Default => return None,
        CanvasColor::Black => Color32::BLACK,
        CanvasColor::DarkGray => Color32::from_gray(40),
        CanvasColor::MediumGray => Color32::from_gray(83),
        CanvasColor::LightGray => Color32::from_gray(184),
        CanvasColor::Custom => color_of(&i.canvas_custom_color),
    })
}

fn theme_kind(t: Theme) -> ThemeKind {
    match t {
        Theme::Pro => ThemeKind::Pro,
        Theme::ProMedium => ThemeKind::ProMedium,
        Theme::Studio => ThemeKind::Studio,
        Theme::StudioLight => ThemeKind::StudioLight,
        Theme::Classic => ThemeKind::Classic,
    }
}

fn theme_pref(k: ThemeKind) -> Theme {
    match k {
        ThemeKind::Pro => Theme::Pro,
        ThemeKind::ProMedium => Theme::ProMedium,
        ThemeKind::Studio => Theme::Studio,
        ThemeKind::StudioLight => Theme::StudioLight,
        ThemeKind::Classic => Theme::Classic,
    }
}

// Menu routing

/// Shell front ends for Edit-menu commands invoked without parameters (dialogs, pickers).
/// `None` when `id` isn't handled here.
pub fn invoke(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let empty = params.as_object().is_none_or(|o| o.is_empty());
    if !empty {
        return None;
    }
    if let Some(section) = id.strip_prefix("edit.preferences.") {
        return Some(Ok(json!({"dialog": open_preferences(app, section)})));
    }
    let dialog = |d: Option<u64>| Some(d.map(|d| json!({"dialog": d})).ok_or_else(|| "not available".to_string()));
    match id {
        "edit.keyboardShortcuts" => Some(Ok(json!({"dialog": open_shortcuts(app, 0)}))),
        "edit.menus" => Some(Ok(json!({"dialog": open_shortcuts(app, 1)}))),
        "edit.toolbar" => Some(Ok(json!({"dialog": open_shortcuts(app, 2)}))),
        "edit.colorSettings" => {
            let d = crate::filter_dialog::open(app, "edit.colorSettings");
            let cur = serde_json::to_value(&app.session.color.settings).unwrap_or_default();
            // What Monitor Profile resolves to, so a fallback to sRGB is visible (#569): read the
            // displays again now, and the dialog refreshes this line as it draws.
            crate::monitor_status::read_now(app);
            let note = crate::monitor_status::note(app);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                for (k, v) in cur.as_object().into_iter().flatten() {
                    if dm.fields.contains_key(k) {
                        dm.fields.insert(k.clone(), v.clone());
                    }
                }
                dm.fields.insert("__note".into(), json!(note));
            }
            dialog(d)
        }
        "edit.fade" | "edit.findAndReplaceText" | "edit.defineBrushPreset" | "edit.defineCustomShape" | "edit.autoAlignLayers" | "edit.autoBlendLayers" => {
            if !app.session.is_enabled(id) {
                return Some(Err(photocraft_engine::commands::find(id)
                    .map_or("not available".into(), |c| format!("{} is not available right now", c.label.trim_end_matches('…')))));
            }
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                // Optional parameters the generic dialog can't express keep their defaults.
                for k in ["reference", "path"] {
                    dm.fields.remove(k);
                }
            }
            dialog(d)
        }
        "edit.contentAwareFill" => {
            if !app.session.is_enabled(id) {
                return Some(Err("Content-Aware Fill needs a selection on a pixel layer".into()));
            }
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                dm.fields.insert("__preview".into(), json!(true));
                for k in ["margin", "seed", "channel"] {
                    dm.fields.remove(k);
                }
            }
            dialog(d)
        }
        "edit.contentAwareScale" => {
            let st = app.session.active()?;
            let b = match &st.doc.selection {
                Some(s) => s.content_bounds(),
                None => st.active_layer.and_then(|l| st.doc.layer(l)).and_then(|l| l.surface()).map(|s| s.content_bounds()).unwrap_or_default(),
            };
            let d = crate::filter_dialog::open(app, id);
            if let Some(dm) = d.and_then(|d| app.ui.dialog_mut(d)) {
                dm.fields.insert("width".into(), json!(b.width()));
                dm.fields.insert("height".into(), json!(b.height()));
                dm.fields.remove("scaleX");
                dm.fields.remove("scaleY");
                dm.fields.insert("__preview".into(), json!(true));
            }
            dialog(d)
        }
        "edit.presets.presetManager" => {
            Some(Ok(json!({"dialog": open_kind(app, "presets", "Preset Manager", json!({"kind": "brushes", "selected": 0, "newName": ""}))})))
        }
        "edit.presets.exportImportPresets" => Some(Ok(
            json!({"dialog": open_kind(app, "presetsIO", "Export/Import Presets", json!({"action": "export", "brushes": true, "customShapes": true}))}),
        )),
        _ => None,
    }
}

fn open_kind(app: &mut PhotocraftApp, kind: &str, label: &str, fields: Value) -> u64 {
    let mut f = Map::new();
    f.insert("__prefsui".into(), json!(kind));
    f.insert("__label".into(), json!(label));
    if let Value::Object(m) = fields {
        f.extend(m);
    }
    app.ui.open_dialog(DialogKind::Command, f)
}

/// Revert the toolbar preview when the Shortcuts/Toolbar dialog is cancelled.
pub(crate) fn cancel(app: &mut PhotocraftApp, fields: &Map<String, Value>) {
    if is_preferences(fields) {
        crate::ai_ui::settings_ui::reset();
    }
    if fields.get("__prefsui").and_then(Value::as_str) == Some("shortcuts")
        && let Some(original) = fields.get("toolbarOriginal").and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        app.session.edit_prefs(|p| p.toolbar = original);
    }
}

/// Is this a dialog rendered by this module?
pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key("__prefsui")
}

/// Only Preferences supports applying changes without closing the dialog.
pub fn is_preferences(fields: &Map<String, Value>) -> bool {
    fields.get("__prefsui").and_then(Value::as_str) == Some("prefs")
}

fn preference_values(p: &prefs::Preferences) -> Map<String, Value> {
    let values = p.to_json();
    let mut sections: Map<String, Value> = SECTIONS.iter().filter_map(|(id, _)| Some((id.to_string(), values.get(id)?.clone()))).collect();
    sections.insert("toolbar".into(), json!({"vectorKeyboard": p.toolbar.vector_keyboard}));
    sections
}

/// Compare the editable sections, excluding settings managed outside Preferences.
pub fn preferences_changed(app: &PhotocraftApp, fields: &Map<String, Value>) -> bool {
    is_preferences(fields)
        && (fields.get("hostValues").is_some_and(|v| app.host_preferences.as_ref().is_some_and(|host| *v != host.values()))
            || crate::ai_ui::settings_ui::changed()
            || fields.get("values").and_then(Value::as_object).is_some_and(|values| *values != preference_values(app.session.prefs())))
}

/// Commit the working copy and keep the current section open. Failure leaves the draft intact.
pub fn apply(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    let d = app.ui.dialogs.iter().find(|d| d.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    if d.kind != DialogKind::Command || !is_preferences(&d.fields) {
        return Err("Apply is only available for Preferences".into());
    }
    let fields = d.fields.clone();
    let result = match confirm(app, &fields) {
        Ok(result) => result,
        Err(error) => {
            if let Some(dialog) = app.ui.dialog_mut(id) {
                dialog.fields.insert("__error".into(), json!(error));
            }
            return Err(error);
        }
    };
    let values = Value::Object(preference_values(app.session.prefs()));
    if let Some(d) = app.ui.dialog_mut(id) {
        // Use the validated values as the next draft, including any normalisation by prefs.set.
        d.fields.insert("values".into(), values);
        d.fields.insert("hostValues".into(), app.host_preferences.as_ref().map_or_else(|| app.host_preferences_values.clone(), |host| host.values()));
        d.fields.remove("__error");
    }
    Ok(result)
}

/// Max dialog width for our dialogs.
pub fn width(fields: &Map<String, Value>) -> Option<f32> {
    match fields.get("__prefsui").and_then(Value::as_str)? {
        "prefs" => Some(780.0),
        "shortcuts" => Some(720.0),
        _ => Some(460.0),
    }
}

/// Open Edit › Preferences on `section`.
pub fn open_preferences(app: &mut PhotocraftApp, section: &str) -> u64 {
    let known = app.host_preferences_pages.iter().any(|(id, _)| *id == section)
        || SECTIONS.iter().any(|(id, _)| *id == section)
        || section == crate::attributions::SECTION
        || crate::ai_ui::settings_ui::owns(section);
    let section = if known && section != "integrations" { section } else { "general" };

    if let Some(existing) = app.ui.dialogs.iter_mut().find(|d| is_preferences(&d.fields)) {
        existing.fields.insert("section".into(), json!(section));
        return existing.id;
    }
    crate::ai_ui::settings_ui::reset();
    let working = preference_values(app.session.prefs());
    let order = field_order(app.session.prefs(), &working);
    open_kind(
        app,
        "prefs",
        "Preferences",
        json!({"section": section, "values": working, "__order": order,
        "hostValues": app.host_preferences.as_ref().map_or_else(|| app.host_preferences_values.clone(), |host| host.values())}),
    )
}

/// Each section's keys in declaration order (JSON objects sort their keys; the serialised text
/// keeps the struct's order, which groups related settings like Photoshop's dialog).
fn field_order(p: &prefs::Preferences, sections: &Map<String, Value>) -> Value {
    let text = serde_json::to_string(p).unwrap_or_default();
    let mut out = Map::new();
    for (id, v) in sections {
        let Some(start) = text.find(&format!("\"{id}\":{{")) else { continue };
        let body = &text[start..];
        let end = body.find('}').unwrap_or(body.len());
        let region = &body[..end];
        let mut keys: Vec<(usize, String)> =
            v.as_object().into_iter().flatten().map(|(k, _)| (region.find(&format!("\"{k}\":")).unwrap_or(usize::MAX), k.clone())).collect();
        keys.sort();
        out.insert(id.clone(), json!(keys.into_iter().map(|(_, k)| k).collect::<Vec<_>>()));
    }
    Value::Object(out)
}

// Bodies

/// Render one of our dialogs' bodies.
pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    match f.get("__prefsui").and_then(Value::as_str).unwrap_or("") {
        "prefs" => {
            f.insert("__gpuInfo".into(), json!(app.perf.gpu_info.lines()));
            f.insert("__gpuFailure".into(), json!(app.perf.gpu_info.failure()));
            prefs_body(app, ui, f);
            if f.remove("openShortcuts").and_then(|v| v.as_bool()).unwrap_or(false) {
                open_shortcuts(app, 0);
            }
            crate::attributions::open_pending(app, ui.ctx(), f);
        }
        "shortcuts" => shortcuts_body(app, ui, f),
        "presets" => presets_body(app, ui, f),
        "presetsIO" => presets_io_body(ui, f),
        "mismatch" => mismatch_body(ui, f),
        "clipboard" => crate::clipboard_ui::body(ui, f),
        _ => {}
    }
}

// Confirm

/// OK on one of our dialogs.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    match f.get("__prefsui").and_then(Value::as_str).unwrap_or("") {
        "prefs" => {
            let mut values = f.get("values").cloned().unwrap_or(Value::Null);
            if let Some(ai) = crate::ai_ui::settings_ui::pending_settings() {
                let values = values.as_object_mut().ok_or("Preferences values must be an object")?;
                values.insert("ai".into(), serde_json::to_value(ai).map_err(|error| error.to_string())?);
            }
            let params = json!({"path": "", "value": values});
            if app.automation_input
                && let Some(authorize) = app.services.automation_command.as_ref()
            {
                authorize("prefs.set", &params)?;
            }
            // Prepare under the same engine authorization boundary as app.run, then restore
            // its temporary gate. No live history limits/resources change during validation.
            let previous_gate = app.session.authorize;
            if app.automation_input && previous_gate.is_none() {
                app.session.authorize = app.services.automation_authorize;
            }
            let prepared = app.session.prepare_preferences_update(&params);
            app.session.authorize = previous_gate;
            let prepared = prepared.map_err(|error| error.to_string())?;
            let host = app.host_preferences.take();
            let result = (|| {
                let publish_host = host.as_ref().zip(f.get("hostValues")).map(|(host, draft)| host.prepare(draft)).transpose()?;
                // A failed checked AI write leaves core/host values and their resources intact.
                crate::ai_ui::settings_ui::apply()?;
                let result = app.session.publish_prepared_preferences(prepared);
                if let Some(publish) = publish_host {
                    publish();
                }
                Ok::<_, String>(result)
            })();
            app.host_preferences = host;
            let result = result?;
            if !app.ui.dialogs.iter().any(|dialog| is_preferences(&dialog.fields)) {
                crate::ai_ui::settings_ui::reset();
            }
            Ok(result)
        }
        "shortcuts" => {
            let overrides = f.get("overrides").cloned().unwrap_or(json!({}));
            // Shortcuts moved to another command are taken from their old owner (shell
            // commands included, which the engine doesn't know).
            // The map replaces every stored override, so a malformed one is an error, not "none".
            let mut ov: BTreeMap<String, String> =
                serde_json::from_value(overrides).map_err(|e| format!("overrides must map command IDs to shortcut strings: {e}"))?;
            let mut profiles: li_keys::ShortcutProfiles = match f.get("profiles") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => app.session.prefs().shortcut_profiles.clone(),
            };
            let mut defaults = shortcut_items(app);
            defaults.extend(f.get("__hostShortcuts").and_then(|v| serde_json::from_value::<Vec<HostShortcutItem>>(v.clone()).ok()).unwrap_or_default());
            profile_defaults(app.session.prefs(), &profiles, &mut defaults);
            let changed: Vec<_> = ov.iter().filter(|(_, key)| !key.is_empty()).map(|(id, key)| (id.clone(), key.clone())).collect();
            for (id, key) in changed {
                if crate::keymaps::global_conflict(&key).is_some() {
                    continue;
                }
                let unchanged_library = id.starts_with("library::")
                    && app.session.prefs().shortcut_overrides(true).get(&id).and_then(|old| li_keys::normalize_shortcut(old)).as_deref()
                        == li_keys::normalize_shortcut(&key).as_deref();
                if let Some(action) = reserved_library_chord(&id, &key)
                    && !unchanged_library
                {
                    return Err(reserved_message(action));
                }
                // Reviewing an unchanged Library binding must not silently disable a default
                // elsewhere; only a changed assignment moves keys.
                if unchanged_library && profiles.library.parent() == app.session.prefs().shortcut_profiles.library.parent() {
                    continue;
                }
                for (other, _, _, default) in &defaults {
                    if *other != id
                        && !li_keys::keymaps::is_global(other)
                        && same_shortcut_scope(&id, other)
                        && !ov.contains_key(other)
                        && default.as_deref().is_some_and(|old| same_chord_in_scope(&id, old, &key))
                    {
                        ov.insert(other.clone(), String::new());
                    }
                }
            }
            for library in [false, true] {
                let workspace = profiles.workspace_mut(library);
                let current: BTreeMap<_, _> = ov.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(k, v)| (k.clone(), v.clone())).collect();
                let old = workspace.custom.get(&workspace.active).map(|p| p.overrides.clone()).unwrap_or_default();
                if current != old {
                    workspace.editable().overrides = current;
                }
            }
            // Apply through a single prefs transaction: invalid profiles cannot partially save.
            app.run("prefs.set", json!({"values": {"shortcutProfiles": profiles, "menus": {"hidden": f.get("hidden").cloned().unwrap_or(json!([])), "colors": f.get("colors").cloned().unwrap_or(json!({}))}}}))
        }
        "presets" => Ok(Value::Null),
        "presetsIO" => {
            let kinds: Vec<&str> = ["brushes", "customShapes"].into_iter().filter(|k| f.get(*k).and_then(Value::as_bool).unwrap_or(true)).collect();
            if f.get("action").and_then(Value::as_str) == Some("import") {
                let (name, bytes) = app.pick_file_bytes().ok_or_else(|| "cancelled".to_string())??;
                let text = String::from_utf8(bytes).map_err(|_| format!("{name} is not a preset file"))?;
                app.run("edit.presets.exportImportPresets", json!({"action": "import", "kinds": kinds, "data": text}))
            } else {
                let out = app.run("edit.presets.exportImportPresets", json!({"action": "export", "kinds": kinds}))?;
                let text = serde_json::to_string_pretty(&out["data"]).map_err(|e| e.to_string())?;
                let path = app.services.pick_save.as_mut().and_then(|p| p("Presets.pcpresets")).ok_or("cancelled")?;
                let write = app.services.write.as_mut().ok_or("no writer configured")?;
                write(&path, text.as_bytes())?;
                app.ui.status = format!("Exported presets to {path}");
                Ok(json!({"path": path}))
            }
        }
        "clipboard" => crate::clipboard_ui::confirm(app, f),
        "mismatch" => confirm_mismatch(app, f),
        _ => Err("unknown dialog".into()),
    }
}

#[cfg(test)]
#[path = "shortcut_capture_tests.rs"]
mod shortcut_capture_tests;

#[cfg(test)]
#[path = "shared_library_shortcut_tests.rs"]
mod shared_library_shortcut_tests;

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod keymap_tests;

#[cfg(test)]
#[path = "design_system_shots.rs"]
pub(crate) mod design_system_shots;

#[cfg(test)]
mod tests;
