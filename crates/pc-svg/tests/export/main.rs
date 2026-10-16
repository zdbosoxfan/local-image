//! Synthetic GPL-3.0-or-later fixtures; see fixtures/export/SOURCES.md.
use base64::Engine as _;
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::vector_design::exchange::ExportScope;
use photocraft_doc::{Color, ColorMode, Document, Fill, GradientStyle, Layer, LayerContent, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
use photocraft_svg::export::{self, ExportError, ExportOptions};
fn scene(path: Path, fill: Fill) -> Document {
    let mut doc = Document::new("Synthetic", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 96.0;
    let mut shape = ShapeLayer { path, fill: Some(fill), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    doc.layers.push(Layer::new("A<&\"'\u{1}", LayerContent::Shape(shape)));
    doc
}
fn svg(doc: &Document) -> String {
    String::from_utf8(export::export(doc, &ExportScope::Document, &ExportOptions::default()).unwrap().bytes).unwrap()
}
fn xml(s: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(s).unwrap()
}
fn render(s: &str) -> Vec<[u8; 4]> {
    let opts = resvg::usvg::Options { dpi: 96.0, ..Default::default() };
    let tree = resvg::usvg::Tree::from_str(s, &opts).unwrap();
    let size = tree.size().to_int_size();
    let mut pix = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pix.as_mut());
    pix.data().as_chunks::<4>().0.to_vec()
}
mod oracle;
fn assert_oracle(doc: &Document, label: &str) {
    assert_oracle_svg(doc, &svg(doc), label);
}
fn assert_oracle_svg(doc: &Document, s: &str, label: &str) {
    let out = render(s);
    let reference = photocraft_compose::flatten(doc).to_rgba8();
    let mut delta = 0.0;
    for (a, b) in out.iter().zip(reference.pixels.as_chunks::<4>().0.iter()) {
        let white = |p: [u8; 4], premul: bool| -> [u8; 3] {
            std::array::from_fn(|i| {
                let v = if premul { u32::from(p[i]) } else { (u32::from(p[i]) * u32::from(p[3]) + 127) / 255 };
                (v + 255 - u32::from(p[3])).min(255) as u8
            })
        };
        let x = white(*a, true);
        let y = white([b[0], b[1], b[2], b[3]], false);
        let x = photocraft_testkit::vector::oklab(x);
        let y = photocraft_testkit::vector::oklab(y);
        delta += x.iter().zip(y).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt() * 100.0;
    }
    let iou = oracle::coverage_iou(doc, s);
    let mean = delta / out.len() as f64;
    eprintln!("{label}: IoU={iou:.8} mean DeltaE_ok={mean:.6}");
    assert!(iou > 0.995, "{label}: IoU {iou}");
    assert!(mean < 0.5, "{label}: DeltaE {mean}");
}
#[test]
fn svg_export_resvg_independent_geometry_and_gradient_oracles() {
    for (label, path) in
        [("rect", photocraft_vector::shapes::rect(16.0, 20.0, 224.0, 216.0)), ("ellipse", photocraft_vector::shapes::ellipse(16.0, 20.0, 224.0, 216.0))]
    {
        let doc = scene(path.clone(), Fill::Solid(Color::rgb(0.25, 0.5, 0.75)));
        assert_oracle(&doc, label);
        for style in [GradientStyle::Linear, GradientStyle::Reflected, GradientStyle::Radial] {
            for angle in [0.0, 31.0, 90.0] {
                let fill = Fill::Gradient {
                    stops: vec![(1.0, Color::WHITE), (0.0, Color::rgb(0.1, 0.2, 0.4))],
                    style,
                    angle,
                    scale: 0.8,
                    reverse: true,
                    opacity_stops: vec![(0.0, 0.5), (1.0, 1.0)],
                    midpoints: vec![0.3],
                    offset: (0.1, -0.05),
                    dither: false,
                    align: true,
                };
                assert_oracle(&scene(path.clone(), fill), &format!("{label} {style:?} {angle}"));
            }
        }
    }
}
#[test]
fn svg_export_xml_escape_precision_minify_and_stability() {
    let mut doc = scene(photocraft_vector::shapes::rect(-0.00001, 20.125, 120.5, 60.0), Fill::Solid(Color::BLACK));
    let scope = ExportScope::Document;
    for minify in [false, true] {
        let options = ExportOptions { minify, precision: 3, ..Default::default() };
        let a = export::export(&doc, &scope, &options).unwrap();
        let b = export::export(&doc, &scope, &options).unwrap();
        assert_eq!(a, b);
        let s = String::from_utf8(a.bytes).unwrap();
        let parsed = xml(&s);
        assert_eq!(parsed.descendants().find(|n| n.has_tag_name("title")).unwrap().text(), Some("A<&\"'�"));
        assert_eq!(s.contains('\n'), !minify);
        assert!(!s.contains("-0 "));
        assert!(!s.contains("NaN"));
    }
    let before = svg(&doc);
    doc.layers[0].id = photocraft_doc::LayerId(123456);
    assert_eq!(before, svg(&doc));
}
#[test]
fn svg_export_absolute_dash_zero_width_and_closepath() {
    let mut doc = scene(Path::new(vec![Subpath::polyline(&[(12., 24.), (220., 24.)])]), Fill::Solid(Color::BLACK));
    if let LayerContent::Shape(s) = &mut doc.layers[0].content {
        s.fill = None;
        s.stroke = Some(ShapeStroke { width: 3.0, dashes: vec![2.0, 1.0], dash_offset: -1.5, ..Default::default() });
    }
    let out = svg(&doc);
    let x = xml(&out);
    let p = x.descendants().find(|n| n.has_tag_name("path")).unwrap();
    assert_eq!(p.attribute("stroke-dasharray"), Some("6 3"));
    assert_eq!(p.attribute("stroke-dashoffset"), Some("-4.5"));
    assert!(!p.attribute("d").unwrap().contains('Z'));
    if let LayerContent::Shape(s) = &mut doc.layers[0].content {
        s.stroke.as_mut().unwrap().width = 0.0;
    }
    assert_eq!(xml(&svg(&doc)).descendants().filter(|n| n.has_tag_name("path")).count(), 0);
    let mut path = Path::new(vec![Subpath::polygon(&[(20., 20.), (220., 20.), (220., 220.)])]);
    path.subpaths[0].knots[2].out_ctrl = photocraft_geom::Point::new(50., 250.);
    path.subpaths[0].knots[0].in_ctrl = photocraft_geom::Point::new(5., 100.);
    let doc = scene(path.clone(), Fill::Solid(Color::BLACK));
    let out = svg(&doc);
    assert!(out.contains("C50 250 5 100 20 20 Z"));
    assert_oracle(&doc, "closing cubic");
    path.subpaths[0].closed = false;
    let out = svg(&scene(path, Fill::Solid(Color::BLACK)));
    assert!(!out.contains("C50 250"));
    assert!(!out.contains(" Z"));
}
#[test]
fn svg_export_selection_artboard_and_physical_units() {
    let child = scene(photocraft_vector::shapes::rect(24., 40., 48., 64.), Fill::Solid(Color::BLACK)).layers.remove(0);
    let id = child.id;
    let mut group = Layer::group("Board", vec![child]);
    if let LayerContent::Group(g) = &mut group.content {
        g.artboard = Some(photocraft_doc::Artboard::new(photocraft_doc::Rect::new(16, 24, 112, 120)));
    }
    let board = group.id;
    let mut doc = Document::new("Units", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 96.;
    doc.layers.push(group);
    for (units, width) in [
        (photocraft_doc::DesignUnits::Pixels, "96"),
        (photocraft_doc::DesignUnits::Points, "72pt"),
        (photocraft_doc::DesignUnits::Millimeters, "25.4mm"),
        (photocraft_doc::DesignUnits::Inches, "1in"),
    ] {
        doc.design_units = units;
        let out = export::export(&doc, &ExportScope::Artboard(board), &Default::default()).unwrap();
        let s = String::from_utf8(out.bytes).unwrap();
        let p = xml(&s);
        assert_eq!(p.root_element().attribute("width"), Some(width));
        assert_eq!(p.root_element().attribute("viewBox"), Some("16 24 96 96"));
        assert!(s.contains("clipPathUnits=\"userSpaceOnUse\""));
    }
    let out = export::export(&doc, &ExportScope::Selection(vec![id]), &Default::default()).unwrap();
    let s = String::from_utf8(out.bytes).unwrap();
    assert_eq!(xml(&s).root_element().attribute("viewBox"), Some("24 40 48 64"));
    // A valid portable appearance is exported from current geometry, not a stale
    // render cache (here still the original child rectangle).
    let selected = doc.layer_mut(id).unwrap();
    selected.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::BLACK)))] });
    if let LayerContent::Shape(shape) = &mut selected.content {
        shape.path = photocraft_vector::shapes::rect(32., 48., 24., 32.);
    }
    let out = export::export(&doc, &ExportScope::Selection(vec![id]), &Default::default()).unwrap();
    let source = String::from_utf8(out.bytes).unwrap();
    assert_eq!(xml(&source).root_element().attribute("viewBox"), Some("32 48 24 32"));
    let outside = scene(photocraft_vector::shapes::rect(-48., -64., 32., 48.), Fill::Solid(Color::BLACK));
    let out = export::export(&outside, &ExportScope::Selection(vec![outside.layers[0].id]), &Default::default()).unwrap();
    let out = String::from_utf8(out.bytes).unwrap();
    assert_eq!(xml(&out).root_element().attribute("viewBox"), Some("-48 -64 32 48"));
    assert!(export::export(&doc, &ExportScope::Selection(vec![id, id]), &Default::default()).is_err());
    assert!(export::export(&doc, &ExportScope::Artboard(id), &Default::default()).is_err());
    assert!(export::export(&doc, &ExportScope::Selection(vec![id, photocraft_doc::LayerId(0)]), &Default::default()).is_err());
}
#[test]
fn svg_export_appearance_stack_order_and_empty_stack() {
    let mut doc = scene(photocraft_vector::shapes::rect(10., 10., 220., 220.), Fill::Solid(Color::BLACK));
    doc.layers[0].vector.appearance = Some(Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
            AppearanceItem::Stroke(StrokeItem::default()),
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0., 0., 1.)))),
        ],
    });
    let s = svg(&doc);
    let p = xml(&s);
    let paths = p.descendants().filter(|n| n.has_tag_name("path")).collect::<Vec<_>>();
    assert_eq!(paths[0].attribute("fill"), Some("#ff0000"));
    assert_eq!(paths[2].attribute("fill"), Some("#0000ff"));
    doc.layers[0].vector.appearance = Some(Appearance::default());
    assert_eq!(xml(&svg(&doc)).descendants().filter(|n| n.has_tag_name("path")).count(), 0);
}
#[test]
fn svg_export_clip_defs_are_stable_and_deduplicated() {
    let mut doc = scene(photocraft_vector::shapes::rect(10., 10., 220., 220.), Fill::Solid(Color::BLACK));
    let clip = photocraft_vector::shapes::rect(40., 50., 120., 100.);
    doc.layers[0].vector.clip = Some(clip.clone());
    let mut second = doc.layers[0].clone();
    second.id = photocraft_doc::LayerId::fresh();
    doc.layers.push(second);
    let s = svg(&doc);
    assert_eq!(xml(&s).descendants().filter(|n| n.has_tag_name("clipPath")).count(), 1);
    let p = render(&s);
    assert_eq!(p[0], [0; 4]);
    assert!(p[60 * 256 + 60][3] > 250);
    assert_eq!(p[60 * 256 + 180][3], 0);
}
#[test]
fn svg_export_png_fallback_preserves_pixels_and_aggregates_warnings() {
    let mut doc = scene(
        photocraft_vector::shapes::rect(10., 10., 220., 220.),
        Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 20., 1., GradientStyle::Angle, false),
    );
    let mut other = doc.layers[0].clone();
    other.id = photocraft_doc::LayerId::fresh();
    doc.layers.push(other);
    let output = export::export(&doc, &ExportScope::Document, &Default::default()).unwrap();
    assert_eq!(output.warnings.iter().filter(|w| w.feature == "paint").count(), 1);
    let s = String::from_utf8(output.bytes).unwrap();
    let p = xml(&s);
    let imgs = p.descendants().filter(|n| n.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(imgs.len(), 2);
    let href = imgs[0].attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
    let original = photocraft_compose::flatten(&scene(
        photocraft_vector::shapes::rect(10., 10., 220., 220.),
        Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 20., 1., GradientStyle::Angle, false),
    ))
    .to_rgba8();
    let x: usize = imgs[0].attribute("x").unwrap().parse().unwrap();
    let y: usize = imgs[0].attribute("y").unwrap().parse().unwrap();
    assert_eq!(decoded.width(), 220);
    assert_eq!(decoded.height(), 220);
    let mut reconstructed = vec![0u8; original.pixels.len()];
    for row in 0..decoded.height() as usize {
        let target = ((y + row) * 256 + x) * 4;
        let source = row * decoded.width() as usize * 4;
        reconstructed[target..target + decoded.width() as usize * 4].copy_from_slice(&decoded.as_raw()[source..source + decoded.width() as usize * 4]);
    }
    assert_eq!(reconstructed, original.pixels, "cropped embedding preserves every original pixel");
}
#[test]
fn svg_export_text_curves_use_pc_text_and_preserve_style_colors() {
    let mut doc = Document::new("Type", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let t = photocraft_doc::TextLayer {
        text: "Hi".into(),
        font_family: "Inter".into(),
        size_pt: 48.,
        transform: photocraft_doc::Affine::translate(20., 100.),
        runs: vec![
            photocraft_doc::text::TextRun {
                len: 1,
                style: photocraft_doc::text::CharStyle { color: Color::rgb(1., 0., 0.), size_pt: 48., ..Default::default() },
            },
            photocraft_doc::text::TextRun {
                len: 1,
                style: photocraft_doc::text::CharStyle { color: Color::rgb(0., 0., 1.), size_pt: 48., ..Default::default() },
            },
        ],
        ..Default::default()
    };
    let mut t = t;
    photocraft_text::TextEngine::new().render_layer(&mut t, doc.resolution_dpi, doc.pixel_format());
    doc.layers.push(Layer::new("Type", LayerContent::Text(t)));
    let out = export::export(&doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap();
    let s = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&s);
    assert!(!parsed.descendants().any(|n| n.has_tag_name("text")));
    assert!(s.contains("#ff0000"));
    assert!(s.contains("#0000ff"));
    assert!(render(&s).iter().any(|p| p[3] > 0));
    assert_oracle_svg(&doc, &s, "multicolor text outlines");
    let s = svg(&doc);
    assert!(xml(&s).descendants().any(|n| n.has_tag_name("text")));
}
#[test]
fn svg_export_rejects_nonfinite_budget_and_missing_projection() {
    let mut doc = scene(photocraft_vector::shapes::rect(10., 10., 20., 20.), Fill::Solid(Color::BLACK));
    let options = ExportOptions { precision: 10, ..Default::default() };
    assert!(export::export(&doc, &ExportScope::Document, &options).is_err());
    let options = ExportOptions { max_pixels: 10, ..Default::default() };
    assert!(matches!(export::export(&doc, &ExportScope::Document, &options), Err(ExportError::Budget(_))));
    if let LayerContent::Shape(s) = &mut doc.layers[0].content {
        s.path.subpaths[0].knots[0].anchor.x = f64::NAN;
    }
    assert!(export::export(&doc, &ExportScope::Document, &Default::default()).is_err());
    let mut fonts = photocraft_text::TextEngine::new();
    let mut cx = export::ExportContext { cancelled: &|| true, projection: None, text: &mut fonts };
    assert!(matches!(export::export_with(&doc, &ExportScope::Document, &Default::default(), &mut cx), Err(ExportError::Cancelled)));
}
use proptest::prelude::*;
proptest! {
    #[test]
    fn svg_export_finite_rectangles_are_xml_and_deterministic(x in -10.0_f64..100.0,y in -10.0_f64..100.0,w in 0.01_f64..100.0,h in 0.01_f64..100.0,precision in 0_u8..10) {
        let doc=scene(photocraft_vector::shapes::rect(x,y,w,h),Fill::Solid(Color::BLACK)); let options=ExportOptions { precision,..Default::default() };
        let a=export::export(&doc,&ExportScope::Document,&options).unwrap(); let b=export::export(&doc,&ExportScope::Document,&options).unwrap(); prop_assert_eq!(&a,&b); let s=String::from_utf8(a.bytes).unwrap(); prop_assert!(roxmltree::Document::parse(&s).is_ok());
    }
}
#[cfg(feature = "svg-import-integrated")]
mod roundtrip;

#[test]
fn svg_export_dashed_stroke_resvg_gate() {
    let mut doc = scene(Path::new(vec![Subpath::polyline(&[(24., 64.), (232., 64.)])]), Fill::Solid(Color::BLACK));
    let format = doc.pixel_format();
    let canvas = doc.bounds();
    if let LayerContent::Shape(s) = &mut doc.layers[0].content {
        s.fill = None;
        s.stroke = Some(ShapeStroke { width: 8., dashes: vec![2., 1.], dash_offset: 0.5, ..Default::default() });
        s.cache = Some(photocraft_vector::render_shape(s, format, canvas));
    }
    assert_oracle(&doc, "absolute dashed stroke");
}
#[test]
fn svg_export_stacked_appearance_independent_layer_render() {
    let path = photocraft_vector::shapes::rect(24., 24., 208., 208.);
    let items = vec![
        AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
        AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 12., paint: Fill::Solid(Color::rgb(0., 1., 0.)), ..Default::default() },
            ..Default::default()
        }),
        AppearanceItem::Fill(FillItem { paint: Fill::Solid(Color::rgb(0., 0., 1.)), opacity: 0.5, ..FillItem::new(Fill::Solid(Color::BLACK)) }),
        AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width: 4., paint: Fill::Solid(Color::BLACK), ..Default::default() }, ..Default::default() }),
    ];
    let mut doc = scene(path.clone(), Fill::Solid(Color::BLACK));
    doc.layers[0].vector.appearance = Some(Appearance { items: items.clone() });
    let mut reference = doc.clone();
    reference.layers.clear();
    for item in items {
        let (fill, stroke, opacity) = match item {
            AppearanceItem::Fill(f) => (Some(f.paint), None, f.opacity),
            AppearanceItem::Stroke(s) => (None, Some(s.base), 1.),
            AppearanceItem::Mesh(_) => panic!("fixture contains only fills and strokes"),
        };
        let mut shape = ShapeLayer { path: path.clone(), fill, stroke, ..Default::default() };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        let mut layer = Layer::new("paint", LayerContent::Shape(shape));
        layer.opacity = opacity;
        reference.layers.push(layer);
    }
    assert_oracle_svg(&reference, &svg(&doc), "appearance vs independent layers");
}

#[test]
fn svg_export_projection_fixture_is_detached_and_required() {
    use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
    let mut doc = scene(photocraft_vector::shapes::rect(24., 24., 208., 208.), Fill::Solid(Color::BLACK));
    doc.layers[0].vector.corners.push(KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 10., kind: CornerKind::Round } });
    assert!(matches!(export::export(&doc, &ExportScope::Document, &Default::default()), Err(ExportError::Projection("corners"))));
    struct Fixture;
    impl export::Projection for Fixture {
        fn layer(&self, _: &Document, layer: &Layer) -> export::Result<Option<Layer>> {
            let mut layer = layer.clone();
            layer.vector.corners.clear();
            if let LayerContent::Shape(s) = &mut layer.content {
                s.path = photocraft_vector::shapes::ellipse(24., 24., 208., 208.);
            }
            Ok(Some(layer))
        }
    }
    let before = doc.clone();
    let mut fonts = photocraft_text::TextEngine::new();
    let mut cx = export::ExportContext { cancelled: &|| false, projection: Some(&Fixture), text: &mut fonts };
    let out = export::export_with(&doc, &ExportScope::Document, &Default::default(), &mut cx).unwrap();
    assert_eq!(doc, before);
    let pixels = render(std::str::from_utf8(&out.bytes).unwrap());
    assert_eq!(pixels[24 * 256 + 24][3], 0);
    assert!(pixels[128 * 256 + 128][3] > 250);
}

#[test]
fn svg_export_boolean_fill_and_evenodd_resvg_gate() {
    use photocraft_doc::{FillRule, PathOp};
    let outer = Subpath::polygon(&[(24., 24.), (232., 24.), (232., 232.), (24., 232.)]);
    let inner = Subpath::polygon(&[(64., 64.), (192., 64.), (192., 192.), (64., 192.)]);
    for (op, rule, inverted) in
        [(PathOp::Subtract, FillRule::NonZero, false), (PathOp::Join, FillRule::EvenOdd, false), (PathOp::Subtract, FillRule::NonZero, true)]
    {
        let path = Path { subpaths: vec![outer.clone(), inner.clone().with_op(op)], fill_rule: rule, inverted };
        let doc = scene(path, Fill::Solid(Color::BLACK));
        assert_oracle(&doc, &format!("boolean {op:?} {rule:?} inverted={inverted}"));
    }
}
#[test]
fn svg_export_hard_stops_and_color_alpha_resvg_gate() {
    let mut a = Color::rgb(1., 0., 0.);
    a.alpha = 0.6;
    let mut b = Color::rgb(0., 0., 1.);
    b.alpha = 0.9;
    let paint = Fill::Gradient {
        stops: vec![(0., a), (0.5, a), (0.5, b), (1., b)],
        style: GradientStyle::Linear,
        angle: 31.,
        scale: 0.8,
        reverse: false,
        opacity_stops: vec![(0., 0.7), (1., 1.)],
        midpoints: vec![0.5, 0.5, 0.5],
        offset: (0.05, -0.05),
        dither: false,
        align: false,
    };
    assert_oracle(&scene(photocraft_vector::shapes::rect(16.25, 20.75, 224., 216.), paint), "hard stops with independent alpha");
}

#[test]
fn svg_export_translucent_text_decorations_share_native_color_coverage() {
    let mut doc = Document::new("Decorated type", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let mut color = Color::BLACK;
    color.alpha = 0.6;
    let mut t = photocraft_doc::TextLayer {
        text: "HH".into(),
        size_pt: 48.,
        color,
        transform: photocraft_doc::Affine::translate(20., 100.),
        runs: vec![photocraft_doc::text::TextRun {
            len: 2,
            style: photocraft_doc::text::CharStyle { size_pt: 48., color, strikethrough: true, ..Default::default() },
        }],
        ..Default::default()
    };
    photocraft_text::TextEngine::new().render_layer(&mut t, doc.resolution_dpi, doc.pixel_format());
    doc.layers.push(Layer::new("Type", LayerContent::Text(t)));
    let bytes = export::export(&doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap().bytes;
    let s = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(xml(s).descendants().filter(|n| n.has_tag_name("path")).count(), 1);
    assert!(!xml(s).descendants().any(|n| n.has_tag_name("rect")));
    let actual = render(s);
    let expected = photocraft_compose::flatten(&doc).to_rgba8();
    let mut checked = 0;
    // Inspect fully covered interiors independently of the renderers' different AA
    // kernels; this includes the glyph/strikethrough overlap that must stay at 60%.
    for y in 1..255 {
        for x in 1..255 {
            if (-1_isize..=1).all(|dy| (-1_isize..=1).all(|dx| expected.pixels[((y as isize + dy) as usize * 256 + (x as isize + dx) as usize) * 4 + 3] == 153))
            {
                let a = actual[y * 256 + x];
                assert!(a[3].abs_diff(153) <= 1, "overlapping glyph/decoration alpha {a:?}");
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
}

/// Raster fallbacks preserve coverage at their requested output resolution. The
/// vector-only 8x geometry oracle cannot scale baked masks/shadows or raster pixels.
/// Keep the same IoU and colour gates, with independent native image compositing.
fn assert_raster_oracle_svg(doc: &Document, source: &str, label: &str) {
    let actual = oracle::native_raster_render(source);
    let expected = photocraft_compose::flatten(doc).to_rgba8();
    assert_eq!(actual.len() * 4, expected.pixels.len());
    let (mut intersection, mut union, mut delta) = (0u64, 0u64, 0.0);
    for (a, b) in actual.iter().zip(expected.pixels.as_chunks::<4>().0) {
        intersection += u64::from(a[3].min(b[3]));
        union += u64::from(a[3].max(b[3]));
        let white = |pixel: [u8; 4], premultiplied: bool| {
            std::array::from_fn(|i| {
                let colour = if premultiplied { u32::from(pixel[i]) } else { (u32::from(pixel[i]) * u32::from(pixel[3]) + 127) / 255 };
                (colour + 255 - u32::from(pixel[3])).min(255) as u8
            })
        };
        let a = photocraft_testkit::vector::oklab(white(*a, false));
        let b = photocraft_testkit::vector::oklab(white(*b, false));
        delta += a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt() * 100.;
    }
    let iou = if union == 0 { 1. } else { intersection as f64 / union as f64 };
    let mean = delta / actual.len() as f64;
    eprintln!("{label}: native raster IoU={iou:.8} mean DeltaE_ok={mean:.6}");
    assert!(iou > 0.995, "{label}: IoU {iou}");
    assert!(mean < 0.5, "{label}: DeltaE {mean}");
}

#[test]
fn svg_cropped_fallback_scales_with_small_effect_layers_and_preserves_oracle() {
    let mut doc = Document::new("sparse effects", Size::new(2048, 2048), ColorMode::Rgb, SampleType::U8);
    for i in 0..8 {
        let x = 80. + f64::from(i % 4) * 480.;
        let y = 90. + f64::from(i / 4) * 1500.;
        let mut shape =
            ShapeLayer { path: photocraft_vector::shapes::rect(x, y, 24., 20.), fill: Some(Fill::Solid(Color::rgb(0.2, 0.5, 0.8))), ..Default::default() };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        let mut layer = Layer::new("small shadow", LayerContent::Shape(shape));
        layer.effects.items.push(photocraft_doc::Effect::default_drop_shadow());
        doc.layers.push(layer);
    }
    let source = svg(&doc);
    let xml = xml(&source);
    let images = xml.descendants().filter(|node| node.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(images.len(), 8);
    let mut area = 0u64;
    for (index, image) in images.into_iter().enumerate() {
        let w = image.attribute("width").unwrap().parse::<u64>().unwrap();
        let h = image.attribute("height").unwrap().parse::<u64>().unwrap();
        assert!(w < 128 && h < 128, "small icon fallback must not allocate canvas-sized PNG");
        area += w * h;
        let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width() as u64, decoded.height() as u64), (w, h));
        let x: i32 = image.attribute("x").unwrap().parse().unwrap();
        let y: i32 = image.attribute("y").unwrap().parse().unwrap();
        let rect = photocraft_doc::Rect::new(x, y, x + w as i32, y + h as i32);
        let mut isolated = doc.clone();
        isolated.layers = vec![doc.layers[index].clone()];
        let reference = photocraft_compose::render(&isolated, rect).to_rgba8();
        assert!(decoded.to_rgba8().pixels().any(|pixel| pixel[3] > 0), "embedded image must contain art");
        assert_eq!(decoded.to_rgba8().as_raw(), &reference.pixels, "PNG samples and SVG coordinates must match original layer {index} at {rect:?}");
    }
    assert!(area < 128 * 128 * 8);
    assert_raster_oracle_svg(&doc, &source, "cropped sparse shadow layers");
}

#[test]
fn svg_cropped_fallback_keeps_masks_nontransparent_defaults_and_skips_empty_scope() {
    let mut doc = scene(photocraft_vector::shapes::rect(60., 65., 80., 75.), Fill::Solid(Color::rgb(0.3, 0.7, 0.1)));
    let mut mask = photocraft_doc::LayerMask::reveal_all();
    mask.feather = 4.;
    mask.surface.fill_rect(photocraft_doc::Rect::new(80, 60, 120, 150), &[0.]);
    doc.layers[0].mask = Some(mask);
    doc.layers[0].effects.items.push(photocraft_doc::Effect::default_drop_shadow());
    assert_raster_oracle_svg(&doc, &svg(&doc), "cropped feathered-mask shadow");
    let surface = photocraft_doc::Surface::with_default(doc.pixel_format(), &[0.1, 0.2, 0.3, 1.]);
    doc.layers = vec![Layer::new("opaque infinite surface", LayerContent::Raster(surface))];
    let source = svg(&doc);
    let xml = xml(&source);
    let image = xml.descendants().find(|node| node.has_tag_name("image")).unwrap();
    assert_eq!(image.attribute("width"), Some("256"));
    assert_eq!(image.attribute("height"), Some("256"));
    assert_raster_oracle_svg(&doc, &source, "nontransparent-default raster");
    let mut outside = photocraft_doc::Surface::new(doc.pixel_format());
    outside.fill_rect(photocraft_doc::Rect::new(1000, 1000, 1010, 1010), &[1., 0., 0., 1.]);
    doc.layers = vec![Layer::new("outside", LayerContent::Raster(outside))];
    assert!(!roxmltree::Document::parse(&svg(&doc)).unwrap().descendants().any(|node| node.has_tag_name("image")));
}

#[test]
fn svg_cropped_fallback_preserves_canvas_relative_gradient_frame() {
    for align in [false, true] {
        let fill = Fill::Gradient {
            stops: vec![(0., Color::BLACK), (1., Color::WHITE)],
            style: GradientStyle::Linear,
            angle: 0.,
            scale: 1.,
            reverse: false,
            opacity_stops: vec![],
            midpoints: vec![],
            offset: (0., 0.),
            dither: false,
            align,
        };
        let mut doc = scene(photocraft_vector::shapes::rect(70., 80., 48., 40.), fill);
        doc.layers[0].effects.items.push(photocraft_doc::Effect::default_drop_shadow());
        let source = svg(&doc);
        let xml = xml(&source);
        let image = xml.descendants().find(|node| node.has_tag_name("image")).unwrap();
        let coordinate = |name| image.attribute(name).unwrap().parse::<i32>().unwrap();
        let (x, y, w, h) = (coordinate("x"), coordinate("y"), coordinate("width"), coordinate("height"));
        assert!(w < 128 && h < 128, "gradient fallback must remain cropped");
        let crop = photocraft_doc::Rect::new(x, y, x + w, y + h);
        let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
        let png = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
        let pixels = image::load_from_memory(&png).unwrap().into_rgba8();
        let expected = photocraft_compose::render(&doc, crop).to_rgba8();
        assert_eq!(pixels.as_raw(), &expected.pixels, "align={align}: cropped PNG retains original canvas paint frame");
        if !align {
            // Prove this fixture detects mistakenly treating the crop as the canvas.
            let mut wrong_frame = doc.clone();
            let format = wrong_frame.pixel_format();
            if let LayerContent::Shape(shape) = &mut wrong_frame.layers[0].content {
                shape.cache = Some(photocraft_vector::CompiledShape::new(shape, 0.01, crop).render(format, doc.bounds()));
            }
            assert_ne!(photocraft_compose::render(&wrong_frame, crop).to_rgba8().pixels, expected.pixels);
        }
        assert_raster_oracle_svg(&doc, &source, &format!("cropped gradient align={align}"));
    }
}

#[test]
fn svg_cropped_fallback_preserves_rotated_warped_text() {
    let mut doc = Document::new("Rotated warped type", Size::new(512, 512), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = 96.;
    let mut text = photocraft_doc::TextLayer {
        text: "WARPED TEXT".into(),
        size_pt: 28.,
        color: Color::rgb(0.2, 0.4, 0.8),
        transform: photocraft_doc::Affine::rotate(0.2).then(&photocraft_doc::Affine::translate(90., 140.)),
        warp: Some(photocraft_doc::text::TextWarp { style: "warpArc".into(), value: 60., horizontal: true, ..Default::default() }),
        ..Default::default()
    };
    let mut fonts = photocraft_text::TextEngine::new();
    let layout = fonts.layout(&text, doc.resolution_dpi);
    let unwarped = photocraft_text::render::ink_rect(&layout, &text.transform);
    let warped = fonts.render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
    assert_ne!(warped.intersect(&unwarped), warped, "fixture must expose ink outside the unwarped bounds");
    doc.layers.push(Layer::new("Rotated arc", LayerContent::Text(text)));
    let source = svg(&doc);
    let xml = xml(&source);
    let images = xml.descendants().filter(|node| node.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(images.len(), 1, "warped type must exercise the cropped raster path");
    assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
    let image = images[0];
    let coordinate = |name| image.attribute(name).unwrap().parse::<i32>().unwrap();
    let (x, y, w, h) = (coordinate("x"), coordinate("y"), coordinate("width"), coordinate("height"));
    let crop = photocraft_doc::Rect::new(x, y, x + w, y + h);
    assert_eq!(crop, warped.intersect(&doc.bounds()), "crop must include rotated and warped ink");
    assert!(w > 0 && h > 0 && w < 512 && h < 512, "small text must not embed the full canvas");
    let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
    let pixels = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(pixels.dimensions(), (w as u32, h as u32));
    assert!(pixels.pixels().any(|pixel| pixel[3] > 0));
    assert_eq!(pixels.as_raw(), &photocraft_compose::render(&doc, crop).to_rgba8().pixels);
    assert_raster_oracle_svg(&doc, &source, "cropped rotated warped text");
}

mod blends;

mod clipping;

#[test]
fn svg_text_outlines_preserve_adjacent_paint_runs_and_bound_traversal() {
    use photocraft_doc::text::{CharStyle, TextRun};
    for alpha in [1.0, 0.5] {
        let mut doc = Document::new("paint order", Size::new(256, 128), ColorMode::Rgb, SampleType::U8);
        let mut text = photocraft_doc::TextLayer { text: "HHH".into(), transform: photocraft_doc::Affine::translate(100., 90.), ..Default::default() };
        text.runs = (0..3)
            .map(|i| {
                let mut color = if i == 1 { Color::rgb(0., 0., 1.) } else { Color::rgb(1., 0., 0.) };
                color.alpha = alpha;
                TextRun { len: 1, style: CharStyle { size_pt: 64., tracking: -650., color, ..Default::default() } }
            })
            .collect();
        photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
        doc.layers.push(Layer::new("Type", LayerContent::Text(text)));
        let options = ExportOptions { text_as_curves: true, ..Default::default() };
        let source = String::from_utf8(export::export(&doc, &ExportScope::Document, &options).unwrap().bytes).unwrap();
        let colors = xml(&source)
            .descendants()
            .filter(|node| node.has_tag_name("path"))
            .filter_map(|node| node.attribute("fill").map(str::to_owned))
            .collect::<Vec<_>>();
        assert_eq!(colors, ["#ff0000", "#0000ff", "#ff0000"]);
        let rendered = render(&source);
        let expected = photocraft_compose::flatten(&doc).get(147, 67);
        let pixel = rendered[67 * 256 + 147];
        for channel in 0..4 {
            let value = if channel == 3 { expected[3] } else { expected[channel] * expected[3] };
            assert!((f32::from(pixel[channel]) / 255. - value).abs() < 0.02, "alpha={alpha}: {pixel:?} versus {expected:?}");
        }
        let limited = ExportOptions { max_bytes: 256, ..options.clone() };
        assert!(matches!(export::export(&doc, &ExportScope::Document, &limited), Err(ExportError::Budget(_))));
        let checks = std::cell::Cell::new(0);
        let cancel = || {
            checks.set(checks.get() + 1);
            checks.get() > 30
        };
        let mut text = photocraft_text::TextEngine::new();
        let mut context = export::ExportContext { cancelled: &cancel, projection: None, text: &mut text };
        assert!(matches!(export::export_with(&doc, &ExportScope::Document, &options, &mut context), Err(ExportError::Cancelled)));
        assert!(checks.get() < 100, "cancellation should stop the current traversal promptly");
    }
}
