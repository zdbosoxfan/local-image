use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image, decode_as, encode};

#[test]
fn rle_packets_stay_within_scanlines_and_roundtrip_every_layout() {
    for layout in [ChannelLayout::Gray, ChannelLayout::GrayA, ChannelLayout::Rgb, ChannelLayout::Rgba] {
        for (width, height) in [(1, 1), (300, 4), (129, 3), (5, 130)] {
            for solid in [false, true] {
                let bpp = layout.channels();
                let data: Vec<u8> = (0..width * height)
                    .flat_map(|i| {
                        let v = if solid || i % width < 150 { 40 } else { i as u8 };
                        [v, v.wrapping_add(1), v ^ 90, 255 - v][..bpp].to_vec()
                    })
                    .collect();
                let img = Image::from_u8(width, height, layout, data).unwrap();
                let file = encode(&img, Format::Tga, &EncodeOptions::default()).unwrap();
                let mut at = 18;
                let mut seen = 0;
                while seen < width * height {
                    let header = file[at];
                    let count = u32::from(header & 127) + 1;
                    assert!(seen % width + count <= width, "{layout:?}: packet crosses a row");
                    at += 1 + if header & 128 != 0 { bpp } else { count as usize * bpp };
                    seen += count;
                }
                let back = decode_as(Format::Tga, &file).unwrap();
                assert_eq!((back.dimensions(), back.layout(), back.data()), (img.dimensions(), layout, img.data()));
            }
        }
    }
}

#[test]
fn targa_stores_bottom_up_bgra_with_alpha_and_footer() {
    let img = Image::from_u8(2, 2, ChannelLayout::Rgba, vec![255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255]).unwrap();
    let file = encode(&img, Format::Tga, &EncodeOptions::default()).unwrap();
    assert_eq!((file[2], file[16], file[17]), (10, 32, 8));
    assert_eq!(&file[18..23], &[129, 255, 0, 0, 255]);
    assert!(file.ends_with(b"\0\0\0\0\0\0\0\0TRUEVISION-XFILE.\0"));
}
