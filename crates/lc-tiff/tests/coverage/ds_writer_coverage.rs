use lightcraft_tiff::{
    ByteOrder, Tiff, TiffError, Value,
    writer::{self, IfdBuilder, ImageData, TiffWriter},
};
use std::path::PathBuf;

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn make_temp_dir(name: &str) -> TempDir {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("lightcraft_tiff_writer_{}_{}_{}", name, std::process::id(), nanos));
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

fn parse_written(writer: &TiffWriter, chain: &[IfdBuilder]) -> Tiff {
    let bytes = writer.write(chain).unwrap();
    Tiff::parse(&bytes).unwrap()
}

#[test]
fn write_empty_chain_returns_invalid_error() {
    let err = TiffWriter::new(ByteOrder::Little, false).write(&[]).unwrap_err();
    assert!(matches!(err, TiffError::Invalid(_)));
}

#[test]
fn round_trip_single_ifd_little_endian_inline() {
    let mut b = IfdBuilder::new();
    b.set(256, Value::Byte(vec![1, 2, 3]));
    b.set(258, Value::Short(vec![1, 0x0102]));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[b]);

    assert_eq!(t.order, ByteOrder::Little);
    assert!(!t.bigtiff);
    assert_eq!(t.ifds.len(), 1);
    assert_eq!(t.ifds[0].value(256), Some(&Value::Byte(vec![1, 2, 3])));
    assert_eq!(t.ifds[0].value(258), Some(&Value::Short(vec![1, 0x0102])));
}

#[test]
fn round_trip_big_endian_bigtiff_with_long8() {
    let mut b = IfdBuilder::new();
    b.set(256, Value::Long8(vec![u64::MAX - 1, 42]));
    b.set(258, Value::Short(vec![0x1234]));

    let t = parse_written(&TiffWriter::new(ByteOrder::Big, true), &[b]);

    assert_eq!(t.order, ByteOrder::Big);
    assert!(t.bigtiff);
    assert_eq!(t.ifds[0].value(256), Some(&Value::Long8(vec![u64::MAX - 1, 42])));
    assert_eq!(t.ifds[0].value(258), Some(&Value::Short(vec![0x1234])));
}

#[test]
fn multi_ifd_chain_round_trip() {
    let ifd0 = IfdBuilder::new().with(256, Value::Byte(vec![1]));
    let ifd1 = IfdBuilder::new().with(256, Value::Byte(vec![2]));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[ifd0, ifd1]);

    assert_eq!(t.ifds.len(), 2);
    assert_eq!(t.ifds[0].value(256), Some(&Value::Byte(vec![1])));
    assert_eq!(t.ifds[1].value(256), Some(&Value::Byte(vec![2])));
}

#[test]
fn out_of_line_undefined_value_round_trip() {
    let data = vec![0xABu8; 300];
    let mut b = IfdBuilder::new();
    b.set(250, Value::Undefined(data.clone()));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[b]);

    assert_eq!(t.ifds[0].value(250), Some(&Value::Undefined(data)));
}

#[test]
fn ascii_value_round_trip() {
    let mut b = IfdBuilder::new();
    b.set(270, Value::Ascii("hello".to_string()));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[b]);

    assert_eq!(t.ifds[0].string(270).as_deref(), Some("hello"));
}

#[test]
fn sub_ifds_are_written_and_resolved() {
    let mut parent = IfdBuilder::new();
    parent.add_sub_ifd(IfdBuilder::new().with(256, Value::Byte(vec![1])));
    parent.add_sub_ifd(IfdBuilder::new().with(256, Value::Byte(vec![2])));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[parent]);

    assert_eq!(t.ifds.len(), 1);
    assert_eq!(t.ifds[0].sub_ifds.len(), 2);
    assert_eq!(t.all_ifds().len(), 3);
    assert_eq!(t.ifds[0].sub_ifds[0].value(256), Some(&Value::Byte(vec![1])));
    assert_eq!(t.ifds[0].sub_ifds[1].value(256), Some(&Value::Byte(vec![2])));
}

#[test]
fn pointer_children_exif_gps_interop_are_resolved() {
    let mut parent = IfdBuilder::new();
    parent.set_child(34665, IfdBuilder::new().with(37500, Value::Undefined(vec![1, 2, 3])));
    parent.set_child(34853, IfdBuilder::new().with(1, Value::Ascii("N".to_string())));
    parent.set_child(40965, IfdBuilder::new().with(2, Value::Ascii("I".to_string())));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[parent]);

    assert!(t.ifds[0].exif.is_some());
    assert_eq!(t.ifds[0].exif.as_ref().unwrap().value(37500), Some(&Value::Undefined(vec![1, 2, 3])));
    assert!(t.ifds[0].gps.is_some());
    assert!(t.ifds[0].interop.is_some());
}

#[test]
fn strips_image_round_trip() {
    let mut b = IfdBuilder::new();
    b.set_image(ImageData::Strips { rows_per_strip: 2, strips: vec![vec![1, 2, 3], vec![4, 5]] });

    let writer = TiffWriter::new(ByteOrder::Little, false);
    let bytes = writer.write(&[b]).unwrap();
    let t = Tiff::parse(&bytes).unwrap();

    let ifd = &t.ifds[0];
    assert_eq!(ifd.u64(278), Some(2)); // RowsPerStrip
    assert_eq!(ifd.value(279).unwrap().to_u64_vec(), vec![3, 2]); // StripByteCounts
    let offsets = ifd.value(273).unwrap().to_u64_vec(); // StripOffsets
    assert_eq!(offsets.len(), 2);
    assert_eq!(&bytes[offsets[0] as usize..offsets[0] as usize + 3], &[1, 2, 3]);
    assert_eq!(&bytes[offsets[1] as usize..offsets[1] as usize + 2], &[4, 5]);
}

#[test]
fn tiles_image_round_trip() {
    let mut b = IfdBuilder::new();
    b.set_image(ImageData::Tiles { tile_width: 16, tile_height: 8, tiles: vec![vec![0xDE, 0xAD], vec![0xBE, 0xEF, 0x01]] });

    let writer = TiffWriter::new(ByteOrder::Little, false);
    let bytes = writer.write(&[b]).unwrap();
    let t = Tiff::parse(&bytes).unwrap();

    let ifd = &t.ifds[0];
    assert_eq!(ifd.u64(322), Some(16)); // TileWidth
    assert_eq!(ifd.u64(323), Some(8)); // TileLength
    assert_eq!(ifd.value(325).unwrap().to_u64_vec(), vec![2, 3]); // TileByteCounts
    let offsets = ifd.value(324).unwrap().to_u64_vec(); // TileOffsets
    assert_eq!(offsets.len(), 2);
    assert_eq!(&bytes[offsets[0] as usize..offsets[0] as usize + 2], &[0xDE, 0xAD]);
    assert_eq!(&bytes[offsets[1] as usize..offsets[1] as usize + 3], &[0xBE, 0xEF, 0x01]);
}

#[test]
fn encode_numeric_values_respect_byte_order() {
    assert_eq!(writer::encode(ByteOrder::Little, &Value::Short(vec![0x1234])), vec![0x34, 0x12]);
    assert_eq!(writer::encode(ByteOrder::Big, &Value::Short(vec![0x1234])), vec![0x12, 0x34]);
    assert_eq!(writer::encode(ByteOrder::Little, &Value::Long(vec![0x12345678])), vec![0x78, 0x56, 0x34, 0x12]);
    assert_eq!(writer::encode(ByteOrder::Big, &Value::Long(vec![0x12345678])), vec![0x12, 0x34, 0x56, 0x78]);
    assert_eq!(writer::encode(ByteOrder::Little, &Value::Double(vec![1.0])), vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F]);
}

#[test]
fn float_and_double_special_values_round_trip_bits() {
    let floats = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0_f32, -0.0_f32, 1.5_f32];
    let doubles = vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0_f64, -0.0_f64, 1.5_f64];

    let mut b = IfdBuilder::new();
    b.set(500, Value::Float(floats.clone()));
    b.set(501, Value::Double(doubles.clone()));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[b]);

    let parsed_floats = match t.ifds[0].value(500).unwrap() {
        Value::Float(v) => v,
        other => panic!("expected Float, got {other:?}"),
    };
    let parsed_doubles = match t.ifds[0].value(501).unwrap() {
        Value::Double(v) => v,
        other => panic!("expected Double, got {other:?}"),
    };

    assert_eq!(parsed_floats.len(), floats.len());
    for (parsed, original) in parsed_floats.iter().zip(floats.iter()) {
        assert_eq!(parsed.to_bits(), original.to_bits());
    }
    assert_eq!(parsed_doubles.len(), doubles.len());
    for (parsed, original) in parsed_doubles.iter().zip(doubles.iter()) {
        assert_eq!(parsed.to_bits(), original.to_bits());
    }
}

#[test]
fn rational_helpers_are_sane_for_edge_and_normal_values() {
    assert_eq!(writer::rational(0.0), (0, 1));
    assert_eq!(writer::rational(f64::NAN), (0, 1));
    assert_eq!(writer::rational(f64::NEG_INFINITY), (0, 1));
    assert_eq!(writer::rational(-2.0), (0, 1));
    assert_eq!(writer::rational(1.5), (15, 10));
    assert_eq!(writer::rational(0.1), (1, 10));

    assert_eq!(writer::srational(f64::NAN), (0, 1));
    assert_eq!(writer::srational(-1.5), (-15, 10));
    assert_eq!(writer::srational(2.5), (25, 10));
}

#[test]
fn excessive_sub_ifd_depth_returns_limit_error_not_panic() {
    fn nested(depth: usize) -> IfdBuilder {
        let mut b = IfdBuilder::new();
        if depth > 1 {
            b.add_sub_ifd(nested(depth - 1));
        }
        b
    }

    let chain = [nested(20)];
    let err = TiffWriter::new(ByteOrder::Little, false).write(&chain).unwrap_err();

    assert!(matches!(err, TiffError::Limit(_)));
}

#[test]
fn default_writer_is_little_endian_classic() {
    let mut b = IfdBuilder::new();
    b.set(256, Value::Byte(vec![7]));

    let t = parse_written(&TiffWriter::default(), &[b]);

    assert_eq!(t.order, ByteOrder::Little);
    assert!(!t.bigtiff);
    assert_eq!(t.ifds[0].value(256), Some(&Value::Byte(vec![7])));
}

#[test]
fn writes_are_deterministic_for_same_input() {
    let b = IfdBuilder::new().with(256, Value::Byte(vec![1, 2, 3])).with(258, Value::Short(vec![4, 5]));

    let writer = TiffWriter::new(ByteOrder::Big, false);

    assert_eq!(writer.write(std::slice::from_ref(&b)).unwrap(), writer.write(std::slice::from_ref(&b)).unwrap());
}

#[test]
fn empty_value_arrays_round_trip() {
    let mut b = IfdBuilder::new();
    b.set(256, Value::Byte(vec![]));
    b.set(270, Value::Ascii(String::new()));
    b.set(258, Value::Short(vec![]));
    b.set(259, Value::Long(vec![]));

    let t = parse_written(&TiffWriter::new(ByteOrder::Little, false), &[b]);

    assert_eq!(t.ifds[0].value(256).cloned(), Some(Value::Byte(vec![])));
    assert_eq!(t.ifds[0].value(270).cloned(), Some(Value::Ascii(String::new())));
    assert_eq!(t.ifds[0].value(258).cloned(), Some(Value::Short(vec![])));
    assert_eq!(t.ifds[0].value(259).cloned(), Some(Value::Long(vec![])));
}

#[test]
fn write_bytes_can_be_saved_and_parsed_from_file() {
    let dir = make_temp_dir("file_roundtrip");
    let path = dir.0.join("out.tif");

    let mut b = IfdBuilder::new();
    b.set(256, Value::Byte(vec![9, 8, 7]));

    let bytes = TiffWriter::new(ByteOrder::Little, false).write(&[b]).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    let read = std::fs::read(&path).unwrap();
    let t = Tiff::parse(&read).unwrap();

    assert_eq!(t.ifds[0].value(256), Some(&Value::Byte(vec![9, 8, 7])));
}
