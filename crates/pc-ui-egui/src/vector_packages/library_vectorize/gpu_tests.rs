//! Package 28-dependent offscreen live-preview gate; owner speed rule forbids running it here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use eframe::{App, egui_wgpu, wgpu};
use egui_kittest::{Harness, kittest::Queryable};
use image::ImageEncoder;
use std::sync::Arc;

// Fixture launcher represents the Library host request; all handoff/dialog state
// and subsequent frames live in the production editor.
struct Host {
    app: PhotocraftApp,
    bytes: Vec<u8>,
}
impl Host {
    fn new(app: PhotocraftApp) -> Self {
        Self { app, bytes: Vec::new() }
    }
}
impl App for Host {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        egui::Area::new(egui::Id::new("library29-test-launcher")).fixed_pos(egui::pos2(950., 40.)).show(ui.ctx(), |ui| {
            if ui.button("Load context photo").clicked() {
                let mut handoff = std::mem::take(&mut self.app.library_vectorize);
                handoff.open_file(&mut self.app, 29, "context-photo.png", &self.bytes).unwrap();
                self.app.library_vectorize = handoff;
            }
        });
    }
}

#[test]
#[ignore = "owner speed rule: RTX 5090 live-app acceptance runs at integration"]
fn library_vectorize_gpu_loaded_dialog_live_preview_drag_release_matches_cpu_cancel_keeps_photo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN };
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| {
                let info = a.get_info();
                if cfg!(target_os = "macos") {
                    info.backend == wgpu::Backend::Metal && info.device_type != wgpu::DeviceType::Cpu
                } else {
                    info.name.contains("RTX 5090") && info.backend == wgpu::Backend::Vulkan && info.device_type == wgpu::DeviceType::DiscreteGpu
                }
            })
            .cloned()
            .ok_or_else(|| "package 29 requires RTX 5090 / Vulkan / discrete".into())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        assert!(crate::gpu_canvas::supports_f16_canvas(adapter));
    });
    let mut h = Harness::builder().with_size(egui::vec2(1280., 900.)).with_pixels_per_point(1.).with_max_steps(64).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = tests::app();
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU renderer").clone());
        app.ui.trace_preferences.last_preset = photocraft_engine::vector_packages::trace_output::Preset::PixelArt;
        let mut host = Host::new(app);
        let pixels: Vec<u8> = (0..96).flat_map(|_| (0..128).flat_map(|x| if (x / 32) % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })).collect();
        host.bytes.clear();
        image::codecs::png::PngEncoder::new(&mut host.bytes).write_image(&pixels, 128, 96, image::ExtendedColorType::Rgba8).unwrap();
        host
    });
    h.run_steps(8);
    assert!(h.state().app.gpu_active());
    #[cfg(not(target_os = "macos"))]
    assert!(h.state().app.perf.gpu_info.adapter.contains("RTX 5090"));
    assert_eq!(h.state().app.perf.gpu_info.backend, if cfg!(target_os = "macos") { "metal" } else { "vulkan" });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(h.state().app.perf.gpu_info.device_type, "discreteGpu");
    #[cfg(target_os = "macos")]
    assert_ne!(h.state().app.perf.gpu_info.device_type, "cpu");
    h.get_by_label("Load context photo").click();
    h.run_steps(3);
    {
        let host = h.state_mut();
        host.app.trace_dialog.request_preview(&mut host.app.session, 1.);
        if let Some(job) = host.app.trace_dialog.pending_preview() {
            host.app.session.wait_job(job).unwrap();
        }
    }
    h.run_steps(4);
    h.get_by_label("Result").click();
    h.run_steps(3);
    let loaded = h.state().app.session.active().unwrap().doc.clone();
    let revision = h.state().app.session.active().unwrap().revision;
    // Align one source texel per screen pixel, avoiding a second reference sampler.
    let rect = h.state().app.trace_dialog.preview_rect;
    let fit = (rect.width() / 128.).min(rect.height() / 96.);
    h.state_mut().app.trace_dialog.zoom = 1. / fit;
    assert!((0.1..=64.).contains(&h.state().app.trace_dialog.zoom));
    let origin = rect.center() - egui::vec2(64., 48.);
    h.state_mut().app.trace_dialog.pan = origin.round() - origin;
    h.run_steps(3);
    h.hover_at(egui::pos2(1100., 820.));
    h.run_steps(3);
    assert_preview(&mut h);
    // Zoom to a clickable preview for a real pointer drag, then return to texel scale.
    h.state_mut().app.trace_dialog.zoom *= 20.;
    h.run_steps(3);
    let from = h.state().app.trace_dialog.preview_rect.center();
    for (at, pressed) in [(from, Some(true)), (from + egui::vec2(24., 18.), None), (from + egui::vec2(24., 18.), Some(false))] {
        h.event(egui::Event::PointerMoved(at));
        if let Some(pressed) = pressed {
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(3);
    }
    assert!(h.state().app.trace_dialog.pan.length() > 20.);
    h.state_mut().app.trace_dialog.zoom /= 20.;
    h.run_steps(3);
    let rect = h.state().app.trace_dialog.preview_rect;
    let state = &mut h.state_mut().app.trace_dialog;
    let origin = rect.center() + state.pan - egui::vec2(64., 48.);
    state.pan += origin.round() - origin;
    h.run_steps(3);
    h.hover_at(egui::pos2(1100., 820.));
    h.run_steps(3);
    assert_preview(&mut h);
    assert_eq!(h.state().app.session.active().unwrap().doc, loaded);
    assert_eq!(h.state().app.session.active().unwrap().revision, revision);
    h.get_by_label("Cancel").click();
    h.run_steps(6);
    assert!(!h.state().app.trace_dialog.open);
    assert!(!h.state().app.library_vectorize.pending());
    assert!(h.state().app.library_vectorize.preserves_library_state(&h.state().app.session));
    assert_eq!(h.state().app.session.active().unwrap().doc, loaded);
    assert_eq!(h.state().app.session.active().unwrap().history.past_len(), 0);
}

fn assert_preview(h: &mut Harness<'_, Host>) {
    let image = h.render().expect("live GPU readback");
    let state = &h.state().app.trace_dialog;
    let preview = state.preview.as_ref().unwrap();
    let rect = state.preview_rect;
    let dimensions = egui::vec2(preview.bounds[2] as f32, preview.bounds[3] as f32);
    let fit = (rect.width() / dimensions.x).min(rect.height() / dimensions.y);
    let origin = rect.center() + state.pan - dimensions * fit * state.zoom * 0.5;
    assert_eq!([preview.trace.width, preview.trace.height], [128, 96]);
    let mut max = 0u8;
    for y in 0..96 {
        for x in 0..128 {
            let pos = origin + egui::vec2(x as f32 + 0.5, y as f32 + 0.5);
            assert!(rect.contains(pos));
            let actual = image.get_pixel(pos.x.floor() as u32, pos.y.floor() as u32).0;
            let i = ((y * 128 + x) * 4) as usize;
            let cpu = &preview.result_rgba[i..i + 4];
            assert_eq!(cpu[3], 255, "opaque synthetic fixture avoids checkerboard blending");
            for channel in 0..4 {
                max = max.max(actual[channel].abs_diff(cpu[channel]));
            }
        }
    }
    assert!(max <= 1, "premultiplied max diff {max}/255 rect={rect:?} zoom={} pan={:?} view={:?}", state.zoom, state.pan, state.view);
}
