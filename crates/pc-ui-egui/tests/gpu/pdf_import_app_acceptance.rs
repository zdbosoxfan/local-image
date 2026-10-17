//! Actual app canvas, pointer preview/release,
//! GPU texture readback and Undo; RTX 5090 Vulkan/Rgba32Float are mandatory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Document, SampleType, Size};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, gpu_canvas::GpuCanvas};
use std::sync::{Arc, Mutex};
#[allow(dead_code)]
mod fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../pc-pdf/fixtures/import/source.rs"));
}
type H = Harness<'static, PhotocraftApp>;
fn harness() -> (H, GpuCanvas) {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") {
                    i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu
                } else {
                    i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
                }
            })
            .ok_or_else(|| (if cfg!(target_os = "macos") { "hardware Metal non-CPU adapter required" } else { "RTX 5090 Vulkan required" }).to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("PDF full-app adapter {:?}", a.get_info());
        Ok(a.clone())
    }));
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(setup)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("GPU required");
        *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(rs.clone());
        app.session.add_document(Document::new("PDF placement", Size::new(128, 128), ColorMode::Rgb, SampleType::F32), None);
        app.sync_views();
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 1.0;
        app.ui.views[0].center = [64.0, 64.0];
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app.open_bytes("Mask.pdf", &fixture::masked()).unwrap();
        assert!(app.pdf_import.dialog_open);
        app
    });
    h.run_steps(3);
    let gpu = observer.lock().unwrap().clone().unwrap();
    (h, gpu)
}
fn screen(h: &H, x: f32, y: f32) -> Pos2 {
    ViewXform::active(h.state()).unwrap().to_screen(x, y)
}
fn pointer(h: &mut H, x: f32, y: f32, press: Option<bool>) {
    let started = std::time::Instant::now();
    let pos = screen(h, x, y);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
    h.render().expect("real app pointer frame");
    eprintln!("PDF pointer update+GPU presentation diagnostic {:?}", started.elapsed());
}
fn present(h: &mut H, gpu: &GpuCanvas, doc: &Document) {
    h.run_steps(2);
    h.render().expect("actual GPU callbacks");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none(), "{:?}", h.state().perf.gpu_fallback);
    let (format, size, pixels) = gpu.read_texels(doc.id.0).expect("actual app texture");
    // The real app composites with adapter-preferred Rgba32Float (asserted above),
    // then converts to its established premultiplied Rgba16Float display tiles.
    assert_eq!(format, eframe::egui_wgpu::wgpu::TextureFormat::Rgba16Float);
    assert_eq!(size, [128, 128]);
    let cpu = photocraft_compose::flatten(doc);
    assert_eq!(pixels.len(), cpu.px.len());
    let max = cpu.px.iter().zip(&pixels).flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] } else { a[i] * a[3] } - b[i]).abs())).fold(0.0_f32, f32::max);
    assert!(max <= 1.0 / 255.0, "premultiplied app max {max}");
    eprintln!("PDF app GPU premultiplied max {max}");
}
#[test]
fn pdf_import_full_app_rtx_pointer_preview_release_select_edit_undo_cancel() {
    let _gpu = crate::gpu_lock();
    let (mut h, gpu) = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_by_label("Preview PDF").click();
    h.run_steps(2);
    assert!(h.state().pdf_import.pending(), "{:?}", h.state().pdf_import.error);
    pointer(&mut h, 20.0, 20.0, Some(true));
    pointer(&mut h, 30.0, 25.0, None);
    assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
    assert_eq!(revision, h.state().session.active().unwrap().revision);
    let preview = h.state().pdf_import.preview.clone().unwrap();
    present(&mut h, &gpu, &preview);
    pointer(&mut h, 30.0, 25.0, Some(false));
    let released = h.state().session.active().unwrap().doc.clone();
    assert_eq!(history + 1, h.state().session.active().unwrap().history.past_len());
    assert_eq!(revision + 1, h.state().session.active().unwrap().revision);
    assert!(!h.state().pdf_import.pending());
    present(&mut h, &gpu, &released);
    // Use actual canvas selection and Move tool drag on the imported group.
    h.state_mut().pdf_import.dialog_open = false;
    h.state_mut().ui.tool = photocraft_ui_egui::state::Tool::Move;
    pointer(&mut h, 35.0, 35.0, Some(true));
    pointer(&mut h, 39.0, 37.0, None);
    pointer(&mut h, 39.0, 37.0, Some(false));
    let moved = h.state().session.active().unwrap().doc.clone();
    assert_ne!(photocraft_compose::flatten(&released), photocraft_compose::flatten(&moved));
    present(&mut h, &gpu, &moved);
    h.state_mut().run("edit.undo", serde_json::json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().doc, released);
    present(&mut h, &gpu, &released);
    h.state_mut().run("edit.undo", serde_json::json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    present(&mut h, &gpu, &before);
    h.state_mut().pdf_import.dialog_open = true;
    h.run_steps(2);
    h.get_by_label("Preview PDF").click();
    h.run_steps(2);
    pointer(&mut h, 25.0, 25.0, Some(true));
    pointer(&mut h, 35.0, 30.0, None);
    h.event(Event::WindowFocused(false));
    h.step();
    pointer(&mut h, 35.0, 30.0, Some(false));
    assert!(!h.state().pdf_import.pending());
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
