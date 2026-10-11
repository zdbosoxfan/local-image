//! Full production-app gate. Requires 00's menu/frame/display hooks from integration/36.md;
//! missing hooks fail, and the adapter gate has no software fallback. Never opens a window.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::PhotocraftApp;
use egui::{Event, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
type H = Harness<'static, PhotocraftApp>;
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness(four_paints: bool) -> H {
    use eframe::{
        egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
        wgpu,
    };
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("RTX 5090")
            })
            .ok_or_else(|| "Object 36 requires RTX 5090 Vulkan discrete adapter".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
        eprintln!("Object 36 app adapter {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = tests::fixture();
        if four_paints {
            use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
            use photocraft_doc::{Color, Fill, LayerContent, ShapeStroke};
            let st = s.active_mut().unwrap();
            let mut doc = (*st.doc).clone();
            let bounds = doc.bounds();
            let format = doc.pixel_format();
            let l = &mut doc.layers[0];
            let fill =
                |color| AppearanceItem::Fill(FillItem { paint: Fill::Solid(color), opacity: 0.7, blend: photocraft_color::BlendMode::Normal, visible: true });
            let stroke = |color, width| {
                AppearanceItem::Stroke(StrokeItem {
                    base: ShapeStroke { paint: Fill::Solid(color), width, opacity: 0.6, ..Default::default() },
                    ..Default::default()
                })
            };
            l.vector.appearance = Some(Appearance {
                items: vec![fill(Color::rgb(0.8, 0.1, 0.1)), stroke(Color::BLACK, 8.), fill(Color::rgb(0.1, 0.8, 0.1)), stroke(Color::WHITE, 2.)],
            });
            l.opacity = 0.8;
            let LayerContent::Shape(sh) = &mut l.content else { panic!("shape fixture") };
            let appearance = photocraft_vector::appearance::CompiledAppearance::with_projection(
                sh,
                l.vector.appearance.as_ref(),
                &doc.patterns,
                bounds,
                &expand::Projection,
            )
            .unwrap();
            sh.cache = Some(appearance.render(format, bounds).unwrap());
            st.doc = Arc::new(doc);
        }
        s.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.interface.contextual_task_bar = false;
        });
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU renderer").clone());
        app.sync_views();
        app.ui.tool = crate::state::Tool::PathSelection;
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [90., 70.];
        app
    });
    h.run_steps(4);
    assert!(h.state().gpu_active());
    h
}
fn parity(h: &mut H, expected: &Document) -> Vec<[f32; 4]> {
    h.render().expect("actual app GPU frame/readback");
    assert!(h.state().perf.gpu);
    assert!(h.state().perf.gpu_fallback.is_none(), "CPU fallback does not pass");
    let gpu = h.state().gpu.as_ref().unwrap();
    let (_, size, pixels) = gpu.read_texels(expected.id.0).expect("actual GPU canvas texture");
    assert_eq!(size, [expected.size.width, expected.size.height]);
    let cpu = photocraft_compose::flatten(expected);
    assert_eq!(cpu.px.len(), pixels.len());
    for (a, b) in cpu.px.iter().zip(&pixels) {
        for c in 0..4 {
            assert!(a[c].is_finite() && b[c].is_finite());
            let a = if c == 3 { a[c] } else { a[c] * a[3] };
            let b = b[c];
            assert!((a - b).abs() <= 1. / 255., "premultiplied app pixel diff {}", (a - b).abs());
        }
    }
    pixels
}
fn pointer(h: &mut H, pos: Pos2, down: Option<bool>) {
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = down {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn open(h: &mut H, label: &str) {
    h.get_by_label("Object").click();
    h.run();
    h.get_by_label("Path ⏵").click();
    h.run();
    h.get_by_label(label).click();
    h.run();
}
#[test]
#[ignore = "integration gate: install 00 production hooks; speed rules prohibit real GPU execution"]
fn object36_full_app_gpu_simplify_slider_live_cancel_release_one_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(false);
    let before = h.state().session.active().unwrap().doc.clone();
    parity(&mut h, &before);
    open(&mut h, "Simplify…");
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Curve precision").rect();
    let from = Pos2::new(rect.left() + 40., rect.center().y);
    let to = Pos2::new(rect.left() + 20., rect.center().y);
    pointer(&mut h, from, Some(true));
    pointer(&mut h, to, None);
    let preview = app_hooks::bridge(&h.ctx).state.display_document(&h.state().session).expect("00 live preview display hook required").0;
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    parity(&mut h, &preview);
    pointer(&mut h, to, Some(false));
    h.get_by_label("Cancel").click();
    h.run();
    parity(&mut h, &before);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    open(&mut h, "Simplify…");
    h.get_by_label("OK").click();
    h.run_steps(3);
    let after = h.state().session.active().unwrap().doc.clone();
    parity(&mut h, &after);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    parity(&mut h, &before);
}
#[test]
#[ignore = "integration gate: install 00 production hooks; speed rules prohibit real GPU execution"]
fn object36_full_app_gpu_expand_appearance_pixel_parity_menu_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(true);
    let before = h.state().session.active().unwrap().doc.clone();
    let a = parity(&mut h, &before);
    h.get_by_label("Object").click();
    h.run();
    h.get_by_label("Expand Appearance").click();
    h.run_steps(3);
    let after = h.state().session.active().unwrap().doc.clone();
    assert_eq!(after.layers[0].children().unwrap().len(), 4);
    let b = parity(&mut h, &after);
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        for c in 0..4 {
            let a = a[c];
            let b = b[c];
            assert!((a - b).abs() <= 1. / 255.);
        }
    }
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    parity(&mut h, &before);
}
