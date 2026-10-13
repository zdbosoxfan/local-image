//! Real command, click and offscreen screenshot acceptance for U023/U029.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{PhotocraftApp, Services, Tool, canvas_tool_menu, command_feedback, menus};
use egui::{Event, Modifiers, PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::{Session, toolsets::Persona};
use serde_json::json;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), Services::default());
    app.run("file.new", json!({"width": 96, "height": 64})).unwrap();
    app.run("layer.new.layer", json!({"name": "Paint"})).unwrap();
    app.sync_views();
    app
}
fn fail(app: &mut PhotocraftApp, command: &str, params: serde_json::Value, expected: &str) {
    let raw = app.run(command, params).unwrap_err();
    assert_eq!(app.ui.status, expected, "{raw}");
    let notice = app.ui.notice_history.last().unwrap();
    assert_eq!(notice.title, expected);
    assert_eq!(notice.lines, [raw]);
}

#[test]
fn reasons_ux_reproduced_engine_errors_keep_raw_history() {
    let mut empty = PhotocraftApp::new(Session::new(), Services::default());
    fail(&mut empty, "edit.fill", json!({}), "Open a photo first.");
    let raw = empty.session.edit("Test", |_, _| Ok(())).unwrap_err().to_string();
    crate::vector_integration::pick_object(&mut empty, [0., 0.], false, false);
    assert_eq!(empty.ui.status, "Open a photo first.");
    assert_eq!(empty.ui.notice_history.last().unwrap().lines, [raw]);
    let mut app = app();
    fail(&mut app, "select.deselect", json!({}), "Make a selection first.");
    fail(&mut app, "edit.paste", json!({}), "Copy an image first.");
    fail(&mut app, "edit.redo", json!({}), "There are no edits to redo.");
    fail(&mut app, "layer.duplicate", json!({"layer": u64::MAX}), "This layer is no longer available. Select another layer.");
    fail(&mut app, "shape.create", json!({"kind":"rect"}), "Check the settings and try again.");
    app.run("shape.create", json!({"kind":"rect","rect":[8,8,20,20]})).unwrap();
    fail(&mut app, "edit.fill", json!({}), "Select a pixel layer first.");
    fail(&mut app, "edit.cut", json!({}), "Select a pixel layer first.");
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("layer.setProps", json!({"locked":true})).unwrap();
    fail(&mut app, "edit.fill", json!({"color":"#ff0000"}), "This layer is locked.");
    app.session.active_mut().unwrap().active_layer = None;
    fail(&mut app, "layer.duplicate", json!({}), "Select a layer first.");
    let mut fresh = PhotocraftApp::new(Session::new(), Services::default());
    fresh.run("file.new", json!({"width":10,"height":10})).unwrap();
    fail(&mut fresh, "edit.undo", json!({}), "There are no edits to undo.");
    // Unknown wording passes through; the mapper never guesses at unrelated file errors.
    assert_eq!(command_feedback::message("A specific file failure"), "A specific file failure");
}

fn fonts_ready(ui: &egui::Ui) -> bool {
    let id = egui::Id::new("reasons-fonts");
    if ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        return true;
    }
    crate::theme::install_fonts(ui.ctx());
    ui.ctx().data_mut(|d| d.insert_temp(id, true));
    ui.ctx().request_repaint();
    false
}
#[derive(Clone, Copy)]
enum Surface {
    Main,
    Panel,
    Selection,
    Vector,
    Status,
}
fn draw(ui: &mut egui::Ui, (app, surface): &mut (PhotocraftApp, Surface)) {
    if !fonts_ready(ui) {
        return;
    }
    match surface {
        Surface::Main => {
            menus::menu_bar(app, ui);
        }
        Surface::Panel => {
            ui.menu_button("Layer actions", |ui| {
                let st = app.session.active().unwrap();
                let l = st.doc.layer(st.active_layer.unwrap()).unwrap();
                let mut actions = Vec::new();
                crate::layer_menu_ui::show(app, ui, l, false, &mut actions);
                for (id, params) in actions {
                    menus::invoke(app, ui.ctx(), &id, params).unwrap();
                }
            });
        }
        Surface::Selection | Surface::Vector => {
            canvas_tool_menu::show(app, ui.ctx());
        }
        Surface::Status => {
            crate::panels::status_bar(app, ui);
            crate::notices::panel(app, ui);
            crate::notices::show(app, ui.ctx());
        }
    }
}
fn harness(app: PhotocraftApp, surface: Surface) -> Harness<'static, (PhotocraftApp, Surface)> {
    let mut h = Harness::builder().with_size(vec2(900., 850.)).build_ui_state(draw, (app, surface));
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.;
        s.interaction.tooltip_grace_time = 0.;
    });
    h.run_steps(4);
    h
}
fn click_at<T>(h: &mut Harness<'_, T>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}
fn hover_reason(h: &mut Harness<'_, (PhotocraftApp, Surface)>, label: &str, reason: &str) {
    let p = h.get_by_label_contains(label).rect().center();
    h.hover_at(p);
    h.run_steps(6);
    assert!(h.query_by_label(reason).is_some(), "missing tooltip: {label} -> {reason}");
    let before = h.state().0.session.journal.len();
    click_at(h, p);
    assert_eq!(h.state().0.session.journal.len(), before, "disabled click invoked {label}");
}
#[test]
fn reasons_ux_disabled_main_panel_context_clicks_and_tooltips() {
    let empty = PhotocraftApp::new(Session::new(), Services::default());
    let mut h = harness(empty, Surface::Main);
    h.get_by_label("Edit").click();
    h.run_steps(4);
    hover_reason(&mut h, if cfg!(target_os = "macos") { "Fill… ⇧F5" } else { "Fill… Shift+F5" }, "Open a photo first.");
    let mut a = app();
    a.run("layer.setProps", json!({"locked":true})).unwrap();
    let mut h = harness(a, Surface::Panel);
    h.get_by_label("Layer actions").click();
    h.run_steps(4);
    // Pixel layers cannot be rasterized again; the inherited lock is disclosed.
    hover_reason(&mut h, "Rasterize Layer", "This layer is locked.");
    let mut a = app();
    a.ui.tool = Tool::RectMarquee;
    a.run("select.rect", json!({"x":8,"y":8,"width":20,"height":20})).unwrap();
    canvas_tool_menu::open(&mut a, Tool::RectMarquee, [20., 20.]);
    let mut h = harness(a, Surface::Selection);
    hover_reason(&mut h, "Last Filter", "no filter applied yet");
}
#[test]
fn reasons_ux_every_disabled_main_row_has_a_reason_and_profile_labels_match() {
    for a in [PhotocraftApp::new(Session::new(), Services::default()), app()] {
        for row in menus::menu_items(&a) {
            if !row.enabled && row.id != "---" {
                assert!(row.tooltip.is_some(), "{}", row.id);
            }
        }
    }
    let mut a = app();
    a.session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    for row in crate::vector_integration::menu_items(&a) {
        if !row.enabled {
            assert!(row.tooltip.is_some(), "{}", row.id);
        }
    }
    a.session.edit_prefs(|p| p.set("shortcuts.select.deselect", json!("Ctrl+Alt+D")).unwrap());
    assert_eq!(crate::shortcuts::shortcut_label(&a, "select.deselect").as_deref(), Some(if cfg!(target_os = "macos") { "Ctrl⌥D" } else { "Ctrl+Alt+D" }));
    a.session.edit_prefs(|p| p.set("shortcuts.select.deselect", json!("")).unwrap());
    assert!(crate::shortcuts::shortcut_label(&a, "select.deselect").is_none());
}
fn shape_app() -> (PhotocraftApp, photocraft_doc::LayerId) {
    let mut a = app();
    a.session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    let id = a.run("shape.create", json!({"kind":"rect", "rect":[8,8,30,24], "fill":"#3070c0"})).unwrap()["layer"].as_u64().unwrap();
    a.ui.tool = Tool::Move;
    (a, photocraft_doc::LayerId(id))
}
#[test]
fn reasons_ux_vector_object_menu_real_clicks_group_transform_and_lock() {
    let (mut a, shape) = shape_app();
    canvas_tool_menu::open(&mut a, Tool::Move, [20., 20.]);
    let mut h = harness(a, Surface::Vector);
    hover_reason(&mut h, "Make Clipping Mask", "Select a clipping path and at least one content object.");
    h.get_by_label("Group").click();
    h.run_steps(3);
    let st = h.state().0.session.active().unwrap();
    assert!(st.doc.layer(st.active_layer.unwrap()).unwrap().is_group());
    assert!(st.doc.layer(shape).is_some());
    canvas_tool_menu::open(&mut h.state_mut().0, Tool::Move, [20., 20.]);
    h.run_steps(3);
    h.get_by_label("Ungroup").click();
    h.run_steps(3);
    canvas_tool_menu::open(&mut h.state_mut().0, Tool::Move, [20., 20.]);
    h.run_steps(3);
    h.get_by_label("Move…").click();
    h.run_steps(3);
    // The operation is captured in the existing transform dialog and has not edited yet.
    assert!(h.state().0.vector_transform_tools.dialog_open());
    h.state_mut().0.vector_transform_tools.cancel();
    canvas_tool_menu::open(&mut h.state_mut().0, Tool::Move, [20., 20.]);
    h.run_steps(3);
    h.get_by_label("Lock Objects").click();
    h.run_steps(3);
    assert!(h.state().0.session.active().unwrap().doc.layer(shape).unwrap().locks.all);
}
#[test]
fn reasons_ux_stale_context_target_never_invokes_ai() {
    let mut a = app();
    a.ui.tool = Tool::RectMarquee;
    a.run("select.rect", json!({"x":8,"y":8,"width":20,"height":20})).unwrap();
    canvas_tool_menu::open(&mut a, Tool::RectMarquee, [20., 20.]);
    a.run("select.all", json!({})).unwrap();
    let mut h = harness(a, Surface::Selection);
    hover_reason(&mut h, "Generative Fill…", "The document or selection changed. Open the menu again.");
    assert!(h.state().0.ai_remove.is_none());
    canvas_tool_menu::choose(&mut h.state_mut().0, &egui::Context::default(), "li.removeSelection");
    assert!(h.state().0.ai_remove.is_none());
}
#[test]
fn reasons_ux_vector_move_picks_geometry_without_layer_pick() {
    let (mut a, shape) = shape_app();
    a.session
        .edit("Clear raster cache", |doc, _| {
            let photocraft_doc::LayerContent::Shape(sh) = &mut doc.layer_mut(shape).unwrap().content else { panic!("expected a shape fixture") };
            sh.cache = None;
            Ok(())
        })
        .unwrap();
    a.run("layer.select", json!({"layer":a.session.active().unwrap().doc.layers[1].id.0})).unwrap();
    let before = a.session.journal.len();
    crate::canvas::tool_event(&mut a, crate::canvas::ToolEvent::Down { x: 16., y: 16., pressure: 1. }, Modifiers::NONE);
    crate::canvas::tool_event(&mut a, crate::canvas::ToolEvent::Up { x: 16., y: 16. }, Modifiers::NONE);
    assert_eq!(a.session.active().unwrap().active_layer, Some(shape));
    assert!(!a.session.journal[before..].iter().any(|(id, _)| id == "layer.pickAt"));
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
        eprintln!("reasons UX screenshots: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    egui_kittest::wgpu::WgpuTestRenderer::from_setup(WgpuSetup::CreateNew(setup))
}
#[test]
#[ignore = "real RTX 5090 offscreen screenshots; run explicitly"]
fn reasons_ux_rtx5090_menu_and_tooltip_screenshots() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/reasons-ux");
    std::fs::create_dir_all(&dir).unwrap();
    for (theme, theme_name) in [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")] {
        for width in [250., 400.] {
            for (surface, name) in [(Surface::Panel, "panel"), (Surface::Selection, "selection"), (Surface::Vector, "vector")] {
                let mut a = if matches!(surface, Surface::Vector) { shape_app().0 } else { app() };
                if matches!(surface, Surface::Selection) {
                    a.ui.tool = Tool::RectMarquee;
                    a.run("select.rect", json!({"x":8,"y":8,"width":20,"height":20})).unwrap();
                    canvas_tool_menu::open(&mut a, Tool::RectMarquee, [8., 8.]);
                } else if matches!(surface, Surface::Vector) {
                    canvas_tool_menu::open(&mut a, Tool::Move, [8., 8.]);
                } else {
                    a.run("layer.setProps", json!({"locked":true})).unwrap();
                }
                let mut h = Harness::builder().with_size(vec2(width, 850.)).renderer(renderer()).build_ui_state(draw, (a, surface));
                PhotocraftApp::setup_context(&h.ctx, theme);
                h.ctx.all_styles_mut(|s| {
                    s.interaction.tooltip_delay = 0.;
                    s.interaction.tooltip_grace_time = 0.;
                });
                h.run_steps(4);
                if matches!(surface, Surface::Panel) {
                    h.get_by_label("Layer actions").click();
                    h.run_steps(4);
                }
                h.render().unwrap().save(dir.join(format!("{name}-{theme_name}-{}-menu.png", width as u32))).unwrap();
                let (label, why) = match surface {
                    Surface::Panel => ("Rasterize Layer", "This layer is locked."),
                    Surface::Selection => ("Last Filter", "no filter applied yet"),
                    Surface::Vector => ("Make Clipping Mask", "Select a clipping path and at least one content object."),
                    _ => panic!("main menu uses its separate screenshot harness"),
                };
                h.get_by_label(label).hover();
                h.run_steps(6);
                assert!(h.query_by_label(why).is_some(), "{name}: {why}");
                h.render().unwrap().save(dir.join(format!("{name}-{theme_name}-{}-reason.png", width as u32))).unwrap();
            }
        }
        for width in [760., 1440.] {
            let mut app = PhotocraftApp::new(Session::new(), Services::default());
            app.run("edit.fill", json!({})).unwrap_err();
            let mut h = Harness::builder().with_size(vec2(width, 160.)).renderer(renderer()).build_ui_state(draw, (app, Surface::Status));
            PhotocraftApp::setup_context(&h.ctx, theme);
            h.run_steps(4);
            assert!(h.query_by_label("Open a photo first.").is_some());
            assert!(h.query_by_label("Details / History").is_some());
            h.render().unwrap().save(dir.join(format!("status-{theme_name}-{}.png", width as u32))).unwrap();
        }
        let mut h = Harness::builder()
            .with_size(vec2(1000., 850.))
            .renderer(renderer())
            .build_ui_state(draw, (PhotocraftApp::new(Session::new(), Services::default()), Surface::Main));
        PhotocraftApp::setup_context(&h.ctx, theme);
        h.ctx.all_styles_mut(|s| {
            s.interaction.tooltip_delay = 0.;
            s.interaction.tooltip_grace_time = 0.;
        });
        h.run_steps(4);
        h.get_by_label("Edit").click();
        h.run_steps(4);
        h.get_by_label_contains(if cfg!(target_os = "macos") { "Fill… ⇧F5" } else { "Fill… Shift+F5" }).hover();
        h.run_steps(6);
        assert!(h.query_by_label("Open a photo first.").is_some());
        h.render().unwrap().save(dir.join(format!("main-{theme_name}-reason.png"))).unwrap();
    }
}

#[test]
fn reasons_ux_vector_clipping_and_compound_context_actions_make_and_release() {
    for clipping in [true, false] {
        let (mut a, first) = shape_app();
        let second = a.run("shape.create", json!({"kind":"ellipse","rect":[16,12,20,20],"fill":"#3070c0"})).unwrap()["layer"].as_u64().unwrap();
        a.run("layer.select", json!({"layer":first.0})).unwrap();
        a.run("layer.select", json!({"layer":second,"mode":"add"})).unwrap();
        canvas_tool_menu::open(&mut a, Tool::Move, [20., 20.]);
        let mut h = harness(a, Surface::Vector);
        let (make, release) = if clipping { ("Make Clipping Mask", "Release Clipping Mask") } else { ("Make Compound Path", "Release Compound Path") };
        h.get_by_label(make).click();
        h.run_steps(3);
        let st = h.state().0.session.active().unwrap();
        let result = st.doc.layer(st.active_layer.unwrap()).unwrap();
        if clipping {
            assert!(result.vector_mask.is_some());
        } else {
            assert!(matches!(&result.content,photocraft_doc::LayerContent::Shape(sh) if sh.path.subpaths.len()==2));
        }
        canvas_tool_menu::open(&mut h.state_mut().0, Tool::Move, [20., 20.]);
        h.run_steps(3);
        h.get_by_label(release).click();
        h.run_steps(3);
        assert_eq!(h.state().0.session.active().unwrap().selected_layers().len(), 2);
    }
}

#[test]
fn reasons_ux_details_history_real_click_reveals_raw_error() {
    let old: crate::notices::Notice = serde_json::from_value(json!({"id":1,"title":"Old error","lines":[],"error":true})).unwrap();
    assert!(!old.status_only, "older saved notices retain the results-row behavior");
    let mut app = PhotocraftApp::new(Session::new(), Services::default());
    let raw = app.run("edit.fill", json!({})).unwrap_err();
    let mut h = Harness::builder().with_size(vec2(900., 500.)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !fonts_ready(ui) {
                return;
            }
            crate::panels::status_bar(app, ui);
            crate::notices::panel(app, ui);
            crate::notices::show(app, ui.ctx());
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(4);
    assert!(h.query_by_label("Open a photo first.").is_some());
    assert!(h.query_by_label(&raw).is_none());
    h.get_by_label("Details / History").click();
    h.run_steps(4);
    assert!(h.query_by_label(&raw).is_some());
}

#[test]
fn reasons_ux_nonactive_clicked_layer_reasons_do_not_change_selection() {
    let mut a = app();
    let locked = a.session.active().unwrap().active_layer.unwrap();
    a.run("layer.setProps", json!({"locked":true})).unwrap();
    a.run("layer.new.layer", json!({"name":"Active"})).unwrap();
    let active = a.session.active().unwrap().active_layer;
    let before = a.session.journal.len();
    let mut h = Harness::builder().with_size(vec2(900., 850.)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !fonts_ready(ui) {
                return;
            }
            ui.menu_button("Clicked layer actions", |ui| {
                let layer = app.session.active().unwrap().doc.layer(locked).unwrap().clone();
                let mut actions = Vec::new();
                crate::layer_menu_ui::show(app, ui, &layer, false, &mut actions);
                for (id, params) in actions {
                    menus::invoke(app, ui.ctx(), &id, params).unwrap();
                }
            });
        },
        a,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.;
        s.interaction.tooltip_grace_time = 0.;
    });
    h.run_steps(4);
    h.get_by_label("Clicked layer actions").click();
    h.run_steps(4);
    let p = h.get_by_label("Rasterize Layer").rect().center();
    h.hover_at(p);
    h.run_steps(6);
    assert!(h.query_by_label("This layer is locked.").is_some());
    click_at(&mut h, p);
    assert_eq!(h.state().session.journal.len(), before);
    assert_eq!(h.state().session.active().unwrap().active_layer, active);
}

#[test]
fn reasons_ux_inverse_of_select_all_disables_ai_for_empty_mask() {
    let mut a = app();
    a.ui.tool = Tool::RectMarquee;
    a.run("select.all", json!({})).unwrap();
    a.run("select.inverse", json!({})).unwrap();
    assert!(a.session.active().unwrap().doc.selection.as_ref().unwrap().content_bounds().is_empty());
    for id in ["li.generativeFill", "li.removeSelection", "ai.remove"] {
        assert!(!menus::is_enabled(&a, id), "{id}");
        assert_eq!(command_feedback::disabled_reason(&a, id), "Make a selection first.");
    }
    canvas_tool_menu::open(&mut a, Tool::RectMarquee, [20., 20.]);
    assert_eq!(canvas_tool_menu::rows(a.ui.canvas_tool_menu.as_ref().unwrap()), canvas_tool_menu::NO_SELECTION_MENU);
    let before = a.session.journal.len();
    canvas_tool_menu::choose(&mut a, &egui::Context::default(), "li.removeSelection");
    assert!(a.ai_remove.is_none());
    assert_eq!(a.session.journal.len(), before);
}
