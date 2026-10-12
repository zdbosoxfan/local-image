pub use photocraft_compose::{Buffer, cmyk_space, effects, fill_frame, gradient_fill, masks, pattern, render, render_tiled};
pub use photocraft_gpu::*;
pub use wgpu;
#[path = "../../pc-compose/src/scaled_view.rs"]
pub mod cpu;
#[path = "../src/scaled_view.rs"]
pub mod scaled;
use photocraft_doc::{
    Color, ColorMode, Document, Effect, Fill, Layer, LayerContent, LayerMask, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath, VectorMask,
};
use photocraft_geom::Rect;
use std::collections::HashMap;

#[test]
#[ignore = "required RTX 5090 Vulkan acceptance; see docs/GPU-VALIDATION.md"]
fn scaled_view_rtx5090_vulkan_mixed_masks_clips_effects_tiled_parity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("RTX Vulkan adapter required");
    let info = adapter.get_info();
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("scaled compositor GPU {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }))
        .expect("required GPU device");
    let mut d = Document::new("mixed scaled parity", Size::new(96, 80), ColorMode::Rgb, SampleType::U8);
    let shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(12.25, 13.5), (76.5, 15.25), (72.0, 63.25), (15.5, 62.25)])]),
        fill: Some(Fill::Solid(Color::rgba(0.1, 0.6, 0.9, 0.7))),
        stroke: Some(ShapeStroke { width: 1.0, ..Default::default() }),
        ..Default::default()
    };
    let mut base = Layer::new("vector", LayerContent::Shape(shape));
    base.effects.enabled = true;
    base.effects.items.push(Effect::default_drop_shadow());
    d.layers.push(base);
    let mut raster = Layer::raster("raster clipped to shape fill", d.pixel_format());
    raster.surface_mut().unwrap().fill_rect(Rect::new(4, 4, 89, 73), &[0.9, 0.1, 0.2, 0.5]);
    raster.clipped = true;
    let mut mask = LayerMask::reveal_all();
    mask.surface.fill_rect(Rect::new(10, 10, 40, 50), &[0.0]);
    mask.feather = 1.0;
    raster.mask = Some(mask);
    raster.vector_mask = Some(VectorMask::new(Path::new(vec![Subpath::polygon(&[(8.0, 5.0), (85.0, 5.0), (85.0, 69.0), (8.0, 69.0)])])));
    d.layers.push(raster);
    let before = d.clone();
    let mut comp = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    for s in [2.0, 4.0, 8.0] {
        let projected = cpu::ProjectionCache::default().project_at(&d, d.bounds(), s, &HashMap::new(), cpu::legacy_projector).unwrap();
        let rect = projected.bounds();
        let expected = cpu::flatten_scaled(&d, d.bounds(), s).unwrap().buffer();
        let actual = render_to_vec(&mut comp, &device, &queue, &projected, rect).unwrap();
        let check = |a: &[[f32; 4]], b: &[[f32; 4]]| {
            assert_eq!(a.len(), b.len());
            let mut max = 0.0f32;
            for (a, b) in a.iter().zip(b) {
                for c in 0..4 {
                    max = max.max((a[c] * if c == 3 { 1.0 } else { a[3] } - b[c] * if c == 3 { 1.0 } else { b[3] }).abs());
                }
            }
            eprintln!("scaled {s}x premultiplied max={max}");
            assert!(max <= 1.0 / 255.0, "{s}x premultiplied max={max}");
        };
        check(&actual, &expected.px);
        let mut stitched = Buffer::transparent(rect);
        for r in cpu::tile_rects(rect, 256).unwrap() {
            let pixels = render_to_vec(&mut comp, &device, &queue, &projected, r).unwrap();
            for y in r.y0..r.y1 {
                let o = (y - rect.y0) as usize * rect.width() as usize + (r.x0 - rect.x0) as usize;
                let i = (y - r.y0) as usize * r.width() as usize;
                stitched.px[o..o + r.width() as usize].copy_from_slice(&pixels[i..i + r.width() as usize]);
            }
        }
        check(&actual, &stitched.px);
    }
    assert_eq!(d, before);
    let mut hairline = Document::new("analytic hairline", Size::new(10, 10), ColorMode::Rgb, SampleType::U8);
    hairline.layers.push(Layer::new(
        "one pixel stroke",
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polyline(&[(4.5, 1.0), (4.5, 8.0)])]),
            stroke: Some(ShapeStroke { width: 1.0, ..Default::default() }),
            ..Default::default()
        }),
    ));
    let projected = cpu::ProjectionCache::default().project_at(&hairline, hairline.bounds(), 8.0, &HashMap::new(), cpu::legacy_projector).unwrap();
    let pixels = render_to_vec(&mut comp, &device, &queue, &projected, projected.bounds()).unwrap();
    for x in 0..80 {
        assert_eq!(pixels[32 * 80 + x][3], if (32..40).contains(&x) { 1.0 } else { 0.0 }, "GPU 800% hairline column {x}");
    }
}
