//! Layered TIFF (Photoshop layer data in tags 37724 / 34377): Document → TIFF → Document round
//! trips in every mode and depth, equivalence with the PSD path, the flat fall-backs, files
//! written by an independent implementation (psdtags, both byte orders), and malformed input.

use crate::common;

use common::*;
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, LayerContent};
use photocraft_io::*;
use photocraft_psd::tiff::{ByteOrder, ImageSourceData};

fn tiff(doc: &Document, layers: bool) -> ExportResult {
    export(doc, "x.tif", &ExportOptions { tiff_layers: layers, ..Default::default() }).expect("export")
}

/// The two Photoshop tags of a TIFF: (resources, layers).
fn tags(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (r, l) = photocraft_codecs::tiff_photoshop_tags(bytes);
    (r.map(<[u8]>::to_vec), l.map(<[u8]>::to_vec))
}

fn roundtrip(doc: &Document) -> Document {
    let r = tiff(doc, true);
    let (resources, layers) = tags(&r.bytes);
    let layers = layers.expect("layer data");
    assert!(resources.is_some_and(|r| !r.is_empty()), "image resources");
    // The layer data matches the container's byte order and is byte-stable through the psd crate.
    let order = if photocraft_codecs::tiff_writes_little_endian() { ByteOrder::Little } else { ByteOrder::Big };
    let (isd, w) = ImageSourceData::from_bytes(&layers).expect("parse layer data");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(isd.byte_order, order);
    assert_eq!(isd.to_bytes(order).unwrap().0, layers);
    // The TIFF is a plain image to any other reader: the composite at the document's size.
    let flat = photocraft_codecs::decode(&r.bytes).expect("flat decode");
    assert_eq!(flat.dimensions(), (doc.size.width, doc.size.height));
    // A TIFF holds one transparency channel: extra alpha channels (and a Quick Mask) are not
    // kept, and the export says so.
    let extra = doc.channels.len() + usize::from(doc.quick_mask.is_some());
    assert_eq!(r.warnings.iter().any(|w| w.contains("alpha channel(s) are not written")), extra > 0, "{:?}", r.warnings);
    import("x.tif", &r.bytes).expect("import").document
}

/// `doc` as a layered TIFF round trip returns it: without extra alpha channels or a Quick Mask.
fn expected(doc: &Document) -> Document {
    let mut d = doc.clone();
    d.channels.clear();
    d.quick_mask = None;
    d
}

macro_rules! rt_case {
    ($name:ident, $mode:expr, $depth:expr, $f:expr) => {
        #[test]
        fn $name() {
            let d = gen_doc($mode, $depth, $f);
            let back = roundtrip(&d);
            assert_docs_eq(&expected(&d), &back);
        }
    };
}

rt_case!(rt_rgb8_all, ColorMode::Rgb, SampleType::U8, Features::ALL);
rt_case!(rt_rgb16_all, ColorMode::Rgb, SampleType::U16, Features::ALL);
rt_case!(rt_rgb32_all, ColorMode::Rgb, SampleType::F32, Features::ALL);
rt_case!(rt_gray8_all, ColorMode::Grayscale, SampleType::U8, Features::ALL);
rt_case!(rt_gray16_all, ColorMode::Grayscale, SampleType::U16, Features::ALL);
rt_case!(rt_gray32_all, ColorMode::Grayscale, SampleType::F32, Features::ALL);
rt_case!(rt_cmyk8_all, ColorMode::Cmyk, SampleType::U8, Features::ALL);
rt_case!(rt_cmyk16_all, ColorMode::Cmyk, SampleType::U16, Features::ALL);
rt_case!(rt_rgb8_pixels, ColorMode::Rgb, SampleType::U8, Features::PIXELS);
rt_case!(rt_gray16_pixels, ColorMode::Grayscale, SampleType::U16, Features::PIXELS);
rt_case!(rt_cmyk8_pixels, ColorMode::Cmyk, SampleType::U8, Features::PIXELS);

#[test]
fn tiff_and_psd_open_as_the_same_document() {
    for (mode, depth) in [(ColorMode::Rgb, SampleType::U8), (ColorMode::Cmyk, SampleType::U16), (ColorMode::Grayscale, SampleType::F32)] {
        let d = gen_doc(mode, depth, Features::ALL);
        let psd = export(&d, "x.psd", &ExportOptions::default()).unwrap();
        let from_psd = import("x.psd", &psd.bytes).unwrap().document;
        let from_tif = roundtrip(&d);
        assert_docs_eq(&expected(&from_psd), &from_tif);
        // The composite the TIFF carries is what the document looks like.
        let tif = tiff(&d, true);
        let flat = import(
            "flat.tif",
            &photocraft_codecs::encode(&photocraft_codecs::decode(&tif.bytes).unwrap(), photocraft_codecs::Format::Tiff, &Default::default()).unwrap(),
        )
        .unwrap()
        .document;
        let a = photocraft_compose::flatten(&d).px;
        let b = photocraft_compose::flatten(&flat).px;
        assert!(max_diff(&a, &b) <= 2.0 / 255.0, "{mode:?} {depth:?}: composite differs by {}", max_diff(&a, &b));
    }
}

#[test]
fn every_compression_keeps_the_layers() {
    use photocraft_codecs::TiffCompression;
    let d = gen_doc(ColorMode::Rgb, SampleType::U16, Features::PIXELS);
    for c in [TiffCompression::None, TiffCompression::Lzw, TiffCompression::Deflate, TiffCompression::PackBits] {
        let mut opts = ExportOptions { tiff_layers: true, ..Default::default() };
        opts.encode.tiff_compression = c;
        let r = export(&d, "x.tiff", &opts).unwrap();
        let back = import("x.tiff", &r.bytes).unwrap().document;
        assert_docs_eq(&d, &back);
    }
}

#[test]
fn discard_layers_writes_a_flat_file() {
    let d = gen_doc(ColorMode::Rgb, SampleType::U8, Features::PIXELS);
    let r = tiff(&d, false);
    assert_eq!(tags(&r.bytes), (None, None));
    assert!(r.warnings.iter().any(|w| w.contains("flattened")), "{:?}", r.warnings);
    let back = import("x.tif", &r.bytes).unwrap().document;
    assert_eq!(back.layers.len(), 1);
    assert!(!tiff_layers::would_write_layers(&back));
}

#[test]
fn a_plain_background_needs_no_layer_data() {
    let d = import("x.png", &export(&gen_doc(ColorMode::Rgb, SampleType::U8, Features::PIXELS), "x.png", &Default::default()).unwrap().bytes).unwrap().document;
    assert_eq!(d.layers.len(), 1);
    assert!(!tiff_layers::would_write_layers(&d));
    let r = tiff(&d, true);
    assert_eq!(tags(&r.bytes), (None, None));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let back = import("x.tif", &r.bytes).unwrap().document;
    assert_docs_eq(&d, &back);
}

#[test]
fn lab_documents_are_saved_flat_with_a_warning() {
    let d = gen_doc(ColorMode::Lab, SampleType::U8, Features::PIXELS);
    let r = tiff(&d, true);
    assert_eq!(tags(&r.bytes), (None, None));
    assert!(r.warnings.iter().any(|w| w.contains("Lab") && w.contains("layers are not kept")), "{:?}", r.warnings);
    assert!(import("x.tif", &r.bytes).is_ok());
}

#[test]
fn transparency_is_straight_in_the_composite() {
    // A translucent layer over nothing: the TIFF's composite keeps its colour under the alpha
    // instead of being matted against white like a PSD's merged image.
    let mut d = Document::new("t", photocraft_geom::Size::new(4, 4), ColorMode::Rgb, SampleType::U8);
    let fmt = d.pixel_format();
    let mut l = raster("Red", fmt, photocraft_geom::Rect::new(0, 0, 4, 4), 0, false);
    if let LayerContent::Raster(s) = &mut l.content {
        let px: Vec<f32> = (0..16).flat_map(|_| [1.0, 0.0, 0.0, 0.5]).collect();
        s.write_region(photocraft_geom::Rect::new(0, 0, 4, 4), &px);
    }
    d.layers.push(l);
    let r = tiff(&d, true);
    let flat = photocraft_codecs::decode(&r.bytes).unwrap();
    assert!(flat.layout().has_alpha());
    let px = flat.data();
    assert_eq!(&px[..4], &[255, 0, 0, 128]);
    let back = import("x.tif", &r.bytes).unwrap().document;
    assert_docs_eq(&d, &back);
}

#[test]
fn big_documents_use_the_psb_signature() {
    // Forcing PSB stands in for a > 30000 px document.
    let d = gen_doc(ColorMode::Rgb, SampleType::U8, Features::PIXELS);
    let r = export(&d, "x.tif", &ExportOptions { force_psb: true, tiff_layers: true, ..Default::default() }).unwrap();
    let (_, layers) = tags(&r.bytes);
    assert!(layers.unwrap().starts_with(photocraft_psd::tiff::SIGNATURE_PSB));
    let back = import("x.tif", &r.bytes).unwrap().document;
    assert_docs_eq(&d, &back);
}

/// Files written by psdtags (`scripts/layered_tiff_fixtures.py`): two RGB layers, "Background"
/// (RLE) and an offset, translucent "Upper é" (ZIP, Multiply, opacity 200), in both byte orders
/// at 8 and 16 bits. Binary files stay out of this repository: run the script to write them
/// to `corpus/layered-tiff/` (gitignored); without them these checks are skipped.
fn fixture(name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(format!("{}/../../corpus/layered-tiff/{name}", env!("CARGO_MANIFEST_DIR"))).ok();
    if bytes.is_none() {
        eprintln!("skipped {name}: run `python scripts/layered_tiff_fixtures.py` to write corpus/layered-tiff/");
    }
    bytes
}

#[test]
fn psdtags_fixtures_open_with_their_layers() {
    for (name, depth) in [
        ("layered-le-8bit.tif", SampleType::U8),
        ("layered-be-8bit.tif", SampleType::U8),
        ("layered-le-16bit.tif", SampleType::U16),
        ("layered-be-16bit.tif", SampleType::U16),
    ] {
        let Some(bytes) = fixture(name) else { continue };
        let r = import(name, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let d = &r.document;
        assert_eq!(d.size, photocraft_geom::Size::new(6, 4), "{name}");
        assert_eq!(d.mode, ColorMode::Rgb);
        assert_eq!(d.depth, depth, "{name}");
        let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Background", "Upper \u{e9}"], "{name}");
        let upper = &d.layers[1];
        assert_eq!(upper.blend, photocraft_color::BlendMode::Multiply);
        assert!((upper.opacity - 200.0 / 255.0).abs() < 1e-6);
        let LayerContent::Raster(s) = &upper.content else { panic!("raster") };
        let bounds = s.content_bounds();
        assert_eq!((bounds.x0, bounds.y0, bounds.width(), bounds.height()), (1, 1, 4, 3), "{name}");
        // Pixel (x=2, y=0) of the upper layer: the generator's ramp, alpha 160/255.
        let px = s.pixel(3, 1);
        let expect = |v: u32| (v % 256) as f32 / 255.0;
        assert!((px[0] - expect(2 * 40 + 7)).abs() < 1e-3, "{name}: {px:?}");
        assert!((px[1] - expect(7 * 3)).abs() < 1e-3, "{name}: {px:?}");
        assert!((px[3] - 160.0 / 255.0).abs() < 1e-3, "{name}: {px:?}");
        let LayerContent::Raster(bg) = &d.layers[0].content else { panic!("raster") };
        let b = bg.pixel(5, 3);
        assert!((b[0] - expect(5 * 40 + 1)).abs() < 1e-3, "{name}: {b:?}");
        assert!((b[3] - 1.0).abs() < 1e-6);
        assert_eq!(d.resolution_dpi, 300.0);
        // tifffile writes a Software tag; nothing else is worth a warning.
        assert!(r.warnings.iter().all(|w| w.contains("text metadata")), "{name}: {:?}", r.warnings);
        // psdtags wrote the composite's alpha sample without the transparency flag, so by the
        // PSD rules it is an alpha channel (which a re-save cannot keep). Re-saving keeps
        // everything else; the file is then in the encoder's byte order.
        assert_eq!(d.channels.len(), 1, "{name}");
        let out = tiff(d, true);
        let back = import(name, &out.bytes).unwrap().document;
        assert_docs_eq(&expected(d), &back);
    }
}

#[test]
fn damaged_layer_data_opens_flattened() {
    let d = gen_doc(ColorMode::Rgb, SampleType::U8, Features::PIXELS);
    let r = tiff(&d, true);
    let (_, layers) = tags(&r.bytes);
    let layers = layers.unwrap();
    let at = r.bytes.windows(layers.len()).position(|w| w == &layers[..]).unwrap();
    // Garbage after the signature: no layers, a warning, the composite as one layer.
    let mut bad = r.bytes.clone();
    for b in &mut bad[at + 36..at + 36 + 24] {
        *b ^= 0xa5;
    }
    let r2 = import("x.tif", &bad).unwrap();
    assert_eq!(r2.document.layers.len(), 1);
    assert!(r2.warnings.iter().any(|w| w.contains("could not be read")), "{:?}", r2.warnings);
    // Every truncation and a sweep of bit flips through the whole file: never a panic.
    for cut in (0..r.bytes.len()).step_by(11) {
        let _ = import("x.tif", &r.bytes[..cut]);
    }
    for i in (0..r.bytes.len()).step_by(29) {
        let mut b = r.bytes.clone();
        b[i] ^= 0x5a;
        let _ = import("x.tif", &b);
    }
    for name in ["layered-le-8bit.tif", "layered-be-16bit.tif"] {
        let Some(bytes) = fixture(name) else { continue };
        for cut in (0..bytes.len()).step_by(7) {
            let _ = import(name, &bytes[..cut]);
        }
        for i in (0..bytes.len()).step_by(5) {
            let mut b = bytes.clone();
            b[i] ^= 0xff;
            let _ = import(name, &b);
        }
    }
}

#[test]
fn default_export_options_write_a_flat_tiff() {
    // Scripted and agent saves (CLI, batch, MCP) build on the defaults: layers only on request.
    let d = gen_doc(ColorMode::Rgb, SampleType::U8, Features::PIXELS);
    assert!(tiff_layers::would_write_layers(&d));
    let r = export(&d, "x.tif", &ExportOptions::default()).unwrap();
    assert_eq!(tags(&r.bytes), (None, None));
    assert_eq!(import("x.tif", &r.bytes).unwrap().document.layers.len(), 1);
}

#[test]
fn private_layer_and_global_blocks_keep_bytes_and_source_order_over_two_tiff_saves() {
    use photocraft_doc::{Layer, Surface};
    use photocraft_psd::TaggedBlock;
    const LAYER: &[u8] = b"layer\x80\0";
    const GLOBAL: &[u8] = b"global\xfe\0";
    fn payload(key: [u8; 4], bytes: &[u8], wanted: [u8; 4]) -> Option<(u8, Vec<u8>)> {
        if key == wanted {
            return Some((0, bytes.to_vec()));
        }
        if key == *b"liOP" && bytes.starts_with(b"LI-TIFF-OPAQUE\0\x01") && bytes.get(24..28) == Some(wanted.as_slice()) {
            let len = u32::from_be_bytes(bytes[28..32].try_into().unwrap()) as usize;
            return Some((bytes[16], bytes[32..32 + len].to_vec()));
        }
        None
    }
    fn container(order: ByteOrder, layers: &[u8]) -> Vec<u8> {
        fn put(bytes: &mut [u8], at: usize, n: usize, value: u32, order: ByteOrder) {
            let raw = if order == ByteOrder::Little { value.to_le_bytes() } else { value.to_be_bytes() };
            bytes[at..at + n].copy_from_slice(if order == ByteOrder::Little { &raw[..n] } else { &raw[4 - n..] });
        }
        let mut bytes = vec![0; 300];
        bytes[..2].copy_from_slice(if order == ByteOrder::Little { b"II" } else { b"MM" });
        put(&mut bytes, 2, 2, 42, order);
        put(&mut bytes, 4, 4, 8, order);
        let fields = [
            (256, 4, 1, 2),
            (257, 4, 1, 2),
            (258, 3, 3, 200),
            (259, 3, 1, 1),
            (262, 3, 1, 2),
            (273, 4, 1, 256),
            (277, 3, 1, 3),
            (278, 4, 1, 2),
            (279, 4, 1, 12),
            (37724, 7, layers.len() as u32, 300),
        ];
        put(&mut bytes, 8, 2, fields.len() as u32, order);
        for (index, (tag, typ, count, value)) in fields.into_iter().enumerate() {
            let at = 10 + 12 * index;
            put(&mut bytes, at, 2, tag, order);
            put(&mut bytes, at + 2, 2, typ, order);
            put(&mut bytes, at + 4, 4, count, order);
            put(&mut bytes, at + 8, if typ == 3 && count == 1 { 2 } else { 4 }, value, order);
        }
        for at in [200, 202, 204] {
            put(&mut bytes, at, 2, 8, order);
        }
        bytes.extend_from_slice(layers);
        bytes
    }
    for order in [ByteOrder::Big, ByteOrder::Little] {
        let mut doc = Document::new("private blocks", photocraft_geom::Size::new(2, 2), ColorMode::Rgb, SampleType::U8);
        let mut pixels = Surface::new(doc.pixel_format());
        pixels.fill_rect(doc.bounds(), &[0.25, 0.5, 0.75, 1.]);
        doc.layers.push(Layer::new("pixels", LayerContent::Raster(pixels)));
        let mut file = document_to_psd(&doc);
        file.layer_info.as_mut().unwrap().layers[0].blocks.push(TaggedBlock::new(*b"ZzLy", LAYER.to_vec()));
        file.global_blocks.push(TaggedBlock::new(*b"ZzGb", GLOBAL.to_vec()));
        let (mut layers, _) = ImageSourceData::from_psd(&file).to_bytes(order).unwrap();
        if order == ByteOrder::Little {
            // Construct native LE opaque fields, not BE fields merely wrapped in LE.
            let positions: Vec<_> = layers.windows(16).enumerate().filter(|(_, bytes)| *bytes == b"LI-TIFF-OPAQUE\0\x01").map(|(at, _)| at).collect();
            for at in positions {
                layers[at + 16] = 1;
            }
            layers = ImageSourceData::from_bytes(&layers).unwrap().0.to_bytes(order).unwrap().0;
        }
        let original = container(order, &layers);
        let mut back = import("source.tif", &original).unwrap().document;
        for _ in 0..2 {
            let layer = back.layers[0].psd_blocks.iter().find_map(|(key, bytes)| payload(*key, bytes, *b"ZzLy")).unwrap();
            let global = back.metadata.psd_global_blocks.iter().find_map(|(_, key, bytes)| payload(*key, bytes, *b"ZzGb")).unwrap();
            assert_eq!(layer, (u8::from(order == ByteOrder::Little), LAYER.to_vec()));
            assert_eq!(global, (u8::from(order == ByteOrder::Little), GLOBAL.to_vec()));
            assert_eq!(back.layers[0].surface().unwrap().to_interleaved(doc.bounds()), doc.layers[0].surface().unwrap().to_interleaved(doc.bounds()));
            let saved = tiff(&back, true);
            back = import("saved.tif", &saved.bytes).unwrap().document;
        }
        let layer = back.layers[0].psd_blocks.iter().find_map(|(key, bytes)| payload(*key, bytes, *b"ZzLy")).unwrap();
        assert_eq!(layer, (u8::from(order == ByteOrder::Little), LAYER.to_vec()));
    }
}

#[test]
fn independent_pdf_groups_export_with_their_whole_document_backdrop() {
    use photocraft_doc::{BlendMode, Color, Fill, GroupIsolation, Layer, Size};
    let mut doc = Document::new("PDF groups", Size::new(4, 4), ColorMode::Rgb, SampleType::F32);
    let mut bg = Color::rgb(0.2, 0.4, 0.6);
    bg.alpha = 0.4;
    let mut child = Layer::new("screen", LayerContent::Fill(Fill::Solid(Color::rgb(0.8, 0.6, 0.4))));
    child.opacity = 0.6;
    child.blend = BlendMode::Screen;
    let mut group = Layer::group("nonisolated", vec![child]);
    group.blend = BlendMode::Multiply;
    group.opacity = 0.5;
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = GroupIsolation::NonIsolated;
    }
    doc.layers = vec![Layer::new("backdrop", LayerContent::Fill(Fill::Solid(bg))), group];
    let before = doc.clone();
    let expected = photocraft_compose::flatten(&doc);
    let (psd, warnings) = document_to_psd_with(&doc, &PsdExportOptions { merged_matte: false, ..Default::default() });
    assert!(warnings.iter().any(|w| w.contains("complete document") && w.contains("flattened")));
    let (loaded, _) = psd_to_document(&psd);
    assert_eq!(loaded.layers.len(), 1);
    let actual = photocraft_compose::flatten(&loaded);
    for (a, b) in actual.px.iter().zip(&expected.px) {
        for c in 0..4 {
            assert!((a[c] - b[c]).abs() < 0.00001);
        }
    }
    let tiff = tiff(&doc, true);
    assert!(tags(&tiff.bytes).1.is_none());
    assert!(tiff.warnings.iter().any(|w| w.contains("complete document") && w.contains("flattened")));
    let loaded = import("groups.tif", &tiff.bytes).unwrap().document;
    let actual = photocraft_compose::flatten(&loaded);
    for (a, b) in actual.px.iter().zip(&expected.px) {
        for c in 0..4 {
            assert!((a[c] - b[c]).abs() < 0.00001);
        }
    }
    assert_eq!(doc, before, "exports must leave the editable native source intact");
}

#[test]
fn normal_nonisolated_groups_retain_editable_psd_and_tiff_layers() {
    use photocraft_doc::{BlendMode, Color, Fill, GroupIsolation, Layer, LayerMask, PixelFormat, Size, Surface};
    let mut doc = Document::new("compatible groups", Size::new(4, 4), ColorMode::Rgb, SampleType::F32);
    let mut bg = Color::rgb(0.2, 0.4, 0.6);
    bg.alpha = 0.4;
    let mut child = Layer::raster("paint", PixelFormat::RGBA32F);
    child.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.8, 0.6, 0.4, 0.6]);
    child.blend = BlendMode::Screen;
    let mut group = Layer::group("normal nonisolated", vec![child]);
    group.blend = BlendMode::Normal;
    group.opacity = 0.6;
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = GroupIsolation::NonIsolated;
    }
    let mut mask = LayerMask::hide_all();
    mask.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
    mask.surface.fill_rect(doc.bounds(), &[0.4]);
    group.mask = Some(mask);
    doc.layers = vec![Layer::new("backdrop", LayerContent::Fill(Fill::Solid(bg))), group];
    let before = doc.clone();
    let expected = photocraft_compose::flatten(&doc);
    let (file, warnings) = document_to_psd_with(&doc, &PsdExportOptions { merged_matte: false, ..Default::default() });
    assert!(!warnings.iter().any(|w| w.contains("Independent PDF group isolation")));
    let (psd, _) = psd_to_document(&file);
    assert!(psd.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Group(_))));
    let tiff = tiff(&doc, true);
    assert!(tags(&tiff.bytes).1.is_some());
    assert!(!tiff.warnings.iter().any(|w| w.contains("Independent PDF group isolation")));
    let tiff = import("groups.tif", &tiff.bytes).unwrap().document;
    for loaded in [psd, tiff] {
        for (a, b) in photocraft_compose::flatten(&loaded).px.iter().zip(&expected.px) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 0.00001);
            }
        }
    }
    assert_eq!(doc, before);
}
