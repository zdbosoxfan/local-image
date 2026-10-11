//! Real egui clicks/drags/keys through the owned UI and engine specs. The small host dispatches
//! package specs directly; full-app tests separately require 00's Session registry/canvas wiring.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
struct Host {
    s: Session,
    state: State,
    rect: Rect,
    error: Option<String>,
}
fn dispatch(s: &mut Session, id: &str, v: Value) -> Result<Value> {
    let spec = engine::specs().into_iter().find(|c| c.id == id).unwrap();
    (spec.run)(s, &v)
}
fn host() -> Host {
    let mut doc = Document::new("Distort kittest", Size::new(240, 180), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20., 30.), (220., 30.), (220., 130.), (20., 130.)])]),
        fill: Some(Fill::Solid(Color::rgb(0.1, 0.4, 0.8))),
        ..Default::default()
    };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let l = Layer::new("Curve", LayerContent::Shape(shape));
    let id = l.id;
    doc.layers.push(l);
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(id).unwrap();
    let mut state = State::default();
    state.options.brush.simplify_on = false;
    Host { s, state, rect: Rect::NOTHING, error: None }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(1100., 460.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            context(ui, &mut h.state);
            h.error = conversion_prompt_with(ui, &mut h.state, &mut h.s, dispatch).err().map(|e| e.to_string());
            let (response, painter) = ui.allocate_painter(vec2(240., 180.), egui::Sense::click_and_drag());
            h.rect = response.rect;
            let rect = h.rect;
            if let Err(e) =
                input_with(&mut h.state, &mut h.s, ui.ctx(), rect, |p| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)], true, 1., 5., dispatch)
            {
                h.error = Some(e.to_string());
            }
            paint_overlay(&h.state, &painter, |p| rect.min + vec2(p[0] as f32, p[1] as f32));
        },
        host(),
    )
}
fn event(h: &mut Harness<'_, Host>, at: [f32; 2], pressed: Option<bool>, mods: Modifiers) {
    let pos = h.state().rect.min + vec2(at[0], at[1]);
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
fn choose(h: &mut Harness<'_, Host>, t: Tool) {
    h.get_by_label(t.label()).click();
    h.run_steps(2);
    assert_eq!(h.state().state.options.tool, t);
}
#[test]
fn distort41_kittest_every_brush_real_drag_preview_release_one_undo() {
    for t in Tool::ALL.into_iter().filter(|t| *t != Tool::PuppetWarp) {
        let mut h = harness();
        choose(&mut h, t);
        let before = h.state().s.active().unwrap().doc.clone();
        let history = h.state().s.active().unwrap().history.past_len();
        event(&mut h, [210., 80.], Some(true), Modifiers::NONE);
        event(&mut h, [220., 90.], None, Modifiers::NONE);
        let preview = h.state().state.preview.as_ref().unwrap().document.clone();
        assert_ne!(preview, before, "{t:?}");
        assert_eq!(h.state().s.active().unwrap().doc, before);
        assert_eq!(h.state().s.active().unwrap().history.past_len(), history);
        event(&mut h, [220., 90.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().s.active().unwrap().doc, preview, "{t:?}");
        assert_eq!(h.state().s.active().unwrap().history.past_len(), history + 1);
        assert!(h.state_mut().s.undo());
        assert_eq!(h.state().s.active().unwrap().doc, before);
    }
}
#[test]
fn distort41_kittest_alt_resize_shift_proportions_and_escape_do_not_edit() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    let history = h.state().s.active().unwrap().history.past_len();
    event(&mut h, [100., 80.], Some(true), Modifiers::ALT);
    event(&mut h, [120., 90.], None, Modifiers::ALT);
    assert_eq!((h.state().state.options.brush.width, h.state().state.options.brush.height), (140., 120.));
    event(&mut h, [120., 90.], Some(false), Modifiers::ALT);
    event(&mut h, [100., 80.], Some(true), Modifiers::ALT);
    event(&mut h, [90., 85.], None, Modifiers::ALT | Modifiers::SHIFT);
    assert!((h.state().state.options.brush.height / 120. - h.state().state.options.brush.width / 140.).abs() < 1e-12);
    event(&mut h, [90., 85.], Some(false), Modifiers::ALT | Modifiers::SHIFT);
    event(&mut h, [210., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [230., 85.], None, Modifiers::NONE);
    h.key_press(Key::Escape);
    h.run_steps(1);
    event(&mut h, [230., 85.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), history);
}
#[test]
fn distort41_kittest_pins_automatic_add_drag_delete_and_escape_restores_pins() {
    let mut h = harness();
    choose(&mut h, Tool::PuppetWarp);
    assert!(h.state().state.pins.len() >= 3);
    event(&mut h, [90., 50.], Some(true), Modifiers::NONE);
    event(&mut h, [90., 50.], Some(false), Modifiers::NONE);
    let i = h.state().state.selected_pins[0];
    assert_eq!(h.state().state.pins[i].at, [90., 50.]);
    let before = h.state().s.active().unwrap().doc.clone();
    let pins = h.state().state.pins.clone();
    let history = h.state().s.active().unwrap().history.past_len();
    event(&mut h, [90., 50.], Some(true), Modifiers::NONE);
    event(&mut h, [95., 55.], None, Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    h.key_press(Key::Escape);
    h.run_steps(1);
    event(&mut h, [95., 55.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().state.pins, pins);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    event(&mut h, [90., 50.], Some(true), Modifiers::NONE);
    event(&mut h, [95., 55.], None, Modifiers::NONE);
    let preview = h.state().state.preview.as_ref().unwrap().document.clone();
    event(&mut h, [95., 55.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, preview);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), history + 1);
    let count = h.state().state.pins.len();
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().state.pins.len(), count - 1);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn distort41_kittest_alt_rotate_selected_pin_and_all_pins_rigid_drag() {
    let mut h = harness();
    choose(&mut h, Tool::PuppetWarp);
    let p = h.state().state.pins[0].at;
    let at = [p[0].round() as f32, p[1].round() as f32];
    event(&mut h, at, Some(true), Modifiers::NONE);
    event(&mut h, at, Some(false), Modifiers::NONE);
    event(&mut h, [at[0] + 12., at[1]], Some(true), Modifiers::ALT);
    event(&mut h, [at[0] + 12., at[1] + 1.], None, Modifiers::ALT);
    assert!(h.state().state.pins[0].angle.unwrap().abs() > 0.05);
    assert!(h.state().state.preview.is_some());
    h.key_press(Key::Escape);
    h.run_steps(1);
    event(&mut h, [at[0] + 12., at[1] + 1.], Some(false), Modifiers::ALT);
    let before_rotation = h.state().s.active().unwrap().doc.clone();
    let history = h.state().s.active().unwrap().history.past_len();
    event(&mut h, [at[0] + 12., at[1]], Some(true), Modifiers::ALT);
    event(&mut h, [at[0] + 12., at[1] + 1.], None, Modifiers::ALT);
    let rotation = h.state().state.preview.as_ref().unwrap().document.clone();
    event(&mut h, [at[0] + 12., at[1] + 1.], Some(false), Modifiers::ALT);
    assert_eq!(h.state().s.active().unwrap().doc, rotation);
    assert_eq!(h.state().s.active().unwrap().history.past_len(), history + 1);
    assert!(h.state_mut().s.undo());
    h.run_steps(1);
    assert_eq!(h.state().s.active().unwrap().doc, before_rotation);
    h.get_by_label("Select all pins").click();
    h.run_steps(2);
    assert_eq!(h.state().state.selected_pins.len(), h.state().state.pins.len());
    let before = h.state().s.active().unwrap().doc.clone();
    event(&mut h, at, Some(true), Modifiers::NONE);
    event(&mut h, [at[0] + 8., at[1] + 4.], None, Modifiers::NONE);
    event(&mut h, [at[0] + 8., at[1] + 4.], Some(false), Modifiers::NONE);
    let st = h.state().s.active().unwrap();
    let id = st.active_layer.unwrap();
    let LayerContent::Shape(sh) = &st.doc.layer(id).unwrap().content else { panic!() };
    let LayerContent::Shape(old) = &before.layer(id).unwrap().content else { panic!() };
    assert_eq!(sh.path, old.path.transform(&photocraft_geom::Affine::translate(8., 4.)));
}
#[test]
fn distort41_kittest_conversion_prompt_real_confirm_and_cancel() {
    let mut h = harness();
    let id = h.state().s.active().unwrap().active_layer.unwrap();
    h.state_mut()
        .s
        .edit("Live", |doc, _| {
            let LayerContent::Shape(sh) = &mut doc.layer_mut(id).unwrap().content else { panic!() };
            sh.live = Some(photocraft_doc::LiveShape::Rect { rect: [20., 30., 200., 100.], radii: [0.; 4] });
            Ok(())
        })
        .unwrap();
    let before = h.state().s.active().unwrap().doc.clone();
    let pos = h.state().rect.min + vec2(210., 80.);
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().state.conversion.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().state.conversion.is_none());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    let pos = h.state().rect.min + vec2(210., 80.);
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.get_by_label("Convert to curves").click();
    h.run_steps(2);
    assert!(h.state().state.conversion.is_none());
    let LayerContent::Shape(sh) = &h.state().s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert!(sh.live.is_none());
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
#[test]
fn distort41_kittest_focus_tool_and_document_revision_cancel_capture() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    event(&mut h, [210., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [220., 80.], None, Modifiers::NONE);
    h.event(Event::WindowFocused(false));
    h.run_steps(1);
    assert!(!h.state().state.captured());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.event(Event::WindowFocused(true));
    h.run_steps(1);
    event(&mut h, [210., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [220., 80.], None, Modifiers::NONE);
    choose(&mut h, Tool::Pucker);
    assert!(!h.state().state.captured());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    event(&mut h, [210., 80.], Some(true), Modifiers::NONE);
    h.state_mut()
        .s
        .edit("External", |doc, _| {
            doc.name = "New revision".into();
            Ok(())
        })
        .unwrap();
    h.run_steps(1);
    assert!(!h.state().state.captured());
    assert!(h.state().state.preview.is_none());
}

#[test]
fn distort41_kittest_numeric_brush_entry_and_keyboard_focus_use_real_widgets() {
    let mut h = harness();
    choose(&mut h, Tool::Pucker);
    let before = h.state().s.active().unwrap().doc.clone();
    h.get_by_label("Width").click();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::SHIFT, Key::R);
    h.run_steps(1);
    assert_eq!(h.state().state.options.tool, Tool::Pucker, "typing owns shortcuts while the numeric field is focused");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.get_by_label("Width").type_text("64");
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(1);
    assert_eq!(h.state().state.options.brush.width, 64.);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.key_press_modifiers(Modifiers::SHIFT, Key::R);
    h.run_steps(1);
    assert_eq!(h.state().state.options.tool, Tool::Warp);
    let settings: Options = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.tool, Tool::Warp);
}
#[test]
fn distort41_kittest_selected_text_prompts_conversion_then_native_paths_are_editable() {
    let mut h = harness();
    let result = h.state_mut().s.execute("type.create", serde_json::json!({"x":40,"y":90,"text":"oil","size":36})).unwrap();
    let id = LayerId(result["layer"].as_u64().unwrap());
    h.state_mut().s.select_layer(id).unwrap();
    h.run_steps(1);
    let before = h.state().s.active().unwrap().doc.clone();
    let history = h.state().s.active().unwrap().history.past_len();
    let pos = h.state().rect.min + vec2(80., 80.);
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().state.conversion.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.get_by_label("Convert to curves").click();
    h.run_steps(2);
    let LayerContent::Shape(sh) = &h.state().s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert!(!sh.path.subpaths.is_empty());
    assert_eq!(h.state().s.active().unwrap().history.past_len(), history + 1);
    engine::Capture::begin(&h.state().s, vec![id]).unwrap();
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
}

#[test]
fn distort41_kittest_unselected_live_shape_swept_between_samples_prompts_exact_target() {
    let mut h = harness();
    let id = h.state().s.active().unwrap().active_layer.unwrap();
    h.state_mut()
        .s
        .edit("Live", |doc, _| {
            let LayerContent::Shape(sh) = &mut doc.layer_mut(id).unwrap().content else { panic!() };
            sh.live = Some(photocraft_doc::LiveShape::Rect { rect: [20., 30., 200., 100.], radii: [0.; 4] });
            Ok(())
        })
        .unwrap();
    let st = h.state_mut().s.active_mut().unwrap();
    st.active_layer = None;
    st.selected_layers.clear();
    h.run_steps(1);
    let before = h.state().s.active().unwrap().doc.clone();
    event(&mut h, [5., 80.], Some(true), Modifiers::NONE);
    let pos = h.state().rect.min + vec2(500., 80.);
    h.event(Event::PointerMoved(pos));
    h.run_steps(2);
    assert_eq!(h.state().state.conversion.as_ref().unwrap().layers, vec![id]);
    assert!(!h.state().state.captured());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    h.get_by_label("Convert to curves").click();
    h.run_steps(2);
    let LayerContent::Shape(sh) = &h.state().s.active().unwrap().doc.layer(id).unwrap().content else { panic!() };
    assert!(sh.live.is_none());
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    choose(&mut h, Tool::PuppetWarp);
    assert!(h.state().state.pins.is_empty(), "Puppet Warp requires a vector selection");
    assert_eq!(h.state().s.active().unwrap().doc, before);
}
