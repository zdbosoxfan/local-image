//! Coordinator verifies RTX 5090 parity; software adapters are logged separately.
use pc_trace::{Image, Params, trace};
use photocraft_color::{Color, ColorMode, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, ShapeLayer, Size};
#[test]
fn traced_shape_group_matches_cpu() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("skipped: no GPU adapter");
        return Ok(());
    };
    eprintln!("GPU parity adapter: {:?}", adapter.get_info());
    if photocraft_gpu::Compositor::preferred_acc_format(&adapter) != wgpu::TextureFormat::Rgba32Float {
        eprintln!("skipped: GPU adapter lacks Rgba32Float parity targets");
        return Ok(());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let mut compositor = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float)?;
    let mut pixels = vec![255; 48 * 48 * 4];
    for y in 6..42 {
        for x in 6..42 {
            let c = if x < 24 { [15, 65, 185, 255] } else { [195, 20, 45, 255] };
            let i = (y * 48 + x) * 4;
            pixels[i..i + 4].copy_from_slice(&c);
        }
    }
    for y in 16..32 {
        for x in 16..32 {
            pixels[(y * 48 + x) * 4..(y * 48 + x) * 4 + 4].fill(255);
        }
    }
    let out = trace(Image { width: 48, height: 48, rgba: &pixels }, &Params::default())?;
    let mut doc = Document::with_background("trace parity", Size::new(48, 48), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let mut children = Vec::new();
    for color in out.layers {
        let [r, g, b, a] = color.color;
        let mut sh = ShapeLayer {
            path: color.path,
            fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
            ..Default::default()
        };
        sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
        children.push(Layer::new("colour", LayerContent::Shape(sh)));
    }
    doc.layers.push(Layer::group("Vectorized", children));
    let cpu = photocraft_compose::flatten(&doc);
    let gpu = photocraft_gpu::render_to_vec(&mut compositor, &device, &queue, &doc, doc.bounds())?;
    let worst = cpu
        .px
        .iter()
        .zip(&gpu)
        .flat_map(|(a, b)| (0..4).map(move |i| if i == 3 { (a[3] - b[3]).abs() } else { (a[i] * a[3] - b[i] * b[3]).abs() }))
        .fold(0., f32::max);
    assert!(worst <= 1. / 255., "traced shape group differs by {worst}");
    Ok(())
}
