//! Strict live-preview/cache compositor parity on the owner's Vulkan RTX 5090.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
#[test]
fn text_on_path_layers_rtx5090_preview_flip_commit_undo_parity() {
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
    .expect("RTX 5090 Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("path text GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, id) = session();
    let mut c = request(&s, id, "ABC office");
    // Actual curved source, not a raster stand-in.
    let path = photocraft_vector::shapes::ellipse(28., 28., 200., 200.);
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(id).unwrap().content =
        LayerContent::Shape(ShapeLayer { path: path.clone(), ..Default::default() });
    c.edits[0].path.path = path;
    for flip in [false, true] {
        c.edits[0].path.flip = flip;
        let doc = preview_document(&s, &c).unwrap();
        let cpu = photocraft_compose::flatten(&doc);
        let gpu = render_to_vec(&mut compositor, &device, &queue, &doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0.0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied max diff={max}");
        eprintln!("text_on_path flip={flip}: max diff={max}");
        assert!(cpu.px.iter().any(|p| p[3] > 0.9));
    }
    let preview = preview_document(&s, &c).unwrap();
    publish(&mut s, &c);
    assert_eq!(photocraft_compose::flatten(&preview), photocraft_compose::flatten(&s.active().unwrap().doc));
    assert!(s.undo());
}
