#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    state: State,
    ids: Vec<LayerId>,
}
pub(super) fn fixture() -> Session {
    let mut doc = Document::new("Object UI 36", Size::new(180, 140), ColorMode::Rgb, SampleType::F32);
    let path = Path::new(vec![Subpath::polygon(
        &(0..100)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 100.;
                (70. + 40. * a.cos(), 70. + 40. * a.sin())
            })
            .collect::<Vec<_>>(),
    )]);
    let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    doc.layers.push(Layer::new("Circle", LayerContent::Shape(sh)));
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(s.active().unwrap().doc.layers[0].id).unwrap();
    s
}
fn host(ui: &mut Ui, h: &mut Host) {
    let ctx = ui.ctx().clone();
    h.state.shortcuts(&ctx, &mut h.session, &h.ids, &[]);
    egui::CentralPanel::default().show(ui, |ui| {
        h.state.menu(ui, &mut h.session, &h.ids, &[], str::to_owned);
        h.state.paste_menu(ui, &mut h.session, &h.ids, str::to_owned);
    });
    h.state.dialog(&ctx, &mut h.session, str::to_owned);
}
fn harness() -> Harness<'static, Host> {
    let s = fixture();
    let ids = s.active().unwrap().selected_layers();
    Harness::builder().with_size(egui::vec2(850., 700.)).with_max_steps(64).build_ui_state(host, Host { session: s, state: State::default(), ids })
}
fn pointer(h: &mut Harness<'_, Host>, pos: Pos2, down: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = down {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
#[test]
fn object36_kittest_menu_simplify_slider_preview_cancel_and_commit_one_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Path").click();
    h.run();
    h.get_by_label("Simplify…").click();
    h.run();
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Curve precision").rect();
    let a = Pos2::new(rect.left() + 30., rect.center().y);
    let b = Pos2::new(rect.left() + 10., rect.center().y);
    pointer(&mut h, a, Some(true));
    pointer(&mut h, b, None);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    pointer(&mut h, b, Some(false));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.get_by_label("Path").click();
    h.run();
    h.get_by_label("Simplify…").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
fn type_number(h: &mut Harness<'_, Host>, label: &str, text: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).type_text(text);
    h.key_press(Key::Enter);
    h.run();
}
#[test]
fn object36_kittest_grid_types_rows_columns_gutters_guides() {
    let mut h = harness();
    h.get_by_label("Path").click();
    h.run();
    h.get_by_label("Split Into Grid…").click();
    h.run();
    type_number(&mut h, "Rows", "3");
    type_number(&mut h, "Columns", "2");
    type_number(&mut h, "Row gutter", "10");
    type_number(&mut h, "Column gutter", "10");
    h.get_by_label("Add guides").click();
    h.run();
    assert!(h.state().state.preview.is_some());
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    let d = &h.state().session.active().unwrap().doc;
    assert_eq!(d.layers[0].children().unwrap().len(), 6);
    assert!(!d.guides.horizontal.is_empty());
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn object36_kittest_expand_options_checkboxes_cancel_then_appearance_menu() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Expand…").click();
    h.run();
    h.get_by_label("Stroke").click();
    h.run();
    assert!(h.state().state.preview.is_some());
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.get_by_label("Expand Appearance").click();
    h.run();
    assert!(h.state().session.active().unwrap().doc.layers[0].children().is_some());
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn object36_kittest_shortcuts_join_vector_copy_front_back() {
    let mut h = harness();
    let mut doc = (*h.state().session.active().unwrap().doc).clone();
    let ids = h.state().ids.clone();
    let LayerContent::Shape(sh) = &mut doc.layers[0].content else { panic!("shape") };
    sh.path = Path::new(vec![Subpath::polyline(&[(20., 20.), (40., 20.)])]);
    h.state_mut().session.active_mut().unwrap().doc = Arc::new(doc);
    h.key_press_modifiers(Modifiers::COMMAND, Key::J);
    h.run();
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert!(matches!(&h.state().session.active().unwrap().doc.layers[0].content,LayerContent::Shape(sh) if sh.path.subpaths[0].closed));
    h.key_press_modifiers(Modifiers::COMMAND, Key::C);
    h.run();
    assert!(h.state().state.clipboard.is_some());
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    assert_eq!(h.state().session.active().unwrap().doc.layers[0].id, ids[0]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 3);
    assert_eq!(h.state().session.active().unwrap().doc.layers[1].id, ids[0]);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 3);
}
#[test]
fn object36_kittest_escape_stale_document_and_preview_off_do_not_publish() {
    let mut h = harness();
    let ids = h.state().ids.clone();
    {
        let host = h.state_mut();
        host.state.open_simplify(&host.session, &ids);
    }
    h.run();
    h.get_by_label("Preview").click();
    h.run();
    assert!(h.state().state.preview.is_none());
    h.key_press(Key::Escape);
    h.run();
    assert!(!h.state().state.is_open());
    {
        let host = h.state_mut();
        host.state.open_simplify(&host.session, &ids);
    }
    h.run();
    h.state_mut().session.active_mut().unwrap().revision += 1;
    h.run_steps(2);
    assert!(!h.state().state.is_open());
    assert!(h.state().session.journal.is_empty());
}
