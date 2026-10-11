//! Real-GPU projection parity gate; not executed under package speed rules.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
#[ignore = "RTX 5090/Vulkan integrated live paint GPU gate; owner speed rules"]
fn live_paint42_rtx_5090_vulkan_float_preview_release_expand_parity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX adapter required");
    let info = adapter.get_info();
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("live_paint42 hardware {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = tests::fixture();
    let group = Gesture::begin(&s, &ids).unwrap().commit(&mut s, Action::Make { gap: Gap::None }).unwrap()[0];
    let before = s.active().unwrap().doc.clone();
    let count = s.journal.len();
    let g = Gesture::begin(&s, &[group]).unwrap();
    let action = Action::Paint {
        points: vec![[32., 40.], [76., 40.]],
        paint: Some(Fill::Solid(photocraft_doc::Color::rgb(0.7, 0.2, 0.4))),
        edges: false,
        width: 2.,
        tolerance: 4.,
    };
    let preview = g.preview(&s, action.clone()).unwrap();
    let cpu = photocraft_compose::flatten(&preview.display);
    let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &preview.display, preview.display.bounds()).unwrap();
    let max = cpu
        .px
        .iter()
        .zip(&gpu)
        .flat_map(|(a, b)| (0..4).map(move |c| (if c == 3 { a[c] - b[c] } else { a[c] * a[3] - b[c] * b[3] }).abs()))
        .fold(0f32, f32::max);
    assert!(max <= 1. / 255., "premultiplied max diff={max}");
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), count);
    g.commit(&mut s, action).unwrap();
    let display = evaluator::project_document(&s.active().unwrap().doc).unwrap();
    assert_eq!(photocraft_compose::flatten(&display).px, cpu.px);
    let g = Gesture::begin(&s, &[group]).unwrap();
    g.commit(&mut s, Action::Expand).unwrap();
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, cpu.px);
}
