//! Detached node preview and committed document pixels on the required real adapter.
//! Full-app capture/display routing is separately gated by direct_select_tests.
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Layer, SampleType, ShapeLayer, Size, Subpath};

#[test]
fn node_gpu_preview_release_cancel_native_undo_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("required real Vulkan adapter");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("node GPU adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut comp = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut doc = Document::new("Node GPU", Size::new(96, 96), ColorMode::Rgb, SampleType::U8);
    let mut targets = Vec::new();
    for (x, color) in [(8.0, Color::rgb(0.8, 0.2, 0.1)), (48.0, Color::rgb(0.1, 0.3, 0.9))] {
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 8.0), (x + 32.0, 8.0), (x + 32.0, 40.0), (x, 40.0)])]),
            fill: Some(Fill::Solid(color)),
            ..Default::default()
        };
        crate::vector_cmds::refresh_shape(&doc, &mut sh);
        let l = Layer::new("Curve", LayerContent::Shape(sh));
        targets.push(Target { name: "layer".into(), layer: Some(l.id.0), anchors: vec![[0, 2]] });
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let native = photocraft_format::save_to_bytes(&before, &Default::default()).unwrap();
    let op = Operation::Move { delta: [7.0, 15.0] };
    let shown = prepare(&before, &targets, &op, false).unwrap();
    fn parity(doc: &Document, comp: &mut photocraft_gpu::Compositor, device: &wgpu::Device, queue: &wgpu::Queue) {
        let cpu = photocraft_compose::flatten(doc);
        let gpu = photocraft_gpu::render_to_vec(comp, device, queue, doc, doc.bounds()).unwrap();
        let max = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |c| if c == 3 { (a[c] - b[c]).abs() } else { (a[c] * a[3] - b[c] * b[3]).abs() }))
            .fold(0.0_f32, f32::max);
        eprintln!("node GPU premultiplied max diff: {max}");
        assert!(max <= 1.0 / 255.0, "premultiplied max diff {max}");
    }
    parity(&shown.doc, &mut comp, &device, &queue);
    assert_eq!(s.active().unwrap().revision, revision);
    assert!(s.journal.is_empty());
    assert_eq!(s.active().unwrap().doc, before);
    // Dropping a preview is cancellation, with no engine edit to undo.
    drop(shown);
    assert!(!s.undo());
    s.execute("path.node.edit", json!({"document":before.id.0,"revision":revision,"targets":targets,"operation":op})).unwrap();
    parity(&s.active().unwrap().doc, &mut comp, &device, &queue);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(photocraft_format::save_to_bytes(&s.active().unwrap().doc, &Default::default()).unwrap(), native);
    parity(&s.active().unwrap().doc, &mut comp, &device, &queue);
}
