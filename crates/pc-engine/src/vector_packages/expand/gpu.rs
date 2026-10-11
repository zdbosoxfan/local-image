//! Strict compositor acceptance; owner speed rules leave real GPU execution to integration.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("Object 36 GPU: {info:?}");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}
#[test]
#[ignore = "integration gate: RTX 5090 Vulkan; task speed rules prohibit GPU execution"]
fn object36_gpu_expand_four_paints_before_preview_release_undo() {
    let (device, queue) = gpu();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, id) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = request(&s, &[id]);
    let preview = prepare(&s, &r).unwrap().document;
    let a = render_to_vec(&mut compositor, &device, &queue, &before, before.bounds()).expect("source appearance GPU must be supported");
    let b = render_to_vec(&mut compositor, &device, &queue, &preview, preview.bounds()).expect("expanded plain paths GPU must be supported");
    for (doc, pixels) in [(&*before, &a), (&preview, &b)] {
        let cpu = photocraft_compose::flatten(doc);
        assert_eq!(cpu.px.len(), pixels.len());
        let diff = cpu
            .px
            .iter()
            .zip(pixels)
            .flat_map(|(a, b)| {
                (0..4).map(move |c| {
                    assert!(a[c].is_finite() && b[c].is_finite());
                    if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }
                })
            })
            .fold(0., f32::max);
        assert!(diff <= 1. / 255., "CPU/GPU premultiplied max {diff}");
    }
    let diff = a
        .iter()
        .zip(&b)
        .flat_map(|(a, b)| (0..4).map(move |c| if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }))
        .fold(0., f32::max);
    assert!(diff <= 1. / 255., "before/after premultiplied max {diff}");
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    s.execute("object.expand", json!(r)).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().children().unwrap().len(), 4);
    assert!(max_diff(&s.active().unwrap().doc, &preview) <= 1. / 255.);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
