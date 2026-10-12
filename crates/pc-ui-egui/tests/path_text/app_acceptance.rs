//! Full PhotocraftApp offscreen GPU acceptance. Requires package 00 routing hooks.
//! Registration recipe and actual execution evidence live in integration/12.md.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use photocraft_doc::{Layer, LayerContent, ShapeLayer, text::TextShape};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
use serde_json::json;
fn setup() -> egui_wgpu::WgpuSetup {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("path text app adapter: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    setup
}
type H = Harness<'static, PhotocraftApp>;
fn screen(h: &H, x: f32, y: f32) -> egui::Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}
fn button(h: &mut H, x: f32, y: f32, pressed: bool) {
    h.event(Event::PointerButton { pos: screen(h, x, y), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn key(h: &mut H, key: Key) {
    h.event(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.step();
}
fn harness(zoom: f32, ppp: f32) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_pixels_per_point(ppp).wgpu_setup(setup()).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU required").clone());
        app
    });
    h.state_mut().run("file.new", json!({"width":256,"height":256,"background":"white"})).unwrap();
    h.state_mut().sync_views();
    h.state_mut()
        .session
        .edit("Fixture", |doc, active| {
            let mut shape = ShapeLayer {
                path: photocraft_vector::shapes::ellipse(28., 28., 200., 200.),
                stroke: Some(photocraft_doc::ShapeStroke {
                    width: 1.,
                    paint: photocraft_doc::Fill::Solid(photocraft_doc::Color::rgb(0.65, 0.65, 0.65)),
                    ..Default::default()
                }),
                ..Default::default()
            };
            shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
            *active = Some(doc.insert_above(None, Layer::new("Curve", LayerContent::Shape(shape))));
            Ok(())
        })
        .unwrap();
    let app = h.state_mut();
    app.ui.tool = Tool::Type;
    app.ui.tool_options.type_font = "Inter".into();
    app.ui.tool_options.type_size = 20.;
    app.session.tools.foreground = [0., 0., 0., 1.];
    app.ui.extras.rulers = false;
    app.ui.extras.snap = false;
    app.ui.views[0].zoom = zoom;
    app.ui.views[0].center = [228., 148.];
    app.ui.views[0].fit_pending = false;
    h.run_steps(3);
    assert!(h.state().gpu_active());
    h
}
#[test]
fn path_text_app_gpu_actual_curve_typing_zoomed_preview_release_undo_cancel() {
    let _gpu_gate = crate::gpu_lock();

    for (zoom, ppp) in [(1., 1.), (2., 2.), (4., 1.), (8., 1.)] {
        let mut h = harness(zoom, ppp);
        let before = h.state().session.active().unwrap().doc.clone();
        let journal = h.state().session.journal.len();
        button(&mut h, 228., 128., true);
        button(&mut h, 228., 128., false);
        h.event(Event::Text("ABC".into()));
        h.step();
        h.run_steps(2);
        let detached = h.state().shared_tools.path_text.preview.clone().expect("actual app path preview");
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.journal.len(), journal);
        let caret = h.state().shared_tools.path_text.caret_segment().map(|c| c.map(|p| h.state().shared_tools.path_text.transform.apply(p)));
        let expected = photocraft_compose::flatten(&detached);
        let preview = h.render().unwrap();
        key(&mut h, Key::Enter);
        h.run_steps(2);
        let committed = h.render().unwrap();
        assert_eq!(expected, photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
        let st = h.state().session.active().unwrap();
        let id = st.active_layer.unwrap();
        let l = st.doc.layer(id).unwrap();
        let LayerContent::Text(t) = &l.content else { panic!("type") };
        assert_eq!(t.text, "ABC");
        assert_eq!(t.shape, TextShape::Point);
        assert!(l.vector.on_path.is_some());
        let mut max = 0;
        let mut samples = 0;
        let mut ink = 0;
        for y in 108..169 {
            for x in 207..239 {
                let p = screen(&h, x as f32 + 0.5, y as f32 + 0.5) * ppp;
                if p.x < 0. || p.y < 0. || p.x >= preview.width() as f32 || p.y >= preview.height() as f32 {
                    continue;
                }
                // Avoid the bracket handles. Preview and committed glyph edges must agree.
                if (x - 228_i32).abs() < 6 && (y - 128_i32).abs() < 6 {
                    continue;
                }
                // The transient caret is an interaction overlay, not text ink.
                if let Some([a, b]) = caret {
                    let point = photocraft_geom::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0., 1.);
                    if (point.x - a.x - t * dx).hypot(point.y - a.y - t * dy) < 2.5 / f64::from(zoom) {
                        continue;
                    }
                }
                let a = preview.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                let b = committed.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                for c in 0..4 {
                    max = max.max(a[c].abs_diff(b[c]));
                }
                samples += 1;
                if b[0] < 80 && b[1] < 80 && b[2] < 80 {
                    ink += 1;
                }
            }
        }
        assert!(samples > 0 && ink > 5, "live GPU frame must contain text");
        assert!(max <= 1, "zoom={zoom} ppp={ppp} preview/release max diff={max}");
        eprintln!("path text app zoom={zoom},ppp={ppp},samples={samples},ink={ink},maxdiff={max}");
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.run_steps(2);
        button(&mut h, 228., 128., true);
        button(&mut h, 228., 128., false);
        h.event(Event::Text("cancel".into()));
        h.step();
        key(&mut h, Key::Escape);
        h.run_steps(2);
        assert!(h.state().shared_tools.path_text.preview.is_none());
        let state = h.state().session.active().unwrap();
        let LayerContent::Text(text) = &state.doc.layer(state.active_layer.unwrap()).unwrap().content else { panic!("finished path text") };
        assert_eq!(text.text, "cancel");
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}

#[test]
#[ignore = "release real-app pointer-frame measurement"]
// Runtime guard: the ignored benchmark must compile in debug.
#[allow(clippy::assertions_on_constants)]
fn path_text_app_gpu_release_pointer_frame_measurement() {
    let _gpu_gate = crate::gpu_lock();
    assert!(!cfg!(debug_assertions), "run this gate with --release");

    let mut h = harness(4., 1.);
    button(&mut h, 228., 128., true);
    button(&mut h, 228., 128., false);
    h.event(Event::Text("ABC office".into()));
    h.step();
    key(&mut h, Key::Enter);
    h.run_steps(2);
    button(&mut h, 228., 128., true);
    let mut times = Vec::new();
    for frame in 0..80 {
        // The first move crosses egui's native drag threshold (8 screen px).
        let theta = 0.02 + frame as f32 * 0.002;
        let p = screen(&h, 128. + 100. * theta.cos(), 128. + 100. * theta.sin());
        let start = std::time::Instant::now();
        h.event(Event::PointerMoved(p));
        h.step();
        let pixels = h.render().unwrap();
        std::hint::black_box(pixels);
        times.push(start.elapsed().as_secs_f64() * 1000.);
        assert!(h.state().shared_tools.path_text.preview.is_some(), "frame={frame}: {}", h.state().ui.status);
    }
    let start = std::time::Instant::now();
    let theta = 0.02 + 79. * 0.002;
    button(&mut h, 128. + 100. * f32::cos(theta), 128. + 100. * f32::sin(theta), false);
    h.run_steps(2);
    std::hint::black_box(h.render().unwrap());
    let release = start.elapsed().as_secs_f64() * 1000.;
    let first = times[0];
    times.sort_by(f64::total_cmp);
    eprintln!(
        "path text real-app pointer+GPU readback document256x256 viewport1280x900 zoom4: median={:.3}ms p95={:.3}ms p99={:.3}ms first={first:.3}ms release={release:.3}ms frames=80",
        (times[39] + times[40]) * 0.5,
        times[75],
        times[79]
    );
}
