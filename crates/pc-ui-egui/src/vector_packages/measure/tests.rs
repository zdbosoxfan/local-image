use super::*;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Layer, LayerContent, SampleType, ShapeLayer, Size};
struct Host {
    doc: Document,
    revision: u64,
    state: State,
    mode: Mode,
    rect: egui::Rect,
    zoom: f64,
    selected: Vec<LayerId>,
    settings: Settings,
    requests: Vec<ScaleEdit>,
    errors: Vec<String>,
    cancel: bool,
}
fn harness() -> Harness<'static, Host> {
    let mut doc = Document::new("Measure UI", Size::new(400, 300), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 300.;
    doc.layers
        .push(Layer::new("Rectangle", LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::rect(20., 20., 100., 50.), ..Default::default() })));
    let host = Host {
        doc,
        revision: 0,
        state: State::default(),
        mode: Mode::Measure,
        rect: egui::Rect::NOTHING,
        zoom: 1.,
        selected: Vec::new(),
        settings: Settings { curves: true, bounds: false, alignment: false, spacing: false, ..Default::default() },
        requests: Vec::new(),
        errors: Vec::new(),
        cancel: false,
    };
    let mut h = Harness::builder().with_size(egui::vec2(1000., 700.)).build_ui_state(
        |ui, h: &mut Host| {
            let scale = DrawingScale::default();
            ui.allocate_ui(egui::vec2(980., 180.), |ui| {
                ui.set_min_height(180.);
                match options(ui, &mut h.state, &h.doc, h.revision, None, scale, str::to_owned) {
                    Ok(Some(r)) => h.requests.push(r),
                    Ok(None) => {}
                    Err(e) => h.errors.push(e.to_string()),
                }
            });
            let (rect, response) = ui.allocate_exact_size(egui::vec2(800., 400.), egui::Sense::click_and_drag());
            h.rect = rect;
            let mapping = Mapping { origin: rect.min, document_origin: [0., 0.], zoom: h.zoom, flip: false, rotation: 0.0 };
            if let Err(e) = h.state.input(
                &response,
                Input {
                    doc: &h.doc,
                    revision: h.revision,
                    selected: &h.selected,
                    mode: h.mode,
                    mapping,
                    view: View { zoom: h.zoom, pixels_per_point: 1. },
                    settings: &h.settings,
                    scale,
                    cancel: h.cancel,
                },
            ) {
                h.errors.push(e.to_string());
            }
            paint(ui.painter(), &h.state, mapping, h.state.unit.unwrap_or(h.doc.design_units), str::to_owned);
        },
        host,
    );
    h.run_steps(3);
    h
}
fn pointer(h: &mut Harness<'_, Host>, p: [f64; 2], button: Option<bool>, mods: Modifiers) {
    let pos = h.state().rect.min + egui::vec2((p[0] * h.state().zoom) as f32, (p[1] * h.state().zoom) as f32);
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
}
#[test]
fn measure44_ui_snapped_drag_persistent_readout_click_units_and_assign_type() {
    let mut h = harness();
    let original = h.state().doc.clone();
    pointer(&mut h, [19., 19.], Some(true), Modifiers::NONE);
    pointer(&mut h, [121., 71.], None, Modifiers::NONE);
    assert!(h.state().state.captured());
    assert_eq!(h.state().state.line.as_ref().unwrap().a, [20., 20.]);
    assert_eq!(h.state().state.line.as_ref().unwrap().b, [120., 70.]);
    pointer(&mut h, [121., 71.], Some(false), Modifiers::NONE);
    assert!(!h.state().state.captured());
    pointer(&mut h, [180., 180.], None, Modifiers::NONE);
    assert_eq!(h.state().state.distance.unwrap().distance, 12500_f64.sqrt());
    h.get_by_label("Millimeters").click();
    h.run_steps(3);
    assert!((h.state().state.distance.unwrap().distance - 12500_f64.sqrt() * 25.4 / 300.).abs() < 1e-10);
    h.get_by_label("Real length").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("Real length").type_text("254");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    h.get_by_label("Assign drawing scale").click();
    h.run_steps(3);
    assert_eq!(h.state().requests.len(), 1);
    let expected = 3000. / 12500_f64.sqrt();
    assert!((h.state().requests[0].scale.factor - expected).abs() < 1e-8);
    assert_eq!(h.state().doc, original);
    assert_eq!(h.state().revision, 0);
    assert!(h.state().errors.is_empty());
}
#[test]
fn measure44_ui_area_hover_modes_selection_sum_and_unit_switch() {
    let mut h = harness();
    h.state_mut().mode = Mode::Area;
    h.run_steps(3);
    pointer(&mut h, [60., 20.], None, Modifiers::NONE);
    let a = h.state().state.area.unwrap();
    assert_eq!(a.area, 5000.);
    assert_eq!(a.perimeter, 300.);
    assert_eq!(a.segment, Some(300.));
    h.get_by_label("Cusped segments").click();
    h.run_steps(3);
    pointer(&mut h, [60., 20.], None, Modifiers::NONE);
    assert_eq!(h.state().state.area.unwrap().segment, Some(100.));
    h.get_by_label("Millimeters").click();
    h.run_steps(3);
    pointer(&mut h, [60., 20.], None, Modifiers::NONE);
    assert!((h.state().state.area.unwrap().area - 5000. * 25.4 * 25.4 / 90_000.).abs() < 1e-10);
    let second = Layer::new("Second", h.state().doc.layers[0].content.clone());
    let id = second.id;
    h.state_mut().doc.layers.push(second);
    h.state_mut().revision += 1;
    h.state_mut().selected = vec![h.state().doc.layers[0].id, id];
    pointer(&mut h, [200., 100.], None, Modifiers::NONE);
    assert!((h.state().state.area.unwrap().area - 10000. * 25.4 * 25.4 / 90_000.).abs() < 1e-10);
    assert_eq!(h.state().state.hovered, None);
    assert!(h.state().errors.is_empty());
}
#[test]
fn measure44_ui_shift_current_release_bypass_and_cancel_restore() {
    let mut h = harness();
    h.state_mut().settings.enabled = false;
    pointer(&mut h, [20., 20.], Some(true), Modifiers::NONE);
    pointer(&mut h, [83., 65.], None, Modifiers::SHIFT);
    let d = h.state().state.distance.unwrap();
    assert!((d.dx - d.dy).abs() < 1e-10);
    pointer(&mut h, [83., 65.], Some(false), Modifiers::CTRL);
    assert_eq!(h.state().state.line.as_ref().unwrap().b, [83., 65.]);
    let old = h.state().state.line.clone();
    pointer(&mut h, [50., 50.], Some(true), Modifiers::NONE);
    pointer(&mut h, [90., 90.], None, Modifiers::NONE);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().state.line, old);
    assert!(!h.state().state.captured());
    pointer(&mut h, [90., 90.], Some(false), Modifiers::NONE);
    assert_eq!(h.state().state.line, old);
}
#[test]
fn measure44_ui_stale_focus_persona_and_document_switch() {
    for reason in 0..5 {
        let mut h = harness();
        pointer(&mut h, [19., 19.], Some(true), Modifiers::NONE);
        pointer(&mut h, [121., 71.], None, Modifiers::NONE);
        match reason {
            0 => h.state_mut().revision += 1,
            1 => h.event(Event::WindowFocused(false)),
            2 => h.state_mut().cancel = true,
            3 => h.state_mut().mode = Mode::Area,
            _ => h.state_mut().doc.id = DocId::fresh(),
        };
        h.step();
        assert!(!h.state().state.captured());
        pointer(&mut h, [121., 71.], Some(false), Modifiers::NONE);
        assert!(h.state().state.completed.is_none());
    }
}
#[test]
fn measure44_ui_frozen_mapping_outside_canvas_release() {
    let mut h = harness();
    h.state_mut().settings.enabled = false;
    pointer(&mut h, [20., 20.], Some(true), Modifiers::NONE);
    h.state_mut().zoom = 2.;
    let pos = h.state().rect.min + egui::vec2(820., 100.);
    h.event(Event::PointerMoved(pos));
    h.step();
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    assert_eq!(h.state().state.line.as_ref().unwrap().b, [820., 100.]);
    assert!(!h.state().state.captured());
}
