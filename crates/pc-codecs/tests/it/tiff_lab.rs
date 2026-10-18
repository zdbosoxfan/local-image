use photocraft_codecs::{
    ChannelLayout as L, EncodeOptions, FidelityWarning, Format, Image, SampleType as S, TiffCompression, decode, encode, fidelity_warnings,
};

#[test]
fn lab_tiff_preserves_channels_and_depth_with_each_compression() {
    for layout in [L::Lab, L::LabA] {
        for sample in [S::U8, S::U16, S::F32] {
            for compression in [TiffCompression::None, TiffCompression::Lzw, TiffCompression::Deflate, TiffCompression::PackBits] {
                let values: Vec<f32> =
                    [[0.5, 128. / 255., 128. / 255., 0.5], [0.8, 200. / 255., 80. / 255., 1.]].iter().flat_map(|p| p[..layout.channels()].to_vec()).collect();
                let img = Image::from_normalized(2, 1, layout, sample, &values).unwrap();
                let bytes = encode(&img, Format::Tiff, &EncodeOptions { tiff_compression: compression, ..Default::default() }).unwrap();
                let file = lightcraft_tiff::Tiff::parse(&bytes).unwrap();
                assert_eq!(file.ifds[0].u16(262), Some(8));
                let back = decode(&bytes).unwrap();
                assert_eq!(back.layout(), layout);
                assert_eq!(back.sample_type(), if sample == S::U8 { S::U8 } else { S::U16 });
                for (a, b) in back.to_normalized().iter().zip(img.to_normalized()) {
                    assert!((a - b).abs() <= 1.5 / 65535.);
                }
            }
        }
    }
}

#[test]
fn lab_to_rgb_warns_and_neutral_lab_converts_to_neutral_rgb() {
    let img = Image::from_normalized(1, 1, L::Lab, S::F32, &[0.5, 128. / 255., 128. / 255.]).unwrap();
    assert!(fidelity_warnings(&img, Format::Png).iter().any(|w| matches!(w, FidelityWarning::LabConverted { .. })));
    let rgb = img.convert(L::Rgb, S::F32).to_normalized();
    for v in rgb {
        assert!((v - 0.4663).abs() < 0.001);
    }
}

#[test]
fn cielab_and_icclab_decode_chroma_in_both_byte_orders() {
    use lightcraft_tiff::{ByteOrder, IfdBuilder, ImageData, TiffWriter, Value};
    for order in [ByteOrder::Little, ByteOrder::Big] {
        for signed in [false, true] {
            for bits in [8u16, 16] {
                let mut dir = IfdBuilder::new();
                for (tag, value) in [
                    (256, Value::Long(vec![1])),
                    (257, Value::Long(vec![1])),
                    (258, Value::Short(vec![bits; 3])),
                    (259, Value::Short(vec![1])),
                    (262, Value::Short(vec![if signed { 8 } else { 9 }])),
                    (277, Value::Short(vec![3])),
                ] {
                    dir.set(tag, value);
                }
                let bytes = if bits == 8 {
                    vec![128, if signed { 236 } else { 108 }, if signed { 30 } else { 158 }]
                } else {
                    let values = [32768u16, if signed { (-5120i16) as u16 } else { 27648 }, if signed { 7680 } else { 40448 }];
                    values.into_iter().flat_map(|v| if order == ByteOrder::Little { v.to_le_bytes() } else { v.to_be_bytes() }).collect()
                };
                dir.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![bytes] });
                let back = decode(&TiffWriter::new(order, false).write(&[dir]).unwrap()).unwrap();
                assert_eq!(back.layout(), L::Lab);
                let values = back.to_normalized();
                assert!((values[1] - 108. / 255.).abs() <= 1. / 65535.);
                assert!((values[2] - 158. / 255.).abs() <= 1. / 65535.);
            }
        }
    }
}
