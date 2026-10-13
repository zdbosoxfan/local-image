//! Integration-only strict real hardware gate. No software fallback and no successful skip.
use super::*;
#[test]
#[ignore = "owner speed rules: RTX 5090 Vulkan acceptance at integration"]
fn clip35_gpu_raster_child_vector_parent_preview_commit_release_and_undo() {
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
    .expect("real adapter required");
    let info = adapter.get_info();
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("clip35 adapter: {info:?}");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })).unwrap();
    let mut comp = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let pixels = doc.layers[0].surface().unwrap().clone();
    doc.layers[0].content = LayerContent::Raster(pixels);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, &ids).unwrap();
    let preview = prepare(&s, &c, &Action::MakeClip).unwrap();
    fn check(doc: &Document, comp: &mut photocraft_gpu::Compositor, device: &wgpu::Device, queue: &wgpu::Queue) {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(comp, device, queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        // render_to_vec returns straight RGBA, like CPU flatten. Compare premultiplied values.
        let max = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(cpu, gpu)| {
                (0..4).map(move |k| {
                    assert!(gpu[k].is_finite());
                    (if k == 3 { gpu[k] - cpu[k] } else { gpu[k] * gpu[3] - cpu[k] * cpu[3] }).abs()
                })
            })
            .fold(0f32, f32::max);
        assert!(max <= 1. / 255. + 1e-7, "premultiplied GPU max diff={max}");
    }
    check(&preview.document, &mut comp, &device, &queue);
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    let gid = group(&mut s, &ids);
    check(&s.active().unwrap().doc, &mut comp, &device, &queue);
    command(&mut s, &[gid], "object.clippingMask.release");
    check(&s.active().unwrap().doc, &mut comp, &device, &queue);
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    check(&before, &mut comp, &device, &queue);
    assert_eq!(s.journal.len(), 2);
}
