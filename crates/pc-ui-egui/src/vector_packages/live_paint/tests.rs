#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Layer, LayerContent, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    tools: State,
    origin: Pos2,
    keymap: Keymap,
}
fn fixture(keymap: Keymap) -> Host {
    let mut doc = Document::new("Live paint UI", Size::new(160, 128), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for (i, path) in [
        photocraft_vector::shapes::ellipse(16., 24., 80., 80.),
        photocraft_vector::shapes::ellipse(56., 24., 80., 80.),
        photocraft_doc::Path::new(vec![Subpath::polyline(&[(0., 64.), (150., 64.)])]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut shape = ShapeLayer { path, fill: if i < 2 { Some(Fill::Solid(Color::BLACK)) } else { None }, ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        let layer = Layer::new("Source", LayerContent::Shape(shape));
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut session = Session::new();
    session.add_document(doc, None);
    Host { session, tools: State { targets: ids, paint: Some(Fill::Solid(Color::rgb(1., 0., 0.))), ..Default::default() }, origin: Pos2::ZERO, keymap }
}
fn ui(ui: &mut egui::Ui, host: &mut Host) {
    host.tools.frame(&host.session, ui.ctx(), true);
    host.tools.shortcuts(&mut host.session, ui.ctx(), host.keymap, true);
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| host.tools.context_bar(ui, &mut host.session));
        let (response, painter) = ui.allocate_painter(egui::vec2(180., 180.), egui::Sense::click_and_drag());
        host.origin = response.rect.min;
        let xf = crate::canvas::ViewXform { rect: response.rect, zoom: 1., center: [90., 90.], flip: false };
        host.tools.canvas_input(&mut host.session, &response, &xf);
        host.tools.draw_overlay(&painter, &|p| host.origin + egui::vec2(p.x as f32, p.y as f32), 1.);
    });
}
fn harness(keymap: Keymap) -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1250., 340.)).build_ui_state(ui, fixture(keymap))
}
fn move_to(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    let p = h.state().origin + egui::vec2(x, y);
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool, modifiers: Modifiers) {
    let pos = h.state().origin + egui::vec2(x, y);
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
    h.step();
}
#[test]
fn live_paint42_ui_r_drag_flood_preview_release_and_undo() {
    let mut h = harness(Keymap::Affinity);
    h.key_press(Key::R);
    h.run();
    assert_eq!(h.state().tools.tool, Some(Tool::Flood));
    let before = h.state().session.active().unwrap().doc.clone();
    move_to(&mut h, 32., 40.);
    button(&mut h, 32., 40., true, Modifiers::NONE);
    move_to(&mut h, 76., 40.);
    assert!(h.state().tools.is_dragging());
    assert!(h.state().tools.display_document(&h.state().session).is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    button(&mut h, 76., 40., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none(), "{:?}", h.state().tools.error);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(!h.state_mut().session.undo());
}
#[test]
fn live_paint42_ui_shift_l_select_faces_edges_and_alt_sample_without_edit() {
    let mut h = harness(Keymap::Illustrator);
    h.key_press_modifiers(Modifiers::SHIFT, Key::L);
    h.run();
    assert_eq!(h.state().tools.tool, Some(Tool::Selection));
    let before = h.state().session.active().unwrap().doc.clone();
    move_to(&mut h, 32., 40.);
    button(&mut h, 32., 40., true, Modifiers::NONE);
    button(&mut h, 32., 40., false, Modifiers::NONE);
    assert_eq!(h.state().tools.selected_faces.len(), 1);
    move_to(&mut h, 70., 64.);
    button(&mut h, 70., 64., true, Modifiers::SHIFT);
    button(&mut h, 70., 64., false, Modifiers::SHIFT);
    assert_eq!(h.state().tools.selected_edges.len(), 1);
    assert_eq!(h.state().tools.selected_faces.len(), 1);
    button(&mut h, 32., 40., true, Modifiers::ALT);
    button(&mut h, 32., 40., false, Modifiers::ALT);
    assert_eq!(h.state().tools.paint, Some(Fill::Solid(Color::BLACK)));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    assert!(!h.state_mut().session.undo());
}
#[test]
fn live_paint42_ui_cancellation_focus_stale_escape_and_outside_release() {
    for cancel in 0..4 {
        let mut h = harness(Keymap::Affinity);
        h.key_press(Key::R);
        h.run();
        let before = h.state().session.active().unwrap().doc.clone();
        move_to(&mut h, 32., 40.);
        button(&mut h, 32., 40., true, Modifiers::NONE);
        move_to(&mut h, 76., 40.);
        match cancel {
            0 => {
                h.key_press(Key::Escape);
                h.run();
            }
            1 => {
                h.event(Event::WindowFocused(false));
                h.step();
                h.event(Event::WindowFocused(true));
                h.step();
            }
            2 => {
                h.state_mut().session.active_mut().unwrap().revision += 1;
                h.step();
            }
            _ => h.state_mut().tools.set_tool(None),
        }
        button(&mut h, 76., 40., false, Modifiers::NONE);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        assert!(!h.state().tools.is_dragging());
    }
    let mut h = harness(Keymap::Affinity);
    h.key_press(Key::R);
    h.run();
    move_to(&mut h, 32., 40.);
    button(&mut h, 32., 40., true, Modifiers::NONE);
    button(&mut h, 210., 40., false, Modifiers::NONE);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state().tools.error.is_none());
}
#[test]
fn live_paint42_ui_k_bucket_data_dependency_and_numeric_width_type() {
    let mut h = harness(Keymap::Illustrator);
    h.key_press(Key::K);
    h.run();
    assert_eq!(h.state().tools.tool, Some(Tool::Bucket));
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Edge width").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Edge width").type_text("2.5");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().tools.width, 2.5);
    let before = h.state().session.active().unwrap().doc.clone();
    h.state_mut().tools.set_targets(vec![photocraft_doc::LayerId(u64::MAX)]);
    move_to(&mut h, 32., 40.);
    button(&mut h, 32., 40., true, Modifiers::NONE);
    button(&mut h, 32., 40., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_some());
    assert!(!h.state().tools.is_dragging());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
}
#[test]
#[ignore = "00 native LivePaint data dependency; integrated bucket click/drag/Shift edge acceptance"]
fn live_paint42_ui_k_click_drag_shift_edge_and_expand_pixels() {
    let mut h = harness(Keymap::Illustrator);
    h.key_press(Key::K);
    h.run();
    move_to(&mut h, 32., 40.);
    button(&mut h, 32., 40., true, Modifiers::NONE);
    button(&mut h, 32., 40., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none());
    assert_eq!(h.state().session.journal.len(), 1);
    move_to(&mut h, 76., 40.);
    button(&mut h, 76., 40., true, Modifiers::NONE);
    move_to(&mut h, 120., 40.);
    button(&mut h, 120., 40., false, Modifiers::NONE);
    assert!(h.state().tools.error.is_none());
    assert_eq!(h.state().session.journal.len(), 2);
    move_to(&mut h, 70., 64.);
    button(&mut h, 70., 64., true, Modifiers::SHIFT);
    button(&mut h, 70., 64., false, Modifiers::SHIFT);
    assert!(h.state().tools.error.is_none());
    assert_eq!(h.state().session.journal.len(), 3);
    let group = h.state().tools.targets[0];
    let g = Gesture::begin(&h.state().session, &[group]).unwrap();
    assert!(!g.spec.edges.is_empty());
    let projected = engine::evaluator::project_document(&h.state().session.active().unwrap().doc).unwrap();
    let pixels = photocraft_compose::flatten(&projected).px;
    h.get_by_label("Expand Live Paint").click();
    h.run();
    assert!(h.state().tools.error.is_none());
    assert_eq!(h.state().session.journal.len(), 4);
    assert!(h.state().session.active().unwrap().doc.walk().into_iter().all(|(_, _, layer)| layer.vector.live.is_none()));
    assert_eq!(photocraft_compose::flatten(&h.state().session.active().unwrap().doc).px, pixels);
}
