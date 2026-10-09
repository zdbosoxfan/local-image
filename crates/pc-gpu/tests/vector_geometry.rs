//! Constructive geometry results retain CPU/GPU parity through the existing shape cache.
use photocraft_color::{Color, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, ShapeLayer, ShapeStroke, Size};
use photocraft_pathops as ops;

#[test]
fn constructive_vector_geometry_matches_cpu() {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    if photocraft_gpu::Compositor::preferred_acc_format(&adapter) != wgpu::TextureFormat::Rgba32Float {
        eprintln!("skipped: no Rgba32Float GPU target");
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut comp = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let a = photocraft_vector::shapes::ellipse(6.0, 6.0, 32.0, 32.0);
    let b = photocraft_vector::shapes::ellipse(18.0, 8.0, 32.0, 32.0);
    let paths = [
        ops::boolean(&a, &b, ops::BoolOp::Difference).unwrap(),
        ops::offset_path(&b, 2.0, ops::Join::Round, 4.0).unwrap(),
        ops::outline_stroke(&a, &ShapeStroke { width: 3.0, ..Default::default() }, 0.01).unwrap(),
    ];
    for path in paths {
        let mut doc = Document::new("V1 parity", Size::new(64, 48), ColorMode::Rgb, SampleType::F32);
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::rgba(0.7, 0.2, 0.1, 0.6))), ..Default::default() };
        sh.cache = Some(photocraft_vector::render_shape(&sh, PixelFormat::RGBA32F, doc.bounds()));
        doc.layers.push(Layer::new("Geometry", LayerContent::Shape(sh)));
        let cpu = photocraft_compose::flatten(&doc);
        let gpu = photocraft_gpu::render_to_vec(&mut comp, &device, &queue, &doc, doc.bounds()).unwrap();
        let worst = cpu
            .px
            .iter()
            .zip(gpu)
            .map(|(c, g)| (0..4).map(|i| if i == 3 { (c[i] - g[i]).abs() } else { (c[i] * c[3] - g[i] * g[3]).abs() }).fold(0.0, f32::max))
            .fold(0.0, f32::max);
        assert!(worst <= 1.0 / 255.0, "max premultiplied difference: {worst}");
    }
}
