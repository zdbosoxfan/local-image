//! Menus/task U018–U022/U031: drive the actual title bar, menu tree and tool flyouts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_engine::toolsets::Persona;

fn app(vector: bool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width":64,"height":48})).unwrap();
    app.sync_views();
    app.session.edit_prefs(|p| p.toolbar.persona = if vector { Persona::Vector } else { Persona::Pixel });
    app.custom_titlebar = true;
    app.host_modes = true;
    app
}

fn fonts_ready(ui: &egui::Ui) -> bool {
    let id = egui::Id::new("menus-test-fonts-ready");
    if ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        return true;
    }
    crate::theme::install_fonts(ui.ctx());
    crate::theme::apply(ui.ctx(), crate::theme::ThemeKind::Pro);
    ui.ctx().data_mut(|d| d.insert_temp(id, true));
    ui.ctx().request_repaint();
    false
}

fn draw(ui: &mut egui::Ui, app: &mut PhotocraftApp) {
    if !fonts_ready(ui) {
        return;
    }
    crate::panels::title_bar(app, ui);
    crate::shortcuts::handle(app, ui.ctx());
    crate::dialogs::show(app, ui.ctx());
}

fn harness(vector: bool, size: (f32, f32), theme: crate::theme::ThemeKind) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(size.0, size.1)).with_max_steps(64).build_ui_state(draw, app(vector));
    PhotocraftApp::setup_context(&h.ctx, theme);
    h.ctx.all_styles_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::none());
    h.run_steps(4);
    h
}
fn key(h: &mut Harness<'_, PhotocraftApp>, key: egui::Key) {
    h.key_press(key);
    h.run_steps(4);
}
fn rows(h: &Harness<'_, PhotocraftApp>, level: usize) -> Vec<crate::menu_nav::Row> {
    crate::menu_nav::Nav::current(&h.ctx).rows.get(level).cloned().unwrap_or_default()
}
fn open_pointer(h: &mut Harness<'_, PhotocraftApp>, top: &str) {
    if h.query_by_label("Menu").is_some() {
        h.get_by_label("Menu").click();
        h.run_steps(4);
        let index = persona_menus(h.state()).iter().position(|title| *title == top).unwrap();
        let at = rows(h, 0)[index].rect.center();
        h.hover_at(at);
        h.run_steps(8);
    } else {
        h.get_by_label(top).click();
        h.run_steps(4);
    }
}
fn choose_row(h: &mut Harness<'_, PhotocraftApp>, level: usize, index: usize) {
    for _ in 0..rows(h, level).len() + 1 {
        key(h, egui::Key::ArrowDown);
        if crate::menu_nav::Nav::current(&h.ctx).highlighted(level) == Some(index) {
            return;
        }
    }
    panic!("row {index} could not be highlighted");
}

#[test]
fn menus_ux_order_geometry_and_verified_duplicates() {
    for vector in [false, true] {
        let app = app(vector);
        let items = menu_items(&app);
        let titles = persona_menus(&app);
        assert_eq!(titles.last(), Some(&"Help"));
        if vector {
            for title in ["Object", "Effect"] {
                assert!(titles.iter().position(|t| *t == title).unwrap() < titles.iter().position(|t| *t == "Window").unwrap());
            }
        }
        let layer: Vec<_> = items.iter().filter(|i| i.path.first().is_some_and(|p| p == "Layer")).collect();
        assert_eq!(layer[0].path.get(1).map(String::as_str), Some("New"));
        let combine = layer.iter().rposition(|i| i.path.get(1).is_some_and(|p| p == "Combine Shapes")).unwrap();
        assert_eq!(layer[combine + 1].path, ["Layer", "Geometry"]);
        assert_eq!(items.iter().filter(|item| item.path == ["Window", "Arrange"] && item.label == "New Window for Document").count(), 1);
        for old in ["help.website", "window.newWindowForDocument", "type.panels.character", "ui.svg.export", "ui.pdf.export", "file.export.pdf"] {
            assert!(!items.iter().any(|item| item.id == old) && !is_live(old), "old command ID {old} is gone");
        }
        let mut ids = std::collections::HashSet::new();
        for item in &items {
            assert!(item.id == "---" || ids.insert(&item.id), "duplicate {}", item.id);
        }
        assert_eq!(items.iter().filter(|i| i.id == "li.draftRefine").count(), 1);
        assert!(items.iter().any(|i| i.id == "window.panel.swatches"));
        assert!(items.iter().any(|i| i.id == "window.panel.color"));
        assert!(items.iter().any(|i| i.id == "window.panel.brushSettings"));
        assert!(items.iter().any(|i| i.id == "window.panel.brushes"));
        assert!(!items.iter().any(|i| matches!(i.id.as_str(), "edit.preferences.integrations" | "li.aiModels")));
        for id in ["edit.preferences.integrations", "li.aiModels"] {
            assert!(is_live(id));
        }
    }
}

#[test]
fn menus_ux_pointer_reaches_every_root_at_both_sizes_and_themes() {
    for theme in [crate::theme::ThemeKind::Pro, crate::theme::ThemeKind::StudioLight] {
        for size in [(760., 480.), (1440., 900.)] {
            for vector in [false, true] {
                for top in persona_menus(&app(vector)) {
                    let mut h = harness(vector, size, theme);
                    let compact = h.query_by_label("Menu").is_some();
                    assert_eq!(compact, size.0 == 760.);
                    open_pointer(&mut h, top);
                    let depth = usize::from(compact);
                    assert!(!rows(&h, depth).is_empty(), "{theme:?} {size:?} {top} did not open");
                    let nav = crate::menu_nav::Nav::current(&h.ctx);
                    assert!(h.ctx.content_rect().expand(0.5).contains_rect(nav.views[depth]), "{top} view {:?}", nav.views[depth]);
                    if *top == "Help" {
                        assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_some());
                    }
                }
            }
        }
    }
}

#[test]
fn menus_ux_f10_and_arrows_reach_every_root_at_both_sizes() {
    for size in [(760., 480.), (1440., 900.)] {
        for vector in [false, true] {
            for (index, top) in persona_menus(&app(vector)).iter().enumerate() {
                let mut h = harness(vector, size, crate::theme::ThemeKind::Pro);
                key(&mut h, egui::Key::F10);
                if size.0 == 760. {
                    choose_row(&mut h, 0, index);
                    key(&mut h, egui::Key::ArrowRight);
                    assert!(!rows(&h, 1).is_empty(), "keyboard {top}");
                } else {
                    for _ in 0..index {
                        key(&mut h, egui::Key::ArrowRight);
                    }
                    assert!(!rows(&h, 0).is_empty(), "keyboard {top}");
                    // Match a representative command, rather than merely an open popup.
                    let expected = menu_items(h.state()).into_iter().find(|i| i.path == [top.to_string()] && i.id != "---");
                    if let Some(expected) = expected {
                        assert!(rows(&h, 0).iter().any(|r| r.command.as_deref() == Some(&expected.id)));
                    }
                }
            }
        }
    }
}

#[test]
fn menus_ux_overflow_press_drag_and_keyboard_activate_the_same_command() {
    let mut h = harness(true, (760., 480.), crate::theme::ThemeKind::Pro);
    let before = h.state().ui.extras.rulers;
    key(&mut h, egui::Key::F10);
    choose_row(&mut h, 0, VECTOR_TOP_MENUS.iter().position(|t| *t == "View").unwrap());
    key(&mut h, egui::Key::ArrowRight);
    let index = rows(&h, 1).iter().position(|r| r.command.as_deref() == Some("view.rulers")).unwrap();
    choose_row(&mut h, 1, index);
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().ui.extras.rulers, !before);
    let at = h.get_by_label("Menu").rect().center();
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.run_steps(4);
    let at = rows(&h, 0)[VECTOR_TOP_MENUS.iter().position(|top| *top == "View").unwrap()].rect.center();
    h.hover_at(at);
    h.run_steps(8);
    let at = h.get_by_label("Rulers").rect().center();
    h.hover_at(at);
    h.run_steps(3);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(4);
    assert_eq!(h.state().ui.extras.rulers, before);
}

#[test]
fn menus_ux_checks_have_clean_labels_and_aligned_gutters() {
    let mut h = harness(false, (1440., 900.), crate::theme::ThemeKind::Pro);
    h.state_mut().ui.panels.generate = true;
    h.state_mut().draft_refine.open = true;
    open_pointer(&mut h, "Window");
    for (id, label, on) in [
        ("li.panel.generate", "Generate", true),
        ("li.draftRefine", "Draft / Refinement…", true),
        ("li.browseModels", "Model Browser…", crate::model_browser::is_open()),
        ("li.filmstrip", "Filmstrip", crate::filmstrip_ui::is_open()),
    ] {
        let item = menu_items(h.state()).into_iter().find(|i| i.id == id).unwrap();
        assert_eq!(item.checked, Some(on));
        assert_eq!(h.get_by_label(label).accesskit_node().toggled(), Some(if on { egui::accesskit::Toggled::True } else { egui::accesskit::Toggled::False }));
    }
    let items = rows(&h, 0);
    let left = items[0].rect.left();
    assert!(items.iter().all(|row| (row.rect.left() - left).abs() < 0.1));
    let mut jobs = Vec::new();
    let mut fixture = Harness::builder().build_ui(|ui| {
        for label in ["Generate", "Model Browser…", "Theme"] {
            jobs.push(crate::widgets::menu_row_label(ui, label));
        }
    });
    fixture.run_steps(2);
    drop(fixture);
    assert!(jobs.iter().all(|job| job.sections[0].leading_space == jobs[0].sections[0].leading_space));
}

#[test]
fn menus_ux_toolbar_presets_and_saved_custom_sets_keep_their_tools() {
    use crate::state::Tool;
    let mut app = app(false);
    app.ui.tool = Tool::Move;
    for persona in [Persona::Pixel, Persona::Vector] {
        for set in photocraft_engine::toolsets::builtins_for(persona) {
            app.session.edit_prefs(|p| p.toolbar.persona = persona);
            app.run("toolset.select", json!({"id":set.id})).unwrap();
            let tools: Vec<_> = crate::panels::visible_sections(&app).into_iter().flatten().flat_map(|(_, ts)| ts).collect();
            for name in &set.tools {
                assert!(tools.contains(&Tool::from_name(name).unwrap()), "{} lost {name}", set.id);
            }
            if persona == Persona::Pixel && set.id != "allTools" {
                assert!(tools.iter().all(|tool| set.tools.iter().any(|name| Tool::from_name(name) == Some(*tool))));
            }
        }
    }
    app.session.edit_prefs(|p| p.toolbar.persona = Persona::Pixel);
    app.run("toolset.select", json!({"id":"allTools"})).unwrap();
    let tools: Vec<_> = crate::panels::visible_sections(&app).into_iter().flatten().flat_map(|(_, ts)| ts).collect();
    for name in photocraft_engine::shared_tools::SHARED_ADDITIONS {
        assert!(tools.contains(&Tool::from_name(name).unwrap()));
    }
    let saved = app.run("toolset.save", json!({"name":"My menu tools","tools":["Move","Pencil","VectorPencil","ArtText","VectorCrop"]})).unwrap();
    let prefs = app.session.prefs_to_json();
    let mut restarted = self::app(false);
    restarted.session.load_prefs_json(&prefs).unwrap();
    restarted.run("toolset.select", json!({"id":saved["created"]})).unwrap();
    let tools: Vec<_> = crate::panels::visible_sections(&restarted).into_iter().flatten().flat_map(|(_, ts)| ts).collect();
    assert_eq!(tools.len(), 5);
    for tool in [Tool::Pencil, Tool::VectorPencil, Tool::ArtText, Tool::VectorCrop] {
        assert!(tools.contains(&tool));
    }
    restarted.run("toolset.select", json!({"id":"photographer"})).unwrap();
    restarted.ui.tool = Tool::VectorPencil;
    assert!(crate::panels::visible_sections(&restarted).into_iter().flatten().any(|(_, ts)| ts.contains(&Tool::VectorPencil)));
}

#[test]
fn menus_ux_toolbar_flyout_pointer_selects_frame_text_without_raster_aliasing() {
    use crate::state::Tool;
    let mut app = app(false);
    app.run("toolset.save", json!({"name":"Types","tools":["Pencil","ArtText","FrameText"]})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(760., 480.)).build_ui_state(|ui, app| crate::panels::toolbar(app, ui), app);
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(4);
    h.get_by_label(Tool::ArtText.label()).click_secondary();
    h.run_steps(4);
    h.get_by_label(Tool::FrameText.label()).click();
    h.run_steps(4);
    assert_eq!(h.state().ui.tool, Tool::FrameText);
    assert!(h.query_by_label(Tool::Pencil.label()).is_some());
}

#[test]
fn menus_ux_preferences_hide_empty_integrations_and_preserve_legacy_route() {
    let mut h = harness(false, (1440., 900.), crate::theme::ThemeKind::Pro);
    let ctx = h.ctx.clone();
    invoke(h.state_mut(), &ctx, "edit.preferences.general", json!({})).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label("AI Integrations").is_none());
    assert_eq!(h.get_by_role_and_label(egui::accesskit::Role::Button, "General").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    h.state_mut().ui.dialogs.clear();
    invoke(h.state_mut(), &ctx, "edit.preferences.integrations", json!({})).unwrap();
    assert_eq!(h.state().ui.dialogs.last().unwrap().fields["section"], "ai.connection", "the legacy alias opens shared AI Preferences");
    h.run_steps(4);
    h.get_by_label("Find on LAN");
}

#[test]
fn menus_ux_f10_and_tab_respect_modal_ownership() {
    let mut h = harness(false, (760., 480.), crate::theme::ThemeKind::Pro);
    let ctx = h.ctx.clone();
    invoke(h.state_mut(), &ctx, "edit.preferences.general", json!({})).unwrap();
    h.run_steps(4);
    key(&mut h, egui::Key::F10);
    assert!(rows(&h, 0).is_empty());
    let hidden = h.state().ui.view.panels_hidden;
    key(&mut h, egui::Key::Tab);
    assert_eq!(h.state().ui.view.panels_hidden, hidden);
    assert!(h.ctx.memory(|m| m.focused().is_some()));
}

#[test]
fn menus_ux_active_editor_profile_labels_and_swatches_tab_are_retained() {
    let mut app = app(false);
    app.session.edit_prefs(|p| {
        p.shortcut_profiles.editor.duplicate("Menus profile").unwrap();
        p.shortcut_profiles.editor.custom.get_mut("Menus profile").unwrap().overrides.insert("edit.search".into(), "Cmd+Alt+9".into());
        p.shortcut_profiles.editor.custom.get_mut("Menus profile").unwrap().overrides.insert("window.arrange.newWindowForDocument".into(), "Cmd+Alt+8".into());
        p.shortcut_profiles.library.duplicate("Library profile").unwrap();
        p.shortcut_profiles.library.custom.get_mut("Library profile").unwrap().overrides.insert("edit.search".into(), "Cmd+Alt+8".into());
    });
    assert_eq!(menu_items(&app).iter().find(|i| i.id == "edit.search").unwrap().shortcut.as_deref(), Some("Cmd+Alt+9"));
    assert_eq!(menu_items(&app).iter().find(|i| i.id == "window.arrange.newWindowForDocument").unwrap().shortcut.as_deref(), Some("Cmd+Alt+8"));
    let ctx = egui::Context::default();
    invoke(&mut app, &ctx, "window.panel.swatches", json!({})).unwrap();
    assert_eq!(checked(&app, "window.panel.swatches"), Some(true));
    assert_eq!(checked(&app, "window.panel.color"), Some(false));
    invoke(&mut app, &ctx, "window.panel.color", json!({})).unwrap();
    assert_eq!(checked(&app, "window.panel.color"), Some(true));
    assert_eq!(checked(&app, "window.panel.swatches"), Some(false));
}

fn renderer() -> egui_kittest::wgpu::WgpuTestRenderer {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu && a.get_info().name.contains("5090"))
            .ok_or("RTX 5090 required")?;
        eprintln!("menus UX screenshots: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup))
}

#[test]
#[ignore = "real RTX 5090 offscreen screenshots; run explicitly"]
fn menus_ux_rtx5090_screenshots() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/menus-ux");
    std::fs::create_dir_all(&dir).unwrap();
    for (theme, name) in [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        for size in [(760., 480.), (1440., 900.)] {
            let mut h = Harness::builder().with_size(egui::vec2(size.0, size.1)).renderer(renderer()).build_ui_state(draw, app(true));
            PhotocraftApp::setup_context(&h.ctx, theme);
            h.run_steps(4);
            let save =
                |h: &mut Harness<'_, PhotocraftApp>, suffix| h.render().unwrap().save(dir.join(format!("{name}-{}-{suffix}.png", size.0 as u32))).unwrap();
            save(&mut h, "bar");
            if size.0 == 760. {
                h.get_by_label("Menu").click();
                h.run_steps(4);
                save(&mut h, "overflow");
                key(&mut h, egui::Key::Escape);
            }
            open_pointer(&mut h, "Layer");
            save(&mut h, "long-menu");
            key(&mut h, egui::Key::Escape);
            h.state_mut().ui.panels.generate = true;
            h.state_mut().draft_refine.open = true;
            open_pointer(&mut h, "Window");
            save(&mut h, "checks");
        }
    }
}

#[test]
fn menus_ux_toolbar_flyouts_for_every_builtin_persona_preset() {
    use crate::state::Tool;
    for persona in [Persona::Pixel, Persona::Vector] {
        for set in photocraft_engine::toolsets::builtins_for(persona) {
            let mut app = self::app(persona == Persona::Vector);
            app.ui.tool = Tool::Move;
            app.run("toolset.select", json!({"id":set.id})).unwrap();
            let slots: Vec<_> = crate::panels::visible_sections(&app).into_iter().flatten().map(|(_, ts)| ts).filter(|ts| ts.len() > 1).collect();
            let mut h = Harness::builder().with_size(egui::vec2(1440., 1800.)).build_ui_state(
                |ui, app| {
                    if fonts_ready(ui) {
                        crate::panels::toolbar(app, ui);
                    }
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
            h.run_steps(4);
            for tools in slots {
                h.get_by_label(tools[0].label()).click_secondary();
                h.run_steps(4);
                for tool in &tools {
                    assert!(h.query_all_by_label(tool.label()).count() >= if *tool == tools[0] { 2 } else { 1 }, "{} flyout missing {tool:?}", set.id);
                }
                h.query_all_by_label(tools.last().unwrap().label()).max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap().click();
                h.run_steps(4);
                assert_eq!(h.state().ui.tool, *tools.last().unwrap());
                assert!(h.ctx.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::new("tool-flyout"))).is_none());
            }
        }
    }
}

#[test]
fn menus_ux_focused_title_opens_with_enter_and_space() {
    for size in [(760., 480.), (1440., 900.)] {
        for k in [egui::Key::Enter, egui::Key::Space] {
            let mut h = harness(false, size, crate::theme::ThemeKind::Pro);
            let title = if size.0 == 760. { "Menu" } else { "File" };
            h.get_by_label(title).focus();
            h.run_steps(2);
            key(&mut h, k);
            assert!(!rows(&h, 0).is_empty(), "{title} via {k:?}");
        }
    }
}

#[test]
fn menus_ux_f10_keeps_explicit_profile_binding_ownership() {
    let mut h = harness(false, (760., 480.), crate::theme::ThemeKind::Pro);
    h.state_mut().session.edit_prefs(|p| {
        p.shortcut_profiles.editor.duplicate("F10 owner").unwrap();
        p.shortcut_profiles.editor.custom.get_mut("F10 owner").unwrap().overrides.insert("view.rulers".into(), "F10".into());
    });
    let before = h.state().ui.extras.rulers;
    assert_eq!(menu_items(h.state()).iter().find(|i| i.id == "view.rulers").unwrap().shortcut.as_deref(), Some("F10"));
    key(&mut h, egui::Key::F10);
    assert_eq!(h.state().ui.extras.rulers, !before);
    assert!(rows(&h, 0).is_empty());
}

#[test]
fn menus_ux_overflow_help_search_runs_commands_from_other_roots() {
    let mut h = harness(true, (760., 480.), crate::theme::ThemeKind::Pro);
    let before = h.state().ui.extras.rulers;
    open_pointer(&mut h, "Help");
    h.get_by_role(egui::accesskit::Role::TextInput).type_text("Rulers");
    h.run_steps(4);
    h.get_by_label("Menu › View › Rulers").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.extras.rulers, !before);
    assert!(rows(&h, 0).is_empty());
}

#[test]
fn menus_ux_type_panels_have_one_location_and_toggle() {
    for name in ["character", "paragraph", "glyphs", "characterStyles", "paragraphStyles"] {
        let mut app = self::app(false);
        let old = format!("type.panels.{name}");
        let canonical = format!("window.panel.{name}");
        app.session.edit_prefs(|p| {
            p.shortcut_profiles.editor.duplicate("Panel profile").unwrap();
            p.shortcut_profiles.editor.custom.get_mut("Panel profile").unwrap().overrides.insert(canonical.clone(), "Cmd+Alt+9".into());
        });
        let items = menu_items(&app);
        assert!(!items.iter().any(|item| item.id == old));
        assert!(!is_live(&old), "old Type › Panels ID {old} is gone");
        assert_eq!(items.iter().filter(|item| item.id == canonical).count(), 1);
        assert_eq!(items.iter().find(|item| item.id == canonical).unwrap().shortcut.as_deref(), Some("Cmd+Alt+9"));
        let before = checked(&app, &canonical).unwrap();
        let journal = app.session.journal.len();
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, &canonical, json!({})).unwrap();
        assert_eq!(checked(&app, &canonical), Some(!before));
        invoke(&mut app, &ctx, &canonical, json!({})).unwrap();
        assert_eq!(checked(&app, &canonical), Some(before));
        assert_eq!(app.session.journal.len(), journal);
    }
}
