use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, SampleType, ShapeLayer, Size};
struct Host {
    session: Session,
    state: State,
    targets: Vec<LayerId>,
    rect: egui::Rect,
}
fn host() -> Host {
    let mut session = Session::new();
    let mut doc = Document::new("Panel", Size::new(128, 96), ColorMode::Rgb, SampleType::U8);
    let mut sh =
        ShapeLayer { path: photocraft_vector::shapes::rect(1., 2., 12., 10.), fill: Some(Fill::Solid(Color::rgb(1.5, 0.2, 0.1))), ..Default::default() };
    sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
    let layer = Layer::new("Badge", LayerContent::Shape(sh));
    let id = layer.id;
    doc.layers.push(layer);
    session.add_document(doc, None);
    session.select_layer(id).unwrap();
    Host { session, state: State::default(), targets: vec![id], rect: egui::Rect::NOTHING }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(680., 650.)).build_ui_state(
        |ui, h| {
            h.targets = h.session.active().map(|s| s.selected_layers()).unwrap_or_default();
            panel(ui, &mut h.session, &mut h.state, &h.targets, str::to_owned);
            h.rect = ui.allocate_exact_size(egui::vec2(300., 120.), egui::Sense::drag()).0;
            let (press, down, release, pos) =
                ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.interact_pos()));
            if let Some(pos) = pos.filter(|p| h.rect.contains(*p)) {
                let x = f64::from(pos.x - h.rect.min.x);
                let y = f64::from(pos.y - h.rect.min.y);
                let e = if press {
                    Some(Event::Down(x, y))
                } else if release {
                    Some(Event::Up(x, y))
                } else if down {
                    Some(Event::Move(x, y))
                } else {
                    None
                };
                if let Some(e) = e {
                    pointer(&mut h.session, &mut h.state, e, &h.targets);
                }
            }
        },
        host(),
    )
}
fn press(h: &mut Harness<'_, Host>, x: f32, y: f32, button: Option<bool>) {
    let pos = h.state().rect.min + egui::vec2(x, y);
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(egui::Event::PointerButton { pos, pressed, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
    }
    h.step();
}
#[test]
fn symbols_kittest_create_place_master_edit_detach_and_undo() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Symbol name").click();
    h.event(egui::Event::Text("Badge symbol".to_owned()));
    h.run();
    h.get_by_label("Create Symbol").click();
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc.symbols.len(), 1);
    let created = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Edit Master").click();
    h.run();
    h.get_by_label("Apply Master").click();
    h.run();
    assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &created), "opening/applying must preserve existing HDR paint without an edit");
    assert_eq!(h.state().session.journal.len(), 1);
    h.get_by_label("Place Symbol").click();
    h.run();
    press(&mut h, 30., 20., Some(true));
    press(&mut h, 40., 30., None);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, created);
    press(&mut h, 40., 30., Some(false));
    h.run();
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    assert_eq!(h.state().session.journal.len(), 2);
    h.get_by_label("Edit Master").click();
    h.run();
    // Real typing in egui's DragValue text editor, discovered by its neighbouring label.
    let input = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Master red").rect();
    let pos = input.center();
    h.event(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        h.event(egui::Event::PointerButton { pos, pressed: true, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(egui::Event::PointerButton { pos, pressed: false, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.event(egui::Event::Text("0.3".to_owned()));
    h.key_press(egui::Key::Enter);
    h.run();
    h.get_by_label("Apply Master").click();
    h.run();
    assert_eq!(h.state().session.journal.len(), 3);
    let edited = h.state().session.active().unwrap().doc.clone();
    let LayerContent::Shape(sh) = &edited.symbols[0].layers[0].content else { panic!() };
    let Some(Fill::Solid(c)) = &sh.fill else { panic!() };
    assert!((c.c[0] - 0.3).abs() < 1e-5);
    h.get_by_label("Detach Symbols").click();
    h.run();
    assert!(matches!(h.state().session.active().unwrap().doc.layers[1].content, LayerContent::Group(_)));
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, edited);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, created);
}
#[test]
fn symbols_kittest_cancel_and_revision_switch_drop_preview() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Symbol name").click();
    h.event(egui::Event::Text("Badge".to_owned()));
    h.run();
    h.get_by_label("Create Symbol").click();
    h.run();
    let before = h.state().session.active().unwrap().doc.clone();
    let n = h.state().session.journal.len();
    h.get_by_label("Place Symbol").click();
    h.run();
    press(&mut h, 30., 20., Some(true));
    press(&mut h, 50., 30., None);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    press(&mut h, 50., 30., Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), n);
    assert!(h.state().state.preview.is_none());
    h.get_by_label("Place Symbol").click();
    h.run();
    press(&mut h, 30., 20., Some(true));
    let host = h.state_mut();
    let id = host.state.selected.unwrap();
    let mut p = Capture::new(&host.session).unwrap().params();
    p["symbol"] = json!(id.0);
    p["name"] = json!("Changed");
    host.session.execute("symbol.rename", p).unwrap();
    h.run_steps(2);
    press(&mut h, 50., 30., Some(false));
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 1);
    h.get_by_label("Place Symbol").click();
    h.run();
    press(&mut h, 30., 20., Some(true));
    h.state_mut().session.add_document(Document::new("Other", Size::new(16, 16), ColorMode::Rgb, SampleType::U8), None);
    h.run_steps(2);
    press(&mut h, 30., 20., Some(false));
    assert!(h.state().state.selected.is_none());
    assert!(h.state().state.preview.is_none());
    assert!(!h.state().state.armed());
}
#[test]
fn symbols_old_settings_default_and_transient_captures_are_not_saved() {
    let state: State = serde_json::from_str("{}").unwrap();
    assert!(!state.armed());
    assert!(state.preview.is_none());
    let json = serde_json::to_value(state).unwrap();
    assert!(json.get("preview").is_none());
    assert!(json.get("placement").is_none());
    assert!(json.get("draft").is_none());
    assert!(json.get("source_document").is_none());
}
