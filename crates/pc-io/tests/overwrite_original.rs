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
        assert_eq!(lightcraft_tiff::exif::orientation(&saved), 1);
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
    let raw = lightcraft_raw::testgen::DngSpec::cfa(4, 4, vec![1000; 16]).build();
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

#[test]
fn ds2_overwrite_psd_retains_merged_alpha_and_refuses_damaged_channels() {
    for layout in [ChannelLayout::Rgb, ChannelLayout::Rgba] {
        let image = Image::from_u8(4, 3, layout, vec![100; 12 * layout.channels()]).unwrap();
        let png = photocraft_codecs::encode(&image, Format::Png, &EncodeOptions::default()).unwrap();
        let document = photocraft_io::import("photo", &png).unwrap().document;
        let original = photocraft_io::export(&document, "psd", &Default::default()).unwrap();
        let psd = photocraft_psd::PsdFile::from_bytes(&original.bytes).unwrap();
        assert_eq!(psd.merged_has_alpha(), layout.has_alpha());
        let saved = photocraft_io::export_over_original(&document, &original.bytes, 85).unwrap();
        assert_eq!(photocraft_psd::PsdFile::from_bytes(&saved.bytes).unwrap().merged_has_alpha(), layout.has_alpha());
        let mut damaged = psd;
        let channel = &mut damaged.layer_info.as_mut().unwrap().layers[0].channels[0];
        channel.compression = Some(photocraft_psd::Compression::Raw);
        channel.data.clear();
        let bytes = damaged.to_bytes().unwrap();
        assert!(!photocraft_io::import("damaged", &bytes).unwrap().warnings.is_empty());
        assert!(photocraft_io::export_over_original(&document, &bytes, 85).unwrap_err().to_string().contains("damaged originals"));
    }
}

// Fixed-offset Exif fixture: its opaque MakerNote holds an absolute TIFF-relative
// pointer into a private tail. Rebuilding or shifting the IFDs would corrupt it.
fn overwrite_geometry_exif(little: bool) -> Vec<u8> {
    fn put(b: &mut [u8], at: usize, n: usize, value: u32, little: bool) {
        let raw = if little { value.to_le_bytes() } else { value.to_be_bytes() };
        b[at..at + n].copy_from_slice(if little { &raw[..n] } else { &raw[4 - n..] });
    }
    fn entry(b: &mut [u8], at: usize, tag: u16, typ: u16, count: u32, value: u32, little: bool) {
        put(b, at, 2, tag.into(), little);
        put(b, at + 2, 2, typ.into(), little);
        put(b, at + 4, 4, count, little);
        put(b, at + 8, if typ == 3 && count == 1 { 2 } else { 4 }, value, little);
    }
    let mut exif = vec![0; 143];
    exif[..2].copy_from_slice(if little { b"II" } else { b"MM" });
    put(&mut exif, 2, 2, 42, little);
    put(&mut exif, 4, 4, 8, little);
    put(&mut exif, 8, 2, 5, little);
    entry(&mut exif, 10, 256, 3, 1, 12, little);
    entry(&mut exif, 22, 257, 4, 1, 8, little);
    entry(&mut exif, 34, 274, 3, 1, 6, little);
    entry(&mut exif, 46, 271, 2, 7, 128, little);
    entry(&mut exif, 58, 34665, 4, 1, 74, little);
    put(&mut exif, 74, 2, 3, little);
    entry(&mut exif, 76, 40962, 3, 1, 12, little);
    entry(&mut exif, 88, 40963, 4, 1, 8, little);
    entry(&mut exif, 100, 37500, 7, 12, 116, little);
    exif[116..120].copy_from_slice(b"NOTE");
    put(&mut exif, 120, 4, 135, little);
    exif[124..128].copy_from_slice(&[0xfa, 0xce, 0x80, 0]);
    exif[128..135].copy_from_slice(b"Camera\0");
    exif[135..].copy_from_slice(&[0xde, 0xad, 0, 0xbe, 0xef, 0x80, 0xff, 0]);
    exif
}

fn jpeg_with_metadata(jpeg: &[u8], records: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = vec![0xff, 0xd8];
    for &(marker, payload) in records {
        out.extend_from_slice(&[0xff, marker]);
        out.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn overwrite_resized_jpeg_updates_geometry_without_relocating_opaque_metadata() {
    let source = Image::from_u8(12, 8, ChannelLayout::Rgb, vec![80; 12 * 8 * 3]).unwrap();
    let jpeg = photocraft_codecs::encode(&source, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let replacement = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![160; 3 * 2 * 3]).unwrap();
    let png = photocraft_codecs::encode(&replacement, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("resized.png", &png).unwrap().document;
    let xmp = concat!(
        "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>",
        "<r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/' xmlns:e='http://ns.adobe.com/exif/1.0/' ",
        "xmlns:f='urn:foreign' f:Exact = 'untouched &amp; opaque' t:Orientation='6' t:ImageWidth='12' e:PixelYDimension='8'>",
        "<t:ImageLength>8</t:ImageLength><e:PixelXDimension>12</e:PixelXDimension>",
        "<f:History><r:Seq><r:li>original history</r:li></r:Seq></f:History></r:Description></r:RDF>   "
    );
    let expected_xmp = xmp
        .replace("t:Orientation='6'", "t:Orientation='1'")
        .replace("t:ImageWidth='12'", "t:ImageWidth='3'")
        .replace("e:PixelYDimension='8'", "e:PixelYDimension='2'")
        .replace("<t:ImageLength>8", "<t:ImageLength>2")
        .replace("<e:PixelXDimension>12", "<e:PixelXDimension>3");
    let mut xmp_payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    xmp_payload.extend_from_slice(xmp.as_bytes());
    for little in [true, false] {
        let exif = overwrite_geometry_exif(little);
        let mut exif_payload = b"Exif\0\0".to_vec();
        exif_payload.extend_from_slice(&exif);
        let iptc = b"Photoshop 3.0\0opaque IPTC \xff\x80";
        let private = b"camera-private\0\xff\x91\0\x05";
        let original = jpeg_with_metadata(&jpeg, &[(0xe1, &exif_payload), (0xe1, &xmp_payload), (0xed, iptc), (0xef, private)]);
        let mut expected_exif = lightcraft_tiff::exif::upright(&exif).into_owned();
        for (at, size, value) in [(18, 2, 3u32), (30, 4, 2), (84, 2, 3), (96, 4, 2)] {
            let raw = if little { value.to_le_bytes() } else { value.to_be_bytes() };
            expected_exif[at..at + size].copy_from_slice(if little { &raw[..size] } else { &raw[4 - size..] });
        }
        let mut saved = original;
        for _ in 0..2 {
            saved = photocraft_io::export_over_original(&doc, &saved, 90).unwrap().bytes;
            let decoded = photocraft_codecs::decode(&saved).unwrap();
            assert_eq!(decoded.dimensions(), (3, 2), "encoded SOF must match the replacement");
            let embedded = lightcraft_meta::jpeg_segments(&saved);
            let got = embedded.exif.unwrap();
            assert_eq!(got, expected_exif, "every byte outside geometry stays at its original offset");
            assert_eq!(lightcraft_tiff::exif::orientation(&got), 1);
            let metadata = lightcraft_meta::read_exif(&got);
            assert_eq!((metadata.width, metadata.height), (Some(3), Some(2)));
            assert_eq!(metadata.make.as_deref(), Some("Camera"));
            let pointer: [u8; 4] = got[120..124].try_into().unwrap();
            let pointer = if little { u32::from_le_bytes(pointer) } else { u32::from_be_bytes(pointer) } as usize;
            assert_eq!(&got[pointer..pointer + 8], &exif[135..]);
            assert_eq!(embedded.xmp.as_deref(), Some(expected_xmp.as_str()));
            for (marker, payload) in [(0xed, iptc.as_slice()), (0xef, private.as_slice())] {
                let record = jpeg_with_metadata(&[0xff, 0xd8], &[(marker, payload)]);
                assert!(saved.windows(record.len() - 2).any(|bytes| bytes == &record[2..]));
            }
        }
    }
}

#[test]
fn overwrite_jpeg_refuses_unreadable_geometry_metadata() {
    let image = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![100; 18]).unwrap();
    let jpeg = photocraft_codecs::encode(&image, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let png = photocraft_codecs::encode(&image, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("replacement.png", &png).unwrap().document;
    let mut exif = overwrite_geometry_exif(true);
    exif[66..70].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut exif_payload = b"Exif\0\0".to_vec();
    exif_payload.extend_from_slice(&exif);
    let original = jpeg_with_metadata(&jpeg, &[(0xe1, &exif_payload)]);
    let before = original.clone();
    assert!(photocraft_io::export_over_original(&doc, &original, 90).is_err());
    assert_eq!(original, before);
    let packet = b"http://ns.adobe.com/xap/1.0/\0<broken>";
    let original = jpeg_with_metadata(&jpeg, &[(0xe1, packet)]);
    assert!(photocraft_io::export_over_original(&doc, &original, 90).is_err());
}

const EXTENDED_XMP_HEADER: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
// Independently generated with Python hashlib.md5, not the production GUID routine.
const EXTENDED_OLD_GUID: &str = "A4AB635057D3BDE49D8FDAD7B888BC32";
const EXTENDED_NEW_GUID: &str = "1C1A85B3C7F85F31F24FF5B1CCADE1C3";

fn extended_geometry_fixture() -> (String, String, Vec<Vec<u8>>) {
    let packet = format!(
        "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/' xmlns:e='http://ns.adobe.com/exif/1.0/' xmlns:f='urn:foreign' t:Orientation='6' e:PixelXDimension='12'><t:ImageLength>8</t:ImageLength><f:Data>{}</f:Data></r:Description></r:RDF>",
        "z".repeat(70000)
    );
    assert_eq!(packet.len(), 70292);
    let main = format!(
        "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:n='http://ns.adobe.com/xmp/note/' xmlns:f='urn:foreign' n:HasExtendedXMP='{EXTENDED_OLD_GUID}' f:UnrelatedGuid='{EXTENDED_OLD_GUID}'/></r:RDF>"
    );
    // Deliberately reverse two differently sized chunks to exercise reassembly by offset.
    let mut chunks = Vec::new();
    for (offset, data) in [(30000u32, &packet.as_bytes()[30000..]), (0, &packet.as_bytes()[..30000])] {
        let mut chunk = EXTENDED_XMP_HEADER.to_vec();
        chunk.extend_from_slice(EXTENDED_OLD_GUID.as_bytes());
        chunk.extend_from_slice(&(packet.len() as u32).to_be_bytes());
        chunk.extend_from_slice(&offset.to_be_bytes());
        chunk.extend_from_slice(data);
        chunks.push(chunk);
    }
    (packet, main, chunks)
}

#[test]
fn overwrite_resized_jpeg_rechunks_extended_xmp_with_coherent_content_guid() {
    let image = Image::from_u8(12, 8, ChannelLayout::Rgb, vec![80; 12 * 8 * 3]).unwrap();
    let jpeg = photocraft_codecs::encode(&image, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let replacement = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![160; 18]).unwrap();
    let png = photocraft_codecs::encode(&replacement, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("resized.png", &png).unwrap().document;
    let (packet, main, chunks) = extended_geometry_fixture();
    let expected = packet
        .replace("t:Orientation='6'", "t:Orientation='1'")
        .replace("e:PixelXDimension='12'", "e:PixelXDimension='3'")
        .replace("<t:ImageLength>8", "<t:ImageLength>2");
    let expected_main = main.replace(&format!("n:HasExtendedXMP='{EXTENDED_OLD_GUID}'"), &format!("n:HasExtendedXMP='{EXTENDED_NEW_GUID}'"));
    let mut main_payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    main_payload.extend_from_slice(main.as_bytes());
    let mut saved = jpeg_with_metadata(&jpeg, &[(0xe1, &main_payload), (0xe1, &chunks[0]), (0xe1, &chunks[1])]);
    for _ in 0..2 {
        saved = photocraft_io::export_over_original(&doc, &saved, 90).unwrap().bytes;
        assert_eq!(photocraft_codecs::decode(&saved).unwrap().dimensions(), (3, 2));
        assert_eq!(lightcraft_meta::jpeg_segments(&saved).xmp.as_deref(), Some(expected_main.as_str()));
        // Independently walk JPEG markers and reconstruct the Extended XMP; do not
        // use the production reassembler or GUID generator as the expected oracle.
        let mut reconstructed = Vec::new();
        let mut at = 2;
        let mut count = 0;
        while saved[at + 1] != 0xda {
            assert_eq!(saved[at], 0xff);
            let len = u16::from_be_bytes([saved[at + 2], saved[at + 3]]) as usize;
            assert!(len >= 2);
            if saved[at + 1] == 0xe1
                && let Some(payload) = saved[at + 4..at + 2 + len].strip_prefix(EXTENDED_XMP_HEADER)
            {
                assert_eq!(&payload[..32], EXTENDED_NEW_GUID.as_bytes());
                assert_eq!(u32::from_be_bytes(payload[32..36].try_into().unwrap()) as usize, expected.len());
                assert_eq!(u32::from_be_bytes(payload[36..40].try_into().unwrap()) as usize, reconstructed.len());
                reconstructed.extend_from_slice(&payload[40..]);
                count += 1;
            }
            at += 2 + len;
        }
        assert_eq!(count, 2);
        assert_eq!(reconstructed, expected.as_bytes(), "foreign text must remain byte-exact across rechunking");
    }
}

#[test]
fn overwrite_jpeg_rejects_incoherent_or_oversized_extended_xmp() {
    let image = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![100; 18]).unwrap();
    let jpeg = photocraft_codecs::encode(&image, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let png = photocraft_codecs::encode(&image, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("replacement.png", &png).unwrap().document;
    let (_, main, chunks) = extended_geometry_fixture();
    let mut main_payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    main_payload.extend_from_slice(main.as_bytes());
    let h = EXTENDED_XMP_HEADER.len();
    for case in 0..8 {
        let mut parts = chunks.clone();
        let mut main = main_payload.clone();
        match case {
            0 => {
                parts.pop();
            } // missing chunk
            1 => parts[0][h + 36..h + 40].copy_from_slice(&29999u32.to_be_bytes()), // overlap
            2 => parts[0][h + 36..h + 40].copy_from_slice(&30001u32.to_be_bytes()), // gap/range
            3 => parts[0][h + 32..h + 36].copy_from_slice(&u32::MAX.to_be_bytes()), // allocation budget
            4 => parts[0][h] = b'0',                                                // conflicting chunk GUIDs
            5 => {
                let last = parts[0].len() - 1;
                parts[0][last] = b'!';
            } // content/GUID mismatch
            6 => {
                main.clear();
                main.extend_from_slice(b"http://ns.adobe.com/xap/1.0/\0<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/>");
            } // missing link
            7 => {
                parts.push(parts[0].clone());
            }        // repeated chunk
            _ => unreachable!(),
        }
        let mut records = vec![(0xe1, main.as_slice())];
        records.extend(parts.iter().map(|part| (0xe1, part.as_slice())));
        let original = jpeg_with_metadata(&jpeg, &records);
        assert!(photocraft_io::export_over_original(&doc, &original, 90).is_err(), "case {case}");
    }
}

#[test]
fn overwrite_jpeg_accepts_xml_semantic_extended_guid_links_and_rejects_missing_chunks() {
    let image = Image::from_u8(3, 2, ChannelLayout::Rgb, vec![100; 18]).unwrap();
    let jpeg = photocraft_codecs::encode(&image, Format::Jpeg, &EncodeOptions::default()).unwrap();
    let png = photocraft_codecs::encode(&image, Format::Png, &EncodeOptions::default()).unwrap();
    let doc = photocraft_io::import("replacement.png", &png).unwrap().document;
    let (_, main, chunks) = extended_geometry_fixture();
    let encoded_guid = EXTENDED_OLD_GUID.replacen('A', "&#x41;", 1);
    let escaped = main
        .replace("http://ns.adobe.com/xmp/note/", "http://ns.adobe.com/xmp/note&#x2F;")
        .replace(&format!("n:HasExtendedXMP='{EXTENDED_OLD_GUID}'"), &format!("n:HasExtendedXMP='{encoded_guid}'"));
    let no_attribute = main.replace(&format!(" n:HasExtendedXMP='{EXTENDED_OLD_GUID}'"), "");
    let cdata = no_attribute.replace("/></r:RDF>", &format!("><n:HasExtendedXMP><![CDATA[{EXTENDED_OLD_GUID}]]></n:HasExtendedXMP></r:Description></r:RDF>"));
    let entity_text = no_attribute.replace("/></r:RDF>", &format!("><n:HasExtendedXMP>{encoded_guid}</n:HasExtendedXMP></r:Description></r:RDF>"));
    for (main, encoded_link) in [(escaped, encoded_guid.clone()), (cdata, format!("<![CDATA[{EXTENDED_OLD_GUID}]]>")), (entity_text, encoded_guid)] {
        let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        payload.extend_from_slice(main.as_bytes());
        let original = jpeg_with_metadata(&jpeg, &[(0xe1, &payload), (0xe1, &chunks[0]), (0xe1, &chunks[1])]);
        let saved = photocraft_io::export_over_original(&doc, &original, 90).unwrap();
        assert_eq!(lightcraft_meta::jpeg_segments(&saved.bytes).xmp.as_deref(), Some(main.replace(&encoded_link, EXTENDED_NEW_GUID).as_str()));
        let dangling = jpeg_with_metadata(&jpeg, &[(0xe1, &payload)]);
        let error = photocraft_io::export_over_original(&doc, &dangling, 90).unwrap_err().to_string();
        assert!(error.contains("missing Extended XMP chunks"), "{error}");
    }
    let foreign = b"http://ns.adobe.com/xap/1.0/\0<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:n='urn:foreign' n:HasExtendedXMP='foreign text'/></r:RDF>";
    let original = jpeg_with_metadata(&jpeg, &[(0xe1, foreign)]);
    let saved = photocraft_io::export_over_original(&doc, &original, 90).unwrap();
    assert_eq!(lightcraft_meta::jpeg_segments(&saved.bytes).xmp, lightcraft_meta::jpeg_segments(&original).xmp);
}

fn maker_note_tiff(little: bool, big: bool, sample: SampleType) -> Vec<u8> {
    fn put(bytes: &mut [u8], at: usize, n: usize, value: u64, little: bool) {
        let data = if little { value.to_le_bytes() } else { value.to_be_bytes() };
        bytes[at..at + n].copy_from_slice(if little { &data[..n] } else { &data[8 - n..] });
    }
    let mut bytes = vec![0; 512];
    bytes[..2].copy_from_slice(if little { b"II" } else { b"MM" });
    put(&mut bytes, 2, 2, if big { 43 } else { 42 }, little);
    if big {
        put(&mut bytes, 4, 2, 8, little);
    }
    put(&mut bytes, if big { 8 } else { 4 }, if big { 8 } else { 4 }, if big { 16 } else { 8 }, little);
    let (root, count_size, entry_size, offset_size) = if big { (16, 8, 20, 8) } else { (8, 2, 12, 4) };
    let bits = sample.bytes() * 8;
    let fields = [
        (256u16, 4u16, 3u64),
        (257, 4, 2),
        (258, 3, bits as u64),
        (259, 3, 1),
        (262, 3, 1),
        (273, 4, 512),
        (277, 3, 1),
        (278, 4, 2),
        (279, 4, (6 * sample.bytes()) as u64),
        (339, 3, if sample == SampleType::F32 { 3 } else { 1 }),
        (34665, 4, 300),
    ];
    put(&mut bytes, root, count_size, fields.len() as u64, little);
    for (index, (tag, typ, value)) in fields.into_iter().enumerate() {
        let at = root + count_size + index * entry_size;
        put(&mut bytes, at, 2, tag.into(), little);
        put(&mut bytes, at + 2, 2, typ.into(), little);
        put(&mut bytes, at + 4, if big { 8 } else { 4 }, 1, little);
        put(&mut bytes, at + entry_size - offset_size, if typ == 3 { 2 } else { 4 }, value, little);
    }
    put(&mut bytes, 300, count_size, 1, little);
    let entry = 300 + count_size;
    put(&mut bytes, entry, 2, 37500, little);
    put(&mut bytes, entry + 2, 2, 7, little);
    put(&mut bytes, entry + 4, if big { 8 } else { 4 }, 64, little);
    put(&mut bytes, entry + entry_size - offset_size, offset_size, 400, little);
    bytes[400..464].fill(0x91);
    put(&mut bytes, 408, 4, 480, little);
    put(&mut bytes, 412, 4, 80, little);
    bytes[480..496].copy_from_slice(b"opaque-reference");
    for _ in 0..6 {
        match sample {
            SampleType::U8 => bytes.push(64),
            SampleType::U16 => bytes.extend_from_slice(&if little { 16384u16.to_le_bytes() } else { 16384u16.to_be_bytes() }),
            SampleType::F32 => bytes.extend_from_slice(&if little { 0.25f32.to_le_bytes() } else { 0.25f32.to_be_bytes() }),
            _ => unreachable!(),
        }
    }
    bytes
}

#[test]
fn overwrite_tiff_refuses_opaque_makernotes_in_both_orders_variants_and_depths() {
    for little in [true, false] {
        for big in [false, true] {
            for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
                let original = maker_note_tiff(little, big, sample);
                let before = original.clone();
                let replacement = Image::from_normalized(2, 3, ChannelLayout::Gray, sample, &[0.5; 6]).unwrap();
                let encoded = photocraft_codecs::encode(&replacement, Format::Tiff, &EncodeOptions::default()).unwrap();
                let doc = photocraft_io::import("replacement.tiff", &encoded).unwrap().document;
                let err = photocraft_io::export_over_original(&doc, &original, 90).unwrap_err().to_string();
                assert!(err.contains("cannot be safely rewritten"), "{little}/{big}/{sample:?}: {err}");
                assert_eq!(original, before);
            }
        }
    }
}
