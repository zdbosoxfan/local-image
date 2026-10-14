//! Integration-only strict real hardware gate. No software fallback and no successful skip.
use super::*;
#[test]
#[ignore = "owner speed rules: RTX 5090 Vulkan acceptance at integration"]
fn clip35_gpu_raster_child_vector_parent_preview_commit_release_and_undo() {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device_adapter_limits("clip35");
    let mut comp = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let pixels = doc.layers[0].surface().unwrap().clone();
    doc.layers[0].content = LayerContent::Raster(pixels);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, &ids).unwrap();
    let preview = prepare(&s, &c, &Action::MakeClip).unwrap();
    fn check(doc: &Document, comp: &mut photocraft_gpu::Compositor, device: &wgpu::Device, queue: &wgpu::Queue) {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(comp, device, queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        // render_to_vec returns straight RGBA, like CPU flatten. Compare premultiplied values.
        let max = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(cpu, gpu)| {
                (0..4).map(move |k| {
                    assert!(gpu[k].is_finite());
                    (if k == 3 { gpu[k] - cpu[k] } else { gpu[k] * gpu[3] - cpu[k] * cpu[3] }).abs()
                })
            })
            .fold(0f32, f32::max);
        assert!(max <= 1. / 255. + 1e-7, "premultiplied GPU max diff={max}");
    }
    check(&preview.document, &mut comp, &device, &queue);
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    let gid = group(&mut s, &ids);
    check(&s.active().unwrap().doc, &mut comp, &device, &queue);
    command(&mut s, &[gid], "object.clippingMask.release");
    check(&s.active().unwrap().doc, &mut comp, &device, &queue);
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    check(&before, &mut comp, &device, &queue);
    assert_eq!(s.journal.len(), 2);
}
