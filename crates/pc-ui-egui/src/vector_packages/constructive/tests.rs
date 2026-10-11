#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, Key, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size, Subpath};

struct Host {
    session: Session,
    tools: State,
    origin: Pos2,
}
fn fixture(tool: Tool, circles: bool) -> Host {
    let mut doc = Document::new("Constructive UI", Size::new(160, 160), ColorMode::Rgb, SampleType::F32);
    let paths = if circles {
        vec![photocraft_vector::shapes::ellipse(16., 24., 80., 80.), photocraft_vector::shapes::ellipse(56., 24., 80., 80.)]
    } else {
        vec![photocraft_vector::shapes::rect(20., 20., 100., 100.)]
    };
    let mut targets = Vec::new();
    for path in paths {
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
        let layer = Layer::new("Shape", LayerContent::Shape(sh));
        targets.push(layer.id);
        doc.layers.push(layer);
    }
    let mut session = Session::new();
    session.add_document(doc, None);
    Host { session, tools: State { tool: Some(tool), targets, ..Default::default() }, origin: Pos2::ZERO }
}
fn ui(ui: &mut egui::Ui, host: &mut Host) {
    host.tools.frame(&host.session, ui.ctx(), host.tools.tool.is_some());
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| host.tools.context_bar(ui, &mut host.session));
        let (response, painter) = ui.allocate_painter(egui::vec2(180., 180.), egui::Sense::click_and_drag());
        host.origin = response.rect.min;
        let xf = crate::canvas::ViewXform { rect: response.rect, zoom: 1., center: [90., 90.], flip: false };
        host.tools.canvas_input(&mut host.session, &response, &xf);
        host.tools.draw_overlay(&painter, &|p| host.origin + egui::vec2(p.x as f32, p.y as f32), 1.);
    });
}
fn harness(tool: Tool, circles: bool) -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(700., 330.)).build_ui_state(ui, fixture(tool, circles))
}
fn move_to(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool, mods: Modifiers) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}
#[test]
fn constructive_ui_builder_hover_drag_merge_and_alt_mid_drag_remove() {
    for remove in [false, true] {
        let mut h = harness(Tool::Builder, true);
        let before = h.state().session.active().unwrap().doc.clone();
        move_to(&mut h, 32., 64.);
        assert!(h.state().tools.hover.is_some());
        button(&mut h, 32., 64., true, Modifiers::NONE);
        move_to(&mut h, 76., 64.);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        assert!(h.state().tools.display_document(&h.state().session).is_some());
        let mods = if remove { Modifiers::ALT } else { Modifiers::NONE };
        h.event(Event::ModifiersChanged(mods));
        h.step();
        assert_eq!(h.state().tools.removing, remove);
        button(&mut h, 76., 64., false, mods);
        assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
        assert_eq!(h.state().session.active().unwrap().doc.layers.len(), if remove { 1 } else { 2 });
        assert_eq!(h.state().session.journal.len(), 1);
        let host = h.state_mut();
        assert!(host.session.undo());
        assert_eq!(host.session.active().unwrap().doc, before);
        assert!(!host.session.undo());
    }
}
#[test]
fn constructive_ui_knife_live_preview_release_and_undo() {
    let mut h = harness(Tool::Knife, false);
    let before = h.state().session.active().unwrap().doc.clone();
    move_to(&mut h, 70., 0.);
    button(&mut h, 70., 0., true, Modifiers::NONE);
    move_to(&mut h, 70., 140.);
    assert!(h.state().tools.is_dragging());
    assert!(h.state().tools.display_document(&h.state().session).is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    button(&mut h, 70., 150., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn constructive_ui_contour_drag_cancel_focus_switch_stale_and_outside_release() {
    let mut h = harness(Tool::Contour, false);
    let before = h.state().session.active().unwrap().doc.clone();
    move_to(&mut h, 120., 70.);
    button(&mut h, 120., 70., true, Modifiers::NONE);
    move_to(&mut h, 130., 70.);
    assert!((h.state().tools.delta - 10.).abs() < 1e-6);
    assert!(h.state().tools.display_document(&h.state().session).is_some());
    h.key_press(Key::Escape);
    h.run();
    button(&mut h, 130., 70., false, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    for cancel in 0..3 {
        move_to(&mut h, 120., 70.);
        button(&mut h, 120., 70., true, Modifiers::NONE);
        move_to(&mut h, 130., 70.);
        match cancel {
            0 => {
                h.event(Event::WindowFocused(false));
                h.step();
                h.event(Event::WindowFocused(true));
                h.step();
            }
            1 => {
                h.state_mut().tools.set_tool(Some(Tool::Knife));
                h.state_mut().tools.set_tool(Some(Tool::Contour));
            }
            _ => {
                h.state_mut().session.active_mut().unwrap().revision += 1;
                move_to(&mut h, 135., 70.);
            }
        }
        button(&mut h, 135., 70., false, Modifiers::NONE);
        assert!(!h.state().tools.is_dragging());
        assert!(h.state().session.journal.is_empty());
    }
    move_to(&mut h, 120., 70.);
    button(&mut h, 120., 70., true, Modifiers::NONE);
    move_to(&mut h, 130., 70.);
    button(&mut h, 210., 70., false, Modifiers::NONE);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(!h.state().tools.is_dragging());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn constructive_ui_contour_numeric_type_enter_and_escape() {
    for cancel in [false, true] {
        let mut h = harness(Tool::Contour, false);
        let before = h.state().session.active().unwrap().doc.clone();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").type_text("12");
        h.run();
        assert!(h.state().session.journal.is_empty());
        h.key_press(if cancel { Key::Escape } else { Key::Enter });
        h.run();
        assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
        if cancel {
            assert!(h.state().session.journal.is_empty());
            assert_eq!(h.state().session.active().unwrap().doc, before);
        } else {
            assert_eq!(h.state().session.journal.len(), 1);
            assert!(h.state_mut().session.undo());
            assert_eq!(h.state().session.active().unwrap().doc, before);
        }
    }
}
#[test]
fn constructive_ui_scissors_click_and_line_cut_shift_modifier() {
    let mut h = harness(Tool::Scissors, false);
    move_to(&mut h, 70., 20.);
    button(&mut h, 70., 20., true, Modifiers::NONE);
    button(&mut h, 70., 20., false, Modifiers::NONE);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state().tools.error.is_none());
    assert!(h.state_mut().session.undo());
    h.state_mut().tools.set_tool(Some(Tool::LineCut));
    move_to(&mut h, 70., 40.);
    button(&mut h, 70., 40., true, Modifiers::NONE);
    move_to(&mut h, 75., 100.);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.step();
    let Action::LineCut { from, to } = h.state().tools.drag.as_ref().unwrap().action.as_ref().unwrap() else { panic!() };
    assert!((from[0] - to[0]).abs() < 1e-6);
    button(&mut h, 75., 100., false, Modifiers::SHIFT);
    assert!(h.state().tools.error.is_none());
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
}
#[test]
fn constructive_ui_smooth_eraser_and_join_use_actual_scrubs() {
    let mut h = harness(Tool::Smooth, false);
    let id = h.state().tools.targets[0];
    let mut doc = (*h.state().session.active().unwrap().doc).clone();
    let LayerContent::Shape(sh) = &mut doc.layer_mut(id).unwrap().content else { panic!() };
    sh.path = Path::new(vec![Subpath::polyline(&[(20., 70.), (50., 80.), (80., 60.), (120., 70.)])]);
    h.state_mut().session.active_mut().unwrap().doc = Arc::new(doc);
    move_to(&mut h, 80., 50.);
    button(&mut h, 80., 50., true, Modifiers::NONE);
    move_to(&mut h, 80., 80.);
    button(&mut h, 80., 80., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    h.state_mut().tools.set_tool(Some(Tool::Eraser));
    move_to(&mut h, 65., 40.);
    button(&mut h, 65., 40., true, Modifiers::NONE);
    move_to(&mut h, 65., 100.);
    button(&mut h, 65., 100., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    let ids = h.state().session.active().unwrap().doc.layers.iter().map(|l| l.id).collect();
    h.state_mut().tools.set_targets(ids);
    h.state_mut().tools.set_tool(Some(Tool::Join));
    h.state_mut().tools.radius = 20.;
    move_to(&mut h, 45., 70.);
    button(&mut h, 45., 70., true, Modifiers::NONE);
    move_to(&mut h, 85., 70.);
    button(&mut h, 85., 70., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
}

#[test]
fn constructive_ui_knife_keeps_all_pointer_samples_in_one_frame() {
    let mut h = harness(Tool::Knife, false);
    move_to(&mut h, 70., 0.);
    button(&mut h, 70., 0., true, Modifiers::NONE);
    let origin = h.state().origin;
    for p in [(25., 50.), (115., 90.), (70., 140.)] {
        h.event(Event::PointerMoved(origin + egui::vec2(p.0, p.1)));
    }
    h.step();
    let points = &h.state().tools.drag.as_ref().unwrap().points;
    assert!(points.contains(&Point::new(25., 50.)));
    assert!(points.contains(&Point::new(115., 90.)));
    assert_eq!(points.last(), Some(&Point::new(70., 140.)));
    h.key_press(Key::Escape);
    h.run();
    button(&mut h, 70., 140., false, Modifiers::NONE);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn constructive_ui_numeric_stale_revision_abandons_typed_offset() {
    let mut h = harness(Tool::Contour, false);
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").type_text("12");
    h.run();
    h.state_mut().session.active_mut().unwrap().revision += 1;
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().session.journal.is_empty());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().tools.preview.is_none());
}

#[test]
fn constructive_ui_contour_numeric_uses_document_points_and_dpi() {
    let mut h = harness(Tool::Contour, false);
    let mut doc = (*h.state().session.active().unwrap().doc).clone();
    doc.design_units = photocraft_doc::vector_design::structure::DesignUnits::Points;
    doc.resolution_dpi = 144.;
    h.state_mut().session.active_mut().unwrap().doc = Arc::new(doc);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Offset").type_text("10");
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layers[0].content else { panic!() };
    let (x0, y0, x1, y1) = sh.path.control_bounds().unwrap();
    assert!(x0.abs() < 0.01 && y0.abs() < 0.01 && (x1 - 140.).abs() < 0.01 && (y1 - 140.).abs() < 0.01);
    assert_eq!(h.state().session.journal.len(), 1);
}
