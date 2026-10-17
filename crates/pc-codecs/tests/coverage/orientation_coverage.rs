use std::borrow::Cow;

use lightcraft_meta::upright_xmp;
use lightcraft_tiff::exif::{orientation as exif_orientation, upright as upright_exif};

// Helpers for constructing synthetic TIFF/EXIF blocks

/// Build a little-endian classic TIFF with the given IFD0 entries.
/// Each entry is `(tag, field_type, count, value_field)` where `value_field`
/// is the 4-byte inline value/offset field, little-endian encoded.
fn classic_tiff(entries: &[(u16, u16, u32, [u8; 4])]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"II");
    b.extend_from_slice(&42u16.to_le_bytes());
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for &(tag, ty, cnt, val_field) in entries {
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&ty.to_le_bytes());
        b.extend_from_slice(&cnt.to_le_bytes());
        b.extend_from_slice(&val_field);
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    b
}

// Orientation tag and types
const TAG_ORIENTATION: u16 = 274;
const TYPE_SHORT: u16 = 3;
const TYPE_ASCII: u16 = 2;

// Exif_orientation tests

#[test]
fn exif_orientation_empty_and_tiny_inputs_are_one() {
    assert_eq!(exif_orientation(&[]), 1);
    assert_eq!(exif_orientation(b"I"), 1);
    assert_eq!(exif_orientation(b"II"), 1);
    assert_eq!(exif_orientation(b"II*\0"), 1); // header only, no IFD offset
    assert_eq!(exif_orientation(&[0; 8]), 1); // not a TIFF header
    // IFD offset < 8
    let mut data = Vec::new();
    data.extend_from_slice(b"II");
    data.extend_from_slice(&42u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // offset 0
    assert_eq!(exif_orientation(&data), 1);
}

#[test]
fn exif_orientation_malformed_entries_are_one() {
    // No entries
    let data = classic_tiff(&[]);
    assert_eq!(exif_orientation(&data), 1);

    // Count != 1
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 2, [6, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // Bad field type (ASCII)
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_ASCII, 1, [0, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);
}

#[test]
fn exif_orientation_first_malformed_blocks_later_well_formed() {
    // First orientation entry has count=2 -> malformed -> tag ignored,
    // even though second entry is well formed.
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 2, [0, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // First well-formed wins over later well-formed
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [7, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 6);
}

// Upright_exif tests

#[test]
fn upright_exif_no_change_when_empty_already_one_or_malformed() {
    assert!(matches!(upright_exif(&[]), Cow::Borrowed(_)));

    // Already 1
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 1, [1, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Borrowed(_)));
    assert_eq!(result.as_ref(), data.as_slice());

    // No orientation tag
    let data = classic_tiff(&[(256, TYPE_SHORT, 1, [0, 0, 0, 0])]);
    assert!(matches!(upright_exif(&data), Cow::Borrowed(_)));

    // Malformed orientation tag (count != 1)
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 2, [0, 0, 0, 0])]);
    assert!(matches!(upright_exif(&data), Cow::Borrowed(_)));

    // Unparseable input
    let data = b"not a tiff";
    assert!(matches!(upright_exif(data), Cow::Borrowed(_)));
}

#[test]
fn upright_exif_sets_all_duplicates_to_one() {
    let data = classic_tiff(&[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [7, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Owned(_)));
    let out = result.as_ref();
    // First entry value at 18, second entry value at 30
    assert_eq!(&out[18..22], &[1, 0, 0, 0]);
    assert_eq!(&out[30..34], &[1, 0, 0, 0]);
}

// Upright_xmp tests

#[test]
fn upright_xmp_no_change_cases() {
    assert!(matches!(upright_xmp(""), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("no key"), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("<tiff:Orientation>1</tiff:Orientation>"), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("tiff:Orientation=\"Rotate90\""), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("tiff:orientation=\"6\""), Cow::Borrowed(_)));
}
