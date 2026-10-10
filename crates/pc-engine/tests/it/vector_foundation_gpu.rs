//! Real GPU gate for path-command cache publication and undo. No desktop window is opened.
use photocraft_doc::*;
use photocraft_engine::Session;
use photocraft_gpu::{Compositor, render_to_vec};
use serde_json::json;

#[test]
fn vector_foundation_transaction_cache_matches_real_gpu() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("real Vulkan GPU required by the vector task");
    let info = adapter.get_info();
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu, "software adapters do not satisfy this gate: {info:?}");
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert!(info.name.contains("5090"), "owner RTX 5090 required: {info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("vector foundation adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut comp = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = Session::new();
    let mut doc = Document::new("Vector GPU", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let path = Path::new(vec![Subpath::polygon(&[(1., 1.), (9., 1.), (9., 9.), (1., 9.)])]);
    let mut shape = ShapeLayer { path: path.clone(), fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
    photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
    let layer = Layer::new("Shape", LayerContent::Shape(shape));
    let id = layer.id;
    doc.layers.push(layer);
    s.add_document(doc, None);
    fn check(s: &Session, device: &wgpu::Device, queue: &wgpu::Queue, comp: &mut Compositor) {
        let doc = &s.active().unwrap().doc;
        let cpu = photocraft_compose::flatten(doc);
        let gpu = render_to_vec(comp, device, queue, doc, doc.bounds()).unwrap();
        assert_eq!(cpu.px.len(), gpu.len());
        let max = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0.0_f32, f32::max);
        assert!(max <= 1.0 / 255.0, "premultiplied max diff: {max}");
    }
    check(&s, &device, &queue, &mut comp);
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path.transform(&Affine::translate(12.,3.))}]})).unwrap();
    check(&s, &device, &queue, &mut comp);
    assert!(s.undo());
    check(&s, &device, &queue, &mut comp);
    assert!(s.redo());
    check(&s, &device, &queue, &mut comp);
}
