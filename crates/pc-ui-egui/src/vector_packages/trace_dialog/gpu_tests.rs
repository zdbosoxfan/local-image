//! Offscreen real-app acceptance; no desktop windows. The owner defers execution to 00.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use eframe::{App, egui_wgpu, wgpu};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

/// Calls the production app and package modal in their actual frame order. 00 replaces
/// this small wiring host with the permanent app fields/menu hook after integration.
struct Host {
    app: crate::PhotocraftApp,
    state: State,
    prefs: Preferences,
}
impl App for Host {
    fn logic(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        egui::Area::new(egui::Id::new("trace28-test-launcher")).fixed_pos(egui::pos2(950., 45.)).show(ui.ctx(), |ui| {
            if ui.button("Open Vectorize").clicked() {
                self.state.open(&mut self.app.session, &self.prefs);
            }
        });
        show(ui.ctx(), &mut self.state, &mut self.app.session, &mut self.prefs, &[]);
    }
}
fn harness() -> Harness<'static, Host> {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                info.name.contains("RTX 5090") && info.backend == wgpu::Backend::Vulkan && info.device_type == wgpu::DeviceType::DiscreteGpu
            })
            .cloned()
            .ok_or_else(|| "trace28 gate requires RTX 5090 / Vulkan / discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
    });
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_pixels_per_point(1.).with_max_steps(64).wgpu_setup(setup).build_eframe(|cc| {
        crate::PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = crate::PhotocraftApp::new(super::tests::session(128, 96), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU renderer").clone());
        // Compare image pixels without tool-dependent selection handles.
        app.ui.tool = crate::state::Tool::Hand;
        Host { app, state: State::default(), prefs: Preferences { last_preset: Preset::PixelArt } }
    });
    h.run_steps(8);
    assert!(h.state().app.gpu_active());
    assert!(h.state().app.perf.gpu_info.adapter.contains("RTX 5090"));
    assert_eq!(h.state().app.perf.gpu_info.backend, "vulkan");
    assert_eq!(h.state().app.perf.gpu_info.device_type, "discreteGpu");
    let ctx = h.ctx.clone();
    let (req, _) = crate::control::ControlRequest::new("ui.set", json!({"zoom":1.,"center":[64,48]}));
    crate::control::handle(&mut h.state_mut().app, &ctx, &req);
    h.run_steps(6);
    h
}
fn canvas(h: &mut Harness<'_, Host>) -> Vec<[u8; 4]> {
    h.run_steps(6);
    assert!(h.state().app.perf.gpu, "actual app canvas must use GPU");
    let xf = crate::canvas::ViewXform::active(&h.state().app).unwrap();
    let image = h.render().expect("real GPU readback");
    (0..96)
        .flat_map(|y| (0..128).map(move |x| (x, y)))
        .map(|(x, y)| {
            let pos = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5);
            image.get_pixel(pos.x.floor() as u32, pos.y.floor() as u32).0
        })
        .collect()
}
fn parity(a: &[[u8; 4]], b: &[[u8; 4]]) {
    assert_eq!(a.len(), b.len());
    // Harness render() returns premultiplied bytes: tolerance exactly 1/255.
    let max = a.iter().zip(b).flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i]))).max().unwrap();
    assert!(max <= 1, "premultiplied max difference {max}/255");
}
#[test]
#[ignore = "owner speed rule: real RTX full-app gate runs at integration"]
fn trace28_gpu_live_modal_zoom_drag_cancel_apply_undo_canvas_parity() {
    let mut h = harness();
    let before_doc = h.state().app.session.active().unwrap().doc.clone();
    let before = canvas(&mut h);
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    if let Some(id) = h.state().state.pending_preview() {
        h.state_mut().app.session.wait_job(id).unwrap();
    }
    h.run_steps(4);
    assert!(h.state().state.preview.is_some());
    let split = h.render().unwrap();
    h.get_by_label("Outlines").click();
    h.run_steps(3);
    let outlines = h.render().unwrap();
    assert_ne!(split, outlines, "live modes must draw different pixels");
    let rect = h.state().state.preview_rect;
    let from = rect.center();
    for (at, down) in [(from, Some(true)), (from + egui::vec2(35., 20.), None), (from + egui::vec2(35., 20.), Some(false))] {
        h.event(egui::Event::PointerMoved(at));
        if let Some(pressed) = down {
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(3);
    }
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        phase: egui::TouchPhase::Move,
        delta: egui::vec2(0., 100.),
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(8);
    assert!(h.state().state.pan.length() > 20. && h.state().state.zoom > 1.);
    assert_ne!(h.render().unwrap(), outlines, "zoom/pan must change actual preview pixels");
    assert_eq!(h.state().app.session.active().unwrap().doc, before_doc);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    parity(&before, &canvas(&mut h));
    h.get_by_label("Open Vectorize").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").click();
    h.run_steps(3);
    if let Some(id) = h.state().state.pending_apply() {
        h.state_mut().app.session.wait_job(id).unwrap();
    }
    h.run_steps(4);
    assert!(!h.state().state.open);
    assert_eq!(h.state().app.session.active().unwrap().doc.layers.len(), 2);
    parity(&before, &canvas(&mut h));
    assert!(h.state_mut().app.session.undo());
    h.run_steps(4);
    assert_eq!(h.state().app.session.active().unwrap().doc, before_doc);
    parity(&before, &canvas(&mut h));
}
