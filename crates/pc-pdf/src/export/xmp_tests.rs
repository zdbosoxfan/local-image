use super::*;
use crate::export::{ExportContext, ExportOptions, export, export_with_info};
use hayro_syntax::{Pdf, object::{Dict, Name, Object, Stream}};
use photocraft_doc::vector_design::exchange::ExportScope;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};

fn document() -> Document {
    let mut document = Document::new("PDF metadata", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
    document.layers.push(Layer::new("Rectangle", LayerContent::Shape(ShapeLayer {
        path: photocraft_vector::shapes::rect(4.0, 4.0, 40.0, 40.0),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    })));
    document
}

fn output(info: &FileInfo, compressed: bool) -> Vec<u8> {
    let mut text = photocraft_text::TextEngine::new();
    export_with_info(&document(), &ExportScope::Document, &ExportOptions { compress: compressed, ..Default::default() },
        &mut ExportContext { cancelled: &|| false, projection: None, text: &mut text }, info).unwrap().bytes
}

fn packet(bytes: &[u8]) -> String {
    let pdf = Pdf::new(bytes.to_vec()).unwrap();
    let catalog = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()).unwrap();
    let stream = catalog.get::<Stream<'_>>(b"Metadata").unwrap();
    String::from_utf8(stream.decoded().unwrap().into_owned()).unwrap()
}

#[test]
fn rights_status_and_escaped_unicode_survive_pdf_import() {
    for compressed in [false, true] {
        for marked in [None, Some(true), Some(false)] {
            let info = FileInfo {
                author: "Renée & Co".into(),
                description: "Layout <A> & façade".into(),
                keywords: vec!["building & site".into(), "日本語".into()],
                copyright_marked: marked,
                copyright: "© Renée <Studio> & \"Partners\"\r2026\u{1}".into(),
                copyright_url: "https://example.test/rights?a=1&b=2".into(),
            };
            let bytes = output(&info, compressed);
            let packet = packet(&bytes);
            assert!(packet.contains("© Renée &lt;Studio&gt; &amp; &quot;Partners&quot;&#xD;2026"));
            assert!(!packet.contains('\u{1}'));
            assert!(packet.contains("dc:creator") && packet.contains("dc:description") && packet.contains("dc:subject"));
            assert!(packet.contains("日本語"));
            match marked {
                Some(true) => assert!(packet.contains(">True</xmpRights:Marked>")),
                Some(false) => assert!(packet.contains(">False</xmpRights:Marked>")),
                None => assert!(!packet.contains("xmpRights:Marked")),
            }
            assert_eq!(Pdf::new(bytes.clone()).unwrap().pages().len(), 1);
            let imported = crate::import::import(&bytes, &Default::default()).unwrap();
            assert_eq!(imported.size, document().size);
        }
    }
}

#[test]
fn metadata_postpass_preserves_artwork_streams_and_legacy_export() {
    let info = FileInfo { copyright: "Rights".into(), keywords: vec!["test".into()], ..Default::default() };
    let plain = export(&document(), &ExportScope::Document, &Default::default()).unwrap().bytes;
    assert_eq!(plain, output(&FileInfo::default(), true));
    let content = |bytes: Vec<u8>| {
        let pdf = Pdf::new(bytes).unwrap();
        pdf.objects().into_iter().filter_map(|object| match object {
            Object::Stream(stream) if !stream.dict().get::<Name<'_>>(b"Type").is_some_and(|name| name.as_ref() == b"Metadata") =>
                Some(stream.decoded().unwrap().into_owned()),
            _ => None,
        }).collect::<Vec<_>>()
    };
    assert_eq!(content(plain), content(output(&info, true)));
    assert!(!packet(&output(&FileInfo::default(), true)).contains("dc:rights"));
}

#[test]
fn metadata_growth_respects_the_output_budget_and_rejects_unknown_writer_layouts() {
    let plain = output(&FileInfo::default(), true);
    let info = FileInfo { copyright: "long copyright notice".repeat(100), ..Default::default() };
    assert!(matches!(apply(plain.clone(), &info, plain.len()), Err(ExportError::Budget("output bytes"))));
    assert!(matches!(metadata("title", &info, 5), Err(ExportError::Budget("metadata bytes"))));
    assert!(matches!(apply(b"%PDF-1.7\ninvalid".to_vec(), &info, 1 << 20), Err(ExportError::Write(_))));
}
