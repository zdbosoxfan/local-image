use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size, Subpath};
pub(super) struct Host {
    pub session: Session,
    pub state: State,
    pub origin: Pos2,
    pub group: LayerId,
    pub source: LayerId,
}
pub(super) fn fixture() -> Host {
    let mut session = Session::new();
    let mut doc = Document::new("Envelope UI", Size::new(192, 160), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20.0, 30.0), (140.0, 30.0), (140.0, 130.0), (20.0, 130.0)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let layer = Layer::new("Editable path", LayerContent::Shape(shape));
    let source = layer.id;
    let layer = Layer::group("Envelope sources", vec![layer]);
    let group = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    Host { session, state: State { target: Some(group), ..Default::default() }, origin: Pos2::ZERO, group, source }
}
fn host_ui(ui: &mut egui::Ui, h: &mut Host) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| h.state.context_bar(ui, &mut h.session));
        let (response, painter) = ui.allocate_painter(egui::vec2(192.0, 160.0), egui::Sense::click_and_drag());
        h.origin = response.rect.min;
        let xf = ViewXform { rect: response.rect, zoom: 1.0, center: [96.0, 80.0], flip: false };
        h.state.canvas_input(&mut h.session, &response, &xf);
        h.state.draw_overlay(&h.session, &painter, &xf);
    });
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1200.0, 500.0)).build_ui_state(host_ui, fixture())
}
pub(super) fn pointer(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: Option<bool>) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
#[test]
fn envelope_ui_mesh_drag_preview_release_expand_and_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    assert!(h.state().state.is_dragging());
    assert!(h.state().state.display_document(&h.state().session).is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 100.0, 60.0, Some(false));
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert_eq!(h.state().session.journal.len(), 1);
    let live = h.state().session.active().unwrap().doc.clone();
    let Some(LiveGroup::Envelope(EnvelopeSpec::Mesh { points, .. })) = &live.layer(h.state().group).unwrap().vector.live else { panic!("live mesh") };
    assert!((points[4][0] - 2.0 / 3.0).abs() < 1e-6);
    assert!((points[4][1] - 0.3).abs() < 1e-6);
    h.get_by_label("Expand envelope").click();
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert!(h.state().session.active().unwrap().doc.layer(h.state().group).unwrap().vector.live.is_none());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, live);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn envelope_ui_real_escape_focus_stale_and_return_to_origin_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    h.key_press(Key::Escape);
    h.run();
    pointer(&mut h, 100.0, 60.0, Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    h.event(Event::WindowFocused(false));
    h.step();
    pointer(&mut h, 100.0, 60.0, Some(false));
    assert!(!h.state().state.is_dragging());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.event(Event::WindowFocused(true));
    h.step();
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    h.state_mut().session.active_mut().unwrap().revision += 1;
    pointer(&mut h, 100.0, 60.0, Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    pointer(&mut h, 80.0, 80.0, Some(false));
    let doc = &h.state().session.active().unwrap().doc;
    let Some(LiveGroup::Envelope(EnvelopeSpec::Mesh { points, .. })) = &doc.layer(h.state().group).unwrap().vector.live else { panic!("live mesh") };
    assert_eq!(points[4], [0.5, 0.5]);
}
#[test]
fn envelope_ui_numeric_typing_style_apply_and_cancel() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Envelope kind").click();
    h.run();
    h.get_by_label("Warp").click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Bend").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Bend").type_text("40");
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert!(h.state().session.journal.is_empty());
    assert!(h.state().state.display_document(&h.state().session).is_some());
    h.get_by_label("Apply envelope").click();
    h.run();
    let doc = &h.state().session.active().unwrap().doc;
    let Some(LiveGroup::Envelope(EnvelopeSpec::Warp { bend, .. })) = &doc.layer(h.state().group).unwrap().vector.live else { panic!("warp") };
    assert_eq!(*bend, 40.0);
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Warp style").click();
    h.run();
    h.get_by_label("Arch").click();
    h.run();
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Cancel envelope").click();
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().state.pending.is_none());
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn envelope_ui_source_edit_invalidates_display_and_target_switch_cancels() {
    let mut h = harness();
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 100.0, 60.0, None);
    pointer(&mut h, 100.0, 60.0, Some(false));
    let a = {
        let host = h.state_mut();
        host.state.display_for_document(&host.session, 0).unwrap().unwrap().0
    };
    let id = h.state().source;
    h.state_mut()
        .session
        .execute(
            "shape.applyPaths",
            serde_json::json!({"edits":[{"layer":id.0,"path":Path::new(vec![Subpath::polygon(&[(20.0,30.0),(110.0,50.0),(140.0,130.0),(20.0,130.0)])])}]}),
        )
        .unwrap();
    let b = {
        let host = h.state_mut();
        host.state.display_for_document(&host.session, 0).unwrap().unwrap().0
    };
    assert!(!Arc::ptr_eq(&a, &b));
    assert_ne!(a.layer(id), b.layer(id));
    let p = h.state().state.handles(&h.state().session)[4];
    pointer(&mut h, p[0] as f32, p[1] as f32, Some(true));
    pointer(&mut h, p[0] as f32 + 5.0, p[1] as f32 + 5.0, None);
    h.state_mut().state.set_target(None);
    assert!(h.state().state.pending.is_none());
    assert!(!h.state().state.is_dragging());
}

#[test]
fn envelope_ui_real_resized_mesh_keeps_capture_when_dragging() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    for label in ["Rows", "Columns"] {
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).type_text("4");
        h.key_press(Key::Enter);
        h.run();
    }
    assert!(h.state().session.active().unwrap().doc == before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 80.0, 80.0, Some(true));
    pointer(&mut h, 90.0, 70.0, None);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert!(h.state().session.active().unwrap().doc == before);
    pointer(&mut h, 90.0, 70.0, Some(false));
    let Some(LiveGroup::Envelope(EnvelopeSpec::Mesh { rows, cols, points })) =
        &h.state().session.active().unwrap().doc.layer(h.state().group).unwrap().vector.live
    else {
        panic!("live mesh")
    };
    assert_eq!((*rows, *cols, points.len()), (4, 4, 25));
    assert!((points[12][0] - 7.0 / 12.0).abs() < 1e-6);
    assert!((points[12][1] - 0.4).abs() < 1e-6);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert!(h.state().session.active().unwrap().doc == before);
}
