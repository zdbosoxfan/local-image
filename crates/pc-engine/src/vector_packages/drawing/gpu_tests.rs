//! Strict RTX 5090/Vulkan parity for every generated family, through the actual compositor.
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size};
#[test]
fn live_shapes_all_families_real_gpu_cpu_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("live shapes");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    for parameters in shape_tests::families() {
        let mut s = Session::new();
        s.add_document(Document::new("Shape parity", Size::new(160, 160), ColorMode::Rgb, SampleType::U8), None);
        let c = Capture::new(&s).unwrap();
        let design = DesignShape { rect: [10., 10., 100., 100.], angle: 0.2, parameters };
        s.execute("shape.design.create", json!({"document":c.document.0,"revision":c.revision,"design":design})).unwrap();
        let doc = &s.active().unwrap().doc;
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "{design:?}: premultiplied max diff {max}");
    }
}
