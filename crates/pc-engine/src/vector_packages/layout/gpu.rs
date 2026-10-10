//! Real RTX parity. No adapter skips, window, network or external fixture.
use super::*;
#[test]
fn layout16_rtx_crop_and_resize_mixed_layers_preview_release_undo_parity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"));
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("layout16 GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = setup();
    s.edit("Mixed layers", |doc, _| {
        let mut raster = Layer::raster("Clipped image", doc.pixel_format());
        raster.clipped = true;
        raster.surface_mut().unwrap().fill_rect(Rect::new(0, 0, 256, 192), &[0.1, 0.6, 0.8, 0.5]);
        doc.layers.insert(1, raster);
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    let request = Commit { capture: c, operation: Operation::Crop { rect: Some([15.25, 25.5, 10.5, 15.25]) } };
    let detached = preview(&s, &request).unwrap();
    let shown = project_clips(&detached).unwrap();
    fn parity(c: &mut photocraft_gpu::Compositor, d: &wgpu::Device, q: &wgpu::Queue, doc: &Document) {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(c, d, q, doc, doc.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0.0f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied max difference {max}");
        eprintln!("layout16 premultiplied max diff {max}");
    }
    parity(&mut compositor, &device, &queue, &shown);
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    s.execute("vector.layout.apply", json!(request)).unwrap();
    parity(&mut compositor, &device, &queue, &project_clips(&s.active().unwrap().doc).unwrap());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    let mut doc = (*before).clone();
    let mut group = Layer::group("Parent", std::mem::take(&mut doc.layers));
    let gid = group.id;
    if let LayerContent::Group(g) = &mut group.content {
        g.artboard = Some(Artboard { rect: Rect::new(0, 0, 200, 160), background: ArtboardBackground::Transparent, preset: String::new() });
        g.children[0].vector.constraints = Some(photocraft_doc::vector_design::structure::Constraints { right: true, fixed_width: true, ..Default::default() });
    }
    doc.layers.push(group);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let c = Capture::begin(&s, vec![gid]).unwrap();
    let request = Commit { capture: c, operation: Operation::Resize { rect: [0., 0., 240., 180.], basis: Affine::IDENTITY } };
    let detached = preview(&s, &request).unwrap();
    parity(&mut compositor, &device, &queue, &detached);
    s.execute("vector.layout.apply", json!(request)).unwrap();
    assert_eq!(s.active().unwrap().doc.as_ref(), &detached);
    parity(&mut compositor, &device, &queue, &s.active().unwrap().doc);
}
