//! Keyboard Shortcuts and Menus: the shortcut catalog, host (Library) shortcuts, reserved chords
//! and the dialog body.
use super::*;

// Shortcuts

pub use crate::shortcuts::{default_shortcut, effective_shortcut};

/// Every shortcut-bearing command (engine registry plus shell commands): (id, label, menu path,
/// default shortcut), menu items first in Photoshop order.
pub fn shortcut_items(app: &PhotocraftApp) -> Vec<(String, String, Vec<String>, Option<String>)> {
    let mut out: Vec<(String, String, Vec<String>, Option<String>)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in crate::menus::menu_items(app) {
        if it.label == "---" || !seen.insert(it.id.clone()) {
            continue;
        }
        let default = default_shortcut(&it.id)
            .or(it.shortcut.clone().filter(|_| !app.session.prefs().shortcut_overrides(it.id.starts_with("library::")).contains_key(&it.id)));
        out.push((it.id, it.label, it.path, default));
    }
    // Commands without a menu item: the colour and fill keys (D, X, ⌥⌫…) sit under Tools, as
    // Photoshop lists its colour keys in the Tools shortcut set, after the rest; then the held
    // temporary tools (#249).
    let tools_key = |id: &str| id.starts_with("tools.") || photocraft_engine::fill_key_cmds::IDS.contains(&id);
    let mut tools = Vec::new();
    let mut layer = Vec::new();
    for c in photocraft_engine::command_specs() {
        if seen.insert(c.id.to_string()) && c.shortcut.is_some() {
            let item = (c.id.to_string(), c.label.to_string(), c.menu.iter().map(|s| s.to_string()).collect::<Vec<_>>(), c.shortcut.map(Into::into));
            if c.menu.is_empty() && tools_key(c.id) {
                tools.push((item.0, item.1, vec!["Tools".to_string()], item.3));
            } else if c.menu.is_empty() && c.id.starts_with("layer.") {
                // Stamp Visible / Stamp Down (#217): no menu item in Photoshop; listed at the end
                // of the Layer section.
                layer.push((item.0, item.1, vec!["Layer".to_string()], item.3));
            } else {
                out.push(item);
            }
        }
    }
    let at = out.iter().rposition(|i| i.2.first().map(String::as_str) == Some("Layer")).map_or(out.len(), |i| i + 1);
    out.splice(at..at, layer);
    out.extend(tools);
    for (id, label, def) in prefs::TOOL_SHORTCUTS {
        out.push((id.to_string(), label.to_string(), vec!["Tools".into()], def.map(str::to_string)));
    }
    for tool in photocraft_algo::liquify::LiquifyTool::ALL {
        out.push((
            crate::liquify_ui::shortcut_id(tool).to_owned(),
            tool.label().to_string(),
            vec!["Tools".into(), "Liquify".into()],
            (!crate::liquify_ui::shortcut(tool).is_empty()).then(|| crate::liquify_ui::shortcut(tool).to_string()),
        ));
    }
    for tool in crate::tool_tips::CameraRawTool::ALL {
        out.push((tool.shortcut_id().into(), tool.title().into(), vec!["Tools".into(), "Camera Raw".into()], Some(tool.default_shortcut().into())));
    }
    for (id, label, def) in prefs::TEMPORARY_TOOLS {
        if seen.insert(id.to_string()) {
            out.push((id.to_string(), label.to_string(), vec!["Tools".into(), "Temporary".into()], Some(def.to_string())));
        }
    }
    // Integrated vector menus may append commands to an earlier section. The
    // shortcut editor renders one heading per contiguous section, preserving
    // the menu's first-encounter order and each section's command order.
    let mut sections = Vec::<Option<String>>::new();
    for item in &out {
        let section = item.2.first().cloned();
        if !sections.contains(&section) {
            sections.push(section);
        }
    }
    out.sort_by_key(|item| sections.iter().position(|section| section.as_ref() == item.2.first()).unwrap_or(sections.len()));
    out
}

/// Runtime host catalog: key, human label, menu path, default chord. The shared dialog and
/// the Library shortcut profile remain the only editor and persisted authority.
pub type HostShortcutItem = (String, String, Vec<String>, Option<String>);
pub fn set_host_shortcut_catalog(app: &mut PhotocraftApp, ctx: &egui::Context, items: Vec<HostShortcutItem>) {
    for dialog in &mut app.ui.dialogs {
        if dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("shortcuts") {
            dialog.fields.insert("__hostShortcuts".into(), json!(items));
        }
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("host-shortcut-catalog"), items));
}
/// Match the host's documented exact chords, retaining the genuine metadata-save default.
pub(super) fn reserved_library_chord(id: &str, chord: &str) -> Option<&'static str> {
    if !id.starts_with("library::") {
        return None;
    }
    let shortcut = li_keys::Chord::parse(chord)?;
    if ["Cmd+Alt+1", "Cmd+Alt+2", "Cmd+Alt+3"].iter().any(|key| li_keys::Chord::parse(key).is_some_and(|key| shortcut.overlaps(&key))) {
        Some("switching Library, Develop and Compositing")
    } else if id != "library::photo.saveMetadataToFile" && li_keys::Chord::parse("Cmd+S").is_some_and(|key| shortcut.overlaps(&key)) {
        Some("saving the selected unsaved composite in Library")
    } else {
        None
    }
}
pub(super) fn reserved_message(action: &str) -> String {
    format!("This shortcut is reserved for {action}. Choose another shortcut; the existing binding is unchanged.")
}

pub(super) fn same_chord_in_scope(_id: &str, a: &str, b: &str) -> bool {
    li_keys::Chord::parse(a).zip(li_keys::Chord::parse(b)).is_some_and(|(a, b)| a.overlaps(&b))
}

pub(super) fn same_shortcut_scope(a: &str, b: &str) -> bool {
    a.starts_with("library::") == b.starts_with("library::")
}

/// Open Keyboard Shortcuts and Menus on a tab (0 shortcuts, 1 menus, 2 toolbar).
pub fn open_shortcuts(app: &mut PhotocraftApp, tab: u64) -> u64 {
    let p = app.session.prefs();
    let mut overrides = p.shortcut_overrides(false).clone();
    overrides.extend(p.shortcut_overrides(true).iter().map(|(id, key)| (id.clone(), key.clone())));
    let fields = json!({
        "tab": tab,
        "filter": "",
        "overrides": overrides,
        "profiles": p.shortcut_profiles,
        "hidden": p.menus.hidden,
        "colors": p.menus.colors,
        "toolbarOriginal": p.toolbar,
        "selected": "",
        "capture": false,
        "message": "",
    });
    open_kind(app, "shortcuts", tl!("Keyboard Shortcuts and Menus"), fields)
}

/// Keyboard Shortcuts and Menus: a searchable list of commands by menu with editable shortcuts
/// (click a shortcut, press keys; ⌫ removes it), menu visibility and colours, toolbar tools.
pub(super) fn shortcuts_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let mut tab = f.get("tab").and_then(Value::as_u64).unwrap_or(0);
    ui.horizontal(|ui| {
        for (i, name) in [tl!("Keyboard Shortcuts"), tl!("Menus"), tl!("Toolbar")].iter().enumerate() {
            if crate::widgets::pill_tab(ui, name, tab == i as u64).clicked() {
                tab = i as u64;
                f.insert("capture".into(), json!(false));
            }
        }
    });
    f.insert("tab".into(), json!(tab));
    ui.add_space(6.0);
    if tab == 2 {
        crate::toolsets_ui::tab(app, ui, f);
        return;
    }
    let mut filter = f.get("filter").and_then(Value::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Search")).color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut filter).desired_width(260.0).hint_text(tl!("command or shortcut")));
    });
    f.insert("filter".into(), json!(filter));
    // A malformed map (set over automation) is left as it is, so OK rejects it instead of saving
    // the empty map drawn in its place (upstream PhotoCraft 57be0f58, #956).
    let parsed = f.get("overrides").map(|v| serde_json::from_value::<BTreeMap<String, String>>(v.clone()));
    let overrides_ok = parsed.as_ref().is_none_or(Result::is_ok);
    let mut overrides = parsed.and_then(Result::ok).unwrap_or_default();
    let mut hidden: Vec<String> = f.get("hidden").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut colors: BTreeMap<String, String> = f.get("colors").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut selected = f.get("selected").and_then(Value::as_str).unwrap_or("").to_string();
    let mut capture = f.get("capture").and_then(Value::as_bool).unwrap_or(false);
    let mut message = f.get("message").and_then(Value::as_str).unwrap_or("").to_string();
    let mut items = shortcut_items(app);
    if tab == 0 {
        let host = ui.ctx().data(|d| d.get_temp::<Vec<HostShortcutItem>>(egui::Id::new("host-shortcut-catalog"))).unwrap_or_default();
        f.insert("__hostShortcuts".into(), json!(host));
        items.extend(host);
    }
    let mut scope = f.get("shortcutScope").and_then(Value::as_str).unwrap_or("editor").to_string();
    let previous_scope = scope.clone();
    let parsed_profiles = f.get("profiles").cloned().map(serde_json::from_value::<li_keys::ShortcutProfiles>);
    if parsed_profiles.as_ref().is_some_and(Result::is_err) {
        ui.label("Invalid shortcut profiles. Cancel to keep your saved profiles.");
        return;
    }
    let mut profiles = parsed_profiles.and_then(Result::ok).unwrap_or_else(|| app.session.prefs().shortcut_profiles.clone());
    if tab == 0 {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Scope")).color(t.text_dim));
            let scopes = [("global".to_string(), "Global"), ("library".to_string(), "Library + Develop"), ("editor".to_string(), "Editor")];
            crate::widgets::dropdown(ui, "shortcut-module", &mut scope, &scopes, 180.0);
        });
        if scope != previous_scope {
            capture = false;
            selected.clear();
            message.clear();
        }
        f.insert("shortcutScope".into(), json!(scope));
        if scope == "global" {
            ui.label(RichText::new("Global shortcuts are fixed and take precedence in every workspace.").color(t.text_dim));
            for (_, label, key) in li_keys::GLOBAL_SHORTCUTS {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.label(crate::shortcuts::pretty(key));
                });
            }
            f.insert("capture".into(), json!(false));
            f.insert("profiles".into(), json!(profiles));
            return;
        }
        let library = scope == "library";
        let workspace = profiles.workspace_mut(library);
        let previous = workspace.active.clone();
        let builtins = if library { ["Lightroom", "Capture One"] } else { ["Photoshop", "Affinity Photo"] };
        let mut names: Vec<String> = builtins.iter().map(|name| (*name).into()).collect();
        names.extend(workspace.custom.keys().cloned());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Profile").color(t.text_dim));
            // Profile names are the user's own words: shown as typed, never looked up.
            let active = workspace.active.clone();
            crate::widgets::dropdown_menu(ui, "shortcut-profile", &active, 180.0, |ui| {
                for name in names {
                    ui.selectable_value(&mut workspace.active, name.clone(), &name);
                }
            });
        });
        // The profile commands on their own row, so the buttons keep their size in a narrow dialog.
        ui.horizontal_wrapped(|ui| {
            let mut name = f.get("profileName").and_then(Value::as_str).unwrap_or("").to_string();
            let name_field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0).hint_text("Profile name"));
            name_field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Profile name"));
            f.insert("profileName".into(), json!(name));
            if crate::widgets::secondary_button(ui, "Duplicate", 0.0).clicked() {
                if let Err(error) = workspace.duplicate(&name) {
                    message = error;
                } else {
                    message.clear();
                }
            }
            let custom = workspace.custom.contains_key(&workspace.active);
            if ui.add_enabled_ui(custom, |ui| crate::widgets::secondary_button(ui, "Rename", 0.0)).inner.clicked()
                && let Err(error) = workspace.rename(&name)
            {
                message = error;
            }
            if ui.add_enabled_ui(custom, |ui| crate::widgets::secondary_button(ui, "Delete Profile", 0.0)).inner.clicked() {
                workspace.delete_active();
            }
        });
        if previous != workspace.active {
            overrides.retain(|id, _| id.starts_with("library::") != library);
            if let Some(profile) = workspace.custom.get(&workspace.active) {
                overrides.extend(profile.overrides.clone());
            }
            capture = false;
            selected.clear();
            message.clear();
        }
        if workspace.parent() == "Affinity Photo" {
            ui.label(RichText::new("Supported Affinity equivalents only; gaps are unassigned. R: Rotate View; J: Healing / AI Remove.").color(t.text_dim));
        }
        if workspace.parent() == "Capture One" {
            ui.label(RichText::new("B / E / G / M apply with Develop Edit or Mask active. L also selects Linear Gradient.").color(t.text_dim));
        }
        profile_defaults(app.session.prefs(), &profiles, &mut items);
    }
    let original_overrides = overrides.clone();
    // Menu path and command label as shown in the menus, in the UI language.
    let shown = |id: &str, label: &str, path: &[String]| -> (Vec<String>, String) {
        let lang = li_i18n::current();
        (path.iter().map(|p| li_i18n::tr(lang, p).to_string()).collect(), li_i18n::tr_id(lang, id, label).to_string())
    };
    let eff = |overrides: &BTreeMap<String, String>, id: &str, def: &Option<String>| -> Option<String> {
        match overrides.get(id) {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.clone()),
            None => def.clone(),
        }
    };
    // Key capture for the selected command.
    if capture && tab == 0 && !selected.is_empty() {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } if !is_modifier_key(*key) => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, m)) = pressed {
            ui.input_mut(|i| {
                i.consume_key(m, key);
            });
            if key == egui::Key::Escape {
                capture = false;
            } else if matches!(key, egui::Key::Backspace | egui::Key::Delete) && m.is_none() {
                overrides.insert(selected.clone(), String::new());
                capture = false;
                message = tl!("Shortcut removed.").into();
            } else if let Some(sc) = shortcut_text(key, m) {
                if let Some(action) = crate::keymaps::global_conflict(&sc) {
                    overrides.insert(selected.clone(), sc.clone());
                    message = format!("Conflict with Global: {action}. This binding is ignored.");
                    capture = false;
                } else if let Some(action) = reserved_library_chord(&selected, &sc) {
                    message = reserved_message(action);
                    capture = false;
                } else {
                    let clash: Vec<String> = items
                        .iter()
                        .filter(|(id, _, _, def)| {
                            *id != selected
                                && same_shortcut_scope(id, &selected)
                                && eff(&overrides, id, def).as_deref().is_some_and(|old| same_chord_in_scope(&selected, old, &sc))
                        })
                        .map(|(id, label, path, _)| {
                            let (path, label) = shown(id, label, path);
                            format!("{} › {}", path.join(" › "), label.trim_end_matches('…'))
                        })
                        .collect();
                    if selected.starts_with("library::") {
                        let displaced: Vec<_> = items
                            .iter()
                            .filter(|(id, _, _, def)| {
                                *id != selected
                                    && same_shortcut_scope(id, &selected)
                                    && eff(&overrides, id, def).as_deref().is_some_and(|old| same_chord_in_scope(&selected, old, &sc))
                            })
                            .map(|(id, _, _, _)| id.clone())
                            .collect();
                        for id in displaced {
                            overrides.insert(id, String::new());
                        }
                    }
                    overrides.insert(selected.clone(), sc.clone());
                    message = if clash.is_empty() {
                        format!("{} assigned.", crate::shortcuts::pretty(&sc))
                    } else {
                        format!("{} is already in use by {} and will be removed from it when you click OK.", crate::shortcuts::pretty(&sc), clash.join(", "))
                    };
                    capture = false;
                }
            }
        }
    }
    if message.is_empty()
        && let Some(chord) = overrides.get(&selected)
        && let Some(action) = reserved_library_chord(&selected, chord)
    {
        message = format!("Saved binding retained. {}", reserved_message(action));
    }
    let needle = filter.to_ascii_lowercase();
    let name_width = (ui.available_width() - 250.0).clamp(110.0, 440.0);
    egui::ScrollArea::both().max_height(360.0).id_salt("shortcuts-scroll").show(ui, |ui| {
        let mut last_top = String::new();
        egui::Grid::new("shortcut-grid").num_columns(3).spacing([12.0, 3.0]).striped(true).show(ui, |ui| {
            for (id, label, path, def) in &items {
                if tab == 0 && li_keys::keymaps::is_global(id) {
                    continue;
                }
                if tab == 0 && ((scope == "library" && !id.starts_with("library::")) || (scope == "editor" && id.starts_with("library::"))) {
                    continue;
                }
                let cur = eff(&overrides, id, def);
                let (shown_path, shown_label) = shown(id, label, path);
                // Match what is shown and the English name (commands are documented in English).
                let hay = format!("{} {} {} {} {}", shown_path.join(" "), shown_label, path.join(" "), label, cur.clone().unwrap_or_default()).to_lowercase();
                if !needle.is_empty() && !hay.contains(&needle) && !id.to_ascii_lowercase().contains(&needle) {
                    continue;
                }
                if tab == 0 && path.is_empty() && def.is_none() && cur.is_none() {
                    continue;
                }
                let top = shown_path.first().cloned().unwrap_or_else(|| tl!("Other").into());
                if top != last_top {
                    ui.label(RichText::new(&top).font(crate::theme::semibold(12.5)).color(t.text));
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    last_top = top;
                }
                let name = match shown_path.get(1..) {
                    Some(rest) if !rest.is_empty() => format!("{} › {}", rest.join(" › "), shown_label),
                    _ => shown_label,
                };
                let sel = selected == *id;
                if ui
                    .add_sized(
                        [name_width, 20.0],
                        egui::Button::selectable(sel, RichText::new(format!("   {name}")).color(t.text_dim)).wrap_mode(egui::TextWrapMode::Truncate),
                    )
                    .on_hover_text(&name)
                    .clicked()
                {
                    selected = id.clone();
                }
                if tab == 0 {
                    let text = if sel && capture { tl!("Press keys…").to_string() } else { cur.as_deref().map(crate::shortcuts::pretty).unwrap_or_default() };
                    let changed = overrides.contains_key(id);
                    let r = ui.add(
                        egui::Button::new(RichText::new(tl!(&text)).size(12.0).color(if changed { t.accent_text } else { t.text })).min_size(vec2(120.0, 18.0)),
                    );
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-capture", id)), r.rect));
                    if r.clicked() {
                        selected = id.clone();
                        capture = true;
                        message.clear();
                    }
                    let conflict = cur.as_deref().and_then(crate::keymaps::global_conflict).map(|action| format!("Global: {action}")).or_else(|| {
                        cur.as_deref().and_then(|chord| {
                            items
                                .iter()
                                .find(|(other, _, _, default)| {
                                    *other != *id
                                        && same_shortcut_scope(other, id)
                                        && !li_keys::keymaps::is_global(other)
                                        && eff(&overrides, other, default).as_deref().is_some_and(|key| same_chord_in_scope(id, key, chord))
                                })
                                .map(|(_, label, _, _)| format!("Conflict: {label}"))
                        })
                    });
                    ui.add_sized(
                        [100.0, 20.0],
                        egui::Label::new(
                            RichText::new(conflict.as_deref().unwrap_or(if changed { "modified" } else { "" }))
                                .color(if conflict.is_some() { t.warning } else { t.text_faint })
                                .size(10.5),
                        )
                        .wrap(),
                    )
                    .on_hover_text(conflict.as_deref().unwrap_or(""));
                } else {
                    let mut visible = !hidden.contains(id);
                    crate::widgets::checkbox(ui, &mut visible, tl!("Visible"));
                    if visible {
                        hidden.retain(|h| h != id);
                    } else if !hidden.contains(id) {
                        hidden.push(id.clone());
                    }
                    let mut col = colors.get(id).cloned().unwrap_or_else(|| "none".into());
                    let opts: Vec<(String, &str)> =
                        ["none", "red", "orange", "yellow", "green", "blue", "violet", "gray"].iter().map(|c| (c.to_string(), *c)).collect();
                    crate::widgets::dropdown(ui, &format!("menu-color-{id}"), &mut col, &opts, 90.0);
                    if col == "none" {
                        colors.remove(id);
                    } else {
                        colors.insert(id.clone(), col);
                    }
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(6.0);
    if tab == 0 {
        ui.horizontal(|ui| {
            let has_sel = !selected.is_empty();
            let action = ui.add_enabled_ui(has_sel, |ui| crate::widgets::secondary_button(ui, tl!("Use Default"), 0.0)).inner;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "default")), action.rect));
            if action.clicked() {
                overrides.remove(&selected);
                message = tl!("Default restored.").into();
            }
            let action = ui.add_enabled_ui(has_sel, |ui| crate::widgets::secondary_button(ui, tl!("Delete Shortcut"), 0.0)).inner;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "delete")), action.rect));
            if action.clicked() {
                overrides.insert(selected.clone(), String::new());
                message = tl!("Shortcut removed.").into();
            }
            let action = crate::widgets::secondary_button(ui, tl!("Reset All to Defaults"), 0.0);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("shortcut-action", "reset")), action.rect));
            if action.clicked() {
                overrides.retain(|id, _| id.starts_with("library::") != (scope == "library"));
                message = tl!("All shortcuts in this profile reset to defaults.").into();
            }
        });
    } else if crate::widgets::secondary_button(ui, tl!("Show All Menu Items"), 0.0).clicked() {
        hidden.clear();
        colors.clear();
    }
    if !message.is_empty() {
        ui.label(RichText::new(&message).color(if message.contains("already in use") || message.contains("reserved") || message.contains("Conflict") {
            t.warning
        } else {
            t.text_dim
        }));
    }
    if overrides_ok {
        if tab == 0 && overrides != original_overrides {
            let library = scope == "library";
            let workspace = profiles.workspace_mut(library);
            if !workspace.custom.contains_key(&workspace.active) {
                let mut name = format!("Custom ({})", workspace.active);
                while workspace.custom.contains_key(&name) {
                    name.push('2');
                }
                let _ = workspace.duplicate(&name);
            }
            if let Some(profile) = workspace.custom.get_mut(&workspace.active) {
                profile.overrides =
                    overrides.iter().filter(|(id, _)| id.starts_with("library::") == library).map(|(id, key)| (id.clone(), key.clone())).collect();
            }
        }
        f.insert("profiles".into(), json!(profiles));
        f.insert("overrides".into(), json!(overrides));
    }
    f.insert("hidden".into(), json!(hidden));
    f.insert("colors".into(), json!(colors));
    f.insert("selected".into(), json!(selected));
    f.insert("capture".into(), json!(capture));
    f.insert("message".into(), json!(message));
}

/// Both the dialog and OK resolve defaults against the profile being edited.
pub(super) fn profile_defaults(prefs: &prefs::Preferences, profiles: &li_keys::ShortcutProfiles, items: &mut [HostShortcutItem]) {
    let mut working = prefs.clone();
    working.shortcut_profiles = profiles.clone();
    for (id, _, _, def) in items {
        *def = if id.starts_with("library::") {
            li_keys::keymaps::library_default(profiles.library.parent(), id, true, def.as_deref()).map(str::to_string)
        } else {
            crate::keymaps::default(&working, id, def.as_deref()).map(str::to_string)
        };
    }
}
