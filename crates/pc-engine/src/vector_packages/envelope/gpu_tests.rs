//! Strict offscreen projection parity. Full-app hook gate is supplied with the UI package.
use super::*;
#[test]
#[ignore = "integration milestone: requires real RTX 5090 Vulkan"]
fn envelope_gpu_warped_path_text_preview_release_parity() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN },
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("RTX 5090 required");
    let info = adapter.get_info();
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, id, _) = super::tests::fixture(true);
    let source = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &[id]).unwrap();
    g.update(EnvelopeSpec::Warp { style: "wave".into(), bend: 40.0, horizontal: 0.0, vertical: 0.0 }).unwrap();
    let preview = preview_document(&s, &g.request).unwrap();
    assert_eq!(s.active().unwrap().doc, source);
    g.commit(&mut s).unwrap();
    let release = project_document(&s.active().unwrap().doc).unwrap();
    for shown in [preview, release] {
        let cpu = photocraft_compose::render(&shown, shown.bounds());
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &shown, shown.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let mut max = 0.0_f32;
        for (a, b) in cpu.px.iter().zip(&gpu) {
            for c in 0..4 {
                let x = if c == 3 { a[c] } else { a[c] * a[3] };
                let y = if c == 3 { b[c] } else { b[c] * b[3] };
                max = max.max((x - y).abs());
            }
        }
        assert!(max <= 1.0 / 255.0, "premultiplied diff={max}; {info:?}");
    }
}
