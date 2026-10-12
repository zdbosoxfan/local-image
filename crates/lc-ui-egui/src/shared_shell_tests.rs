#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use serde_json::json;
fn press(app: &mut LightcraftApp, ctx: &egui::Context, key: egui::Key, mods: egui::Modifiers) {
    for pressed in [true, false] {
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods });
        ctx.run_ui(raw, |ui| shortcuts::handle(app, ui.ctx())).textures_delta.clear();
    }
}
#[test]
fn shared_shell_library_rebind_unbind_reset_dispatch_and_labels() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    let ctx = egui::Context::default();
    let key = "library::view.develop";
    assert!(shortcuts::parse("unknown+F6").is_none());
    assert!(shortcuts::parse("A+F6").is_none());
    assert_eq!(shortcuts::parse("Cmd++").unwrap(), (egui::Modifiers::COMMAND, egui::Key::Plus));
    app.ui.view = state::ViewMode::PhotoGrid;
    press(&mut app, &ctx, egui::Key::D, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::Detail);
    app.shortcut_overrides.insert(key.into(), "F6".into());
    app.ui.view = state::ViewMode::PhotoGrid;
    press(&mut app, &ctx, egui::Key::D, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::PhotoGrid);
    press(&mut app, &ctx, egui::Key::F6, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::Detail);
    assert_eq!(menus::menu_entries(&app).iter().find(|e| e.id == "view.develop").unwrap().shortcut.as_deref(), Some("F6"));
    shortcuts::publish_labels(&app, &ctx);
    assert_eq!(shortcuts::label_in(&ctx, "view.develop").as_deref(), Some("F6"));
    let saved = serde_json::to_vec(&app.shortcut_overrides).unwrap();
    app.shortcut_overrides = serde_json::from_slice(&saved).unwrap();
    app.shortcut_overrides.insert(key.into(), String::new());
    app.ui.view = state::ViewMode::PhotoGrid;
    press(&mut app, &ctx, egui::Key::F6, egui::Modifiers::NONE);
    press(&mut app, &ctx, egui::Key::D, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::PhotoGrid);
    shortcuts::publish_labels(&app, &ctx);
    assert_eq!(shortcuts::label_in(&ctx, "view.develop"), None);
    app.shortcut_overrides.remove(key);
    app.shortcut_overrides.insert("library::obsolete-command".into(), "D".into());
    press(&mut app, &ctx, egui::Key::D, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::Detail, "unknown retained keys never reserve live bindings");
    assert!(menus::menu_entries(&app).iter().any(|e| e.id == "app.settings" && e.label == "Preferences…"));
}
#[test]
fn shared_shell_library_aliases_grid_and_modal_share_binding_authority() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    let ctx = egui::Context::default();
    let photo = app.session.active().unwrap();
    let alias = shortcuts::catalog::catalog()
        .into_iter()
        .find(|b| b.command == "photo.flag" && b.params.as_ref().is_some_and(|p| p["flag"] == "pick"))
        .unwrap();
    app.shortcut_overrides.insert(alias.key, "F8".into());
    press(&mut app, &ctx, egui::Key::F8, egui::Modifiers::NONE);
    assert_eq!(app.session.catalog.photo(photo).unwrap().flag, lightcraft_catalog::Flag::Pick);
    app.shortcut_overrides.insert("library::grid.down".into(), "F9".into());
    let mut raw = egui::RawInput::default();
    raw.events.push(egui::Event::Key { key: egui::Key::F9, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    let mut direction = None;
    ctx.run_ui(raw, |ui| {
        direction = shortcuts::catalog::consume_grid(&app, ui.ctx());
    })
    .textures_delta
    .clear();
    assert_eq!(direction, Some(egui::Key::ArrowDown));
    app.host_modal = true;
    app.ui.view = state::ViewMode::PhotoGrid;
    let selected = app.session.active();
    press(&mut app, &ctx, egui::Key::D, egui::Modifiers::NONE);
    press(&mut app, &ctx, egui::Key::ArrowRight, egui::Modifiers::NONE);
    assert_eq!(app.ui.view, state::ViewMode::PhotoGrid);
    assert_eq!(app.session.active(), selected);
    app.shared_shortcuts = true;
    app.run("app.shortcuts", json!({})).unwrap();
    assert!(app.open_shared_shortcuts);
    assert!(app.ui.dialog.is_none());
}

#[test]
fn shared_shell_platform_equivalent_legacy_chord_dispatches_once() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    let ctx = egui::Context::default();
    let active = app.session.active();
    app.shortcut_overrides.insert("library::view.develop".into(), "Ctrl+D".into());
    app.ui.view = state::ViewMode::PhotoGrid;
    let mods = if cfg!(target_os = "macos") { egui::Modifiers::CTRL } else { egui::Modifiers::CTRL | egui::Modifiers::COMMAND };
    press(&mut app, &ctx, egui::Key::D, mods);
    assert_eq!(app.ui.view, state::ViewMode::Detail);
    assert_eq!(app.session.active(), active, "default Cmd+D Select None must not also execute on Linux/Windows");
}
struct SharedHoldRenders;
impl crate::render::RenderOffload for SharedHoldRenders {
    fn try_start(&mut self, _: crate::render::Slot, job: lightcraft_engine::media::RenderJob) -> Option<lightcraft_engine::media::RenderJob> {
        Some(job)
    }
    fn finished(&mut self) -> Vec<(crate::render::Slot, lightcraft_engine::media::RenderResult, f64)> {
        Vec::new()
    }
}
fn real_frame(app: &mut LightcraftApp, view: &mut crate::headless::HeadlessView, tick: &mut u64, events: Vec<egui::Event>) {
    *tick += 1;
    let raw = crate::headless::HeadlessView::raw_input(egui::vec2(900., 700.), 1., *tick as f64 / 60., events);
    view.run(raw, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
}
fn key_events(key: egui::Key, mods: egui::Modifiers, pressed: bool) -> Vec<egui::Event> {
    vec![egui::Event::ModifiersChanged(mods), egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods }]
}
#[test]
fn shared_shell_specific_chords_never_also_navigate_or_show_original() {
    use crate::render::{Slot, Tex};
    use std::sync::Arc;
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.ui.view = state::ViewMode::PhotoGrid;
    app.ui.left_panel = false;
    app.ui.right = state::RightPanel::None;
    app.ui.thumb_size = 160.;
    let first = app.session.visible_cloned()[0];
    app.run("library.select", json!({"ids":[first.0]})).unwrap();
    app.renderer.set_offload(Box::new(SharedHoldRenders));
    app.shortcut_overrides.insert("library::grid.down".into(), "F6".into());
    app.shortcut_overrides.insert("library::rate:5:0".into(), "Shift+F6".into());
    let mut view = crate::headless::HeadlessView::new();
    let mut tick = 0;
    for _ in 0..4 {
        real_frame(&mut app, &mut view, &mut tick, vec![]);
    }
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::SHIFT, true));
    assert_eq!(app.session.active(), Some(first));
    assert_eq!(app.session.catalog.photo(first).unwrap().rating, 5, "more-specific chord must still execute its intended rating");
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, false));
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, true));
    assert_ne!(app.session.active(), Some(first), "exact plain chord still moves to the next visual row");
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, false));
    app.run("view.detail", json!({})).unwrap();
    app.shortcut_overrides.clear();
    app.shortcut_overrides.insert("library::view.showOriginal".into(), "F6".into());
    app.shortcut_overrides.insert("library::rate:4:0".into(), "Shift+F6".into());
    let photo = app.session.active().unwrap();
    for (slot, color) in [(Slot::Main, egui::Color32::RED), (Slot::Before, egui::Color32::BLUE)] {
        let pixels = Arc::new(egui::ColorImage::filled([2, 2], color));
        let tex = view.ctx.load_texture(format!("specific-{slot:?}"), pixels.clone(), egui::TextureOptions::NEAREST);
        app.renderer.textures.insert(slot, Tex { key: 0, photo, tex, size: [2, 2], histogram: None, ms: 0., quick: None, pixels: Some(pixels) });
    }
    let pixel = |app: &LightcraftApp, view: &crate::headless::HeadlessView| {
        let p = app.image_rect.unwrap().center();
        view.paint(&app.renderer.cpu_textures())[(p.x as usize, p.y as usize)]
    };
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::SHIFT, true));
    assert_eq!(app.session.catalog.photo(photo).unwrap().rating, 4);
    assert_eq!(pixel(&app, &view), egui::Color32::RED, "Shift+F6 must not display Original bound to F6");
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, false));
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, true));
    assert_eq!(pixel(&app, &view), egui::Color32::BLUE);
    real_frame(&mut app, &mut view, &mut tick, key_events(egui::Key::F6, egui::Modifiers::NONE, false));
    assert_eq!(pixel(&app, &view), egui::Color32::RED);
}
#[test]
fn shared_shell_visible_footer_back_hints_and_status_preferences_follow_bindings() {
    fn text(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().into()),
            egui::Shape::Vec(v) => {
                for s in v {
                    text(s, out)
                }
            }
            _ => {}
        }
    }
    fn draw(app: &mut LightcraftApp, ctx: &egui::Context, tick: &mut u64, events: Vec<egui::Event>) -> Vec<String> {
        *tick += 1;
        let raw = crate::headless::HeadlessView::raw_input(egui::vec2(760., 480.), 1., *tick as f64 / 5., events);
        let mut out = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        let mut words = Vec::new();
        for s in &out.shapes {
            text(&s.shape, &mut words);
        }
        out.textures_delta.clear();
        words
    }
    fn rect(app: &LightcraftApp, key: &str) -> egui::Rect {
        app.widgets.iter().find(|(id, _)| id == key).unwrap().1
    }
    fn click(app: &mut LightcraftApp, ctx: &egui::Context, tick: &mut u64, key: &str) -> Vec<String> {
        let p = rect(app, key).center();
        for pressed in [true, false] {
            draw(
                app,
                ctx,
                tick,
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                ],
            );
        }
        draw(app, ctx, tick, vec![])
    }
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.renderer.set_offload(Box::new(SharedHoldRenders));
    app.run("view.develop", json!({})).unwrap();
    app.ui.left_panel = false;
    app.shortcut_overrides.extend([
        ("library::view.detail".into(), "F7".into()),
        ("library::view.back".into(), "F8".into()),
        ("library::view.compare".into(), String::new()),
        ("library::view.filmstrip".into(), "F9".into()),
    ]);
    let ctx = egui::Context::default();
    let mut tick = 0;
    for _ in 0..4 {
        draw(&mut app, &ctx, &mut tick, vec![]);
    }
    for (id, label) in [("icon:back", "Back (F8)"), ("icon:filmstrip", "Filmstrip (F9)")] {
        let p = rect(&app, id).center();
        let mut words = Vec::new();
        draw(&mut app, &ctx, &mut tick, vec![egui::Event::PointerMoved(p)]);
        for _ in 0..8 {
            words = draw(&mut app, &ctx, &mut tick, vec![]);
        }
        assert!(words.iter().any(|w| w == label), "actual hover text {label}: {words:?}");
    }
    let mut words = click(&mut app, &ctx, &mut tick, "button:compactViews");
    for _ in 0..2 {
        words = draw(&mut app, &ctx, &mut tick, vec![]);
    }
    assert!(words.iter().any(|w| w == "Loupe (F7)"));
    assert!(words.iter().any(|w| w == "Compare"));
    assert!(!words.iter().any(|w| w == "Loupe (E)" || w == "Compare (C)"));
    egui::Popup::close_all(&ctx);
    draw(&mut app, &ctx, &mut tick, vec![]);
    let mut words = click(&mut app, &ctx, &mut tick, "icon:cloud");
    for _ in 0..2 {
        words = draw(&mut app, &ctx, &mut tick, vec![]);
    }
    assert!(words.iter().any(|w| w == "Preferences…"));
    assert!(!words.iter().any(|w| w == "Settings…"));
    click(&mut app, &ctx, &mut tick, "menu:libraryStatus:settings");
    assert!(matches!(app.ui.dialog, Some(state::Dialog::Settings { .. })));
}
