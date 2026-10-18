use lightcraft_tiff::{ByteOrder, IfdBuilder, Tiff, TiffWriter, Value};
use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image, decode, encode};

#[test]
fn tiff_carries_exif_gps_and_unknown_tags_in_both_byte_orders() {
    for order in [ByteOrder::Little, ByteOrder::Big] {
        let mut root = IfdBuilder::new();
        root.set(271, Value::Ascii("Camera maker".into()));
        root.set_child(34665, IfdBuilder::new().with(33434, Value::Rational(vec![(1, 125)])).with(65000, Value::Short(vec![17, 42])));
        root.set_child(34853, IfdBuilder::new().with(1, Value::Ascii("N".into())).with(2, Value::Rational(vec![(42, 1), (30, 1), (0, 1)])));
        let exif = TiffWriter::new(order, false).write(&[root]).unwrap();
        let mut img = Image::from_u8(2, 1, ChannelLayout::Rgb, vec![100; 6]).unwrap();
        img.meta.exif = Some(exif);
        for big in [false, true] {
            let bytes = encode(&img, Format::Tiff, &EncodeOptions { tiff_bigtiff: big, ..Default::default() }).unwrap();
            let file = Tiff::parse(&bytes).unwrap();
            assert_eq!(file.ifds.len(), 1);
            assert_eq!(file.ifds[0].string(271).as_deref(), Some("Camera maker"));
            assert_eq!(file.exif().unwrap().value(65000), Some(&Value::Short(vec![17, 42])));
            assert_eq!(file.ifds[0].gps.as_ref().unwrap().value(2), Some(&Value::Rational(vec![(42, 1), (30, 1), (0, 1)])));
            if !big {
                let back = decode(&bytes).unwrap();
                let block = Tiff::parse(back.meta.exif.as_ref().unwrap()).unwrap();
                assert_eq!(block.exif().unwrap().value(33434), Some(&Value::Rational(vec![(1, 125)])));
            }
        }
        let bytes = encode(&img, Format::Tiff, &EncodeOptions { embed_metadata: false, ..Default::default() }).unwrap();
        assert!(Tiff::parse(&bytes).unwrap().exif().is_none());
    }
}
