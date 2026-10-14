use super::*;
#[test]
fn symbol_instances_real_rtx_5090_vulkan_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("symbols");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = tests::fixture();
    let (master, ids) = tests::three(&mut s);
    // Master update must invalidate all dependent Surface revisions in the persistent GPU planner.
    for paint in [photocraft_doc::Color::rgba(0.8, 0.2, 0.1, 0.75), photocraft_doc::Color::BLACK, photocraft_doc::Color::WHITE] {
        let child = s.active().unwrap().doc.symbols[0].layers[0].id;
        let mut p = Capture::new(&s).unwrap().params();
        p["symbol"] = json!(master.0);
        p["edits"] = json!([{"layer":child.0,"fill":photocraft_doc::Fill::Solid(paint)}]);
        s.execute("symbol.editMaster", p).unwrap();
        let doc = &s.active().unwrap().doc;
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, doc, doc.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied diff {max}");
        assert!(gpu.iter().any(|p| p[3] > 0.5));
    }
    let mut p = Capture::new(&s).unwrap().params();
    p["layers"] = json!([ids[0].0]);
    s.execute("symbol.detach", p).unwrap();
    let doc = &s.active().unwrap().doc;
    let cpu = photocraft_compose::flatten(doc);
    let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, doc, doc.bounds()).unwrap();
    let max = cpu
        .px
        .iter()
        .zip(&gpu)
        .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
        .fold(0_f32, f32::max);
    assert!(max <= 1. / 255.);
}
