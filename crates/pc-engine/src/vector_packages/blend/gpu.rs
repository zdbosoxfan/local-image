//! Registered owned real-GPU gate; speed rules defer execution to integration.
use super::*;
#[test]
#[ignore = "RTX integration gate; package speed rules forbid GPU execution"]
fn blend18_live_blend_groups_rtx_5090_vulkan_clipped_effect_layers() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("blend18 GPU {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = tests::fixture();
    let id = tests::create(&mut s, 5);
    s.edit("Clip and effect", |doc, _| {
        let l = doc.layer_mut(id).unwrap();
        l.blend = photocraft_doc::BlendMode::Normal;
        l.effects.items.push(photocraft_doc::effects::Effect::default_drop_shadow());
        let mut clipped = Layer::raster("Clipped colour", doc.pixel_format());
        clipped.clipped = true;
        clipped.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.2, 0.5, 0.9, 0.4]);
        doc.layers.push(clipped);
        Ok(())
    })
    .unwrap();
    for y in [50., 90., 30.] {
        let mut spec = spec(&s, id).unwrap();
        spec.spine = Some(photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polyline(&[(25., 50.), (75., y), (125., 50.)])]));
        let request = Request { capture: Capture::begin(&s, vec![id]).unwrap(), operation: Operation::Options { spec } };
        let preview = preview(&s, &request).unwrap();
        let before = s.active().unwrap().doc.clone();
        let journal = s.journal.len();
        let cpu = photocraft_compose::flatten(&preview.display);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &preview.display, preview.display.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0f32, f32::max);
        assert!(max <= 1. / 255., "{max}");
        assert_eq!(s.active().unwrap().doc, before);
        assert_eq!(s.journal.len(), journal);
        commit(&mut s, &request).unwrap();
        let release = evaluator::project_document(&s.active().unwrap().doc).unwrap();
        assert_eq!(photocraft_compose::flatten(&release).px, cpu.px);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
