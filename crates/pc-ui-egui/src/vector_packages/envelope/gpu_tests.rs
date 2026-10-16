//! Real offscreen full-app renderer gate with a package-owned display/input host.
//! This exercises the app canvas with detached projection. Production router wiring is
//! separately owned by 00; this test does not claim the menu/router is integrated.
use super::*;
use crate::{PhotocraftApp, state::Tool};
use eframe::{egui_wgpu, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
fn harness() -> Harness<'static, PhotocraftApp> {
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
        session.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.toolbar.vector_tool = "Envelope".into();
        });
        let mut app = PhotocraftApp::new(session, Default::default());
        app.ui.tool = Tool::Envelope;
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU renderer required").clone());
        app.envelope_state.target = Some(group);
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
fn pointer(h: &mut Harness<'_, PhotocraftApp>, pos: egui::Pos2, press: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
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
fn envelope_gpu_app_mesh_drag_warp_text_cancel_release_cpu_parity() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let journal = h.state().session.journal.len();
    let xf = ViewXform::active(h.state()).unwrap();
    let center = xf.to_screen(80.0, 80.0);
    pointer(&mut h, center, Some(true));
    pointer(&mut h, center + egui::vec2(16.0, -14.0), None);
    assert!(h.state().envelope_state.is_dragging());
    assert!(h.state().envelope_state.display_document(&h.state().session).is_some());
    assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
    parity(&mut h);
    h.key_press(Key::Escape);
    h.run_steps(2);
    pointer(&mut h, center, Some(false));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, center, Some(true));
    pointer(&mut h, center + egui::vec2(16.0, -14.0), None);
    pointer(&mut h, center + egui::vec2(16.0, -14.0), Some(false));
    assert!(!h.state().envelope_state.is_dragging());
    assert_eq!(h.state().session.journal.len(), journal + 1);
    parity(&mut h);
    let host = h.state_mut();
    host.envelope_state.update(&host.session, EnvelopeSpec::Warp { style: "wave".into(), bend: 40.0, horizontal: 0.0, vertical: 0.0 }).unwrap();
    parity(&mut h);
    let id = h.state().session.active().unwrap().doc.id;
    let (_, _, pixels) = h.state().read_canvas_texels(id.0).unwrap();
    assert!(pixels.iter().any(|p| p[0] > 0.5 && p[1] < 0.2 && p[2] < 0.2), "warped editable text remains visible");
    let host = h.state_mut();
    host.envelope_state.apply(&mut host.session).unwrap();
    parity(&mut h);
}
