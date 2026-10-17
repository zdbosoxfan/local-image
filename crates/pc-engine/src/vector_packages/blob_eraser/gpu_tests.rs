//! Real-GPU eraser parity gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
fn blob40_gpu_rtx5090_vulkan_preview_release_eraser_and_undo_pixels() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("blob40");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let check = |doc: &Document, c: &mut photocraft_gpu::Compositor| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(c, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(gpu.len(), cpu.px.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0_f32, f32::max);
        assert!(gpu.iter().flatten().all(|v| v.is_finite()));
        assert!(max <= 1. / 255., "premultiplied max diff={max}");
        gpu
    };
    let mut s = tests::session();
    let before = s.active().unwrap().doc.clone();
    let p = prepare(&s, &tests::request(&s, Mode::Blob, &[(20., 70.), (140., 70.)], 40.)).unwrap();
    let live = check(&p.document, &mut compositor);
    p.commit(&mut s).unwrap();
    assert_eq!(live, check(&s.active().unwrap().doc, &mut compositor));
    let p = prepare(&s, &tests::request(&s, Mode::Eraser, &[(80., 0.), (80., 150.)], 10.)).unwrap();
    let live = check(&p.document, &mut compositor);
    p.commit(&mut s).unwrap();
    assert_eq!(live, check(&s.active().unwrap().doc, &mut compositor));
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    check(&before, &mut compositor);
}
