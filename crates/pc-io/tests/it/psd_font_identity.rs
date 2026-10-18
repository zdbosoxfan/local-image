use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::text::{CharStyle, TextRun};
use photocraft_doc::{Document, Layer, LayerContent, TextLayer};
use photocraft_geom::Size;

#[test]
fn psd_reopen_preserves_hyphenated_postscript_family_and_style() {
    let mut doc = Document::new("font", Size::new(32, 32), ColorMode::Rgb, SampleType::U8);
    let mut text = TextLayer {
        text: "A".into(),
        runs: vec![TextRun {
            len: 1,
            style: CharStyle {
                font_family: "Vendor-Family Pro".into(),
                postscript_name: Some("Vendor-FamilyPro-BoldIt".into()),
                weight: 700,
                italic: true,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    text.sync_summary();
    doc.layers.push(Layer::new("type", LayerContent::Text(text)));
    let bytes = photocraft_io::export(&doc, "psd", &Default::default()).unwrap().bytes;
    let reopened = photocraft_io::import("font.psd", &bytes).unwrap().document;
    let LayerContent::Text(text) = &reopened.layers[0].content else { panic!("editable text") };
    let style = &text.char_runs()[0].style;
    assert_eq!(style.font_family, "Vendor-Family Pro");
    assert_eq!(style.postscript_name.as_deref(), Some("Vendor-FamilyPro-BoldIt"));
    assert_eq!(style.weight, 700);
    assert!(style.italic);
}
