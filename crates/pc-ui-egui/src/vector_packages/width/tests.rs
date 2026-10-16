#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
struct Host {
    session: Session,
    width: State,
    ids: Vec<LayerId>,
    canvas: Rect,
}
fn fixture() -> Host {
    let mut doc = Document::new("Width UI", Size::new(180, 100), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polyline(&[(20.0, 50.0), (160.0, 50.0)])]),
        stroke: Some(ShapeStroke { width: 10.0, paint: Fill::Solid(Color::BLACK), ..Default::default() }),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
    let layer = Layer::new("Line", LayerContent::Shape(sh));
    let id = layer.id;
    doc.layers.push(layer);
    let mut session = Session::new();
    session.add_document(doc, None);
    Host { session, width: State::default(), ids: vec![id], canvas: Rect::NOTHING }
}
fn host_ui(ui: &mut Ui, host: &mut Host) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.scope(|ui| {
            // The real Stroke dock does not move the canvas when point controls appear.
            // Keep this host's canvas at a stable origin too.
            ui.set_min_height(390.0);
            host.width.panel(ui, &mut host.session, &host.ids);
        });
        let (response, painter) = ui.allocate_painter(egui::vec2(200.0, 110.0), Sense::click_and_drag());
        host.canvas = response.rect;
        let origin = host.canvas.min;
        host.width.input(&mut host.session, ui.ctx(), host.canvas, |p| Point::new(f64::from(p.x - origin.x), f64::from(p.y - origin.y)), &host.ids, true, 5.0);
        host.width.paint_handles(&host.session, &host.ids, &painter, |p| origin + egui::vec2(p.x as f32, p.y as f32));
    });
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(750.0, 650.0)).build_ui_state(host_ui, fixture())
}
fn event(h: &mut Harness<'_, Host>, p: Pos2, pressed: Option<bool>, mods: Modifiers) {
    match pressed {
        Some(pressed) => h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods }),
        None => h.event(Event::PointerMoved(p)),
    }
    h.step();
}
fn pointer(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: Option<bool>) {
    let p = h.state().canvas.min + egui::vec2(x, y);
    event(h, p, pressed, Modifiers::NONE);
}
fn profile(host: &Host) -> WidthProfile {
    engine::Gesture::begin(&host.session, &host.ids, None).unwrap().profile(0).unwrap()
}
fn add(h: &mut Harness<'_, Host>) {
    pointer(h, 90.0, 50.0, None);
    pointer(h, 90.0, 50.0, Some(true));
    pointer(h, 90.0, 30.0, None);
    pointer(h, 90.0, 30.0, Some(false));
    assert!(h.state().width.error.is_none(), "{:?}", h.state().width.error);
}
#[test]
fn width_package_ui_canvas_add_preview_release_one_undo() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, 90.0, 50.0, None);
    pointer(&mut h, 90.0, 50.0, Some(true));
    pointer(&mut h, 90.0, 30.0, None);
    assert!(h.state().width.is_dragging());
    assert!(h.state().width.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    let preview = h.state().width.preview.clone().unwrap();
    pointer(&mut h, 90.0, 30.0, Some(false));
    assert!(h.state().width.error.is_none(), "{:?}", h.state().width.error);
    let p = profile(h.state());
    assert_eq!(p.points.len(), 3);
    assert_eq!(p.points[1], WidthPoint { t: 0.5, left: 20.0, right: 20.0 });
    assert_eq!(*h.state().session.active().unwrap().doc, preview);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(!h.state_mut().session.undo());
}
#[test]
fn width_package_ui_canvas_move_asymmetric_modifiers_and_delete() {
    let mut h = harness();
    add(&mut h);
    pointer(&mut h, 90.0, 50.0, None);
    pointer(&mut h, 90.0, 50.0, Some(true));
    pointer(&mut h, 125.0, 50.0, None);
    pointer(&mut h, 125.0, 50.0, Some(false));
    assert!((profile(h.state()).points[1].t - 0.75).abs() < 1e-6);
    pointer(&mut h, 125.0, 30.0, None);
    pointer(&mut h, 125.0, 30.0, Some(true));
    h.event(Event::ModifiersChanged(Modifiers::ALT));
    h.step();
    pointer(&mut h, 125.0, 20.0, None);
    assert_eq!(h.state().width.gesture.as_ref().unwrap().profile(0).unwrap().points[1].right, 20.0);
    let p = h.state().canvas.min + egui::vec2(125.0, 20.0);
    event(&mut h, p, Some(false), Modifiers::ALT);
    let p = profile(h.state());
    assert_eq!((p.points[1].left, p.points[1].right), (30.0, 20.0));
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(profile(h.state()).points.len(), 2);
    assert_eq!(h.state().session.journal.len(), 4);
}
#[test]
fn width_package_ui_graph_drag_presets_saved_profile_and_old_options() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Width profile").click();
    h.run();
    h.get_by_label("Lens").click();
    h.run();
    assert_eq!(profile(h.state()).points[0].left, 0.0);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    let r = h.state().width.graph_rect;
    let start = egui::pos2(r.center().x, r.bottom() - r.height() * 5.0 / 12.5);
    event(&mut h, start, None, Modifiers::NONE);
    event(&mut h, start, Some(true), Modifiers::NONE);
    let end = egui::pos2(r.left() + r.width() * 0.6, r.bottom() - r.height() * 0.8);
    event(&mut h, end, None, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    event(&mut h, end, Some(false), Modifiers::NONE);
    let p = profile(h.state());
    assert!((p.points[1].t - 0.6).abs() < 1e-6);
    assert!((p.points[1].left - 10.0).abs() < 1e-5);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    h.get_by_label("Profile name").click();
    h.run();
    h.get_by_label("Profile name").type_text("Fine");
    h.run();
    h.get_by_label("Save width profile").click();
    h.run();
    assert_eq!(h.state().width.options.saved.len(), 1);
    let options: Options = serde_json::from_str(&serde_json::to_string(&h.state().width.options).unwrap()).unwrap();
    assert_eq!(options, h.state().width.options);
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Width profile").click();
    h.run();
    h.get_by_label("Uniform").click();
    h.run();
    assert_eq!(profile(h.state()).points.len(), 2);
    h.get_by_label("Fine").click();
    h.run();
    assert_eq!(profile(h.state()), p);
}
#[test]
fn width_package_ui_numeric_typing_escape_and_stale_cancel() {
    let mut h = harness();
    add(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Left width").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Left width").type_text("12");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(profile(h.state()).points[1].left, 12.0);
    assert_eq!(profile(h.state()).points[1].right, 20.0);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    for stale in [false, true] {
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Left width").click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Left width").type_text("35");
        h.run();
        assert_eq!(h.state().session.active().unwrap().doc, before);
        if stale {
            h.state_mut().session.active_mut().unwrap().revision += 1;
            h.key_press(Key::Enter);
        } else {
            h.key_press(Key::Escape);
        }
        h.run();
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), journal);
    }
}
#[test]
fn width_package_ui_escape_focus_selection_tool_switch_and_release_outside() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    for mode in 0..4 {
        pointer(&mut h, 90.0, 50.0, None);
        pointer(&mut h, 90.0, 50.0, Some(true));
        pointer(&mut h, 90.0, 30.0, None);
        match mode {
            0 => {
                h.key_press(Key::Escape);
                h.run();
            }
            1 => {
                h.event(Event::WindowFocused(false));
                h.step();
            }
            2 => {
                let host = h.state_mut();
                host.width.lifecycle(&host.session, &host.ids, false, true, false);
            }
            _ => {
                h.state_mut().session.active_mut().unwrap().revision += 1;
            }
        }
        pointer(&mut h, 90.0, 30.0, Some(false));
        assert!(h.state().width.preview.is_none());
        assert!(!h.state().width.is_dragging());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        h.event(Event::WindowFocused(true));
        h.step();
    }
    pointer(&mut h, 90.0, 50.0, None);
    pointer(&mut h, 90.0, 50.0, Some(true));
    pointer(&mut h, 90.0, 30.0, None);
    pointer(&mut h, 220.0, -30.0, Some(false));
    assert!(!h.state().width.is_dragging());
    assert_eq!(h.state().session.journal.len(), 1);
}

#[test]
fn width_package_ui_click_adds_inherited_width_without_pinching() {
    let mut h = harness();
    pointer(&mut h, 90.0, 50.0, None);
    pointer(&mut h, 90.0, 50.0, Some(true));
    pointer(&mut h, 90.0, 50.0, Some(false));
    let p = profile(h.state());
    assert_eq!(p.points.len(), 3);
    assert_eq!((p.points[1].left, p.points[1].right), (5.0, 5.0));
    assert_eq!(h.state().session.journal.len(), 1);
}
#[test]
fn bugfix_vector_width_delete_runs_before_generic_clear() {
    let mut h = harness();
    add(&mut h);
    let mut session = std::mem::replace(&mut h.state_mut().session, Session::new());
    session.select_layer(h.state().ids[0]).unwrap();
    session.edit_prefs(|p| p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector);
    let mut app = crate::PhotocraftApp::new(session, Default::default());
    app.ui.tool = crate::state::Tool::Width;
    app.width_state = std::mem::take(&mut h.state_mut().width);
    let ids = app.session.active().unwrap().selected_layers();
    let before = engine::Gesture::begin(&app.session, &ids, None).unwrap().profile(0).unwrap().points.len();
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        events: vec![Event::Key { key: Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
        ..Default::default()
    };
    ctx.run_ui(raw, |ui| crate::shortcuts::handle(&mut app, ui.ctx())).textures_delta.clear();
    let after = engine::Gesture::begin(&app.session, &ids, None).unwrap().profile(0).unwrap().points.len();
    assert_eq!(after + 1, before);
    assert!(!app.ui.status_error, "{}", app.ui.status);
}
