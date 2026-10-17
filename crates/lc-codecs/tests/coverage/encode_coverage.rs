use lightcraft_codecs::encode::{
    ChromaSubsampling, EncodeImage, EncodeMeta, Samples, TiffCompression, encode_avif, encode_jpeg, encode_png, encode_tiff, encode_webp_lossless,
};

// Helper to build a small u8 EncodeImage.
fn u8_image(width: u32, height: u32, channels: u8, data: &[u8]) -> EncodeImage<'_> {
    EncodeImage::new(width, height, channels, Samples::U8(data))
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
fn png_rejects_float_samples() {
    let data = [0.0f32; 3];
    let img = EncodeImage::new(1, 1, 3, Samples::F32(&data));
    assert!(encode_png(&img, &EncodeMeta::default()).is_err());
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
