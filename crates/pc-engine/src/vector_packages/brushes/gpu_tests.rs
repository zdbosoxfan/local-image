#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Integration gate; intentionally not executed under the owner's speed rules.
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size};
#[test]
#[ignore = "package 17 speed rules: coordinator RTX integration gate"]
fn brush17_rtx5090_vulkan_preview_release_expanded_geometry_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("Vector Brush");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let check = |doc: &Document, c: &mut photocraft_gpu::Compositor| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(c, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        assert!(cpu.px.iter().chain(&gpu).flatten().all(|v| v.is_finite()));
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied max diff: {max}");
        gpu
    };
    for (_, brush) in geometry::presets() {
        let mut s = Session::new();
        s.add_document(Document::new("Brush RTX", Size::new(160, 120), ColorMode::Rgb, SampleType::U8), None);
        let before = s.active().unwrap().doc.clone();
        let c = Capture::begin(&s, Options { brush, width: 4., ..Default::default() }).unwrap();
        let samples: Vec<_> = (0..=32)
            .map(|i| Sample { point: photocraft_geom::Point::new(20. + i as f64 * 3.5, 60. + (i as f64 * 0.12).sin() * 20.), pressure: 0.2 + i as f64 / 40. })
            .collect();
        let p = prepare(&s, &c, &samples).unwrap();
        let preview = check(&p.document, &mut compositor);
        p.commit(&mut s).unwrap();
        assert_eq!(preview, check(&s.active().unwrap().doc, &mut compositor));
        let c = TargetCapture::begin(&s, vec![s.active().unwrap().active_layer.unwrap()]).unwrap();
        s.execute("vector.brush.expand", json!({"capture":c})).unwrap();
        let expanded = check(&s.active().unwrap().doc, &mut compositor);
        let max = preview
            .iter()
            .zip(expanded)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255.);
        assert!(s.undo());
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
