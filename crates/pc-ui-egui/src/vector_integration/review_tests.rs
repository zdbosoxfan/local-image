#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width":200,"height":200})).unwrap();
    app.run("shape.create", json!({"kind":"rect","rect":[20,20,60,60],"fill":"#ff0000"})).unwrap();
    app.ui.tool = Tool::Move;
    app.session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    app
}
fn host() -> Harness<'static, PhotocraftApp> {
    Harness::builder().with_size(egui::vec2(1200., 800.)).build_ui_state(
        |ui, app| {
            frame(app, ui.ctx());
            egui::CentralPanel::default().show(ui, |ui| {
                context(app, ui);
            });
        },
        app(),
    )
}
#[test]
fn bugfix_vector_menu_transforms_survive_production_frames() {
    use crate::vector_transform_tools::Kind;
    for tool in [Tool::Move, Tool::Pen, Tool::Rotate] {
        for kind in [Kind::Move, Kind::Rotate, Kind::Reflect, Kind::Scale, Kind::Shear, Kind::Each] {
            let mut h = host();
            h.state_mut().ui.tool = tool;
            h.run();
            let ctx = h.ctx.clone();
            let suffix = match kind {
                Kind::Move => "move",
                Kind::Rotate => "rotate",
                Kind::Reflect => "reflect",
                Kind::Scale => "scale",
                Kind::Shear => "shear",
                Kind::Each => "each",
                _ => panic!("unexpected transform kind"),
            };
            crate::menus::invoke(h.state_mut(), &ctx, &format!("ui.vector.transform.{suffix}"), json!({})).unwrap();
            h.run();
            h.step();
            assert!(h.state().vector_transform_tools.dialog_open(), "{tool:?}/{kind:?}");
            assert_eq!(h.state().vector_transform_tools.kind, kind);
        }
    }
}
#[test]
fn bugfix_vector_focus_loss_and_foreign_dialog_preserve_recolor_and_ase() {
    let mut h = host();
    let app = h.state_mut();
    let ids = app.session.active().unwrap().selected_layers();
    app.vector_runtime.recolor.open(&app.session, photocraft_engine::vector_packages::colours::access().unwrap(), &ids).unwrap();
    let capture = photocraft_engine::vector_packages::colours::Capture::begin(&app.session).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    app.vector_runtime.ase = Some(rx);
    app.ui.dialogs.push(crate::state::Dialog { id: 987, kind: crate::state::DialogKind::About, fields: Default::default() });
    h.input_mut().focused = false;
    h.event(egui::Event::WindowFocused(false));
    h.step();
    assert!(h.state().vector_runtime.recolor.plan.is_some());
    assert!(h.state().vector_runtime.ase.is_some());
    let mut palette = photocraft_doc::vector_design::swatches::SwatchBook::default();
    palette.swatches.push(photocraft_doc::vector_design::swatches::Swatch {
        id: photocraft_doc::vector_design::swatches::SwatchId(1),
        name: "Imported".into(),
        paint: photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK),
        global: false,
        spot: false,
        group: None,
        stops: vec![],
    });
    tx.send((capture, Ok((palette, vec![])))).unwrap();
    h.input_mut().focused = true;
    h.event(egui::Event::WindowFocused(true));
    h.step();
    assert!(h.state().session.active().unwrap().doc.swatches.swatches.iter().any(|s| s.name == "Imported"));
}
#[test]
fn bugfix_vector_path_dialog_drawn_with_text_focus() {
    let mut h = host();
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    crate::object_path::app_hooks::menu_command(app, &ctx, "simplify").unwrap();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Curve precision").click();
    h.run();
    h.step();
    assert!(crate::object_path::app_hooks::bridge(&h.ctx).state.is_open());
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Curve precision");
}
#[test]
fn bugfix_vector_printed_command_and_tool_bindings_match_dispatch() {
    let mut app = app();
    app.session.edit_prefs(|p| {
        p.set_shortcut("select.deselect", Some("Cmd+D"));
    });
    let bindings = crate::shortcut_dispatch::binding_strings(&app);
    assert!(bindings.iter().any(|(id, sc)| id == "select.deselect" && sc == "Cmd+D"));
    assert!(!bindings.iter().any(|(id, sc)| id != "select.deselect" && sc == "Cmd+D"));
    let items = crate::menus::menu_items(&app);
    for item in items.iter().filter(|i| crate::menus::is_live(&i.id)) {
        assert_eq!(item.shortcut.as_ref(), bindings.iter().find(|(id, _)| id == &item.id).map(|(_, key)| key), "{}", item.id);
    }
    assert_eq!(crate::tool_tips::shortcut(&app.session, Tool::Rectangle), Some("M".into()));
    assert_eq!(items.iter().find(|item| item.id == "object.path.join").unwrap().shortcut.as_deref(), Some("Cmd+J"));
    assert!(items.iter().any(|item| item.id == "ui.vector.path.offset"));
    let ctx = egui::Context::default();
    crate::menus::invoke(&mut app, &ctx, "ui.vector.path.offset", json!({})).unwrap();
    assert!(crate::object_path::app_hooks::bridge(&ctx).state.is_open());
    let mut joined = self::app();
    let id = joined.session.active().unwrap().active_layer.unwrap();
    joined
        .run(
            "shape.edit",
            json!({"layer":id.0,"path":{"subpaths":[
                {"closed":false,"knots":[[20,20],[40,20]]}, {"closed":false,"knots":[[40,20],[60,30]]}
            ]}}),
        )
        .unwrap();
    let before = joined.session.active().unwrap().doc.clone();
    let journal = joined.session.journal.len();
    crate::menus::invoke(&mut joined, &egui::Context::default(), "object.path.join", json!({})).unwrap();
    let LayerContent::Shape(shape) = &joined.session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(shape.path.subpaths.len(), 1);
    assert_eq!(shape.path.subpaths[0].knots.len(), 3);
    assert_eq!(joined.session.journal.len(), journal + 1);
    assert!(joined.session.undo());
    assert_eq!(joined.session.active().unwrap().doc, before);
}

#[test]
fn bugfix_vector_shared_options_own_nodes_and_live_shapes() {
    let mut app = app();
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |root| {
        egui::CentralPanel::default().show(root, |ui| {
            for tool in [Tool::DirectSelection, Tool::Rectangle, Tool::EllipseShape, Tool::Polygon, Tool::Triangle, Tool::Line] {
                assert!(!crate::vector_ui::options_bar(&mut app, ui, tool), "{tool:?}");
            }
        });
    });
    output.textures_delta.clear();
}
#[test]
fn bugfix_vector_design_rectangle_move_bar_exposes_corners() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut app = app();
        app.session.edit_prefs(|p| p.toolbar.persona = persona);
        crate::vector_ui::finish_shape(&mut app, Tool::Rectangle, [30., 30.], [90., 90.], egui::Modifiers::NONE);
        let before = app.session.active().unwrap().doc.clone();
        let journal = app.session.journal.len();
        let mut h = Harness::builder().with_size(egui::vec2(1800., 500.)).build_ui_state(
            |ui, app| {
                crate::vector_ui::shape_layer_bar(app, ui);
            },
            app,
        );
        h.run();
        h.get_by_label("Corners").click();
        h.run();
        let at = h.get_by_label("Top left").rect().center();
        h.event(egui::Event::PointerMoved(at));
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(egui::Event::PointerMoved(at + egui::vec2(10., 0.)));
        h.step();
        h.event(egui::Event::PointerButton {
            pos: at + egui::vec2(10., 0.),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        let st = h.state().session.active().unwrap();
        let design = st.doc.layer(st.active_layer.unwrap()).unwrap().vector.live_shape.as_ref().unwrap();
        let photocraft_doc::vector_design::drawing::ShapeParameters::Rect { radii, .. } = &design.parameters else { panic!() };
        assert!(radii[0] > 0.);
        assert_eq!(h.state().session.journal.len(), journal + 1);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}
#[test]
fn bugfix_vector_place_svg_uses_picker() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let picked = Arc::new(AtomicBool::new(false));
    let flag = picked.clone();
    let mut app = app();
    app.services.file_dialog = Some(crate::file_dialog::answering(move |_| {
        flag.store(true, Ordering::SeqCst);
        None
    }));
    let mut h = Harness::builder().with_size(egui::vec2(1500., 500.)).build_ui_state(
        |ui, app| {
            crate::toolsets_ui::persona::geometry_bar(app, ui);
        },
        app,
    );
    h.run();
    h.get_by_label("Place SVG…").click();
    h.run();
    assert!(picked.load(Ordering::SeqCst));
}
struct CanvasHost {
    app: PhotocraftApp,
    rect: egui::Rect,
    overlay: bool,
    zoom: f32,
}
fn canvas_host(overlay: bool) -> Harness<'static, CanvasHost> {
    let mut app = app();
    app.ui.tool = Tool::Pen;
    Harness::builder().with_size(egui::vec2(800., 600.)).build_ui_state(
        |ui, h| {
            frame(&mut h.app, ui.ctx());
            if h.overlay {
                egui::Window::new("Overlay").fixed_pos(egui::pos2(40., 40.)).fixed_size(egui::vec2(200., 200.)).show(ui.ctx(), |ui| {
                    ui.label("Floating vector panel");
                });
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let response = ui.allocate_rect(egui::Rect::from_min_size(egui::pos2(0., 0.), egui::vec2(300., 300.)), egui::Sense::click_and_drag());
                h.rect = response.rect;
                let xf = ViewXform { rect: h.rect, zoom: h.zoom, center: [100., 100.], flip: false, rotation: 0.0 };
                input(&mut h.app, ui, 0, &response, &xf, h.rect);
            });
        },
        CanvasHost { app, rect: egui::Rect::NOTHING, overlay, zoom: 1. },
    )
}
#[test]
fn bugfix_vector_floating_panel_click_does_not_start_pen() {
    for persona in [Persona::Vector, Persona::Pixel] {
        for tool in [Tool::Pen, Tool::Rectangle] {
            for overlay in [true, false] {
                let mut h = canvas_host(overlay);
                h.state_mut().app.session.edit_prefs(|p| p.toolbar.persona = persona);
                h.state_mut().app.ui.tool = tool;
                if !overlay {
                    h.state_mut().zoom = 5.;
                }
                h.run();
                let pos = if overlay { egui::pos2(80., 80.) } else { egui::pos2(295., 150.) };
                h.event(egui::Event::PointerMoved(pos));
                h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
                h.step();
                assert!(h.state().app.shared_tools.pen.gesture.is_none(), "{persona:?}/{tool:?}/{overlay}");
                assert!(!h.state().app.shared_tools.drawing.is_active(), "{persona:?}/{tool:?}/{overlay}");
            }
        }
    }
}
#[test]
fn bugfix_vector_space_pan_does_not_start_pen() {
    for persona in [Persona::Vector, Persona::Pixel] {
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::COMMAND] {
            let mut h = canvas_host(false);
            h.state_mut().app.session.edit_prefs(|p| p.toolbar.persona = persona);
            h.run();
            h.event(egui::Event::ModifiersChanged(modifiers));
            h.event(egui::Event::Key { key: egui::Key::Space, physical_key: None, pressed: true, repeat: false, modifiers });
            let pos = egui::pos2(100., 100.);
            h.event(egui::Event::PointerMoved(pos));
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers });
            h.step();
            assert!(h.state().app.shared_tools.pen.gesture.is_none(), "{persona:?}/{modifiers:?}");
        }
    }
}
#[test]
fn bugfix_vector_closed_document_releases_vector_view_cache() {
    let mut app = app();
    let source = app.session.active().unwrap().doc.clone();
    let weak = Arc::downgrade(&source);
    views::display(&mut app, 0, source.clone(), source.clone(), 0).unwrap();
    drop(source);
    app.session.close(0);
    crate::canvas::retain_gpu_documents(&mut app);
    assert!(weak.upgrade().is_none());
}
#[test]
fn bugfix_vector_puppet_delete_runs_before_generic_clear() {
    let mut app = app();
    app.ui.tool = Tool::VectorPuppetWarp;
    let ctx = egui::Context::default();
    let mut b = crate::distort_tools::bridge(&ctx);
    b.state.pins = serde_json::from_value(json!([{"rest":[30.,30.],"at":[30.,30.],"angle":null},{"rest":[70.,70.],"at":[70.,70.],"angle":null}])).unwrap();
    b.state.selected_pins = vec![0];
    crate::distort_tools::save_bridge(&ctx, b);
    let raw = egui::RawInput {
        events: vec![egui::Event::Key { key: egui::Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
        ..Default::default()
    };
    ctx.run_ui(raw, |ui| crate::shortcuts::handle(&mut app, ui.ctx())).textures_delta.clear();
    assert_eq!(crate::distort_tools::bridge(&ctx).state.pins.len(), 1);
    assert!(!app.ui.status_error, "{}", app.ui.status);
}
#[test]
fn bugfix_vector_pixel_polygon_survives_focus_loss() {
    let mut app = app();
    app.session.edit_prefs(|p| p.toolbar.persona = Persona::Pixel);
    app.ui.tool = Tool::PolygonLasso;
    let mut h = Harness::builder().build_ui_state(|ui, app| frame(app, ui.ctx()), app);
    h.run();
    let polygon = vec![[20., 20.], [50., 20.], [50., 50.]];
    h.state_mut().ui.polygon = polygon.clone();
    h.input_mut().focused = false;
    h.event(egui::Event::WindowFocused(false));
    h.step();
    assert_eq!(h.state().ui.polygon, polygon);
}

#[test]
fn bugfix_vector_menu_transform_numeric_ok_copy_and_undo() {
    for button in ["OK", "Copy"] {
        let mut h = host();
        h.run();
        let ctx = h.ctx.clone();
        let before = h.state().session.active().unwrap().doc.clone();
        let journal = h.state().session.journal.len();
        crate::menus::invoke(h.state_mut(), &ctx, "ui.vector.transform.move", json!({})).unwrap();
        h.run();
        h.get_by_label("Horizontal").type_text("20");
        h.run();
        h.step();
        assert!(h.state().vector_transform_tools.dialog_open());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.get_by_label(button).click();
        h.run();
        assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
        assert_eq!(h.state().session.journal.len(), journal + 1);
        let count = before.layer_count() + usize::from(button == "Copy");
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), count);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}

#[test]
fn bugfix_vector_escape_in_recolor_text_field_preserves_plan() {
    let mut h = host();
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "ui.vector.recolor", json!({})).unwrap();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Assign Row 1").click();
    h.run();
    h.key_press(egui::Key::Escape);
    h.step();
    assert!(h.state().vector_runtime.recolor.plan.is_some());
}

#[test]
fn bugfix_vector_actual_object_transform_menu_click() {
    let mut h = Harness::builder().with_size(egui::vec2(1200., 800.)).build_ui_state(
        |ui, app| {
            frame(app, ui.ctx());
            crate::menus::menu_bar(app, ui);
        },
        app(),
    );
    h.run();
    h.get_by_label("Object").click();
    h.run();
    h.get_by_label("Transform ⏵").hover();
    h.run();
    h.get_by_label("Move…").click();
    h.run();
    h.step();
    assert!(h.state().vector_transform_tools.dialog_open());
    h.get_by_label("Horizontal");
}
#[test]
fn bugfix_vector_preferences_preset_apply_cancel_and_custom_keys() {
    use photocraft_engine::vector_toolbar_layout::Preset;
    let mut app = app();
    let original = app.session.prefs().toolbar.vector_keyboard.preset;
    let id = crate::prefs_ui::open_preferences(&mut app, "tools");
    let mut fonts_installed = false;
    let mut h = Harness::builder().with_size(egui::vec2(1200., 800.)).build_ui_state(
        move |ui, app| {
            if !fonts_installed {
                crate::theme::install_fonts(ui.ctx());
                fonts_installed = true;
                ui.ctx().request_repaint();
                return;
            }
            crate::dialogs::show(app, ui.ctx());
        },
        app,
    );
    h.run();
    h.get_by_label("Vector Keyboard Preset");
    h.state_mut().ui.dialog_mut(id).unwrap().fields["values"]["toolbar"]["vectorKeyboard"]["preset"] = json!("affinity");
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(h.state().session.prefs().toolbar.vector_keyboard.preset, Preset::Affinity);
    let bindings = crate::shortcut_dispatch::binding_strings(h.state());
    assert!(bindings.contains(&("layer.duplicate".into(), "Cmd+J".into())));
    assert!(bindings.contains(&("select.deselect".into(), "Cmd+D".into())));
    h.state_mut().ui.dialog_mut(id).unwrap().fields["values"]["toolbar"]["vectorKeyboard"]["preset"] = json!(original);
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(h.state().session.prefs().toolbar.vector_keyboard.preset, Preset::Affinity);
    let mut app = self::app();
    app.session.edit_prefs(|p| {
        p.set_shortcut("tools.select.Pen", Some("M"));
        p.set_shortcut("select.deselect", Some("Cmd+D"));
    });
    let mut h = Harness::builder().build_ui_state(
        |ui, app| {
            crate::shortcuts::handle(app, ui.ctx());
        },
        app,
    );
    h.key_press(egui::Key::M);
    h.run();
    assert_eq!(h.state().ui.tool, Tool::Pen);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    assert!(h.state().session.active().unwrap().selected_layers().is_empty());
}

#[test]
fn dsfix_outline_and_transform_again_menu_reasons() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    for id in ["path.outlineStroke", "vector.transform.again"] {
        assert!(disabled_reason(&app, id).is_some(), "{id} without document");
    }
    app.run("file.new", json!({"width":32,"height":32})).unwrap();
    app.session.edit_prefs(|p| p.toolbar.persona = Persona::Vector);
    assert!(disabled_reason(&app, "path.outlineStroke").is_some(), "a raster cannot be outlined");
    assert!(disabled_reason(&app, "vector.transform.again").is_some(), "no previous transform");
    app.run("shape.create", json!({"kind":"rect","rect":[1,1,10,10],"fill":"#ffffff","stroke":{"color":"#000000","width":2}})).unwrap();
    assert!(disabled_reason(&app, "path.outlineStroke").is_none(), "an ordinary stroked shape can be outlined");
}
