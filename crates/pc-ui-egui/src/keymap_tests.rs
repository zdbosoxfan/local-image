#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, Key, Modifiers};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
type State = (PhotocraftApp, Map<String, Value>);
fn state() -> State {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let id = open_shortcuts(&mut app, 0);
    let fields = app.ui.dialog_mut(id).unwrap().fields.clone();
    app.ui.dialogs.clear();
    (app, fields)
}
fn host() -> Vec<HostShortcutItem> {
    vec![
        ("library::view.maskOverlay".into(), "Mask overlay".into(), vec!["Library".into(), "View".into()], Some("O".into())),
        ("library::tool.brush".into(), "Brush".into(), vec!["Library".into(), "Tools".into()], Some("K".into())),
        ("library::tool.linear".into(), "Linear Gradient".into(), vec!["Library".into(), "Tools".into()], Some("M".into())),
        ("library::tool.erase".into(), "Erase mask".into(), vec!["Library".into(), "Tools".into()], None),
    ]
}
fn setup_harness(h: &mut Harness<'_, State>) {
    crate::theme::install_fonts(&h.ctx);
    crate::theme::apply(&h.ctx, crate::theme::ThemeKind::Pro);
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("host-shortcut-catalog"), host()));
    h.run_steps(4);
}
fn draw(ui: &mut egui::Ui, (app, fields): &mut State) {
    let ready = egui::Id::new("keys-test-fonts-ready");
    if !ui.ctx().data(|data| data.get_temp::<bool>(ready)).unwrap_or(false) {
        crate::theme::install_fonts(ui.ctx());
        crate::theme::apply(ui.ctx(), crate::theme::ThemeKind::Pro);
        ui.ctx().data_mut(|data| data.insert_temp(ready, true));
        ui.ctx().request_repaint();
        return;
    }
    shortcuts_body(app, ui, fields);
}
fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder().with_size(vec2(900., 760.)).with_max_steps(64).build_ui_state(draw, state());
    setup_harness(&mut h);
    h
}
fn key(h: &mut Harness<'_, State>, key: Key, modifiers: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.run_steps(1);
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(2);
}
fn capture(h: &mut Harness<'_, State>, id: &str, key_value: Key, modifiers: Modifiers) {
    let pos = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", id)))).unwrap().center();
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    key(h, key_value, modifiers);
}
#[test]
fn keys_profiles_g3_widget_lifecycle_capture_global_and_cancel() {
    let mut h = harness();
    h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Editor")).click();
    h.run();
    h.get_by_label("Library + Develop").click();
    h.run();
    h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Lightroom")).click();
    h.run();
    h.get_by_label("Capture One").click();
    h.run();
    assert_eq!(h.state().1["profiles"]["library"]["active"], "Capture One");
    assert_eq!(h.state().1["profiles"]["editor"]["active"], "Photoshop");
    h.get_by_label("Profile name").click();
    h.run();
    h.get_by_label("Profile name").type_text("My C1");
    h.run();
    h.get_by_label("Duplicate").click();
    h.run();
    assert_eq!(h.state().1["profiles"]["library"]["active"], "My C1");
    h.state_mut().1.insert("filter".into(), json!("library::view.maskOverlay"));
    h.run();
    capture(&mut h, "library::view.maskOverlay", Key::O, Modifiers::SHIFT);
    assert_eq!(h.state().1["overrides"]["library::view.maskOverlay"], "Shift+O");
    h.state_mut().1.insert("filter".into(), json!("library::tool.brush"));
    h.run();
    capture(&mut h, "library::tool.brush", Key::Q, Modifiers::COMMAND);
    assert!(h.state().1["message"].as_str().unwrap().contains("Conflict with Global: Quit"));
    assert_eq!(h.state().1["overrides"]["library::tool.brush"], "Cmd+Q");
    let (app, f) = h.state_mut();
    confirm(app, f).unwrap();
    assert_eq!(h.state().0.session.prefs().shortcut_profiles.library.active, "My C1");
    assert_eq!(crate::keymaps::effective(h.state().0.session.prefs(), "edit.undo", None), Some("Cmd+Z"));
    assert_eq!(h.state().0.session.prefs().shortcut("library::tool.brush", Some("B")), None);
    // Rename through the actual text input and button.
    h.get_by_label("Profile name").click();
    key(&mut h, Key::A, Modifiers::COMMAND);
    h.get_by_label("Profile name").type_text("Portrait C1");
    h.run();
    h.get_by_label("Rename").click();
    h.run();
    assert_eq!(h.state().1["profiles"]["library"]["active"], "Portrait C1");
    h.get_by_label("Delete Profile").click();
    h.run();
    assert_eq!(h.state().1["profiles"]["library"]["active"], "Capture One");
    let (app, f) = h.state_mut();
    confirm(app, f).unwrap();
    let saved = h.state().0.session.prefs_to_json();
    let mut restarted = photocraft_engine::Session::new();
    restarted.load_prefs_json(&saved).unwrap();
    assert_eq!(restarted.prefs().shortcut_profiles.library.active, "Capture One");
    assert_eq!(restarted.prefs().shortcut_profiles.editor.active, "Photoshop");
    let (app, _) = h.state_mut();
    let id = open_shortcuts(app, 0);
    app.ui.dialog_mut(id).unwrap().fields.insert("profiles".into(), json!({"library":{"active":"Lightroom"},"editor":{"active":"Affinity Photo"}}));
    crate::dialogs::cancel(app, id).unwrap();
    assert_eq!(app.session.prefs().shortcut_profiles.editor.active, "Photoshop");
}
#[test]
fn keys_profiles_editor_tools_globals_and_spring_loaded_keys() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.session.edit_prefs(|p| {
        p.shortcut_profiles.editor.active = "Affinity Photo".into();
    });
    assert_eq!(crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::CloneStamp).as_deref(), Some("K"));
    assert_eq!(crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::RotateView).as_deref(), Some("R"));
    assert_eq!(crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::AiRemove).as_deref(), Some("J"));
    assert_eq!(crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::RectMarquee).as_deref(), Some("M"));
    assert!(crate::hold_keys::binding(&app, crate::hold_keys::Temporary::Hand).is_some());
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![Event::Key { key: Key::K, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
            ..Default::default()
        },
        |ui| crate::shortcuts::handle(&mut app, ui.ctx()),
    );
    output.textures_delta.clear();
    assert_eq!(app.ui.tool, crate::state::Tool::CloneStamp);
    app.session.edit_prefs(|p| {
        p.shortcut_profiles.editor.duplicate("Mine").unwrap();
        let profile = p.shortcut_profiles.editor.custom.get_mut("Mine").unwrap();
        profile.overrides.insert("edit.search".into(), "Cmd+Q".into());
        profile.overrides.insert("tools.select.Brush".into(), "Cmd+Z".into());
    });
    let bindings = crate::shortcut_dispatch::binding_strings(&app);
    assert!(bindings.contains(&("file.exit".into(), "Cmd+Q".into())));
    assert!(!bindings.iter().any(|(id, _)| id == "edit.search"));
    assert!(crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::Brush).is_none());
    crate::shortcut_dispatch::set_dry_run(&ctx, true);
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![Event::Key { key: Key::Q, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }],
            ..Default::default()
        },
        |ui| crate::shortcuts::handle(&mut app, ui.ctx()),
    );
    output.textures_delta.clear();
    assert_eq!(
        crate::shortcut_dispatch::take_log(&ctx),
        vec![("file.exit".into(), crate::shortcut_dispatch::Outcome::Ran)],
        "the real event must reach Global Quit, never the conflicting Search binding"
    );
    app.session.edit_prefs(|p| {
        p.shortcut_profiles.editor.custom.get_mut("Mine").unwrap().overrides.insert("edit.undo".into(), "V".into());
        p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
    });
    assert_eq!(
        crate::shortcuts::tool_shortcut(&app.session, crate::state::Tool::Move).as_deref(),
        Some("V"),
        "ignored Global overrides must not reserve Vector's default tool keys"
    );
}
#[test]
fn keys_profiles_gpu_screenshots_all_scopes_dark_light_narrow_normal() {
    let _gate = crate::gpu_test_support::lock();
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                if cfg!(target_os = "macos") {
                    info.backend == wgpu::Backend::Metal && info.device_type != wgpu::DeviceType::Cpu
                } else {
                    info.device_type == wgpu::DeviceType::DiscreteGpu && info.name.contains("5090")
                }
            })
            .ok_or("hardware Metal (macOS) or RTX 5090 required")?;
        eprintln!("Keyboard editor renderer: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup));
    let mut h = Harness::builder().with_size(vec2(900., 760.)).renderer(renderer).build_ui_state(draw, state());
    setup_harness(&mut h);
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/keys-ux");
    std::fs::create_dir_all(&output).unwrap();
    for (theme, name) in [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        crate::theme::apply(&h.ctx, theme);
        for width in [480., 900.] {
            h.set_size(vec2(width, 760.));
            for scope in ["global", "library", "editor"] {
                let f = &mut h.state_mut().1;
                f.insert("shortcutScope".into(), json!(scope));
                f.insert("filter".into(), json!(if scope == "library" { "library::view.maskOverlay" } else { "edit.search" }));
                f.insert("overrides".into(), json!({"library::view.maskOverlay":"Cmd+Q","edit.search":"Cmd+Q"}));
                f.insert("profiles".into(),json!({"library":{"active":"Mine","custom":{"Mine":{"parent":"Capture One","overrides":{"library::view.maskOverlay":"Cmd+Q"}}}},"editor":{"active":"Mine","custom":{"Mine":{"parent":"Photoshop","overrides":{"edit.search":"Cmd+Q"}}}}}));
                h.run();
                h.render().unwrap().save(output.join(format!("{scope}-{name}-{}.png", width as u32))).unwrap();
            }
        }
    }
    h.get_by_label("Profile name").click();
    h.run();
    h.get_by_label("Profile name").type_text("Timing sample");
    h.run();
    let start = std::time::Instant::now();
    h.get_by_label("Duplicate").click();
    h.run();
    let _ = h.render().unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(h.state().1["profiles"]["editor"]["active"], "Timing sample");
    std::fs::write(output.join("runtime.txt"),format!("{}\nWarm shortcut editor Duplicate widget click, actual editor body + profile update + GPU render/readback: {ms:.3} ms\nOne headless interaction sample; not a desktop frame-rate, compositor, or photo-pipeline benchmark.\n", if cfg!(target_os = "macos") { "hardware Metal" } else { "RTX 5090 / Vulkan" })).unwrap();
    eprintln!("Keyboard profile warm Duplicate click + GPU readback: {ms:.3} ms");
}

#[test]
fn keys_profiles_preferences_entry_and_malformed_draft_are_safe() {
    let mut h = Harness::builder().with_size(vec2(1100., 760.)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), Default::default())
    });
    open_preferences(h.state_mut(), "general");
    h.run_steps(4);
    h.get_by_label("Keyboard Shortcuts…").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.dialogs.last().unwrap().fields["__prefsui"], "shortcuts");
    let id = h.state().ui.dialogs.last().unwrap().id;
    let before = h.state().session.prefs().clone();
    h.state_mut().ui.dialog_mut(id).unwrap().fields.insert("profiles".into(), json!("bad"));
    h.run_steps(3);
    assert!(crate::dialogs::confirm(h.state_mut(), id).is_err());
    assert_eq!(h.state().session.prefs(), &before);
}

#[test]
fn dsfix_shortcuts_ok_uses_selected_capture_one_defaults() {
    let mut h = harness();
    h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Editor")).click();
    h.run();
    h.get_by_label("Library + Develop").click();
    h.run();
    h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Lightroom")).click();
    h.run();
    h.get_by_label("Capture One").click();
    h.run();
    // Lightroom's Linear default is M, Capture One's is G. Assigning M must keep Linear.
    h.state_mut().1.insert("overrides".into(), json!({"library::view.maskOverlay":"M"}));
    let (app, fields) = h.state_mut();
    confirm(app, fields).unwrap();
    assert!(!h.state().0.session.prefs().shortcut_overrides(true).contains_key("library::tool.linear"));
    // G really conflicts with the selected profile's Linear default and is taken from it.
    let (mut app, mut fields) = state();
    fields["profiles"]["library"]["active"] = json!("Capture One");
    fields.insert("__hostShortcuts".into(), json!(host()));
    fields.insert("overrides".into(), json!({"library::view.maskOverlay":"G"}));
    confirm(&mut app, &fields).unwrap();
    assert_eq!(app.session.prefs().shortcut_overrides(true).get("library::tool.linear").map(String::as_str), Some(""));
    // A saved chord can acquire a new default conflict when its parent profile changes.
    let (mut app, mut fields) = state();
    app.session.edit_prefs(|prefs| prefs.set_shortcut("library::view.maskOverlay", Some("G")));
    fields["profiles"]["library"]["active"] = json!("Capture One");
    fields.insert("__hostShortcuts".into(), json!(host()));
    fields.insert("overrides".into(), json!({"library::view.maskOverlay":"G"}));
    confirm(&mut app, &fields).unwrap();
    assert_eq!(app.session.prefs().shortcut_overrides(true).get("library::tool.linear").map(String::as_str), Some(""));
}
