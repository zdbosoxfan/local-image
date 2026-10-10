//! Strict offscreen real-RTX acceptance for detached preview, release, clipping and effects.
use super::*;
use photocraft_doc::{
    ShapeStroke,
    effects::{Effect, Shadow},
};
use photocraft_gpu::{Compositor, render_to_vec};

fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("appearance adapter: {info:?}");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}
fn parity(c: &mut Compositor, d: &wgpu::Device, q: &wgpu::Queue, doc: &Document) {
    let cpu = photocraft_compose::flatten(doc);
    let gpu = render_to_vec(c, d, q, doc, doc.bounds()).expect("GPU appearance plan must succeed");
    assert_eq!(cpu.px.len(), gpu.len());
    let max = cpu
        .px
        .iter()
        .zip(gpu)
        .flat_map(|(a, b)| {
            assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()), "nonfinite compositor pixel");
            (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs())
        })
        .fold(0.0_f32, f32::max);
    eprintln!("appearance premultiplied max diff: {max}");
    assert!(max <= 1. / 255., "{max}");
}
fn stack(g: &mut Gesture) {
    g.update(|a| {
        a.items = vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0.8, 0.2, 0.1)))),
            AppearanceItem::Fill(FillItem {
                paint: Fill::Solid(Color::rgb(0.1, 0.7, 0.2)),
                opacity: 0.4,
                blend: photocraft_doc::BlendMode::Multiply,
                visible: true,
            }),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 8., paint: Fill::Solid(Color::rgb(0., 0., 1.)), ..Default::default() },
                ..Default::default()
            }),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke {
                    width: 2.,
                    opacity: 0.8,
                    paint: Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false),
                    ..Default::default()
                },
                gradient_mode: photocraft_doc::vector_design::appearance::StrokeGradientMode::Along,
                ..Default::default()
            }),
        ];
        Ok(())
    })
    .unwrap();
}
#[test]
fn appearance_stacks_on_shapes_rtx5090_preview_release_clipping() {
    let (device, queue) = device();
    let mut c = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = setup();
    s.edit("clipping fixture", |doc, _| {
        let mut layer = Layer::raster("clip", doc.pixel_format());
        if let LayerContent::Raster(surface) = &mut layer.content {
            surface.fill_rect(doc.bounds(), &[0.1, 0.5, 0.9, 0.45]);
        }
        layer.clipped = true;
        doc.layers.insert(1, layer);
        Ok(())
    })
    .unwrap();
    let mut g = Gesture::begin(&s, &ids).unwrap();
    stack(&mut g);
    let preview = preview_document(&s, g.request()).unwrap();
    // Independent clipping oracle at a pixel covered only by the broad blue stroke.
    // The merged stack is the clipping base: clip alpha .45 blends over blue once.
    let actual = photocraft_compose::flatten(&preview).px[40 * 96 + 9];
    for (actual, expected) in actual.into_iter().zip([0.045, 0.225, 0.955, 1.0]) {
        assert!((actual - expected).abs() <= 2.0 / 255.0, "merged clip oracle {actual} vs {expected}");
    }
    parity(&mut c, &device, &queue, &preview);
    g.commit(&mut s).unwrap();
    parity(&mut c, &device, &queue, &s.active().unwrap().doc);
    assert_eq!(*s.active().unwrap().doc, preview);
    assert!(s.undo());
    assert!(s.redo());
    parity(&mut c, &device, &queue, &s.active().unwrap().doc);
}
#[test]
fn layer_effects_on_appearance_shapes_rtx5090_merged_alpha() {
    let (device, queue) = device();
    let mut c = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let (mut s, ids) = setup();
    s.edit("effect fixture", |doc, _| {
        doc.layer_mut(ids[0]).unwrap().effects.items.push(Effect::DropShadow(Shadow {
            common: photocraft_doc::effects::FxCommon::new(photocraft_doc::BlendMode::Multiply, 0.7),
            color: Color::BLACK,
            angle: 135.,
            use_global_light: false,
            distance: 4.,
            spread: 0.,
            size: 3.,
            contour: Default::default(),
            anti_alias: true,
            noise: 0.,
            knocks_out: true,
        }));
        Ok(())
    })
    .unwrap();
    let mut g = Gesture::begin(&s, &ids).unwrap();
    stack(&mut g);
    let preview = preview_document(&s, g.request()).unwrap();
    parity(&mut c, &device, &queue, &preview);
    g.commit(&mut s).unwrap();
    parity(&mut c, &device, &queue, &s.active().unwrap().doc);
    let mut g = Gesture::begin(&s, &ids).unwrap();
    g.update(|a| {
        a.items.clear();
        Ok(())
    })
    .unwrap();
    let empty = preview_document(&s, g.request()).unwrap();
    parity(&mut c, &device, &queue, &empty);
    assert!(photocraft_compose::flatten(&empty).px.iter().all(|p| p[3] == 0.));
}
