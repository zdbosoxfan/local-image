//! Opt-in strict real GPU width-profile acceptance, never a software-adapter success.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
fn device() -> (wgpu::Device, wgpu::Queue) {
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
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("width adapter: {info:?}");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}
fn parity(c: &mut Compositor, d: &wgpu::Device, q: &wgpu::Queue, doc: &Document) {
    let cpu = photocraft_compose::flatten(doc);
    let gpu = render_to_vec(c, d, q, doc, doc.bounds()).expect("GPU appearance plan must succeed");
    assert_eq!(cpu.px.len(), gpu.len());
    let max = cpu
        .px
        .iter()
        .zip(gpu)
        .flat_map(|(a, b)| {
            assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()), "nonfinite compositor pixel");
            (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs())
        })
        .fold(0.0_f32, f32::max);
    eprintln!("width premultiplied max diff: {max}");
    assert!(max <= 1. / 255., "{max}");
}

#[test]
#[ignore = "integration gate: real RTX 5090 Vulkan required; task speed rules prohibit execution"]
fn width_gpu_profile_strokes_preview_release_and_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let (device, queue) = device();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids, None).unwrap();
    g.preset("lens").unwrap();
    g.set_point(0, None, WidthPoint { t: 0.5, left: 18.0, right: 4.0 }).unwrap();
    let preview = preview_document(&s, g.request()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    parity(&mut compositor, &device, &queue, &preview);
    g.commit(&mut s).unwrap();
    parity(&mut compositor, &device, &queue, &s.active().unwrap().doc);
    assert_eq!(*s.active().unwrap().doc, preview);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    parity(&mut compositor, &device, &queue, &s.active().unwrap().doc);
}
