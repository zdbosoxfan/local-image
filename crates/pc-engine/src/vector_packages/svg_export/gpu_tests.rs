//! Real RTX original vs public-import roundtrip gate, registered only after package 13.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::exchange::ImportOptions;
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
#[test]
fn svg_export_real_gpu_original_vs_reimport_premultiplied_gate() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090/Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("SVG export GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    for path in [photocraft_vector::shapes::rect(24., 24., 64., 64.), photocraft_vector::shapes::ellipse(24., 24., 64., 64.)] {
        let mut doc = Document::new("Roundtrip GPU", Size::new(128, 128), ColorMode::Rgb, SampleType::U8);
        doc.resolution_dpi = 96.;
        let mut shape = ShapeLayer { path, fill: Some(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), ..Default::default() };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        doc.layers.push(Layer::new("Curve", LayerContent::Shape(shape)));
        let bytes = photocraft_svg::export::export(&doc, &ExportScope::Document, &Default::default()).unwrap().bytes;
        let scene = photocraft_svg::import::import(&bytes, &ImportOptions { dpi: 96., ..Default::default() }).unwrap();
        let mut reimport = Document::new("Imported", scene.size, ColorMode::Rgb, SampleType::U8);
        reimport.layers = scene.layers;
        reimport.resolution_dpi = scene.resolution_dpi;
        let a = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &doc, doc.bounds()).unwrap();
        let b = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &reimport, reimport.bounds()).unwrap();
        assert_eq!(a.len(), b.len());
        let max = a
            .iter()
            .zip(&b)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0.0_f32, f32::max);
        assert!(max <= 1.0 / 255.0, "original vs imported premultiplied max {max}");
    }
}
