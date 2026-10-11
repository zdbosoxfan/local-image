use lightcraft_codecs::{
    DecodeOptions, decode,
    encode::{
        ChromaSubsampling, EncodeImage, EncodeMeta, Samples, TiffCompression, encode_avif, encode_jpeg, encode_png, encode_tiff, encode_webp_lossless,
    },
};

// Helper to build a small u8 EncodeImage.
fn u8_image(width: u32, height: u32, channels: u8, data: &[u8]) -> EncodeImage<'_> {
    EncodeImage::new(width, height, channels, Samples::U8(data))
}

// Helper to read an RGBA pixel from Rgba8's `data` (which is a row-major Vec<[u8;4]>).
fn rgba_pixel(out: &lightcraft_raster::Rgba8, x: u32, y: u32) -> [u8; 4] {
    let idx = y as usize * out.width + x as usize;
    out.data[idx]
}

#[test]
fn jpeg_rejects_float_samples() {
    let data = [0.0f32; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::F32(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
}

#[test]
fn jpeg_rejects_zero_dimensions() {
    let data = [0u8; 1];
    let img = EncodeImage::new(0, 1, 1, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
    let img = EncodeImage::new(1, 0, 1, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
}

#[test]
fn jpeg_rejects_invalid_channels() {
    let data = [0u8; 8];
    let img = EncodeImage::new(1, 1, 0, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
    let img = EncodeImage::new(1, 1, 5, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
}

#[test]
fn jpeg_rejects_short_buffer() {
    let data = [0u8; 3]; // need 6 for 2x2 RGB
    let img = EncodeImage::new(2, 2, 3, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
}

#[test]
fn jpeg_rejects_large_dimensions() {
    let data = vec![0u8; 65536]; // width 65536
    let img = EncodeImage::new(65536, 1, 1, Samples::U8(&data));
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::default(), &EncodeMeta::default()).is_err());
}

#[test]
fn jpeg_encodes_rgb_and_starts_with_soi() {
    let data = [255u8, 0, 0, 0, 255, 0];
    let img = u8_image(2, 1, 3, &data);
    let out = encode_jpeg(&img, 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
    assert!(out.len() > 2);
    assert_eq!(out[0], 0xFF);
    assert_eq!(out[1], 0xD8);
}

#[test]
fn jpeg_encodes_grayscale() {
    let data = [128u8; 4];
    let img = u8_image(2, 2, 1, &data);
    let out = encode_jpeg(&img, 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert!(decoded.grayscale);
}

#[test]
fn jpeg_encodes_rgba_drops_alpha() {
    let data = [255u8, 0, 0, 255, 0, 255, 0, 128];
    let img = u8_image(1, 2, 4, &data);
    let out = encode_jpeg(&img, 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert!(!decoded.has_alpha);
    assert_eq!(decoded.width, 1);
    assert_eq!(decoded.height, 2);
}

#[test]
fn jpeg_metadata_limits_rejected() {
    let data = [0u8; 3];
    let img = u8_image(1, 1, 3, &data);

    // EXIF > 65527
    let large_exif = vec![0u8; 65528];
    let meta = EncodeMeta { exif: Some(&large_exif), ..Default::default() };
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::S444, &meta).is_err());

    // XMP > 65533
    let large_xmp = "a".repeat(65534);
    let meta = EncodeMeta { xmp: Some(&large_xmp), ..Default::default() };
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::S444, &meta).is_err());

    // ICC too many chunks (>255 chunks)
    let huge_icc = vec![0u8; 255 * 65519 + 1];
    let meta = EncodeMeta { icc: Some(&huge_icc), ..Default::default() };
    assert!(encode_jpeg(&img, 90, ChromaSubsampling::S444, &meta).is_err());
}

#[test]
fn jpeg_roundtrip_via_decode() {
    let data: Vec<u8> = (0..48).collect(); // 4x4 RGB
    let img = u8_image(4, 4, 3, &data);
    let out = encode_jpeg(&img, 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 4);
    assert!(!decoded.float);
}

#[test]
fn png_rejects_float_samples() {
    let data = [0.0f32; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::F32(&data));
    assert!(encode_png(&img, &EncodeMeta::default()).is_err());
}

#[test]
fn png_roundtrip_u8_exact() {
    // 2x2 RGBA with distinct values
    let data: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 0];
    let img = u8_image(2, 2, 4, &data);
    let out = encode_png(&img, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    let srgb8 = decoded.to_srgb8();

    assert_eq!(srgb8.width, 2);
    assert_eq!(srgb8.height, 2);

    // Check each pixel (allow tolerance of 1 due to float rounding)
    for y in 0..2 {
        for x in 0..2 {
            let expected = [
                data[((y * 2 + x) * 4) as usize],
                data[((y * 2 + x) * 4 + 1) as usize],
                data[((y * 2 + x) * 4 + 2) as usize],
                data[((y * 2 + x) * 4 + 3) as usize],
            ];
            let actual = rgba_pixel(&srgb8, x as u32, y as u32);
            for c in 0..4 {
                assert!(
                    (expected[c] as i32 - actual[c] as i32).abs() <= 1,
                    "pixel ({},{}) channel {} off by >1: expected {}, got {}",
                    x,
                    y,
                    c,
                    expected[c],
                    actual[c]
                );
            }
        }
    }
}

#[test]
fn png_roundtrip_u16_bit_depth() {
    let data: Vec<u16> = vec![65535, 0, 32768];
    let img = EncodeImage::new(1, 1, 3, Samples::U16(&data));
    let out = encode_png(&img, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert_eq!(decoded.bit_depth, 16);
    assert!(!decoded.float);
}

#[test]
fn png_metadata_passthrough() {
    let data = [128u8; 4]; // 1x1 RGBA
    let img = u8_image(1, 1, 4, &data);
    let icc = b"dummy ICC profile";
    let exif = b"II*\0\x08\0\0\0";
    let xmp = "test xmp packet";
    let meta = EncodeMeta { icc: Some(icc), exif: Some(exif), xmp: Some(xmp), ppi: Some(300) };
    let out = encode_png(&img, &meta).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert!(decoded.icc.is_some());
    assert!(decoded.exif.is_some());
    assert!(decoded.xmp.is_some());
    assert_eq!(decoded.xmp.as_deref(), Some(xmp));
}

#[test]
fn tiff_roundtrip_u8_rgb() {
    let data: Vec<u8> = (0..12).collect(); // 2x2 RGB
    let img = u8_image(2, 2, 3, &data);
    for comp in [TiffCompression::None, TiffCompression::Lzw, TiffCompression::Deflate, TiffCompression::PackBits] {
        let out = encode_tiff(&img, comp, &EncodeMeta::default()).unwrap();
        assert!(!out.is_empty());
        let decoded = decode(&out, DecodeOptions::default()).unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert!(!decoded.float);
    }
}

#[test]
fn tiff_roundtrip_f32() {
    let data: Vec<f32> = vec![0.0, 0.5, 1.0, 0.25, 0.75, 0.125];
    let img = EncodeImage::new(2, 1, 3, Samples::F32(&data));
    let out = encode_tiff(&img, TiffCompression::Deflate, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    assert!(decoded.float);
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 1);
}

#[test]
fn tiff_rejects_float_for_png() {
    let data = [0.0f32; 4];
    let img = EncodeImage::new(1, 1, 4, Samples::F32(&data));
    assert!(encode_png(&img, &EncodeMeta::default()).is_err());
    assert!(encode_webp_lossless(&img, &EncodeMeta::default()).is_err());
}

#[test]
fn webp_roundtrip_lossless_u8() {
    let data: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 0];
    let img = u8_image(2, 2, 4, &data);
    let out = encode_webp_lossless(&img, &EncodeMeta::default()).unwrap();
    let decoded = decode(&out, DecodeOptions::default()).unwrap();
    let srgb8 = decoded.to_srgb8();

    assert_eq!(srgb8.width, 2);
    assert_eq!(srgb8.height, 2);
    for y in 0..2 {
        for x in 0..2 {
            let expected = [
                data[((y * 2 + x) * 4) as usize],
                data[((y * 2 + x) * 4 + 1) as usize],
                data[((y * 2 + x) * 4 + 2) as usize],
                data[((y * 2 + x) * 4 + 3) as usize],
            ];
            let actual = rgba_pixel(&srgb8, x as u32, y as u32);
            for c in 0..4 {
                assert!(
                    (expected[c] as i32 - actual[c] as i32).abs() <= 1,
                    "pixel ({},{}) channel {} off by >1: expected {}, got {}",
                    x,
                    y,
                    c,
                    expected[c],
                    actual[c]
                );
            }
        }
    }
}

#[test]
fn webp_rejects_non_u8() {
    let data_u16 = [0u16; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::U16(&data_u16));
    assert!(encode_webp_lossless(&img, &EncodeMeta::default()).is_err());
    let data_f32 = [0.0f32; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::F32(&data_f32));
    assert!(encode_webp_lossless(&img, &EncodeMeta::default()).is_err());
}

#[test]
fn avif_rejects_float_samples() {
    let data = [0.0f32; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::F32(&data));
    // Either the encoder rejects floats, or AVIF is not available in this build.
    assert!(encode_avif(&img, 90, 5, &EncodeMeta::default()).is_err());
}

#[test]
fn determinism_png() {
    let data = [1u8, 2, 3, 4, 5, 6];
    let img = u8_image(2, 1, 3, &data);
    let out1 = encode_png(&img, &EncodeMeta::default()).unwrap();
    let out2 = encode_png(&img, &EncodeMeta::default()).unwrap();
    assert_eq!(out1, out2);
}

#[test]
fn determinism_tiff() {
    let data = [1u8, 2, 3, 4, 5, 6];
    let img = u8_image(2, 1, 3, &data);
    let out1 = encode_tiff(&img, TiffCompression::Deflate, &EncodeMeta::default()).unwrap();
    let out2 = encode_tiff(&img, TiffCompression::Deflate, &EncodeMeta::default()).unwrap();
    assert_eq!(out1, out2);
}

#[test]
fn determinism_webp() {
    let data = [1u8, 2, 3, 4, 5, 6];
    let img = u8_image(2, 1, 3, &data);
    let out1 = encode_webp_lossless(&img, &EncodeMeta::default()).unwrap();
    let out2 = encode_webp_lossless(&img, &EncodeMeta::default()).unwrap();
    assert_eq!(out1, out2);
}
