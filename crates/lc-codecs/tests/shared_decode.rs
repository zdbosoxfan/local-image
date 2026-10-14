//! Synthetic fixtures pin the Library linear-light decode contract across consolidation.
use lightcraft_codecs::{DecodeOptions, decode};

fn hash(values: impl Iterator<Item = u8>) -> u64 {
    values.fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

#[test]
fn shared_decode_fixed_linear_goldens() {
    for (name, expected) in [
        ("rgb8-444.jpg", 0x9aec4fb5728d1d08),
        ("rgb8-420.jpg", 0xed2d4f2bb84f9cdd),
        ("rgb8.png", 0x7d1015b5f18350d5),
        ("rgb8.tif", 0x7d1015b5f18350d5),
        ("rgb8.webp", 0x7d1015b5f18350d5),
        ("rgba16.png", 0x686806e270699146),
        ("rgba16-none.tif", 0x686806e270699146),
        ("rgba16-lzw.tif", 0x686806e270699146),
        ("rgba16-deflate.tif", 0x686806e270699146),
        ("rgba16-packbits.tif", 0x686806e270699146),
        ("rgb32f.tif", 0x7aec8af0710cc9a8),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shared-export").join(name);
        let bytes = std::fs::read(path).unwrap();
        let start = std::time::Instant::now();
        let d = decode(&bytes, DecodeOptions::default()).unwrap();
        let elapsed = start.elapsed();
        let h = hash(
            d.image
                .data
                .iter()
                .flatten()
                .flat_map(|v| v.to_bits().to_le_bytes())
                .chain(d.alpha.iter().flat_map(|a| a.data.iter()).flat_map(|v| v.to_bits().to_le_bytes())),
        );
        assert_eq!(h, expected, "{name}: linear pixels/alpha changed");
        eprintln!("{name}: {h:#018x}, {}x{}, {:?}", d.width, d.height, elapsed);
    }
}

#[test]
fn shared_decode_heic_opens_in_library_and_matches_apple_reference() {
    let bytes = include_bytes!("fixtures/shared-decode-heic/checker-64.heic");
    let reference = include_bytes!("fixtures/shared-decode-heic/checker-64.ref.png");
    assert!(lightcraft_codecs::Format::Heif.can_decode());
    let start = std::time::Instant::now();
    let image = decode(bytes, DecodeOptions::default()).unwrap();
    let elapsed = start.elapsed();
    let expected = decode(reference, DecodeOptions::default()).unwrap();
    assert_eq!((image.width, image.height, image.orientation), (64, 64, 1));
    let a = image.to_srgb8();
    let b = expected.to_srgb8();
    let max_error = a.data.iter().flatten().zip(b.data.iter().flatten()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert_eq!(max_error, 0, "achromatic HEIC matches Apple's decode sample for sample");
    let thumb = lightcraft_codecs::decode_thumbnail(bytes, 16).unwrap();
    assert_eq!((thumb.image.width, thumb.image.height), (16, 16));
    eprintln!("HEIC Library decode 64x64={elapsed:?}; max Apple-reference error={max_error}");
}

#[cfg(feature = "jxl")]
#[test]
fn shared_decode_jxl_library_retains_color_alpha_precision_and_editor_agrees() {
    let bytes = include_bytes!("../../pc-codecs/tests/fixtures/j06-jxl-conformance/alpha_premultiplied.jxl");
    let library = decode(bytes, DecodeOptions::default()).unwrap();
    let editor = photocraft_codecs::decode(bytes).unwrap();
    assert_eq!((library.width, library.height), editor.dimensions());
    assert_eq!(library.bit_depth, 12);
    assert!(!library.float);
    assert_eq!(library.space.named, Some(lightcraft_codecs::NamedSpace::Rec2020));
    let alpha = library.alpha.as_ref().unwrap();
    let mut max_error = 0.0f32;
    for (i, pixel) in library.image.data.iter().enumerate() {
        for (c, value) in pixel.iter().enumerate() {
            max_error = max_error.max((value - editor.sample_normalized(i * 4 + c)).abs());
        }
        assert_eq!(alpha.data[i], editor.sample_normalized(i * 4 + 3).clamp(0.0, 1.0));
    }
    assert!(max_error < 1e-6, "shared associated alpha handling: {max_error}");
}

#[test]
fn shared_decode_container_gamma_orientation_and_source_limits() {
    let mut bytes = Vec::new();
    {
        let mut info = png::Info::with_size(3, 2);
        info.exif_metadata = Some(std::borrow::Cow::Owned(lightcraft_codecs::exif::minimal_exif(6)));
        let mut encoder = png::Encoder::with_info(&mut bytes, info).unwrap();
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_source_gamma(png::ScaledFloat::new(1.0));
        encoder.write_header().unwrap().write_image_data(&[128; 18]).unwrap();
    }
    let image = decode(&bytes, DecodeOptions::default()).unwrap();
    assert_eq!((image.width, image.height, image.orientation), (3, 2, 6));
    assert!((image.image.data[0][0] - 128.0 / 255.0).abs() < 1e-6);
    let opts = lightcraft_codecs::ThumbnailOptions { max_pixels: 5, ..lightcraft_codecs::ThumbnailOptions::new(1) };
    assert!(matches!(lightcraft_codecs::decode_thumbnail_with(&bytes, &opts), Err(lightcraft_codecs::Error::TooLarge(3, 2))));
}
