//! Integration-only strict GPU gate. Never accepts fallback/skipped adapters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
#[ignore = "owner speed rules: run real-GPU gates only at integration"]
fn styles38_gpu_rtx5090_preview_release_redefine_parity() {
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
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("styles38 adapter {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = tests::fixture();
    let st = s.active().unwrap();
    let r = capture(
        &s,
        ids[1..].to_vec(),
        Operation::Copy { sample: Box::new(sample(&st.doc, ids[0], None).unwrap()), attributes: Attributes::ALL, append: false },
    )
    .unwrap();
    let p = preview(&s, &r).unwrap();
    let before = st.doc.clone();
    let mut parity = |doc: &Document| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| {
                assert!(a.iter().chain(b).all(|v| v.is_finite()));
                (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() })
            })
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied max diff {max}");
    };
    parity(&p.document);
    assert_eq!(s.active().unwrap().doc, before);
    commit(&mut s, &r).unwrap();
    parity(&s.active().unwrap().doc);
    assert_eq!(*s.active().unwrap().doc, p.document);
    let hooks = tests::fixture_hooks();
    let r = capture(&s, vec![], Operation::New { source: ids[0], name: "GPU style".into() }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    let r = capture(&s, ids[1..].to_vec(), Operation::Apply { style: StyleId(1), add: false }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    s.edit("local fixture", |doc, _| {
        doc.layer_mut(ids[4]).unwrap().opacity = 0.25;
        doc.layer_mut(ids[0]).unwrap().opacity = 0.6;
        Ok(())
    })
    .unwrap();
    let r = capture(&s, vec![], Operation::Redefine { style: StyleId(1), source: ids[0] }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    parity(&s.active().unwrap().doc);
    assert_eq!(s.active().unwrap().doc.layer(ids[4]).unwrap().opacity, 0.25);
    assert!(s.undo());
    assert!(s.redo());
    parity(&s.active().unwrap().doc);
}
