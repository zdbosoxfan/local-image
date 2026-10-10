#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Full PhotocraftApp canvas, pointer frames, GPU callbacks and release cache gate.
//! Package 00 supplies the app field/routing listed in integration/05.md. Until persona
//! package 01 integrates, register_ui.py rehearses that wiring under target/ using Pen
//! only as an activation sentinel. This is not final persona acceptance.
use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use eframe::{egui_wgpu, wgpu};
use egui::{Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;
use std::time::Instant;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                info.backend == wgpu::Backend::Vulkan && info.device_type == wgpu::DeviceType::DiscreteGpu && info.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 discrete Vulkan GPU required".to_owned())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("Pencil full-app hardware: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut h = Harness::builder().with_size(vec2(1000., 700.)).with_max_steps(64).wgpu_setup(egui_wgpu::WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU render state").clone());
        app.run("file.new", json!({"width":240,"height":160,"background":"transparent"})).unwrap();
        app.sync_views();
        // The existing floating task bar covers canvas pixels only after selection changes.
        app.session.prefs.edit(|p| p.interface.contextual_task_bar = false);
        app.ui.tool = crate::state::Tool::Pen;
        app.vector_pencil_enabled = true;
        app.vector_pencil.options.close_within = 0.0;
        app
    });
    h.run_steps(8);
    assert!(h.state().gpu_active());
    h
}
use std::sync::Arc;
fn event(h: &mut Harness<'_, PhotocraftApp>, p: [f32; 2], button: Option<bool>, mods: Modifiers) {
    let xf = ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(p[0], p[1]);
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = button {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: mods });
    }
    h.run_steps(2);
}
fn crop(h: &mut Harness<'_, PhotocraftApp>, label: &str) -> Vec<[u8; 4]> {
    // Kittest paints a synthetic cursor into screenshots; exclude it from pixel parity.
    h.event(egui::Event::PointerGone);
    h.run_steps(2);
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(0., 0.);
    let b = xf.to_screen(240., 160.);
    let img = h.render().unwrap();
    if let Ok(dir) = std::env::var("PENCIL_DIAGNOSTIC_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        img.save(std::path::Path::new(&dir).join(format!("{label}.png"))).unwrap();
        eprintln!(
            "{label}: rect={:?} center={:?} zoom={} revision={} enabled={} active={}",
            xf.rect,
            xf.center,
            xf.zoom,
            h.state().session.active().unwrap().revision,
            h.state().vector_pencil_enabled,
            h.state().vector_pencil.active()
        );
    }
    let mut pixels = Vec::new();
    for y in a.y.ceil() as u32..b.y.floor() as u32 {
        for x in a.x.ceil() as u32..b.x.floor() as u32 {
            pixels.push(img.get_pixel(x, y).0);
        }
    }
    pixels
}
#[test]
fn pencil_full_app_gpu_live_pointer_preview_release_cancel_and_frame_measurements() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let empty = crop(&mut h, "empty");
    event(&mut h, [20., 80.], Some(true), Modifiers::NONE);
    let mut frames = Vec::new();
    let mut present = Vec::new();
    let mut readback = Vec::new();
    let mut end_to_end = Vec::new();
    for i in 1..=40 {
        let whole = Instant::now();
        let start = Instant::now();
        let xf = ViewXform::active(h.state()).unwrap();
        let pos = xf.to_screen(20. + i as f32 * 4.5, 80. + (i as f32 * 0.1).sin() * 25.);
        h.event(egui::Event::PointerMoved(pos));
        h.run_steps(1);
        frames.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        h.run_steps(1);
        present.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        h.render().unwrap(); // Real GPU canvas submission/readback, offscreen.
        readback.push(start.elapsed().as_secs_f64() * 1000.0);
        end_to_end.push(whole.elapsed().as_secs_f64() * 1000.0);
    }
    assert!(h.state().vector_pencil.preview().is_some(), "real canvas pointer moves must reach the tool");
    assert_eq!(h.state().session.active().unwrap().doc, before);
    let preview = h.state().vector_pencil.preview().unwrap().document.clone();
    let live = crop(&mut h, "live");
    assert_ne!(live, empty, "GPU canvas must display live geometry");
    assert!(h.state().perf.last_refresh.starts_with("gpu"), "{}", h.state().perf.last_refresh);
    event(&mut h, [200., 80. + 4.0_f32.sin() * 25.], Some(false), Modifiers::NONE);
    let committed = crop(&mut h, "committed");
    assert_eq!(h.state().session.active().unwrap().doc, preview);
    assert_eq!(live.len(), committed.len());
    assert!(live.iter().chain(&committed).all(|p| p[3] == 255), "opaque canvas bytes equal premultiplied RGBA");
    let max = live.iter().zip(&committed).flat_map(|(a, b)| (0..4).map(move |c| a[c].abs_diff(b[c]))).max().unwrap();
    eprintln!("Pencil full-app preview/release premultiplied max diff: {max}/255");
    assert!(max <= 1, "full-app preview/release max diff {max}/255");
    assert_eq!(h.state().session.journal.iter().filter(|(id, _)| id == "path.pencil.commit").count(), 1);
    assert!(h.state_mut().session.undo());
    h.run_steps(4);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    event(&mut h, [30., 30.], Some(true), Modifiers::NONE);
    event(&mut h, [100., 45.], None, Modifiers::NONE);
    h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    event(&mut h, [100., 45.], Some(false), Modifiers::NONE);
    assert!(h.state().vector_pencil.preview().is_none());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(crop(&mut h, "cancelled"), empty);
    if !cfg!(debug_assertions) {
        for (label, mut data) in
            [("pointer frame", frames), ("presentation frame", present), ("GPU readback", readback), ("pointer + presentation + readback", end_to_end)]
        {
            data.sort_by(f64::total_cmp);
            eprintln!("Pencil release full-app {label} (240x160, 40 moves): median={:.3}ms p95={:.3}ms p99={:.3}ms", data[20], data[37], data[39]);
        }
    }
}
