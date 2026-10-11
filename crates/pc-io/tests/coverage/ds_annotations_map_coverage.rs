use std::sync::Arc;

use photocraft_doc::{ColorMode, CountGroup, Document, MeasurementScale, Note, PsdGlobalBlock, SampleType, Size};
use photocraft_io::annotations_map::{
    ANNO, COUNT_INFO, MEASUREMENT_SCALE, export_blocks, fresh_scale_resource, keep_raw_scale, keep_resource, notes_from_blocks, parse_anno, raw_scale,
    scale_from_resource, write_anno, write_scale_resource,
};

fn test_document() -> Document {
    Document::new("n", Size::new(10, 10), ColorMode::Rgb, SampleType::U8)
}

fn sample_note() -> Note {
    Note {
        text: "Hello\nWorld".into(),
        author: "tester".into(),
        position: [12.0, 34.0],
        popup: [50.0, 60.0, 300.0, 200.0],
        open: true,
        modified: "D:20240101120000".into(),
        ..Default::default()
    }
}

fn anno_block(notes: &[Note]) -> PsdGlobalBlock {
    (*b"8BIM", ANNO, Arc::new(write_anno(notes, &[])))
}

// Color is stored as 16-bit integers in the file, so exact float equality
// after roundtrip is not guaranteed. Use a tolerance similar to the library.
fn assert_note_eq(actual: &Note, expected: &Note) {
    assert_eq!(actual.author, expected.author);
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.position, expected.position);
    assert_eq!(actual.popup, expected.popup);
    assert_eq!(actual.open, expected.open);
    assert_eq!(actual.modified, expected.modified);
    let ca = actual.color.to_rgb();
    let ce = expected.color.to_rgb();
    for (a, e) in ca.iter().zip(ce.iter()) {
        assert!((a - e).abs() < 1.0 / 512.0, "color component mismatch: {a} vs {e}");
    }
}

#[test]
fn parse_anno_rejects_empty_truncated_and_bad_lengths() {
    assert!(parse_anno(&[]).is_none());
    assert!(parse_anno(&[0, 2, 0, 1]).is_none());
    assert!(parse_anno(&[0, 2, 0, 1, 0, 0, 0, 1]).is_none()); // count=1 but no entry
    assert!(parse_anno(&[0, 2, 0, 1, 0, 0, 0, 0]).is_some()); // empty list
}

#[test]
fn write_anno_empty_roundtrip() {
    let empty = write_anno(&[], &[]);
    assert_eq!(empty.len(), 8);
    let parsed = parse_anno(&empty).unwrap();
    assert!(parsed.is_empty());
}

#[test]
fn write_anno_roundtrip_preserves_text_and_fields() {
    let note = sample_note();
    let bytes = write_anno(std::slice::from_ref(&note), &[]);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed.len(), 1);
    let p = &parsed[0];
    assert_eq!(p.author, "tester");
    assert_eq!(p.text, "Hello\nWorld");
    assert_eq!(p.position, [12.0, 34.0]);
    assert_eq!(p.popup, [50.0, 60.0, 300.0, 200.0]);
    assert!(p.open);
    assert_eq!(p.modified, "D:20240101120000".to_string());
}

#[test]
fn write_anno_normalizes_crlf_and_cr_to_lf() {
    let mut note = sample_note();
    note.text = "line1\r\nline2\rline3".into();
    let bytes = write_anno(&[note], &[]);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed[0].text, "line1\nline2\nline3");
}

#[test]
fn write_anno_unicode_text_survives_utf16be_roundtrip() {
    let mut note = sample_note();
    note.text = "héllo ✓ 日本語".into();
    let bytes = write_anno(&[note], &[]);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed[0].text, "héllo ✓ 日本語");
}

#[test]
fn write_anno_pascal_handles_long_and_non_latin1_chars() {
    let mut note = sample_note();
    note.author = "a✓b".repeat(100); // length 300, will truncate to 255
    let bytes = write_anno(&[note], &[]);
    let parsed = parse_anno(&bytes).unwrap();
    let author = &parsed[0].author;
    assert_eq!(author.len(), 255);
    assert!(author.starts_with("a?b"));
    assert!(author.chars().all(|c| c == 'a' || c == '?' || c == 'b'));

    let mut short = sample_note();
    short.author = "a✓b".into();
    let short_bytes = write_anno(&[short], &[]);
    assert_eq!(parse_anno(&short_bytes).unwrap()[0].author, "a?b");
}

#[test]
fn parse_anno_rejects_truncated_entry() {
    let note = sample_note();
    let bytes = write_anno(&[note], &[]);
    assert!(bytes.len() > 10);
    let truncated = &bytes[..bytes.len() - 1];
    assert!(parse_anno(truncated).is_none());
}

#[test]
fn write_anno_multiple_notes_roundtrip_in_order() {
    let n1 = sample_note();
    let mut n2 = sample_note();
    n2.text = "second".into();
    n2.open = false;
    let bytes = write_anno(&[n1.clone(), n2.clone()], &[]);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_note_eq(&parsed[0], &n1);
    assert_note_eq(&parsed[1], &n2);
}

#[test]
fn write_anno_includes_raw_extra_without_affecting_parsed_notes() {
    let note = sample_note();
    // A minimal raw `sndA` entry: u32 length (8 including length) + kind.
    let raw = vec![0, 0, 0, 8, b's', b'n', b'd', b'A'];
    let bytes = write_anno(std::slice::from_ref(&note), std::slice::from_ref(&raw));
    assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 2);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_note_eq(&parsed[0], &note);
    assert!(bytes.ends_with(&raw));
}

#[test]
fn export_blocks_no_notes_and_no_blocks_is_noop() {
    let doc = test_document();
    assert!(export_blocks(&doc, vec![]).is_empty());
}

#[test]
fn export_blocks_appends_block_for_new_note() {
    let mut doc = test_document();
    let note = sample_note();
    doc.notes.push(note.clone());
    let blocks = export_blocks(&doc, vec![]);
    assert_eq!(blocks.len(), 1);
    let parsed = parse_anno(&blocks[0].2).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_note_eq(&parsed[0], &note);
}

#[test]
fn export_blocks_replaces_block_when_note_changes() {
    let mut doc = test_document();
    let original = sample_note();
    doc.notes.push(original.clone());
    let blocks = export_blocks(&doc, vec![]);
    assert_eq!(blocks.len(), 1);

    doc.notes[0].text = "changed".into();
    doc.notes[0].author = "another".into();
    let new_blocks = export_blocks(&doc, blocks);
    assert_eq!(new_blocks.len(), 1);
    let parsed = parse_anno(&new_blocks[0].2).unwrap();
    assert_eq!(parsed[0].text, "changed");
    assert_eq!(parsed[0].author, "another");
}

#[test]
fn export_blocks_drops_block_when_notes_deleted() {
    let mut doc = test_document();
    doc.notes.push(sample_note());
    let blocks = export_blocks(&doc, vec![]);
    assert_eq!(blocks.len(), 1);

    doc.notes.clear();
    let new_blocks = export_blocks(&doc, blocks);
    assert!(new_blocks.is_empty());
}

#[test]
fn export_blocks_keeps_unchanged_block_byte_exact() {
    let mut doc = test_document();
    let note = sample_note();
    doc.notes.push(note.clone());
    let blocks = export_blocks(&doc, vec![]);
    assert_eq!(blocks.len(), 1);

    doc.metadata.psd_global_blocks = blocks.clone();
    let out = export_blocks(&doc, blocks.clone());
    assert_eq!(out.len(), 1);
    assert!(Arc::ptr_eq(&out[0].2, &blocks[0].2));
}

#[test]
fn notes_from_blocks_extracts_notes_from_global_blocks() {
    let mut doc = test_document();
    let note = sample_note();
    doc.metadata.psd_global_blocks = vec![anno_block(std::slice::from_ref(&note))];
    let notes = notes_from_blocks(&doc);
    assert_eq!(notes.len(), 1);
    assert_note_eq(&notes[0], &note);
}

#[test]
fn notes_from_blocks_is_empty_without_anno_block() {
    let doc = test_document();
    assert!(notes_from_blocks(&doc).is_empty());

    let mut doc2 = test_document();
    doc2.metadata.psd_global_blocks = vec![(*b"8BIM", [1u8, 2, 3, 4], Arc::new(vec![]))];
    assert!(notes_from_blocks(&doc2).is_empty());
}

#[test]
fn scale_resource_roundtrip_with_units() {
    let scale = MeasurementScale { pixel_length: 150.0, logical_length: 2.5, units: "mm".into() };
    let bytes = write_scale_resource(&scale);
    assert!(bytes.len().is_multiple_of(2));
    let parsed = scale_from_resource(&bytes).unwrap();
    assert_eq!(parsed, scale);
}

#[test]
fn scale_from_resource_rejects_malformed_data() {
    assert!(scale_from_resource(&[]).is_none());
    assert!(scale_from_resource(&[0, 0, 0, 16, 1]).is_none());
    assert!(scale_from_resource(&[0, 0, 0, 16, 0, 0]).is_none());
}

#[test]
fn raw_scale_reads_preserved_resource() {
    let mut doc = test_document();
    let scale = MeasurementScale { pixel_length: 10.0, logical_length: 3.0, units: "cm".into() };
    let resource = write_scale_resource(&scale);
    doc.metadata.psd_resources.push((MEASUREMENT_SCALE, String::new(), Arc::new(resource)));
    assert_eq!(raw_scale(&doc), Some(scale));
}

#[test]
fn keep_and_fresh_scale_handle_default_and_custom() {
    let mut doc = test_document();
    assert!(keep_raw_scale(&doc));
    assert!(fresh_scale_resource(&doc).is_none());

    // Unparseable raw resource + default scale: keep raw, no fresh.
    doc.metadata.psd_resources.push((MEASUREMENT_SCALE, String::new(), Arc::new(vec![0, 0, 0, 16, 1])));
    assert!(keep_raw_scale(&doc));
    assert!(fresh_scale_resource(&doc).is_none());

    // Custom scale with unparseable raw: cannot keep, fresh needed.
    let custom = MeasurementScale { pixel_length: 300.0, logical_length: 2.0, units: "px".into() };
    doc.measurement.scale = custom.clone();
    assert!(!keep_raw_scale(&doc));
    let fresh = fresh_scale_resource(&doc).unwrap();
    assert_eq!(scale_from_resource(&fresh), Some(custom));
}

#[test]
fn keep_resource_rules_for_measurement_and_count_info() {
    let mut doc = test_document();
    assert!(keep_resource(&doc, MEASUREMENT_SCALE));
    assert!(keep_resource(&doc, COUNT_INFO));
    assert!(keep_resource(&doc, 9999)); // unknown ids are kept

    doc.measurement.scale = MeasurementScale { pixel_length: 1.0, logical_length: 1.0, units: "m".into() };
    assert!(!keep_resource(&doc, MEASUREMENT_SCALE));

    doc.measurement.count_groups.push(CountGroup { points: vec![[1.0, 1.0]], ..Default::default() });
    assert!(!keep_resource(&doc, COUNT_INFO));
}

#[test]
fn write_anno_generates_default_popup_for_empty_popup_field() {
    let mut note = sample_note();
    note.popup = [0.0, 0.0, 0.0, 0.0];
    note.position = [12.0, 34.0];
    let bytes = write_anno(&[note], &[]);
    let parsed = parse_anno(&bytes).unwrap();
    assert_eq!(parsed[0].popup, [32.0, 54.0, 272.0, 194.0]);
}
