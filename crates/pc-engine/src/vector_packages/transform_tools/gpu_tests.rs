//! Integration milestone gate. Intentionally excluded by the transform34_ CPU filter.
//! No software adapter or missing float format can produce a successful skip.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
fn transform_tools_real_gpu_detached_preview_release_and_cancel_parity() {
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
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut renderer = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = tests::fixture(2);
    let before = s.active().unwrap().doc.clone();
    let operations = [
        Operation::Affine { matrix: rotate(Point::new(20., 20.), 45.).unwrap() },
        Operation::Distort { source: [20., 20., 70., 20.], quad: [[25., 20.], [85., 20.], [90., 40.], [20., 40.]], perspective: false, tolerance: 0.25 },
        Operation::Distort { source: [20., 20., 70., 20.], quad: [[25., 20.], [85., 20.], [90., 40.], [20., 40.]], perspective: true, tolerance: 0.25 },
    ];
    for operation in operations {
        let c = Commit { capture: Capture::begin(&s, &ids).unwrap(), operation, options: Options::default(), copy: false, conversion: ConversionChoice::Ask };
        let preview = prepare(&s, &c).unwrap();
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        let cpu = photocraft_compose::flatten(&preview.document);
        let gpu = photocraft_gpu::render_to_vec(&mut renderer, &device, &queue, &preview.document, preview.document.bounds()).unwrap();
        let diff = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(diff <= 1. / 255., "{diff}");
        commit(&mut s, &c).unwrap();
        assert_eq!(s.journal.len(), 1);
        let released = photocraft_gpu::render_to_vec(&mut renderer, &device, &queue, &s.active().unwrap().doc, preview.document.bounds()).unwrap();
        assert_eq!(gpu, released);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
        s.journal.clear();
    }
}
