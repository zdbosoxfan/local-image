//! Real GPU widget frames; the full app acceptance gate remains app_acceptance.rs.
use super::*;
use eframe::egui_wgpu::{self, wgpu};
pub fn setup() -> egui_wgpu::WgpuSetup {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("appearance widget GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    egui_wgpu::WgpuSetup::CreateNew(setup)
}
#[test]
fn appearance_ui_gpu_actual_drags_preview_release_cancel() {
    let mut h = Harness::builder().with_size(egui::vec2(760., 1200.)).wgpu_setup(setup()).build_ui_state(draw, host());
    photocraft_ui_egui::PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(3);
    let before = h.render().unwrap();
    button(&mut h, 28., 60., true);
    motion(&mut h, 96., 60.);
    assert!(h.state().s.journal.is_empty());
    let preview = h.render().unwrap();
    assert_ne!(before, preview);
    let expected = photocraft_compose::flatten(h.state().state.preview.as_ref().unwrap());
    button(&mut h, 96., 60., false);
    assert_eq!(expected, pixels(&h));
    let released = h.render().unwrap();
    for y in 35..95 {
        for x in 40..92 {
            let p = screen(&h, x as f32 + 0.5, y as f32 + 0.5);
            let a = preview.get_pixel(p.x as u32, p.y as u32).0;
            let b = released.get_pixel(p.x as u32, p.y as u32).0;
            for (a, b) in a.into_iter().zip(b) {
                assert!(a.abs_diff(b) <= 1);
            }
        }
    }
    h.get_by_label("Transparency tool").click();
    h.step();
    let saved = pixels(&h);
    button(&mut h, 96., 60., true);
    motion(&mut h, 80., 90.);
    assert_ne!(released, h.render().unwrap());
    h.key_press(egui::Key::Escape);
    h.step();
    button(&mut h, 80., 90., false);
    assert_eq!(saved, pixels(&h));
}
