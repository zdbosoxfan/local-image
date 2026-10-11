//! Real offscreen app acceptance. Missing 00 menu/frame/display/persistence/36 hooks fail.
//! The final task speed rule prohibits running this gate here; it opens no desktop window.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::PhotocraftApp;
use egui_kittest::{Harness, kittest::Queryable};
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
type H = Harness<'static, PhotocraftApp>;
fn harness() -> H {
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
            .ok_or_else(|| "path effects require RTX 5090 Vulkan discrete GPU".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(a));
        eprintln!("path effects app adapter: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = Session::new();
        let mut d = Document::new("App45", Size::new(160, 128), ColorMode::Rgb, SampleType::F32);
        let sh = ShapeLayer {
            path: photocraft_vector::shapes::rect(20., 20., 70., 60.),
            fill: Some(Fill::Solid(Color::rgba(0.2, 0.5, 0.8, 0.7))),
            ..Default::default()
        };
        let mut layer = Layer::new("Source", LayerContent::Shape(sh));
        engine::refresh(&d, &mut layer, engine::access().expect("native effects access must be installed by 00")).unwrap();
        let id = layer.id;
        d.layers.push(layer);
        s.add_document(d, None);
        s.select_layer(id).unwrap();
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
        app.ui.views[0].center = [80., 64.];
        app
    });
    h.run_steps(4);
    assert!(h.state().gpu_active());
    h
}
fn parity(h: &mut H, doc: &Document) {
    h.render().expect("real app GPU frame/readback");
    assert!(h.state().perf.gpu);
    assert!(h.state().perf.gpu_fallback.is_none());
    let (_, size, pixels) = h.state().gpu.as_ref().unwrap().read_texels(doc.id.0).expect("actual app canvas texture");
    assert_eq!(size, [doc.size.width, doc.size.height]);
    let cpu = photocraft_compose::flatten(doc);
    assert_eq!(cpu.px.len(), pixels.len());
    for (a, b) in cpu.px.iter().zip(pixels) {
        for c in 0..4 {
            let x = if c == 3 { a[c] } else { a[c] * a[3] };
            let y = if c == 3 { b[c] } else { b[c] * b[3] };
            assert!(x.is_finite() && y.is_finite() && (x - y).abs() <= 1. / 255., "premultiplied diff {}", (x - y).abs());
        }
    }
}
fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}
fn open(h: &mut H, group: &str, label: &str) {
    click(h, "Effect");
    h.get_by_label_contains(group).click();
    h.run_steps(3);
    click(h, label);
}
#[test]
#[ignore = "integration full-app GPU gate; task speed rules prohibit execution"]
fn path_effects45_full_app_rtx5090_preview_pointer_release_cancel_and_expand() {
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    parity(&mut h, &before);
    open(&mut h, "Distort & Transform", "Pucker & Bloat");
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Amount").rect();
    let from = rect.center();
    let to = egui::pos2(rect.left() + rect.width() * 0.75, from.y);
    h.event(egui::Event::PointerMoved(from));
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerMoved(to));
    h.step();
    let preview = app_hooks::bridge(&h.ctx).state.display_document(&h.state().session).expect("00 canvas preview hook required").0;
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    parity(&mut h, &preview);
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    click(&mut h, "Cancel");
    parity(&mut h, &before);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    open(&mut h, "Stylize", "Round Corners");
    click(&mut h, "OK");
    let after = h.state().session.active().unwrap().doc.clone();
    assert_eq!(h.state().session.journal.len(), 1);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    let shown = engine::project_document(&after, engine::access().unwrap(), &mut engine::ProjectionCache::default()).unwrap();
    parity(&mut h, &shown);
    click(&mut h, "Object");
    click(&mut h, "Expand Appearance");
    let expanded = h.state().session.active().unwrap().doc.clone();
    assert!(expanded.layers[0].children().is_some());
    assert!((engine::access().unwrap().read)(&expanded.layers[0]).unwrap().is_empty());
    parity(&mut h, &expanded);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    parity(&mut h, &before);
}
