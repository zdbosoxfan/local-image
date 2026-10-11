//! Offscreen full-app live-preview/release gate. The host supplies package-owned input
//! and the documented display boundary until 00 installs production menu/router wiring.
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform, state::Tool};
use eframe::{egui_wgpu, wgpu};
use std::sync::Arc;
struct AppHost {
    app: PhotocraftApp,
    state: State,
    ids: Vec<LayerId>,
    enabled: bool,
}
impl eframe::App for AppHost {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if ui.input(|i| i.key_pressed(Key::U)) {
            self.enabled = true;
        }
        if let Some(xf) = ViewXform::active(&self.app) {
            self.state.input(&mut self.app.session, &self.ids, ui.ctx(), xf.rect, |p| xf.to_doc(p), 4., self.enabled).unwrap();
        }
        let original = self.app.session.active().unwrap().doc.clone();
        let revision = self.app.session.active().unwrap().revision;
        if let Some(doc) = &self.state.preview {
            let st = self.app.session.active_mut().unwrap();
            st.doc = Arc::new(doc.clone());
            st.revision = revision ^ self.state.generation ^ (1 << 56);
        }
        eframe::App::ui(&mut self.app, ui, frame);
        let st = self.app.session.active_mut().unwrap();
        st.doc = original;
        st.revision = revision;
    }
}
fn harness(lines: usize) -> Harness<'static, AppHost> {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
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
        let mut app = PhotocraftApp::new(session, Default::default());
        app.ui.tool = Tool::Eyedropper;
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU state required").clone());
        AppHost { app, state: State::default(), ids: vec![id], enabled: false }
    });
    h.run_steps(10);
    assert!(h.state().app.gpu_active());
    let info = &h.state().app.perf.gpu_info;
    assert!(info.adapter.contains("RTX 5090"));
    assert_eq!(info.backend, "vulkan");
    assert_eq!(info.device_type, "discreteGpu");
    h
}
fn pointer(h: &mut Harness<'_, AppHost>, pos: Pos2, pressed: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}
fn canvas(h: &mut Harness<'_, AppHost>) -> Vec<[u8; 4]> {
    h.run_steps(3);
    let xf = ViewXform::active(&h.state().app).unwrap();
    let image = h.render().expect("RTX readback");
    (0..120)
        .flat_map(|y| (0..120).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5);
            image.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0
        })
        .collect()
}
fn parity(h: &mut Harness<'_, AppHost>) {
    let gpu = canvas(h);
    assert!(h.state().app.perf.gpu);
    let renderer = h.state_mut().app.gpu.take().expect("GPU path required");
    h.state_mut().app.canvases.clear();
    let cpu = canvas(h);
    h.state_mut().app.gpu = Some(renderer);
    let max = gpu
        .iter()
        .zip(cpu)
        .flat_map(|(a, b)| {
            (0..4).map(move |i| {
                let a = a.map(|v| f32::from(v) / 255.);
                let b = b.map(|v| f32::from(v) / 255.);
                (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()
            })
        })
        .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "app premultiplied mesh diff {max}");
}
#[test]
#[ignore = "integration milestone: 00/08 native mesh schema and actual RTX 5090 Vulkan app renderer"]
fn mesh43_gpu_app_u_point_drag_cancel_and_release() {
    let mut h = harness(2);
    h.key_press(Key::U);
    h.run_steps(2);
    let before = h.state().app.session.active().unwrap().doc.clone();
    let history = h.state().app.session.active().unwrap().history.past_len();
    let original_pixels = canvas(&mut h);
    let xf = ViewXform::active(&h.state().app).unwrap();
    let p = xf.to_screen(60., 60.);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, p + egui::vec2(13., 8.), None);
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().app.session.active().unwrap().doc, before);
    assert_ne!(canvas(&mut h), original_pixels, "live preview must change rendered pixels");
    parity(&mut h);
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer(&mut h, p + egui::vec2(13., 8.), Some(false));
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), history);
    assert_eq!(canvas(&mut h), original_pixels, "cancel restores original pixels");
    pointer(&mut h, p, Some(true));
    pointer(&mut h, p + egui::vec2(10., 6.), None);
    assert!(h.state().state.preview.is_some());
    parity(&mut h);
    let preview_pixels = canvas(&mut h);
    pointer(&mut h, p + egui::vec2(10., 6.), Some(false));
    assert!(h.state().state.preview.is_none());
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), history + 1);
    assert_ne!(h.state().app.session.active().unwrap().doc, before);
    assert_eq!(canvas(&mut h), preview_pixels, "release publishes preview pixels");
    parity(&mut h);
}

#[test]
#[ignore = "integration milestone: release full-app RTX 5090 Vulkan pointer-frame measurement"]
fn mesh43_gpu_app_20_by_20_pointer_frame_budget() {
    assert!(!std::hint::black_box(cfg!(debug_assertions)), "performance gate requires a release build");
    let mut h = harness(20);
    h.key_press(Key::U);
    h.run_steps(2);
    let xf = ViewXform::active(&h.state().app).unwrap();
    let p = xf.to_screen(60., 60.);
    let history = h.state().app.session.active().unwrap().history.past_len();
    pointer(&mut h, p, Some(true));
    let mut times = Vec::new();
    for i in 0..60 {
        let start = std::time::Instant::now();
        pointer(&mut h, p + egui::vec2((i % 7) as f32 * 0.2, (i % 5) as f32 * 0.2), None);
        assert!(h.state().state.preview.is_some());
        h.render().expect("real GPU readback");
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let start = std::time::Instant::now();
    pointer(&mut h, p + egui::vec2(0.6, 0.8), Some(false));
    h.render().expect("release GPU readback");
    let release = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), history + 1, "measurement must include a real release edit");
    times.sort_by(f64::total_cmp);
    eprintln!(
        "20x20 mesh, 120x120 document, 1280x800 full-app offscreen UI+GPU readback: median={}ms p95={}ms p99={}ms release={}ms hardware={:?}",
        (times[29] + times[30]) / 2.,
        times[56],
        times[59],
        release,
        h.state().app.perf.gpu_info
    );
    assert!(times.iter().all(|t| t.is_finite() && *t > 0.));
}
