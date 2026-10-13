//! Package 00: #1321 through real pointer events and the app's actual GPU canvas.
//! Synthetic GPL-3.0-or-later inputs; fixture parameters and references are in
//! docs/wip/vector-tasks/integration/00-SOURCES.md. No desktop windows or adapter skips.

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_color::{BlendMode, Color, ColorMode, SampleType};
use photocraft_doc::{
    Document, Effect, Fill, FxCommon, FxPaint, GradientStyle, Layer, LayerContent, LayerId, ShapeLayer, ShapeStroke, StrokeFx, StrokePosition,
};
use photocraft_geom::Size;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas::GpuCanvas, state::Tool};
use serde_json::json;
use std::sync::{Arc, Mutex};

type H = Harness<'static, PhotocraftApp>;

fn fixture(depth: SampleType, style: GradientStyle, align: bool) -> (Document, LayerId) {
    let mut doc = Document::new("00 gradient route", Size::new(320, 240), ColorMode::Rgb, depth);
    let paint = Fill::Gradient {
        stops: vec![(1.0, Color::WHITE), (0.5, Color::rgb(0.9, 0.2, 0.4)), (0.0, Color::BLACK)],
        angle: 31.0,
        scale: 1.3,
        style,
        reverse: true,
        opacity_stops: vec![(1.0, 0.4), (0.0, 0.9)],
        midpoints: vec![0.25, 0.75],
        offset: (0.1, -0.15),
        dither: true,
        align,
    };
    let mut shape = ShapeLayer {
        path: photocraft_vector::shapes::rect(60.25, 50.5, 115.5, 120.0),
        fill: Some(paint.clone()),
        stroke: Some(ShapeStroke { width: 4.5, paint, ..Default::default() }),
        ..Default::default()
    };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let mut layer = Layer::new("Gradient fill and stroke", LayerContent::Shape(shape));
    // A visible effect routes Move through the app compositor's live preview. Independent
    // Move planes have a separate screen-space shader; the document texture is unchanged
    // on that route, so document-texture readback would observe the idle image instead.
    layer.effects.items.push(Effect::Stroke(StrokeFx {
        common: FxCommon::new(BlendMode::Normal, 0.7),
        size: 2.0,
        position: StrokePosition::Outside,
        paint: FxPaint::Color(Color::rgb(0.2, 0.5, 0.9)),
    }));
    let id = layer.id;
    doc.layers.push(layer);
    (doc, id)
}

fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}

fn present(h: &mut H) {
    // Tool input is handled after canvas paint; a second frame presents the new preview.
    h.run_steps(2);
    h.render().expect("offscreen app paint callbacks");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none(), "actual app compositor, no CPU fallback");
}

fn compare(gpu: &GpuCanvas, expected: &Document, label: &str) {
    // Move intentionally translates its cached pixels until release. Retain that preview
    // contract, but mint fresh surface identities so this CPU reference cannot reuse the
    // app's translated split-cache entry (the same oracle as layout_perf's group test).
    let mut fresh = expected.clone();
    for layer in &mut fresh.layers {
        if let LayerContent::Shape(shape) = &mut layer.content
            && let Some(cache) = &mut shape.cache
        {
            let bounds = photocraft_compose::bounds::content_bounds(cache);
            *cache = photocraft_raster::Surface::from_interleaved(cache.format(), bounds, &cache.to_interleaved(bounds));
        }
    }
    let cpu = photocraft_compose::flatten(&fresh);
    let (_, size, pixels) = gpu.read_texels(expected.id.0).expect("actual app canvas texture");
    assert_eq!(size, [320, 240]);
    assert_eq!(pixels.len(), cpu.px.len());
    assert!(pixels.iter().any(|p| p[3] > 0.5), "nonempty gradient canvas");
    let (mut worst, mut at) = (0.0f32, 0usize);
    for (i, (c, g)) in cpu.px.iter().zip(&pixels).enumerate() {
        for k in 0..4 {
            let want = if k == 3 { c[3] } else { c[k] * c[3] };
            assert!(g[k].is_finite());
            if (want - g[k]).abs() > worst {
                worst = (want - g[k]).abs();
                at = i;
            }
        }
    }
    assert!(worst <= 1.0 / 255.0, "{label}: premultiplied max diff {worst} at ({}, {}), CPU {:?}, GPU {:?}", at % 320, at / 320, cpu.px[at], pixels[at]);
}

#[test]
fn vector_00_gradient_app_canvas_preview_release_and_undo() {
    let _gpu_lock = crate::gpu_lock();
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h: H =
        Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(64).wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup()).build_eframe(
            move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let rs = cc.wgpu_render_state.as_ref().expect("real GPU render state required");
                let info = rs.adapter.get_info();
                eprintln!("00 app canvas adapter: {info:?}");
                #[cfg(not(target_os = "macos"))]
                assert!(info.name.contains("RTX 5090"), "owner's RTX 5090 is required");
                assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
                #[cfg(not(target_os = "macos"))]
                assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
                #[cfg(target_os = "macos")]
                assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
                assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&rs.adapter), eframe::wgpu::TextureFormat::Rgba32Float);
                *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                app.set_wgpu(rs.clone());
                app
            },
        );
    let gpu = observer.lock().unwrap().clone().expect("app GPU observer");
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for style in [GradientStyle::Linear, GradientStyle::Reflected, GradientStyle::Radial, GradientStyle::Diamond, GradientStyle::Angle] {
            for align in [false, true] {
                let (doc, id) = fixture(depth, style, align);
                if h.state().session.active().is_some() {
                    h.state_mut().run("file.close", json!({"discard": true})).unwrap();
                }
                h.state_mut().session.open_document(doc, None);
                h.state_mut().sync_views();
                h.state_mut().run("layer.select", json!({"layer": id.0})).unwrap();
                let app = h.state_mut();
                app.ui.tool = Tool::Move;
                app.ui.tool_options.move_auto_select = false;
                app.ui.extras.snap = false;
                app.ui.view.show.smart_guides = false;
                app.session.edit_prefs(|p| {
                    p.interface.show_bounding_box_when_dragging_layer = false;
                    p.interface.contextual_task_bar = false;
                });
                app.ui.tool_options.move_show_transform = false;
                app.ui.views[0].zoom = 1.0;
                app.ui.views[0].center = [160.0, 120.0];
                app.ui.views[0].fit_pending = false;
                present(&mut h);
                assert_eq!(h.state().ui.views[0].zoom, 1.0, "pointer deltas are document pixels");
                let before = h.state().session.active().unwrap().doc.clone();
                let history = h.state().session.active().unwrap().history.past_len();
                let label = format!("{depth:?} {style:?} align={align}");
                compare(&gpu, &before, &label);
                let start = screen(&h, 100.0, 100.0);
                h.event(Event::PointerMoved(start));
                h.step();
                h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                h.step();
                for (dx, dy) in [(7, 4), (21, 12)] {
                    h.event(Event::PointerMoved(start + egui::vec2(dx as f32, dy as f32)));
                    present(&mut h);
                    assert_ne!(h.state().perf.last_refresh, "gpu-move-offset", "compositor preview texture, not independent Move planes");
                    assert!(h.state().perf.spans.contains_key("move preview"), "pointer must drive a live Move preview");
                    let active = h.state().session.active().unwrap();
                    assert!(Arc::ptr_eq(&before, &active.doc), "preview must stay detached");
                    assert_eq!(active.history.past_len(), history, "no preview history");
                    let expected = photocraft_engine::layer_multi_cmds::moved(&before, &[id], dx, dy).unwrap();
                    compare(&gpu, &expected, &format!("{label} preview {dx}/{dy}"));
                }
                h.event(Event::PointerButton {
                    pos: start + egui::vec2(21.0, 12.0),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                present(&mut h);
                let active = h.state().session.active().unwrap();
                assert_eq!(active.history.past_len(), history + 1, "one release undo step");
                let expected = photocraft_engine::layer_multi_cmds::moved(&before, &[id], 21, 12).unwrap();
                let path = |d: &Document| match &d.layer(id).unwrap().content {
                    LayerContent::Shape(s) => s.path.clone(),
                    _ => panic!("shape route retained"),
                };
                assert_eq!(path(&active.doc), path(&expected));
                // Commit re-renders world-dependent paint. Check the released app pixels
                // against freshly rasterized geometry as well as the detached preview.
                let mut rendered = expected;
                let (fmt, bounds) = (rendered.pixel_format(), rendered.bounds());
                if let LayerContent::Shape(shape) = &mut rendered.layer_mut(id).unwrap().content {
                    shape.cache = Some(photocraft_vector::render_shape(shape, fmt, bounds));
                }
                compare(&gpu, &rendered, &format!("{label} release"));
                h.state_mut().run("edit.undo", json!({})).unwrap();
                present(&mut h);
                assert_eq!(path(&h.state().session.active().unwrap().doc), path(&before));
                compare(&gpu, &before, &format!("{label} undo"));

                let restored = h.state().session.active().unwrap().doc.clone();
                let history = h.state().session.active().unwrap().history.past_len();
                h.event(Event::PointerMoved(start));
                h.step();
                h.event(Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                h.step();
                h.event(Event::PointerMoved(start + egui::vec2(7.0, 4.0)));
                present(&mut h);
                compare(&gpu, &photocraft_engine::layer_multi_cmds::moved(&before, &[id], 7, 4).unwrap(), &format!("{label} cancel preview"));
                h.event(Event::WindowFocused(false));
                h.event(Event::PointerButton { pos: start + egui::vec2(7.0, 4.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
                present(&mut h);
                let active = h.state().session.active().unwrap();
                assert!(Arc::ptr_eq(&restored, &active.doc), "focus loss cancels the detached preview");
                assert_eq!(active.history.past_len(), history, "cancel adds no history");
                compare(&gpu, &before, &format!("{label} cancelled"));
                h.event(Event::WindowFocused(true));
                h.step();
            }
        }
    }
}
