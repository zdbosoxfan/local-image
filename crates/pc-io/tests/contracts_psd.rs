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
#[ignore = "known defect: PSD import synthesizes postscript_name for text runs that stored None"]
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
#[ignore = "known structural gap: independent shape masks use the saved clipped-raster fallback; editable shape is lost"]
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
        smart.develop = Some(DevelopLink { settings: serde_json::to_value(settings).unwrap(), photo: Some(42) });
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
#[ignore = "known defect: PSD smart import drops an explicitly stored all-white smart-filter mask"]
fn psd_smart_embedded_source_transform_filters_mask_and_cache() {
    smart_contract(false);
}
#[test]
#[ignore = "known defect: PSD Develop import drops an explicitly stored all-white smart-filter mask"]
fn psd_develop_settings_photo_link_and_embedded_source() {
    smart_contract(true);
}
