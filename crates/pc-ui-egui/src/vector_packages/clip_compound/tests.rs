#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Layer, LayerContent, SampleType, ShapeLayer, Size};
struct Host {
    session: Session,
    state: State,
    rows: Vec<(LayerId, Rect)>,
    canvas: Rect,
    vector: bool,
    drawing: bool,
    start: Option<Pos2>,
}
pub(super) fn fixture() -> Session {
    let mut doc = Document::new("Clip UI", Size::new(160, 160), ColorMode::Rgb, SampleType::U8);
    for (name, path, color) in [
        ("Contents", photocraft_vector::shapes::rect(10., 10., 130., 130.), Color::BLACK),
        ("Ellipse", photocraft_vector::shapes::ellipse(30., 30., 80., 80.), Color::WHITE),
    ] {
        let mut shape = ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        doc.layers.push(Layer::new(name, LayerContent::Shape(shape)));
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    let ids = s.active().unwrap().doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    s.active_mut().unwrap().selected_layers = ids;
    s
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(720., 660.)).build_ui_state(
        |ui, h: &mut Host| {
            shortcuts(ui.ctx(), &mut h.session, &mut h.state, h.vector);
            draw_mode_widget(ui, &h.session, &mut h.state, str::to_owned);
            ui.menu_button("Object", |ui| context_menu(ui, &mut h.session, &mut h.state, str::to_owned));
            h.rows = layers_fragment(ui, &mut h.session, &mut h.state);
            let (rect, response) = ui.allocate_exact_size(egui::vec2(220., 220.), Sense::click_and_drag());
            h.canvas = rect;
            indicator(ui.painter(), &h.state, rect.shrink(20.));
            if h.drawing {
                if response.drag_started() {
                    h.start = ui.input(|i| i.pointer.press_origin());
                    h.state.begin_draw(&h.session, None, Some(Fill::Solid(Color::BLACK)), None).unwrap();
                }
                if h.state.drawing.is_some()
                    && let (Some(start), Some(end)) = (h.start, ui.input(|i| i.pointer.interact_pos()))
                {
                    let a = start - rect.min;
                    let b = end - rect.min;
                    let path = photocraft_vector::shapes::rect(
                        f64::from(a.x.min(b.x)),
                        f64::from(a.y.min(b.y)),
                        f64::from((b.x - a.x).abs()),
                        f64::from((b.y - a.y).abs()),
                    );
                    h.state.preview_draw(&h.session, path).unwrap();
                }
                if response.drag_stopped() && h.state.drawing.is_some() {
                    h.state.commit_draw(&mut h.session).unwrap();
                    h.start = None;
                }
            }
        },
        Host { session: fixture(), state: State::default(), rows: Vec::new(), canvas: Rect::NOTHING, vector: true, drawing: false, start: None },
    )
}
fn key(h: &mut Harness<'_, Host>, mods: Modifiers, key: Key) {
    h.key_press_modifiers(mods, key);
    h.run_steps(2);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
}
fn pointer(h: &mut Harness<'_, Host>, p: Pos2, button: Option<bool>) {
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
#[test]
fn clip35_kittest_keyboard_make_release_clip_compound_fill_rule_and_persona_scope() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    key(&mut h, Modifiers { ctrl: true, ..Modifiers::COMMAND }, Key::Num7);
    let doc = h.state().session.active().unwrap().doc.clone();
    assert_eq!(doc.layers.len(), 1);
    assert!(doc.layers[0].vector_mask.is_some());
    key(&mut h, Modifiers { alt: true, ..Modifiers::COMMAND }, Key::Num7);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    assert_eq!(h.state().session.journal.len(), 2);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    // History restores the layers targeted when the document state was created.
    // Start the independent compound keyboard flow with both shapes selected.
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::Num8);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    h.get_by_label("Object").click();
    h.run_steps(2);
    h.get_by_label("Even-Odd Fill").click();
    h.run_steps(2);
    let doc = h.state().session.active().unwrap().doc.clone();
    let LayerContent::Shape(sh) = &doc.layers[0].content else { panic!("compound shape") };
    assert_eq!(sh.path.fill_rule, FillRule::EvenOdd);
    key(&mut h, Modifiers { alt: true, shift: true, ..Modifiers::COMMAND }, Key::Num8);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    h.state_mut().vector = false;
    let before = h.state().session.active().unwrap().doc.clone();
    key(&mut h, Modifiers::COMMAND, Key::Num7);
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn clip35_kittest_shift_d_widget_cycle_defaults_and_inside_disablement() {
    let mut h = harness();
    assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    h.state_mut().session.select_layer(id).unwrap();
    key(&mut h, Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().state.settings.mode, DrawMode::Behind);
    key(&mut h, Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().state.settings.mode, DrawMode::Inside);
    key(&mut h, Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().state.settings.mode, DrawMode::Normal);
    h.get_by_label("Draw Inside").click();
    h.run_steps(2);
    assert_eq!(h.state().state.settings.mode, DrawMode::Inside);
    h.get_by_label("Draw Behind").click();
    h.run_steps(2);
    assert_eq!(h.state().state.settings.mode, DrawMode::Behind);
    let ids = h.state().session.active().unwrap().doc.layers.iter().map(|l| l.id).collect();
    h.state_mut().session.active_mut().unwrap().selected_layers = ids;
    key(&mut h, Modifiers::SHIFT, Key::D);
    assert_eq!(h.state().state.settings.mode, DrawMode::Normal);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn clip35_kittest_layers_actual_drag_in_preview_release_outside_and_cancel() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let source = before.layers[0].id;
    let target = before.layers[1].id;
    h.state_mut().session.select_layer(source).unwrap();
    h.run_steps(2);
    let from = h.state().rows.iter().find(|(id, _)| *id == source).unwrap().1.center();
    let to = h.state().rows.iter().find(|(id, _)| *id == target).unwrap().1.center();
    pointer(&mut h, from, Some(true));
    pointer(&mut h, from + egui::vec2(8., 0.), None);
    pointer(&mut h, to, None);
    assert!(h.state().state.captured());
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
    assert_eq!(h.state().session.journal.len(), 1);
    let gid = engine::clip_parent(&h.state().session.active().unwrap().doc, source).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc.layer(gid).unwrap().children().unwrap().len(), 2);
    h.get_by_label("Object").click();
    h.run_steps(2);
    h.get_by_label("Move Outside").click();
    h.run_steps(2);
    assert!(h.state().session.active().unwrap().doc.layers.iter().any(|l| l.id == source));
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    h.run_steps(2);
    pointer(&mut h, from, Some(true));
    pointer(&mut h, from + egui::vec2(8., 0.), None);
    pointer(&mut h, to, None);
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer(&mut h, to, Some(false));
    assert!(!h.state().state.captured());
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(!h.state_mut().session.undo());
}
#[test]
fn clip35_kittest_draw_rectangle_inside_ellipse_actual_pointer_and_stale_cancel() {
    let mut h = harness();
    let target = h.state().session.active().unwrap().active_layer.unwrap();
    h.state_mut().session.select_layer(target).unwrap();
    h.state_mut().drawing = true;
    h.run_steps(2);
    h.get_by_label("Draw Inside").click();
    h.run_steps(2);
    let rect = h.state().canvas;
    let from = rect.min + egui::vec2(10., 10.);
    let to = rect.min + egui::vec2(130., 130.);
    let before = h.state().session.active().unwrap().doc.clone();
    pointer(&mut h, from, Some(true));
    pointer(&mut h, from + egui::vec2(8., 8.), None);
    pointer(&mut h, to, None);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, to, Some(false));
    h.run_steps(2);
    let doc = h.state().session.active().unwrap().doc.clone();
    let gid = h.state().session.active().unwrap().active_layer.unwrap();
    let g = doc.layer(gid).unwrap();
    assert!(g.vector_mask.is_some());
    assert_eq!(g.children().unwrap().len(), 2);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let Host { session, state, .. } = h.state_mut();
    state.begin_draw(session, None, Some(Fill::Solid(Color::BLACK)), None).unwrap();
    session.select_layer(target).unwrap();
    state.frame(session, true, true, false);
    assert!(!state.captured());
    assert!(state.preview.is_none());
    state.begin_draw(session, None, Some(Fill::Solid(Color::BLACK)), None).unwrap();
    state.preview_draw(session, photocraft_vector::shapes::rect(10., 10., 30., 30.)).unwrap();
    let mut invalid = photocraft_vector::shapes::rect(10., 10., 30., 30.);
    invalid.subpaths[0].knots[0].anchor.x = f64::NAN;
    assert!(state.preview_draw(session, invalid).is_err());
    assert!(!state.captured());
    assert!(state.preview.is_none());
    assert!(state.commit_draw(session).is_err());
    assert_eq!(session.active().unwrap().doc, before);
}
#[test]
fn clip35_kittest_edit_mask_contents_clicks_target_authoritative_group_path() {
    let mut h = harness();
    key(&mut h, Modifiers::COMMAND, Key::Num7);
    let gid = h.state().session.active().unwrap().active_layer.unwrap();
    let rev = h.state().session.active().unwrap().revision;
    for (label, mask) in [("Edit Mask", true), ("Edit Contents", false)] {
        h.get_by_label("Object").click();
        h.run_steps(2);
        h.get_by_label(label).click();
        h.run_steps(2);
        assert!(h.state().state.error.is_none());
        if mask {
            assert_eq!(h.state().state.mask_target, Some(gid));
        } else {
            assert_ne!(h.state().session.active().unwrap().active_layer, Some(gid));
        }
        assert_eq!(h.state().session.active().unwrap().revision, rev);
        assert_eq!(h.state().session.journal.len(), 1);
    }
}
