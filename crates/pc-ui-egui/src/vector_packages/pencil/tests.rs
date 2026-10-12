#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Layer, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    pencil: State,
    rect: egui::Rect,
    error: Option<String>,
}
fn host() -> Host {
    let mut session = Session::new();
    session.add_document(Document::new("Pencil kittest", Size::new(200, 200), ColorMode::Rgb, SampleType::U8), None);
    Host { session, pencil: State::default(), rect: egui::Rect::NOTHING, error: None }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(900., 400.)).build_ui_state(
        |ui, h| {
            ui.horizontal(|ui| context_bar(ui, &mut h.pencil));
            let (response, _) = ui.allocate_painter(vec2(200., 200.), egui::Sense::click_and_drag());
            h.rect = response.rect;
            let xf = ViewXform { rect: h.rect, zoom: 1.0, center: [100., 100.], flip: false, rotation: 0.0 };
            h.error = canvas_input(&mut h.pencil, &mut h.session, &response, &xf).err().map(|e| e.to_string());
        },
        host(),
    )
}
fn event(h: &mut Harness<'_, Host>, p: (f32, f32), kind: u8, mods: Modifiers) {
    let pos = h.state().rect.min + vec2(p.0, p.1);
    h.event(if kind == 1 {
        egui::Event::PointerMoved(pos)
    } else {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: kind == 0, modifiers: mods }
    });
    h.run_steps(1);
}
#[test]
fn pencil_kittest_real_freehand_release_alt_continue_close_and_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, (20., 20.), 0, Modifiers::NONE);
    event(&mut h, (60., 50.), 1, Modifiers::NONE);
    event(&mut h, (100., 20.), 1, Modifiers::NONE);
    assert!(h.state().pencil.preview().is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    event(&mut h, (100., 20.), 2, Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(h.state().session.journal.len(), 1);
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    event(&mut h, (100., 20.), 0, Modifiers::ALT);
    event(&mut h, (130., 60.), 1, Modifiers::NONE);
    event(&mut h, (150., 100.), 2, Modifiers::NONE);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert!(!sh.path.subpaths[0].closed);
    assert_eq!(sh.path.subpaths[0].knots.last().unwrap().anchor, Point::new(150., 100.));
    event(&mut h, (150., 100.), 0, Modifiers::ALT);
    event(&mut h, (80., 120.), 1, Modifiers::NONE);
    event(&mut h, (20., 20.), 2, Modifiers::ALT);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert!(sh.path.subpaths[0].closed);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    assert_eq!(h.state().session.journal.len(), 3);
    for _ in 0..3 {
        assert!(h.state_mut().session.undo());
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn pencil_kittest_click_stabilizers_escape_focus_loss_and_no_tail_on_next_stroke() {
    let mut h = harness();
    h.get_by_label("Rope").click();
    h.run_steps(2);
    assert_eq!(h.state().pencil.options.stabilizer, StabilizerKind::Rope);
    event(&mut h, (20., 20.), 0, Modifiers::NONE);
    event(&mut h, (80., 40.), 1, Modifiers::NONE);
    h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert!(!h.state().pencil.active());
    assert!(h.state().pencil.preview().is_none());
    event(&mut h, (80., 40.), 2, Modifiers::NONE);
    h.get_by_label("Window").click();
    h.run_steps(2);
    assert_eq!(h.state().pencil.options.stabilizer, StabilizerKind::Window);
    event(&mut h, (20., 150.), 0, Modifiers::NONE);
    event(&mut h, (80., 150.), 1, Modifiers::NONE);
    h.event(egui::Event::WindowFocused(false));
    h.run_steps(1);
    assert!(!h.state().pencil.active());
    event(&mut h, (80., 150.), 2, Modifiers::NONE);
    h.event(egui::Event::WindowFocused(true));
    h.run_steps(1);
    event(&mut h, (140., 150.), 0, Modifiers::NONE);
    event(&mut h, (160., 150.), 1, Modifiers::NONE);
    event(&mut h, (180., 150.), 2, Modifiers::NONE);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layers[0].content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots[0].anchor, Point::new(140., 150.));
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn pencil_kittest_sculpt_click_and_drag_retains_outside_anchors() {
    let mut h = harness();
    let path = Path::new(vec![Subpath::polyline(&[(20., 100.), (60., 100.), (100., 100.), (140., 100.), (180., 100.)])]);
    let mut shape = ShapeLayer { path: path.clone(), stroke: Some(Default::default()), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&h.state().session.active().unwrap().doc, &mut shape);
    let layer = Layer::new("Curve", LayerContent::Shape(shape));
    let id = layer.id;
    Arc::make_mut(&mut h.state_mut().session.active_mut().unwrap().doc).layers.push(layer);
    h.state_mut().session.select_layer(id).unwrap();
    h.get_by_label("Sculpt").click();
    h.run_steps(2);
    event(&mut h, (60., 100.), 0, Modifiers::NONE);
    event(&mut h, (100., 140.), 1, Modifiers::NONE);
    event(&mut h, (140., 100.), 2, Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots[0], path.subpaths[0].knots[0]);
    assert_eq!(sh.path.subpaths[0].knots.last(), path.subpaths[0].knots.last());
    assert!(sh.path.subpaths[0].knots.iter().any(|k| k.anchor.y > 100.));
    assert!(h.state_mut().session.undo());
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert_eq!(sh.path, path);
}
#[test]
fn pencil_ui_old_state_defaults_and_stale_cancel() {
    let mut state: State = serde_json::from_str("{}").unwrap();
    let mut host = host();
    state.pointer(&mut host.session, ToolEvent::Down { x: 20., y: 20., pressure: 0.2 }, Modifiers::NONE, 1.0).unwrap();
    state.pointer(&mut host.session, ToolEvent::Move { x: 50., y: 50., pressure: 0.8 }, Modifiers::NONE, 1.0).unwrap();
    host.session
        .edit("Change", |doc, _| {
            doc.name = "new name".into();
            Ok(())
        })
        .unwrap();
    assert!(state.display_doc(&host.session).is_none());
    assert!(state.pointer(&mut host.session, ToolEvent::Up { x: 80., y: 20. }, Modifiers::NONE, 1.0).is_err());
    assert!(!state.active());
    assert!(host.session.journal.is_empty());
}
#[test]
fn pencil_kittest_types_a_bounded_tolerance() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Tolerance").click();
    h.run_steps(2);
    h.event(egui::Event::Text("0.75".into()));
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().pencil.options.tolerance, 0.75);
}
#[test]
fn pencil_kittest_sculpt_segment_interior_retains_original_end_anchors() {
    let mut h = harness();
    let path = Path::new(vec![Subpath::polyline(&[(20., 100.), (180., 100.)])]);
    let layer = Layer::new("Curve", LayerContent::Shape(ShapeLayer { path: path.clone(), ..Default::default() }));
    let id = layer.id;
    Arc::make_mut(&mut h.state_mut().session.active_mut().unwrap().doc).layers.push(layer);
    h.state_mut().session.select_layer(id).unwrap();
    h.get_by_label("Sculpt").click();
    h.run_steps(2);
    event(&mut h, (60., 100.), 0, Modifiers::NONE);
    event(&mut h, (100., 145.), 1, Modifiers::NONE);
    event(&mut h, (140., 100.), 2, Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    for k in &path.subpaths[0].knots {
        assert!(sh.path.subpaths[0].knots.iter().any(|n| n.anchor == k.anchor));
    }
    assert!(sh.path.subpaths[0].knots.iter().any(|k| k.anchor.y > 100.));
}
