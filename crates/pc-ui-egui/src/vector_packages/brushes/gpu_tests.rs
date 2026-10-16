#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Register this module after the 00/01 app hooks in integration/17.md. The required
//! vector_brush field and Tool::VectorBrush variant are supplied by those shared hooks.
//! Offscreen full-app gate, not an isolated egui painter or a software adapter skip.
use crate::{PhotocraftApp, canvas::ViewXform, state::Tool};
use eframe::{egui_wgpu, wgpu};
use egui::{Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;
use std::sync::Arc;
fn harness() -> Harness<'static, PhotocraftApp> {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") { i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu } else { i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090") }
            })
            .ok_or_else(|| "RTX 5090 discrete Vulkan GPU required".to_owned())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("Vector Brush full-app GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut h = Harness::builder().with_size(vec2(1100., 750.)).with_max_steps(64).wgpu_setup(egui_wgpu::WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU render state").clone());
        app.run("file.new", json!({"width":240,"height":160,"background":"transparent"})).unwrap();
        app.sync_views();
        app.session.prefs.edit(|p| p.interface.contextual_task_bar = false);
        app.ui.tool = Tool::VectorBrush;
        app
    });
    h.run_steps(8);
    assert!(h.state().gpu_active());
    h
}
fn event(h: &mut Harness<'_, PhotocraftApp>, p: [f32; 2], button: Option<bool>) {
    let xf = ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(p[0], p[1]);
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}
fn crop(h: &mut Harness<'_, PhotocraftApp>) -> Vec<[u8; 4]> {
    h.event(egui::Event::PointerGone);
    h.run_steps(2);
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(0., 0.);
    let b = xf.to_screen(240., 160.);
    let image = h.render().unwrap();
    let mut out = Vec::new();
    for y in a.y.ceil() as u32..b.y.floor() as u32 {
        for x in a.x.ceil() as u32..b.x.floor() as u32 {
            out.push(image.get_pixel(x, y).0);
        }
    }
    out
}
#[test]
#[ignore = "package 17 speed rules: coordinator full-app GPU acceptance"]
fn brush17_full_app_rtx_preview_release_cancel_expand_and_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    for (_, brush) in photocraft_engine::vector_packages::brushes::geometry::presets() {
        let mut h = harness();
        h.state_mut().vector_brush.options.brush = brush;
        let before = h.state().session.active().unwrap().doc.clone();
        let empty = crop(&mut h);
        event(&mut h, [20., 60.], Some(true));
        event(&mut h, [70., 80.], None);
        event(&mut h, [170., 60.], None);
        assert!(h.state().vector_brush.preview().is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        let live = crop(&mut h);
        assert_ne!(live, empty, "canvas must present live brush preview");
        assert!(h.state().perf.last_refresh.starts_with("gpu"));
        event(&mut h, [170., 60.], Some(false));
        let committed = crop(&mut h);
        assert_eq!(live.len(), committed.len());
        assert!(live.iter().chain(&committed).all(|p| p[3] == 255));
        let max = live.iter().zip(&committed).flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i]))).max().unwrap();
        assert!(max <= 1, "opaque canvas premultiplied max diff {max}/255");
        assert_eq!(h.state().session.journal.len(), 1);
        let id = h.state().session.active().unwrap().active_layer.unwrap();
        let capture = photocraft_engine::vector_packages::brushes::TargetCapture::begin(&h.state().session, vec![id]).unwrap();
        h.state_mut().session.execute("vector.brush.expand", json!({"capture":capture})).unwrap();
        h.run_steps(4);
        let expanded = crop(&mut h);
        let max = committed.iter().zip(&expanded).flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i]))).max().unwrap();
        assert!(max <= 1);
        assert!(h.state_mut().session.undo());
        assert!(h.state_mut().session.undo());
        h.run_steps(4);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        event(&mut h, [20., 30.], Some(true));
        event(&mut h, [120., 40.], None);
        h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
        event(&mut h, [120., 40.], Some(false));
        assert!(h.state().vector_brush.preview().is_none());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(crop(&mut h), empty);
    }
}
