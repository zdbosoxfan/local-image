//! Registered in the GPU binary. Real full-app pointer/preview/release,
//! repeated GPU cache invalidation, zoom and undo; no window, fallback or adapter skip.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas::GpuCanvas};
use serde_json::json;
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
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
                    i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
                }
            })
            .ok_or_else(|| "RTX 5090 Vulkan required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("symbols full-app adapter: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU required");
        *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(rs.clone());
        let mut doc = Document::new("Full app symbols", Size::new(128, 96), ColorMode::Rgb, SampleType::U8);
        let mut sh =
            ShapeLayer { path: photocraft_vector::shapes::rect(2., 3., 12., 10.), fill: Some(Fill::Solid(Color::rgb(0.8, 0.2, 0.1))), ..Default::default() };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        let layer = Layer::new("Badge", LayerContent::Shape(sh));
        let id = layer.id;
        doc.layers.push(layer);
        app.session.add_document(doc, None);
        app.session.select_layer(id).unwrap();
        app.sync_views();
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 2.;
        app.ui.views[0].center = [64., 48.];
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app
    });
    h.run_steps(3);
    let gpu = observer.lock().unwrap().clone().unwrap();
    let ctx = h.ctx.clone();
    photocraft_ui_egui::menus::invoke(h.state_mut(), &ctx, "window.panel.symbols", json!({})).unwrap();
    h.run_steps(3);
    (h, gpu)
}
fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}
fn pointer(h: &mut H, x: f32, y: f32, press: Option<bool>) {
    let pos = screen(h, x, y);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}
fn present(h: &mut H, gpu: &GpuCanvas, doc: &Document) {
    h.run_steps(2);
    h.render().expect("actual app callbacks");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
    let (_, size, pixels) = gpu.read_texels(doc.id.0).expect("actual app document texture");
    assert_eq!(size, [128, 96]);
    let expected = photocraft_compose::flatten(doc);
    assert_eq!(pixels.len(), expected.px.len());
    let max =
        expected.px.iter().zip(&pixels).flat_map(|(c, g)| (0..4).map(move |i| (if i == 3 { c[3] } else { c[i] * c[3] } - g[i]).abs())).fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "actual app premultiplied diff {max}");
}
#[test]
fn symbols_full_app_rtx_preview_release_master_edit_zoom_detach_undo_and_cancel() {
    let _lock = super::gpu_lock();
    let (mut h, gpu) = harness();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Symbol name").click();
    h.event(egui::Event::Text("Reusable".to_owned()));
    h.run_steps(2);
    h.get_by_label("Create Symbol").click();
    h.run_steps(3);
    let created = h.state().session.active().unwrap().doc.clone();
    assert_eq!(created.symbols.len(), 1);
    present(&mut h, &gpu, &created);
    for (x, y) in [(30., 20.), (60., 20.)] {
        h.get_by_label("Place Symbol").click();
        h.run_steps(2);
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.past_len();
        let journal = h.state().session.journal.len();
        pointer(&mut h, x, y, Some(true));
        pointer(&mut h, x + 3., y + 2., None);
        assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &before));
        assert_eq!(h.state().session.journal.len(), journal);
        let preview = h.state().ui.symbols.preview.clone().expect("detached symbol preview").0;
        present(&mut h, &gpu, &preview);
        pointer(&mut h, x + 3., y + 2., Some(false));
        let released = h.state().session.active().unwrap().doc.clone();
        present(&mut h, &gpu, &released);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert_eq!(h.state().session.journal.len(), journal + 1);
    }
    // Change the actual master with typed input, then render each logical zoom using live app caches.
    h.get_by_label("Edit Master").click();
    h.run_steps(2);
    h.get_by_label("Master colour").click();
    h.run_steps(2);
    h.get_by_label("Hex colour").click();
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("4d331a".to_owned()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let preview = h.state().ui.symbols.preview.clone().expect("master edit preview").0;
    present(&mut h, &gpu, &preview);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    assert_ne!(photocraft_compose::flatten(&before).px, photocraft_compose::flatten(&preview).px);
    h.get_by_label("Apply Master").click();
    h.run_steps(2);
    let edited = h.state().session.active().unwrap().doc.clone();
    assert_eq!(h.state().session.journal.len(), journal + 1);
    for zoom in [2., 4., 8., 1.] {
        h.state_mut().ui.views[0].zoom = zoom;
        present(&mut h, &gpu, &edited);
    }
    h.get_by_label("Detach Symbols").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Detach Symbols").click();
    h.run_steps(2);
    let detached = h.state().session.active().unwrap().doc.clone();
    present(&mut h, &gpu, &detached);
    assert!(matches!(detached.layers.last().unwrap().content, LayerContent::Group(_)));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(2);
    present(&mut h, &gpu, &edited);
    let restored = h.state().session.active().unwrap().doc.clone();
    assert!(restored == edited, "undo restores the edited instances");
    h.get_by_label("Place Symbol").click();
    h.run_steps(2);
    pointer(&mut h, 40., 50., Some(true));
    pointer(&mut h, 50., 60., None);
    assert!(h.state().ui.symbols.preview.is_some());
    h.event(Event::WindowFocused(false));
    h.step();
    pointer(&mut h, 50., 60., Some(false));
    assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &restored), "focus loss cancels without publishing");
    assert!(h.state().ui.symbols.preview.is_none());
    present(&mut h, &gpu, &edited);
}

#[test]
#[ignore = "release-only real-app pointer-frame measurement; run explicitly"]
fn symbols_release_real_app_pointer_frames() {
    if cfg!(debug_assertions) {
        panic!("run this measurement with --release");
    }
    let _lock = super::gpu_lock();
    let (mut h, gpu) = harness();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Symbol name").click();
    h.event(Event::Text("Measured master".to_owned()));
    h.run_steps(2);
    h.get_by_label("Create Symbol").click();
    h.run_steps(2);
    h.get_by_label("Place Symbol").click();
    h.run_steps(2);
    pointer(&mut h, 30., 20., Some(true));
    let mut frames = Vec::new();
    for i in 0..148 {
        let pos = screen(&h, 30. + (i % 40) as f32, 20. + (i % 20) as f32);
        h.event(Event::PointerMoved(pos));
        let start = std::time::Instant::now();
        h.step();
        h.render().expect("real app GPU pointer frame");
        if i >= 20 {
            frames.push(start.elapsed().as_secs_f64() * 1000.);
        }
        assert_eq!(h.state().session.journal.len(), 1);
    }
    frames.sort_by(f64::total_cmp);
    eprintln!(
        "symbols release pointer frames: fixture=128x96, master=12x10 rectangle, instances=2 (one preview), zoom=2, samples={}, warmup=20, p50_ms={:.3}, p95_ms={:.3}, max_ms={:.3}",
        frames.len(),
        frames[64],
        frames[121],
        frames[127]
    );
    pointer(&mut h, 45., 35., Some(false));
    let doc = h.state().session.active().unwrap().doc.clone();
    present(&mut h, &gpu, &doc);
    assert_eq!(h.state().session.journal.len(), 2);
}
