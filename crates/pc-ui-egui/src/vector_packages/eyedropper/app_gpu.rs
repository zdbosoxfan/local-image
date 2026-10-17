//! Full eframe app gate for the advanced style sampler through the native Style Picker
//! flyout. The shared Eyedropper keeps the same color-sampling behavior in both personas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::PhotocraftApp;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::vector_packages::styles;
#[test]
fn styles38_gpu_full_app_i_alt_shift_preview_release_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let fixture = super::tests::host();
    let ids = fixture.ids.clone();
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |a| {
        let i = a.get_info();
        assert_eq!(i.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
        #[cfg(not(target_os = "macos"))]
        assert_eq!(i.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        #[cfg(target_os = "macos")]
        assert_ne!(i.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
        #[cfg(not(target_os = "macos"))]
        assert!(i.name.contains("5090"), "{i:?}");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), eframe::wgpu::TextureFormat::Rgba32Float);
        eprintln!("styles38 full app {i:?}");
    });
    let mut h = Harness::builder().with_size(egui::vec2(1200., 800.)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(fixture.session, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("required GPU").clone());
        app
    });
    h.state_mut().sync_views();
    h.state_mut().ui.extras.rulers = false;
    h.state_mut().ui.views[0].zoom = 1.;
    h.state_mut().ui.views[0].center = [150., 50.];
    h.state_mut().ui.views[0].fit_pending = false;
    h.run_steps(3);
    h.get_by_label("Vector persona").click();
    h.run_steps(3);
    h.event(Event::Key { key: egui::Key::I, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().ui.tool, crate::state::Tool::Eyedropper);
    let at = h.get_by_label("Eyedropper Tool").rect().center();
    for pressed in [true, false] {
        h.event(Event::PointerMoved(at));
        h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    h.get_by_label("Style Picker Tool").click();
    h.run_steps(2);
    h.get_by_label("Vector Eyedropper").click();
    h.run_steps(2);
    assert!(h.state().gpu_active());
    assert_eq!(h.state().perf.gpu_info.canvas, "gpu");
    fn screen(h: &Harness<'_, PhotocraftApp>, p: [f32; 2]) -> egui::Pos2 {
        let a = h.state();
        let v = &a.ui.views[0];
        crate::canvas::ViewXform { rect: crate::rulers::content_rect(a, a.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
            .to_screen(p[0], p[1])
    }
    fn event(h: &mut Harness<'_, PhotocraftApp>, p: [f32; 2], pressed: bool, modifiers: Modifiers) {
        let pos = screen(h, p);
        h.event(Event::PointerMoved(pos));
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers });
        h.run_steps(2);
    }
    fn pixel(h: &mut Harness<'_, PhotocraftApp>, p: [f32; 2], expected: [f32; 3]) {
        h.run_steps(3);
        h.render().expect("live app GPU readback");
        let id = h.state().session.active().unwrap().doc.id;
        let (_, size, pixels) = h.state().read_canvas_texels(id.0).expect("actual canvas texture");
        let actual = pixels[(p[1] as u32 * size[0] + p[0] as u32) as usize];
        for (c, e) in actual[..3].iter().zip(expected) {
            assert!((*c - e).abs() <= 1. / 255., "live preview/release {actual:?} versus {expected:?}");
        }
    }
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    event(&mut h, [25., 40.], true, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    // Independent compositing oracle: half-opacity green over opaque red = (.5,.5,0).
    pixel(&mut h, [95., 40.], [0.5, 0.5, 0.]);
    event(&mut h, [25., 40.], false, Modifiers::NONE);
    pixel(&mut h, [95., 40.], [0.5, 0.5, 0.]);
    assert_eq!(h.state().session.journal.len(), journal + 1);
    assert_eq!(
        styles::sample(&h.state().session.active().unwrap().doc, ids[0], None).unwrap(),
        styles::sample(&h.state().session.active().unwrap().doc, ids[1], None).unwrap()
    );
    event(&mut h, [165., 40.], true, Modifiers::ALT);
    event(&mut h, [165., 40.], false, Modifiers::ALT);
    pixel(&mut h, [165., 50.], [0.5, 0.5, 0.]);
    event(&mut h, [230., 40.], true, Modifiers::SHIFT);
    event(&mut h, [230., 40.], false, Modifiers::SHIFT);
    // The sampled RGB sits in the half-opacity top fill; retain the red underlying fill.
    pixel(&mut h, [95., 40.], [0.6, 0.2, 0.4]);
}
