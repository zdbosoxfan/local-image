use photocraft_cms::Builtin;
use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image, Metadata, SampleType};

#[test]
fn overwrite_original_retains_profile_depth_metadata_and_converts_composite() {
    for (format, layout, sample, profile) in [
        (Format::Png, ChannelLayout::Rgb, SampleType::U16, Builtin::DisplayP3),
        (Format::Jpeg, ChannelLayout::Rgb, SampleType::U8, Builtin::AdobeRgbCompat),
        (Format::WebP, ChannelLayout::Rgb, SampleType::U8, Builtin::DisplayP3),
        (Format::Tiff, ChannelLayout::Rgb, SampleType::F32, Builtin::ProPhotoCompat),
        (Format::Tiff, ChannelLayout::Cmyk, SampleType::U16, Builtin::CoatedCmyk),
        (Format::Png, ChannelLayout::Gray, SampleType::U16, Builtin::SGray),
    ] {
        let icc = profile.profile().to_bytes().to_vec();
        let source =
            Image::from_normalized(4, 3, layout, sample, &vec![0.3; 12 * layout.channels()]).unwrap().with_icc(Some(icc.clone())).with_meta(Metadata {
                xmp: Some("<x:xmpmeta xmlns:x='adobe:ns:meta/'>original</x:xmpmeta>".into()),
                dpi: Some((300.0, 300.0)),
                text: vec![("Artist".into(), "original artist".into())],
                ..Default::default()
            });
        let original = photocraft_codecs::encode(&source, format, &EncodeOptions::default()).unwrap();
        let before = photocraft_codecs::decode(&original).unwrap();
        let red = Image::from_u8(4, 3, ChannelLayout::Rgb, [180, 50, 70].repeat(12)).unwrap();
        let png = photocraft_codecs::encode(&red, Format::Png, &EncodeOptions::default()).unwrap();
        let doc = photocraft_io::import("composite.png", &png).unwrap().document;
        let out = photocraft_io::export_over_original(&doc, &original, 93).unwrap();
        assert_eq!(photocraft_codecs::detect(&out.bytes), Some(format));
        let new = photocraft_codecs::decode(&out.bytes).unwrap();
        assert_eq!(new.icc, Some(icc));
        assert_eq!(new.layout(), before.layout());
        assert_eq!(new.sample_type(), sample);
        assert_eq!(new.meta.xmp, before.meta.xmp);
        assert_eq!(new.meta.text, before.meta.text);
        assert_ne!(new.data(), before.data());
        // Tagged values must be transformed, not relabelled with the original profile.
        if layout.is_rgb() {
            let t = photocraft_cms::Transform::new(profile.profile(), Builtin::Srgb.profile(), photocraft_cms::Intent::RelativeColorimetric, true).unwrap();
            let vals = new.to_normalized();
            let mut got = vec![0.0; vals.len()];
            t.convert_f32(&vals, 3, &mut got, 3, false);
            for pixel in got.as_chunks::<3>().0 {
                for (value, wanted) in pixel.iter().zip([180.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0]) {
                    assert!((value - wanted).abs() < 0.05, "{format:?}: {pixel:?}");
                }
            }
        }
    }
}

#[test]
fn overwrite_original_normalizes_exif_orientation_and_preserves_camera_metadata() {
    let mut exif = b"II*\0\x08\0\0\0\x02\0".to_vec();
    exif.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0]);
    exif.extend_from_slice(&[0x0f, 0x01, 2, 0, 6, 0, 0, 0, 38, 0, 0, 0]);
    exif.extend_from_slice(&[0, 0, 0, 0]);
    exif.extend_from_slice(b"Camera\0");
    for format in [Format::Png, Format::Jpeg, Format::WebP] {
        let img = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![80; 18]).unwrap().with_meta(Metadata { exif: Some(exif.clone()), ..Default::default() });
        let original = photocraft_codecs::encode(&img, format, &EncodeOptions::default()).unwrap();
        let doc = photocraft_io::import("source", &original).unwrap().document;
        let out = photocraft_io::export_over_original(&doc, &original, 85).unwrap();
        let back = photocraft_codecs::decode(&out.bytes).unwrap();
        assert_eq!(back.dimensions(), (doc.size.width, doc.size.height));
        let saved = back.meta.exif.unwrap();
        assert_eq!(photocraft_codecs::exif_orientation(&saved), 1);
        assert!(saved.windows(6).any(|b| b == b"Camera"));
    }
}

#[test]
fn overwrite_original_jpeg_quality_uses_preference_and_unsupported_input_fails() {
    let img = Image::from_u8(8, 8, ChannelLayout::Rgb, (0..192).map(|i| (i * 37) as u8).collect()).unwrap();
    let png = photocraft_codecs::encode(&img, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("composite", &png).unwrap().document;
    let jpeg = photocraft_codecs::encode(&img, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let app13 = b"opaque IPTC and Photoshop metadata";
    let mut tagged = vec![0xff, 0xd8, 0xff, 0xed];
    tagged.extend_from_slice(&((app13.len() + 2) as u16).to_be_bytes());
    tagged.extend_from_slice(app13);
    tagged.extend_from_slice(&jpeg[2..]);
    let preserved = photocraft_io::export_over_original(&doc, &tagged, 85).unwrap();
    assert!(preserved.bytes.windows(app13.len()).any(|b| b == app13));
    let low = photocraft_io::export_over_original(&doc, &jpeg, 10).unwrap();
    let high = photocraft_io::export_over_original(&doc, &jpeg, 100).unwrap();
    assert_ne!(low.bytes, high.bytes);
    assert!(photocraft_io::export_over_original(&doc, b"not an image", 85).is_err());
    let raw = photocraft_raw::testgen::DngSpec::cfa(4, 4, vec![1000; 16]).build();
    let error = photocraft_io::export_over_original(&doc, &raw, 85).unwrap_err().to_string();
    assert!(error.contains("Raw files can't be overwritten"), "{error}");
}

#[test]
fn overwrite_original_removes_stale_embedded_jpeg_thumbnail() {
    let red = Image::from_u8(4, 3, ChannelLayout::Rgb, [255, 0, 0].repeat(12)).unwrap();
    let blue = Image::from_u8(4, 3, ChannelLayout::Rgb, [0, 0, 255].repeat(12)).unwrap();
    let thumb = photocraft_codecs::encode(&blue, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let mut exif = b"II*\0\x08\0\0\0\0\0\x0e\0\0\0\x02\0".to_vec();
    for (tag, value) in [(513u16, 44u32), (514, thumb.len() as u32)] {
        exif.extend_from_slice(&tag.to_le_bytes());
        exif.extend_from_slice(&4u16.to_le_bytes());
        exif.extend_from_slice(&1u32.to_le_bytes());
        exif.extend_from_slice(&value.to_le_bytes());
    }
    exif.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(exif.len(), 44);
    exif.extend_from_slice(&thumb);
    let source = red.clone().with_meta(Metadata { exif: Some(exif), ..Default::default() });
    let jpeg = photocraft_codecs::encode(&source, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let options = lightcraft_codecs::ThumbnailOptions { max_edge: 4, min_embedded_edge: 1, ..lightcraft_codecs::ThumbnailOptions::new(4) };
    let old = lightcraft_codecs::decode_thumbnail_with(&jpeg, &options).unwrap();
    assert_eq!(old.source, lightcraft_codecs::ThumbnailSource::ExifThumbnail);
    let png = photocraft_codecs::encode(&red, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("composite.png", &png).unwrap().document;
    let saved = photocraft_io::export_over_original(&doc, &jpeg, 85).unwrap();
    let new = lightcraft_codecs::decode_thumbnail_with(&saved.bytes, &options).unwrap();
    assert_eq!(new.source, lightcraft_codecs::ThumbnailSource::Scaled);
    assert!(new.image.data.iter().all(|p| p[0] > 240 && p[2] < 10));
}

#[test]
fn overwrite_original_psd_psb_stay_native_flattened_with_profile_and_depth() {
    let source = Image::from_u16(4, 3, ChannelLayout::Rgb, &[20000; 36]).unwrap().with_icc(Some(Builtin::AdobeRgbCompat.profile().to_bytes().to_vec()));
    let png = photocraft_codecs::encode(&source, Format::Png, &EncodeOptions::default()).unwrap();
    let mut original_doc = photocraft_io::import("photo", &png).unwrap().document;
    original_doc.layers.push(photocraft_doc::Layer::new("second", original_doc.layers[0].content.clone()));
    original_doc.metadata.psd_resources.push((65000, "original metadata".into(), std::sync::Arc::new(vec![1, 2, 3])));
    let red = Image::from_u8(4, 3, ChannelLayout::Rgb, [180, 50, 70].repeat(12)).unwrap();
    let red = photocraft_codecs::encode(&red, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("composite", &red).unwrap().document;
    for format in ["psd", "psb"] {
        let original = photocraft_io::export(&original_doc, format, &photocraft_io::ExportOptions::default()).unwrap();
        let saved = photocraft_io::export_over_original(&doc, &original.bytes, 85).unwrap();
        assert_eq!(&saved.bytes[..6], &original.bytes[..6]);
        let flat = photocraft_io::import("result", &saved.bytes).unwrap().document;
        assert_eq!(flat.layers.len(), 1);
        assert_eq!(flat.mode, original_doc.mode);
        assert_eq!(flat.depth, original_doc.depth);
        assert_eq!(flat.icc_profile, original_doc.icc_profile);
        assert!(flat.metadata.psd_resources.iter().any(|(id, _, data)| *id == 65000 && data.as_slice() == [1, 2, 3]));
    }
}
