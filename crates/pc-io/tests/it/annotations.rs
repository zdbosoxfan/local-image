//! Notes (`Anno`) and the measurement scale (resource 1074) through PSD: verbatim while
//! unchanged, regenerated after edits.

#[cfg(feature = "corpus")]
use std::path::PathBuf;

use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, MeasurementScale, Note};
use photocraft_geom::Size;
use photocraft_io::annotations_map::MEASUREMENT_SCALE;
use photocraft_io::{ExportOptions, export, import};
use photocraft_psd::PsdFile;

#[cfg(feature = "corpus")]
fn corpus(rel: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/psd").join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}: run `cargo xtask corpus --all`", p.display()))
}

fn to_psd(doc: &Document) -> Vec<u8> {
    export(doc, "x.psd", &ExportOptions::default()).expect("export").bytes
}

#[cfg(feature = "corpus")]
#[test]
fn corpus_notes_import_and_verbatim_export() {
    let bytes = corpus("ag-psd/read-write/annotations/src.psd");
    let src = PsdFile::from_bytes(&bytes).unwrap();
    let mut doc = import("src.psd", &bytes).unwrap().document;
    assert_eq!(doc.notes.len(), 2);
    assert_eq!(doc.notes[1].text, "open note");
    let out = to_psd(&doc);
    let f = PsdFile::from_bytes(&out).unwrap();
    assert_eq!(f.global_block(b"Anno").unwrap().data, src.global_block(b"Anno").unwrap().data);

    // Edit a note: the block is regenerated and reads back with the change.
    doc.notes[0].text = "changed".into();
    doc.notes.push(Note { author: "agent".into(), text: "third".into(), position: [5.0, 6.0], ..Default::default() });
    let back = import("x.psd", &to_psd(&doc)).unwrap().document;
    assert_eq!(back.notes.len(), 3);
    assert_eq!(back.notes[0].text, "changed");
    assert_eq!(back.notes[2].position, [5.0, 6.0]);
    // Deleting every note drops the block.
    doc.notes.clear();
    let f = PsdFile::from_bytes(&to_psd(&doc)).unwrap();
    assert!(f.global_block(b"Anno").is_none());
}

#[test]
fn measurement_scale_roundtrips_through_psd() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        let mut doc = Document::with_background("m", Size::new(16, 8), ColorMode::Rgb, depth, photocraft_color::Color::rgb(1.0, 1.0, 1.0));
        let f = PsdFile::from_bytes(&to_psd(&doc)).unwrap();
        assert!(f.resource(MEASUREMENT_SCALE).is_none());
        doc.measurement.scale = MeasurementScale { pixel_length: 120.0, logical_length: 3.0, units: "cm".into() };
        doc.notes.push(Note { text: "n".into(), ..Default::default() });
        let back = import("x.psd", &to_psd(&doc)).unwrap().document;
        assert_eq!(back.measurement.scale, doc.measurement.scale);
        assert_eq!(back.notes.len(), 1);
        assert_eq!((back.notes[0].text.as_str(), back.notes[0].position), ("n", [0.0, 0.0]));
        // Unchanged → the preserved resource is written byte-exact (once).
        let again = PsdFile::from_bytes(&to_psd(&back)).unwrap();
        assert_eq!(again.resources.iter().filter(|r| r.id == MEASUREMENT_SCALE).count(), 1);
    }
}

/// A PSD holding `anno` as its global `Anno` block.
fn psd_with_anno(anno: Vec<u8>) -> Vec<u8> {
    let mut doc = Document::new("notes", Size::new(16, 16), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(photocraft_doc::Layer::raster("Background", doc.pixel_format()));
    let mut file = PsdFile::from_bytes(&to_psd(&doc)).unwrap();
    let mut tb = photocraft_psd::TaggedBlock::new(*b"Anno", anno.clone());
    tb.padding = Some(vec![0; (4 - anno.len() % 4) % 4]);
    file.global_blocks.push(tb);
    file.to_bytes().unwrap()
}

/// Upstream issue #944: an `Anno` block the reader rejects opened as "no notes" without a word,
/// and the next save deleted it. It now opens with a warning and is written back byte for byte.
#[test]
fn an_unreadable_anno_block_warns_and_survives_a_save() {
    // Two annotations declared, one truncated entry present.
    let broken = vec![0, 2, 0, 1, 0, 0, 0, 9];
    let imp = import("notes.psd", &psd_with_anno(broken.clone())).expect("import");
    assert!(imp.document.notes.is_empty());
    assert!(imp.warnings.iter().any(|w| w.contains("Anno")), "{:?}", imp.warnings);
    let f = PsdFile::from_bytes(&to_psd(&imp.document)).unwrap();
    assert_eq!(f.global_block(b"Anno").expect("Anno block kept").data, broken);
    // Control: a document with no Anno block reads with no warning.
    let none = import("none.psd", &psd_with_anno_free()).expect("import");
    assert!(!none.warnings.iter().any(|w| w.contains("Anno")));
}

fn psd_with_anno_free() -> Vec<u8> {
    let mut doc = Document::new("plain", Size::new(16, 16), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(photocraft_doc::Layer::raster("Background", doc.pixel_format()));
    to_psd(&doc)
}
