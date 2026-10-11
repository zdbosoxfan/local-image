#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, Modifiers, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_geom::Point;
struct Host {
    session: Session,
    state: State,
    vector: bool,
    group: LayerId,
    outside: LayerId,
    rectangle: bool,
    origin: Pos2,
    drag: Option<(Capture, Point)>,
}
fn shape(doc: &Document, rect: [f64; 4]) -> Layer {
    let mut s =
        ShapeLayer { path: photocraft_vector::shapes::rect(rect[0], rect[1], rect[2], rect[3]), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(doc, &mut s);
    Layer::new("Rectangle", LayerContent::Shape(s))
}
fn harness() -> Harness<'static, Host> {
    let mut doc = Document::new("UI Isolation", Size::new(200, 130), ColorMode::Rgb, SampleType::U8);
    let group = Layer::group("Group", vec![shape(&doc, [20., 20., 60., 60.])]);
    let group_id = group.id;
    let outside = shape(&doc, [130., 20., 30., 30.]);
    let outside_id = outside.id;
    doc.layers = vec![group, outside];
    let mut session = Session::new();
    session.add_document(doc, None);
    Harness::builder().with_size(egui::vec2(640., 480.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            if h.state.synchronize(&h.session, h.vector) {
                h.drag = None;
            }
            if escape(ui.ctx(), &mut h.state) {
                h.drag = None;
            }
            if ui.button("Pixel persona").clicked() {
                h.vector = false;
                h.state.synchronize(&h.session, false);
                h.drag = None;
            }
            ui.checkbox(&mut h.rectangle, "Rectangle Tool");
            bar(ui, &mut h.session, &mut h.state, str::to_owned).unwrap();
            let thumbnail_response = ui.button("Group thumbnail");
            thumbnail(ui, &thumbnail_response, &mut h.session, &mut h.state, h.group).unwrap();
            let (rect, response) = ui.allocate_exact_size(egui::vec2(200., 130.), egui::Sense::click_and_drag());
            h.origin = rect.min;
            let point = ui.input(|i| i.pointer.interact_pos()).map(|p| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)));
            let hit = point.and_then(|p| {
                if (20.0..80.).contains(&p.x) && (20.0..80.).contains(&p.y) {
                    Some(h.group)
                } else if (130.0..160.).contains(&p.x) && (20.0..50.).contains(&p.y) {
                    Some(h.outside)
                } else {
                    None
                }
            });
            if !h.rectangle {
                let consumed = canvas(ui, &response, &mut h.session, &mut h.state, hit, h.vector).unwrap();
                if !consumed
                    && response.clicked()
                    && let Some(id) = hit
                {
                    let _ = select(&mut h.session, &h.state, &[id]);
                }
            } else if h.vector {
                if response.drag_started()
                    && let Some(p) = point
                {
                    h.drag = Some((Capture::new(&h.session, &h.state.scope).unwrap(), p));
                }
                if response.drag_stopped()
                    && let Some((capture, start)) = h.drag.take()
                    && let Some(end) = point
                {
                    let doc = h.session.active().unwrap().doc.clone();
                    let layer = shape(&doc, [start.x.min(end.x), start.y.min(end.y), (end.x - start.x).abs(), (end.y - start.y).abs()]);
                    engine::insert_prepared(&mut h.session, &h.state.scope, capture, None, layer).unwrap();
                }
            }
        },
        Host { session, state: State::default(), vector: true, group: group_id, outside: outside_id, rectangle: false, origin: Pos2::ZERO, drag: None },
    )
}
fn pointer(h: &mut Harness<'_, Host>, p: [f32; 2], press: Option<bool>, modifiers: Modifiers) {
    let pos = h.state().origin + egui::vec2(p[0], p[1]);
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
    }
    h.step();
}
fn click(h: &mut Harness<'_, Host>, p: [f32; 2]) {
    pointer(h, p, Some(true), Modifiers::NONE);
    pointer(h, p, Some(false), Modifiers::NONE);
}
fn double_click(h: &mut Harness<'_, Host>, p: [f32; 2]) {
    // Separate independent gestures from egui's 0.6s triple-click history.
    h.run_steps(40);
    click(h, p);
    click(h, p);
    h.run();
}
#[test]
fn view37_kittest_double_click_draw_rectangle_scope_and_escape() {
    let mut h = harness();
    let group = h.state().group;
    let outside = h.state().outside;
    let before = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    double_click(&mut h, [40., 40.]);
    assert_eq!(h.state().state.scope.current(), Some(Target::Group { layer: group }));
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    let state = h.state().state.clone();
    assert!(select(&mut h.state_mut().session, &state, &[outside]).is_err());
    h.get_by_label("Rectangle Tool").click();
    h.run();
    pointer(&mut h, [90., 70.], Some(true), Modifiers::NONE);
    pointer(&mut h, [100., 80.], None, Modifiers::NONE);
    pointer(&mut h, [120., 100.], None, Modifiers::NONE);
    pointer(&mut h, [120., 100.], Some(false), Modifiers::NONE);
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc.layer(group).unwrap().children().unwrap().len(), 2);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    assert_eq!(h.state().session.active().unwrap().revision, rev + 1);
    h.key_press(Key::Escape);
    h.run();
    assert!(h.state().state.scope.current().is_none());
    assert_eq!(h.state().session.active().unwrap().revision, rev + 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn view37_kittest_alt_thumbnail_and_persona_switch_exit_without_edits() {
    let mut h = harness();
    let doc = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    h.event(Event::ModifiersChanged(Modifiers::ALT));
    h.get_by_label("Group thumbnail").click();
    h.run();
    assert!(h.state().state.scope.current().is_some());
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.get_by_label("Pixel persona").click();
    h.run();
    assert!(h.state().state.scope.current().is_none());
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn view37_kittest_double_click_outside_and_breadcrumb_exit() {
    let mut h = harness();
    double_click(&mut h, [40., 40.]);
    assert!(h.state().state.scope.current().is_some());
    double_click(&mut h, [185., 110.]);
    assert!(h.state().state.scope.current().is_none());
    double_click(&mut h, [40., 40.]);
    h.get_by_label("Exit Isolation").click();
    h.run();
    assert!(h.state().state.scope.current().is_none());
}
#[test]
fn view37_isolation_selection_rejects_entire_set_atomically() {
    let mut h = harness();
    double_click(&mut h, [40., 40.]);
    let st = h.state().session.active().unwrap();
    let before = st.selected_layers();
    let rev = st.revision;
    let child = st.doc.layer(h.state().group).unwrap().children().unwrap()[0].id;
    let outside = h.state().outside;
    let state = h.state().state.clone();
    assert!(select(&mut h.state_mut().session, &state, &[child, outside]).is_err());
    assert_eq!(h.state().session.active().unwrap().selected_layers(), before);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    let mut captured = State::default();
    captured.capture = Some(PointerCapture {
        document: Capture::new(&h.state().session, &captured.scope).unwrap(),
        target: Some(outside),
        response: egui::Id::new("captured target"),
    });
    let group = h.state().group;
    assert!(captured.enter(&mut h.state_mut().session, group).is_err());
    assert!(captured.scope.current().is_none());
    assert_eq!(h.state().session.active().unwrap().selected_layers(), before);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
}
