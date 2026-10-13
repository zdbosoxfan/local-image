//! Real-GPU gate supplied for integration; owner speed rules forbid executing it here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
#[ignore = "owner speed rules: coordinator RTX 5090 integration acceptance"]
fn blob40_gpu_rtx5090_vulkan_preview_release_eraser_and_undo_pixels() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN },
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 required");
    let info = adapter.get_info();
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("blob40 GPU {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let check = |doc: &Document, c: &mut photocraft_gpu::Compositor| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(c, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(gpu.len(), cpu.px.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(gpu.iter().flatten().all(|v| v.is_finite()));
        assert!(max <= 1. / 255., "premultiplied max diff={max}");
        gpu
    };
    let mut s = tests::session();
    let before = s.active().unwrap().doc.clone();
    let p = prepare(&s, &tests::request(&s, Mode::Blob, &[(20., 70.), (140., 70.)], 40.)).unwrap();
    let live = check(&p.document, &mut compositor);
    p.commit(&mut s).unwrap();
    assert_eq!(live, check(&s.active().unwrap().doc, &mut compositor));
    let p = prepare(&s, &tests::request(&s, Mode::Eraser, &[(80., 0.), (80., 150.)], 10.)).unwrap();
    let live = check(&p.document, &mut compositor);
    p.commit(&mut s).unwrap();
    assert_eq!(live, check(&s.active().unwrap().doc, &mut compositor));
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    check(&before, &mut compositor);
}
