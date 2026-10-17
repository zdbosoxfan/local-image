use lightcraft_tiff::{
    ByteOrder, Tiff, TiffError, Value,
    writer::{self, IfdBuilder, TiffWriter},
};
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
