use super::*;
use photocraft_doc::BlendMode;

fn shape(doc: &Document, name: &str, xywh: (f64, f64, f64, f64), color: Color) -> Layer {
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::rect(xywh.0, xywh.1, xywh.2, xywh.3), fill: Some(Fill::Solid(color)), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    Layer::new(name, LayerContent::Shape(shape))
}
fn fixture(mode: BlendMode) -> Document {
    let mut doc = Document::new("Native blends", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 96.;
    doc.layers.push(shape(&doc, "Backdrop", (0., 0., 256., 256.), Color::rgb(0.22, 0.53, 0.81)));
    for i in 0..20 {
        doc.layers.push(shape(
            &doc,
            "Retained vector",
            (8. + f64::from(i % 5) * 46., 8. + f64::from(i / 5) * 45., 32., 28.),
            Color::rgb(0.12 + (i % 3) as f32 * 0.3, 0.25, 0.65),
        ));
    }
    let mut text = photocraft_doc::TextLayer {
        text: "Vector type".into(),
        font_family: "Inter".into(),
        size_pt: 16.,
        color: Color::BLACK,
        transform: photocraft_doc::Affine::translate(30., 228.),
        ..Default::default()
    };
    photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
    doc.layers.push(Layer::new("Retained type", LayerContent::Text(text)));
    let mut color = Color::rgb(0.78, 0.18, 0.37);
    color.alpha = 0.8;
    let mut top = shape(&doc, "Blend", (25., 26., 190., 145.), color);
    top.blend = mode;
    top.opacity = 0.7;
    top.fill_opacity = 0.8;
    doc.layers.push(top);
    doc
}
const MODES: [BlendMode; 14] = [
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

#[test]
fn native_blends_preserve_vectors_text_and_rendering() {
    for mode in MODES {
        let doc = fixture(mode);
        assert_native(&doc, false, &format!("native-blend-{mode:?}"));
    }
}
#[test]
fn native_blends_preserve_group_isolation_and_pass_through() {
    let mut base = fixture(BlendMode::Multiply);
    let first = base.layers.pop().unwrap();
    let mut second = shape(&base, "Screen child", (90., 70., 145., 120.), Color::rgb(0.1, 0.72, 0.31));
    second.blend = BlendMode::Screen;
    let mut passthrough_pixels = None;
    for (mode, fill) in [(BlendMode::PassThrough, 1.), (BlendMode::Normal, 1.), (BlendMode::Multiply, 0.75), (BlendMode::PassThrough, 0.75)] {
        let mut doc = base.clone();
        let mut group = Layer::group("Blend group", vec![first.clone(), second.clone()]);
        group.blend = mode;
        group.fill_opacity = fill;
        if mode == BlendMode::Multiply {
            group.opacity = 0.65;
        }
        doc.layers.push(group);
        let pixels = photocraft_compose::flatten(&doc).to_rgba8().pixels;
        if mode == BlendMode::PassThrough && fill == 1. {
            passthrough_pixels = Some(pixels.clone());
        }
        if mode == BlendMode::Normal {
            assert_ne!(passthrough_pixels.as_ref().unwrap(), &pixels, "fixture must distinguish group isolation");
        }
        assert_native(&doc, false, &format!("native-group-{mode:?}-{fill}"));
    }
}
#[test]
fn native_blends_apply_to_raster_source_once() {
    for mode in [BlendMode::Multiply, BlendMode::Screen] {
        let mut doc = fixture(mode);
        let top = doc.layers.last_mut().unwrap();
        let LayerContent::Shape(shape) = &top.content else { panic!() };
        top.content = LayerContent::Raster(shape.cache.clone().unwrap());
        let mut mask = photocraft_doc::LayerMask::reveal_all();
        mask.density = 0.65;
        mask.surface.fill_rect(photocraft_doc::Rect::new(50, 20, 110, 180), &[0.]);
        top.mask = Some(mask);
        assert_native(&doc, true, &format!("native-raster-blend-{mode:?}"));
    }
}
#[test]
fn unsupported_compositions_keep_warned_visual_fallback() {
    for kind in ["soft-light", "linear-dodge", "blend-if", "adjustment", "clipping", "effect-blend"] {
        let mut doc = fixture(BlendMode::Multiply);
        let top = doc.layers.last_mut().unwrap();
        match kind {
            "soft-light" => top.blend = BlendMode::SoftLight,
            "linear-dodge" => top.blend = BlendMode::LinearDodge,
            "blend-if" => top.blend_if.set(0, [photocraft_doc::BlendRange::FULL, photocraft_doc::BlendRange { black: [60, 130], white: [200, 240] }]),
            "adjustment" => doc.layers.push(Layer::new("Invert", LayerContent::Adjustment(photocraft_doc::Adjustment::Invert))),
            "clipping" => top.clipped = true,
            "effect-blend" => top.effects.items.push(photocraft_doc::Effect::default_drop_shadow()),
            _ => unreachable!(),
        }
        assert_fallback(&doc, &format!("unsupported-{kind}"));
    }
}

fn assert_native(doc: &Document, raster: bool, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{label}: {:?}", out.warnings);
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(has_paths(&pdf));
    assert_eq!(images(&pdf) > 0, raster);
    let objects = format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>());
    assert!(objects.contains("FontFile2"), "editable text font remains: {label}");
    assert!(
        pdf.objects().into_iter().any(|o| matches!(o, Object::Dict(d) if d.get::<Name<'_>>(b"BM").is_some_and(|n| n.as_ref()!=b"Normal"))),
        "native blend resource: {label}"
    );
    let paths = streams(&pdf)
        .iter()
        .map(|(bytes, _)| {
            use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
            let mut iter = TypedIter::new(bytes);
            let mut count = 0;
            while let Some(op) = iter.next() {
                if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_)) {
                    count += 1;
                }
            }
            count
        })
        .sum::<usize>();
    assert!(paths >= 21, "many original vector paths remain: {label}");
    oracle(doc, label);
}
fn assert_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.feature == "compositing"), "{label}");
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(images(&pdf) > 0);
    assert!(!has_paths(&pdf));
    oracle(doc, label);
}
