//! PSD/PSB contracts for editable content the adapter claims to preserve.
//! Advanced vector stacks/live groups/symbols/mesh are documented warned raster fallbacks,
//! so their editable structural contracts belong to contracts_native / contracts_storage.
#[path = "it/common/mod.rs"]
mod legacy;
use photocraft_doc::text::{CharStyle, ParagraphRun, ParagraphStyle, TextAlign, TextRun};
use photocraft_doc::*;
use photocraft_io::{ExportOptions, export, import};
use photocraft_psd::PsdFile;
use std::sync::Arc;

fn surface_eq(a: &Surface, b: &Surface, context: &str) {
    let (mut a, mut b) = (a.clone(), b.clone());
    a.prune();
    b.prune();
    assert_eq!(a.format(), b.format(), "{context}: pixel format");
    assert_eq!(a.default_pixel(), b.default_pixel(), "{context}: default pixel");
    assert_eq!(a.content_bounds(), b.content_bounds(), "{context}: content bounds");
    let bounds = a.content_bounds().union(&b.content_bounds());
    if !bounds.is_empty() {
        assert_eq!(a.to_interleaved(bounds), b.to_interleaved(bounds), "{context}: pixel samples");
    }
}
fn reopen(d: &Document, extension: &str) -> Document {
    let exported = export(d, extension, &ExportOptions::default()).unwrap();
    let psd = PsdFile::from_bytes(&exported.bytes).unwrap();
    assert_eq!(PsdFile::from_bytes(&psd.to_bytes().unwrap()).unwrap(), psd, "PSD wire structure");
    import("contracts.psd", &exported.bytes).unwrap().document
}
fn normalized(mut d: Document) -> Document {
    // Only process-local IDs change on import; PSD IDs and every other field remain checked.
    d.id = DocId(0);
    fn layers(ls: &mut [Layer], next: &mut u64) {
        for l in ls {
            l.id = LayerId(*next);
            *next += 1;
            if let Some(children) = l.children_mut() {
                layers(children, next);
            }
        }
    }
    layers(&mut d.layers, &mut 1);
    d
}
fn stable(d: &Document, extension: &str) {
    let back = reopen(d, extension);
    assert_eq!(normalized(back), normalized(d.clone()), "complete imported model must survive a further save/reopen");
}
#[test]
fn psd_psb_raster_groups_masks_adjustments_fills_and_metadata() {
    for extension in ["psd", "psb"] {
        for (mode, depth) in [
            (ColorMode::Rgb, SampleType::U8),
            (ColorMode::Rgb, SampleType::U16),
            (ColorMode::Rgb, SampleType::F32),
            (ColorMode::Grayscale, SampleType::U8),
            (ColorMode::Grayscale, SampleType::U16),
            (ColorMode::Grayscale, SampleType::F32),
            (ColorMode::Cmyk, SampleType::U8),
            (ColorMode::Cmyk, SampleType::U16),
            (ColorMode::Lab, SampleType::U8),
            (ColorMode::Lab, SampleType::U16),
        ] {
            let d = legacy::gen_doc(mode, depth, legacy::Features::ALL);
            let back = reopen(&d, extension);
            legacy::assert_docs_eq(&d, &back);
            stable(&back, extension);
        }
    }
}
#[test]
fn psd_text_keeps_editable_runs_paragraphs_transform_and_styles() {
    let mut d = Document::new("Text", Size::new(128, 64), ColorMode::Rgb, SampleType::U8);
    let mut t = TextLayer {
        text: "calm LOUD".into(),
        runs: vec![
            TextRun { len: 5, style: CharStyle { font_family: "Inter".into(), size_pt: 12., ..Default::default() } },
            TextRun { len: 4, style: CharStyle { font_family: "Inter".into(), size_pt: 18., underline: true, ..Default::default() } },
        ],
        paragraphs: vec![ParagraphRun { len: 9, style: ParagraphStyle { align: TextAlign::Center, ..Default::default() } }],
        transform: Affine::translate(4., 30.),
        ..Default::default()
    };
    t.sync_summary();
    photocraft_text::TextEngine::new().render_layer(&mut t, d.resolution_dpi, d.pixel_format());
    d.layers.push(Layer::new("Text", LayerContent::Text(t.clone())));
    let back = reopen(&d, "psd");
    let LayerContent::Text(actual) = &back.layers[0].content else { panic!("editable text lost") };
    assert_eq!(actual.text, t.text);
    assert_eq!(actual.font_family, t.font_family);
    assert_eq!(actual.size_pt, t.size_pt);
    assert_eq!(actual.color, t.color);
    assert_eq!(actual.transform, t.transform);
    assert_eq!(actual.char_runs(), t.char_runs());
    assert_eq!(actual.paragraph_runs(), t.paragraph_runs());
    assert_eq!(actual.shape, t.shape);
    assert_eq!(actual.orientation, t.orientation);
    assert_eq!(actual.antialias, t.antialias);
    assert_eq!(actual.warp, t.warp);
    surface_eq(actual.cache.as_ref().unwrap(), t.cache.as_ref().unwrap(), "text pixels");
    stable(&back, "psd");
}
#[test]
fn psd_shape_keeps_path_fill_stroke_live_shape_and_vector_mask() {
    let mut d = Document::new("Shape", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let s = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(16., 16.), (128., 16.), (128., 192.), (16., 192.)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        stroke: Some(ShapeStroke { width: 4., dashes: vec![2., 1.], cap: LineCap::Round, join: LineJoin::Bevel, ..Default::default() }),
        live: Some(LiveShape::Rect { rect: [16., 16., 112., 176.], radii: [0.; 4] }),
        ..Default::default()
    };
    let mut layer = Layer::new("Shape", LayerContent::Shape(s.clone()));
    layer.vector_mask = Some(VectorMask::new(s.path.clone()));
    let LayerContent::Shape(rendered) = &mut layer.content else { unreachable!() };
    rendered.cache = Some(photocraft_vector::render_shape(&s, d.pixel_format(), d.bounds()));
    d.layers.push(layer);
    let back = reopen(&d, "psd");
    let LayerContent::Shape(actual) = &back.layers[0].content else { panic!("editable shape lost") };
    assert_eq!(actual.path, s.path);
    assert_eq!(actual.fill, s.fill);
    assert_eq!(actual.stroke, s.stroke);
    assert_eq!(actual.live, s.live);
    assert_eq!(back.layers[0].vector_mask, d.layers[0].vector_mask);
    stable(&back, "psd");
}
#[test]
fn psd_groups_and_artboard_geometry_background_and_child_order() {
    let mut d = Document::new("Boards", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let mut board = Layer::group("Board", vec![Layer::new("Child", LayerContent::Raster(legacy::pattern(d.pixel_format(), Rect::new(4, 4, 8, 8), 3, true)))]);
    let LayerContent::Group(g) = &mut board.content else { unreachable!() };
    g.artboard = Some(Artboard {
        rect: Rect::new(2, 3, 30, 22),
        background: ArtboardBackground::Custom(Color::rgb(1., 0., 0.)),
        preset: "Custom contract".into(),
        drawing_scale: None,
    });
    let expected = g.clone();
    d.layers.push(board);
    let back = reopen(&d, "psd");
    let LayerContent::Group(actual) = &back.layers[0].content else { panic!("group lost") };
    assert_eq!(actual.artboard, expected.artboard);
    assert_eq!(actual.expanded, expected.expanded);
    assert_eq!(actual.children.len(), expected.children.len());
    for (a, b) in actual.children.iter().zip(&expected.children) {
        legacy::assert_layers_eq(b, a, "board");
    }
    stable(&back, "psd");
}
fn embedded() -> (Vec<u8>, Surface) {
    let rgba: Vec<_> = (0..8 * 6).flat_map(|_| [51u8, 102, 204, 255]).collect();
    let image = photocraft_codecs::Image::from_u8(8, 6, photocraft_codecs::ChannelLayout::Rgba, rgba).unwrap();
    let bytes = photocraft_codecs::encode(&image, photocraft_codecs::Format::Png, &Default::default()).unwrap();
    let mut pixels = Surface::new(PixelFormat::RGBA8);
    pixels.fill_rect(Rect::new(0, 0, 8, 6), &[0.2, 0.4, 0.8, 1.]);
    (bytes, pixels)
}
fn smart_contract(develop: bool) {
    let mut d = Document::new("Smart", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
    let (bytes, pixels) = embedded();
    let mut smart = SmartObject::new(SmartSource::Embedded { file_name: "source.png".into(), bytes: Arc::new(bytes.clone()) }, Affine::IDENTITY, Some(pixels));
    smart.smart_filters.push(SmartFilter {
        command: "filter.blur.gaussianBlur".into(),
        params: serde_json::json!({"radius":2.}),
        blend: BlendMode::Multiply,
        opacity: 0.4,
        visible: false,
    });
    smart.filters_enabled = false;
    smart.filter_mask = Some(LayerMask::reveal_all());
    if develop {
        let mut settings = lightcraft_develop::DevelopSettings::default();
        settings.light.exposure = 1.25;
        settings.profile.id = "lc.vivid".into();
        smart.develop = Some(DevelopLink { source: None, library: None, settings: serde_json::to_value(settings).unwrap(), photo: Some(42) });
    }
    d.layers.push(Layer::new("Smart", LayerContent::Smart(smart.clone())));
    let back = reopen(&d, "psd");
    let LayerContent::Smart(actual) = &back.layers[0].content else { panic!("editable smart object lost") };
    assert_eq!(actual.transform, smart.transform);
    assert_eq!(actual.smart_filters, smart.smart_filters);
    assert_eq!(actual.filters_enabled, smart.filters_enabled);
    assert_eq!(actual.filter_mask, smart.filter_mask);
    assert_eq!(actual.develop, smart.develop);
    assert_eq!(actual.warp, smart.warp);
    assert_eq!(actual.stack_mode, smart.stack_mode);
    let SmartSource::Linked { path } = &actual.source else { panic!("expected PSD UUID source") };
    assert_eq!(photocraft_io::linked::find_linked_file(&back.metadata, path).unwrap().bytes, bytes);
    surface_eq(actual.cache.as_ref().unwrap(), smart.cache.as_ref().unwrap(), "smart cache");
    stable(&back, "psd");
}
#[test]
fn psd_smart_embedded_source_transform_filters_mask_and_cache() {
    smart_contract(false);
}
#[test]
fn psd_develop_settings_photo_link_and_embedded_source() {
    smart_contract(true);
}

#[test]
fn private_editable_shape_is_stale_after_external_pixel_or_block_edits() {
    let mut doc = Document::new("private controls", Size::new(16, 16), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::rect(0., 0., 12., 12.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let mut layer = Layer::new("shape", LayerContent::Shape(shape));
    layer.vector_mask = Some(VectorMask::new(photocraft_vector::shapes::rect(0., 0., 8., 8.)));
    doc.layers.push(layer);
    for extension in ["psd", "psb"] {
        let exported = export(&doc, extension, &Default::default()).unwrap();
        let parsed = PsdFile::from_bytes(&exported.bytes).unwrap();
        // Match the filename used by stable() so the document name remains part of the full equality check.
        let original = import("contracts.psd", &exported.bytes).unwrap();
        assert!(matches!(original.document.layers[0].content, LayerContent::Shape(_)));
        stable(&original.document, extension);
        for modify_pixels in [true, false] {
            let mut edited = parsed.clone();
            let record = &mut edited.layer_info.as_mut().unwrap().layers[0];
            if modify_pixels {
                let channel = record.channels.iter_mut().find(|channel| channel.id == 0).unwrap();
                channel.compression = Some(photocraft_psd::Compression::Raw);
                channel.data = vec![100; 64];
            } else {
                record.blocks.push(photocraft_psd::TaggedBlock::new(*b"extE", b"external edit".to_vec()));
            }
            let bytes = edited.to_bytes().unwrap();
            let back = import("edited.psd", &bytes).unwrap();
            assert!(matches!(back.document.layers[0].content, LayerContent::Raster(_)));
            assert!(back.warnings.iter().any(|warning| warning.contains("stale native editable")));
        }
        let mut malformed = parsed.clone();
        let block = malformed.layer_info.as_mut().unwrap().layers[0].blocks.iter_mut().find(|block| block.key == *b"liED").unwrap();
        block.data[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        let back = import("malformed.psd", &malformed.to_bytes().unwrap()).unwrap();
        assert!(matches!(back.document.layers[0].content, LayerContent::Raster(_)));
        assert!(back.warnings.iter().any(|warning| warning.contains("invalid native editable")));
    }
}

#[test]
fn stale_native_text_runs_do_not_override_external_type_edits() {
    let mut doc = Document::new("Text stale", Size::new(64, 32), ColorMode::Rgb, SampleType::U8);
    let mut text = TextLayer { text: "A".into(), runs: vec![TextRun { len: 1, style: CharStyle::default() }], ..Default::default() };
    text.sync_summary();
    photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
    doc.layers.push(Layer::new("Text", LayerContent::Text(text)));
    let output = export(&doc, "psd", &Default::default()).unwrap();
    let mut parsed = PsdFile::from_bytes(&output.bytes).unwrap();
    let layer = &mut parsed.layer_info.as_mut().unwrap().layers[0];
    // An external editor may change type or its pixels without understanding liED.
    // A changed ordinary block invalidates the complete private run snapshot.
    layer.blocks.push(photocraft_psd::TaggedBlock::new(*b"extT", b"edited type".to_vec()));
    let back = import("external.psd", &parsed.to_bytes().unwrap()).unwrap();
    assert!(back.warnings.iter().any(|warning| warning.contains("stale native editable")));
    let LayerContent::Text(text) = &back.document.layers[0].content else { panic!("standard type lost") };
    assert!(text.char_runs()[0].style.postscript_name.is_some());
}

fn generated_document(depth: SampleType) -> Document {
    let mut doc = Document::new("generation", Size::new(4, 4), ColorMode::Rgb, depth);
    let mut pixels = Surface::new(doc.pixel_format());
    pixels.write_pixel(1, 1, &[0.25, 0.5, 0.75, 1.]);
    let mut layer = Layer::new("generated", LayerContent::Raster(pixels));
    layer.generation = Some(serde_json::json!({
        "command":"ai.generativeFill", "prompt":"light over water; 🌅", "seed":u64::MAX,
        "engine":"qwen", "model":"custom:example", "bounds":[1,1,2,2],
        "refine":{"model":"other","strength":0.25,"loras":[{"name":"style","strength":0.7}]},
        "extension":{"future":[true,null,-4,0.125]}
    }));
    doc.layers.push(layer);
    doc
}

#[test]
fn generation_psd_psb_layered_tiff_two_cycles_keep_settings_and_pixels() {
    for extension in ["psd", "psb", "tif"] {
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let mut doc = generated_document(depth);
            let mut duplicate = doc.layers[0].duplicate();
            duplicate.name = "duplicate".into();
            let mut group = Layer::group("group", vec![duplicate]);
            group.generation = Some(serde_json::json!({"extension":{"group":"provenance"}}));
            doc.layers.push(group);
            let expected = doc.clone();
            for _ in 0..2 {
                let output = export(&doc, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
                assert!(output.warnings.iter().all(|w| !w.contains("generation") && !w.contains("liGM")), "{:?}", output.warnings);
                let opened = import(extension, &output.bytes).unwrap();
                assert!(opened.warnings.iter().all(|w| !w.contains("generation") && !w.contains("liGM")), "{:?}", opened.warnings);
                doc = opened.document;
                let expected_layers = expected.walk();
                let actual_layers = doc.walk();
                assert_eq!(actual_layers.len(), expected_layers.len());
                for ((_, _, want), (_, _, got)) in expected_layers.iter().zip(&actual_layers) {
                    assert_eq!(got.name, want.name);
                    assert_eq!(got.generation, want.generation);
                    assert!(got.psd_blocks.iter().all(|(key, _)| key != b"liGM"));
                    if let (Some(a), Some(b)) = (want.surface(), got.surface()) {
                        surface_eq(a, b, "generation roundtrip");
                    }
                }
            }
        }
    }
}

#[test]
fn generation_edits_and_clear_cannot_restore_stale_private_block() {
    for extension in ["psd", "psb", "tif"] {
        let source = generated_document(SampleType::U8);
        let first = export(&source, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
        let mut doc = import(extension, &first.bytes).unwrap().document;
        // Simulate a caller retaining stale raw private data. Current typed state wins.
        doc.layers[0].psd_blocks.push((*b"liGM", Arc::new(b"stale".to_vec())));
        for value in [Some(serde_json::json!({"command":"ai.generateBackground","prompt":"changed","seed":7})), None] {
            doc.layers[0].generation = value.clone();
            let output = export(&doc, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
            doc = import(extension, &output.bytes).unwrap().document;
            assert_eq!(doc.layers[0].generation, value);
            surface_eq(source.layers[0].surface().unwrap(), doc.layers[0].surface().unwrap(), "edited metadata pixels");
        }
    }
}

#[test]
fn generation_locked_single_layer_tiff_remains_layered() {
    let mut doc = generated_document(SampleType::U8);
    doc.layers[0].locks.transparency = true;
    assert!(photocraft_io::tiff_layers::would_write_layers(&doc));
    for _ in 0..2 {
        let output = export(&doc, "tif", &ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
        assert!(photocraft_codecs::tiff_photoshop_tags(&output.bytes).1.is_some());
        let back = import("tif", &output.bytes).unwrap().document;
        assert_eq!(back.layers[0].generation, doc.layers[0].generation);
        assert!(back.layers[0].locks.transparency);
        doc = back;
    }
}

#[test]
fn generation_bad_import_keeps_standard_pixels_and_warns() {
    let doc = generated_document(SampleType::U8);
    let output = export(&doc, "psd", &ExportOptions::default()).unwrap();
    let original = PsdFile::from_bytes(&output.bytes).unwrap();
    for case in 0..5 {
        let mut file = original.clone();
        let record = &mut file.layer_info.as_mut().unwrap().layers[0];
        let block = record.blocks.iter_mut().find(|b| b.key == *b"liGM").unwrap();
        match case {
            0 => block.data[7] = b'2',
            1 => block.data.truncate(11),
            2 => block.data[8..12].copy_from_slice(&u32::MAX.to_be_bytes()),
            3 => block.data[12] = 0xff,
            _ => {
                let duplicate = block.clone();
                record.blocks.push(duplicate);
            }
        }
        let bytes = file.to_bytes().unwrap();
        let opened = import("psd", &bytes).unwrap();
        assert_eq!(opened.warnings.iter().filter(|w| w.contains("generation metadata")).count(), 1, "{:?}", opened.warnings);
        assert!(opened.document.layers[0].generation.is_none());
        surface_eq(doc.layers[0].surface().unwrap(), opened.document.layers[0].surface().unwrap(), "malformed generation pixels");
    }
}

#[test]
fn generation_export_limits_fail_before_publishing_bytes() {
    let mut doc = generated_document(SampleType::U8);
    for extension in ["psd", "psb", "tif"] {
        doc.layers[0].generation = Some(serde_json::json!({"prompt":"x".repeat(256*1024)}));
        assert!(export(&doc, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).is_err());
    }
    doc.layers[0].generation = Some(serde_json::json!({"prompt":"x".repeat(255*1024)}));
    let layer = doc.layers[0].clone();
    doc.layers = (0..33).map(|_| layer.duplicate()).collect();
    for extension in ["psd", "psb", "tif"] {
        assert!(export(&doc, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).is_err());
    }
}

#[test]
fn generation_coexists_with_editable_shape_fingerprint_for_two_cycles() {
    let mut doc = Document::new("private controls", Size::new(16, 16), ColorMode::Rgb, SampleType::U8);
    let mut shape = ShapeLayer { path: photocraft_vector::shapes::rect(0., 0., 12., 12.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let mut layer = Layer::new("generated shape", LayerContent::Shape(shape));
    layer.vector_mask = Some(VectorMask::new(photocraft_vector::shapes::rect(0., 0., 8., 8.)));
    layer.generation = Some(serde_json::json!({"extension":{"provenance":"shape source"}}));
    doc.layers.push(layer);
    for extension in ["psd", "psb", "tif"] {
        let mut current = doc.clone();
        for _ in 0..2 {
            let output = export(&current, extension, &ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
            let opened = import(extension, &output.bytes).unwrap();
            assert!(opened.warnings.iter().all(|w| !w.contains("generation metadata") && !w.contains("stale native editable")), "{:?}", opened.warnings);
            current = opened.document;
            assert!(matches!(current.layers[0].content, LayerContent::Shape(_)));
            assert_eq!(current.layers[0].vector_mask, doc.layers[0].vector_mask);
            assert_eq!(current.layers[0].generation, doc.layers[0].generation);
        }
    }
}
