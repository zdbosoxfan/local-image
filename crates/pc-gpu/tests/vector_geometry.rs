//! Constructive geometry results retain CPU/GPU parity through the existing shape cache.
use photocraft_color::{Color, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, ShapeLayer, ShapeStroke, Size};
use photocraft_pathops as ops;

#[test]
fn constructive_vector_geometry_matches_cpu() {
    let _lock = li_testkit::gpu::lock();
    let li_testkit::gpu::HardwareGpu { adapter, info, device, queue } = li_testkit::gpu::hardware_device("photocraft-gpu vector geometry");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float, "hardware adapter can't render Rgba32Float");
    if std::env::var_os("REQUIRE_VECTOR_GPU").is_some() {
        assert!(info.name.contains("RTX 5090") && info.device_type == wgpu::DeviceType::DiscreteGpu, "RTX 5090 required: {info:?}");
    }
    let mut comp = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let a = photocraft_vector::shapes::ellipse(6.0, 6.0, 32.0, 32.0);
    let b = photocraft_vector::shapes::ellipse(18.0, 8.0, 32.0, 32.0);
    let mut paths = vec![
        ops::boolean(&a, &b, ops::BoolOp::Union).unwrap(),
        ops::boolean(&a, &b, ops::BoolOp::Intersect).unwrap(),
        ops::boolean(&a, &b, ops::BoolOp::Xor).unwrap(),
        ops::boolean(&a, &b, ops::BoolOp::Difference).unwrap(),
        ops::offset_path(&b, 2.0, ops::Join::Round, 4.0).unwrap(),
        ops::outline_stroke(&a, &ShapeStroke { width: 3.0, ..Default::default() }, 0.01).unwrap(),
    ];
    paths.extend(
        ops::pathfinder(ops::PathfinderOp::Divide, &[ops::Shape::new(a.clone(), 0), ops::Shape::new(b.clone(), 1)]).unwrap().into_iter().map(|s| s.path),
    );
    paths.push(ops::simplify(&a, 0.1).unwrap());
    paths.push(ops::smooth(&a, 0.2).unwrap());
    paths.push(ops::reverse(&a).unwrap());
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
