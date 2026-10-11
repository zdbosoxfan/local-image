#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    state: State,
    canvas: Rect,
}
fn fixture() -> Host {
    let mut doc = Document::new("Transform input", Size::new(200, 200), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20., 20.), (100., 20.), (100., 100.), (20., 100.)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    let l = Layer::new("Square", LayerContent::Shape(sh));
    let id = l.id;
    doc.layers.push(l);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.active_mut().unwrap().active_layer = Some(id);
    session.active_mut().unwrap().selected_layers = vec![id];
    let mut state = State::default();
    state.preferences.snapping.tolerance_px = 4.;
    Host { session, state, canvas: Rect::NOTHING }
}
fn ui(ui: &mut Ui, host: &mut Host) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.scope(|ui| {
            ui.set_min_height(40.);
            host.state.context_bar(ui);
        });
        let (response, painter) = ui.allocate_painter(egui::vec2(240., 240.), egui::Sense::click_and_drag());
        host.canvas = response.rect;
        let origin = host.canvas.min;
        let ids = host.session.active().unwrap().selected_layers();
        host.state.input(
            &mut host.session,
            ui.ctx(),
            host.canvas,
            |p| Point::new(f64::from(p.x - origin.x), f64::from(p.y - origin.y)),
            &ids,
            true,
            snap::View::default(),
        );
        host.state.paint(&host.session, &ids, &painter, |p| origin + egui::vec2(p.x as f32, p.y as f32));
        host.state.dialogs(ui.ctx(), &mut host.session);
    });
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(700., 600.)).build_ui_state(ui, fixture())
}
fn pointer(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: Option<bool>, mods: Modifiers) {
    let p = h.state().canvas.min + egui::vec2(x, y);
    h.event(match pressed {
        Some(pressed) => Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods },
        None => Event::PointerMoved(p),
    });
    h.step();
}
fn click(h: &mut Harness<'_, Host>, x: f32, y: f32, mods: Modifiers) {
    pointer(h, x, y, None, mods);
    pointer(h, x, y, Some(true), mods);
    pointer(h, x, y, Some(false), mods);
    h.run();
}
fn drag(h: &mut Harness<'_, Host>, start: [f32; 2], end: [f32; 2], mods: Modifiers) {
    pointer(h, start[0], start[1], None, mods);
    pointer(h, start[0], start[1], Some(true), mods);
    pointer(h, end[0], end[1], None, mods);
    pointer(h, end[0], end[1], Some(false), mods);
    h.run();
}
fn type_field(h: &mut Harness<'_, Host>, label: &str, value: &str) {
    h.get_by_label(label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_label(label).type_text(value);
    h.run();
}
fn path(host: &Host) -> Path {
    let st = host.session.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!() };
    sh.path.clone()
}
fn no_error(h: &Harness<'_, Host>) {
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
}
#[test]
fn transform34_ui_r_click_snapped_origin_drag_preview_release_single_journal() {
    let mut h = harness();
    h.key_press(Key::R);
    h.run();
    let before = h.state().session.active().unwrap().doc.clone();
    click(&mut h, 22., 19., Modifiers::NONE);
    assert_eq!(h.state().state.origin, Some(Point::new(20., 20.)));
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 100., 20., None, Modifiers::NONE);
    pointer(&mut h, 100., 20., Some(true), Modifiers::NONE);
    pointer(&mut h, 20., 100., None, Modifiers::NONE);
    no_error(&h);
    let preview = h.state().state.preview.clone().unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 20., 100., Some(false), Modifiers::NONE);
    h.run();
    no_error(&h);
    assert_eq!(*h.state().session.active().unwrap().doc, preview);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!((path(h.state()).subpaths[0].knots[1].anchor.x - 20.).abs() < 1e-9);
    assert!((path(h.state()).subpaths[0].knots[1].anchor.y - 100.).abs() < 1e-9);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn transform34_ui_alt_click_type_45_copy_ctrl_d_three_times() {
    let mut h = harness();
    click(&mut h, 20., 20., Modifiers::ALT);
    assert!(h.state().state.dialog_open());
    type_field(&mut h, "Angle", "45");
    no_error(&h);
    assert!(h.state().state.preview.is_some());
    assert!(h.state().session.journal.is_empty());
    h.get_by_label("Copy").click();
    h.run();
    no_error(&h);
    assert_eq!(h.state().session.active().unwrap().doc.layer_count(), 2);
    for _ in 0..3 {
        h.key_press_modifiers(Modifiers::CTRL, Key::D);
        h.run();
        no_error(&h);
    }
    assert_eq!(h.state().session.active().unwrap().doc.layer_count(), 5);
    assert_eq!(h.state().session.journal.len(), 4);
    let p = path(h.state()).subpaths[0].knots[1].anchor;
    assert!((p.x + 60.).abs() < 1e-7 && (p.y - 20.).abs() < 1e-7);
}
#[test]
fn transform34_ui_e_free_distort_corner_escape_and_release() {
    let mut h = harness();
    h.key_press(Key::E);
    h.run();
    h.get_by_label("Free Distort").click();
    h.run();
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, 20., 20., None, Modifiers::NONE);
    pointer(&mut h, 20., 20., Some(true), Modifiers::NONE);
    pointer(&mut h, 35., 25., None, Modifiers::NONE);
    no_error(&h);
    assert!(h.state().state.preview.is_some());
    h.key_press(Key::Escape);
    h.run();
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, 35., 25., Some(false), Modifiers::NONE);
    h.run();
    drag(&mut h, [20., 20.], [35., 25.], Modifiers::NONE);
    no_error(&h);
    assert_eq!(path(h.state()).subpaths[0].knots[0].anchor, Point::new(35., 25.));
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn transform34_ui_scale_shift_alt_release_modifier_copy() {
    let mut h = harness();
    h.key_press(Key::S);
    h.run();
    click(&mut h, 20., 20., Modifiers::NONE);
    pointer(&mut h, 100., 100., None, Modifiers::NONE);
    pointer(&mut h, 100., 100., Some(true), Modifiers::NONE);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.step();
    pointer(&mut h, 140., 120., None, Modifiers::SHIFT);
    pointer(&mut h, 140., 120., Some(false), Modifiers { alt: true, shift: true, ..Modifiers::NONE });
    h.run();
    no_error(&h);
    assert_eq!(h.state().session.active().unwrap().doc.layer_count(), 2);
    let b = path(h.state()).control_bounds().unwrap();
    assert!((b.2 - b.0 - (b.3 - b.1)).abs() < 1e-9);
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn transform34_ui_enter_move_numeric_preview_toggle_and_copy() {
    let mut h = harness();
    h.state_mut().state.kind = Kind::Move;
    h.run();
    h.key_press(Key::Enter);
    h.run();
    type_field(&mut h, "Horizontal", "10");
    type_field(&mut h, "Vertical", "5");
    h.get_by_label("Preview").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    assert!(h.state().session.journal.is_empty());
    h.get_by_label("Copy").click();
    h.run();
    no_error(&h);
    assert_eq!(h.state().session.active().unwrap().doc.layer_count(), 2);
    assert_eq!(path(h.state()).subpaths[0].knots[0].anchor, Point::new(30., 25.));
}
#[test]
fn transform34_ui_transform_each_shortcut_seed_and_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    h.key_press_modifiers(Modifiers { ctrl: true, alt: true, shift: true, ..Modifiers::NONE }, Key::D);
    h.run();
    assert_eq!(h.state().state.kind, Kind::Each);
    assert!(h.state().state.dialog_open());
    type_field(&mut h, "Horizontal move", "15");
    h.get_by_label("Random").click();
    h.run();
    type_field(&mut h, "Seed", "123");
    no_error(&h);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn transform34_ui_reshape_node_selection_real_drag_falloff() {
    let mut h = harness();
    h.state_mut().state.kind = Kind::Reshape;
    h.state_mut().state.preferences.reshape_radius = 160.;
    h.run();
    drag(&mut h, [20., 20.], [20., 36.], Modifiers::NONE);
    no_error(&h);
    let p = path(h.state());
    assert_eq!(p.subpaths[0].knots[0].anchor.y, 36.);
    assert_eq!(p.subpaths[0].knots[1].anchor.y, 29.);
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn transform34_ui_stale_focus_tool_lifecycle_and_old_preferences() {
    let mut h = harness();
    pointer(&mut h, 100., 20., None, Modifiers::NONE);
    pointer(&mut h, 100., 20., Some(true), Modifiers::NONE);
    pointer(&mut h, 20., 100., None, Modifiers::NONE);
    assert!(h.state().state.preview.is_some());
    h.state_mut().session.active_mut().unwrap().revision += 1;
    h.run();
    assert!(h.state().state.preview.is_none());
    assert!(!h.state().state.active());
    assert!(h.state().session.journal.is_empty());
    let p: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(p.options, Options::default());
    assert_eq!(p.free_mode, FreeMode::Free);
}
#[test]
fn transform34_keyboard_focus_regression_editor_shortcut_does_not_switch_tool() {
    let mut h = harness();
    click(&mut h, 20., 20., Modifiers::ALT);
    assert!(h.state().state.dialog_open());
    h.get_by_label("Angle").click();
    h.run();
    h.key_press(Key::S);
    h.run();
    assert_eq!(h.state().state.kind, Kind::Rotate);
    assert!(h.state().state.dialog_open());
    assert!(h.state().session.journal.is_empty());
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().state.active());
    assert!(h.state().state.preview.is_none());
}
