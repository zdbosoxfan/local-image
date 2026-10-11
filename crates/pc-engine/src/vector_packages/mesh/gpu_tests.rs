//! Integrated native mesh GPU parity gate, serialized with the engine's other GPU tests.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("mesh gate requires RTX 5090 Vulkan");
    let info = adapter.get_info();
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("mesh adapter: {info:?}");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}
fn parity(c: &mut Compositor, d: &wgpu::Device, q: &wgpu::Queue, doc: &Document) {
    let cpu = photocraft_compose::flatten(doc);
    let gpu = render_to_vec(c, d, q, doc, doc.bounds()).expect("mesh GPU plan required");
    assert_eq!(cpu.px.len(), gpu.len());
    let max = cpu
        .px
        .iter()
        .zip(gpu)
        .flat_map(|(a, b)| {
            assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()));
            (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs())
        })
        .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "premultiplied mesh max diff {max}");
}
#[test]
fn mesh43_mesh_paint_shapes_rtx5090_at_1x_and_09_scales_preview_release() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let (device, queue) = device();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = fixture();
    s.edit("circle fixture", |doc, _| {
        if let LayerContent::Shape(shape) = &mut doc.layer_mut(ids[0]).unwrap().content {
            shape.path = photocraft_vector::shapes::ellipse(5., 5., 90., 90.);
        }
        Ok(())
    })
    .unwrap();
    let original = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids, Some(&Options { rows: 2, cols: 2, ..Default::default() })).unwrap();
    g.update(|_, m| {
        m.set_color(0, [1., 0., 0., 1.], 0.5)?;
        m.set_color(2, [0., 1., 0., 1.], 1.)?;
        m.set_color(6, [0., 0., 1., 1.], 1.)?;
        m.set_color(8, [1., 1., 1., 1.], 1.)?;
        m.move_point(4, Some(mesh_paint::RIGHT), [72., 43.])
    })
    .unwrap();
    let shown = preview_document(&s, g.request()).unwrap();
    assert_eq!(s.active().unwrap().doc, original);
    let view = view_hooks().expect("09 scaled projection provider must be integrated");
    for scale in [1., 2., 4., 8.] {
        let scaled = (view.project)(&shown, scale).unwrap();
        parity(&mut compositor, &device, &queue, &scaled);
    }
    g.commit(&mut s).unwrap();
    assert_eq!(*s.active().unwrap().doc, shown);
    assert_eq!(s.journal.len(), 1);
    for scale in [1., 2., 4., 8.] {
        let scaled = (view.project)(&s.active().unwrap().doc, scale).unwrap();
        parity(&mut compositor, &device, &queue, &scaled);
    }
}
