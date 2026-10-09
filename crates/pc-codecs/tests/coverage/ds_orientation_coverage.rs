use std::borrow::Cow;

use photocraft_codecs::{exif_orientation, upright_exif, upright_xmp};

// ---------- helpers for constructing synthetic TIFF/EXIF blocks ----------

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

/// Build a classic TIFF (little or big endian) with the given IFD0 entries.
/// Each entry is `(tag, field_type, count, value_field)` where `value_field`
/// is the 4-byte inline value/offset field, already encoded in the TIFF's
/// byte order.
fn classic_tiff(endian: Endian, entries: &[(u16, u16, u32, [u8; 4])]) -> Vec<u8> {
    let mut b = Vec::new();
    match endian {
        Endian::Little => b.extend_from_slice(b"II"),
        Endian::Big => b.extend_from_slice(b"MM"),
    }
    match endian {
        Endian::Little => b.extend_from_slice(&42u16.to_le_bytes()),
        Endian::Big => b.extend_from_slice(&42u16.to_be_bytes()),
    }
    let ifd_offset: u32 = 8;
    match endian {
        Endian::Little => b.extend_from_slice(&ifd_offset.to_le_bytes()),
        Endian::Big => b.extend_from_slice(&ifd_offset.to_be_bytes()),
    }
    let count = entries.len() as u16;
    match endian {
        Endian::Little => b.extend_from_slice(&count.to_le_bytes()),
        Endian::Big => b.extend_from_slice(&count.to_be_bytes()),
    }
    for &(tag, ty, cnt, val_field) in entries {
        match endian {
            Endian::Little => {
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&ty.to_le_bytes());
                b.extend_from_slice(&cnt.to_le_bytes());
            }
            Endian::Big => {
                b.extend_from_slice(&tag.to_be_bytes());
                b.extend_from_slice(&ty.to_be_bytes());
                b.extend_from_slice(&cnt.to_be_bytes());
            }
        }
        b.extend_from_slice(&val_field);
    }
    let next_ifd: u32 = 0;
    match endian {
        Endian::Little => b.extend_from_slice(&next_ifd.to_le_bytes()),
        Endian::Big => b.extend_from_slice(&next_ifd.to_be_bytes()),
    }
    b
}

/// Build a little-endian BigTIFF with the given IFD0 entries.
/// Entries are `(tag, field_type, count, value_field)` where `value_field`
/// is the 8-byte inline value/offset field, little-endian encoded.
fn big_tiff_little(entries: &[(u16, u16, u64, [u8; 8])]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"II");
    b.extend_from_slice(&43u16.to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes()); // offset size
    b.extend_from_slice(&0u16.to_le_bytes()); // reserved
    b.extend_from_slice(&16u64.to_le_bytes()); // IFD offset
    b.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for &(tag, ty, cnt, val_field) in entries {
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&ty.to_le_bytes());
        b.extend_from_slice(&cnt.to_le_bytes());
        b.extend_from_slice(&val_field);
    }
    b.extend_from_slice(&0u64.to_le_bytes());
    b
}

// Orientation tag and types
const TAG_ORIENTATION: u16 = 274;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;
const TYPE_ASCII: u16 = 2;

// ---------- exif_orientation tests ----------

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
fn exif_orientation_classic_little_endian_short() {
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 6);
}

#[test]
fn exif_orientation_classic_big_endian_short() {
    let data = classic_tiff(Endian::Big, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [0, 6, 0, 0])]);
    assert_eq!(exif_orientation(&data), 6);
}

#[test]
fn exif_orientation_accepts_long_type() {
    let data_le = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_LONG, 1, [7, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data_le), 7);

    let data_be = classic_tiff(Endian::Big, &[(TAG_ORIENTATION, TYPE_LONG, 1, [0, 0, 0, 7])]);
    assert_eq!(exif_orientation(&data_be), 7);
}

#[test]
fn exif_orientation_values_outside_one_to_eight_are_one() {
    // SHORT 0
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [0, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // SHORT 9
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [9, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // LONG 65536 (out of u16)
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_LONG, 1, [0, 0, 1, 0])]);
    assert_eq!(exif_orientation(&data), 1);
}

#[test]
fn exif_orientation_malformed_entries_are_one() {
    // No entries
    let data = classic_tiff(Endian::Little, &[]);
    assert_eq!(exif_orientation(&data), 1);

    // Count != 1
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 2, [6, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // Bad field type (ASCII)
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_ASCII, 1, [0, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);
}

#[test]
fn exif_orientation_first_malformed_blocks_later_well_formed() {
    // First orientation entry has count=2 -> malformed -> tag ignored,
    // even though second entry is well formed.
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 2, [0, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 1);

    // First well-formed wins over later well-formed
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [7, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data), 6);
}

#[test]
fn exif_orientation_skips_exif_prefix() {
    let body = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0])]);
    let mut with_prefix = Vec::new();
    with_prefix.extend_from_slice(b"Exif\0\0");
    with_prefix.extend_from_slice(&body);
    assert_eq!(exif_orientation(&with_prefix), 6);
}

#[test]
fn exif_orientation_parses_big_tiff() {
    let data_short = big_tiff_little(&[(TAG_ORIENTATION, TYPE_SHORT, 1, [8, 0, 0, 0, 0, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data_short), 8);

    let data_long = big_tiff_little(&[(TAG_ORIENTATION, TYPE_LONG, 1, [7, 0, 0, 0, 0, 0, 0, 0])]);
    assert_eq!(exif_orientation(&data_long), 7);
}

// ---------- upright_exif tests ----------

#[test]
fn upright_exif_no_change_when_empty_already_one_or_malformed() {
    assert!(matches!(upright_exif(&[]), Cow::Borrowed(_)));

    // Already 1
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [1, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Borrowed(_)));
    assert_eq!(result.as_ref(), data.as_slice());

    // No orientation tag
    let data = classic_tiff(Endian::Little, &[(256, TYPE_SHORT, 1, [0, 0, 0, 0])]);
    assert!(matches!(upright_exif(&data), Cow::Borrowed(_)));

    // Malformed orientation tag (count != 1)
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 2, [0, 0, 0, 0])]);
    assert!(matches!(upright_exif(&data), Cow::Borrowed(_)));

    // Unparseable input
    let data = b"not a tiff";
    assert!(matches!(upright_exif(data), Cow::Borrowed(_)));
}

#[test]
fn upright_exif_sets_little_endian_short_to_one() {
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Owned(_)));
    let out = result.as_ref();
    // Value field at entry offset 10 + 8 = 18
    assert_eq!(&out[18..22], &[1, 0, 0, 0]);
    // Everything before and after the value field is untouched
    assert_eq!(&out[..18], &data[..18]);
    assert_eq!(&out[22..], &data[22..]);
}

#[test]
fn upright_exif_sets_big_endian_long_to_one() {
    let data = classic_tiff(Endian::Big, &[(TAG_ORIENTATION, TYPE_LONG, 1, [0, 0, 0, 6])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Owned(_)));
    let out = result.as_ref();
    assert_eq!(&out[18..22], &[0, 0, 0, 1]);
    assert_eq!(&out[..18], &data[..18]);
    assert_eq!(&out[22..], &data[22..]);
}

#[test]
fn upright_exif_sets_all_duplicates_to_one() {
    let data = classic_tiff(Endian::Little, &[(TAG_ORIENTATION, TYPE_SHORT, 1, [6, 0, 0, 0]), (TAG_ORIENTATION, TYPE_SHORT, 1, [7, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Owned(_)));
    let out = result.as_ref();
    // First entry value at 18, second entry value at 30
    assert_eq!(&out[18..22], &[1, 0, 0, 0]);
    assert_eq!(&out[30..34], &[1, 0, 0, 0]);
}

#[test]
fn upright_exif_big_tiff_rewrites_short_value() {
    let data = big_tiff_little(&[(TAG_ORIENTATION, TYPE_SHORT, 1, [8, 0, 0, 0, 0, 0, 0, 0])]);
    let result = upright_exif(&data);
    assert!(matches!(result, Cow::Owned(_)));
    let out = result.as_ref();
    // BigTIFF entry starts at 24; value offset = 24 + 12 = 36
    assert_eq!(&out[36..44], &[1, 0, 0, 0, 0, 0, 0, 0]);
    // Other bytes preserved
    assert_eq!(&out[..36], &data[..36]);
    assert_eq!(&out[44..], &data[44..]);
}

// ---------- upright_xmp tests ----------

#[test]
fn upright_xmp_no_change_cases() {
    assert!(matches!(upright_xmp(""), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("no key"), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("<tiff:Orientation>1</tiff:Orientation>"), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("tiff:Orientation=\"Rotate90\""), Cow::Borrowed(_)));
    assert!(matches!(upright_xmp("tiff:orientation=\"6\""), Cow::Borrowed(_)));
}

#[test]
fn upright_xmp_attribute_double_quotes() {
    let input = r#"<rdf:Description tiff:Orientation="6">"#;
    let result = upright_xmp(input);
    assert!(matches!(result, Cow::Owned(_)));
    assert_eq!(result.as_ref(), r#"<rdf:Description tiff:Orientation="1">"#);
}

#[test]
fn upright_xmp_attribute_single_quotes() {
    let input = "tiff:Orientation='8'";
    let result = upright_xmp(input);
    assert!(matches!(result, Cow::Owned(_)));
    assert_eq!(result.as_ref(), "tiff:Orientation='1'");
}

#[test]
fn upright_xmp_element_form_and_whitespace() {
    let input = "<tiff:Orientation>6</tiff:Orientation>";
    let result = upright_xmp(input);
    assert!(matches!(result, Cow::Owned(_)));
    assert_eq!(result.as_ref(), "<tiff:Orientation>1</tiff:Orientation>");

    let spaced = "tiff:Orientation = \"6\"";
    let result = upright_xmp(spaced);
    assert!(matches!(result, Cow::Owned(_)));
    assert_eq!(result.as_ref(), "tiff:Orientation = \"1\"");
}

#[test]
fn upright_xmp_multiple_values_and_leading_zeros() {
    let input = r#"<x tiff:Orientation="06"/> <y tiff:Orientation='0'/> <z><tiff:Orientation>8</tiff:Orientation>"#;
    let result = upright_xmp(input);
    assert!(matches!(result, Cow::Owned(_)));
    assert_eq!(result.as_ref(), r#"<x tiff:Orientation="1"/> <y tiff:Orientation='1'/> <z><tiff:Orientation>1</tiff:Orientation>"#);
}
