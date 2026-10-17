//! Full app, real native caches, public PSD save/reopen, pointer preview/release.
//! Registered in pc-ui-egui/tests/gpu/main.rs. Never opens desktop windows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::vector_design::{appearance::*, text::*};
use photocraft_doc::{
    Affine, BlendMode, Color, ColorMode, Document, Effect, Fill, FxCommon, FxPaint, Layer, LayerContent, LayerId, SampleType, ShapeLayer, ShapeStroke, Size,
    StrokeFx, StrokePosition, SymbolDef, SymbolId, SymbolInstance, TextLayer,
};
use photocraft_engine::vector_packages::{path_text, symbols};
use photocraft_io::{
    ExportOptions, export, import,
    vector_map::fallback::{LossCollector, Projection},
};
use photocraft_ui_egui::{PhotocraftApp, Services, canvas::ViewXform, gpu_canvas::GpuCanvas, state::Tool};
use serde_json::json;
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;

fn fixture() -> Document {
    let mut doc = Document::new("PSD app", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(Layer::new("Background", LayerContent::Fill(Fill::Solid(Color::WHITE))));
    let a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0.15, 0.4, 0.9)))),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 9., paint: Fill::Solid(Color::rgb(0.1, 0.8, 0.3)), ..Default::default() },
                ..Default::default()
            }),
            AppearanceItem::Fill(FillItem { paint: Fill::Solid(Color::rgb(0.8, 0.2, 0.4)), opacity: 0.6, blend: BlendMode::Multiply, visible: true }),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 3., paint: Fill::Solid(Color::rgb(0.9, 0.8, 0.1)), ..Default::default() },
                ..Default::default()
            }),
        ],
    };
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::rect(30.25, 32.5, 64., 60.), ..Default::default() };
    shape.cache = Some(
        photocraft_vector::appearance::CompiledAppearance::new(&shape, Some(&a), &[], doc.bounds()).unwrap().render(doc.pixel_format(), doc.bounds()).unwrap(),
    );
    let mut layer = Layer::new("Multi paint", LayerContent::Shape(shape));
    layer.vector.appearance = Some(a);
    // Effects retain their independent compositing envelope after raster-cache import.
    layer.effects.items.push(Effect::Stroke(StrokeFx {
        common: FxCommon::new(BlendMode::Normal, 1.),
        size: 2.,
        position: StrokePosition::Outside,
        paint: FxPaint::Color(Color::rgb(0.3, 0.2, 0.7)),
    }));
    doc.layers.push(layer);
    let options = PathText { path: photocraft_vector::shapes::ellipse(110., 30., 100., 100.), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline };
    let mut text = TextLayer { text: "ABC".into(), ..Default::default() };
    path_text::refresh(&doc, &mut text, &options).unwrap();
    assert!(!text.cache.as_ref().unwrap().content_bounds().is_empty(), "real glyph raster required");
    let mut layer = Layer::new("Path text", LayerContent::Text(text));
    layer.vector.on_path = Some(options);
    doc.layers.push(layer);
    let mut master =
        ShapeLayer { path: photocraft_vector::shapes::ellipse(0., 0., 32., 32.), fill: Some(Fill::Solid(Color::rgb(0.1, 0.4, 0.8))), ..Default::default() };
    master.cache = Some(photocraft_vector::render_shape(&master, doc.pixel_format(), doc.bounds()));
    doc.symbols.push(SymbolDef { id: SymbolId(1), name: "Circle".into(), layers: vec![Layer::new("Master curve", LayerContent::Shape(master))] });
    let mut instance = SymbolInstance { symbol: SymbolId(1), transform: Affine::translate(140., 160.), cache: None };
    instance.cache = Some(symbols::render_instance(&doc, &instance).unwrap());
    assert!(!instance.cache.as_ref().unwrap().content_bounds().is_empty());
    doc.layers.push(Layer::new("Symbol", LayerContent::Symbol(instance)));
    doc
}

fn harness() -> (H, GpuCanvas) {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("RTX 5090")
                }
            })
            .ok_or_else(|| "real RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("PSD full-app GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let h = Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU render state");
        *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
        app.set_wgpu(rs.clone());
        app
    });
    let gpu = observer.lock().unwrap().clone().unwrap();
    (h, gpu)
}
fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let a = h.state();
    let v = &a.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(a, a.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}
fn present(h: &mut H) {
    h.run_steps(2);
    h.render().expect("offscreen full app frame");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
}
fn configure(h: &mut H, id: LayerId) {
    h.state_mut().sync_views();
    h.state_mut().run("layer.select", json!({"layer":id.0})).unwrap();
    let a = h.state_mut();
    a.ui.tool = Tool::Move;
    a.ui.tool_options.move_auto_select = false;
    a.ui.tool_options.move_show_transform = false;
    a.ui.extras.snap = false;
    a.ui.extras.rulers = false;
    a.ui.view.show.smart_guides = false;
    a.ui.view.show.layer_edges = false;
    a.session.edit_prefs(|p| {
        p.interface.show_bounding_box_when_dragging_layer = false;
        p.interface.contextual_task_bar = false;
    });
    a.ui.views[0].zoom = 1.;
    a.ui.views[0].center = [128., 128.];
    a.ui.views[0].fit_pending = false;
}
fn parity(gpu: &GpuCanvas, doc: &Document) {
    let (_, size, actual) = gpu.read_texels(doc.id.0).expect("actual full app canvas texture");
    assert_eq!(size, [256, 256]);
    let expected = photocraft_compose::flatten(doc);
    assert_eq!(actual.len(), expected.px.len());
    let max =
        actual.iter().zip(&expected.px).flat_map(|(a, b)| (0..4).map(move |k| (a[k] - if k == 3 { b[k] } else { b[k] * b[3] }).abs())).fold(0_f32, f32::max);
    eprintln!("PSD full-app canvas premultiplied max {max:.9}");
    assert!(max <= 1. / 255.);
}

/// Called from the acceptance fixtures once their native renderers exist.
/// The provider must be the common native/display projection, never a format-only evaluator.
/// Fixtures have a background then a movable advanced object, with a visible ordinary effect
/// to force the app's composited preview texture (rather than independent Move planes).
#[allow(dead_code)]
pub fn verify_projected_fixture(doc: Document, expected_losses: &[&str], projection: Option<Arc<dyn Projection>>) {
    let (mut h, gpu) = harness();
    let id = doc.layers[1].id;
    h.state_mut().session.open_document(doc, None);
    configure(&mut h, id);
    present(&mut h);
    let original = h.state().session.active().unwrap().doc.clone();
    let displayed = photocraft_engine::vector_integration::project_document(&original).unwrap();
    parity(&gpu, &displayed);
    let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let record = written.clone();
    h.state_mut().services.pick_save = Some(Box::new(|_| Some("fixture.psd".into())));
    h.state_mut().services.export = Some(Box::new(move |doc, path, _| {
        if let Some(provider) = projection.as_deref() {
            let out = photocraft_io::vector_map::fallback::export(doc, &Default::default(), Some(provider)).map_err(|e| e.to_string())?;
            let mut warnings = LossCollector::default();
            warnings.extend(out.warnings);
            return Ok((out.bytes, warnings.notice().into_iter().collect()));
        }
        let out = export(doc, path, &ExportOptions::default()).map_err(|e| e.to_string())?;
        Ok((out.bytes, out.warnings))
    }));
    h.state_mut().services.write = Some(Box::new(move |_, bytes| {
        record.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }));
    // Actual app menu clicks execute Save As, through the public PSD route.
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label_contains("Save As…").click();
    h.run_steps(3);
    assert_eq!(written.lock().unwrap().len(), 1);
    assert_eq!(h.state().ui.notices.len(), 1);
    // Save results (f0558183) list the path, the encoder warnings and the completion line; the
    // compact row shows the title and warning count, the lines appear under Details / History.
    let notice = h.state().ui.notices[0].clone();
    assert_eq!(notice.warning_count, 1, "{:?}", notice.lines);
    assert_eq!(notice.lines.len(), 3, "{:?}", notice.lines);
    assert_eq!((notice.lines[0].as_str(), notice.lines[2].as_str()), ("fixture.psd", "Saved fixture.psd"));
    let warning = notice.lines[1].clone();
    for &feature in expected_losses {
        assert_eq!(warning.matches(feature).count(), 1);
    }
    h.get_by_label("Details / History").click();
    h.run_steps(2);
    let _ = h.get_by_label(&warning);
    h.get_by_label("Dismiss notice").click();
    h.run_steps(2);
    assert!(h.state().ui.notices.is_empty());
    h.get_by_label("Close window").click();
    h.run_steps(2);
    assert!(!h.state().ui.notice_history_open);
    let bytes = written.lock().unwrap()[0].clone();
    let back = import("fixture.psd", &bytes).unwrap().document;
    let (a, b) = (photocraft_compose::flatten(&displayed), photocraft_compose::flatten(&back));
    let max =
        a.px.iter()
            .zip(&b.px)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0_f32, f32::max);
    eprintln!("PSD native/reopened premultiplied max {max:.9}");
    if max > 1. / 255. {
        for (a, b) in original.layers.iter().zip(&back.layers) {
            eprintln!("PSD app effects {}: {:?} -> {:?}", a.name, a.effects.items, b.effects.items);
        }
    }
    assert!(max <= 1. / 255.);
    h.state_mut().run("file.close", json!({"discard":true})).unwrap();
    let id = back.layers[1].id;
    h.state_mut().session.open_document(back, None);
    configure(&mut h, id);
    present(&mut h);
    let before = h.state().session.active().unwrap().doc.clone();
    parity(&gpu, &before);
    let history = h.state().session.active().unwrap().history.past_len();
    let revision = h.state().session.active().unwrap().revision;
    let down = screen(&h, 60., 60.);
    let end = screen(&h, 72., 68.);
    h.event(Event::PointerMoved(down));
    h.event(Event::PointerButton { pos: down, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::PointerMoved(end));
    present(&mut h);
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    assert!(h.state().perf.spans.contains_key("move preview"));
    assert_ne!(h.state().perf.last_refresh, "gpu-move-offset", "observe the composited preview texture");
    let expected = photocraft_engine::layer_multi_cmds::moved(&before, &[id], 12, 8).unwrap();
    parity(&gpu, &expected);
    h.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    present(&mut h);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    parity(&gpu, &h.state().session.active().unwrap().doc);
    let released = photocraft_compose::flatten(&h.state().session.active().unwrap().doc);
    let expected = photocraft_compose::flatten(&expected);
    let diff = released
        .px
        .iter()
        .zip(&expected.px)
        .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
        .fold(0_f32, f32::max);
    assert!(diff <= 1. / 255., "release must publish the held preview pixels: {diff}");
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label_contains("Save As…").click();
    h.run_steps(3);
    assert_eq!(written.lock().unwrap().len(), 2);
    // The save shows its ordinary result card, without the loss warning again.
    let notices = &h.state().ui.notices;
    assert!(
        notices.iter().all(|n| n.warning_count == 0 && !n.lines.iter().any(|line| line.contains("editability lost"))),
        "losses are reported once when editability is first lost: {notices:?}"
    );
    let moved_back = import("moved.psd", &written.lock().unwrap()[1]).unwrap().document;
    let moved_back = photocraft_compose::flatten(&moved_back);
    let diff = released
        .px
        .iter()
        .zip(&moved_back.px)
        .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
        .fold(0_f32, f32::max);
    eprintln!("PSD moved/re-exported premultiplied max {diff:.9}");
    assert!(diff <= 1. / 255.);
}

#[test]
fn vector_fallback_app_real_gpu_native_reopen_preview_release_warning_click() {
    let _gpu_gate = crate::gpu_lock();
    verify_projected_fixture(
        fixture(),
        &[
            "appearance stacks",
            "multiple fills",
            "multiple strokes",
            "per-paint opacity",
            "per-paint blend modes",
            "text on a path",
            "symbol definitions and instances",
        ],
        None,
    );
}

struct SharedProjection;
impl Projection for SharedProjection {
    fn layer(&self, doc: &Document, layer: &Layer) -> Result<Option<Layer>, String> {
        let mut source = doc.clone();
        source.layers = vec![layer.clone()];
        let projected = photocraft_engine::vector_integration::project_document(&source).map_err(|e| e.to_string())?;
        Ok(projected.layers.into_iter().next())
    }
}

fn advanced_fixture(kind: &str) -> Document {
    use photocraft_doc::vector_design::live::{BlendSpacing, BlendSpec, EnvelopeSpec, LiveGroup, RepeatSpec};
    let mut doc = Document::new(kind, Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(Layer::new("Background", LayerContent::Fill(Fill::Solid(Color::WHITE))));
    let mut shape =
        ShapeLayer { path: photocraft_vector::shapes::rect(32., 40., 64., 48.), fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let mut layer = Layer::new(kind, LayerContent::Shape(shape));
    if matches!(kind, "width profiles" | "vector brushes") {
        let mut stroke =
            StrokeItem { base: ShapeStroke { width: 8., paint: Fill::Solid(Color::rgb(0.8, 0.2, 0.1)), ..Default::default() }, ..Default::default() };
        if kind == "width profiles" {
            stroke.profile = Some(WidthProfile {
                points: vec![WidthPoint { t: 0., left: 2., right: 2. }, WidthPoint { t: 0.5, left: 9., right: 6. }, WidthPoint { t: 1., left: 2., right: 2. }],
            });
        } else {
            use photocraft_engine::vector_packages::brushes::geometry::{Brush, Resource};
            stroke.brush = Some(Resource { brush: Brush::Calligraphic(Default::default()), seed: 7 }.reference().unwrap());
        }
        layer.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Stroke(stroke)] });
        if let LayerContent::Shape(shape) = &mut layer.content {
            shape.cache = Some(photocraft_engine::vector_integration::render_shape(&doc, shape, &layer.vector).unwrap());
        }
    } else {
        let mut children = vec![layer];
        let live = match kind {
            "live blends" => {
                let mut other = children[0].clone();
                other.id = LayerId::fresh();
                if let LayerContent::Shape(shape) = &mut other.content {
                    shape.path = photocraft_vector::shapes::rect(144., 100., 80., 64.);
                    shape.fill = Some(Fill::Solid(Color::rgb(0.8, 0.3, 0.1)));
                    photocraft_engine::vector_cmds::refresh_shape(&doc, shape);
                }
                children.push(other);
                LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(4), ..Default::default() })
            }
            "live repeats" => LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 2, spacing: [8., 8.] }),
            "live envelopes" => LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arch".into(), bend: 30., horizontal: 0., vertical: 0. }),
            _ => panic!("unknown native feature fixture"),
        };
        layer = Layer::group(kind, children);
        layer.vector.live = Some(live);
    }
    layer.effects.items.push(Effect::default_drop_shadow());
    doc.layers.push(layer);
    doc
}

#[test]
fn vector_fallback_native_width_brush_blend_repeat_envelope_full_app_gpu_shared_projection() {
    let _gpu_gate = crate::gpu_lock();
    for feature in ["width profiles", "vector brushes", "live blends", "live repeats", "live envelopes"] {
        eprintln!("PSD shared native projection: {feature}");
        verify_projected_fixture(advanced_fixture(feature), &[feature], Some(Arc::new(SharedProjection)));
    }
}
