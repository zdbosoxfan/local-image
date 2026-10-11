use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
struct Host {
    s: Session,
    state: State,
    rect: egui::Rect,
    enabled: bool,
    error: Option<String>,
}
fn fixture() -> Host {
    let mut s = Session::new();
    let mut doc = Document::new("UI blend", Size::new(240, 180), ColorMode::Rgb, SampleType::U8);
    for (x, r, c) in [(40., 10., Color::BLACK), (160., 16., Color::WHITE)] {
        let mut sh = ShapeLayer { path: photocraft_vector::shapes::ellipse(x - r, 60. - r, 2. * r, 2. * r), fill: Some(Fill::Solid(c)), ..Default::default() };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new("Circle", LayerContent::Shape(sh)));
    }
    let ids = doc.layers.iter().map(|l| l.id.0).collect::<Vec<_>>();
    s.add_document(doc, None);
    s.execute("layer.select", json!({"layer":ids[0]})).unwrap();
    s.execute("layer.select", json!({"layer":ids[1],"mode":"toggle"})).unwrap();
    Host { s, state: State::default(), rect: egui::Rect::NOTHING, enabled: true, error: None }
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(1000., 620.)).build_ui_state(
        |ui, h| {
            if let Err(e) = panel(ui, &mut h.state, &mut h.s, str::to_owned) {
                h.error = Some(e.to_string());
            }
            h.rect = ui.allocate_exact_size(egui::vec2(400., 240.), egui::Sense::click_and_drag()).0;
            if let Err(e) =
                input(&mut h.state, &mut h.s, ui.ctx(), h.rect, |p| Point::new(f64::from(p.x - h.rect.min.x), f64::from(p.y - h.rect.min.y)), 6., h.enabled)
            {
                h.error = Some(e.to_string());
                h.state.cancel();
            }
            overlay(&h.state, &h.s, ui.painter(), |p| h.rect.min + egui::vec2(p.x as f32, p.y as f32));
        },
        fixture(),
    )
}
fn pointer(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: Option<bool>, mods: egui::Modifiers) {
    let p = h.state().rect.min + egui::vec2(x, y);
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerMoved(p));
    if let Some(pressed) = pressed {
        h.event(egui::Event::PointerButton { pos: p, pressed, button: egui::PointerButton::Primary, modifiers: mods });
    }
    h.step();
}
fn create(h: &mut Harness<'_, Host>) -> LayerId {
    h.get_by_label("Specified steps").click();
    h.step();
    h.get_by_label("Create Blend").click();
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    h.state().s.active().unwrap().active_layer.unwrap()
}
fn type_steps(h: &mut Harness<'_, Host>, value: &str) {
    let rect = h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Steps").rect();
    let p = rect.center();
    h.event(egui::Event::PointerMoved(p));
    for _ in 0..2 {
        h.event(egui::Event::PointerButton { pos: p, pressed: true, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(egui::Event::PointerButton { pos: p, pressed: false, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text(value.to_owned()));
    h.step();
    // Clicking outside accepts egui's text editor without using Enter's blend commit.
    h.get_by_label("Align to page").click();
    h.step();
}
#[test]
fn blend18_kittest_create_type_options_spine_drag_expand_and_undo() {
    let mut h = harness();
    let original = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    let id = create(&mut h);
    assert_eq!(h.state().s.journal.len(), journal + 1);
    let created = h.state().s.active().unwrap().doc.clone();
    type_steps(&mut h, "1");
    assert_eq!(h.state().s.active().unwrap().doc, created);
    assert!(h.state().state.preview.is_some());
    h.get_by_label("Apply Blend Options").click();
    h.step();
    assert_eq!(engine::spec(&h.state().s, id).unwrap().spacing, BlendSpacing::Steps(1));
    h.get_by_label("Edit blend spine").click();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    pointer(&mut h, 100., 60., Some(true), egui::Modifiers::NONE);
    pointer(&mut h, 100., 110., None, egui::Modifiers::NONE);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.journal.len(), journal);
    pointer(&mut h, 100., 110., Some(false), egui::Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    assert_eq!(h.state().s.journal.len(), journal + 1);
    let display = engine::evaluator::project_document(&h.state().s.active().unwrap().doc).unwrap();
    assert!((engine::evaluator::center(&display.layer(id).unwrap().children().unwrap()[1]).unwrap().y - 110.).abs() < 0.01);
    let live = h.state().s.active().unwrap().doc.clone();
    h.get_by_label("Expand Blend").click();
    h.step();
    assert!(h.state().s.active().unwrap().doc.layer(id).unwrap().vector.live.is_none());
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, live);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, created);
    assert!(h.state_mut().s.undo());
    assert_eq!(h.state().s.active().unwrap().doc, original);
}
#[test]
fn blend18_kittest_escape_focus_persona_and_stale_revision_cancel_capture() {
    let mut h = harness();
    create(&mut h);
    h.get_by_label("Edit blend spine").click();
    h.step();
    for cancel in 0..4 {
        pointer(&mut h, 100., 60., Some(true), egui::Modifiers::NONE);
        pointer(&mut h, 100., 90., None, egui::Modifiers::NONE);
        assert!(h.state().state.preview.is_some());
        let before = h.state().s.active().unwrap().doc.clone();
        let journal = h.state().s.journal.len();
        match cancel {
            0 => h.key_press(Key::Escape),
            1 => h.event(egui::Event::WindowFocused(false)),
            2 => h.state_mut().enabled = false,
            _ => {
                h.state_mut().s.execute("layer.setProps", json!({"name":"Renamed"})).unwrap();
            }
        }
        h.step();
        assert!(h.state().state.preview.is_none());
        pointer(&mut h, 100., 90., Some(false), egui::Modifiers::NONE);
        if cancel != 3 {
            assert_eq!(h.state().s.active().unwrap().doc, before);
            assert_eq!(h.state().s.journal.len(), journal);
        }
        h.event(egui::Event::WindowFocused(true));
        h.state_mut().enabled = true;
        h.step();
        h.state_mut().state.edit_spine = true;
    }
}
#[test]
fn blend18_kittest_lazy_display_cache_reuses_unchanged_source_and_invalidates_key_edit() {
    let mut h = harness();
    let id = create(&mut h);
    let source = h.state().s.active().unwrap().doc.clone();
    let a = h.state_mut().state.project_display(source.clone()).unwrap();
    let b = h.state_mut().state.project_display(source.clone()).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    let key = source.layer(id).unwrap().children().unwrap()[0].id;
    h.state_mut().s.execute("shape.applyPaths", json!({"edits":[{"layer":key,"path":photocraft_vector::shapes::ellipse(20.,40.,40.,40.)}]})).unwrap();
    let source = h.state().s.active().unwrap().doc.clone();
    let c = h.state_mut().state.project_display(source).unwrap();
    assert!(!Arc::ptr_eq(&a, &c));
    assert_ne!(a.layer(id).unwrap().children().unwrap()[1], c.layer(id).unwrap().children().unwrap()[1]);
}

#[test]
fn blend18_kittest_malformed_loaded_spine_returns_error_without_publish() {
    let mut h = harness();
    let id = create(&mut h);
    h.state_mut()
        .s
        .edit("Malformed imported spine", |doc, _| {
            let Some(photocraft_doc::vector_design::live::LiveGroup::Blend(spec)) = &mut doc.layer_mut(id).unwrap().vector.live else { panic!() };
            spec.spine = Some(Path::default());
            Ok(())
        })
        .unwrap();
    h.step();
    h.get_by_label("Edit blend spine").click();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    pointer(&mut h, 100., 60., Some(true), egui::Modifiers::NONE);
    assert!(h.state().error.is_some());
    pointer(&mut h, 100., 90., Some(false), egui::Modifiers::NONE);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert_eq!(h.state().s.journal.len(), journal);
    assert!(h.state().state.preview.is_none());
}

#[test]
fn blend18_kittest_spine_reads_current_shift_alt_and_release_modifiers() {
    let mut h = harness();
    let id = create(&mut h);
    h.get_by_label("Edit blend spine").click();
    h.step();
    let journal = h.state().s.journal.len();
    pointer(&mut h, 100., 60., Some(true), egui::Modifiers::NONE);
    pointer(&mut h, 120., 100., None, egui::Modifiers::SHIFT);
    let anchor = h.state().state.spec.spine.as_ref().unwrap().subpaths[0].knots[1].anchor;
    assert!((anchor.x - 100.).abs() < 1e-6 && (anchor.y - 100.).abs() < 1e-6);
    pointer(&mut h, 120., 100., None, egui::Modifiers::ALT);
    let knot = h.state().state.spec.spine.as_ref().unwrap().subpaths[0].knots[1];
    assert!((knot.anchor.x - 100.).abs() < 1e-6 && (knot.anchor.y - 60.).abs() < 1e-6);
    // V1 inserts a corner on a straight segment: both handles begin at (100,60).
    assert!((knot.out_ctrl.x - 120.).abs() < 1e-6 && (knot.out_ctrl.y - 100.).abs() < 1e-6);
    pointer(&mut h, 120., 100., Some(false), egui::Modifiers::NONE);
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let anchor = engine::spec(&h.state().s, id).unwrap().spine.unwrap().subpaths[0].knots[1].anchor;
    assert!((anchor.x - 120.).abs() < 1e-6 && (anchor.y - 100.).abs() < 1e-6);
    assert_eq!(h.state().s.journal.len(), journal + 1);
}
