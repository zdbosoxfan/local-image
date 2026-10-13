//! Keyboard shortcuts: parse `Cmd+Shift+X` style strings and dispatch UI and engine commands.

use egui::{Key, Modifiers};
use serde_json::json;

use crate::LightcraftApp;
#[path = "shortcuts_catalog.rs"]
pub mod catalog;

/// Secondary key bindings for commands that already exist: `(shortcut, command id, params JSON)`.
/// They complement the primary shortcut declared on the command (Lightroom-desktop keys that our
/// primary keymap assigns elsewhere, see docs/parity.md → Shortcuts). Shown in Help → Keyboard Shortcuts.
pub const ALIASES: &[(&str, &str, &str)] = &[
    ("B", "album.toggleTarget", "{}"),
    ("Cmd+D", "library.selectNone", "{}"),
    ("Shift+E", "dialog.export", "{}"),
    ("Space", "view.zoomToggle", "{}"),
    ("Shift+X", "photo.flag", r#"{"flag": "reject", "advance": true}"#),
    ("Shift+P", "photo.flag", r#"{"flag": "pick", "advance": true}"#),
    ("Shift+U", "photo.flag", r#"{"flag": "none", "advance": true}"#),
    // Shift+[ / Shift+] arrive as { / } on most layouts
    ("Shift+{", "brush.featherLess", "{}"),
    ("Shift+}", "brush.featherMore", "{}"),
    // keyword set: ⌥1–⌥9 toggle the current set's keywords on the selection
    ("Alt+1", "keyword.toggleFromSet", r#"{"index": 1}"#),
    ("Alt+2", "keyword.toggleFromSet", r#"{"index": 2}"#),
    ("Alt+3", "keyword.toggleFromSet", r#"{"index": 3}"#),
    ("Alt+4", "keyword.toggleFromSet", r#"{"index": 4}"#),
    ("Alt+5", "keyword.toggleFromSet", r#"{"index": 5}"#),
    ("Alt+6", "keyword.toggleFromSet", r#"{"index": 6}"#),
    ("Alt+7", "keyword.toggleFromSet", r#"{"index": 7}"#),
    ("Alt+8", "keyword.toggleFromSet", r#"{"index": 8}"#),
    ("Alt+9", "keyword.toggleFromSet", r#"{"index": 9}"#),
];

pub fn parse(s: &str) -> Option<(Modifiers, Key)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    let plus = s == "+" || s.ends_with("++");
    let body = if plus { s.trim_end_matches('+') } else { s };
    for part in body.split('+').filter(|part| !part.is_empty()) {
        match part {
            "Cmd" => m.command = true,
            "Shift" => m.shift = true,
            "Alt" => m.alt = true,
            "Ctrl" => m.ctrl = true,
            // "Delete" means the key labelled ⌫ (egui's Backspace); forward-delete also matches, see `matches`.
            "Delete" => {
                if key.is_some() {
                    return None;
                }
                key = Some(Key::Backspace);
            }
            k => {
                if key.is_some() {
                    return None;
                }
                key = Some(Key::from_name(k).or(match k {
                    "Right" => Some(Key::ArrowRight),
                    "Left" => Some(Key::ArrowLeft),
                    "Up" => Some(Key::ArrowUp),
                    "Down" => Some(Key::ArrowDown),
                    "\\" => Some(Key::Backslash),
                    "/" => Some(Key::Slash),
                    "=" => Some(Key::Equals),
                    "-" => Some(Key::Minus),
                    "[" => Some(Key::OpenBracket),
                    "]" => Some(Key::CloseBracket),
                    "'" => Some(Key::Quote),
                    "," => Some(Key::Comma),
                    _ => None,
                })?)
            }
        }
    }
    if plus {
        if key.is_some() {
            return None;
        }
        key = Some(Key::Plus);
    }
    key.map(|k| (m, k))
}

/// One exact identity for dispatch, row navigation, held actions and collision precedence.
pub(crate) fn chord_identity(mut modifiers: Modifiers, mut key: Key) -> (Modifiers, Key) {
    if !cfg!(target_os = "macos") {
        modifiers.command |= modifiers.ctrl;
        modifiers.ctrl = false;
    } else {
        modifiers.command |= modifiers.mac_cmd;
    }
    modifiers.mac_cmd = false;
    if key == Key::Delete {
        key = Key::Backspace;
    }
    (modifiers, key)
}
pub(crate) fn key_matches(pattern: Modifiers, key: Key, actual: Modifiers, actual_key: Key) -> bool {
    chord_identity(pattern, key) == chord_identity(actual, actual_key)
}
fn matches(i: &egui::InputState, m: Modifiers, k: Key) -> bool {
    i.events.iter().any(|event| matches!(event,egui::Event::Key{key,pressed:true,modifiers,..} if key_matches(m,k,*modifiers,*key)))
}

/// Labels use the same command lookup as dispatch, including UI wrappers of engine keys.
pub fn shortcut_label(command: &str) -> Option<String> {
    crate::menus::ui_commands()
        .find(|c| c.0 == command)
        .and_then(|c| c.2)
        .or_else(|| lightcraft_engine::command_specs().iter().find(|c| c.id == command).and_then(|c| c.shortcut))
        .map(|sc| crate::menubar::shortcut_text(sc, cfg!(target_os = "macos")))
}

/// Rendered labels and dispatch use the same effective primary bindings.
pub fn publish_labels(app: &LightcraftApp, ctx: &egui::Context) {
    let labels: std::collections::BTreeMap<_, _> =
        catalog::effective(app).into_iter().filter(|(b, _)| b.params.is_none()).map(|(b, key)| (b.command, key)).collect();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("library-effective-shortcuts"), std::sync::Arc::new(labels)));
}
pub fn label_in(ctx: &egui::Context, command: &str) -> Option<String> {
    match ctx.data(|d| d.get_temp::<std::sync::Arc<std::collections::BTreeMap<String, String>>>(egui::Id::new("library-effective-shortcuts"))) {
        Some(map) => map.get(command).map(|s| crate::menubar::shortcut_text(s, cfg!(target_os = "macos"))),
        None => shortcut_label(command),
    }
}

pub fn hint_in(ctx: &egui::Context, label: &str, command: &str) -> String {
    let label = crate::i18n::tr(label);
    label_in(ctx, command).map_or_else(|| label.to_string(), |key| format!("{label} ({key})"))
}

pub fn handle(app: &mut LightcraftApp, ctx: &egui::Context) {
    if app.host_modal
        || egui::Popup::is_any_open(ctx)
        // application dialogs are modal: they own their keys (text editing, Esc), and photo
        // shortcuts must not act on the grid behind them (upstream LightCraft e1267a7)
        || app.ui.dialog.is_some()
        || app.ui.people.queue.is_some()
        || app.ui.people.finder.is_some()
        || app.ui.people.merge.is_some()
    {
        return;
    }
    // don't steal keys from text fields
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let mut fire: Vec<String> = Vec::new();
    // shortcuts the native menu bar handles (it consumes those key presses itself)
    let native = |sc: &str| app.native_shortcuts.contains(sc);
    let mut aliased: Vec<(String, serde_json::Value)> = Vec::new();
    ctx.input(|i| {
        for (binding, sc) in catalog::effective(app) {
            if native(&sc) {
                continue;
            }
            if let Some((m, k)) = parse(&sc)
                && matches(i, m, k)
            {
                if let Some(params) = binding.params {
                    aliased.push((binding.command, params));
                } else {
                    fire.push(binding.command);
                }
            }
        }
    });
    // local-image: a Camera Raw Filter session keeps to Develop
    if let Some(h) = &app.host_session {
        fire.retain(|id| h.allows(id));
        aliased.retain(|(id, _)| h.allows(id));
    }
    use crate::panels::compare;
    // rating/flag/label keys: in Compare/Survey they act on the active photo only; Shift+key or
    // Auto Advance then moves on (next candidate in Compare, next photo elsewhere)
    let cull = |app: &mut LightcraftApp, id: &str, mut params: serde_json::Value, advance: bool| {
        compare::target_active(app, &mut params);
        match app.run(id, params) {
            Ok(result) => {
                if advance || app.ui.auto_advance {
                    compare::advance(app);
                }
                result.get("changed").and_then(serde_json::Value::as_u64).unwrap_or(0) > 0
            }
            Err(e) => {
                app.toast_error(ctx, e);
                false
            }
        }
    };
    fire.retain(|id| {
        id != "view.showOriginal"
            && !id.starts_with("grid.")
            // The grid's own row-aware navigation owns the arrows, except in the full-screen
            // preview, which hides the grid (Lightroom: arrows step photos there too).
            && !(matches!(app.ui.view, crate::state::ViewMode::PhotoGrid | crate::state::ViewMode::SquareGrid)
                && !app.ui.fullscreen
                && matches!(id.as_str(), "library.next" | "library.previous"))
    });
    for (id, params) in aliased {
        let id = id.as_str();
        // Space pauses / resumes a slideshow
        if id == "view.zoomToggle"
            && let Some((interval, _, paused)) = app.ui.slideshow
        {
            let now = ctx.input(|i| i.time);
            app.ui.slideshow = Some((interval, now + interval, !paused));
            app.toast(ctx, if paused { "Slideshow resumed" } else { "Slideshow paused" });
            continue;
        }
        // flag/rate aliases (Shift+X…) go through the culling path: active photo in Compare/Survey,
        // `advance` moves to the next candidate there
        if matches!(id, "photo.flag" | "photo.rate" | "photo.label") {
            let mut params = params;
            let advance = params.get("advance").and_then(serde_json::Value::as_bool).unwrap_or(false);
            if let Some(o) = params.as_object_mut() {
                o.remove("advance");
            }
            cull(app, id, params, advance);
        } else if let Err(e) = app.run(id, params) {
            // an export that can't start (e.g. no folder) says why instead of doing nothing
            app.toast(ctx, e);
        }
    }
    for f in fire {
        if let Some(rest) = f.strip_prefix("rate:") {
            let (n, adv) = rest.split_once(':').unwrap_or(("0", "0"));
            let changed = cull(app, "photo.rate", json!({"rating": n.parse::<u8>().unwrap_or(0)}), adv == "1");
            let label =
                if n == "0" { "Rating cleared".to_string() } else { crate::i18n::tr_format!("Rated {}", "★".repeat(n.parse().unwrap_or(0))) };
            if changed {
                app.toast(ctx, label);
            }
        } else if let Some(l) = f.strip_prefix("label:") {
            cull(app, "photo.label", json!({"label": l}), false);
        } else if f == "view.softProof" && matches!(app.ui.view, crate::state::ViewMode::PhotoGrid | crate::state::ViewMode::SquareGrid) {
            // S in a grid: expand / collapse the stack (Lightroom's Library binding)
            let _ = app.run("stack.toggle", json!({}));
        } else if compare::culling(app) && (f == "library.next" || f == "library.previous") {
            let d = if f == "library.next" { 1 } else { -1 };
            let _ = if app.ui.view == crate::state::ViewMode::Compare { compare::compare_step(app, d) } else { compare::survey_step(app, d) };
        } else if matches!(f.as_str(), "photo.pick" | "photo.reject" | "photo.unflag") && app.ui.right != crate::state::RightPanel::Crop {
            if !cull(app, &f, json!({}), false) {
                continue;
            }
            match f.as_str() {
                "photo.pick" => app.toast(ctx, "Flagged as Pick"),
                "photo.reject" => app.toast(ctx, "Flagged as Reject"),
                _ => app.toast(ctx, "Unflagged"),
            }
        } else {
            // in the full-screen preview (no panels) I cycles the info overlay instead
            if f == "panel.info" && app.ui.fullscreen {
                let _ = app.run("view.infoOverlay", json!({}));
                continue;
            }
            // Delete acts on what's being edited: the active mask in the Masking panel; never the
            // photo while retouching (spots are removed from their own panel).
            if f == "photo.delete" {
                use crate::state::RightPanel as R;
                match app.ui.right {
                    R::Masking => {
                        if app.session.active_mask.is_some() {
                            let _ = app.run("mask.delete", json!({}));
                        }
                        continue;
                    }
                    R::Remove => {
                        if app.session.active_spot.is_some() {
                            let _ = app.run("spot.delete", json!({}));
                        }
                        continue;
                    }
                    R::RedEye => continue,
                    _ => {}
                }
                if crate::menus::confirm_delete(app) {
                    continue;
                }
            }
            // X is both reject (library) and swap crop aspect (crop tool)
            if f == "photo.reject" && app.ui.right == crate::state::RightPanel::Crop {
                let _ = app.run("crop.rotateAspect", json!({}));
                continue;
            }
            if f == "crop.rotateAspect" && app.ui.right != crate::state::RightPanel::Crop {
                continue;
            }
            // / refreshes the selected spot's source in the Remove tool (the filmstrip elsewhere)
            if f == "view.filmstrip" && app.ui.right == crate::state::RightPanel::Remove && app.session.active_spot.is_some() {
                let _ = app.run("spot.refreshSource", json!({}));
                continue;
            }
            // Shift+O cycles the mask overlay colour while masking (the crop overlay elsewhere)
            if f == "view.cropOverlay" && app.ui.right == crate::state::RightPanel::Masking {
                let _ = app.run("view.maskOverlayColor", json!({}));
                continue;
            }
            // while cropping: O cycles the guides, Shift+O their orientation, A locks the aspect
            if app.ui.right == crate::state::RightPanel::Crop {
                let crop_key = match f.as_str() {
                    "view.maskOverlay" => Some(("view.cropOverlay", json!({}))),
                    "view.cropOverlay" => Some(("view.cropOverlayOrientation", json!({}))),
                    "view.visualizeSpots" => Some(("crop.aspect", json!({"aspect": "toggle"}))),
                    _ => None,
                };
                if let Some((cmd, p)) = crop_key {
                    let _ = app.run(cmd, p);
                    continue;
                }
            }
            let result = match app.run(&f, json!({})) {
                Ok(result) => result,
                Err(e) => {
                    app.toast_error(ctx, e);
                    continue;
                }
            };
            if matches!(f.as_str(), "photo.pick" | "photo.reject" | "photo.unflag")
                && result.get("changed").and_then(serde_json::Value::as_u64).unwrap_or(0) == 0
            {
                continue;
            }
            match f.as_str() {
                "photo.pick" => app.toast(ctx, "Flagged as Pick"),
                "photo.reject" => app.toast(ctx, "Flagged as Reject"),
                "photo.unflag" => app.toast(ctx, "Unflagged"),
                "edit.undo" => app.toast(ctx, "Undo"),
                "edit.redo" => app.toast(ctx, "Redo"),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bugfix_develop_failed_rating_never_shows_success() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key { key: Key::Num3, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
                ..Default::default()
            },
            |ui| handle(&mut app, ui.ctx()),
        );
        out.textures_delta.clear();
        let text = &app.ui.toast.as_ref().expect("failure is explained").0;
        assert!(!text.starts_with("Rated"), "{text}");
    }

    #[test]
    fn bugfix_develop_classic_keymap() {
        for (command, chord) in [
            ("view.detail", "E"),
            ("view.develop", "D"),
            ("view.compare", "C"),
            ("panel.crop", "R"),
            ("panel.remove", "Q"),
            ("tool.brush", "K"),
            ("view.lightsOut", "L"),
            ("dialog.export", "Cmd+Shift+E"),
            ("photo.editInExternal", "Cmd+E"),
            ("dialog.syncSettings", "Cmd+Shift+S"),
        ] {
            assert_eq!(crate::menus::ui_commands().find(|c| c.0 == command).and_then(|c| c.2), Some(chord), "{command}");
        }
    }

    #[test]
    fn bugfix_develop_effective_chords_have_one_action() {
        // Windows/Linux publish Ctrl as both physical Control and logical Command.
        for modifiers in [Modifiers::CTRL | Modifiers::COMMAND, Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT] {
            for key in [Key::H, Key::E, Key::S] {
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
                        ..Default::default()
                    },
                    |ui| {
                        let fired: Vec<_> = crate::menus::ui_commands()
                            .filter(|c| c.2.and_then(parse).is_some_and(|(m, k)| ui.input(|i| matches(i, m, k))))
                            .map(|c| c.0)
                            .collect();
                        assert!(fired.len() <= 1, "{modifiers:?} {key:?}: {fired:?}");
                    },
                );
                output.textures_delta.clear();
            }
        }
    }

    #[test]
    fn delete_means_the_backspace_key() {
        assert_eq!(parse("Delete"), Some((Modifiers::NONE, Key::Backspace)));
        assert_eq!(parse("Cmd+Delete"), Some((Modifiers::COMMAND, Key::Backspace)));
    }
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let (m, k) = parse("Cmd+Shift+Z").unwrap();
        assert!(m.command && m.shift && !m.alt);
        assert_eq!(k, Key::Z);
        assert_eq!(parse("Right").unwrap().1, Key::ArrowRight);
        assert_eq!(parse("\\").unwrap().1, Key::Backslash);
        // every declared shortcut parses
        for (id, _, sc, _) in crate::menus::ui_commands() {
            if let Some(sc) = sc {
                assert!(parse(sc).is_some(), "{id}: {sc}");
            }
        }
        for c in lightcraft_engine::command_specs() {
            if let Some(sc) = c.shortcut {
                assert!(parse(sc).is_some(), "{}: {sc}", c.id);
            }
        }
    }

    #[test]
    fn aliases_parse_and_target_existing_commands() {
        let ui: Vec<&str> = crate::menus::ui_commands().map(|c| c.0).collect();
        for (sc, id, params) in ALIASES {
            assert!(parse(sc).is_some(), "{id}: {sc}");
            assert!(ui.contains(id) || lightcraft_engine::find_command(id).is_some(), "alias {sc} → unknown command {id}");
            assert!(serde_json::from_str::<serde_json::Value>(params).is_ok(), "alias {sc}: bad params");
        }
    }

    /// Engine commands that intentionally share a key and are disambiguated by context in [`handle`].
    const CONTEXTUAL: &[(&str, &str)] = &[("photo.reject", "crop.rotateAspect")];

    /// No key fires two different actions (a UI command may shadow the engine command it wraps).
    #[test]
    fn no_conflicting_bindings() {
        let mut ui: Vec<((Modifiers, Key), String)> = Vec::new();
        for (id, _, sc, _) in crate::menus::ui_commands() {
            if let Some(k) = sc.and_then(parse) {
                ui.push((k, id.to_string()));
            }
        }
        for (sc, id, _) in ALIASES {
            ui.push((parse(sc).unwrap(), format!("alias {id}")));
        }
        let mut engine: Vec<((Modifiers, Key), &str)> = Vec::new();
        for c in lightcraft_engine::command_specs() {
            if let Some(k) = c.shortcut.and_then(parse) {
                engine.push((k, c.id));
            }
        }
        for (sc, id, _) in ALIASES {
            let k = parse(sc).unwrap();
            assert!(!engine.iter().any(|(k2, _)| *k2 == k), "alias {sc} ({id}) shadows an engine shortcut");
        }
        for (i, (k, a)) in ui.iter().enumerate() {
            for (k2, b) in &ui[i + 1..] {
                assert!(k != k2, "{a} and {b} share a key");
            }
        }
        for (i, (k, a)) in engine.iter().enumerate() {
            for (k2, b) in &engine[i + 1..] {
                let contextual = CONTEXTUAL.iter().any(|(x, y)| (x == a && y == b) || (x == b && y == a));
                assert!(k != k2 || contextual, "{a} and {b} share a key");
            }
        }
    }
}
