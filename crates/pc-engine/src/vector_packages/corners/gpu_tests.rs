// Strict real-hardware projection/compositor gate. Full-app gesture gate is in the UI package.
use super::tests::{metadata, session, square};
use super::*;
use photocraft_doc::vector_design::drawing::CornerKind;
use photocraft_gpu::{Compositor, render_to_vec};
#[test]
fn corners_gpu_rounded_star_and_inverted_preview_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("RTX 5090 Vulkan is required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "wrong adapter: {info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("corners GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let star = photocraft_vector::shapes::polygon(14.0, 14.0, 100.0, 100.0, 5, 0.45);
    for (path, kind, count) in [(star, CornerKind::Round, 10), (square(), CornerKind::Inverted, 4)] {
        let (mut s, id) = session(path);
        let before = s.active().unwrap().doc.clone();
        let mut g = CornerGesture::begin(&s, &[id]).unwrap();
        g.set(id, metadata(kind, 12.0, count)).unwrap();
        let preview = g.preview_document().unwrap();
        assert_eq!(s.active().unwrap().doc, before);
        for shown in [preview.clone(), {
            g.commit(&mut s).unwrap();
            project_document(&s.active().unwrap().doc).unwrap()
        }] {
            let cpu = photocraft_compose::render(&shown, shown.bounds());
            let gpu = render_to_vec(&mut compositor, &device, &queue, &shown, shown.bounds()).unwrap();
            let mut max = 0.0f32;
            for (a, b) in cpu.px.iter().zip(&gpu) {
                for c in 0..4 {
                    let x = if c == 3 { a[c] } else { a[c] * a[3] };
                    let y = if c == 3 { b[c] } else { b[c] * b[3] };
                    max = max.max((x - y).abs());
                }
            }
            assert!(max <= 1.0 / 255.0, "premultiplied diff {max}");
            eprintln!("{kind:?} premultiplied max diff={max}");
        }
    }
}
