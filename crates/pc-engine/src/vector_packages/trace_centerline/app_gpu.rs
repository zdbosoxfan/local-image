//! Requires the actual trace dialog.
//! This is a real full-app acceptance test, deliberately requiring the actual
//! menu/dialog labels and renderer; no miniature substitute widget or skipped GPU.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType, Size};
use photocraft_ui_egui::{PhotocraftApp, gpu_canvas::GpuCanvas};
use std::sync::{Arc, Mutex};

#[test]
fn centerline_full_app_rtx_dialog_mode_live_preview_trace_and_undo() {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    setup.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                if cfg!(target_os = "macos") { i.backend == wgpu::Backend::Metal && i.device_type != wgpu::DeviceType::Cpu } else { i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090") }
            })
            .ok_or_else(|| (if cfg!(target_os = "macos") { "hardware Metal non-CPU adapter required" } else { "RTX 5090 Vulkan required" }).to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("centreline full-app adapter {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = WgpuSetup::CreateNew(setup);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    let observer = Arc::new(Mutex::new(None));
    let record = observer.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200., 900.)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("GPU required");
        *record.lock().unwrap() = Some(GpuCanvas::new(rs));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(rs.clone());
        let mut doc = Document::new("Centreline scan", Size::new(96, 96), ColorMode::Rgb, SampleType::F32);
        let mut scan = Layer::raster("Scan", doc.pixel_format());
        if let LayerContent::Raster(p) = &mut scan.content {
            p.fill_rect(doc.bounds(), &[1., 1., 1., 1.]);
            p.fill_rect(photocraft_geom::Rect::new(16, 40, 80, 45), &[0., 0., 0., 1.]);
        }
        let id = scan.id;
        doc.layers.push(scan);
        app.session.add_document(doc, None);
        app.session.active_mut().unwrap().active_layer = Some(id);
        app.session.active_mut().unwrap().selected_layers = vec![id];
        app.sync_views();
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].center = [48., 48.];
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app
    });
    h.run_steps(3);
    let gpu = observer.lock().unwrap().clone().unwrap();
    let before = h.state().session.active().unwrap().doc.clone();
    let revision = h.state().session.active().unwrap().revision;
    // The trace dialog is reached through the shared Layer > Vectorize entry point.
    h.get_by_label("Layer").click();
    h.run_steps(2);
    h.get_by_label("Vectorize…").click();
    h.run_steps(3);
    h.get_by_label("Centreline").click();
    for _ in 0..64 {
        h.run_steps(2);
        if photocraft_ui_egui::trace_dialog::preview_document(h.state()).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
    assert_eq!(h.state().session.active().unwrap().revision, revision);
    assert!(h.state().session.journal.is_empty());
    // The trace dialog supplies this query hook to expose the same detached document
    // that its actual preview callback displays. No Session mutation is allowed.
    let preview = photocraft_ui_egui::trace_dialog::preview_document(h.state()).expect("live trace preview");
    fn parity(h: &mut Harness<'static, PhotocraftApp>, gpu: &GpuCanvas, doc: &Document) {
        h.run_steps(2);
        h.render().expect("real app GPU callbacks");
        assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
        let (format, size, pixels) = gpu.read_texels(doc.id.0).expect("actual app canvas texels");
        assert_eq!(format, eframe::egui_wgpu::wgpu::TextureFormat::Rgba16Float);
        assert_eq!(size, [96, 96]);
        let cpu = photocraft_compose::flatten(doc);
        assert_eq!(pixels.len(), cpu.px.len());
        let max =
            cpu.px.iter().zip(&pixels).flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] } else { a[i] * a[3] } - b[i]).abs())).fold(0f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied app diff {max}");
    }
    parity(&mut h, &gpu, &preview);
    h.get_by_label("Trace").click();
    for _ in 0..64 {
        h.run_steps(2);
        if h.state().session.active().unwrap().revision != revision {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let after = h.state().session.active().unwrap().doc.clone();
    assert_eq!(h.state().session.active().unwrap().revision, revision + 1);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), 1);
    assert_eq!(h.state().session.journal.len(), 1);
    assert!(after.layers.iter().any(|l| matches!(&l.content,LayerContent::Group(g) if g.children.iter().all(|l|matches!(&l.content,LayerContent::Shape(s) if s.fill.is_none() && s.stroke.is_some() && s.path.subpaths.iter().all(|p|!p.closed))))));
    parity(&mut h, &gpu, &after);
    h.state_mut().run("edit.undo", serde_json::json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    parity(&mut h, &gpu, &before);
}
