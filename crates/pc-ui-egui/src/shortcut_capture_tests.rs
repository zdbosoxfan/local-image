//! #292: Edit › Keyboard Shortcuts records the key pressed with the modifiers, not the
//! modifier key itself (egui reports Ctrl, Shift, Alt and ⌘ as key presses too), so Ctrl+F and
//! any other non-system combination can be bound; a shortcut already in use is reported.

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::PhotocraftApp;

fn key(h: &mut Harness<'_, PhotocraftApp>, key: Key, pressed: bool, modifiers: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers });
}

/// The Keyboard Shortcuts dialog with `command` selected, filtered to it.
fn dialog(ppp: f32, command: &str, filter: &str) -> (Harness<'static, PhotocraftApp>, u64) {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 760.0)).with_pixels_per_point(ppp).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
    });
    let ctx = h.ctx.clone();
    let id = crate::menus::invoke(h.state_mut(), &ctx, "edit.keyboardShortcuts", json!({})).unwrap()["dialog"].as_u64().unwrap();
    let f = &mut h.state_mut().ui.dialog_mut(id).unwrap().fields;
    f.insert("filter".into(), json!(filter));
    f.insert("selected".into(), json!(command));
    h.run_steps(4);
    (h, id)
}

fn field(h: &Harness<'_, PhotocraftApp>, id: u64, name: &str) -> Value {
    h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields.get(name).cloned().unwrap_or(Value::Null)
}

/// Search is intentionally unbound by default: Ctrl/Cmd+K now opens Preferences.
/// Locate the selected command's actual capture button, rather than assuming its
/// old default label. Exercise real pointer input even when that label is empty.
fn click_capture(h: &mut Harness<'_, PhotocraftApp>, command: &str) {
    let rect = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", command)))).unwrap();
    assert!(rect.width() > 0. && rect.height() > 0.);
    assert!(h.ctx.content_rect().contains_rect(rect), "capture button must be fully visible: {command}: {rect:?}");
    let pos = rect.center();
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

/// Click the selected command's shortcut button, then press `modifier` and `k` like a keyboard:
/// the modifier key goes down first (its own key event), then the key with the modifier held.
fn capture(h: &mut Harness<'_, PhotocraftApp>, command: &str, modifier: Key, held: Modifiers, k: Key) {
    click_capture(h, command);
    key(h, modifier, true, held);
    h.run_steps(1);
    key(h, k, true, held);
    h.run_steps(1);
    key(h, k, false, held);
    key(h, modifier, false, Modifiers::NONE);
    h.run_steps(2);
}

#[test]
fn ctrl_f_is_recorded_as_ctrl_f_not_the_control_key() {
    for ppp in [1.0, 2.0] {
        let (mut h, id) = dialog(ppp, "edit.search", "search…");
        // On Windows and Linux the Ctrl key is the command modifier.
        capture(&mut h, "edit.search", Key::ControlLeft, Modifiers::COMMAND, Key::F);
        assert_eq!(field(&h, id, "overrides")["edit.search"], json!("Cmd+F"), "@{ppp}x");
        assert_eq!(field(&h, id, "capture"), json!(false));
        let message = field(&h, id, "message");
        assert!(!message.as_str().unwrap().contains("ontrol"), "@{ppp}x: {message}");
        // The row shows the new shortcut.
        h.get_by_label(&crate::shortcuts::pretty("Cmd+F"));
        // The assertions above are headless; the optional screenshot needs an adapter.
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render().ok())) {
            let message = panic.downcast_ref::<String>().map(String::as_str).or_else(|| panic.downcast_ref::<&str>().copied());
            if message.is_some_and(|m| m.contains("No adapter found")) {
                eprintln!("skipped: no GPU adapter for shortcut screenshot");
            } else {
                std::panic::resume_unwind(panic);
            }
        }
    }
}

#[test]
fn every_modifier_key_waits_for_the_real_key() {
    for (modifier, held) in [
        (Key::ControlRight, Modifiers::COMMAND),
        (Key::ShiftLeft, Modifiers::SHIFT),
        (Key::AltRight, Modifiers::ALT),
        (Key::SuperLeft, Modifiers::COMMAND),
        (Key::ShiftRight, Modifiers::COMMAND | Modifiers::SHIFT),
    ] {
        let (mut h, id) = dialog(1.0, "edit.search", "search…");
        click_capture(&mut h, "edit.search");
        key(&mut h, modifier, true, held);
        h.run_steps(2);
        assert_eq!(field(&h, id, "capture"), json!(true), "{modifier:?} alone keeps capturing");
        assert!(field(&h, id, "overrides").get("edit.search").is_none(), "{modifier:?} alone assigns nothing");
        key(&mut h, Key::J, true, held);
        h.run_steps(2);
        let want = crate::prefs_ui::shortcut_text(Key::J, held).unwrap();
        assert_eq!(field(&h, id, "overrides")["edit.search"], json!(want), "{modifier:?}");
    }
    for k in [Key::ControlLeft, Key::ShiftLeft, Key::AltLeft, Key::SuperRight] {
        assert_eq!(crate::prefs_ui::shortcut_text(k, Modifiers::COMMAND), None, "{k:?}");
    }
}

#[test]
fn a_shortcut_in_use_is_reported_and_moved_on_ok() {
    // ⌘J is Layer › New › Layer via Copy: giving it to Search warns, and OK moves it.
    let (mut h, id) = dialog(1.0, "edit.search", "search…");
    capture(&mut h, "edit.search", Key::ControlLeft, Modifiers::COMMAND, Key::J);
    let message = field(&h, id, "message");
    assert!(message.as_str().unwrap().contains("already in use"), "{message}");
    crate::dialogs::confirm(h.state_mut(), id).unwrap();
    h.run_steps(2);
    let app = h.state();
    assert_eq!(app.session.prefs().shortcut("edit.search", Some("Cmd+K")), Some("Cmd+J"));
    let bound: Vec<String> =
        crate::shortcut_dispatch::bindings(app).into_iter().filter(|(_, sc)| *sc == crate::shortcuts::parse("Cmd+J").unwrap()).map(|(id, _)| id).collect();
    assert_eq!(bound, ["edit.search"], "one owner for ⌘J");
}

#[test]
fn every_named_key_remains_dispatchable_after_preference_normalization() {
    for key in Key::ALL {
        if !crate::prefs_ui::is_modifier_key(*key) {
            let captured = crate::prefs_ui::shortcut_text(*key, Modifiers::ALT).unwrap();
            assert_eq!(crate::shortcuts::parse(&captured).unwrap().logical_key, *key, "capture {key:?}");
        }
        let raw = format!("Alt+{}", key.name());
        let normalized = photocraft_engine::prefs::normalize_shortcut(&raw).unwrap_or_else(|| panic!("key vocabulary missing {raw}"));
        assert_eq!(crate::shortcuts::parse(&normalized).unwrap().logical_key, *key, "{raw}");
        assert_eq!(crate::shortcuts::parse(&raw.to_ascii_lowercase()).unwrap().logical_key, *key, "old persisted {raw}");
    }
    assert!(crate::shortcuts::parse("Cmd+Pageup").is_some());
    assert!(crate::shortcuts::parse("Cmd+Banana").is_none());
}

#[test]
fn named_shortcuts_capture_persist_reload_and_dispatch() {
    for (k, held, modifier) in [
        (Key::PageUp, Modifiers::COMMAND, Key::ControlLeft),
        (Key::PageDown, Modifiers::COMMAND, Key::ControlLeft),
        (Key::OpenCurlyBracket, Modifiers::SHIFT, Key::ShiftLeft),
        (Key::CloseCurlyBracket, Modifiers::SHIFT, Key::ShiftLeft),
    ] {
        let (mut h, id) = dialog(1.0, "edit.search", "search…");
        capture(&mut h, "edit.search", modifier, held, k);
        let expected = crate::prefs_ui::shortcut_text(k, held).unwrap();
        assert_eq!(field(&h, id, "overrides")["edit.search"], json!(expected));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        h.run_steps(2);
        let stored = h.state().session.prefs().to_json();
        let mut reload = photocraft_engine::Session::new();
        reload.load_prefs_json(&serde_json::to_string(&stored).unwrap()).unwrap();
        let shortcut = reload.prefs().shortcut("edit.search", None).unwrap();
        assert_eq!(crate::shortcuts::parse(shortcut).unwrap().logical_key, k);
        let binding = crate::shortcut_dispatch::bindings(h.state()).into_iter().find(|(id, _)| id == "edit.search").unwrap().1;
        assert_eq!(binding.logical_key, k);
        // The captured key actually opens Search after the dialog is gone.
        key(&mut h, k, true, held);
        h.run_steps(2);
        key(&mut h, k, false, Modifiers::NONE);
        h.run_steps(2);
        assert!(h.state().ui.palette_open, "recorded {expected} did not dispatch");
    }
}

#[test]
fn shortcut_dialog_preserves_unknown_legacy_entries_without_clearing_other_bindings() {
    for change in [false, true] {
        let (mut h, id) = dialog(1.0, "edit.search", "search…");
        h.state_mut().session.load_prefs_json(r#"{"shortcuts":{"edit.search":"Alt+F35","edit.undo":"","future.command":"Alt+FutureKey"}}"#).unwrap();
        let mut overrides = h.state().session.prefs().shortcuts.clone();
        if change {
            overrides.insert("edit.search".into(), "Alt+F34".into());
        }
        h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!(overrides));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        assert_eq!(h.state().session.prefs().shortcuts, overrides);
        assert_eq!(h.state().session.prefs().shortcut("layer.new.layerViaCopy", Some("Cmd+J")), Some("Cmd+J"));
    }
}
