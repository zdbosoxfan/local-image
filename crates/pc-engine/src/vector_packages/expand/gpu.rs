//! Strict compositor acceptance on a real GPU.
use super::*;
use photocraft_gpu::{Compositor, render_to_vec};
fn gpu() -> (wgpu::Device, wgpu::Queue) {
    crate::gpu_test_support::compositor_device("Object 36")
}
#[test]
fn object36_gpu_expand_four_paints_before_preview_release_undo() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = gpu();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, id) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = request(&s, &[id]);
    let preview = prepare(&s, &r).unwrap().document;
    let a = render_to_vec(&mut compositor, &device, &queue, &before, before.bounds()).expect("source appearance GPU must be supported");
    let b = render_to_vec(&mut compositor, &device, &queue, &preview, preview.bounds()).expect("expanded plain paths GPU must be supported");
    for (doc, pixels) in [(&*before, &a), (&preview, &b)] {
        let cpu = photocraft_compose::flatten(doc);
        assert_eq!(cpu.px.len(), pixels.len());
        let diff = cpu
            .px
            .iter()
            .zip(pixels)
            .flat_map(|(a, b)| {
                (0..4).map(move |c| {
                    assert!(a[c].is_finite() && b[c].is_finite());
                    if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }
                })
            })
            .fold(0., f32::max);
        assert!(diff <= 1. / 255., "CPU/GPU premultiplied max {diff}");
    }
    let diff = a
        .iter()
        .zip(&b)
        .flat_map(|(a, b)| (0..4).map(move |c| if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }))
        .fold(0., f32::max);
    assert!(diff <= 1. / 255., "before/after premultiplied max {diff}");
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    s.execute("object.expand", json!(r)).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().children().unwrap().len(), 4);
    assert!(max_diff(&s.active().unwrap().doc, &preview) <= 1. / 255.);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
