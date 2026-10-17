use lightcraft_meta::{Metadata, TagRow, file_tag_rows, strip_exif_header, write_exif};

fn tiff_bytes_from_metadata(m: &Metadata) -> Vec<u8> {
    let exif_block = write_exif(m);
    strip_exif_header(&exif_block).to_vec()
}

fn jpeg_bytes_with_exif(m: &Metadata) -> Vec<u8> {
    let tiff = tiff_bytes_from_metadata(m);
    let mut jpeg = Vec::new();
    jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
    let payload_len = 6 + tiff.len(); // "Exif\0\0" + tiff
    jpeg.extend_from_slice(&[0xFF, 0xE1]); // APP1 marker
    jpeg.push((payload_len >> 8) as u8);
    jpeg.push((payload_len & 0xFF) as u8);
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(&tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
    jpeg
}

fn find_row_by_name<'a>(rows: &'a [TagRow], group: &str, name: &str) -> Option<&'a TagRow> {
    rows.iter().find(|r| r.group == group && r.name == name)
}

#[test]
fn empty_input_returns_no_rows() {
    assert!(file_tag_rows(&[]).is_empty());
}

#[test]
fn tiny_inputs_do_not_panic_and_return_empty() {
    for len in 1..=8 {
        let bytes = vec![0xA5; len];
        assert!(file_tag_rows(&bytes).is_empty(), "len={} should produce no rows", len);
    }
}

#[test]
fn random_bytes_return_empty() {
    let bytes = b"not an image at all, just some random text";
    assert!(file_tag_rows(bytes).is_empty());
}

#[test]
fn jpeg_with_exif_yields_rows() {
    let m = Metadata {
        make: Some("Maker".into()),
        model: Some("Model Y".into()),
        exposure_time: Some(1.0 / 125.0),
        f_number: Some(4.0),
        iso: Some(200),
        focal_length: Some(50.0),
        ..Default::default()
    };
    let jpeg = jpeg_bytes_with_exif(&m);
    let rows = file_tag_rows(&jpeg);

    assert!(!rows.is_empty());

    let make = find_row_by_name(&rows, "TIFF", "Make").expect("Make row");
    assert_eq!(make.value, "Maker");
    let exposure = find_row_by_name(&rows, "EXIF", "Exposure Time").expect("Exposure Time row");
    assert_eq!(exposure.value, "1/125 s");
    let fnum = find_row_by_name(&rows, "EXIF", "F-Number").expect("F-Number row");
    assert_eq!(fnum.value, "f/4");
    let focal = find_row_by_name(&rows, "EXIF", "Focal Length").expect("Focal Length row");
    assert_eq!(focal.value, "50 mm");
}

#[test]
fn rational_values_are_formatted() {
    let cases = [(0.5, "1/2 s"), (2.0, "2 s"), (5.6, "f/5.6"), (85.0, "85 mm"), (35.5, "35.5 mm")];

    for (value, expected) in cases {
        let mut m = Metadata::default();
        if expected.ends_with("s") {
            m.exposure_time = Some(value);
        } else if expected.starts_with("f/") {
            m.f_number = Some(value);
        } else if expected.ends_with("mm") {
            m.focal_length = Some(value);
        }
        let tiff = tiff_bytes_from_metadata(&m);
        let rows = file_tag_rows(&tiff);
        let row = if expected.ends_with("s") {
            find_row_by_name(&rows, "EXIF", "Exposure Time")
        } else if expected.starts_with("f/") {
            find_row_by_name(&rows, "EXIF", "F-Number")
        } else {
            find_row_by_name(&rows, "EXIF", "Focal Length")
        }
        .expect("row");
        assert_eq!(row.value, expected, "for input value {}", value);
    }
}

#[test]
fn non_finite_values_do_not_panic_and_do_not_contain_nan_inf() {
    let m = Metadata { exposure_time: Some(f64::NAN), f_number: Some(f64::INFINITY), focal_length: Some(f64::NEG_INFINITY), ..Default::default() };
    let tiff = tiff_bytes_from_metadata(&m);
    let rows = file_tag_rows(&tiff);

    for row in &rows {
        let lower = row.value.to_lowercase();
        assert!(!lower.contains("nan") && !lower.contains("inf"), "row {} (tag 0x{:04X}) has value: {}", row.name, row.tag, row.value);
    }
}

#[test]
fn malformed_tiff_returns_empty() {
    // Valid TIFF header but truncated/garbage after.
    let bytes = vec![
        0x49, 0x49, 0x2A, 0x00, // "II*\0"
        0x08, 0x00, 0x00, 0x00, // arbitrary offset
        0xFF, 0xFF, 0xFF, 0xFF, // garbage entries
    ];
    assert!(file_tag_rows(&bytes).is_empty());
}

#[test]
fn malformed_jpeg_with_exif_returns_empty() {
    // Start of JPEG with APP1 marker containing "Exif\0\0" but invalid TIFF data.
    let invalid_tiff = vec![0x01, 0x02, 0x03, 0x04]; // too short to be valid TIFF
    let mut jpeg = Vec::new();
    jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
    let payload_len = 6 + invalid_tiff.len();
    jpeg.extend_from_slice(&[0xFF, 0xE1]);
    jpeg.push((payload_len >> 8) as u8);
    jpeg.push((payload_len & 0xFF) as u8);
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(&invalid_tiff);
    jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI

    assert!(file_tag_rows(&jpeg).is_empty());
}
