//! Offscreen real-app acceptance; no desktop windows. The owner defers execution to 00.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use eframe::{egui_wgpu, wgpu};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

fn harness() -> Harness<'static, crate::PhotocraftApp> {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                if cfg!(target_os = "macos") {
                    info.backend == wgpu::Backend::Metal && info.device_type != wgpu::DeviceType::Cpu
                } else {
                    info.name.contains("RTX 5090") && info.backend == wgpu::Backend::Vulkan && info.device_type == wgpu::DeviceType::DiscreteGpu
                }
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
        let layer = app.session.active().unwrap().doc.layers[0].id;
        app.session.select_layer(layer).unwrap();
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU renderer").clone());
        // Compare image pixels without tool-dependent selection handles.
        app.ui.tool = crate::state::Tool::Hand;
        app.ui.trace_preferences.last_preset = Preset::PixelArt;
        app
    });
    h.run_steps(8);
    assert!(h.state().gpu_active());
    #[cfg(not(target_os = "macos"))]
    assert!(h.state().perf.gpu_info.adapter.contains("RTX 5090"));
    assert_eq!(h.state().perf.gpu_info.backend, if cfg!(target_os = "macos") { "metal" } else { "vulkan" });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(h.state().perf.gpu_info.device_type, "discreteGpu");
    #[cfg(target_os = "macos")]
    assert_ne!(h.state().perf.gpu_info.device_type, "cpu");
    let ctx = h.ctx.clone();
    let (req, _) = crate::control::ControlRequest::new("ui.set", json!({"zoom":1.,"center":[64,48]}));
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(6);
    h
}
fn canvas(h: &mut Harness<'_, crate::PhotocraftApp>) -> Vec<[u8; 4]> {
    h.run_steps(3);
    h.render().expect("actual RTX app readback");
    assert!(h.state().perf.gpu);
    let id = h.state().session.active().unwrap().doc.id;
    let (_, size, pixels) = h.state().read_canvas_texels(id.0).expect("actual canvas texture");
    assert_eq!(size, [128, 96]);
    pixels.into_iter().map(|p| p.map(|c| (c.clamp(0., 1.) * 255.).round() as u8)).collect()
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
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    let before_doc = h.state().session.active().unwrap().doc.clone();
    let before = canvas(&mut h);
    h.get_by_label("Layer").click();
    h.run_steps(2);
    h.get_by_label("Vectorize / Image Trace").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Vectorize / Image Trace").click();
    h.run_steps(3);
    eprintln!("trace dialog after menu: open={} status={}", h.state().trace_dialog.open, h.state().ui.status);
    h.get_by_label("Cutout").click();
    h.run_steps(3);
    if let Some(id) = h.state().trace_dialog.pending_preview() {
        h.state_mut().session.wait_job(id).unwrap();
    }
    h.run_steps(4);
    assert!(h.state().trace_dialog.preview.is_some());
    let split = h.render().unwrap();
    h.get_by_label("Outlines").click();
    h.run_steps(3);
    let outlines = h.render().unwrap();
    assert_ne!(split, outlines, "live modes must draw different pixels");
    let rect = h.state().trace_dialog.preview_rect;
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
    assert!(h.state().trace_dialog.pan.length() > 20. && h.state().trace_dialog.zoom > 1.);
    assert_ne!(h.render().unwrap(), outlines, "zoom/pan must change actual preview pixels");
    assert_eq!(h.state().session.active().unwrap().doc, before_doc);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    parity(&before, &canvas(&mut h));
    h.get_by_label("Layer").click();
    h.run_steps(2);
    h.get_by_label("Vectorize / Image Trace").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Vectorize / Image Trace").click();
    h.run_steps(3);
    eprintln!("trace dialog after menu: open={} status={}", h.state().trace_dialog.open, h.state().ui.status);
    h.get_by_label("Cutout").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").click();
    h.run_steps(3);
    if let Some(id) = h.state().trace_dialog.pending_apply() {
        h.state_mut().session.wait_job(id).unwrap();
    }
    h.run_steps(4);
    assert!(!h.state().trace_dialog.open);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    parity(&before, &canvas(&mut h));
    assert!(h.state_mut().session.undo());
    h.run_steps(4);
    assert_eq!(h.state().session.active().unwrap().doc, before_doc);
    parity(&before, &canvas(&mut h));
}
