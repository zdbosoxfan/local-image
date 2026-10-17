//! Real offscreen full-app gates. Explicitly opt in; an absent/wrong adapter or absent V1
//! fails, never skips successfully. No desktop window and no external fixture is used.
use std::sync::Arc;

use eframe::{egui_wgpu, wgpu};
use egui_kittest::{Harness, kittest::Queryable};

use super::{
    tests::{pointer, two_shapes},
    *,
};

fn harness() -> Harness<'static, PhotocraftApp> {
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
            .ok_or_else(|| "persona gate requires RTX 5090 / Vulkan / discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
    });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).with_max_steps(64).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width":256,"height":192,"background":"white"})).unwrap();
        let mut app = PhotocraftApp::new(s, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU renderer");
        app.set_wgpu(rs.clone());
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
    eprintln!("persona GPU identity: {:?}", h.state().perf.gpu_info);
    h
}

/// Sample document pixels through the actual app view transform, not a library composite.
fn canvas(h: &mut Harness<'_, PhotocraftApp>) -> Vec<[u8; 4]> {
    h.run_steps(6);
    h.render().expect("real GPU readback");
    assert!(h.state().perf.gpu, "canvas must stay on GPU");
    let id = h.state().session.active().unwrap().doc.id;
    let (_, size, pixels) = h.state().read_canvas_texels(id.0).expect("actual canvas texture");
    assert_eq!(size, [256, 192]);
    pixels.into_iter().map(|p| p.map(|c| (c.clamp(0., 1.) * 255.).round() as u8)).collect()
}

fn parity(a: &[[u8; 4]], b: &[[u8; 4]]) {
    assert_eq!(a.len(), b.len());
    let mut max = 0.0f32;
    for (a, b) in a.iter().zip(b) {
        for i in 0..4 {
            // egui_kittest render() already returns premultiplied RGBA.
            let premul = |p: &[u8; 4]| f32::from(p[i]) / 255.0;
            max = max.max((premul(a) - premul(b)).abs());
        }
    }
    assert!(max <= 1.0 / 255.0 + f32::EPSILON, "premultiplied canvas max diff {max}");
}

#[test]
fn persona_gpu_switch_pen_preview_cancel_and_release_leave_canvas_unchanged() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    h.state_mut().run("shape.create", json!({"kind":"ellipse","rect":[30,30,90,70],"fill":"#c02030"})).unwrap();
    h.run_steps(6);
    let ctx = h.ctx.clone();
    let (req, _) = crate::control::ControlRequest::new("ui.set", json!({"zoom":1.0,"center":[128,96]}));
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(3);
    let before = canvas(&mut h);
    let before_overlay = h.render().unwrap();
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    parity(&before, &canvas(&mut h));
    h.get_by_label("Pen Tool").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Pen);
    let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
    // The selected ellipse's contextual task bar sits below y=100. Start in empty
    // canvas above that bar, so the real press reaches Pen rather than its UI widgets.
    let from = xf.to_screen(160.0, 30.0);
    assert!(h.state().last_canvas_rect.contains(from), "Pen start must be in the actual canvas: {from:?}");
    assert!(h.state().ui.dialogs.is_empty());
    h.hover_at(from);
    h.run_steps(3);
    pointer(&mut h, from, Some(true));
    pointer(&mut h, from + egui::vec2(20.0, 12.0), None);
    assert!(
        h.state().shared_tools.pen.gesture.is_some(),
        "held Pen drag must start: {:?}; from={from:?}; rect={:?}; pointer={:?}; tool={:?}; status={}",
        h.state().shared_tools.pen.gesture.as_ref().map(|g| g.path()),
        h.state().last_canvas_rect,
        h.ctx.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos())),
        h.state().ui.tool,
        h.state().ui.status
    );
    let mut pointer_frames = Vec::new();
    for i in 0..24 {
        let start = std::time::Instant::now();
        h.event(egui::Event::PointerMoved(from + egui::vec2(20.0 + i as f32 * 0.2, 12.0)));
        h.run_steps(1);
        h.render().expect("pointer frame GPU completion/readback");
        assert!(h.state().shared_tools.pen.gesture.is_some(), "Pen drag must remain held on pointer frame {i}");
        pointer_frames.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    pointer_frames.sort_by(f64::total_cmp);
    eprintln!(
        "persona real-app pointer frames (UI+GPU+readback, 24 samples, debug_assertions={}): median={:.3}ms p95={:.3}ms p99={:.3}ms",
        cfg!(debug_assertions),
        pointer_frames[12],
        pointer_frames[22],
        pointer_frames[23]
    );
    assert!(h.state().shared_tools.pen.gesture.is_some());
    canvas(&mut h);
    assert_ne!(before_overlay, h.render().unwrap(), "full-app live Pen overlay is visible");
    assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
    h.key_press_modifiers(Modifiers { command: true, shift: true, ..Default::default() }, Key::Num1);
    h.run_steps(3);
    pointer(&mut h, from, Some(false));
    h.hover_at(egui::pos2(1100.0, 18.0));
    h.run_steps(3);
    parity(&before, &canvas(&mut h));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}

#[test]
fn persona_gpu_geometry_click_preserves_canvas_and_undo_restores_targets() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    two_shapes(h.state_mut());
    // A union changes the floating task bar's position with the selected bounds. Hide it
    // through its existing menu action so the pixel oracle samples the document canvas.
    if crate::context_bar::enabled(h.state()) {
        let ctx = h.ctx.clone();
        crate::menus::invoke(h.state_mut(), &ctx, crate::context_bar::TOGGLE_ID, json!({})).unwrap();
    }
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    h.get_by_label("Hand Tool").click();
    h.run_steps(3);
    assert!(h.state().session.is_enabled("path.boolean"), "V1 integration required");
    let before = canvas(&mut h);
    let doc = h.state().session.active().unwrap().doc.clone();
    let selected = h.state().session.active().unwrap().selected_layers();
    h.get_by_label("Add").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
    parity(&before, &canvas(&mut h));
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert!(Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc));
    assert_eq!(h.state().session.active().unwrap().selected_layers(), selected);
    parity(&before, &canvas(&mut h));
}
