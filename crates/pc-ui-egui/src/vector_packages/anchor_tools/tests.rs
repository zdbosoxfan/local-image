//! Real egui_kittest pointer/keyboard input. Synthetic GPL-3.0-or-later curves.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Layer, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    session: Session,
    state: State,
    rect: Rect,
    error: Option<String>,
    shown: Vec<ShownHandle>,
}
fn host() -> Host {
    let mut doc = Document::new("Anchor kittest", Size::new(240, 180), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polyline(&[(20., 90.), (120., 90.), (220., 90.)])]),
        stroke: Some(ShapeStroke { width: 3., ..Default::default() }),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    let layer = Layer::new("Curve", LayerContent::Shape(sh));
    let id = layer.id;
    doc.layers.push(layer);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    let mut state = State::default();
    state.options.snapping.enabled = false;
    Host { session, state, rect: Rect::NOTHING, error: None, shown: Vec::new() }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(850., 320.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            context(ui, &mut h.state);
            let (response, painter) = ui.allocate_painter(vec2(240., 180.), egui::Sense::click_and_drag());
            h.rect = response.rect;
            let targets = selected_targets(&h.session, &Default::default(), false);
            let rect = h.rect;
            h.error = input(
                &mut h.state,
                &mut h.session,
                ui.ctx(),
                h.rect,
                |p| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)),
                targets,
                &h.shown,
                true,
                4.,
                snap::View::default(),
                None,
                Some(ShapeStroke { width: 3., ..Default::default() }),
            )
            .err()
            .map(|e| e.to_string());
            paint_overlay(&h.state, &painter, |p| rect.min + vec2(p.x as f32, p.y as f32));
        },
        host(),
    )
}
fn event(h: &mut Harness<'_, Host>, at: (f32, f32), kind: u8, mods: Modifiers) {
    let pos = h.state().rect.min + vec2(at.0, at.1);
    h.event(if kind == 1 {
        Event::PointerMoved(pos)
    } else {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed: kind == 0, modifiers: mods }
    });
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
fn click(h: &mut Harness<'_, Host>, p: (f32, f32), mods: Modifiers) {
    event(h, p, 0, mods);
    event(h, p, 2, mods);
}
fn key(h: &mut Harness<'_, Host>, key: Key, mods: Modifiers) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods });
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    h.event(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods });
    h.run_steps(1);
}
fn shape(h: &Host) -> &ShapeLayer {
    let st = h.session.active().unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!() };
    sh
}
#[test]
fn anchor33_kittest_plus_minus_alt_real_clicks_preview_and_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    key(&mut h, Key::Plus, Modifiers::NONE);
    event(&mut h, (70., 90.), 0, Modifiers::NONE);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    event(&mut h, (70., 90.), 2, Modifiers::NONE);
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 4);
    assert_eq!(h.state().session.journal.len(), 1);
    key(&mut h, Key::Minus, Modifiers::NONE);
    click(&mut h, (70., 90.), Modifiers::NONE);
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 3);
    assert_eq!(h.state().session.journal.len(), 2);
    // Delete + Alt adds, Add + Alt deletes.
    click(&mut h, (70., 90.), Modifiers::ALT);
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 4);
    key(&mut h, Key::Plus, Modifiers::NONE);
    click(&mut h, (70., 90.), Modifiers::ALT);
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 3);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 4);
    for _ in 0..4 {
        assert!(h.state_mut().session.undo());
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn anchor33_kittest_shift_c_symmetric_drag_click_corner_hidden_handle_and_escape() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    key(&mut h, Key::C, Modifiers::SHIFT);
    assert_eq!(h.state().state.options.tool, Tool::AnchorPoint);
    event(&mut h, (120., 90.), 0, Modifiers::NONE);
    event(&mut h, (140., 120.), 1, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    event(&mut h, (140., 120.), 2, Modifiers::NONE);
    let k = shape(h.state()).path.subpaths[0].knots[1];
    assert!(k.smooth);
    assert_eq!(k.in_ctrl, Point::new(100., 60.));
    assert_eq!(k.out_ctrl, Point::new(140., 120.));
    assert_eq!(h.state().session.journal.len(), 1);
    // Escape in a captured drag publishes nothing, including the subsequent release.
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, (120., 90.), 0, Modifiers::NONE);
    event(&mut h, (160., 140.), 1, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    event(&mut h, (160., 140.), 2, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), 1);
    // The displayed handle is independently draggable. A hidden handle produces no capture.
    event(&mut h, (140., 120.), 0, Modifiers::NONE);
    assert!(!h.state().state.captured());
    event(&mut h, (140., 120.), 2, Modifiers::NONE);
    h.state_mut().shown = vec![ShownHandle { target: 0, knot: [0, 1], handle: Handle::Out }];
    event(&mut h, (140., 120.), 0, Modifiers::NONE);
    event(&mut h, (160., 110.), 1, Modifiers::NONE);
    event(&mut h, (160., 110.), 2, Modifiers::NONE);
    let k = shape(h.state()).path.subpaths[0].knots[1];
    assert!(!k.smooth);
    assert_eq!(k.in_ctrl, Point::new(100., 60.));
    h.run_steps(30);
    click(&mut h, (120., 90.), Modifiers::NONE);
    let k = shape(h.state()).path.subpaths[0].knots[1];
    assert_eq!(k.in_ctrl, k.anchor);
    assert_eq!(k.out_ctrl, k.anchor);
}
#[test]
fn anchor33_kittest_curvature_three_clicks_escape_finishes_one_open_path() {
    let mut h = harness();
    h.get_by_label("Curvature").click();
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    click(&mut h, (30., 30.), Modifiers::NONE);
    click(&mut h, (110., 140.), Modifiers::NONE);
    click(&mut h, (200., 30.), Modifiers::NONE);
    event(&mut h, (230., 100.), 1, Modifiers::NONE);
    let band = h.state().state.rubber_band().unwrap();
    assert_eq!(band.subpaths[0].knots.len(), 4);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    key(&mut h, Key::Escape, Modifiers::NONE);
    let sh = shape(h.state());
    assert_eq!(sh.path.subpaths[0].knots.len(), 3);
    assert!(!sh.path.subpaths[0].closed);
    assert!(sh.path.subpaths[0].knots[1].smooth);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn anchor33_kittest_curvature_alt_toggle_double_click_drag_delete_and_close() {
    let mut h = harness();
    key(&mut h, Key::Backtick, Modifiers::SHIFT);
    click(&mut h, (30., 30.), Modifiers::NONE);
    click(&mut h, (100., 140.), Modifiers::ALT);
    click(&mut h, (200., 30.), Modifiers::NONE);
    assert!(!h.state().state.curvature_path().unwrap().subpaths[0].knots[1].smooth);
    h.run_steps(30);
    click(&mut h, (100., 140.), Modifiers::ALT);
    assert!(h.state().state.curvature_path().unwrap().subpaths[0].knots[1].smooth);
    // Clear egui's triple-click history as well as its double-click window.
    h.run_steps(60);
    click(&mut h, (100., 140.), Modifiers::NONE);
    click(&mut h, (100., 140.), Modifiers::NONE);
    assert!(h.ctx.input(|i| i.pointer.button_double_clicked(PointerButton::Primary)));
    assert!(!h.state().state.curvature_path().unwrap().subpaths[0].knots[1].smooth);
    h.run_steps(30);
    event(&mut h, (100., 140.), 0, Modifiers::NONE);
    event(&mut h, (120., 120.), 1, Modifiers::NONE);
    event(&mut h, (120., 120.), 2, Modifiers::NONE);
    assert_eq!(h.state().state.curvature_path().unwrap().subpaths[0].knots[1].anchor, Point::new(120., 120.));
    key(&mut h, Key::Delete, Modifiers::NONE);
    assert_eq!(h.state().state.curvature_path().unwrap().subpaths[0].knots.len(), 2);
    click(&mut h, (180., 140.), Modifiers::NONE);
    h.run_steps(30);
    click(&mut h, (30., 30.), Modifiers::NONE);
    assert!(!h.state().state.active());
    assert!(shape(h.state()).path.subpaths[0].closed);
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn anchor33_kittest_curvature_mid_drag_focus_tool_and_revision_cancel_without_edit() {
    let mut h = harness();
    key(&mut h, Key::Backtick, Modifiers::SHIFT);
    let before = h.state().session.active().unwrap().doc.clone();
    click(&mut h, (30., 30.), Modifiers::NONE);
    click(&mut h, (100., 140.), Modifiers::NONE);
    event(&mut h, (100., 140.), 0, Modifiers::NONE);
    event(&mut h, (120., 120.), 1, Modifiers::NONE);
    key(&mut h, Key::Escape, Modifiers::NONE);
    event(&mut h, (120., 120.), 2, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    click(&mut h, (30., 30.), Modifiers::NONE);
    click(&mut h, (100., 140.), Modifiers::NONE);
    h.event(Event::WindowFocused(false));
    h.run_steps(1);
    assert!(!h.state().state.active());
    h.event(Event::WindowFocused(true));
    h.run_steps(1);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    click(&mut h, (30., 30.), Modifiers::NONE);
    click(&mut h, (100., 140.), Modifiers::NONE);
    key(&mut h, Key::Plus, Modifiers::NONE);
    assert!(!h.state().state.active());
    assert!(h.state().session.journal.is_empty());
    event(&mut h, (70., 90.), 0, Modifiers::NONE);
    h.state_mut()
        .session
        .edit("Unrelated", |d, _| {
            d.name = "Changed".into();
            Ok(())
        })
        .unwrap();
    event(&mut h, (70., 90.), 2, Modifiers::NONE);
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 3);
    assert!(h.state().session.journal.is_empty());
    let state: State = serde_json::from_str("{}").unwrap();
    assert!(state.options.rubber_band);
    assert_eq!(state.options.tool, Tool::AddAnchor);
}

#[test]
fn anchor33_kittest_live_shape_requests_conversion_before_any_edit() {
    let mut h = harness();
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    let doc = Arc::make_mut(&mut h.state_mut().session.active_mut().unwrap().doc);
    let LayerContent::Shape(sh) = &mut doc.layer_mut(id).unwrap().content else { panic!() };
    sh.live = Some(photocraft_doc::LiveShape::Ellipse { rect: [20., 20., 200., 100.] });
    let before = h.state().session.active().unwrap().doc.clone();
    let pos = h.state().rect.min + vec2(70., 90.);
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().error.as_ref().is_some_and(|e| e.contains("Convert to curves")));
    assert!(h.state().state.needs_conversion);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    event(&mut h, (70., 90.), 2, Modifiers::NONE);
    let host = h.state_mut();
    host.state.confirm_conversion(&host.session).unwrap();
    event(&mut h, (70., 90.), 2, Modifiers::NONE);
    assert!(shape(h.state()).live.is_none());
    assert_eq!(shape(h.state()).path.subpaths[0].knots.len(), 4);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
