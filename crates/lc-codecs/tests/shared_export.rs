//! Fixed-byte goldens at the actual Library/Compositing codec boundary.
//! Synthetic pixels and the project's CC0 profiles; no client photos.
use lightcraft_codecs::{ChromaSubsampling as Sub, EncodeImage, EncodeMeta, NamedSpace, Samples, TiffCompression as Tc, encode};
use photocraft_codecs::{ChannelLayout as L, EncodeOptions, Format, Image};

fn golden(name: &str, library: &[u8], editor: &[u8]) {
    assert_eq!(library, editor, "{name}: the two front ends must emit identical bytes");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shared-export").join(name);
    if std::env::var_os("UPDATE_SHARED_CORE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, library).unwrap();
    }
    assert_eq!(library, std::fs::read(path).unwrap(), "{name}: fixed golden changed");
}
fn metadata<'a>(img: &mut Image, icc: &'a [u8], exif: &'a [u8]) -> EncodeMeta<'a> {
    img.icc = Some(icc.to_vec());
    img.meta.exif = Some(exif.to_vec());
    img.meta.xmp = Some("<x:xmpmeta/>".into());
    img.meta.dpi = Some((300.0, 300.0));
    EncodeMeta { icc: Some(icc), exif: Some(exif), xmp: Some("<x:xmpmeta/>"), ppi: Some(300) }
}
const EXIF: &[u8] = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x01\0\0\0\0\0\0\0";

#[test]
fn shared_export_u8_goldens() {
    let px: Vec<u8> = (0..17 * 11 * 3).map(|i| (i * 37 + i / 7) as u8).collect();
    let input = EncodeImage::new(17, 11, 3, Samples::U8(&px));
    let mut img = Image::from_u8(17, 11, L::Rgb, px.clone()).unwrap();
    let icc = lightcraft_codecs::icc::write_named(NamedSpace::Srgb);
    let meta = metadata(&mut img, &icc, EXIF);
    let defaults = EncodeOptions::default();
    for (name, format, lib) in [
        ("rgb8.png", Format::Png, encode::encode_png(&input, &meta).unwrap()),
        ("rgb8.tif", Format::Tiff, encode::encode_tiff(&input, Tc::Deflate, &meta).unwrap()),
        ("rgb8.webp", Format::WebP, encode::encode_webp_lossless(&input, &meta).unwrap()),
    ] {
        golden(name, &lib, &photocraft_codecs::encode(&img, format, &defaults).unwrap());
        let back = photocraft_codecs::decode(&lib).unwrap();
        assert_eq!(back.icc.as_deref(), Some(icc.as_slice()));
        assert_eq!(back.convert(L::Rgb, photocraft_codecs::SampleType::U8).data(), px);
    }
    for (name, sub, subsampled) in [("rgb8-444.jpg", Sub::S444, false), ("rgb8-420.jpg", Sub::S420, true)] {
        let lib = encode::encode_jpeg(&input, 91, sub, &meta).unwrap();
        let opts = EncodeOptions { jpeg_quality: 91, jpeg_chroma_subsampling: subsampled, ..Default::default() };
        golden(name, &lib, &photocraft_codecs::encode(&img, Format::Jpeg, &opts).unwrap());
        assert_eq!(photocraft_codecs::decode(&lib).unwrap().icc.as_deref(), Some(icc.as_slice()));
    }
}

#[test]
fn shared_export_deep_and_float_goldens() {
    let px: Vec<u16> = (0..7 * 5 * 4).map(|i| (i * 977 + 13) as u16).collect();
    let input = EncodeImage::new(7, 5, 4, Samples::U16(&px));
    let mut img = Image::from_u16(7, 5, L::Rgba, &px).unwrap();
    let icc = lightcraft_codecs::icc::write_named(NamedSpace::ProPhoto);
    let meta = metadata(&mut img, &icc, EXIF);
    golden("rgba16.png", &encode::encode_png(&input, &meta).unwrap(), &photocraft_codecs::encode(&img, Format::Png, &Default::default()).unwrap());
    for (name, lc, pc) in [
        ("rgba16-none.tif", Tc::None, photocraft_codecs::TiffCompression::None),
        ("rgba16-lzw.tif", Tc::Lzw, photocraft_codecs::TiffCompression::Lzw),
        ("rgba16-deflate.tif", Tc::Deflate, photocraft_codecs::TiffCompression::Deflate),
        ("rgba16-packbits.tif", Tc::PackBits, photocraft_codecs::TiffCompression::PackBits),
    ] {
        let opts = EncodeOptions { tiff_compression: pc, ..Default::default() };
        golden(name, &encode::encode_tiff(&input, lc, &meta).unwrap(), &photocraft_codecs::encode(&img, Format::Tiff, &opts).unwrap());
    }
    let px: Vec<f32> = (0..5 * 3 * 3).map(|i| i as f32 / 16.0 - 0.5).collect();
    let input = EncodeImage::new(5, 3, 3, Samples::F32(&px));
    let img = Image::from_f32(5, 3, L::Rgb, &px).unwrap();
    golden(
        "rgb32f.tif",
        &encode::encode_tiff(&input, Tc::Deflate, &Default::default()).unwrap(),
        &photocraft_codecs::encode(&img, Format::Tiff, &Default::default()).unwrap(),
    );
}

#[test]
fn shared_export_named_profiles_and_cms() {
    use photocraft_cms::{Builtin, Intent, Transform};
    for (lc, pc) in [
        (NamedSpace::Srgb, Builtin::Srgb),
        (NamedSpace::DisplayP3, Builtin::DisplayP3),
        (NamedSpace::AdobeRgb, Builtin::AdobeRgbCompat),
        (NamedSpace::ProPhoto, Builtin::ProPhotoCompat),
        (NamedSpace::Rec2020, Builtin::Rec2020),
    ] {
        assert_eq!(lightcraft_codecs::icc::write_named(lc).as_slice(), pc.profile().to_bytes().as_slice());
    }
    // A LUT/CMYK profile exercises the shared CMS fallback, not the matrix shortcut.
    let profile = Builtin::CoatedCmyk.profile();
    let px = [25u8, 100, 180, 30, 0, 0, 0, 0];
    let mut img = Image::from_u8(2, 1, L::Cmyk, px.to_vec()).unwrap();
    img.icc = Some(profile.to_bytes().as_ref().clone());
    let bytes = photocraft_codecs::encode(&img, Format::Tiff, &Default::default()).unwrap();
    let working = lightcraft_codecs::decode(&bytes, Default::default()).unwrap().to_working();
    let samples: Vec<f32> = px.iter().map(|v| *v as f32 / 255.0).collect();
    let mut expected = [0.0; 6];
    Transform::new(profile, photocraft_cms::builtin::linear_rec2020(), Intent::RelativeColorimetric, true).unwrap().convert_f32(
        &samples,
        4,
        &mut expected,
        3,
        false,
    );
    for (got, want) in working.data.iter().flatten().zip(expected) {
        assert!((got - want).abs() < 1e-6, "{got} vs {want}");
    }
}

#[test]
fn shared_export_multisegment_icc_and_sampling_422() {
    let px = vec![120u8; 19 * 13 * 3];
    let input = EncodeImage::new(19, 13, 3, Samples::U8(&px));
    let mut img = Image::from_u8(19, 13, L::Rgb, px.clone()).unwrap();
    let icc = vec![31; 140_000];
    let meta = metadata(&mut img, &icc, EXIF);
    for (lc, pc) in [
        (Sub::S444, photocraft_codecs::ChromaSubsampling::S444),
        (Sub::S422, photocraft_codecs::ChromaSubsampling::S422),
        (Sub::S420, photocraft_codecs::ChromaSubsampling::S420),
    ] {
        let lib = encode::encode_jpeg(&input, 90, lc, &meta).unwrap();
        assert_eq!(lib, photocraft_codecs::encode_jpeg_with_sampling(&img, &Default::default(), pc).unwrap());
        assert_eq!(photocraft_codecs::decode(&lib).unwrap().icc.as_deref(), Some(icc.as_slice()));
    }
}

#[cfg(all(feature = "avif", not(target_arch = "wasm32")))]
#[test]
fn shared_export_avif_goldens() {
    for depth in [8, 16] {
        let px: Vec<u16> = (0..16 * 16 * 4).map(|i| (i * 71) as u16).collect();
        let small: Vec<u8> = px.iter().map(|v| (v >> 8) as u8).collect();
        let input = EncodeImage::new(16, 16, 4, if depth == 8 { Samples::U8(&small) } else { Samples::U16(&px) });
        let img = if depth == 8 { Image::from_u8(16, 16, L::Rgba, small.clone()).unwrap() } else { Image::from_u16(16, 16, L::Rgba, &px).unwrap() };
        let lib = encode::encode_avif(&input, 90, 8, &Default::default()).unwrap();
        golden(&format!("rgba{depth}.avif"), &lib, &photocraft_codecs::encode(&img, Format::Avif, &Default::default()).unwrap());
    }
}

#[test]
fn shared_export_preserves_stored_orientation_when_requested() {
    let px = vec![100; 17 * 11 * 3];
    let input = EncodeImage::new(17, 11, 3, Samples::U8(&px));
    let mut img = Image::from_u8(17, 11, L::Rgb, px.clone()).unwrap();
    let mut exif = EXIF.to_vec();
    exif[18..20].copy_from_slice(&6u16.to_le_bytes());
    let icc = lightcraft_codecs::icc::write_named(NamedSpace::Srgb);
    let mut meta = metadata(&mut img, &icc, &exif);
    let xmp = r#"<rdf:Description tiff:Orientation="6"/>"#;
    meta.xmp = Some(xmp);
    img.meta.xmp = Some(xmp.into());
    let keep = EncodeOptions { keep_orientation: true, jpeg_chroma_subsampling: false, ..Default::default() };
    for (format, lib) in [
        (Format::Jpeg, encode::encode_jpeg(&input, 90, Sub::S444, &meta).unwrap()),
        (Format::Png, encode::encode_png(&input, &meta).unwrap()),
        (Format::WebP, encode::encode_webp_lossless(&input, &meta).unwrap()),
    ] {
        assert_eq!(lib, photocraft_codecs::encode(&img, format, &keep).unwrap());
        let decoded = lightcraft_codecs::decode(&lib, Default::default()).unwrap();
        assert_eq!(decoded.orientation, 6);
        assert_eq!(decoded.exif.as_deref(), Some(exif.as_slice()));
        assert_eq!(decoded.xmp.as_deref(), Some(xmp));
        let upright = photocraft_codecs::encode(&img, format, &EncodeOptions { keep_orientation: false, ..keep.clone() }).unwrap();
        assert_eq!(lightcraft_codecs::decode(&upright, Default::default()).unwrap().orientation, 1);
    }
}
