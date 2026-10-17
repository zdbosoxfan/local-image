//! Required RTX gate; unavailable/wrong adapters are failures, never skips.
use pc_trace::cutout::trace;
use pc_trace::{Image, Params};
type TestResult = Result<(), Box<dyn std::error::Error>>;
use photocraft_compose::scaled_view as cpu;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
#[test]
fn cutout_rtx5090_vulkan_scaled_mosaic_cpu_parity() -> TestResult {
    let _gpu_gate = li_testkit::gpu::lock();
    let (device, queue) = crate::gpu_test_support::compositor_device("cutout");
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float)?;
    let labels: Vec<_> = (0..32).flat_map(|y| (0..32).map(move |x| u32::from((x - 16i32).pow(2) + (y - 16i32).pow(2) < 100))).collect();
    let data: Vec<_> = labels.iter().flat_map(|&l| if l == 0 { [0, 20, 160, 255] } else { [40, 20, 160, 255] }).collect();
    let t = trace(Image { width: 32, height: 32, rgba: &data }, &Params::default())?;
    let mut doc = Document::new("Cutout RTX mosaic", Size::new(32, 32), ColorMode::Rgb, SampleType::F32);
    let mut children = Vec::new();
    for l in t.layers {
        let [r, g, b, a] = l.color;
        let mut sh = ShapeLayer {
            path: l.path,
            fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
            ..Default::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        children.push(Layer::new("colour", LayerContent::Shape(sh)));
    }
    let mut group = Layer::group("Vectorized", children);
    photocraft_doc::vector_design::mosaic::mark(&mut group)?;
    doc.layers.push(group);
    let before = doc.clone();
    let mut cache = cpu::ProjectionCache::default();
    for scale in [1., 2., 4., 8.] {
        let cpu = cpu::flatten_scaled(&doc, doc.bounds(), scale)?.buffer();
        let projected = cache.project_at(&doc, doc.bounds(), scale, &Default::default(), cpu::legacy_projector)?;
        let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &projected, cpu.rect)?;
        let worst = cpu
            .px
            .iter()
            .zip(gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[i] - b[i]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
            .fold(0., f32::max);
        assert!(worst <= 1. / 255., "scale {scale}: premultiplied max diff {worst}");
        let seam = cpu.px.iter().map(|p| (1. - p[3]).abs()).fold(0., f32::max);
        eprintln!("cutout scale {scale}: premultiplied max diff {worst}; source-over alpha deficit {seam}");
    }
    assert_eq!(doc, before, "display projection mutated persistent caches");
    Ok(())
}

/// Exact shared paths show no seams under per-layer source-over AA.
#[test]
fn cutout_composited_mosaic_has_no_antialias_seam() -> TestResult {
    let _gpu_gate = li_testkit::gpu::lock();
    let labels: Vec<_> = (0..32).flat_map(|y| (0..32).map(move |x| u32::from((x - 16i32).pow(2) + (y - 16i32).pow(2) < 100))).collect();
    let data: Vec<_> = labels.iter().flat_map(|&l| if l == 0 { [0, 20, 160, 255] } else { [200, 20, 40, 255] }).collect();
    let t = trace(Image { width: 32, height: 32, rgba: &data }, &Params::default())?;
    let mut doc = Document::new("seam gate", Size::new(32, 32), ColorMode::Rgb, SampleType::F32);
    for l in t.layers {
        let [r, g, b, a] = l.color;
        doc.layers.push(Layer::new(
            "colour",
            LayerContent::Shape(ShapeLayer {
                path: l.path,
                fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
                ..Default::default()
            }),
        ));
    }
    let mut group = Layer::group("Vectorized", std::mem::take(&mut doc.layers));
    photocraft_doc::vector_design::mosaic::mark(&mut group)?;
    doc.layers.push(group);
    let mut failures = Vec::new();
    for scale in [1., 2., 4., 8.] {
        let b = cpu::flatten_scaled(&doc, doc.bounds(), scale)?.buffer();
        let deficit = b.px.iter().map(|p| (1. - p[3]).abs()).fold(0., f32::max);
        eprintln!("required mosaic seam gate {scale}x: alpha deficit {deficit}");
        if deficit > 1. / 255. {
            failures.push((scale, deficit));
        }
    }
    assert!(failures.is_empty(), "source-over mosaic seam gate failed: {failures:?}");
    Ok(())
}
