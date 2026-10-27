use crate::{LightcraftApp, shortcuts, state::ViewMode};
use egui::{Key, Modifiers};
use serde_json::json;

fn key(app: &mut LightcraftApp, ctx: &egui::Context, key: Key, modifiers: Modifiers) {
    let mut output = ctx.run_ui(
        egui::RawInput { events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }], ..Default::default() },
        |ui| shortcuts::handle(app, ui.ctx()),
    );
    output.textures_delta.clear();
}

#[test]
fn shortcut_and_menu_restore_search_in_grid_and_develop() {
    for profile in ["Lightroom", "Capture One"] {
        for view in [ViewMode::PhotoGrid, ViewMode::Detail] {
            let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
            app.ui.view = view;
            app.shortcut_overrides.insert("library::__profile".into(), profile.into());
            let text = app.session.catalog.photos().find(|photo| photo.rating >= 4).unwrap().file_name.clone();
            app.run("library.filter", json!({"text":text, "rating":4})).unwrap();
            app.ui.search = text.clone();
            let filter = app.session.filter.clone();
            let expected = app.session.visible_cloned();
            assert_eq!(expected.len(), 1);
            assert_eq!(shortcuts::catalog::command_shortcut(&app, "library.toggleFilters").as_deref(), Some("Cmd+L"));
            assert_eq!(super::checked(&app, "library.toggleFilters"), Some(true));
            let ctx = egui::Context::default();
            let modifiers = if cfg!(target_os = "macos") { Modifiers::COMMAND } else { Modifiers::CTRL | Modifiers::COMMAND };
            key(&mut app, &ctx, Key::L, modifiers);
            assert!(app.session.filters_off());
            assert!(app.ui.search.is_empty());
            assert!(app.session.visible().len() > expected.len());
            assert_eq!(super::checked(&app, "library.toggleFilters"), Some(false));
            key(&mut app, &ctx, Key::L, Modifiers::COMMAND);
            assert_eq!(app.session.filter, filter);
            assert_eq!(app.session.visible_cloned(), expected);
            assert_eq!(app.ui.search, text);
            super::run_item(&mut app, "library.toggleFilters", json!({})).unwrap();
            assert!(app.session.filters_off());
            super::run_item(&mut app, "library.toggleFilters", json!({})).unwrap();
            assert_eq!(app.session.filter, filter);
            assert_eq!(app.ui.search, text);
            assert_eq!(super::checked(&app, "library.toggleFilters"), Some(true));
            let layout = super::LAYOUT.iter().find(|(name, _)| *name == "View").unwrap().1;
            assert!(layout.windows(3).any(|items| items == ["view.filterBar", "library.toggleFilters", "library.clearFilter"]));
        }
    }
}

#[test]
fn override_native_and_modal_ownership_prevent_default_dispatch() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.run("library.filter", json!({"rating":4})).unwrap();
    let ctx = egui::Context::default();
    app.shortcut_overrides.insert("library::library.toggleFilters".into(), "Cmd+Shift+L".into());
    assert_eq!(shortcuts::catalog::command_shortcut(&app, "library.toggleFilters").as_deref(), Some("Cmd+Shift+L"));
    key(&mut app, &ctx, Key::L, Modifiers::COMMAND);
    assert!(!app.session.filters_off());
    app.host_modal = true;
    key(&mut app, &ctx, Key::L, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(!app.session.filters_off());
    app.host_modal = false;
    app.native_shortcuts.insert("Cmd+Shift+L".into());
    key(&mut app, &ctx, Key::L, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(!app.session.filters_off());
    app.native_shortcuts.clear();
    key(&mut app, &ctx, Key::L, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(app.session.filters_off());
}

#[test]
fn focused_search_keeps_its_keys() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.run("library.filter", json!({"text":"atrium"})).unwrap();
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut app.ui.search)).request_focus();
    });
    output.textures_delta.clear();
    key(&mut app, &ctx, Key::L, Modifiers::COMMAND);
    assert!(!app.session.filters_off());
    assert_eq!(app.session.filter.text, "atrium");
}
