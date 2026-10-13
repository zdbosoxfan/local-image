//! Real RTX gate. Adapter identity and float target support are mandatory, not skips.
use super::{TestResult, common};
use crate::{
    Image, Params,
    cleanup::{self, Options},
};
use photocraft_color::{Color, ColorMode, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, ShapeLayer, Size};

#[test]
fn cleanup_cleaned_shapes_rtx5090_vulkan_cpu_parity() -> TestResult {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN },
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }),
    )?;
    let info = adapter.get_info();
    eprintln!("cleanup GPU adapter: {info:?}");
    assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    #[cfg(target_os = "macos")]
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    #[cfg(not(target_os = "macos"))]
    assert_eq!(info.vendor, 0x10de);
    #[cfg(not(target_os = "macos"))]
    assert!(info.name.contains("RTX 5090"));
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float)?;
    for index in [0, 4, 5] {
        let case = common::case(index, 64, common::Variant::Clean)?;
        let result = cleanup::trace(Image { width: 64, height: 64, rgba: case.input.as_raw() }, &Params::default(), &Options::logo())?;
        let mut doc = Document::with_background("Cleanup parity", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let mut children = Vec::new();
        for layer in result.trace.layers {
            let [r, g, b, a] = layer.color;
            let mut sh = ShapeLayer {
                path: layer.path,
                fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
                ..Default::default()
            };
            sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
            children.push(Layer::new("Cleaned colour", LayerContent::Shape(sh)));
        }
        doc.layers.push(Layer::group("Vectorized", children));
        let cpu = photocraft_compose::flatten(&doc);
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &doc, doc.bounds())?;
        assert_eq!(cpu.px.len(), gpu.len());
        let worst = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[3] - b[3]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0., f32::max);
        eprintln!("cleanup case {index}: premultiplied max diff {worst}");
        assert!(worst <= 1. / 255.);
    }
    Ok(())
}
