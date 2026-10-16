//! Synthetic GPL-3.0-or-later fixtures driven through real egui events.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Modifiers, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
struct Host {
    session: Session,
    state: State,
    rect: egui::Rect,
    error: Option<String>,
    pressure: f32,
    color: [f32; 4],
    selected: bool,
}
fn host(mode: Mode) -> Host {
    let mut session = Session::new();
    let mut doc = Document::new("Area UI", Size::new(200, 200), ColorMode::Rgb, SampleType::F32);
    if mode == Mode::Eraser {
        let mut sh = ShapeLayer { path: photocraft_vector::shapes::rect(20., 20., 160., 160.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
        doc.layers.push(Layer::new("Square", LayerContent::Shape(sh)));
    }
    session.add_document(doc, None);
    Host {
        session,
        state: State { mode, ..Default::default() },
        rect: egui::Rect::NOTHING,
        error: None,
        pressure: 1.,
        color: [0.7, 0.2, 0.1, 1.],
        selected: true,
    }
}
fn harness(mode: Mode) -> Harness<'static, Host> {
    Harness::builder().with_size(vec2(1200., 550.)).with_step_dt(1. / 60.).build_ui_state(
        |ui, h| {
            ui.horizontal_wrapped(|ui| context_bar(ui, &mut h.state));
            let (response, painter) = ui.allocate_painter(vec2(200., 200.), egui::Sense::click_and_drag());
            h.rect = response.rect;
            let xf = ViewXform { rect: h.rect, zoom: 1., center: [100., 100.], flip: false, rotation: 0.0 };
            h.state.frame(&h.session, ui.ctx(), h.selected);
            if h.selected {
                h.error = canvas_input_with_pressure(&mut h.state, &mut h.session, &response, &xf, h.pressure, h.color).err().map(|e| e.to_string());
            }
            if let Some(pos) = ui.ctx().pointer_hover_pos() {
                paint_overlay(&h.state, &painter, &xf, pos);
            }
        },
        host(mode),
    )
}
fn event(h: &mut Harness<'_, Host>, at: [f32; 2], button: Option<bool>, mods: Modifiers) {
    let pos = h.state().rect.min + vec2(at[0], at[1]);
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: mods });
    }
    h.run_steps(1);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
}
#[test]
fn blob40_kittest_real_drag_synthetic_pressure_current_stroke_preview_release_one_undo() {
    let mut h = harness(Mode::Blob);
    h.get_by_label("Keep Selected").click();
    h.run_steps(2);
    assert!(h.state().state.options.keep_selected);
    h.state_mut().state.options.tip.pressure = 1.;
    h.state_mut().pressure = 0.5;
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, [30., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [150., 80.], None, Modifiers::NONE);
    let preview = h.state().state.preview().unwrap();
    let shown = photocraft_compose::flatten(&preview.document);
    assert!(shown.get(90, 80)[3] > 0.99);
    assert_eq!(shown.get(90, 84)[3], 0.);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    // Stroke swatch changes during capture cannot recolour the stroke already begun.
    h.state_mut().color = [0., 0., 1., 1.];
    event(&mut h, [150., 80.], Some(false), Modifiers::NONE);
    let st = h.state().session.active().unwrap();
    assert_eq!(h.state().session.journal.len(), 1);
    let LayerContent::Shape(sh) = &st.doc.layers[0].content else { panic!() };
    assert_eq!(sh.fill, Some(Fill::Solid(Color::rgba(0.7, 0.2, 0.1, 1.))));
    assert!(sh.stroke.is_none());
    assert_eq!(photocraft_compose::flatten(&st.doc), shown);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(!h.state_mut().session.undo());
}
#[test]
fn blob40_kittest_type_size_click_merge_options_and_eraser_alt_mid_drag() {
    let mut h = harness(Mode::Blob);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Size").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Size").type_text("18");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().state.options.tip.size, 18.);
    h.get_by_label("Merge Only with Selection").click();
    h.run_steps(2);
    assert!(h.state().state.options.merge_only_selection);
    let mut h = harness(Mode::Eraser);
    let before = h.state().session.active().unwrap().doc.clone();
    event(&mut h, [80., 10.], Some(true), Modifiers::NONE);
    event(&mut h, [100., 190.], None, Modifiers::ALT);
    let preview = h.state().state.preview().unwrap();
    assert_eq!(preview.document.layers.len(), 2);
    let result = photocraft_compose::flatten(&preview.document);
    assert_eq!(result.get(90, 100)[3], 0.);
    assert!(result.get(75, 100)[3] > 0.99);
    event(&mut h, [100., 190.], Some(false), Modifiers::ALT);
    assert_eq!(h.state().session.journal.len(), 1);
    assert_eq!(photocraft_compose::flatten(&h.state().session.active().unwrap().doc), result);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn blob40_kittest_escape_focus_persona_switch_and_release_outside_capture() {
    for cancel in 0..3 {
        let mut h = harness(Mode::Blob);
        let before = h.state().session.active().unwrap().doc.clone();
        event(&mut h, [20., 80.], Some(true), Modifiers::NONE);
        event(&mut h, [150., 80.], None, Modifiers::NONE);
        assert!(h.state().state.preview().is_some());
        match cancel {
            0 => h.key_press(egui::Key::Escape),
            1 => h.event(egui::Event::WindowFocused(false)),
            _ => h.state_mut().selected = false,
        }
        h.run_steps(2);
        assert!(!h.state().state.active());
        assert!(h.state().state.preview().is_none());
        event(&mut h, [150., 80.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
    }
    let mut h = harness(Mode::Blob);
    event(&mut h, [20., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [230., 80.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(!h.state().state.active());
}
#[test]
fn blob40_ui_old_settings_deserialize_and_transients_not_persisted() {
    let mut h = harness(Mode::Blob);
    let defaults: State = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.options, Options::default());
    event(&mut h, [20., 80.], Some(true), Modifiers::NONE);
    event(&mut h, [150., 80.], None, Modifiers::NONE);
    let json = serde_json::to_value(&h.state().state).unwrap();
    assert!(json.get("gesture").is_none());
    assert!(json.get("preview").is_none());
    let loaded: State = serde_json::from_value(json).unwrap();
    assert!(!loaded.active());
    assert!(loaded.preview().is_none());
}
