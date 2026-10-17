use std::sync::Arc;

use photocraft_engine::doc::{self, Document, LayerContent, SmartObject, SmartSource};
use photocraft_engine::smart_cmds;
use photocraft_engine::{EngineError, Session};

/// Creates a small 24-bit BMP (2x2 solid colour) as synthetic import input.
fn bmp(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let row_size = (3 * width).div_ceil(4) * 4;
    let pixel_data_size = row_size * height;
    let file_size = 54 + pixel_data_size;
    let mut v = Vec::with_capacity(file_size as usize);

    // BITMAPFILEHEADER
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&file_size.to_le_bytes());
    v.extend_from_slice(&[0u8; 4]); // reserved
    v.extend_from_slice(&54u32.to_le_bytes());

    // BITMAPINFOHEADER
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&width.to_le_bytes());
    v.extend_from_slice(&height.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // planes
    v.extend_from_slice(&24u16.to_le_bytes()); // bpp
    v.extend_from_slice(&0u32.to_le_bytes()); // compression
    v.extend_from_slice(&pixel_data_size.to_le_bytes());
    v.extend_from_slice(&2835u32.to_le_bytes()); // x ppm
    v.extend_from_slice(&2835u32.to_le_bytes()); // y ppm
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());

    // Pixel data: bottom-up, BGR, rows padded to 4 bytes.
    for _ in 0..height {
        for _ in 0..width {
            v.push(rgb[2]);
            v.push(rgb[1]);
            v.push(rgb[0]);
        }
        let pad = (row_size - 3 * width) as usize;
        v.extend(std::iter::repeat_n(0u8, pad));
    }

    v
}

fn test_doc() -> Document {
    let bytes = bmp(2, 2, [255, 0, 0]);
    smart_cmds::decode_source("red.bmp", &bytes).expect("test BMP should decode")
}

#[test]
fn encode_then_decode_source_round_trip() {
    let doc = test_doc();
    let bytes = smart_cmds::encode_source(&doc).expect("encode_source");
    assert!(!bytes.is_empty());

    let decoded = smart_cmds::decode_source("roundtrip.pcraft", &bytes).expect("decode_source");
    assert_eq!(doc.bounds(), decoded.bounds());
    assert_eq!(doc.layers.len(), decoded.layers.len());
}

#[test]
fn decode_source_malformed_input_returns_error() {
    let err = smart_cmds::decode_source("bad.bmp", b"not an image").unwrap_err();
    assert!(matches!(err, EngineError::Other(_)));

    let err = smart_cmds::decode_source("empty.pcraft", b"").unwrap_err();
    assert!(matches!(err, EngineError::Other(_)));
}

#[test]
fn source_bytes_linked_missing_returns_none() {
    let doc = test_doc();
    let source = SmartSource::Linked { path: "/definitely/missing/nowhere".to_string() };
    assert!(smart_cmds::source_bytes(&doc.metadata, &source).is_none());
}

#[test]
fn source_image_caches_by_content_hash() {
    let mut doc = test_doc();
    // Make this document's encoded bytes unique so the static cache is predictable.
    doc.id = doc::DocId::fresh();
    let bytes = smart_cmds::encode_source(&doc).expect("encode");
    let fmt = doc.pixel_format();

    assert_eq!(smart_cmds::source_decode_count(&bytes, fmt), 0);

    let working = photocraft_engine::color_cmds::working_profile(fmt.mode);
    let img = smart_cmds::source_image_in_profile("doc.pcraft", &bytes, fmt, &working).expect("source_image");
    assert_eq!(img.bounds, doc.bounds());
    assert!(!img.surface.content_bounds().is_empty());
    assert_eq!(smart_cmds::source_decode_count(&bytes, fmt), 1);

    let _again = smart_cmds::source_image_in_profile("doc.pcraft", &bytes, fmt, &working).expect("cached");
    assert_eq!(smart_cmds::source_decode_count(&bytes, fmt), 1);
}

#[test]
fn render_missing_linked_source_returns_none() {
    let doc = test_doc();
    let sm = SmartObject::new(SmartSource::Linked { path: "/definitely/missing/nowhere".to_string() }, doc::Affine::translate(0.0, 0.0), None);

    assert!(smart_cmds::render(&doc, &sm).expect("render").is_none());
}

#[test]
fn refresh_layer_on_non_smart_layer_returns_false() {
    let doc = test_doc();
    let mut layer = doc.layers.first().expect("layer").clone();

    let ok = smart_cmds::refresh_layer(&doc, &mut layer).expect("refresh_layer");
    assert!(!ok);
}

#[test]
fn refresh_layer_with_missing_source_returns_false() {
    let doc = test_doc();
    let raster = doc.layers.first().expect("layer");
    let mut layer = smart_cmds::layer_to_smart(&doc, raster).expect("layer_to_smart");

    if let LayerContent::Smart(sm) = &mut layer.content {
        sm.source = SmartSource::Linked { path: "/definitely/missing/nowhere".to_string() };
    }

    let ok = smart_cmds::refresh_layer(&doc, &mut layer).expect("refresh_layer");
    assert!(!ok);
}

#[test]
fn placement_affine_translation_is_represented_exactly() {
    let sm =
        SmartObject::new(SmartSource::Embedded { file_name: "f.pcraft".to_string(), bytes: Arc::new(vec![1, 2, 3]) }, doc::Affine::translate(3.0, -5.0), None);

    let m = smart_cmds::placement(&sm).0;
    assert!((m[0] - 1.0).abs() < 1e-12);
    assert!((m[2] - 3.0).abs() < 1e-12);
    assert!((m[4] - 1.0).abs() < 1e-12);
    assert!((m[5] + 5.0).abs() < 1e-12);
    assert!(m[6].abs() < 1e-12);
    assert!(m[7].abs() < 1e-12);
}

#[test]
fn session_replace_contents_missing_file_errors_and_does_not_change_document() {
    let mut s = Session::new();
    s.add_document(test_doc(), None);
    s.execute("layer.smartObjects.convertToSmartObject", serde_json::json!({})).expect("convert");

    let revision_before = s.active().expect("active").revision;
    let err = s.execute("layer.smartObjects.replaceContents", serde_json::json!({"path": "/definitely/missing/file.png"})).unwrap_err();

    assert!(matches!(err, EngineError::Other(_)));
    assert_eq!(s.active().expect("active").revision, revision_before);
}

#[test]
fn commit_child_on_non_child_document_returns_false() {
    let mut s = Session::new();
    s.add_document(test_doc(), None);

    let ok = smart_cmds::commit_child(&mut s, 0).expect("commit_child");
    assert!(!ok);
}
