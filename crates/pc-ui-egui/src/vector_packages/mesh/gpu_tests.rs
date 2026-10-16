//! Offscreen full-app live-preview/release gates using the integrated mesh input/display route.
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform, state::Tool};
use eframe::{egui_wgpu, wgpu};
use std::sync::Arc;
fn harness(lines: usize) -> Harness<'static, PhotocraftApp> {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
                }
            })
            .cloned()
            .ok_or_else(|| "mesh app gate requires RTX 5090 Vulkan discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
    });
    let mut h = Harness::builder().with_size(egui::vec2(1280., 800.)).with_pixels_per_point(1.).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let (mut session, id) = fixture();
        let mut gesture = Gesture::begin(&session, &[id], Some(&Options { rows: lines, cols: lines, ..Default::default() })).unwrap();
        gesture
            .update(|_, m| {
                for r in 0..=m.rows {
                    for c in 0..=m.cols {
                        let i = m.index(r, c);
                        m.set_color(i, [c as f32 / m.cols as f32, r as f32 / m.rows as f32, 0.3, 1.], 1.)?;
                    }
                }
                Ok(())
            })
            .unwrap();
        gesture.commit(&mut session).unwrap();
        session.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.toolbar.vector_tool = "Mesh".into();
        });
        let mut app = PhotocraftApp::new(session, Default::default());
        app.ui.tool = Tool::Mesh;
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU state required").clone());
        app
    });
    h.run_steps(10);
    assert!(h.state().gpu_active());
    let info = &h.state().perf.gpu_info;
    #[cfg(not(target_os = "macos"))]
    assert!(info.adapter.contains("RTX 5090"));
    assert_eq!(info.backend, if cfg!(target_os = "macos") { "metal" } else { "vulkan" });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, "discreteGpu");
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, "cpu");
    h
}
fn pointer(h: &mut Harness<'_, PhotocraftApp>, pos: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}
fn canvas(h: &mut Harness<'_, PhotocraftApp>) -> Vec<[u8; 4]> {
    h.run_steps(3);
    h.render().expect("RTX readback");
    let id = h.state().session.active().unwrap().doc.id;
    let (_, _, pixels) = h.state().read_canvas_texels(id.0).expect("actual app canvas texture");
    pixels.into_iter().map(|p| p.map(|c| (c.clamp(0., 1.) * 255.).round() as u8)).collect()
}

fn parity(h: &mut Harness<'_, PhotocraftApp>) {
    h.run_steps(3);
    h.render().expect("actual RTX app frame");
    assert!(h.state().perf.gpu);
    let ctx = h.ctx.clone();
    let (doc, _) = crate::canvas::display_doc(h.state_mut(), &ctx, 0);
    let cpu = photocraft_compose::flatten(&doc);
    let (_, size, pixels) = h.state().read_canvas_texels(doc.id.0).expect("actual GPU texture");
    assert_eq!(size, [doc.size.width, doc.size.height]);
    assert_eq!(cpu.px.len(), pixels.len());
    let max = cpu
        .px
        .iter()
        .zip(pixels)
        .flat_map(|(a, b)| {
            (0..4).map(move |c| {
                assert!(b[c].is_finite());
                (b[c] - a[c] * if c == 3 { 1. } else { a[3] }).abs()
            })
        })
        .fold(0f32, f32::max);
    assert!(max <= 1. / 255., "premultiplied GPU app diff={max}");
}

#[test]
fn mesh43_gpu_app_u_point_drag_cancel_and_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness(2);
    h.key_press(Key::U);
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let original_pixels = canvas(&mut h);
    let xf = ViewXform::active(h.state()).unwrap();
    let p = xf.to_screen(60., 60.);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, p + egui::vec2(13., 8.), None);
    assert!(h.state().mesh_state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_ne!(canvas(&mut h), original_pixels, "live preview must change rendered pixels");
    parity(&mut h);
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer(&mut h, p + egui::vec2(13., 8.), Some(false));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    assert_eq!(canvas(&mut h), original_pixels, "cancel restores original pixels");
    pointer(&mut h, p, Some(true));
    pointer(&mut h, p + egui::vec2(10., 6.), None);
    assert!(h.state().mesh_state.preview.is_some());
    parity(&mut h);
    let preview_pixels = canvas(&mut h);
    pointer(&mut h, p + egui::vec2(10., 6.), Some(false));
    assert!(h.state().mesh_state.preview.is_none());
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert_ne!(h.state().session.active().unwrap().doc, before);
    assert_eq!(canvas(&mut h), preview_pixels, "release publishes preview pixels");
    parity(&mut h);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "performance gate requires a release build")]
fn mesh43_gpu_app_20_by_20_pointer_frame_budget() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert!(!std::hint::black_box(cfg!(debug_assertions)), "performance gate requires a release build");
    let mut h = harness(20);
    h.key_press(Key::U);
    h.run_steps(2);
    let xf = ViewXform::active(h.state()).unwrap();
    let p = xf.to_screen(60., 60.);
    let history = h.state().session.active().unwrap().history.past_len();
    pointer(&mut h, p, Some(true));
    let mut times = Vec::new();
    for i in 0..60 {
        let start = std::time::Instant::now();
        pointer(&mut h, p + egui::vec2((i % 7) as f32 * 0.2, (i % 5) as f32 * 0.2), None);
        assert!(h.state().mesh_state.preview.is_some());
        h.render().expect("real GPU readback");
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let start = std::time::Instant::now();
    pointer(&mut h, p + egui::vec2(0.6, 0.8), Some(false));
    h.render().expect("release GPU readback");
    let release = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1, "measurement must include a real release edit");
    times.sort_by(f64::total_cmp);
    eprintln!(
        "20x20 mesh, 120x120 document, 1280x800 full-app offscreen UI+GPU readback: median={}ms p95={}ms p99={}ms release={}ms hardware={:?}",
        (times[29] + times[30]) / 2.,
        times[56],
        times[59],
        release,
        h.state().perf.gpu_info
    );
    assert!(times.iter().all(|t| t.is_finite() && *t > 0.));
}
