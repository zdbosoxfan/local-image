//! Real-GPU projection parity gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
fn live_paint42_rtx_5090_vulkan_float_preview_release_expand_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("live_paint42");
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
    assert_eq!(cpu.px.len(), gpu.len());
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
