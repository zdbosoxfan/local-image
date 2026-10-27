#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
fn session() -> Session {
    let mut doc = Document::new("Export", Size::new(96, 96), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(Layer::new(
        "Curve",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(12., 12., 64., 64.),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    ));
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn request(s: &Session) -> Request {
    let st = s.active().unwrap();
    Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Selection { layers: st.selected_layers() }, options: Default::default() }
}
#[test]
fn pdf_export_command_is_detached_and_non_journaled() {
    let mut s = session();
    let r = request(&s);
    let before = s.active().unwrap().doc.clone();
    let history = s.active().unwrap().history.past_len();
    let result = s.execute("file.pdf.export", serde_json::to_value(&r).unwrap()).unwrap();
    assert_eq!(result["mime"], "application/pdf");
    assert!(!result["bytes"].as_array().unwrap().is_empty());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, r.revision);
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert!(s.journal.is_empty());
}
#[test]
fn pdf_export_rejects_stale_and_entire_missing_target_set() {
    let s = session();
    let mut r = request(&s);
    r.revision += 1;
    assert!(export(&s, &r, &|| false).is_err());
    r = request(&s);
    if let Scope::Selection { layers } = &mut r.scope {
        layers.push(LayerId(0));
    }
    assert!(export(&s, &r, &|| false).is_err());
    r = request(&s);
    r.document += 1;
    assert!(export(&s, &r, &|| false).is_err());
}
#[test]
fn pdf_export_cancelled_has_no_side_effects() {
    let s = session();
    let r = request(&s);
    assert!(export(&s, &r, &|| true).is_err());
    assert_eq!(s.active().unwrap().revision, r.revision);
    assert!(s.journal.is_empty());
}
#[test]
fn pdf_export_old_options_load_with_defaults() {
    assert_eq!(serde_json::from_str::<ExportOptions>("{}").unwrap(), ExportOptions::default());
}

#[test]
fn pdf_export_base64_is_explicit_and_legacy_arrays_stay_compatible() {
    let mut s = session();
    let request = request(&s);
    let mut params = serde_json::to_value(&request).unwrap();
    let legacy = s.execute("file.pdf.export", params.clone()).unwrap();
    assert!(legacy["bytes"].is_array());
    assert!(legacy.get("encoding").is_none());
    params["encoding"] = serde_json::json!("base64");
    let encoded = s.execute("file.pdf.export", params.clone()).unwrap();
    assert_eq!(encoded["encoding"], "base64");
    let bytes = photocraft_paint::tile::b64_decode(encoded["bytes"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let old: Vec<u8> = serde_json::from_value(legacy["bytes"].clone()).unwrap();
    assert_eq!(bytes, old);
    params["encoding"] = serde_json::json!("invalid");
    assert!(s.execute("file.pdf.export", params).is_err());
}

fn exported_xmp(bytes: &[u8]) -> String {
    let opening = b"<x:xmpmeta";
    let closing = b"</x:xmpmeta>";
    let start = bytes.windows(opening.len()).position(|window| window == opening).unwrap();
    let end = start + bytes[start..].windows(closing.len()).position(|window| window == closing).unwrap() + closing.len();
    String::from_utf8(bytes[start..end].to_vec()).unwrap()
}

#[test]
fn pdf_export_includes_only_the_selected_file_info_fields() {
    use serde_json::json;
    for status in ["unknown", "copyrighted", "publicDomain"] {
        let mut session = session();
        let info = json!({"author": "Renée & Co", "description": "Façade <A>", "keywords": ["one & two", "日本語"],
            "copyrightStatus": status, "copyright": "© Studio", "copyrightUrl": "https://example.test/?a=1&b=2"});
        session.execute("file.fileInfo", info.clone()).unwrap();
        session
            .edit("metadata", |document, _| {
                let mut metadata = lightcraft_meta::parse_xmp(document.metadata.xmp.as_deref().unwrap()).unwrap().metadata;
                metadata.make = Some("Private camera".into());
                metadata.rating = Some(4);
                metadata.label = Some("Private label".into());
                metadata.hierarchical_keywords = vec!["Private|Hierarchy".into()];
                metadata.gps = Some(lightcraft_meta::Gps { latitude: 42.0, longitude: -71.0, altitude: Some(8.0) });
                document.metadata.xmp = Some(lightcraft_meta::write_xmp(&metadata, Some("{\"exposure\":1}")));
                Ok(())
            })
            .unwrap();
        let before = session.active().unwrap().doc.clone();
        let output = export(&session, &request(&session), &|| false).unwrap();
        let packet = exported_xmp(&output.bytes);
        let actual = lightcraft_meta::read_file_info(Some(&packet));
        for key in ["author", "description", "keywords", "copyrightStatus", "copyright", "copyrightUrl"] {
            assert_eq!(actual[key], info[key], "{status}: {key}");
        }
        let parsed = lightcraft_meta::parse_xmp(&packet).unwrap();
        assert!(parsed.properties.keys().all(|key| !["exif:", "tiff:", "lc:", "lr:", "xmp:Rating", "xmp:Label"].iter().any(|prefix| key.starts_with(prefix))));
        assert_eq!(session.active().unwrap().doc, before);
    }
}

#[test]
fn pdf_export_rejects_malformed_file_info_without_mutation() {
    let mut session = session();
    session
        .edit("metadata", |document, _| {
            document.metadata.xmp = Some("<invalid".into());
            Ok(())
        })
        .unwrap();
    let before = session.active().unwrap().doc.clone();
    let error = export(&session, &request(&session), &|| false).unwrap_err();
    assert!(error.to_string().contains("PDF File Info could not be read"));
    assert_eq!(session.active().unwrap().doc, before);
}
