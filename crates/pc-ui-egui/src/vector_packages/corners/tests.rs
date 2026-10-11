use super::*;
use egui::{Event, Key, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};

struct Host {
    session: Session,
    corners: State,
    origin: Pos2,
}
fn fixture() -> Host {
    let mut session = Session::new();
    let mut doc = Document::new("Corner UI", Size::new(180, 180), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20.0, 20.0), (120.0, 20.0), (120.0, 120.0), (20.0, 120.0)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    let layer = Layer::new("Square", LayerContent::Shape(sh));
    let id = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    Host { session, corners: State { layer: Some(id), active: true, ..Default::default() }, origin: Pos2::ZERO }
}
fn host_ui(ui: &mut egui::Ui, host: &mut Host) {
    let ctx = ui.ctx().clone();
    host.corners.lifecycle(&host.session, true, ctx.input(|i| i.focused), ctx.input(|i| i.key_pressed(Key::Escape)));
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| host.corners.context_bar(ui, &mut host.session));
        let (response, painter) = ui.allocate_painter(egui::vec2(200.0, 200.0), egui::Sense::click_and_drag());
        host.origin = response.rect.min;
        let xf = crate::canvas::ViewXform { rect: response.rect, zoom: 1.0, center: [100.0, 100.0], flip: false };
        host.corners.canvas_input(&mut host.session, &response, &xf);
        host.corners.draw_overlay(&host.session, &painter, &|p| host.origin + egui::vec2(p.x as f32, p.y as f32), 1.0);
    });
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(500.0, 330.0)).build_ui_state(host_ui, fixture())
}
fn move_to(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
#[test]
fn corners_ui_drag_per_node_all_preview_release_and_single_undo() {
    for all in [true, false] {
        let mut h = harness();
        if !all {
            h.get_by_label("All corners").click();
            h.run();
        }
        let before = h.state().session.active().unwrap().doc.clone();
        move_to(&mut h, 30.0, 30.0);
        button(&mut h, 30.0, 30.0, true);
        move_to(&mut h, 40.0, 40.0);
        assert!(h.state().corners.is_dragging());
        assert!(h.state().corners.display_document(&h.state().session).is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        button(&mut h, 45.0, 45.0, false);
        assert!(h.state().corners.error.is_none(), "{:?}", h.state().corners.error);
        let host = h.state_mut();
        let l = host.session.active().unwrap().doc.layers.last().unwrap();
        assert_eq!(l.vector.corners.len(), if all { 4 } else { 1 });
        assert!(l.vector.corners.iter().all(|c| (c.corner.radius - 15.0).abs() < 1e-6));
        assert_eq!(host.session.journal.len(), 1);
        assert!(host.session.undo());
        assert_eq!(host.session.active().unwrap().doc, before);
        assert!(!host.session.undo());
    }
}
#[test]
fn corners_ui_escape_focus_tool_switch_and_stale_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    move_to(&mut h, 30.0, 30.0);
    button(&mut h, 30.0, 30.0, true);
    move_to(&mut h, 50.0, 50.0);
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().corners.is_dragging());
    button(&mut h, 50.0, 50.0, false);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    for (active, focused) in [(false, true), (true, false)] {
        let host = h.state_mut();
        host.corners.pointer(&mut host.session, crate::canvas::ToolEvent::Down { x: 30.0, y: 30.0, pressure: 1.0 }, Modifiers::NONE, 1.0);
        host.corners.lifecycle(&host.session, active, focused, false);
        assert!(!host.corners.is_dragging());
    }
    move_to(&mut h, 30.0, 30.0);
    button(&mut h, 30.0, 30.0, true);
    h.state_mut().session.active_mut().unwrap().revision += 1;
    move_to(&mut h, 60.0, 60.0);
    button(&mut h, 60.0, 60.0, false);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn corners_ui_kind_menu_and_numeric_typing() {
    let mut h = harness();
    move_to(&mut h, 30.0, 30.0);
    button(&mut h, 30.0, 30.0, true);
    move_to(&mut h, 40.0, 40.0);
    button(&mut h, 40.0, 40.0, false);
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Corner type").click();
    h.run();
    h.get_by_label("Inverted").click();
    h.run();
    assert!(
        h.state()
            .session
            .active()
            .unwrap()
            .doc
            .layers
            .last()
            .unwrap()
            .vector
            .corners
            .iter()
            .all(|c| c.corner.kind == CornerKind::Inverted && c.corner.radius == 10.0)
    );
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").type_text("25");
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().corners.error.is_none(), "{:?}", h.state().corners.error);
    assert!(h.state().session.active().unwrap().doc.layers.last().unwrap().vector.corners.iter().all(|c| c.corner.radius == 25.0));
}
#[test]
fn corners_ui_kind_choice_preserves_mixed_per_node_radii() {
    let mut h = harness();
    let id = h.state().corners.layer.unwrap();
    let mut g = CornerGesture::begin(&h.state().session, &[id]).unwrap();
    g.set(id, (0..4).map(|knot| KnotCorner { subpath: 0, knot, corner: Corner { radius: 5.0 + 5.0 * knot as f64, kind: CornerKind::Round } }).collect())
        .unwrap();
    g.commit(&mut h.state_mut().session).unwrap();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Corner type").click();
    h.run();
    h.get_by_label("Chamfer").click();
    h.run();
    let corners = &h.state().session.active().unwrap().doc.layers.last().unwrap().vector.corners;
    assert_eq!(corners.iter().map(|c| c.corner.radius).collect::<Vec<_>>(), [5.0, 10.0, 15.0, 20.0]);
    assert!(corners.iter().all(|c| c.corner.kind == CornerKind::Chamfer));
}
#[test]
fn corners_ui_numeric_radius_uses_document_points_and_dpi() {
    let mut h = harness();
    let mut doc = (*h.state().session.active().unwrap().doc).clone();
    doc.design_units = photocraft_doc::vector_design::structure::DesignUnits::Points;
    doc.resolution_dpi = 144.0;
    h.state_mut().session.active_mut().unwrap().doc = Arc::new(doc);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").type_text("10");
    h.key_press(egui::Key::Enter);
    h.run();
    let corners = &h.state().session.active().unwrap().doc.layers.last().unwrap().vector.corners;
    assert_eq!(corners.len(), 4);
    assert!(corners.iter().all(|c| c.corner.radius == 20.0));
}

#[test]
fn corners_ui_numeric_escape_and_stale_buffer_do_not_commit() {
    for stale in [false, true] {
        let mut h = harness();
        let before = h.state().session.active().unwrap().doc.clone();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Radius").type_text("25");
        h.run();
        assert!(h.state().corners.controls_dirty);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        if stale {
            h.state_mut().session.active_mut().unwrap().revision += 1;
            h.key_press(Key::Enter);
        } else {
            h.key_press(Key::Escape);
        }
        h.run();
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        assert_eq!(h.state().corners.radius, 0.0);
    }
}

#[test]
fn corners_ui_modifier_changes_and_release_outside_keep_capture() {
    let mut h = harness();
    move_to(&mut h, 30.0, 30.0);
    button(&mut h, 30.0, 30.0, true);
    move_to(&mut h, 40.25, 40.25);
    assert_eq!(h.state().corners.radius, 10.25);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.step();
    assert_eq!(h.state().corners.radius, 10.0);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.step();
    assert_eq!(h.state().corners.radius, 10.25);
    button(&mut h, 260.0, 260.0, false);
    assert!(!h.state().corners.is_dragging());
    assert!(h.state().corners.capture.is_none());
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state().session.active().unwrap().doc.layers.last().unwrap().vector.corners.iter().all(|c| c.corner.radius == 50.0));
}
