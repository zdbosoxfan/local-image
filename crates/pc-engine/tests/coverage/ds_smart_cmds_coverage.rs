use std::sync::Arc;

use photocraft_engine::doc::{self, Document, LayerContent, LayerId, SmartObject, SmartSource};
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
fn source_bytes_embedded_round_trip() {
    let doc = test_doc();
    let bytes = Arc::new(smart_cmds::encode_source(&doc).expect("encode"));
    let source = SmartSource::Embedded { file_name: "doc.pcraft".to_string(), bytes: bytes.clone() };

    let (name, out) = smart_cmds::source_bytes(&doc.metadata, &source).expect("source bytes");
    assert_eq!(name, "doc.pcraft");
    assert_eq!(out.as_ref(), bytes.as_ref());
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

    let img = smart_cmds::source_image("doc.pcraft", &bytes, fmt).expect("source_image");
    assert_eq!(img.bounds, doc.bounds());
    assert!(!img.surface.content_bounds().is_empty());
    assert_eq!(smart_cmds::source_decode_count(&bytes, fmt), 1);

    let _again = smart_cmds::source_image("doc.pcraft", &bytes, fmt).expect("cached");
    assert_eq!(smart_cmds::source_decode_count(&bytes, fmt), 1);
}

#[test]
fn render_embedded_smart_object_returns_finite_surface() {
    let doc = test_doc();
    let bytes = smart_cmds::encode_source(&doc).expect("encode");
    let source = SmartSource::Embedded { file_name: "doc.pcraft".to_string(), bytes: Arc::new(bytes) };
    let sm = SmartObject::new(source, doc::Affine::translate(1.0, 2.0), None);

    let rendered = smart_cmds::render(&doc, &sm).expect("render").expect("embedded source should render");
    let bounds = rendered.content_bounds();
    assert!(!bounds.is_empty());

    let data = rendered.read_region(bounds);
    assert!(!data.is_empty());
    for v in &data {
        assert!(v.is_finite());
    }
    // Solid red: first channel is high in RGB(A).
    assert!(data[0] > 0.5);
    if rendered.format().channels() == 4 {
        assert!(data[3] > 0.5);
    }
}

#[test]
fn render_missing_linked_source_returns_none() {
    let doc = test_doc();
    let sm = SmartObject::new(SmartSource::Linked { path: "/definitely/missing/nowhere".to_string() }, doc::Affine::translate(0.0, 0.0), None);

    assert!(smart_cmds::render(&doc, &sm).expect("render").is_none());
}

#[test]
fn layer_to_smart_produces_smart_object_with_cache() {
    let doc = test_doc();
    let raster = doc.layers.first().expect("decoded BMP should have a layer");

    let smart_layer = smart_cmds::layer_to_smart(&doc, raster).expect("layer_to_smart");
    let LayerContent::Smart(sm) = &smart_layer.content else {
        panic!("expected smart layer");
    };

    assert!(sm.cache.is_some());
    match &sm.source {
        SmartSource::Embedded { file_name, bytes } => {
            assert!(file_name.ends_with(".pcraft"));
            assert!(!bytes.is_empty());
        }
        _ => panic!("expected embedded source"),
    }
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
fn specs_are_well_formed() {
    let specs = smart_cmds::specs();
    assert!(!specs.is_empty());

    let mut ids = std::collections::HashSet::new();
    for spec in &specs {
        assert!(!spec.id.is_empty());
        assert!(!spec.label.is_empty());
        assert!(!spec.params.is_empty());
        assert!(ids.insert(&spec.id));
    }
}

#[test]
fn session_convert_to_smart_object_undo_restores_raster() {
    let mut s = Session::new();
    s.add_document(test_doc(), None);

    let original_id = s.active().expect("active").active_layer.expect("active layer");

    let result = s.execute("layer.smartObjects.convertToSmartObject", serde_json::json!({})).expect("convert");
    let smart_id = result["layer"].as_u64().expect("layer id");

    {
        let st = s.active().expect("active");
        let smart_layer = st.doc.layer(LayerId(smart_id)).expect("smart layer");
        assert!(matches!(smart_layer.content, LayerContent::Smart(_)));
        assert!(st.doc.layer(original_id).is_none());
    }

    assert!(s.undo());
    {
        let st = s.active().expect("active");
        let raster_layer = st.doc.layer(original_id).expect("original raster layer");
        assert!(matches!(raster_layer.content, LayerContent::Raster(_)));
        assert!(st.doc.layer(LayerId(smart_id)).is_none());
    }
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
