//! Package-owned real egui input tests. No desktop window or test-only edit path.
#[path = "../src/vector_packages/layout.rs"]
#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod layout_ui;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, LayerId, SampleType, ShapeLayer, Size};
use photocraft_engine::{Session, vector_packages::layout as engine};
use photocraft_geom::{Point, Rect};
struct Host {
    s: Session,
    state: layout_ui::State,
    rect: egui::Rect,
    error: Option<String>,
    enabled: bool,
}
fn host() -> Host {
    let mut doc = Document::new("Layout widgets", Size::new(300, 240), ColorMode::Rgb, SampleType::U8);
    for (name, b) in [("First", [20., 20., 40., 40.]), ("Key", [100., 20., 60., 40.]), ("Last", [200., 20., 30., 40.])] {
        let mut sh = ShapeLayer { path: engine::crop_path(b).unwrap(), fill: Some(Fill::Solid(Color::rgb(0.3, 0.6, 0.9))), ..Default::default() };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new(name, LayerContent::Shape(sh)));
    }
    let ids = doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    let mut s = Session::new();
    s.add_document(doc, None);
    s.execute("layer.select", serde_json::json!({"layer":ids[0].0})).unwrap();
    Host { s, state: Default::default(), rect: egui::Rect::NOTHING, error: None, enabled: true }
}
fn draw(ui: &mut egui::Ui, h: &mut Host) {
    if let Err(e) = layout_ui::panel(ui, &mut h.state, &mut h.s, str::to_string) {
        h.error = Some(e.to_string());
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(500., 300.), egui::Sense::click_and_drag());
    h.rect = rect;
    if let Err(e) = layout_ui::input(
        &mut h.state,
        &mut h.s,
        ui.ctx(),
        rect,
        |p| Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)),
        Default::default(),
        h.enabled,
    ) {
        h.error = Some(e.to_string());
    }
    layout_ui::overlay(&h.state, &h.s, ui.painter(), |p| rect.min + egui::vec2(p.x as f32, p.y as f32));
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1100., 1100.)).build_ui_state(draw, host())
}
fn pointer(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: Option<bool>, mods: Modifiers) {
    let pos = h.state().rect.min + egui::vec2(x, y);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
}
fn type_field(h: &mut Harness<'_, Host>, label: &str, value: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).click();
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.step();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, label).type_text(value);
    h.step();
}
fn first(h: &Harness<'_, Host>) -> LayerId {
    h.state().s.active().unwrap().doc.layers[0].id
}
#[test]
fn layout16_ui_anchor_actual_click_type_preview_enter_one_undo() {
    let mut h = harness();
    let id = first(&h);
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_label("Anchor bottom right").click();
    h.step();
    type_field(&mut h, "Width", "80");
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    assert!(h.state().state.is_fresh(&h.state().s));
    h.key_press(Key::Enter);
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(engine::bounds(h.state().s.active().unwrap().doc.layer(id).unwrap()).unwrap(), [-20., 20., 80., 40.]);
    assert_eq!(h.state().s.journal.len(), journal + 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn layout16_ui_key_object_and_spacing_are_actual_clicks_and_units() {
    let mut h = harness();
    let ids = h.state().s.active().unwrap().doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    for id in &ids[1..] {
        h.state_mut().s.execute("layer.select", serde_json::json!({"layer":id.0,"mode":"add"})).unwrap();
    }
    h.step();
    h.get_by_label("Key").click();
    h.step();
    type_field(&mut h, "Spacing", "12.5");
    h.key_press(Key::Enter);
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    h.get_by_label("Distribute horizontal spacing").click();
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let st = h.state().s.active().unwrap();
    let boxes = ids.iter().map(|id| engine::bounds(st.doc.layer(*id).unwrap()).unwrap()).collect::<Vec<_>>();
    assert_eq!(boxes[1], [100., 20., 60., 40.]);
    assert_eq!(boxes[0][0], 47.5);
    assert_eq!(boxes[2][0], 172.5);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    // Session undo restores its historical target selection; select this set again.
    for id in &ids {
        h.state_mut().s.execute("layer.select", serde_json::json!({"layer":id.0,"mode":"add"})).unwrap();
    }
    h.step();
    h.get_by_label("Key").click();
    h.step();
    h.get_by_label("Align right").click();
    h.step();
    let st = h.state().s.active().unwrap();
    for id in ids {
        let b = engine::bounds(st.doc.layer(id).unwrap()).unwrap();
        assert_eq!(b[0] + b[2], 160.);
    }
}
#[test]
fn layout16_ui_artboard_drag_shift_current_modifiers_release_outside_and_undo() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_label("Vector Artboard tool").click();
    h.step();
    pointer(&mut h, 30., 90., Some(true), Modifiers::NONE);
    pointer(&mut h, 130., 150., None, Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    let preview = h.state().state.preview.clone().unwrap();
    assert_eq!(preview.artboards()[0].2.rect, Rect::new(30, 90, 130, 150));
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.step();
    assert_eq!(h.state().state.preview.as_ref().unwrap().artboards()[0].2.rect, Rect::new(30, 90, 130, 190));
    pointer(&mut h, 530., 350., Some(false), Modifiers::SHIFT);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(h.state().s.active().unwrap().doc.artboards()[0].2.rect, Rect::new(30, 90, 530, 590));
    assert_eq!(h.state().s.journal.len(), journal + 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn layout16_ui_point_origin_click_node_drag_and_pivot_are_preserved() {
    let mut h = harness();
    let id = first(&h);
    let before = h.state().s.active().unwrap().doc.clone();
    h.get_by_label("Point Transform tool").click();
    h.step();
    // Non-node click chooses a custom origin; dragging a real corner then transforms.
    pointer(&mut h, 40., 40., Some(true), Modifiers::NONE);
    pointer(&mut h, 40., 40., Some(false), Modifiers::NONE);
    assert_eq!(h.state().state.pivot, Some(Point::new(40., 40.)));
    pointer(&mut h, 60., 20., Some(true), Modifiers::NONE);
    pointer(&mut h, 80., 80., None, Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    pointer(&mut h, 80., 80., Some(false), Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let LayerContent::Shape(sh) = &h.state().s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    let knot = sh.path.subpaths[0].knots[1].anchor;
    assert!((knot.x - 80.).abs() < 1e-8 && (knot.y - 80.).abs() < 1e-8);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn layout16_ui_crop_drag_preview_escape_cancel_commit_clear_undo() {
    let mut h = harness();
    let id = first(&h);
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_label("Vector Crop tool").click();
    h.step();
    pointer(&mut h, 60., 60., Some(true), Modifiers::NONE);
    pointer(&mut h, 45., 50., None, Modifiers::NONE);
    let preview = h.state().state.preview.clone().unwrap();
    assert!(preview.layer(id).unwrap().vector.clip.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.key_press(Key::Escape);
    h.step();
    pointer(&mut h, 45., 50., Some(false), Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.journal.len(), journal);
    pointer(&mut h, 60., 60., Some(true), Modifiers::NONE);
    pointer(&mut h, 45., 50., Some(false), Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert!(h.state().s.active().unwrap().doc.layer(id).unwrap().vector.clip.is_some());
    h.get_by_label("Clear vector crop").click();
    h.step();
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state_mut().s.undo());
    assert!(h.state().s.active().unwrap().doc.layer(id).unwrap().vector.clip.is_some());
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn layout16_ui_capture_cancels_on_focus_tool_and_document_change() {
    for kind in 0..3 {
        let mut h = harness();
        let before = h.state().s.active().unwrap().doc.clone();
        h.get_by_label("Vector Crop tool").click();
        h.step();
        pointer(&mut h, 60., 60., Some(true), Modifiers::NONE);
        pointer(&mut h, 45., 50., None, Modifiers::NONE);
        match kind {
            0 => h.event(Event::WindowFocused(false)),
            1 => h.state_mut().enabled = false,
            _ => {
                h.state_mut()
                    .s
                    .edit("Other edit", |doc, _| {
                        doc.name = "Changed".into();
                        Ok(())
                    })
                    .unwrap();
            }
        }
        h.step();
        assert!(h.state().state.preview.is_none());
        pointer(&mut h, 45., 50., Some(false), Modifiers::NONE);
        assert_eq!(h.state().s.active().unwrap().doc.layers, before.layers);
    }
}

#[test]
fn layout16_ui_point_transform_drags_real_child_node_in_selected_group() {
    let mut h = harness();
    let id = first(&h);
    let group = h
        .state_mut()
        .s
        .edit("Group fixture", |doc, _| {
            let child = doc.layers.remove(0);
            let group = Layer::group("Selected group", vec![child]);
            let id = group.id;
            doc.layers.push(group);
            Ok(id)
        })
        .unwrap();
    h.state_mut().s.execute("layer.select", serde_json::json!({"layer":group.0})).unwrap();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    h.get_by_label("Point Transform tool").click();
    h.step();
    pointer(&mut h, 40., 40., Some(true), Modifiers::NONE);
    pointer(&mut h, 40., 40., Some(false), Modifiers::NONE);
    pointer(&mut h, 60., 20., Some(true), Modifiers::NONE);
    pointer(&mut h, 80., 80., None, Modifiers::NONE);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    pointer(&mut h, 80., 80., Some(false), Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let LayerContent::Shape(sh) = &h.state().s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    let point = sh.path.subpaths[0].knots[1].anchor;
    assert!((point.x - 80.).abs() < 1e-8 && (point.y - 80.).abs() < 1e-8);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}

#[test]
fn layout16_ui_artboard_numeric_width_uses_child_constraints_and_one_undo() {
    let mut h = harness();
    let child = first(&h);
    let parent = h
        .state_mut()
        .s
        .edit("Board fixture", |doc, _| {
            let mut child = doc.layers.remove(0);
            child.vector.constraints = Some(photocraft_doc::vector_design::structure::Constraints {
                right: true,
                top: true,
                fixed_width: true,
                fixed_height: true,
                ..Default::default()
            });
            let mut board = Layer::group("Numeric board", vec![child]);
            if let LayerContent::Group(g) = &mut board.content {
                g.artboard = Some(photocraft_doc::Artboard {
                    rect: Rect::new(0, 0, 300, 240),
                    background: photocraft_doc::ArtboardBackground::Transparent,
                    preset: String::new(),
                    drawing_scale: None,
                });
            }
            let id = board.id;
            doc.layers.push(board);
            Ok(id)
        })
        .unwrap();
    h.state_mut().s.execute("layer.select", serde_json::json!({"layer":parent.0})).unwrap();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_label("Anchor top left").click();
    h.step();
    type_field(&mut h, "Width", "400");
    let preview = h.state().state.preview.as_ref().unwrap();
    assert_eq!(engine::bounds(preview.layer(child).unwrap()).unwrap(), [120., 20., 40., 40.]);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.key_press(Key::Enter);
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(engine::bounds(h.state().s.active().unwrap().doc.layer(child).unwrap()).unwrap(), [120., 20., 40., 40.]);
    assert_eq!(h.state().s.journal.len(), journal + 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
