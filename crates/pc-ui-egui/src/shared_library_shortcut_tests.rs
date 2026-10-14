#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
fn app(store: &Arc<Mutex<Option<String>>>, fail: &Arc<AtomicBool>) -> PhotocraftApp {
    let read = store.clone();
    let write = store.clone();
    let fail = fail.clone();
    PhotocraftApp::new(
        photocraft_engine::Session::new(),
        crate::Services {
            load_prefs: Some(Box::new(move || read.lock().unwrap().clone())),
            save_prefs: Some(Box::new(move |s| {
                if fail.load(Ordering::SeqCst) {
                    return Err("test storage unavailable".into());
                }
                *write.lock().unwrap() = Some(s.into());
                Ok(())
            })),
            ..Default::default()
        },
    )
}
fn rows() -> Vec<HostShortcutItem> {
    vec![
        ("library::view.develop".into(), "Develop".into(), vec!["Library".into(), "View".into()], Some("D".into())),
        ("library::view.detail".into(), "Loupe".into(), vec!["Library".into(), "View".into()], Some("E".into())),
        ("library::photo.saveMetadataToFile".into(), "Save Metadata to File".into(), vec!["Library".into(), "Photo".into()], Some("Cmd+S".into())),
    ]
}
#[test]
fn shared_shell_namespace_does_not_shadow_editor_commands_or_tools() {
    let mut a = app(&Arc::default(), &Arc::new(AtomicBool::new(false)));
    for persona in [photocraft_engine::toolsets::Persona::Pixel, photocraft_engine::toolsets::Persona::Vector] {
        a.session.edit_prefs(|p| {
            p.toolbar.persona = persona;
            p.shortcut_profiles.clear_overrides();
        });
        let before = crate::shortcut_dispatch::binding_strings(&a);
        let tool = crate::shortcuts::tool_shortcut(&a.session, crate::state::Tool::Move);
        a.session.edit_prefs(|p| {
            p.set_shortcut("library::file.open", Some("Cmd+N"));
            p.set_shortcut("library::unknown-legacy", Some("V"));
        });
        assert_eq!(crate::shortcut_dispatch::binding_strings(&a), before);
        assert_eq!(crate::shortcuts::tool_shortcut(&a.session, crate::state::Tool::Move), tool);
    }
}
#[test]
fn shared_shell_shortcut_catalog_confirm_restart_cancel_and_failed_save() {
    let store = Arc::default();
    let fail = Arc::new(AtomicBool::new(false));
    let mut a = app(&store, &fail);
    let ctx = egui::Context::default();
    let id = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    assert_eq!(a.ui.dialog_mut(id).unwrap().fields["__hostShortcuts"][0][1], "Develop");
    a.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"library::view.develop":"Cmd+N"}));
    crate::dialogs::confirm(&mut a, id).unwrap();
    assert_eq!(a.session.prefs().shortcut_overrides(true)["library::view.develop"], "Cmd+N");
    assert!(!a.session.prefs().shortcut_overrides(false).contains_key("file.new"), "disjoint module command must keep its default");
    for time in [0., 2., 4.] {
        let _ = persist(&mut a, time);
    }
    let saved = store.lock().unwrap().clone().unwrap();
    let mut restarted = app(&store, &fail);
    assert_eq!(restarted.session.prefs().shortcut_overrides(true)["library::view.develop"], "Cmd+N");
    let id = open_shortcuts(&mut restarted, 0);
    set_host_shortcut_catalog(&mut restarted, &ctx, rows());
    restarted.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"library::view.develop":"F6"}));
    crate::dialogs::cancel(&mut restarted, id).unwrap();
    assert_eq!(restarted.session.prefs().shortcut_overrides(true)["library::view.develop"], "Cmd+N");
    let id = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    a.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"library::view.develop":"F7"}));
    crate::dialogs::confirm(&mut a, id).unwrap();
    fail.store(true, Ordering::SeqCst);
    for time in [10., 12., 16., 24.] {
        let _ = persist(&mut a, time);
    }
    assert_eq!(store.lock().unwrap().as_ref(), Some(&saved));
    assert!(a.ui.notices.iter().any(|n| n.error && n.title == "Preferences can't be saved"));
}
#[test]
fn shared_shell_library_capture_is_same_editor_and_scoped_collision() {
    let mut a = app(&Arc::default(), &Arc::new(AtomicBool::new(false)));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    crate::theme::apply(&ctx, crate::theme::ThemeKind::Pro);
    let id = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    let mut fields = a.ui.dialog_mut(id).unwrap().fields.clone();
    fields.insert("filter".into(), json!("library::view.develop"));
    fields.insert("shortcutScope".into(), json!("library"));
    let key = "library::view.develop".to_string();
    let mut frame = 0;
    let mut draw = |events: Vec<egui::Event>, f: &mut Map<String, Value>| {
        frame += 1;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1000., 800.))),
            time: Some(frame as f64 / 60.),
            ..Default::default()
        };
        raw.events = events;
        ctx.run_ui(raw, |ui| shortcuts_body(&mut a, ui, f)).textures_delta.clear();
    };
    draw(vec![], &mut fields);
    draw(vec![], &mut fields);
    let hit = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", &key)))).unwrap();
    let pos = hit.center();
    for pressed in [true, false] {
        draw(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ],
            &mut fields,
        );
    }
    assert_eq!(fields["capture"], true);
    draw(
        vec![egui::Event::Key {
            key: egui::Key::Num3,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND | egui::Modifiers::ALT,
        }],
        &mut fields,
    );
    assert_eq!(fields["overrides"][&key], "Cmd+Alt+3");
    assert!(fields["message"].as_str().unwrap().contains("Conflict with Global"));
    assert!(fields["message"].as_str().unwrap().contains("ignored"));
    assert_eq!(crate::keymaps::global_conflict("Cmd+Alt+3"), Some("Editor workspace"));
    fields.insert("capture".into(), json!(true));

    draw(vec![egui::Event::Key { key: egui::Key::N, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }], &mut fields);
    assert_eq!(fields["overrides"][&key], "Cmd+N");
    assert!(!fields["message"].as_str().unwrap().contains("already in use"), "Compositing Ctrl+N is a different scope");
    fields.insert("capture".into(), json!(true));
    draw(vec![egui::Event::Key { key: egui::Key::E, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }], &mut fields);
    assert!(fields["message"].as_str().unwrap().contains("Loupe"));
    assert!(!fields["message"].as_str().unwrap().contains("library::"));
    fields["overrides"]["library::view.detail"] = json!("F8");
    fields.insert("capture".into(), json!(true));
    draw(vec![egui::Event::Key { key: egui::Key::F8, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }], &mut fields);
    assert_eq!(fields["overrides"]["library::view.detail"], "");
    assert_eq!(fields["overrides"][&key], "F8", "reassignment wins over a previously customized same-module command");

    for action in ["delete", "default"] {
        let rect = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-action", action)))).unwrap();
        let pos = rect.center();
        for pressed in [true, false] {
            draw(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                ],
                &mut fields,
            );
        }
        if action == "delete" {
            assert_eq!(fields["overrides"][&key], "");
        } else {
            assert!(fields["overrides"].get(&key).is_none());
        }
    }
    let before = fields["overrides"].clone();
    fields.insert("capture".into(), json!(true));
    draw(vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }], &mut fields);
    assert_eq!(fields["capture"], false);
    assert_eq!(fields["overrides"], before);
    fields["overrides"]["library::view.detail"] = json!("Ctrl+N");
    fields.insert("capture".into(), json!(true));
    draw(vec![egui::Event::Key { key: egui::Key::N, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }], &mut fields);
    assert_eq!(
        fields["overrides"]["library::view.detail"],
        if cfg!(target_os = "macos") { "Ctrl+N" } else { "" },
        "shared capture uses platform-equivalent chord identity just like Library dispatch"
    );
}
#[test]
fn shared_shell_reserved_host_chords_refuse_new_conflicts_and_keep_defaults() {
    let id = "library::view.develop";
    assert!(same_chord_in_scope(id, "Ctrl+Delete", "Ctrl+Backspace"));
    assert!(reserved_library_chord(id, "Cmd+Alt+3").is_some());
    assert!(reserved_library_chord(id, "Cmd+S").is_some());
    assert!(reserved_library_chord("library::photo.saveMetadataToFile", "Cmd+S").is_none());
    assert!(reserved_library_chord("library::dialog.syncSettings", "Cmd+Shift+S").is_none());
    assert!(reserved_library_chord(id, "Cmd+Alt+Shift+3").is_none());
    if !cfg!(target_os = "macos") {
        assert!(reserved_library_chord(id, "ctrl+alt+3").is_some());
    }
    let mut a = app(&Arc::default(), &Arc::new(AtomicBool::new(false)));
    let ctx = egui::Context::default();
    let dialog = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    let before = a.session.prefs().shortcut_profiles.clone();
    a.ui.dialog_mut(dialog).unwrap().fields.insert("overrides".into(), json!({id:"Cmd+S"}));
    assert!(crate::dialogs::confirm(&mut a, dialog).unwrap_err().contains("reserved"));
    assert_eq!(a.session.prefs().shortcut_profiles, before);
    // Generic confirm removes the dialog before applying; the actual capture UI
    // refuses a reserved chord before insertion (covered by the input test).
    assert!(a.ui.dialog_mut(dialog).is_none());
    let dialog = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    assert_eq!(a.ui.dialog_mut(dialog).unwrap().fields["__hostShortcuts"][0][1], "Develop");
    a.ui.dialog_mut(dialog).unwrap().fields.insert("overrides".into(), json!({id:"F6"}));
    crate::dialogs::cancel(&mut a, dialog).unwrap();
    assert_eq!(a.session.prefs().shortcut_profiles, before, "cancelling a valid new draft after the failed apply still leaves bindings unchanged");
    // Already persisted values survive unrelated preference review; users can see and repair them.
    a.session.edit_prefs(|p| {
        p.set_shortcut(id, Some("Cmd+S"));
    });
    let dialog = open_shortcuts(&mut a, 0);
    set_host_shortcut_catalog(&mut a, &ctx, rows());
    crate::dialogs::confirm(&mut a, dialog).unwrap();
    assert_eq!(a.session.prefs().shortcut_overrides(true)[id], "Cmd+S");
    assert!(
        !a.session.prefs().shortcut_overrides(true).contains_key("library::photo.saveMetadataToFile"),
        "reviewing retained bindings must not disable a genuine default"
    );
}
