//! Package 04 full-app gate for package 00 registration (see integration/04.md).
//! Requires UiState::vector_pen and the canvas/context/preview hooks specified there.
//! Never replace this with the isolated Host or mark missing integration as a GPU pass.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use photocraft_engine::vector_packages::pen::Mode;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
use serde_json::json;

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
        eprintln!("pen full-app GPU: {:?}", a.get_info());
        Ok(a.clone())
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
fn click(h: &mut H, x: f32, y: f32) {
    button(h, x, y, true);
    button(h, x, y, false);
}

#[test]
fn pen_app_gpu_live_curved_closed_release_undo_cancel_and_modes() {
    let _gpu_gate = crate::gpu_lock();

    for ppp in [1.0, 2.0] {
        let mut h = Harness::builder().with_size(egui::vec2(1100., 800.)).with_pixels_per_point(ppp).wgpu_setup(setup()).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU required").clone());
            app
        });
        h.state_mut().run("file.new", json!({"width":128,"height":128,"background":"white"})).unwrap();
        h.state_mut().sync_views();
        let app = h.state_mut();
        app.ui.tool = Tool::Pen;
        app.ui.tool_options.vector_mode = "shape".into();
        app.ui.tool_options.shape_fill = true;
        app.ui.tool_options.stroke_width = 0.;
        app.session.tools.foreground = [0., 0., 0., 1.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [64., 64.];
        app.ui.views[0].fit_pending = false;
        h.run_steps(3);
        assert!(h.state().gpu_active());
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.entries().len();
        click(&mut h, 20., 20.);
        click(&mut h, 100., 20.);
        button(&mut h, 80., 90., true);
        h.event(Event::PointerMoved(screen(&h, 50., 90.)));
        h.step();
        assert_eq!(h.state().session.active().unwrap().doc, before);
        let curved = h.render().unwrap();
        button(&mut h, 50., 90., false);
        button(&mut h, 20., 20., true);
        let detached = h.state().shared_tools.pen.preview.clone().expect("actual app pen preview");
        let expected = photocraft_compose::flatten(&detached);
        let preview = h.render().unwrap();
        assert_ne!(curved, preview, "closing segment must affect the actual app frame");
        button(&mut h, 20., 20., false);
        let committed = h.render().unwrap();
        assert_eq!(expected, photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
        // Compare actual GPU-presented interior pixels, away from the tool's controls.
        let mut max = 0u8;
        for y in 36..52 {
            for x in 44..66 {
                let p = screen(&h, x as f32 + 0.5, y as f32 + 0.5) * ppp;
                let a = preview.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                let b = committed.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                for c in 0..4 {
                    max = max.max(a[c].abs_diff(b[c]));
                }
                assert!(b[0] < 8 && b[3] == 255, "preview/commit must contain ink");
            }
        }
        assert!(max <= 1, "premultiplied max diff must be <=1/255 (opaque canvas), got {max}/255");
        assert_eq!(h.state().session.active().unwrap().history.entries().len(), history + 1);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        click(&mut h, 20., 20.);
        click(&mut h, 80., 20.);
        h.key_press(egui::Key::Escape);
        h.step();
        let state = h.state().session.active().unwrap();
        let photocraft_doc::LayerContent::Shape(shape) = &state.doc.layer(state.active_layer.unwrap()).unwrap().content else { panic!("finished Pen shape") };
        assert_eq!(shape.path.subpaths[0].knots.len(), 2);
        assert!(!shape.path.subpaths[0].closed);
        assert!(h.state().shared_tools.pen.preview.is_none());
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.state_mut().ui.tool_options.pen_options.mode = Mode::Line;
        click(&mut h, 20., 20.);
        click(&mut h, 80., 20.);
        assert!(h.state().shared_tools.pen.gesture.is_none());
    }
}
