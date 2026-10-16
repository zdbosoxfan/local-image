//! Real resulting stroke caches through the production CPU/GPU compositor.
//! Full dialog/canvas acceptance belongs to 28; this gate never substitutes a software adapter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
use photocraft_vector::appearance::NoProjection;

#[test]
fn centerline_rtx5090_result_strokes_detached_preview_release_and_undo_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("centreline");
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, id, out) = tests::setup();
    let before = s.active().unwrap().doc.clone();
    let c = capture(&s, id, [8., 12.], 0.5).unwrap();
    let prepared = prepare(&s, &c, &out, &NoProjection, || false).unwrap();
    let shown = preview(&s, &prepared).unwrap();
    let parity = |compositor: &mut Compositor, doc: &Document| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = render_to_vec(compositor, &device, &queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| {
                assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()));
                (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() })
            })
            .fold(0f32, f32::max);
        eprintln!("centreline premultiplied max diff {max}");
        assert!(max <= 1. / 255.);
    };
    parity(&mut compositor, &shown);
    assert_eq!(s.active().unwrap().doc, before);
    publish(&mut s, prepared).unwrap();
    parity(&mut compositor, &s.active().unwrap().doc);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    parity(&mut compositor, &before);
}
