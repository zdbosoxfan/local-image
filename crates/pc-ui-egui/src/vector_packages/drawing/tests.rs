use super::*;
use egui::{Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::vector_design::drawing::ShapeParameters as S;
use photocraft_doc::{ColorMode, Document, SampleType, Size};
struct Host {
    session: Session,
    state: State,
    selected: Option<LayerId>,
    origin: Pos2,
    guide: Option<f64>,
    controls: bool,
}
fn host() -> Host {
    let mut session = Session::new();
    session.add_document(Document::new("Shape UI", Size::new(240, 240), ColorMode::Rgb, SampleType::U8), None);
    Host { session, state: State::default(), selected: None, origin: Pos2::ZERO, guide: None, controls: false }
}
fn harness(host: Host) -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(600., 700.)).build_ui_state(
        |ui, host| {
            if host.controls {
                controls(ui, &mut host.session, &mut host.state, &host.selected.into_iter().collect::<Vec<_>>(), str::to_owned);
            }
            let rect = ui.allocate_space(egui::vec2(260., 260.)).1;
            host.origin = rect.min;
            let guide = host.guide;
            canvas(ui, &mut host.session, &mut host.state, rect, View { origin: rect.min, scale: 1. }, host.selected, true, move |p, start, shift| {
                if let Some(x) = guide
                    && (p.x - x).abs() <= 4.
                {
                    if shift {
                        let dx = x - start.x;
                        return Point::new(x, start.y + dx.abs() * if p.y < start.y { -1. } else { 1. });
                    }
                    return Point::new(x, p.y);
                }
                p
            });
        },
        host,
    )
}
fn pointer(h: &mut Harness<'_, Host>, p: [f32; 2], pressed: Option<bool>, mods: Modifiers) {
    let pos = h.state().origin + egui::vec2(p[0], p[1]);
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
}
fn shape(h: &Harness<'_, Host>) -> DesignShape {
    h.state().session.active().unwrap().doc.layers.last().unwrap().vector.live_shape.clone().unwrap()
}
fn add_star(host: &mut Host) {
    let c = Capture::new(&host.session).unwrap();
    let d = DesignShape { rect: [40., 40., 120., 120.], angle: 0., parameters: S::Star { points: 5, inner: 0.5, corner: 0. } };
    let id = host.session.execute("shape.design.create", json!({"document":c.document.0,"revision":c.revision,"design":d})).unwrap()["layer"].as_u64().unwrap();
    host.selected = Some(LayerId(id));
}
#[test]
fn live_shapes_kittest_rect_drag_preview_shift_alt_snap_and_undo() {
    let mut host = host();
    host.guide = Some(150.);
    let mut h = harness(host);
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, [70., 70.], Some(true), Modifiers::NONE);
    pointer(&mut h, [120., 110.], None, Modifiers::NONE);
    assert_eq!(h.state().state.preview.as_ref().unwrap().rect, [70., 70., 50., 40.]);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    let mods = Modifiers { shift: true, alt: true, ..Default::default() };
    pointer(&mut h, [148., 100.], None, mods);
    assert_eq!(h.state().state.preview.as_ref().unwrap().rect, [-10., -10., 160., 160.]);
    pointer(&mut h, [148., 100.], Some(false), mods);
    assert_eq!(shape(&h).rect, [-10., -10., 160., 160.]);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn live_shapes_kittest_star_inner_radius_drag_and_convert_click() {
    let mut host = host();
    add_star(&mut host);
    let mut h = harness(host);
    let before = h.state().session.active().unwrap().doc.clone();
    let d = shape(&h);
    let handle = geometry::handles(&d).unwrap().into_iter().find(|h| h.kind == HandleKind::Inner).unwrap();
    let c = Point::new(100., 100.);
    let destination = Point::new(c.x + (handle.position.x - c.x) * 1.5, c.y + (handle.position.y - c.y) * 1.5);
    pointer(&mut h, [handle.position.x as f32, handle.position.y as f32], Some(true), Modifiers::NONE);
    pointer(&mut h, [destination.x as f32, destination.y as f32], None, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(matches!(h.state().state.preview.as_ref().unwrap().parameters,S::Star{inner,..}if (inner-0.75).abs()<1e-6));
    pointer(&mut h, [destination.x as f32, destination.y as f32], Some(false), Modifiers::NONE);
    let edited = h.state().session.active().unwrap().doc.clone();
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state_mut().session.redo());
    h.state_mut().controls = true;
    h.step();
    h.get_by_label("Convert to Curves").click();
    h.step();
    assert!(h.state().session.active().unwrap().doc.layers.last().unwrap().vector.live_shape.is_none());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, edited);
}
#[test]
fn live_shapes_kittest_cancel_focus_loss_staleness_leave_document_unchanged() {
    let mut h = harness(host());
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, [20., 20.], Some(true), Modifiers::NONE);
    pointer(&mut h, [120., 100.], None, Modifiers::NONE);
    h.key_press(egui::Key::Escape);
    h.step();
    assert!(!h.state().state.is_active());
    pointer(&mut h, [120., 100.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, [20., 20.], Some(true), Modifiers::NONE);
    pointer(&mut h, [120., 100.], None, Modifiers::NONE);
    h.input_mut().focused = false;
    h.step();
    assert!(!h.state().state.is_active());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.input_mut().focused = true;
    pointer(&mut h, [120., 100.], Some(false), Modifiers::NONE);
    pointer(&mut h, [20., 20.], Some(true), Modifiers::NONE);
    pointer(&mut h, [120., 100.], None, Modifiers::NONE);
    h.state_mut()
        .session
        .edit("Concurrent edit", |doc, _| {
            doc.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    let after = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, [120., 100.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, after);
    assert!(h.state().state.error.as_ref().unwrap().contains("stale"));
}
#[test]
fn live_shapes_units_and_rotated_handle_exactness() {
    assert!((unit_scale(DesignUnits::Millimeters, 300.).unwrap() * 25.4 - 300.).abs() < 1e-9);
    assert_eq!(unit_scale(DesignUnits::Points, 144.).unwrap(), 2.);
    assert_eq!(unit_scale(DesignUnits::Inches, 144.).unwrap(), 144.);
    assert!(unit_scale(DesignUnits::Pixels, f64::NAN).is_err());
    let mut d = DesignShape { rect: [40., 40., 120., 120.], angle: std::f64::consts::FRAC_PI_2, parameters: S::Star { points: 5, inner: 0.5, corner: 0. } };
    let h = geometry::handles(&d).unwrap().into_iter().find(|h| h.kind == HandleKind::Inner).unwrap();
    let c = Point::new(100., 100.);
    d = geometry::drag_handle(&d, HandleKind::Inner, Point::new(c.x + (h.position.x - c.x) * 1.5, c.y + (h.position.y - c.y) * 1.5)).unwrap();
    assert!(matches!(d.parameters,S::Star{inner,..}if(inner-0.75).abs()<1e-9));
}
#[test]
fn live_shapes_kittest_numeric_type_commits_once_and_settings_drop_transients() {
    let mut host = host();
    add_star(&mut host);
    host.controls = true;
    let mut h = harness(host);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Inner radius ratio").click();
    h.step();
    h.event(egui::Event::Text("0.65".into()));
    h.step();
    h.key_press(egui::Key::Enter);
    h.step();
    assert!(matches!(shape(&h).parameters,S::Star{inner,..}if(inner-0.65).abs()<1e-9));
    assert_eq!(h.state().session.journal.len(), journal + 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let state = &h.state().state;
    let bytes = serde_json::to_vec(state).unwrap();
    let loaded: State = serde_json::from_slice(&bytes).unwrap();
    assert!(!loaded.is_active());
    assert!(loaded.preview.is_none());
    assert!(loaded.preview_layer.is_none());
    assert!(serde_json::from_str::<State>("{}").unwrap().preview.is_none());
}
#[test]
fn live_shapes_kittest_invalid_numeric_and_escape_do_not_publish() {
    let mut host = host();
    add_star(&mut host);
    host.controls = true;
    let mut h = harness(host);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Inner radius ratio").click();
    h.step();
    h.event(egui::Event::Text("1.20".into()));
    h.step();
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Inner radius ratio").click();
    h.step();
    h.event(egui::Event::Text("0.70".into()));
    h.step();
    h.key_press(egui::Key::Escape);
    h.step();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    assert!(!h.state().state.is_active());
}
#[test]
fn live_shapes_kittest_zero_angle_can_be_typed_and_stale_metadata_is_refused() {
    let mut host = host();
    add_star(&mut host);
    host.controls = true;
    let mut h = harness(host);
    let journal = h.state().session.journal.len();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Rotation (radians)").click();
    h.step();
    h.event(egui::Event::Text("0.3".into()));
    h.step();
    h.key_press(egui::Key::Enter);
    h.step();
    assert!((shape(&h).angle - 0.3).abs() < 1e-9);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    let id = h.state().selected.unwrap();
    h.state_mut()
        .session
        .edit("Legacy geometry change", |doc, _| {
            let LayerContent::Shape(shape) = &mut doc.layer_mut(id).unwrap().content else { panic!("shape required") };
            shape.path.subpaths[0].knots[0].anchor.x += 5.;
            Ok(())
        })
        .unwrap();
    let before = h.state().session.active().unwrap().doc.clone();
    h.step();
    assert!(h.state().state.error.as_ref().unwrap().contains("base path"));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(!h.state().state.is_active());
}
#[test]
fn live_shapes_pointer_release_error_cancels_capture_and_line_shift_is_exact() {
    let mut host = host();
    let before = host.session.active().unwrap().doc.clone();
    let snap = |p: Point, _: Point, _: bool| p;
    super::pointer(&mut host.session, &mut host.state, Event::Down(Point::new(20., 20.)), Modifiers::NONE, None, true, 7., snap);
    super::pointer(&mut host.session, &mut host.state, Event::Move(Point::new(60., 60.)), Modifiers::NONE, None, true, 7., snap);
    super::pointer(&mut host.session, &mut host.state, Event::Up(Point::new(20., 20.)), Modifiers::NONE, None, true, 7., snap);
    assert!(!host.state.is_active());
    assert_eq!(host.session.active().unwrap().doc, before);
    assert!(host.session.journal.is_empty());
    host.state.parameters = parameters_for("line").unwrap();
    super::pointer(&mut host.session, &mut host.state, Event::Down(Point::new(30., 30.)), Modifiers::NONE, None, true, 7., snap);
    super::pointer(&mut host.session, &mut host.state, Event::Move(Point::new(150., 31.)), Modifiers::SHIFT, None, true, 7., snap);
    assert!(matches!(host.state.preview.as_ref().unwrap().parameters,S::Line{from,to}if from[1]==30.&&to[1]==30.));
    host.state.cancel();
}
