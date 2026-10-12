//! Native contracts compare the complete Document, including caches, pixels, IDs and metadata.
mod contracts_support;
#[path = "it/common/mod.rs"]
mod legacy;
use contracts_support::*;
use photocraft_doc::vector_design::{drawing::*, live::*};
use photocraft_doc::*;
use photocraft_format::*;
use std::sync::Arc;

fn compare(d: &Document) {
    // Separate writer/reader lifetimes; cover both ZIP files and directory bundles.
    let dir = legacy::temp_dir("contracts");
    let zip = dir.join("document.pcraft");
    PcraftWriter::new().save_path(d, &zip, &SaveOptions::default()).unwrap();
    assert_eq!(load_path(&zip).unwrap(), *d, "ZIP: {}", d.name);
    let bundle = dir.join("directory");
    PcraftWriter::new().save_dir(d, &bundle, &SaveOptions::default()).unwrap();
    assert_eq!(load_path(&bundle).unwrap(), *d, "directory: {}", d.name);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn raster_all_modes_depths_and_document_metadata() {
    for mode in [
        ColorMode::Rgb,
        ColorMode::Grayscale,
        ColorMode::Cmyk,
        ColorMode::Lab,
        ColorMode::Bitmap,
        ColorMode::Indexed,
        ColorMode::Duotone,
        ColorMode::Multichannel,
    ] {
        for depth in SampleType::ALL {
            compare(&legacy::rich_doc(mode, depth));
        }
    }
}
#[test]
fn text_runs_paragraphs_warp_path_and_cache() {
    let mut d = legacy::rich_doc(ColorMode::Rgb, SampleType::U16);
    let p = legacy::vector_path();
    for l in &mut d.layers {
        if matches!(l.content, LayerContent::Text(_)) {
            l.vector.on_path = Some(photocraft_doc::vector_design::text::PathText {
                path: p.clone(),
                start: 0.2,
                end: 0.8,
                flip: true,
                align: photocraft_doc::vector_design::text::PathTextAlign::Center,
            });
        }
    }
    compare(&d);
}
#[test]
fn shape_appearance_corners_width_profiles_and_live_parameters() {
    let mut d = document();
    let mut l = shape();
    let LayerContent::Shape(s) = &mut l.content else { unreachable!() };
    s.cache = Some(pixels(d.pixel_format()));
    l.vector = vector_data(s.path.clone());
    d.layers.push(l);
    // Every currently registered parametric shape, not just the default rectangle.
    let parameters = [
        ShapeParameters::Rect { radii: [1., 2., 3., 4.], kinds: [CornerKind::Round, CornerKind::Inverted, CornerKind::Chamfer, CornerKind::Straight] },
        ShapeParameters::Ellipse,
        ShapeParameters::Polygon { sides: 6, curved: 0.2, corner: 2. },
        ShapeParameters::Star { points: 5, inner: 0.5, corner: 2. },
        ShapeParameters::DoubleStar { points: 6, inner: 0.4, secondary: 0.7, corner: 1. },
        ShapeParameters::SquareStar { points: 4, inner: 0.5, corner: 1. },
        ShapeParameters::Triangle { top: 0.4 },
        ShapeParameters::Trapezoid { top_left: 0.2, top_right: 0.8 },
        ShapeParameters::Diamond { top: 0.4 },
        ShapeParameters::Donut { hole: 0.3 },
        ShapeParameters::Pie { start: 0.2, end: 2. },
        ShapeParameters::Segment { start: 0.2, end: 2., inner: 0.4 },
        ShapeParameters::Arrow { from: [1., 2.], to: [10., 20.], head_width: 4., head_length: 3., shaft: 2., double: true },
        ShapeParameters::Callout { ellipse: true, tail: [1., 2.], radius: 2. },
        ShapeParameters::Cog { teeth: 8, inner: 0.6, hole: 0.2, curve: 0.1 },
        ShapeParameters::Heart { depth: 0.4 },
        ShapeParameters::Tear { bend: 0.2 },
        ShapeParameters::Crescent { offset: 0.4 },
        ShapeParameters::Cloud { bubbles: 7 },
        ShapeParameters::Spiral { center: [10., 10.], radius: 8., decay: 0.8, turns: 3. },
        ShapeParameters::Arc { start: 0.2, end: 2. },
        ShapeParameters::Line { from: [1., 2.], to: [10., 20.] },
        ShapeParameters::RectGrid { rows: 3, cols: 4 },
        ShapeParameters::PolarGrid { rings: 3, spokes: 8 },
    ];
    for parameters in parameters {
        d.layers[0].vector.live_shape.as_mut().unwrap().parameters = parameters;
        compare(&d);
    }
}
#[test]
fn embedded_linked_smart_objects_and_develop_settings() {
    let mut d = legacy::rich_doc(ColorMode::Rgb, SampleType::F32);
    for l in &mut d.layers {
        if let LayerContent::Smart(s) = &mut l.content {
            s.develop = Some(DevelopLink {
                library_baseline: None,
                source: None,
                library: None,
                settings: serde_json::json!({"light":{"exposure":1.25},"profile":{"id":"lc.vivid"},"private":[1,2,3]}),
                photo: Some(42),
            });
        }
    }
    compare(&d);
}
#[test]
fn groups_artboards_nested_order_and_locks() {
    let mut d = document();
    let mut g = Layer::group("Artboard", vec![Layer::group("Nested", vec![shape(), Layer::new("Pixels", LayerContent::Raster(pixels(d.pixel_format())))])]);
    let LayerContent::Group(group) = &mut g.content else { unreachable!() };
    group.expanded = false;
    group.artboard = Some(Artboard {
        rect: Rect::new(1, 2, 30, 22),
        background: ArtboardBackground::Custom(Color::rgb(0.2, 0.4, 0.6)),
        preset: "Contract".into(),
        drawing_scale: None,
    });
    g.locks.artboard = true;
    d.layers.push(g);
    compare(&d);
}
#[test]
fn adjustment_and_all_fill_kinds() {
    let mut d = document();
    for adjustment in [
        Adjustment::identity_levels(),
        Adjustment::identity_curves(),
        Adjustment::default_hue_saturation(),
        Adjustment::BrightnessContrast { brightness: 12., contrast: -5., legacy: false },
        Adjustment::Exposure { exposure: 1.25, offset: 0.1, gamma: 1.2 },
        Adjustment::Vibrance { vibrance: 20., saturation: -5. },
        Adjustment::ColorBalance { shadows: [1., 2., 3.], midtones: [4., 5., 6.], highlights: [7., 8., 9.], preserve_luminosity: true },
        Adjustment::BlackWhite { weights: [1., 2., 3., 4., 5., 6.], tint: Some([0.2, 0.4, 0.6]) },
        Adjustment::PhotoFilter { color: [0.2, 0.4, 0.6], density: 0.5, preserve_luminosity: false },
        Adjustment::ChannelMixer { matrix: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]], monochrome: true },
        Adjustment::ColorLookup {
            name: "Contract LUT".into(),
            lut: Some(Arc::new((0..24).map(|i| i as f32 / 24.).collect())),
            size: 2,
            tetrahedral: true,
            dither: true,
        },
        Adjustment::Invert,
        Adjustment::Posterize { levels: 6 },
        Adjustment::Threshold { level: 0.4 },
        Adjustment::GradientMap { stops: vec![(0., [0.1, 0.2, 0.3]), (1., [0.7, 0.8, 0.9])], reverse: true, dither: true },
        Adjustment::SelectiveColor { relative: true, adjustments: [[0.1, 0.2, 0.3, 0.4]; 9] },
        Adjustment::Unsupported { psd_key: "test".into(), raw: vec![0, 1, 2, 255] },
    ] {
        d.layers.push(Layer::new("Adjustment", LayerContent::Adjustment(adjustment)));
    }
    d.layers.push(Layer::new("Solid", LayerContent::Fill(Fill::Solid(Color::rgba(0.2, 0.4, 0.6, 0.8)))));
    d.layers
        .push(Layer::new("Gradient", LayerContent::Fill(Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 35., 1.5, GradientStyle::Radial, true))));
    let pattern = Pattern::new("Pattern", pixels(d.pixel_format()), 8, 9);
    d.layers.push(Layer::new(
        "Pattern",
        LayerContent::Fill(Fill::Pattern { id: pattern.id.clone(), name: pattern.name.clone(), scale: 0.5, angle: 45., link: false, phase: (2., 3.) }),
    ));
    d.patterns.push(pattern);
    compare(&d);
}
#[test]
fn symbols_master_pixels_instance_transform_and_cache() {
    let mut d = golden_document();
    let format = d.pixel_format();
    d.symbols[0].layers.push(Layer::new("Master pixels", LayerContent::Raster(pixels(format))));
    if let LayerContent::Symbol(s) = &mut d.layers[2].content {
        s.cache = Some(pixels(format));
    }
    compare(&d);
}
#[test]
fn live_blend_spine_spacing_orientation() {
    for spacing in [BlendSpacing::SmoothColor, BlendSpacing::Steps(9), BlendSpacing::Distance(2.5)] {
        let mut d = document();
        let mut g = Layer::group("Blend", vec![shape(), shape()]);
        g.vector.live = Some(LiveGroup::Blend(BlendSpec { spacing, orientation: BlendOrientation::AlignToPath, spine: Some(legacy::vector_path()) }));
        d.layers.push(g);
        compare(&d);
    }
}
#[test]
fn live_repeat_all_modes() {
    for spec in [
        RepeatSpec::Radial { count: 7, center: [4., 5.], angle: 0.3 },
        RepeatSpec::Grid { rows: 3, cols: 4, spacing: [2., 3.] },
        RepeatSpec::Mirror { origin: [4., 5.], angle: 0.3 },
    ] {
        let mut d = document();
        let mut g = Layer::group("Repeat", vec![shape()]);
        g.vector.live = Some(LiveGroup::Repeat(spec));
        d.layers.push(g);
        compare(&d);
    }
}
#[test]
fn live_envelope_all_modes() {
    for spec in [
        EnvelopeSpec::Warp { style: "arc".into(), bend: 0.2, horizontal: 0.1, vertical: -0.1 },
        EnvelopeSpec::Mesh { rows: 2, cols: 2, points: vec![[0., 0.], [1., 0.], [0.1, 1.], [1., 1.]] },
        EnvelopeSpec::TopObject { path: legacy::vector_path() },
    ] {
        let mut d = document();
        let mut g = Layer::group("Envelope", vec![shape()]);
        g.vector.live = Some(LiveGroup::Envelope(spec));
        d.layers.push(g);
        compare(&d);
    }
}
#[test]
fn video_frames_timeline_and_text_styles() {
    let mut d = document();
    let mut l = Layer::raster("Video", d.pixel_format());
    l.video = Some(VideoData::new(vec![pixels(d.pixel_format()), Surface::with_default(d.pixel_format(), &[0.4, 0.2, 0.8, 1.])], 24.));
    d.timeline = Some(Timeline::new(2, 24.));
    d.layers.push(l);
    d.text_styles.character.push(photocraft_doc::text_styles::CharacterStyleDef { id: 1, name: "Contract character".into(), ..Default::default() });
    d.text_styles.paragraph.push(photocraft_doc::text_styles::ParagraphStyleDef { id: 2, name: "Contract paragraph".into(), ..Default::default() });
    d.metadata.exif = Some(Arc::new(vec![0, 1, 2, 255]));
    compare(&d);
}
#[test]
fn frozen_v2_native_reference_loads_exact_document() {
    let bytes = include_bytes!("fixtures/contracts/document-v2.pcraft");
    let zip = zip::ZipReader::new(bytes).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&zip.read_by_name("manifest.json", 1 << 20).unwrap()).unwrap();
    assert_eq!(raw["format_version"], 2); // Historical version; never change when FORMAT_VERSION grows.
    const { assert!(FORMAT_VERSION >= 2) };
    let loaded = load_from_bytes(bytes).unwrap();
    assert_eq!(loaded, golden_document());
    compare(&loaded);
}
#[test]
fn frozen_v1_native_reference_migrates_without_data_loss() {
    let d = load_from_bytes(include_bytes!("fixtures/vector-v1.pcraft")).unwrap();
    assert_eq!(d.layers.len(), 2);
    assert!(d.symbols.is_empty());
    assert_eq!(d.layers[1].name, "Legacy text");
    compare(&d);
}

#[test]
fn every_layer_content_variant_has_a_structural_fixture() {
    // An added LayerContent variant forces an explicit coverage decision at compile time.
    fn variant(content: &LayerContent) -> &str {
        match content {
            LayerContent::Raster(_) => "raster",
            LayerContent::Group(_) => "group",
            LayerContent::Adjustment(_) => "adjustment",
            LayerContent::Fill(_) => "fill",
            LayerContent::Text(_) => "text",
            LayerContent::Shape(_) => "shape",
            LayerContent::Smart(_) => "smart",
            LayerContent::Symbol(_) => "symbol",
        }
    }
    let mut d = legacy::rich_doc(ColorMode::Rgb, SampleType::U8);
    let reference = golden_document();
    d.symbols = reference.symbols;
    d.layers.push(reference.layers[2].clone());
    let covered: std::collections::BTreeSet<_> = d.walk().into_iter().map(|(_, _, l)| variant(&l.content)).collect();
    assert_eq!(covered, std::collections::BTreeSet::from(["raster", "group", "adjustment", "fill", "text", "shape", "smart", "symbol"]));
    compare(&d);
}
