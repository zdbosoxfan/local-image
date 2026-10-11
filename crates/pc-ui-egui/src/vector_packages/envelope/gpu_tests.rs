//! Real offscreen full-app renderer gate with a package-owned display/input host.
//! This exercises the app canvas with detached projection. Production router wiring is
//! separately owned by 00; this test does not claim the menu/router is integrated.
use super::*;
use crate::{PhotocraftApp, state::Tool};
use eframe::{egui_wgpu, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
struct AppHost {
    app: PhotocraftApp,
    state: State,
}
impl eframe::App for AppHost {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.state.lifecycle(&self.app.session, true, ui.ctx().input(|i| i.focused), ui.ctx().input(|i| i.key_pressed(Key::Escape)));
        egui::Panel::top("envelope-gate-options").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| self.state.context_bar(ui, &mut self.app.session));
        });
        if let Some(xf) = ViewXform::active(&self.app) {
            egui::Area::new(egui::Id::new("envelope-gate-handles")).order(egui::Order::Foreground).fixed_pos(xf.rect.min).show(ui.ctx(), |ui| {
                let (response, painter) = ui.allocate_painter(xf.rect.size(), egui::Sense::click_and_drag());
                self.state.canvas_input(&mut self.app.session, &response, &xf);
                self.state.draw_overlay(&self.app.session, &painter, &xf);
            });
        }
        let shown = self.state.display_for_document(&self.app.session, 0).unwrap();
        // Test-only injection at the documented display boundary. Restore the source Arc
        // and revision after the real app frame; no history/journal edit is performed.
        let original = self.app.session.active().unwrap().doc.clone();
        let revision = self.app.session.active().unwrap().revision;
        if let Some((doc, key)) = shown {
            let st = self.app.session.active_mut().unwrap();
            st.doc = doc;
            st.revision = revision ^ key ^ (1 << 55);
        }
        eframe::App::ui(&mut self.app, ui, frame);
        let st = self.app.session.active_mut().unwrap();
        st.doc = original;
        st.revision = revision;
    }
}
fn harness() -> Harness<'static, AppHost> {
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
            .ok_or_else(|| "envelope gate requires RTX 5090 Vulkan discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
    });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let f = super::tests::fixture();
        let group = f.group;
        let mut session = f.session;
        let mut t = photocraft_doc::TextLayer { text: "ABC O".into(), transform: photocraft_geom::Affine::translate(30.0, 85.0), ..Default::default() };
        t.runs = vec![photocraft_doc::text::TextRun {
            len: t.text.len(),
            style: photocraft_doc::text::CharStyle { size_pt: 16.0, color: photocraft_doc::Color::rgb(1.0, 0.0, 0.0), ..Default::default() },
        }];
        session
            .edit("Fixture text", |d, _| {
                photocraft_text::shared().lock().unwrap().render_layer(&mut t, d.resolution_dpi, d.pixel_format());
                d.layer_mut(group).unwrap().children_mut().unwrap().push(photocraft_doc::Layer::new("Editable text", photocraft_doc::LayerContent::Text(t)));
                Ok(())
            })
            .unwrap();
        let mut app = PhotocraftApp::new(session, Default::default());
        app.ui.tool = Tool::Eyedropper;
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU renderer required").clone());
        AppHost { app, state: State { target: Some(group), ..Default::default() } }
    });
    h.run_steps(10);
    assert!(h.state().app.gpu_active());
    let info = &h.state().app.perf.gpu_info;
    assert!(info.adapter.contains("RTX 5090"));
    assert_eq!(info.backend, "vulkan");
    assert_eq!(info.device_type, "discreteGpu");
    h
}
fn pointer(h: &mut Harness<'_, AppHost>, pos: egui::Pos2, press: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}
fn canvas(h: &mut Harness<'_, AppHost>) -> Vec<[u8; 4]> {
    h.run_steps(3);
    let xf = ViewXform::active(&h.state().app).unwrap();
    let image = h.render().expect("RTX readback");
    // CPU/GPU app comparison uses identical handles, display/CMS/checkerboard
    // transforms; only the compositor is switched.
    (0..160)
        .flat_map(|y| (0..192).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5);
            image.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0
        })
        .collect()
}
fn parity(h: &mut Harness<'_, AppHost>) {
    let gpu = canvas(h);
    assert!(gpu.iter().any(|p| p[0] > 150 && p[1] < 80 && p[2] < 80), "actual warped text pixels must be visible");
    assert!(h.state().app.perf.gpu);
    let renderer = h.state_mut().app.gpu.take().expect("GPU path required");
    h.state_mut().app.canvases.clear();
    let cpu = canvas(h);
    h.state_mut().app.gpu = Some(renderer);
    assert_eq!(gpu.len(), cpu.len());
    let max = gpu.iter().zip(cpu).flat_map(|(a, b)| (0..4).map(move |i| (i16::from(a[i]) - i16::from(b[i])).unsigned_abs())).max().unwrap();
    assert!(max <= 1, "premultiplied app canvas byte diff={max}");
}
#[test]
#[ignore = "integration milestone: full app on real RTX 5090 Vulkan"]
fn envelope_gpu_app_mesh_drag_warp_text_cancel_release_cpu_parity() {
    let mut h = harness();
    let before = h.state().app.session.active().unwrap().doc.clone();
    let history = h.state().app.session.active().unwrap().history.past_len();
    let xf = ViewXform::active(&h.state().app).unwrap();
    let center = xf.to_screen(80.0, 80.0);
    pointer(&mut h, center, Some(true));
    pointer(&mut h, center + egui::vec2(16.0, -14.0), None);
    assert!(h.state().state.is_dragging());
    assert!(h.state().state.display_document(&h.state().app.session).is_some());
    assert!(Arc::ptr_eq(&before, &h.state().app.session.active().unwrap().doc));
    parity(&mut h);
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer(&mut h, center, Some(false));
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), history);
    assert_eq!(h.state().app.session.active().unwrap().doc, before);
    pointer(&mut h, center, Some(true));
    pointer(&mut h, center + egui::vec2(16.0, -14.0), None);
    pointer(&mut h, center + egui::vec2(16.0, -14.0), Some(false));
    assert!(!h.state().state.is_dragging());
    assert_eq!(h.state().app.session.journal.len(), 1);
    parity(&mut h);
    let host = h.state_mut();
    host.state.update(&host.app.session, EnvelopeSpec::Warp { style: "wave".into(), bend: 40.0, horizontal: 0.0, vertical: 0.0 }).unwrap();
    parity(&mut h);
    let host = h.state_mut();
    host.state.apply(&mut host.app.session).unwrap();
    parity(&mut h);
}
