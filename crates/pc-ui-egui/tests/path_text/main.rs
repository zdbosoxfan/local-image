//! Real pointer/keyboard events in an isolated host; app GPU gates are separate.
macro_rules! tl {
    ($s:expr) => {
        li_i18n::t($s)
    };
}
#[path = "../../src/vector_packages/path_text.rs"]
#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod path_text_ui;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, LayerId, SampleType, ShapeLayer, Size, text::CharStyle};
use photocraft_engine::Session;
use photocraft_geom::Point;
struct Host {
    session: Session,
    state: path_text_ui::State,
    rect: egui::Rect,
    error: Option<String>,
    enabled: bool,
}
fn host() -> Host {
    let mut s = Session::new();
    let mut d = Document::new("Type", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    d.insert_above(
        None,
        Layer::new("Ellipse", LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::ellipse(28., 28., 200., 200.), ..Default::default() })),
    );
    s.add_document(d, None);
    Host { session: s, state: Default::default(), rect: egui::Rect::NOTHING, error: None, enabled: true }
}
fn draw(ui: &mut egui::Ui, h: &mut Host) {
    let result = h.state.synchronize(&h.session, h.enabled).and_then(|_| path_text_ui::context(ui, &mut h.state, &mut h.session));
    if let Err(e) = result {
        h.error = Some(e.to_string());
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(300., 300.), egui::Sense::click_and_drag());
    h.rect = rect;
    let to_doc = |p: Pos2| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y));
    if let Err(e) = path_text_ui::input(
        &mut h.state,
        &mut h.session,
        ui.ctx(),
        rect,
        to_doc,
        h.enabled,
        4.,
        &CharStyle { font_family: "Inter".into(), size_pt: 20., ..Default::default() },
    ) {
        h.error = Some(e.to_string());
    }
    path_text_ui::paint_overlay(&h.state, ui.painter(), |p| rect.min + egui::vec2(p.x as f32, p.y as f32));
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(700., 650.)).build_ui_state(draw, host())
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool) {
    let p = h.state().rect.min + egui::vec2(x, y);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn click(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    button(h, x, y, true);
    button(h, x, y, false);
}
fn key(h: &mut Harness<'_, Host>, key: Key) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.step();
}
fn active(h: &Harness<'_, Host>) -> LayerId {
    h.state().session.active().unwrap().active_layer.unwrap()
}
fn create(h: &mut Harness<'_, Host>) {
    click(h, 228., 128.);
    h.event(Event::Text("ABC".into()));
    h.step();
    key(h, Key::Enter);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
#[test]
fn path_text_ui_click_actual_ellipse_type_abc_commit_and_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    click(&mut h, 228., 128.);
    h.event(Event::Text("ABC".into()));
    h.step();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    let preview = h.state().state.preview.clone().unwrap();
    assert!(photocraft_compose::flatten(&preview).px.iter().any(|p| p[3] > 0.9));
    key(&mut h, Key::Enter);
    let st = h.state().session.active().unwrap();
    let l = st.doc.layer(active(&h)).unwrap();
    let LayerContent::Text(t) = &l.content else { panic!("type") };
    assert_eq!(t.text, "ABC");
    assert!(l.vector.on_path.is_some());
    assert_eq!(photocraft_compose::flatten(&st.doc), photocraft_compose::flatten(&preview));
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn path_text_ui_drag_marker_flip_baseline_and_undo() {
    let mut h = harness();
    create(&mut h);
    let id = active(&h);
    let before = h.state().session.active().unwrap().doc.clone();
    // Click captured the rightmost point at quarter length. Drag to half length.
    button(&mut h, 228., 128., true);
    let pos = h.state().rect.min + egui::vec2(128., 228.);
    h.event(Event::PointerMoved(pos));
    h.step();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    button(&mut h, 128., 228., false);
    let path = h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap();
    assert!((path.start - 0.5).abs() < 0.001);
    h.get_by_label("Flip path text").click();
    h.run_steps(2);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert!(h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap().flip);
    h.get_by_label("Ascender").click();
    h.run_steps(2);
    assert_eq!(
        h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap().align,
        photocraft_doc::vector_design::text::PathTextAlign::Ascender
    );
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn path_text_ui_escape_focus_tool_change_and_staleness_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    click(&mut h, 228., 128.);
    h.event(Event::Text("ABC".into()));
    h.step();
    key(&mut h, Key::Escape);
    assert!(h.state().state.preview.is_none());
    let LayerContent::Text(text) = &h.state().session.active().unwrap().doc.layer(active(&h)).unwrap().content else { panic!("finished text") };
    assert_eq!(text.text, "ABC");
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    click(&mut h, 228., 128.);
    h.event(Event::WindowFocused(false));
    h.step();
    assert!(h.state().state.draft.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    click(&mut h, 228., 128.);
    h.state_mut().enabled = false;
    h.step();
    assert!(h.state().state.draft.is_none());
    h.state_mut().enabled = true;
    click(&mut h, 228., 128.);
    h.state_mut()
        .session
        .edit("Other edit", |doc, _| {
            doc.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    h.step();
    assert!(h.state().state.draft.is_none());
}
#[test]
fn path_text_ui_release_outside_canvas_and_unicode_backspace() {
    let mut h = harness();
    click(&mut h, 228., 128.);
    h.event(Event::Ime(egui::ImeEvent::Commit("Aé".into())));
    h.step();
    key(&mut h, Key::Backspace);
    key(&mut h, Key::Enter);
    let id = active(&h);
    let LayerContent::Text(t) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(t.text, "A");
    button(&mut h, 228., 128., true);
    button(&mut h, 400., 128., false);
    assert!(h.state().state.dragging.is_none());
    assert!(h.state().error.is_none());
}
#[test]
fn path_text_ui_flipped_start_end_and_baseline_marker_drag() {
    let mut h = harness();
    create(&mut h);
    let id = active(&h);
    h.get_by_label("Flip path text").click();
    h.run_steps(2);
    // Flip must not exchange the bracket handles' identities.
    button(&mut h, 228., 128., true);
    button(&mut h, 128., 228., false);
    let path = h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap();
    assert!((path.start - 0.5).abs() < 0.001);
    assert_eq!(path.end, 1.);
    button(&mut h, 128., 28., true);
    button(&mut h, 28., 128., false);
    let path = h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap();
    assert!((path.end - 0.75).abs() < 0.001);
    h.run_steps(2);
    let marker = h.state().state.baseline_marker().unwrap();
    let layout = h.state().state.overlay.as_ref().unwrap();
    let c = layout.clusters.iter().find(|c| c.visible).unwrap();
    let ascent = f64::from(layout.shaped.lines[0].ascent) + 12.;
    let target = Point::new(c.anchor.x - c.angle.sin() * ascent, c.anchor.y + c.angle.cos() * ascent);
    button(&mut h, marker.x as f32, marker.y as f32, true);
    button(&mut h, target.x as f32, target.y as f32, false);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(
        h.state().session.active().unwrap().doc.layer(id).unwrap().vector.on_path.as_ref().unwrap().align,
        photocraft_doc::vector_design::text::PathTextAlign::Ascender
    );
}
mod goldens;
