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
        // The assertions above are headless; the screenshot needs a real GPU and fails without one.
        let _gpu = crate::gpu_test_support::lock();
        h.render().unwrap_or_else(|e| panic!("real GPU required for the shortcut screenshot @{ppp}x: {e}"));
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
        let want = li_keys::input::shortcut_text(Key::J, held).unwrap();
        assert_eq!(field(&h, id, "overrides")["edit.search"], json!(want), "{modifier:?}");
    }
    for k in [Key::ControlLeft, Key::ShiftLeft, Key::AltLeft, Key::SuperRight] {
        assert_eq!(li_keys::input::shortcut_text(k, Modifiers::COMMAND), None, "{k:?}");
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
        crate::shortcut_dispatch::bindings(app).into_iter().filter(|(_, sc)| *sc == li_keys::input::parse("Cmd+J").unwrap()).map(|(id, _)| id).collect();
    assert_eq!(bound, ["edit.search"], "one owner for ⌘J");
}

#[test]
fn every_named_key_remains_dispatchable_after_preference_normalization() {
    for key in Key::ALL {
        if !li_keys::input::is_modifier_key(*key) {
            let captured = li_keys::input::shortcut_text(*key, Modifiers::ALT).unwrap();
            assert_eq!(li_keys::input::parse(&captured).unwrap().logical_key, *key, "capture {key:?}");
        }
        let raw = format!("Alt+{}", key.name());
        let normalized = li_keys::normalize_shortcut(&raw).unwrap_or_else(|| panic!("key vocabulary missing {raw}"));
        assert_eq!(li_keys::input::parse(&normalized).unwrap().logical_key, *key, "{raw}");
        assert_eq!(li_keys::input::parse(&raw.to_ascii_lowercase()).unwrap().logical_key, *key, "old persisted {raw}");
    }
    assert!(li_keys::input::parse("Cmd+Pageup").is_some());
    assert!(li_keys::input::parse("Cmd+Banana").is_none());
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
        let expected = li_keys::input::shortcut_text(k, held).unwrap();
        assert_eq!(field(&h, id, "overrides")["edit.search"], json!(expected));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        h.run_steps(2);
        let stored = h.state().session.prefs().to_json();
        let mut reload = photocraft_engine::Session::new();
        reload.load_prefs_json(&serde_json::to_string(&stored).unwrap()).unwrap();
        let shortcut = reload.prefs().shortcut("edit.search", None).unwrap();
        assert_eq!(li_keys::input::parse(shortcut).unwrap().logical_key, k);
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
fn shortcut_dialog_preserves_unknown_entries_without_clearing_other_bindings() {
    for change in [false, true] {
        let (mut h, id) = dialog(1.0, "edit.search", "search…");
        h.state_mut()
            .session
            .load_prefs_json(r#"{"shortcutProfiles":{"editor":{"active":"Mine","custom":{"Mine":{"parent":"Photoshop","overrides":{"edit.search":"Alt+F35","edit.undo":"","future.command":"Alt+FutureKey"}}}}}}"#)
            .unwrap();
        let mut overrides = h.state().session.prefs().shortcut_overrides(false).clone();
        if change {
            overrides.insert("edit.search".into(), "Alt+F34".into());
        }
        // The dialog shows the profiles as loaded (it reads them when it opens).
        let profiles = json!(h.state().session.prefs().shortcut_profiles);
        let fields = &mut h.state_mut().ui.dialog_mut(id).unwrap().fields;
        fields.insert("profiles".into(), profiles);
        fields.insert("overrides".into(), json!(overrides));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        assert_eq!(h.state().session.prefs().shortcut_profiles.editor.active, "Mine");
        assert_eq!(h.state().session.prefs().shortcut_overrides(false), &overrides);
        assert_eq!(h.state().session.prefs().shortcut("layer.new.layerViaCopy", Some("Cmd+J")), Some("Cmd+J"));
    }
}

fn capture_key(h: &mut Harness<'_, PhotocraftApp>, id: u64, k: Key, modifiers: Modifiers) {
    h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("capture".into(), json!(true));
    h.run_steps(2);
    key(h, k, true, modifiers);
    h.run_steps(2);
    key(h, k, false, modifiers);
    h.run_steps(2);
}

#[test]
fn shortcut_conflict_accept_moves_every_holder_and_selects_the_conflict() {
    let (mut h, id) = dialog(1.0, "edit.paste", "");
    h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"edit.copy": "Cmd+X"}));
    h.run_steps(2);
    capture_key(&mut h, id, Key::X, Modifiers::COMMAND);
    assert_eq!(field(&h, id, "conflict")["holders"].as_array().unwrap().len(), 2);
    let target = field(&h, id, "conflict")["with"].clone();
    h.get_by_label("Accept and Go to Conflict").click();
    h.run_steps(3);
    let overrides = field(&h, id, "overrides");
    assert_eq!(overrides["edit.cut"], "");
    assert_eq!(overrides["edit.copy"], "");
    assert_eq!(overrides["edit.paste"], "Cmd+X");
    assert_eq!(field(&h, id, "selected"), target);
    assert!(field(&h, id, "conflict").is_null());
}

#[test]
fn shortcut_conflict_undo_restores_the_original_binding() {
    for original in [None, Some("Cmd+Shift+X")] {
        let (mut h, id) = dialog(1.0, "edit.cut", "");
        if let Some(original) = original {
            h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"edit.cut": original}));
        }
        h.run_steps(2);
        capture_key(&mut h, id, Key::C, Modifiers::COMMAND);
        h.get_by_label("Undo Changes").click();
        h.run_steps(3);
        assert_eq!(field(&h, id, "overrides").get("edit.cut").and_then(Value::as_str), original);
        assert!(field(&h, id, "conflict").is_null());
    }
}

#[test]
fn system_shortcut_capture_keeps_the_existing_binding() {
    let (mut h, id) = dialog(1.0, "edit.cut", "");
    let (k, modifiers) = if cfg!(target_os = "macos") { (Key::Tab, Modifiers::COMMAND | Modifiers::MAC_CMD) } else { (Key::F4, Modifiers::ALT) };
    capture_key(&mut h, id, k, modifiers);
    assert!(field(&h, id, "overrides").get("edit.cut").is_none());
    assert!(field(&h, id, "message").as_str().unwrap().contains("reserved"));
}

#[test]
fn shortcut_conflict_is_discarded_after_reset_or_escape() {
    for action in ["Reset All to Defaults", "Use Default", "escape"] {
        let (mut h, id) = dialog(1.0, "edit.cut", "");
        capture_key(&mut h, id, Key::C, Modifiers::COMMAND);
        assert!(field(&h, id, "conflict").is_object());
        if action == "escape" {
            capture_key(&mut h, id, Key::Escape, Modifiers::NONE);
        } else {
            h.get_by_label(action).click();
            h.run_steps(3);
        }
        assert!(field(&h, id, "conflict").is_null(), "{action}");
    }
}
