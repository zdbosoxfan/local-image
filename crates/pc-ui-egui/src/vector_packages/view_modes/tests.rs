#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::Event;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, ShapeStroke, Size};
use photocraft_engine::Session;
fn fixture() -> Session {
    let mut doc = Document::new("Views", Size::new(40, 40), ColorMode::Rgb, SampleType::U8);
    for (name, x, y) in [("Bottom", 5.5, 5.5), ("Top", 10.5, 10.5)] {
        let mut shape = ShapeLayer {
            path: photocraft_vector::shapes::rect(x, y, 20., 20.),
            fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
            stroke: Some(ShapeStroke { width: 6., ..Default::default() }),
            ..Default::default()
        };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        doc.layers.push(Layer::new(name, LayerContent::Shape(shape)));
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn scope(doc: &Document) -> Scope {
    Scope { document: doc.id, target: None }
}
struct Host {
    session: Session,
    state: State,
    vector: bool,
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(360., 260.)).build_ui_state(
        |ui, h| {
            let doc = &h.session.active().unwrap().doc;
            h.state.synchronize(Some(doc));
            shortcuts(ui.ctx(), &mut h.state, h.vector);
            let eye = ui.button("Layer eye");
            layer_eye(ui, &eye, &mut h.state, doc, doc.layers[1].id).unwrap();
            ui.menu_button("View", |ui| menu(ui, &mut h.state, str::to_owned));
        },
        Host { session: fixture(), state: State::default(), vector: true },
    )
}
fn key(h: &mut Harness<'_, Host>, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::Key { key: Key::Y, physical_key: Some(Key::Y), pressed: true, repeat: false, modifiers });
    h.step();
    h.event(Event::Key { key: Key::Y, physical_key: Some(Key::Y), pressed: false, repeat: false, modifiers });
    h.step();
}
#[test]
fn view37_kittest_ctrl_y_alt_ctrl_y_and_pixel_persona_scoping() {
    let mut h = harness();
    let doc = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    key(&mut h, ctrl);
    assert_eq!(h.state().state.mode, Mode::Outline);
    key(&mut h, ctrl);
    assert_eq!(h.state().state.mode, Mode::Vector);
    key(&mut h, Modifiers { alt: true, ..ctrl });
    assert_eq!(h.state().state.mode, Mode::PixelPreview);
    key(&mut h, Modifiers { alt: true, ..ctrl });
    assert_eq!(h.state().state.mode, Mode::Vector);
    h.state_mut().vector = false;
    key(&mut h, ctrl);
    assert_eq!(h.state().state.mode, Mode::Vector);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn view37_kittest_eye_ctrl_click_and_xray_menu_are_ui_state() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let id = before.layers[1].id;
    h.event(Event::ModifiersChanged(Modifiers { ctrl: true, command: true, ..Modifiers::NONE }));
    h.get_by_label("Layer eye").click();
    h.run();
    assert!(h.state().state.layer_outlines.contains(&id));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("X-ray").click();
    h.run();
    assert_eq!(h.state().state.mode, Mode::Xray);
}
#[test]
fn view37_outline_frame_draws_only_hairlines_and_hit_ignores_covering_fill() {
    let s = fixture();
    let doc = &s.active().unwrap().doc;
    let mut state = State::default();
    state.set_mode(Mode::Outline);
    let frame = prepare(doc, &state, scope(doc), Affine::IDENTITY, doc.bounds(), &NativeGeometry).unwrap();
    assert!(frame.base.is_none());
    assert_eq!(frame.hit(doc, scope(doc), Point::new(25.5, 18.), 0.2).unwrap(), Some(doc.layers[0].id));
    let pixels = frame.overlay_buffer(doc.bounds()).unwrap();
    assert_eq!(pixels.get(18, 18)[3], 0.);
    assert_eq!(pixels.get(25, 18)[3], 1.);
    let high = prepare(doc, &state, scope(doc), Affine::translate(0.5, 0.).mul(&Affine::scale(8.)), Rect::new(0, 0, 320, 320), &NativeGeometry).unwrap();
    let pixels = high.overlay_buffer(Rect::new(0, 0, 320, 320)).unwrap();
    for x in 190..215 {
        assert_eq!(pixels.get(x, 144)[3], if x == 204 { 1. } else { 0. });
    }
}
#[test]
fn view37_xray_fills_are_twenty_percent_without_original_stroke_or_effects() {
    let s = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layers.truncate(1);
    let mut state = State::default();
    state.set_mode(Mode::Xray);
    let frame = prepare(&doc, &state, scope(&doc), Affine::IDENTITY, doc.bounds(), &NativeGeometry).unwrap();
    let base = frame.base.unwrap();
    let pixels = photocraft_compose::flatten(&base);
    assert!((pixels.get(15, 15)[3] - 0.20).abs() <= 1. / 255.);
    assert_eq!(pixels.get(3, 15)[3], 0., "old 6px stroke suppressed");
    assert_eq!(doc.layers[0].opacity, 1.);
    let LayerContent::Shape(shape) = &doc.layers[0].content else {
        panic!("shape");
    };
    assert_eq!(shape.stroke.as_ref().unwrap().width, 6.);
}
#[test]
fn view37_pixel_preview_matches_one_x_cpu_cache_at_eight_x_and_hidpi() {
    let s = fixture();
    let doc = &s.active().unwrap().doc;
    let mut state = State::default();
    state.set_mode(Mode::PixelPreview);
    let frame = prepare(doc, &state, scope(doc), Affine::scale(8.), Rect::new(0, 0, 320, 320), &NativeGeometry).unwrap();
    assert!(frame.pixel_preview);
    let expected = photocraft_compose::flatten(doc);
    let shown = frame.pixel_buffer(doc, Rect::new(0, 0, 320, 320), Affine::scale(8.)).unwrap();
    for y in 0..320 {
        for x in 0..320 {
            assert_eq!(shown.get(x, y), expected.get(x / 8, y / 8));
        }
    }
}
#[test]
fn view37_per_group_outline_hides_descendant_fills_and_document_switch_resets() {
    let s = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let layers = std::mem::take(&mut doc.layers);
    doc.layers = vec![Layer::group("Outlined group", layers)];
    let mut state = State::default();
    state.toggle_layer(&doc, doc.layers[0].id).unwrap();
    let frame = prepare(&doc, &state, scope(&doc), Affine::IDENTITY, doc.bounds(), &NativeGeometry).unwrap();
    assert_eq!(photocraft_compose::flatten(&frame.base.unwrap()).get(18, 18)[3], 0.);
    assert_eq!(frame.outlines.len(), 3);
    let other = Document::new("Other", Size::new(10, 10), ColorMode::Rgb, SampleType::U8);
    assert!(state.synchronize(Some(&other)));
    assert!(state.layer_outlines.is_empty());
}
#[test]
fn view37_frame_cache_invalidation_includes_preview_source_view_and_dependencies() {
    use std::sync::Arc;
    let s = fixture();
    let doc = s.active().unwrap().doc.clone();
    let mut state = State::default();
    state.set_mode(Mode::Outline);
    let mut cache = FrameCache::default();
    let first = cache.prepare(&doc, 1, 0, &state, scope(&doc), Affine::IDENTITY, doc.bounds(), &NativeGeometry).unwrap();
    let same = cache.prepare(&doc, 1, 0, &state, scope(&doc), Affine::IDENTITY, doc.bounds(), &NativeGeometry).unwrap();
    assert!(Arc::ptr_eq(&first, &same));
    for (source, revision, deps, transform) in [
        (Arc::new((*doc).clone()), 1, 0, Affine::IDENTITY),
        (doc.clone(), 2, 0, Affine::IDENTITY),
        (doc.clone(), 2, 1, Affine::IDENTITY),
        (doc.clone(), 2, 1, Affine::scale(2.)),
    ] {
        let next = cache.prepare(&source, revision, deps, &state, scope(&source), transform, doc.bounds(), &NativeGeometry).unwrap();
        assert!(!Arc::ptr_eq(&first, &next));
    }
}
#[test]
fn view37_text_uses_pc_text_glyph_paths_and_raster_uses_cross_box() {
    let s = fixture();
    let doc = &s.active().unwrap().doc;
    let text =
        Layer::new("Text", LayerContent::Text(photocraft_doc::TextLayer { text: "O".into(), font_family: "Inter".into(), size_pt: 18., ..Default::default() }));
    let paths = NativeGeometry.paths(doc, &text).unwrap();
    assert!(!paths.is_empty());
    assert!(paths.iter().any(|p| p.subpaths.len() > 1), "O has an outline and counter");
    let raster = Layer::raster("Raster", doc.pixel_format());
    let mut raster = raster;
    if let LayerContent::Raster(surface) = &mut raster.content {
        surface.fill_rect(Rect::new(2, 2, 12, 12), &[1., 0., 0., 1.]);
    }
    let paths = NativeGeometry.paths(doc, &raster).unwrap();
    assert_eq!(paths[0].subpaths.len(), 3);
}
#[test]
fn view37_kittest_eye_plain_click_does_not_toggle_outline() {
    let mut h = harness();
    h.get_by_label("Layer eye").click();
    h.run();
    assert!(h.state().state.layer_outlines.is_empty());
}
