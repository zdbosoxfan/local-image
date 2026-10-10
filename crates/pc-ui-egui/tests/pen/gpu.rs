//! Strict GPU-rendered isolated widget test. This is separate from the full-app gate.
use super::*;
use eframe::egui_wgpu::{self, wgpu};

pub fn setup() -> egui_wgpu::WgpuSetup {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("pen UI GPU adapter: {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    egui_wgpu::WgpuSetup::CreateNew(setup)
}

#[test]
fn pen_ui_gpu_real_pointer_preview_release_and_cancel() {
    let mut h = Harness::builder().with_size(egui::vec2(700., 320.)).wgpu_setup(setup()).build_ui_state(draw, host());
    let before = h.render().unwrap();
    click(&mut h, 20., 20.);
    click(&mut h, 100., 20.);
    button(&mut h, 80., 90., true, Modifiers::NONE);
    motion(&mut h, 50., 90., Modifiers::NONE);
    assert!(h.state().s.active().unwrap().doc.layers.iter().all(|l| !matches!(l.content, LayerContent::Shape(_))));
    let preview = h.render().unwrap();
    assert_ne!(before, preview, "live pointer preview must draw pixels");
    let preview_doc = h.state().state.preview.clone().unwrap();
    let raster = photocraft_compose::flatten(&preview_doc);
    h.key_press(egui::Key::Escape);
    h.step();
    assert!(h.state().state.preview.is_none());
    h.event(Event::PointerGone);
    h.step();
    assert_eq!(before, h.render().unwrap());
    click(&mut h, 20., 20.);
    click(&mut h, 100., 20.);
    button(&mut h, 80., 90., true, Modifiers::NONE);
    motion(&mut h, 50., 90., Modifiers::NONE);
    button(&mut h, 50., 90., false, Modifiers::NONE);
    h.key_press(egui::Key::Enter);
    h.step();
    assert_eq!(raster, photocraft_compose::flatten(&h.state().s.active().unwrap().doc));
    let committed = h.render().unwrap();
    assert_ne!(before, committed);
}
