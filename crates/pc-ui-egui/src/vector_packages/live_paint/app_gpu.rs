//! Full-app gates for the 00 router. These exercise production dispatch, never invoke
//! package input helpers to pretend the missing router is integrated. Speed rules defer runs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform};
use egui::{Event, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Layer, LayerContent, SampleType, ShapeLayer, Size, Subpath};
use photocraft_engine::toolsets::Persona;
use std::time::Instant;
type H = Harness<'static, PhotocraftApp>;
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness() -> H {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == eframe::wgpu::Backend::Vulkan && i.device_type == eframe::wgpu::DeviceType::DiscreteGpu
            })
            .ok_or_else(|| "Live Paint requires RTX 5090/Vulkan/discrete".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("live_paint42 app hardware {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |_| {});
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_step_dt(1. / 60.).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut doc = Document::with_background("Live Paint App", Size::new(160, 128), ColorMode::Rgb, SampleType::F32, Color::WHITE);
        let mut ids = Vec::new();
        for (i, path) in [
            photocraft_vector::shapes::ellipse(16., 24., 80., 80.),
            photocraft_vector::shapes::ellipse(56., 24., 80., 80.),
            photocraft_doc::Path::new(vec![Subpath::polyline(&[(0., 64.), (150., 64.)])]),
        ]
        .into_iter()
        .enumerate()
        {
            let mut shape = ShapeLayer { path, fill: if i < 2 { Some(Fill::Solid(Color::BLACK)) } else { None }, ..Default::default() };
            photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
            let layer = Layer::new("Source", LayerContent::Shape(shape));
            ids.push(layer.id);
            doc.layers.push(layer);
        }
        let mut s = Session::new();
        s.tools.foreground = [1., 0., 0., 1.];
        s.add_document(doc, None);
        s.select_layer(ids[0]).unwrap();
        s.active_mut().unwrap().selected_layers = ids;
        s.edit_prefs(|p| {
            p.toolbar.persona = Persona::Vector;
            p.interface.contextual_task_bar = true;
        });
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [80., 64.];
        app.ui.views[0].fit_pending = false;
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.extras = false;
        app.ui.tool = crate::state::Tool::Hand;
        app
    });
    h.run_steps(4);
    let center = ViewXform::active(h.state()).unwrap().rect.center();
    h.state_mut().ui.views[0].center = [80. + center.x.fract(), 64. + center.y.fract()];
    h.run_steps(3);
    assert!(h.state().gpu_active());
    h
}
fn event(h: &mut H, x: f32, y: f32, pressed: Option<bool>, mods: Modifiers) {
    let pos = ViewXform::active(h.state()).unwrap().to_screen(x, y);
    h.event(Event::ModifiersChanged(mods));
    h.event(if let Some(pressed) = pressed {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods }
    } else {
        Event::PointerMoved(pos)
    });
    h.step();
}
fn canvas_parity(h: &mut H, doc: &Document) {
    let doc = photocraft_engine::vector_integration::project_document(doc).unwrap();
    let cpu = photocraft_compose::flatten(&doc);
    h.render().expect("real GPU canvas");
    let (_, size, pixels) = h.state().read_canvas_texels(doc.id.0).expect("actual app canvas texture");
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
    assert!(max <= 1. / 255. + 1e-7, "premultiplied app diff={max}");
    assert!(h.state().perf.gpu);
}

#[test]
#[ignore = "00/01/46 live paint routes + RTX full-app gate; owner prohibits GPU runs"]
fn live_paint42_full_app_gpu_k_drag_live_preview_release_cancel_and_expand() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    h.key_press(Key::K);
    h.run();
    assert_eq!(state(&h.ctx).tool, Some(Tool::Bucket), "00 must install production shortcut route");
    event(&mut h, 32., 40., None, Modifiers::NONE);
    event(&mut h, 32., 40., Some(true), Modifiers::NONE);
    event(&mut h, 76., 40., None, Modifiers::NONE);
    let preview = state(&h.ctx).display_document(&h.state().session).expect("production live preview route").0;
    assert!(photocraft_compose::flatten(&preview).get(32, 40)[0] > 0.9, "00 must bind the tool paint to the foreground");
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    canvas_parity(&mut h, &preview);
    event(&mut h, 76., 40., Some(false), Modifiers::NONE);
    h.run_steps(3);
    assert_eq!(h.state().session.journal.len(), 1);
    let committed = engine::evaluator::project_document(&h.state().session.active().unwrap().doc).unwrap();
    canvas_parity(&mut h, &committed);
    event(&mut h, 70., 64., None, Modifiers::SHIFT);
    event(&mut h, 70., 64., Some(true), Modifiers::SHIFT);
    event(&mut h, 80., 64., None, Modifiers::SHIFT);
    h.key_press(Key::Escape);
    h.run();
    event(&mut h, 80., 64., Some(false), Modifiers::SHIFT);
    assert_eq!(h.state().session.journal.len(), 1);
    eprintln!("expand live paint rect={:?}", h.get_by_label("Expand Live Paint").rect());
    h.get_by_label("Expand Live Paint").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Expand Live Paint").click();
    h.run_steps(3);
    assert_eq!(h.state().session.journal.len(), 2, "status={} selection={:?}", h.state().ui.status, h.state().session.active().unwrap().selected_layers());
    assert!(h.state().session.active().unwrap().doc.walk().into_iter().all(|(_, _, layer)| layer.vector.live.is_none()));
    canvas_parity(&mut h, &committed);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
#[ignore = "release real-app pointer-frame measurement + production router; owner prohibits benchmarks"]
#[allow(clippy::assertions_on_constants)] // Runtime release gate; debug builds must still compile it.
fn live_paint42_full_app_release_click_to_paint_latency() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert!(!cfg!(debug_assertions), "release build required for latency claims");
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness();
    h.key_press(Key::K);
    h.run();
    assert_eq!(state(&h.ctx).tool, Some(Tool::Bucket), "00 shortcut route required");
    let mut elapsed = Vec::new();
    for _ in 0..20 {
        event(&mut h, 32., 40., None, Modifiers::NONE);
        let journals = h.state().session.journal.len();
        let t = Instant::now();
        event(&mut h, 32., 40., Some(true), Modifiers::NONE);
        event(&mut h, 32., 40., Some(false), Modifiers::NONE);
        h.render().expect("real app frame GPU readback");
        elapsed.push(t.elapsed().as_secs_f64() * 1000.);
        assert_eq!(h.state().session.journal.len(), journals + 1);
        let painted = engine::evaluator::project_document(&h.state().session.active().unwrap().doc).unwrap();
        assert!(photocraft_compose::flatten(&painted).get(32, 40)[0] > 0.9);
        canvas_parity(&mut h, &painted);
        assert!(h.state_mut().session.undo());
        h.run_steps(2);
    }
    elapsed.sort_by(f64::total_cmp);
    eprintln!("live_paint42 release press-to-painted-frame n=20 median={}ms p95={}ms p99={}ms", elapsed[10], elapsed[18], elapsed[19]);
}
