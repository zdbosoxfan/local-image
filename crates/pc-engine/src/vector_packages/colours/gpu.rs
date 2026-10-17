//! Offscreen RTX gate supplied for the integration milestone, never run by this speed-limited
//! package job. Engine/cache parity is separate from 00's required full-app pointer-frame gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
#[test]
fn colour39_rtx5090_global_edit_preview_release_and_cancel_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("colour39");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut parity = |doc: &Document| {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, doc, doc.bounds()).expect("GPU colour plan must succeed");
        assert_eq!(cpu.px.len(), gpu.len());
        let diff = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(a, b)| {
                assert!(a.iter().chain(&b).all(|x| x.is_finite()));
                (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() })
            })
            .fold(0.0_f32, f32::max);
        assert!(diff <= 1. / 255., "premultiplied diff {diff} (adapter printed above)");
    };
    let (mut s, book) = fixture();
    let before = s.active().unwrap().doc.clone();
    parity(&before);
    let r = edit(&s, &book, Color::rgb(0.8, 0.2, 0.1));
    let preview = prepare(&s, test_access(), &r).unwrap();
    parity(&preview.document);
    assert_eq!(s.active().unwrap().doc, before);
    drop(preview);
    assert!(!s.active().unwrap().history.can_undo());
    commit(&mut s, test_access(), &r).unwrap();
    parity(&s.active().unwrap().doc);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    parity(&s.active().unwrap().doc);
}
