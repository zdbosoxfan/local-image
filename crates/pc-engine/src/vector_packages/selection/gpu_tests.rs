#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, ShapeLayer, Size, Subpath};
#[test]
fn selection_real_gpu_500_shapes_pixels_unchanged_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("selection GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut doc = Document::new("500 objects", Size::new(160, 200), ColorMode::Rgb, SampleType::U8);
    for i in 0..500 {
        let (x, y) = ((i % 20) as f64 * 8., (i / 20) as f64 * 8.);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x + 1., y + 1.), (x + 5., y + 1.), (x + 5., y + 5.), (x + 1., y + 5.)])]),
            fill: Some(Fill::Solid(Color::rgb(1., 0., 0.))),
            ..Default::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new("red", LayerContent::Shape(sh)));
    }
    let reference = doc.layers[0].id;
    let mut s = Session::new();
    s.add_document(doc, None);
    let st = s.active().unwrap();
    let before = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &st.doc, st.doc.bounds()).unwrap();
    let cpu = photocraft_compose::flatten(&st.doc);
    let r = Request {
        document: st.doc.id,
        revision: st.revision,
        scope: Scope::default(),
        mode: Mode::Replace,
        query: Query::Same { criterion: Same::FillColor, reference, compare: Compare::Equal, value: None, item: None },
    };
    assert_eq!(run(&mut s, &r).unwrap().layers.len(), 500);
    let st = s.active().unwrap();
    let after = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &st.doc, st.doc.bounds()).unwrap();
    assert_eq!(before, after);
    let max = cpu
        .px
        .iter()
        .zip(&after)
        .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
        .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "premultiplied max diff {max}");
}
