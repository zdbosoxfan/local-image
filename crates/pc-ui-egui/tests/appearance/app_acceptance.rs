//! Full-app acceptance for the appearance router/preview/panel hooks.
//! Requires UiState::vector_appearance and Tool::{VectorFill,Transparency}; it
//! needs those real hooks and no substitute. Widget GPU tests are separate.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
fn setup() -> egui_wgpu::WgpuSetup {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
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
            .ok_or_else(|| "RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("appearance full-app adapter: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    setup
}
type H = Harness<'static, PhotocraftApp>;
fn app() -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1200., 900.)).wgpu_setup(setup()).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU required").clone());
        app
    });
    let mut doc = Document::new("Appearance app", Size::new(128, 128), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(20., 20.), (108., 20.), (108., 108.), (20., 108.)])]),
        fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.6))),
        ..Default::default()
    };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let l = Layer::new("Appearance", LayerContent::Shape(shape));
    let id = l.id;
    doc.layers.push(l);
    h.state_mut().session.add_document(doc, None);
    h.state_mut().session.select_layer(id).unwrap();
    h.state_mut().sync_views();
    let a = h.state_mut();
    a.ui.tool = Tool::VectorFill;
    a.ui.extras.rulers = false;
    a.ui.extras.snap = false;
    a.ui.views[0].zoom = 1.;
    a.ui.views[0].center = [64., 64.];
    a.ui.views[0].fit_pending = false;
    h.run_steps(3);
    assert!(h.state().gpu_active());
    h
}
fn screen(h: &H, x: f32, y: f32) -> egui::Pos2 {
    let a = h.state();
    let v = &a.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(a, a.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}
fn button(h: &mut H, x: f32, y: f32, pressed: bool) {
    h.event(Event::PointerButton { pos: screen(h, x, y), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn motion(h: &mut H, x: f32, y: f32) {
    h.event(Event::PointerMoved(screen(h, x, y)));
    h.step();
}
#[test]
fn appearance_app_gpu_fill_transparency_panel_preview_release_undo_cancel() {
    let _gpu_gate = crate::gpu_lock();
    let mut h = app();
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    button(&mut h, 28., 60., true);
    motion(&mut h, 96., 60.);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let shown = h.state().shared_tools.paint.preview.clone().expect("actual app detached preview");
    let preview = h.render().unwrap();
    button(&mut h, 96., 60., false);
    let release = h.render().unwrap();
    assert_eq!(photocraft_compose::flatten(&shown), photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
    for y in 36..52 {
        for x in 44..88 {
            let p = screen(&h, x as f32 + 0.5, y as f32 + 0.5);
            let a = preview.get_pixel(p.x as u32, p.y as u32).0;
            let b = release.get_pixel(p.x as u32, p.y as u32).0;
            for (a, b) in a.into_iter().zip(b) {
                assert!(a.abs_diff(b) <= 1, "actual presented preview/release diff");
            }
        }
    }
    assert_eq!(h.state().session.journal.len(), journal + 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state_mut().session.redo());
    h.state_mut().ui.tool = Tool::Transparency;
    h.step();
    button(&mut h, 96., 60., true);
    motion(&mut h, 100., 80.);
    assert!(h.state().shared_tools.paint.preview.is_some());
    button(&mut h, 100., 80., false);
    let before = h.state().session.active().unwrap().doc.clone();
    button(&mut h, 28., 60., true);
    motion(&mut h, 80., 90.);
    h.key_press(egui::Key::Escape);
    h.step();
    button(&mut h, 80., 90., false);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.get_by_label("Add stroke").click();
    h.step();
    let first = photocraft_compose::flatten(&h.state().session.active().unwrap().doc);
    h.get_by_label("Move down").click();
    h.step();
    assert_ne!(first, photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
    h.get_by_label("Visible 1").click();
    h.step();
    assert!(h.state_mut().session.undo());
}
#[test]
#[ignore = "release-build pointer-frame timing benchmark on a real GPU; run with --release --ignored"]
fn appearance_app_pointer_frame_release_measurement_rtx5090() {
    let _gpu_gate = crate::gpu_lock();
    if cfg!(debug_assertions) {
        panic!("performance measurement requires --release");
    }
    let mut h = app();
    let mut samples = Vec::new();
    button(&mut h, 28., 60., true);
    for i in 0..120 {
        let t = std::time::Instant::now();
        motion(&mut h, 40. + i as f32 % 60., 60. + i as f32 % 20.);
        let _ = h.render().unwrap();
        samples.push(t.elapsed().as_secs_f64() * 1000.);
    }
    button(&mut h, 96., 60., false);
    samples.sort_by(f64::total_cmp);
    eprintln!("real app pointer+GPU frame median={} ms p95={} ms; release; adapter identity above", samples[60], samples[114]);
}
