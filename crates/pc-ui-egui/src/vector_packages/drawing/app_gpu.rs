//! Full-app acceptance for the aggregate UI/GPU tests.
//! Requires the registered app routing and UiState::vector_drawing.
//! This is deliberately not substituted with an isolated widget pass.
use egui::{Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::vector_design::drawing::{DesignShape, ShapeParameters};
use photocraft_engine::vector_packages::drawing::{
    Capture,
    geometry::{self, HandleKind},
};
use photocraft_ui_egui::{PhotocraftApp, state::Tool};
use serde_json::json;
type H = Harness<'static, PhotocraftApp>;
fn harness() -> H {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
                }
            })
            .ok_or_else(|| (if cfg!(target_os = "macos") { "hardware Metal non-CPU adapter required" } else { "RTX 5090 Vulkan required" }).to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("live-shapes full-app GPU: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    Harness::builder().with_size(egui::vec2(1000., 760.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.clone().expect("real GPU required"));
        assert!(app.gpu_active(), "the full app GPU canvas must be active");
        app.run("file.new", json!({"width":240,"height":240,"background":"transparent"})).unwrap();
        app.sync_views();
        app.ui.tool = Tool::Rectangle;
        app.session.tools.foreground = [0., 0., 0., 1.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app
    })
}
fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    photocraft_ui_egui::canvas::ViewXform::active(h.state()).expect("the document canvas is laid out").to_screen(x, y)
}
fn pointer(h: &mut H, x: f32, y: f32, button: Option<bool>) {
    let pos = screen(h, x, y);
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(egui::Event::PointerButton { pos, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
    }
    h.step();
}
#[test]
fn live_shapes_full_app_real_gpu_handle_preview_release_and_cancel() {
    let _gpu_gate = crate::gpu_lock();

    let mut h = harness();
    h.run_steps(3);
    let app = h.state_mut();
    let c = Capture::new(&app.session).unwrap();
    let d = DesignShape { rect: [40., 40., 120., 120.], angle: 0., parameters: ShapeParameters::Star { points: 5, inner: 0.5, corner: 0. } };
    app.run("shape.design.create", json!({"document":c.document.0,"revision":c.revision,"design":d})).unwrap();
    h.run_steps(3);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    let handle = geometry::handles(&d).unwrap().into_iter().find(|h| h.kind == HandleKind::Inner).unwrap();
    let to = [100. + (handle.position.x - 100.) * 1.5, 100. + (handle.position.y - 100.) * 1.5];
    let initial = h.render().expect("real app render");
    pointer(&mut h, handle.position.x as f32, handle.position.y as f32, Some(true));
    for i in 1..=6 {
        let t = i as f64 / 6.;
        pointer(&mut h, (handle.position.x + (to[0] - handle.position.x) * t) as f32, (handle.position.y + (to[1] - handle.position.y) * t) as f32, None);
    }
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    assert!(h.state().shared_tools.drawing.preview_layer.is_some());
    let preview = h.render().expect("GPU preview");
    assert_ne!(initial.as_raw(), preview.as_raw(), "mid-drag pixels must change");
    let x = (100. + (handle.position.x - 100.) * 1.25).round() as u32;
    let y = (100. + (handle.position.y - 100.) * 1.25).round() as u32;
    let (_, size, pixels) = h.state().read_canvas_texels(before.id.0).expect("actual app float canvas texture");
    let pixel = pixels[(y * size[0] + x) as usize];
    assert!(pixel[..3].iter().all(|c| *c < 0.1) && pixel[3] > 0.5, "filled live GPU preview must cover newly included star area: {pixel:?}");
    h.key_press(egui::Key::Escape);
    h.step();
    pointer(&mut h, to[0] as f32, to[1] as f32, Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    pointer(&mut h, handle.position.x as f32, handle.position.y as f32, Some(true));
    pointer(&mut h, to[0] as f32, to[1] as f32, None);
    pointer(&mut h, to[0] as f32, to[1] as f32, Some(false));
    h.render().expect("release GPU frame");
    assert_eq!(h.state().session.journal.len(), journal + 1);
    let l = h.state().session.active().unwrap().doc.layers.last().unwrap();
    assert!(matches!(l.vector.live_shape.as_ref().unwrap().parameters,ShapeParameters::Star{inner,..}if(inner-0.75).abs()<1e-5), "{:?}", l.vector.live_shape);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
