//! Package 08 widget interaction and resvg oracle host. Full-app gate is app_acceptance.rs.
macro_rules! tl {
    ($s:expr) => {
        photocraft_ui_egui::i18n::t($s)
    };
}
#[path = "../../src/vector_packages/appearance.rs"]
#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod appearance_ui;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::vector_design::appearance::{AppearanceItem, FillItem};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, LayerId, Path, SampleType, ShapeLayer, Size, Subpath};
use photocraft_engine::Session;
use photocraft_geom::Point;

struct Host {
    s: Session,
    state: appearance_ui::State,
    ids: Vec<LayerId>,
    rect: egui::Rect,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    enabled: bool,
}
fn host() -> Host {
    let mut s = Session::new();
    let mut doc = Document::new("Appearance", Size::new(128, 128), ColorMode::Rgb, SampleType::U8);
    let mut sh = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20., 20.), (108., 20.), (108., 108.), (20., 108.)])]),
        fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.6))),
        ..Default::default()
    };
    sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
    let l = Layer::new("Shape", LayerContent::Shape(sh));
    let ids = vec![l.id];
    doc.layers.push(l);
    s.add_document(doc, None);
    Host { s, state: Default::default(), ids, rect: egui::Rect::NOTHING, texture: None, error: None, enabled: true }
}
fn draw(ui: &mut egui::Ui, h: &mut Host) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(320.);
            if let Err(e) = appearance_ui::panel(ui, &mut h.state, &mut h.s, &h.ids) {
                h.error = Some(e.to_string());
            }
        });
        ui.vertical(|ui| {
            if ui.button("Fill tool").clicked() {
                h.state.cancel();
                h.state.options.tool = appearance_ui::Tool::Fill;
            }
            if ui.button("Transparency tool").clicked() {
                h.state.cancel();
                h.state.options.tool = appearance_ui::Tool::Transparency;
            }
            let r = appearance_ui::canvas_response(ui, egui::vec2(160., 160.));
            h.rect = r.rect;
            let to_doc = |p: Pos2| Point::new(f64::from(p.x - r.rect.min.x), f64::from(p.y - r.rect.min.y));
            if let Err(e) = appearance_ui::input(&mut h.state, &mut h.s, ui.ctx(), r.rect, to_doc, &h.ids, h.enabled, 5.) {
                h.error = Some(e.to_string());
            }
            let doc = h.state.preview.as_ref().unwrap_or(&h.s.active().unwrap().doc);
            let frame = photocraft_compose::flatten(doc);
            let colors = frame
                .px
                .iter()
                .map(|p| egui::Color32::from_rgba_unmultiplied((p[0] * 255.) as u8, (p[1] * 255.) as u8, (p[2] * 255.) as u8, (p[3] * 255.) as u8))
                .collect();
            let image = egui::ColorImage::new([128, 128], colors);
            if let Some(t) = &mut h.texture {
                t.set(image, egui::TextureOptions::NEAREST);
            } else {
                h.texture = Some(ui.ctx().load_texture("appearance", image, egui::TextureOptions::NEAREST));
            }
            ui.painter().rect_filled(r.rect, 0., egui::Color32::from_gray(90));
            ui.painter().image(
                h.texture.as_ref().unwrap().id(),
                egui::Rect::from_min_size(r.rect.min, egui::vec2(128., 128.)),
                egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
            appearance_ui::paint_handles(&h.state, &h.s, &h.ids, ui.painter(), |p| r.rect.min + egui::vec2(p.x as f32, p.y as f32));
        });
    });
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(760., 1200.)).build_ui_state(draw, host())
}
fn screen(h: &Harness<'_, Host>, x: f32, y: f32) -> Pos2 {
    h.state().rect.min + egui::vec2(x, y)
}
fn button(h: &mut Harness<'_, Host>, x: f32, y: f32, pressed: bool) {
    h.event(Event::PointerButton { pos: screen(h, x, y), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn motion(h: &mut Harness<'_, Host>, x: f32, y: f32) {
    h.event(Event::PointerMoved(screen(h, x, y)));
    h.step();
}
fn pixels(h: &Harness<'_, Host>) -> photocraft_compose::Buffer {
    photocraft_compose::flatten(&h.state().s.active().unwrap().doc)
}
#[test]
fn appearance_ui_add_reorder_hide_duplicate_delete_change_canvas_and_undo() {
    let mut h = harness();
    let before = pixels(&h);
    h.get_by_label("Add stroke").click();
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let stroked = pixels(&h);
    assert_ne!(stroked, before);
    h.get_by_label("Move down").click();
    h.step();
    let below = pixels(&h);
    assert_ne!(below, stroked);
    h.get_by_label("Visible 1").click();
    h.step();
    assert_eq!(pixels(&h), before);
    h.get_by_label("Duplicate item").click();
    h.run_steps(2);
    assert_eq!(h.state().s.active().unwrap().doc.layer(h.state().ids[0]).unwrap().vector.appearance.as_ref().unwrap().items.len(), 3);
    h.get_by_label("Delete item").click();
    h.step();
    assert_eq!(h.state().s.active().unwrap().doc.layer(h.state().ids[0]).unwrap().vector.appearance.as_ref().unwrap().items.len(), 2);
    assert_eq!(h.state().s.journal.len(), 5);
    assert!(h.state_mut().s.undo());
    assert!(h.state_mut().s.redo());
}
#[test]
fn appearance_ui_gradient_and_transparency_actual_drags_preview_release_escape_stale() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    button(&mut h, 28., 60., true);
    motion(&mut h, 96., 60.);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().s.journal.is_empty());
    button(&mut h, 96., 60., false);
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().s.journal.len(), 1);
    h.get_by_label("Transparency tool").click();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    button(&mut h, 96., 60., true);
    motion(&mut h, 104., 80.);
    let preview = h.state().state.preview.clone().unwrap();
    assert!(photocraft_compose::flatten(&preview).px.iter().any(|p| p[3] > 0.1 && p[3] < 0.9));
    assert_eq!(h.state().s.active().unwrap().doc, before);
    button(&mut h, 104., 80., false);
    assert_eq!(photocraft_compose::flatten(&preview), pixels(&h));
    assert_eq!(h.state().s.journal.len(), 2);
    let before = h.state().s.active().unwrap().doc.clone();
    button(&mut h, 28., 60., true);
    motion(&mut h, 80., 90.);
    h.key_press(egui::Key::Escape);
    h.step();
    assert!(h.state().state.preview.is_none());
    button(&mut h, 80., 90., false);
    assert_eq!(h.state().s.active().unwrap().doc, before);
    button(&mut h, 28., 60., true);
    motion(&mut h, 80., 90.);
    h.state_mut()
        .s
        .edit("unrelated", |d, _| {
            d.name = "new revision".into();
            Ok(())
        })
        .unwrap();
    motion(&mut h, 95., 90.);
    assert!(h.state().state.preview.is_none());
}
#[test]
fn appearance_ui_slider_actual_drag_one_history_and_old_options() {
    let mut h = harness();
    let before = h.state().s.active().unwrap().doc.clone();
    let r = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Item opacity").rect();
    let p = egui::pos2(r.min.x + 20., r.center().y);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerMoved(p + egui::vec2(30., 0.)));
    h.step();
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    h.event(Event::PointerButton { pos: p + egui::vec2(30., 0.), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
    assert_eq!(h.state().s.journal.len(), 1);
    assert!(h.state().state.preview.is_none());
    let o: appearance_ui::Options = serde_json::from_str("{}").unwrap();
    assert_eq!(o, Default::default());
}
#[test]
fn appearance_ui_resvg_gradient_and_phased_pattern_oracles_and_limits() {
    let mut h = host();
    let id = h.ids[0];
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="20.5" y1="64" x2="108.5" y2="64"><stop stop-color="black"/><stop offset="1" stop-color="white"/></linearGradient></defs><path d="M20 20H108V108H20Z" fill="url(#g)"/></svg>"##;
    let reference = egui_extras::image::load_svg_bytes(svg.as_bytes(), &Default::default()).unwrap();
    let mut g = photocraft_engine::vector_packages::appearance::Gesture::begin(&h.s, &[id]).unwrap();
    g.update(|a| {
        a.items = vec![AppearanceItem::Fill(FillItem::new(Fill::gradient(
            vec![(0., Color::BLACK), (1., Color::WHITE)],
            0.,
            1.,
            photocraft_doc::GradientStyle::Linear,
            false,
        )))];
        Ok(())
    })
    .unwrap();
    g.commit(&mut h.s).unwrap();
    let ours = photocraft_compose::flatten(&h.s.active().unwrap().doc);
    for y in 22..106 {
        for x in 22..106 {
            let i = y * 128 + x;
            assert!((ours.px[i][0] - f32::from(reference.pixels[i].r()) / 255.).abs() <= 1. / 255.);
        }
    }
    let mut tile = photocraft_doc::Surface::new(photocraft_doc::PixelFormat::RGBA8);
    tile.fill_rect(photocraft_geom::Rect::new(0, 0, 4, 4), &[1., 0., 0., 1.]);
    tile.fill_rect(photocraft_geom::Rect::new(0, 0, 2, 4), &[0., 0., 1., 1.]);
    h.s.edit("pattern fixture", |doc, _| {
        doc.patterns.push(photocraft_doc::Pattern::new("stripe", tile, 4, 4));
        Ok(())
    })
    .unwrap();
    let p = h.s.active().unwrap().doc.patterns[0].clone();
    let mut g = photocraft_engine::vector_packages::appearance::Gesture::begin(&h.s, &[id]).unwrap();
    g.update(|a| {
        a.items = vec![AppearanceItem::Fill(FillItem::new(Fill::Pattern {
            name: p.name.clone(),
            id: p.id.clone(),
            scale: 1.,
            angle: 0.,
            link: false,
            phase: (3., 2.),
        }))];
        Ok(())
    })
    .unwrap();
    g.commit(&mut h.s).unwrap();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><defs><pattern id="p" patternUnits="userSpaceOnUse" x="3" y="2" width="4" height="4"><rect width="4" height="4" fill="red"/><rect width="2" height="4" fill="blue"/></pattern></defs><path d="M20 20H108V108H20Z" fill="url(#p)"/></svg>"##;
    let reference = egui_extras::image::load_svg_bytes(svg.as_bytes(), &Default::default()).unwrap();
    let ours = photocraft_compose::flatten(&h.s.active().unwrap().doc);
    for y in 22..106 {
        for x in 22..106 {
            let i = y * 128 + x;
            let q = reference.pixels[i].to_array();
            for (a, b) in ours.px[i].iter().zip(q) {
                assert!((*a - f32::from(b) / 255.).abs() <= 1. / 255., "pattern {x},{y}");
            }
        }
    }
    let limits = photocraft_vector::appearance::PaintLimitations { transform: Some([1., 0., 0.5, 1., 0., 0.]), non_pad_spread: true };
    assert_eq!(photocraft_vector::appearance::check_svg_paint_limits(limits).len(), 2);
}
#[test]
fn appearance_resvg_skew_and_repeat_spread_require_explicit_fallback_warnings() {
    let gradient = r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="8" x2="16" spreadMethod="repeat"><stop stop-color="black"/><stop offset="1" stop-color="white"/></linearGradient></defs><rect width="32" height="16" fill="url(#g)"/></svg>"##;
    let repeated = egui_extras::image::load_svg_bytes(gradient.as_bytes(), &Default::default()).unwrap();
    let pad = egui_extras::image::load_svg_bytes(gradient.replace("repeat", "pad").as_bytes(), &Default::default()).unwrap();
    assert_ne!(repeated, pad);
    let warnings =
        photocraft_vector::appearance::check_svg_paint_limits(photocraft_vector::appearance::PaintLimitations { non_pad_spread: true, ..Default::default() });
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("spread"));
    let skew = r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><pattern id="p" patternUnits="userSpaceOnUse" width="8" height="8" patternTransform="skewX(30)"><rect width="8" height="8" fill="red"/><rect width="4" height="8" fill="blue"/></pattern></defs><rect width="32" height="32" fill="url(#p)"/></svg>"##;
    let reference = egui_extras::image::load_svg_bytes(skew.as_bytes(), &Default::default()).unwrap();
    let uniform = egui_extras::image::load_svg_bytes(skew.replace("skewX(30)", "rotate(0)").as_bytes(), &Default::default()).unwrap();
    assert_ne!(reference, uniform);
    let warnings = photocraft_vector::appearance::check_svg_paint_limits(photocraft_vector::appearance::PaintLimitations {
        transform: Some([1., 0., 30.0_f64.to_radians().tan(), 1., 0., 0.]),
        ..Default::default()
    });
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("skew"));
}
#[test]
fn appearance_ui_radial_fill_handle_drag_preserves_radial_style() {
    let mut h = harness();
    let id = h.state().ids[0];
    let mut g = photocraft_engine::vector_packages::appearance::Gesture::begin(&h.state().s, &[id]).unwrap();
    g.update(|a| {
        a.items = vec![AppearanceItem::Fill(FillItem::new(Fill::gradient(
            vec![(0., Color::BLACK), (1., Color::WHITE)],
            0.,
            1.,
            photocraft_doc::GradientStyle::Radial,
            false,
        )))];
        Ok(())
    })
    .unwrap();
    g.commit(&mut h.state_mut().s).unwrap();
    h.step();
    button(&mut h, 108., 64., true);
    motion(&mut h, 96., 64.);
    assert!(h.state().state.preview.is_some());
    button(&mut h, 96., 64., false);
    let l = h.state().s.active().unwrap().doc.layer(id).unwrap();
    assert!(matches!(
        &l.vector.appearance.as_ref().unwrap().items[0],
        AppearanceItem::Fill(FillItem { paint: Fill::Gradient { style: photocraft_doc::GradientStyle::Radial, .. }, .. })
    ));
    let frame = pixels(&h);
    assert!(frame.px[64 * 128 + 64][0] < 0.04);
    assert!(frame.px[64 * 128 + 100][0] > 0.98);
}
#[test]
fn appearance_ui_multi_add_preserves_each_targets_other_paints() {
    let mut h = harness();
    let id = h
        .state_mut()
        .s
        .edit("second fixture", |doc, _| {
            let mut sh = ShapeLayer {
                path: Path::new(vec![Subpath::polygon(&[(4., 4.), (12., 4.), (12., 12.), (4., 12.)])]),
                fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
                ..Default::default()
            };
            sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
            let l = Layer::new("Second", LayerContent::Shape(sh));
            let id = l.id;
            doc.layers.push(l);
            Ok(id)
        })
        .unwrap();
    h.state_mut().ids.push(id);
    h.step();
    h.get_by_label("Add stroke").click();
    h.step();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    let second = h.state().s.active().unwrap().doc.layer(id).unwrap().vector.appearance.as_ref().unwrap();
    assert_eq!(second.items[0], AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))));
    assert_eq!(second.items.len(), 2);
}
#[test]
fn appearance_ui_stroke_width_actual_click_type_enter_one_commit() {
    let mut h = harness();
    h.get_by_label("Add stroke").click();
    h.step();
    let before = h.state().s.active().unwrap().doc.clone();
    let journal = h.state().s.journal.len();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Width").click();
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.step();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Width").type_text("12.5");
    h.step();
    assert_eq!(h.state().s.active().unwrap().doc, before);
    assert!(h.state().state.preview.is_some());
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(h.state().s.journal.len(), journal + 1);
    let id = h.state().ids[0];
    let a = h.state().s.active().unwrap().doc.layer(id).unwrap().vector.appearance.as_ref().unwrap();
    assert!(matches!(&a.items[1],AppearanceItem::Stroke(s) if (s.base.width-12.5).abs()<0.001));
}
#[test]
fn appearance_ui_stationary_shift_and_captured_release_outside_canvas() {
    let mut h = harness();
    button(&mut h, 28., 60., true);
    motion(&mut h, 96., 90.);
    let unconstrained = h.state().state.preview.clone().unwrap();
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    h.event(Event::ModifiersChanged(shift));
    h.step();
    assert_ne!(unconstrained, h.state().state.preview.clone().unwrap());
    let pos = screen(&h, 180., 200.);
    assert!(!h.state().rect.contains(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: shift });
    h.step();
    assert_eq!(h.state().s.journal.len(), 1);
    assert!(h.state().state.preview.is_none());
    assert!(h.state().error.is_none());
    let layer = h.state().s.active().unwrap().doc.layer(h.state().ids[0]).unwrap();
    assert!(
        matches!(&layer.vector.appearance.as_ref().unwrap().items[0],AppearanceItem::Fill(f) if matches!(f.paint,Fill::Gradient { angle,.. } if (angle+45.).abs()<0.001))
    );
}
#[test]
fn appearance_ui_focus_pointer_loss_and_tool_disable_cancel_capture() {
    for kind in 0..3 {
        let mut h = harness();
        let before = h.state().s.active().unwrap().doc.clone();
        button(&mut h, 28., 60., true);
        motion(&mut h, 96., 90.);
        assert!(h.state().state.preview.is_some());
        match kind {
            0 => h.event(Event::WindowFocused(false)),
            1 => h.event(Event::PointerGone),
            _ => h.state_mut().enabled = false,
        }
        h.step();
        assert!(h.state().state.preview.is_none(), "cancellation kind {kind}");
        button(&mut h, 96., 90., false);
        assert_eq!(h.state().s.active().unwrap().doc, before);
        assert!(h.state().s.journal.is_empty());
    }
}
#[path = "gpu.rs"]
mod gpu;
