//! Full production-app preview/release gate. Requires 00's exact router/display handoff;
//! absent hooks fail. Offscreen eframe/egui_kittest only, no desktop window or software skip.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{PhotocraftApp, state::Tool};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
type H = Harness<'static, PhotocraftApp>;
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "repeat19 requires RTX 5090 Vulkan discrete adapter".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("repeat19 full-app adapter {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut s = tests::fixture();
        s.edit_prefs(|p| {
            p.toolbar.persona = photocraft_engine::toolsets::Persona::Vector;
            p.interface.contextual_task_bar = false;
        });
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU renderer").clone());
        app.sync_views();
        app.ui.tool = Tool::PathSelection;
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [256., 160.];
        app
    });
    h.run_steps(3);
    assert!(h.state().gpu_active());
    let mut b = app_hooks::bridge(&h.ctx);
    b.state.preferences.rows = 2;
    b.state.preferences.cols = 3;
    app_hooks::store(&h.ctx, b);
    app_hooks::open(&h.ctx);
    h.run_steps(3);
    h
}
fn pointer(h: &mut H, p: Point, pressed: Option<bool>) {
    let xf = crate::canvas::ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(p.x as f32, p.y as f32);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn parity(h: &mut H) {
    h.run_steps(3);
    h.render().expect("full app GPU frame/readback");
    assert!(h.state().perf.gpu, "actual app GPU frame required");
    assert!(h.state().perf.gpu_fallback.is_none(), "CPU compositor fallback does not pass repeat GPU acceptance");
    let st = h.state().session.active().unwrap();
    let (shown, _) = app_hooks::display(&h.state().session, &h.ctx, st.doc.clone(), 0).unwrap();
    let gpu = h.state().gpu.as_ref().unwrap();
    let (_, size, pixels) = gpu.read_texels(st.doc.id.0).expect("actual app GPU canvas texture");
    assert_eq!(size, [shown.size.width, shown.size.height]);
    let cpu = photocraft_compose::flatten(&shown);
    assert_eq!(cpu.px.len(), pixels.len());
    for (a, b) in cpu.px.iter().zip(pixels) {
        for k in 0..4 {
            let ca = if k == 3 { a[k] } else { a[k] * a[3] };
            let cb = b[k];
            assert!((ca - cb).abs() <= 1. / 255., "premultiplied app parity {a:?} {b:?}");
        }
    }
}
#[test]
#[ignore = "owner speed rules: integrate 00 canvas hooks then run RTX acceptance"]
fn repeat19_full_app_gpu_live_preview_cancel_release_expand_undo_source_edit_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness();
    // This click requires frame() to be installed in the production app. No substitute host.
    h.get_by_label("Grid Repeat").click();
    h.run_steps(3);
    parity(&mut h);
    let live = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    let j = h.state().session.journal.len();
    let p = Point::new(50., 35.);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Point::new(56., 35.), None);
    h.run_steps(2);
    assert!(app_hooks::bridge(&h.ctx).state.preview.is_some(), "00 repeat canvas route is required");
    assert_eq!(h.state().session.active().unwrap().doc, live);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert_eq!(h.state().session.journal.len(), j);
    parity(&mut h);
    h.key_press(Key::Escape);
    h.step();
    pointer(&mut h, Point::new(56., 35.), Some(false));
    h.run_steps(2);
    assert!(app_hooks::bridge(&h.ctx).state.preview.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, live);
    parity(&mut h);
    pointer(&mut h, p, Some(true));
    for dx in [3., 6., 9.] {
        pointer(&mut h, Point::new(p.x + dx, p.y), None);
        h.run_steps(2);
        parity(&mut h);
    }
    pointer(&mut h, Point::new(59., 35.), Some(false));
    h.run_steps(3);
    assert_eq!(h.state().session.journal.len(), j + 1);
    parity(&mut h);
    let edited = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Expand Repeat").click();
    h.run_steps(3);
    parity(&mut h);
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, edited);
    parity(&mut h);
    let id = edited.layers[0].children().unwrap()[0].id;
    h.state_mut().session.execute("shape.applyPaths",json!({"edits":[{"layer":id.0,"path":photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[(30.,30.),(46.,30.),(46.,40.),(30.,40.)])])}]})).unwrap();
    h.run_steps(3);
    parity(&mut h);
}
