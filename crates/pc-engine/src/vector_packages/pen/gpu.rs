//! Strict real-GPU parity for detached curved/closed pen previews and atomic commit caches.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};

#[test]
fn pen_gpu_curved_closed_preview_commit_undo_parity_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
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
    assert!(info.name.contains("5090"), "RTX 5090 required: {info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("pen GPU adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = session();
    let mut g = begin(&s, Mode::Pen);
    g.release().unwrap();
    click(&mut g, p(100., 20.));
    g.press(p(80., 90.), Modifiers::default(), 4.).unwrap();
    g.motion(p(50., 90.), Modifiers::default()).unwrap();
    g.release().unwrap();
    for closed in [false, true] {
        if closed {
            g.press(p(20., 20.), Modifiers::default(), 4.).unwrap();
        }
        let preview = preview_document(&s, g.request()).unwrap();
        let cpu = photocraft_compose::flatten(&preview);
        let gpu = render_to_vec(&mut compositor, &device, &queue, &preview, preview.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0.0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied difference {max}");
        eprintln!("pen GPU closed={closed}, premultiplied max diff={max}");
        if closed {
            publish(&mut s, &g.finish().unwrap());
            let committed = &s.active().unwrap().doc;
            let after = render_to_vec(&mut compositor, &device, &queue, committed, committed.bounds()).unwrap();
            assert_eq!(gpu, after);
            assert!(s.undo());
            assert!(s.redo());
            assert_eq!(path(&s), g.path());
        }
    }
}
