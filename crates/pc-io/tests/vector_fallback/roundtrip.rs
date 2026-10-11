//! Package 00 registers this module in the IO test aggregate after its importer hook.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use photocraft_doc::vector_design::{appearance::*, text::*};
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, ShapeStroke, Size, SymbolInstance, TextLayer};
use photocraft_io::{ExportOptions, export, import, vector_map::fallback};
use photocraft_psd::PsdFile;

fn max_diff(a: &Document, b: &Document) -> f32 {
    let (a, b) = (photocraft_compose::flatten(a), photocraft_compose::flatten(b));
    assert_eq!(a.rect, b.rect);
    a.px.iter().zip(&b.px).flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs())).fold(0_f32, f32::max)
}

#[test]
fn bugfix_save_f03_plain_shape_separate_vector_mask_psd_psb_pixels_and_loss_notice() {
    let mut doc = Document::new("Masked shape", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::rect(0., 0., 32., 24.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let mut layer = Layer::new("Masked", LayerContent::Shape(shape));
    layer.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(0., 0., 16., 24.)));
    doc.layers.push(layer);
    for ext in ["psd", "psb"] {
        let out = export(&doc, ext, &Default::default()).unwrap();
        let back = import(&format!("masked.{ext}"), &out.bytes).unwrap().document;
        assert!(max_diff(&doc, &back) <= 1. / 255., "{ext} lost visible clipping");
        assert!(out.warnings.iter().any(|w| w.contains("independent shape vector masks")), "editability loss must be declared: {:?}", out.warnings);
        let psd = PsdFile::from_bytes(&out.bytes).unwrap();
        assert!(
            psd.layers()[0].block(b"vmsk").is_none() && psd.layers()[0].block(b"SoCo").is_none(),
            "the fallback must be compatible raster pixels in other PSD readers too"
        );
        assert!(matches!(back.layers[0].content, LayerContent::Raster(_)), "fallback is explicitly pixels");
        let second = export(&back, ext, &Default::default()).unwrap();
        let twice = import(&format!("masked.{ext}"), &second.bytes).unwrap().document;
        assert!(max_diff(&doc, &twice) <= 1. / 255.);
        assert!(matches!(doc.layers[0].content, LayerContent::Shape(_)));
        assert!(doc.layers[0].vector_mask.is_some(), "saving must leave the editable source intact");
    }
}

fn fixture(depth: SampleType) -> Document {
    let mut doc = Document::new("Advanced PSD", Size::new(96, 64), ColorMode::Rgb, depth);
    let mut appearance = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0.8, 0.15, 0.2)))),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 8., paint: Fill::Solid(Color::rgb(0., 0.3, 0.9)), ..Default::default() },
                ..Default::default()
            }),
            AppearanceItem::Fill(FillItem {
                paint: Fill::Solid(Color::rgb(0.1, 0.8, 0.2)),
                opacity: 0.5,
                blend: photocraft_doc::BlendMode::Multiply,
                visible: true,
            }),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 3., opacity: 0.7, paint: Fill::Solid(Color::rgb(1., 0.9, 0.3)), ..Default::default() },
                ..Default::default()
            }),
        ],
    };
    appearance.items.push(AppearanceItem::Fill(FillItem { visible: false, ..FillItem::new(Fill::Solid(Color::BLACK)) }));
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::ellipse(12.25, 10.5, 40., 36.), ..Default::default() };
    shape.cache = Some(
        photocraft_vector::appearance::CompiledAppearance::new(&shape, Some(&appearance), &[], doc.bounds())
            .unwrap()
            .render(doc.pixel_format(), doc.bounds())
            .unwrap(),
    );
    // Deliberately stale legacy mirrors, ensuring export projects the authoritative stack.
    shape.fill = Some(Fill::Solid(Color::WHITE));
    shape.stroke = Some(ShapeStroke::default());
    let mut layer = Layer::new("Multiple paints", LayerContent::Shape(shape));
    layer.vector.appearance = Some(appearance);
    doc.layers.push(layer);
    doc
}

#[test]
fn vector_fallback_integrated_psd_psb_all_depths_reopen_pixels_and_raw_blocks() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        let doc = fixture(depth);
        for extension in ["psd", "psb"] {
            let output = export(&doc, extension, &ExportOptions::default()).unwrap();
            assert_eq!(output.warnings.len(), 1);
            assert_eq!(output.warnings[0].matches("multiple strokes").count(), 1);
            let back = import("fallback.psd", &output.bytes).unwrap().document;
            assert!(matches!(back.layers[0].content, LayerContent::Raster(_)), "cache importer hook must be integrated");
            let diff = max_diff(&doc, &back);
            eprintln!("PSD fallback {extension} {depth:?} premultiplied max {diff:.9}");
            assert!(diff <= 1. / 255.);
            let before = PsdFile::from_bytes(&output.bytes).unwrap();
            let second = export(&back, extension, &Default::default()).unwrap();
            assert!(second.warnings.is_empty(), "the losses already happened on first export");
            let after = PsdFile::from_bytes(&second.bytes).unwrap();
            for block in &before.layers()[0].blocks {
                if [*b"liVF", *b"vmsk", *b"vsms", *b"vscg", *b"vstk", *b"vogk", *b"SoCo", *b"GdFl", *b"PtFl"].contains(&block.key) {
                    assert_eq!(after.layers()[0].block(&block.key).unwrap().data, block.data, "{:?}", block.key);
                }
            }
        }
    }
}

#[test]
fn vector_fallback_integrated_shape_user_mask_and_gradient_effect_preserve_pixels() {
    for mask in [true, false] {
        let mut d = fixture(SampleType::U8);
        if mask {
            d.layers[0].vector_mask = Some(photocraft_doc::VectorMask {
                path: photocraft_vector::shapes::rect(12., 10., 20., 36.),
                density: 0.65,
                feather: 0.5,
                enabled: true,
                linked: true,
            });
        } else {
            d.layers[0].effects.items.push(photocraft_doc::Effect::GradientOverlay {
                common: photocraft_doc::FxCommon::new(photocraft_doc::BlendMode::Normal, 1.),
                gradient: photocraft_doc::Gradient { angle: 30., ..Default::default() },
                dither: false,
            });
        }
        let out = export(&d, "psd", &Default::default()).unwrap();
        assert_eq!(out.warnings.len(), 1);
        let back = import("fallback.psd", &out.bytes).unwrap().document;
        let diff = max_diff(&d, &back);
        assert!(diff <= 1. / 255., "shape mask={mask}: premultiplied max {diff}");
        let second = export(&back, "psd", &Default::default()).unwrap();
        assert!(second.warnings.is_empty());
        assert!(max_diff(&back, &import("again.psd", &second.bytes).unwrap().document) <= 1. / 255.);
        if !mask {
            let mut moved = back.clone();
            moved.layers[0].content = LayerContent::Raster(moved.layers[0].surface().unwrap().translated(7, 5));
            let LayerContent::Shape(sh) = &mut d.layers[0].content else { panic!() };
            sh.path = sh.path.transform(&photocraft_doc::Affine::translate(7., 5.));
            sh.cache = sh.cache.as_ref().map(|s| s.translated(7, 5));
            assert!(max_diff(&d, &moved) <= 1. / 255., "effect frame moves with fallback pixels");
            let out = export(&moved, "psd", &Default::default()).unwrap();
            let again = import("moved.psd", &out.bytes).unwrap().document;
            assert!(max_diff(&moved, &again) <= 1. / 255., "moving invalidates the proxy without changing pixel blending");
        }
    }
}

#[test]
fn vector_fallback_integrated_path_text_symbol_proxy_caches_and_empty_stacks() {
    let mut doc = fixture(SampleType::U8);
    let cache = doc.layers[0].surface().unwrap().clone();
    // Injected native-cache fixtures exercise format fidelity; app_gpu uses real 12/15 providers.
    let mut text = Layer::new("Path type proxy", LayerContent::Text(TextLayer { text: "ABC".into(), cache: Some(cache.clone()), ..Default::default() }));
    text.vector.on_path =
        Some(PathText { path: photocraft_vector::shapes::ellipse(0., 0., 40., 40.), start: 0., end: 1., flip: true, align: PathTextAlign::Baseline });
    doc.layers.push(text);
    doc.layers.push(Layer::new(
        "Symbol proxy",
        LayerContent::Symbol(SymbolInstance { symbol: photocraft_doc::SymbolId(42), transform: Default::default(), cache: Some(cache) }),
    ));
    let output = export(&doc, "psd", &Default::default()).unwrap();
    assert_eq!(output.warnings.len(), 1);
    for loss in ["text on a path", "symbol definitions and instances", "multiple strokes"] {
        assert_eq!(output.warnings[0].matches(loss).count(), 1);
    }
    let back = import("fallback.psd", &output.bytes).unwrap().document;
    assert!(max_diff(&doc, &back) <= 1. / 255.);
    for hidden in [false, true] {
        let mut empty = fixture(SampleType::U8);
        let format = empty.pixel_format();
        let l = &mut empty.layers[0];
        l.vector.appearance = Some(Appearance::default());
        l.visible = !hidden;
        let LayerContent::Shape(s) = &mut l.content else { panic!() };
        s.cache = Some(photocraft_doc::Surface::new(format));
        let output = export(&empty, "psd", &Default::default()).unwrap();
        let back = import("empty.psd", &output.bytes).unwrap().document;
        assert_eq!(max_diff(&empty, &back), 0.);
    }
}

#[test]
fn vector_fallback_integrated_normal_save_refuses_missing_cache_before_bytes() {
    let mut doc = fixture(SampleType::U8);
    let LayerContent::Shape(s) = &mut doc.layers[0].content else { panic!() };
    s.cache = None;
    assert!(export(&doc, "psd", &Default::default()).is_err());
    assert!(fallback::export(&doc, &Default::default(), None).is_err());
}

#[test]
fn vector_fallback_integrated_profile_brush_proxy_uses_cache_and_warns_once() {
    let mut doc = fixture(SampleType::U8);
    let AppearanceItem::Stroke(stroke) = &mut doc.layers[0].vector.appearance.as_mut().unwrap().items[1] else { panic!() };
    stroke.profile = Some(WidthProfile { points: vec![WidthPoint { t: 0., left: 2., right: 5. }, WidthPoint { t: 1., left: 3., right: 1. }] });
    stroke.brush = Some(BrushRef { id: "injected-proxy".into() });
    stroke.start_arrow = Some(Arrowhead::Circle);
    stroke.dash_align = true;
    stroke.gradient_mode = StrokeGradientMode::Across;
    // The pre-existing prepared native cache is authoritative; feature rendering belongs to 11/17.
    let out = export(&doc, "psd", &Default::default()).unwrap();
    assert_eq!(out.warnings.len(), 1);
    for feature in ["width profiles", "vector brushes", "arrowheads", "dash alignment", "along/across stroke gradients"] {
        assert_eq!(out.warnings[0].matches(feature).count(), 1);
    }
    let back = import("fallback.psd", &out.bytes).unwrap().document;
    assert_eq!(max_diff(&doc, &back), 0.);
}

#[test]
fn vector_fallback_integrated_path_text_user_vector_mask_applies_once() {
    let mut doc = fixture(SampleType::U8);
    let mut text = Layer::new(
        "Masked path type",
        LayerContent::Text(TextLayer { text: "ABC".into(), cache: Some(doc.layers[0].surface().unwrap().clone()), ..Default::default() }),
    );
    text.vector.on_path =
        Some(PathText { path: photocraft_vector::shapes::ellipse(0., 0., 40., 40.), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline });
    text.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(10., 0., 20., 64.)));
    doc.layers = vec![text];
    let out = export(&doc, "psd", &Default::default()).unwrap();
    let back = import("masked.psd", &out.bytes).unwrap().document;
    assert!(back.layers[0].vector_mask.is_some());
    assert!(max_diff(&doc, &back) <= 1. / 255.);
    let second = export(&back, "psd", &Default::default()).unwrap();
    let second = import("masked.psd", &second.bytes).unwrap().document;
    assert!(max_diff(&doc, &second) <= 1. / 255.);
}
