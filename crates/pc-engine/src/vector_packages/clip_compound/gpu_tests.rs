//! Integration-only strict real hardware gate. No software fallback and no successful skip.
use super::*;
#[test]
#[ignore = "owner speed rules: RTX 5090 Vulkan acceptance at integration"]
fn clip35_gpu_raster_child_vector_parent_preview_commit_release_and_undo() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("real adapter required");
    let info = adapter.get_info();
    assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
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
        // GPU accumulation/readback is already premultiplied; CPU flatten is straight.
        let max = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(cpu, gpu)| {
                (0..4).map(move |k| {
                    assert!(gpu[k].is_finite());
                    (gpu[k] - if k == 3 { cpu[k] } else { cpu[k] * cpu[3] }).abs()
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
