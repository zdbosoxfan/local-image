//! The Library catalog contributed to the host's existing shortcut editor.
use super::{ALIASES, parse};
use crate::LightcraftApp;
use serde_json::Value;
pub const PREFIX: &str = "library::";
#[derive(Clone)]
pub struct Binding {
    pub key: String,
    pub command: String,
    pub label: String,
    pub menu: Vec<String>,
    pub default: Option<String>,
    pub params: Option<Value>,
}
fn identity(chord: &str) -> Option<(egui::Modifiers, egui::Key)> {
    parse(chord).map(|(modifiers, key)| super::chord_identity(modifiers, key))
}
pub fn catalog() -> Vec<Binding> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut keys = Vec::new();
    for (id, label, shortcut, menu) in crate::menus::ui_commands() {
        if !seen.insert((*id).to_string()) {
            continue;
        }
        if let Some(key) = shortcut.and_then(identity) {
            keys.push(key);
        }
        out.push(Binding {
            key: format!("{PREFIX}{id}"),
            command: (*id).into(),
            label: crate::i18n::tr(label).into(),
            menu: vec!["Library".into(), menu.split('>').next().unwrap_or("Other").into()],
            default: shortcut.map(str::to_string),
            params: None,
        });
    }
    for c in lightcraft_engine::command_specs() {
        if !seen.insert(c.id.to_string()) {
            continue;
        }
        let default = c.shortcut.filter(|s| identity(s).is_some_and(|k| !keys.contains(&k))).map(str::to_string);
        out.push(Binding {
            key: format!("{PREFIX}{}", c.id),
            command: c.id.into(),
            label: crate::i18n::tr(c.label).into(),
            menu: vec!["Library".into(), c.menu.first().copied().unwrap_or("Other").into()],
            default,
            params: None,
        });
    }
    for (sc, id, params) in ALIASES {
        let label = out.iter().find(|b| b.command == *id).map(|b| b.label.clone()).unwrap_or_else(|| "Photo action".into());
        let params: Value = serde_json::from_str(params).unwrap_or_default();
        let label = if *id == "photo.flag" && params["advance"] == true {
            crate::i18n::tr(match params["flag"].as_str() {
                Some("pick") => "Pick and advance",
                Some("reject") => "Reject and advance",
                _ => "Clear flag and advance",
            })
            .to_string()
        } else if let Some(index) = params["index"].as_u64() {
            crate::i18n::tr("Toggle keyword {index}").replace("{index}", &index.to_string())
        } else {
            crate::i18n::tr("{action} (alternate)").replace("{action}", &label)
        };
        out.push(Binding {
            key: format!("{PREFIX}alias/{id}/{sc}"),
            command: (*id).into(),
            label,
            menu: vec!["Library".into(), "Photo".into()],
            default: Some((*sc).into()),
            params: Some(params),
        });
    }
    for rating in 0..=5 {
        for advance in [false, true] {
            let command = format!("rate:{rating}:{}", u8::from(advance));
            out.push(Binding {
                key: format!("{PREFIX}{command}"),
                command,
                label: if advance {
                    crate::i18n::tr("Set rating {rating} and advance").replace("{rating}", &rating.to_string())
                } else {
                    crate::i18n::tr("Set rating {rating}").replace("{rating}", &rating.to_string())
                },
                menu: vec!["Library".into(), "Photo".into()],
                default: Some(format!("{}{rating}", if advance { "Shift+" } else { "" })),
                params: None,
            });
        }
    }
    for (color, key) in [("red", "6"), ("yellow", "7"), ("green", "8"), ("blue", "9")] {
        let command = format!("label:{color}");
        out.push(Binding {
            key: format!("{PREFIX}{command}"),
            command,
            label: crate::i18n::tr("Set {color} label").replace("{color}", crate::i18n::tr(color)),
            menu: vec!["Library".into(), "Photo".into()],
            default: Some(key.into()),
            params: None,
        });
    }
    for (command, label, key) in [("grid.up", "Previous grid row", "Up"), ("grid.down", "Next grid row", "Down")] {
        out.push(Binding {
            key: format!("{PREFIX}{command}"),
            command: command.into(),
            label: crate::i18n::tr(label).into(),
            menu: vec!["Library".into(), "View".into()],
            default: Some(key.into()),
            params: None,
        });
    }
    out.push(Binding {
        key: format!("{PREFIX}tool.erase"),
        command: "tool.erase".into(),
        label: "Erase mask".into(),
        menu: vec!["Library".into(), "Tools".into()],
        default: None,
        params: None,
    });
    out.push(Binding {
        key: format!("{PREFIX}alias/tool.linear/L"),
        command: "tool.linear".into(),
        label: "Linear Gradient (current Capture One)".into(),
        menu: vec!["Library".into(), "Tools".into()],
        default: None,
        params: Some(serde_json::json!({})),
    });
    out
}
pub type HostShortcutItem = (String, String, Vec<String>, Option<String>);
pub fn host_catalog() -> Vec<HostShortcutItem> {
    catalog().into_iter().map(|b| (b.key, b.label, b.menu, b.default)).collect()
}
pub fn effective(app: &LightcraftApp) -> Vec<(Binding, String)> {
    let mut bindings = catalog();
    let parent = app.shortcut_overrides.get("library::__profile").map(String::as_str).unwrap_or("Lightroom");
    let edit =
        matches!(app.ui.right, crate::state::RightPanel::Edit | crate::state::RightPanel::Masking) && app.ui.view == crate::state::ViewMode::Detail;
    for b in &mut bindings {
        b.default = crate::keymaps::default(parent, &b.key, edit, b.default.as_deref()).map(str::to_string);
        if parent == "Capture One" && edit && b.key == "library::alias/tool.linear/L" {
            b.default = Some("L".into());
        }
    }
    bindings.sort_by_key(|b| {
        (
            !matches!(b.command.as_str(), "app.quit" | "edit.undo" | "edit.redo" | "view.enterFullScreen" | "view.fullScreenPreview"),
            !app.shortcut_overrides.contains_key(&b.key),
        )
    });
    let mut seen = Vec::new();
    bindings
        .into_iter()
        .filter_map(|b| {
            let global = matches!(b.command.as_str(), "app.quit" | "edit.undo" | "edit.redo" | "view.enterFullScreen" | "view.fullScreenPreview");
            let sc = if global { b.default.clone() } else { app.shortcut_overrides.get(&b.key).cloned().or_else(|| b.default.clone()) }?;
            if !global && crate::keymaps::GLOBAL_CHORDS.iter().filter_map(|key| identity(key)).any(|key| Some(key) == identity(&sc)) {
                return None;
            }
            let key = identity(&sc)?;
            if seen.contains(&key) {
                return None;
            }
            seen.push(key);
            Some((b, sc))
        })
        .collect()
}
pub fn command_shortcut(app: &LightcraftApp, id: &str) -> Option<String> {
    effective(app).into_iter().find(|(b, _)| b.command == id && b.params.is_none()).map(|(_, s)| s)
}
pub fn consume_grid(app: &LightcraftApp, ctx: &egui::Context) -> Option<egui::Key> {
    if app.host_modal
        || app.ui.dialog.is_some()
        || egui::Popup::is_any_open(ctx)
        || !ctx.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Key { pressed: true, .. })))
    {
        return None;
    }
    for (command, direction) in [
        ("library.previous", egui::Key::ArrowLeft),
        ("library.next", egui::Key::ArrowRight),
        ("grid.up", egui::Key::ArrowUp),
        ("grid.down", egui::Key::ArrowDown),
    ] {
        if let Some((m, k)) = command_shortcut(app, command).as_deref().and_then(parse)
            && ctx.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(|event| !matches!(event,egui::Event::Key{key,pressed:true,modifiers,..} if super::key_matches(m,k,*modifiers,*key)));
                before != i.events.len()
            })
        {
            return Some(direction);
        }
    }
    None
}
pub fn original_held(app: &LightcraftApp, ctx: &egui::Context) -> bool {
    if app.host_modal || egui::Popup::is_any_open(ctx) || ctx.input(|i| i.keys_down.is_empty()) {
        return false;
    }
    command_shortcut(app, "view.showOriginal")
        .as_deref()
        .and_then(parse)
        .is_some_and(|(m, k)| ctx.input(|i| i.keys_down.iter().any(|key| super::key_matches(m, k, i.modifiers, *key))))
}
