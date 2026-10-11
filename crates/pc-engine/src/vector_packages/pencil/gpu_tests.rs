#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size};
use photocraft_gpu::{Compositor, render_to_vec};
#[test]
fn pencil_real_rtx5090_detached_preview_commit_cache_and_undo_parity() {
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
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("Pencil GPU hardware: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut comp = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut session = Session::new();
    session.add_document(
        Document::with_background("GPU Pencil", Size::new(160, 120), ColorMode::Rgb, SampleType::U8, photocraft_doc::Color::rgb(0.2, 0.3, 0.4)),
        None,
    );
    let before = session.active().unwrap().doc.clone();
    let capture = Capture::begin(&session, Operation::Create { above: None }, Options { fill: true, ..Default::default() }).unwrap();
    let samples: Vec<_> = (0..=96)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / 96.;
            Sample { point: Point::new(80. + 40. * t.cos(), 60. + 40. * t.sin()), pressure: 0.7 }
        })
        .collect();
    let preview = prepare(&session, &capture, &samples, true).unwrap();
    let cpu = photocraft_compose::flatten(&preview.document);
    let gpu = render_to_vec(&mut comp, &device, &queue, &preview.document, preview.document.bounds()).unwrap();
    let diff = |a: &[[f32; 4]], b: &[[f32; 4]]| {
        assert_eq!(a.len(), b.len());
        assert!(a.iter().chain(b).flatten().all(|v| v.is_finite()));
        a.iter()
            .zip(b)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0.0_f32, f32::max)
    };
    let max_diff = diff(&cpu.px, &gpu);
    eprintln!("Pencil GPU CPU/preview premultiplied max diff: {max_diff}");
    assert!(max_diff <= 1.0 / 255.0);
    preview.commit(&mut session).unwrap();
    let committed = render_to_vec(&mut comp, &device, &queue, &session.active().unwrap().doc, session.active().unwrap().doc.bounds()).unwrap();
    let max_diff = diff(&gpu, &committed);
    eprintln!("Pencil GPU preview/commit premultiplied max diff: {max_diff}");
    assert!(max_diff <= 1.0 / 255.0);
    assert!(session.undo());
    assert_eq!(session.active().unwrap().doc, before);
    let cpu = photocraft_compose::flatten(&before);
    let gpu = render_to_vec(&mut comp, &device, &queue, &before, before.bounds()).unwrap();
    assert!(diff(&cpu.px, &gpu) <= 1.0 / 255.0);
}
