//! Preserve a TIFF's metadata IFD tree when replacing its raster strips.
//! Uses the existing TIFF reader/writer to rebase EXIF/GPS/Interop pointers and values.

use lightcraft_tiff::{Ifd, IfdBuilder, ImageData, Tiff, TiffError, TiffWriter, Value, tags};

/// Carry original metadata into an already encoded flat TIFF. Pixel storage tags come from
/// `encoded`; EXIF/GPS, IPTC and private metadata stay with the photo. Old layers and image
/// thumbnails are discarded, because they describe the previous pixels.
pub fn preserve_tiff_metadata(original: &[u8], encoded: &[u8]) -> lightcraft_tiff::Result<Vec<u8>> {
    let source = Tiff::parse(original)?;
    let target = Tiff::parse(encoded)?;
    let old = source.ifds.first().ok_or_else(|| TiffError::Invalid("original has no IFD".into()))?;
    let new = target.ifds.first().ok_or_else(|| TiffError::Invalid("replacement has no IFD".into()))?;
    // Unknown camera notes may point anywhere in the original allocation. Relocating them
    // corrupts metadata; retaining the old allocation retains cropped/redacted image pixels.
    // Until the vendor's relocation can be proven safe, leave the original untouched.
    if has_maker_note(old) {
        return Err(TiffError::Invalid(
            "This TIFF's camera metadata uses offsets that cannot be safely rewritten. The original was not changed. Save an edited copy instead."
                .into(),
        ));
    }
    let mut root = metadata(old);
    for entry in &new.entries {
        if PIXEL_TAGS.contains(&entry.tag) {
            root.entries.insert(entry.tag, entry.value.clone());
        } else if !matches!(entry.tag, tags::EXIF_IFD | tags::GPS_IFD | tags::INTEROP_IFD) {
            root.entries.entry(entry.tag).or_insert_with(|| entry.value.clone());
        }
    }
    let offsets = new.u64s(tags::STRIP_OFFSETS).ok_or_else(|| TiffError::Invalid("replacement has no strips".into()))?;
    let counts = new.u64s(tags::STRIP_BYTE_COUNTS).ok_or_else(|| TiffError::Invalid("replacement has no strip sizes".into()))?;
    if offsets.len() != counts.len() {
        return Err(TiffError::Invalid("strip count mismatch".into()));
    }
    let strips = offsets
        .into_iter()
        .zip(counts)
        .map(|(start, len)| {
            let end = start.checked_add(len).ok_or(TiffError::Limit("strip range"))?;
            let start = usize::try_from(start).map_err(|_| TiffError::Limit("strip offset"))?;
            let end = usize::try_from(end).map_err(|_| TiffError::Limit("strip end"))?;
            encoded.get(start..end).map(<[u8]>::to_vec).ok_or_else(|| TiffError::Invalid("strip outside file".into()))
        })
        .collect::<lightcraft_tiff::Result<Vec<_>>>()?;
    root.image = Some(ImageData::Strips { rows_per_strip: new.u32(tags::ROWS_PER_STRIP).unwrap_or(u32::MAX), strips });
    root.entries.insert(tags::ORIENTATION, Value::Short(vec![1]));
    if let Some(exif) = root.children.get_mut(&tags::EXIF_IFD) {
        for (tag, dimension) in [(40962, tags::IMAGE_WIDTH), (40963, tags::IMAGE_LENGTH)] {
            if let Some(value) = exif.entries.get_mut(&tag) {
                *value = Value::Long(vec![new.u32(dimension).unwrap_or(0)]);
            }
        }
    }
    if let Some(value) = root.entries.get_mut(&tags::XMP) {
        let bytes = value.as_bytes().ok_or_else(|| TiffError::Invalid("non-byte XMP field".into()))?;
        // Some writers terminate tag 700 with NULs. Keep that suffix byte for byte.
        let text_len = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
        let packet = std::str::from_utf8(&bytes[..text_len]).map_err(|_| TiffError::Invalid("non-UTF-8 XMP packet".into()))?;
        let packet = crate::normalize_xmp_geometry(packet, new.u32(tags::IMAGE_WIDTH).unwrap_or(0), new.u32(tags::IMAGE_LENGTH).unwrap_or(0))
            .map_err(|e| TiffError::Invalid(e.to_string()))?;
        let mut fixed = packet.as_bytes().to_vec();
        fixed.extend_from_slice(&bytes[text_len..]);
        *value = match value {
            Value::Byte(_) => Value::Byte(fixed),
            Value::Undefined(_) => Value::Undefined(fixed),
            Value::Ascii(_) => Value::Ascii(packet),
            Value::AsciiBytes(_) => Value::AsciiBytes(fixed),
            _ => return Err(TiffError::Invalid("non-byte XMP field".into())),
        };
    }
    // Compressed pixel byte order belongs to the encoder, not to the previous TIFF header.
    TiffWriter::new(target.order, target.bigtiff).write(&[root])
}

fn has_maker_note(ifd: &Ifd) -> bool {
    ifd.get(tags::MAKER_NOTE).is_some() || [&ifd.exif, &ifd.gps, &ifd.interop].into_iter().flatten().any(|child| has_maker_note(child))
}

// TIFF 6.0 raster storage, sample interpretation, old previews and Photoshop layers.
const PIXEL_TAGS: &[u16] = &[
    254, 255, 256, 257, 258, 259, 262, 263, 264, 265, 266, 273, 274, 277, 278, 279, 280, 281, 284, 288, 289, 290, 291, 292, 293, 301, 317, 320, 322,
    323, 324, 325, 330, 338, 339, 340, 341, 347, 512, 513, 514, 515, 517, 518, 519, 520, 521, 529, 530, 531, 532, 37724,
];

fn metadata(ifd: &Ifd) -> IfdBuilder {
    let mut out = IfdBuilder::new();
    for e in &ifd.entries {
        if !PIXEL_TAGS.contains(&e.tag) && !matches!(e.tag, tags::EXIF_IFD | tags::GPS_IFD | tags::INTEROP_IFD) {
            let value = if e.tag == 34377 {
                match &e.value {
                    Value::Byte(bytes) => Value::Byte(without_photoshop_previews(bytes)),
                    Value::Undefined(bytes) => Value::Undefined(without_photoshop_previews(bytes)),
                    other => other.clone(),
                }
            } else {
                e.value.clone()
            };
            out.entries.insert(e.tag, value);
        }
    }
    for (tag, child) in [(tags::EXIF_IFD, &ifd.exif), (tags::GPS_IFD, &ifd.gps), (tags::INTEROP_IFD, &ifd.interop)] {
        if let Some(child) = child {
            out.children.insert(tag, metadata(child));
        }
    }
    out
}

// Resource 34377 also holds IPTC, ICC and XMP. Remove stale previews/layer bookkeeping
// from its framed resource stream, preserving other records and any opaque trailing bytes.
fn without_photoshop_previews(mut bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while bytes.len() >= 12 {
        let little = &bytes[..4] == b"MIB8";
        if !little && !matches!(&bytes[..4], b"8BIM" | b"MeSa" | b"AgHg" | b"PHUT" | b"DCSR") {
            break;
        }
        let id = if little { u16::from_le_bytes([bytes[4], bytes[5]]) } else { u16::from_be_bytes([bytes[4], bytes[5]]) };
        let size_at = 6 + (1 + bytes[6] as usize).next_multiple_of(2);
        let Some(size_bytes) = bytes.get(size_at..size_at + 4).and_then(|b| b.try_into().ok()) else { break };
        let size = if little { u32::from_le_bytes(size_bytes) } else { u32::from_be_bytes(size_bytes) } as usize;
        let Some(end) = size.checked_add(size % 2).and_then(|size| (size_at + 4).checked_add(size)).filter(|end| *end <= bytes.len()) else { break };
        if !matches!(id, 1024 | 1026 | 1033 | 1036 | 1072) {
            out.extend_from_slice(&bytes[..end]);
        }
        bytes = &bytes[end..];
    }
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_tiff::ByteOrder;

    #[test]
    fn overwrite_tiff_refuses_unproven_camera_note_relocation() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for big in [false, true] {
                for note in [vec![1, 2], vec![0x91; 64], b"Nikon\0\x02\x10\0\0opaque-offsets".to_vec()] {
                    let mut old = IfdBuilder::new();
                    old.set_child(tags::EXIF_IFD, IfdBuilder::new().with(tags::MAKER_NOTE, Value::Undefined(note)));
                    old.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![b"original-sensitive-pixels".to_vec()] });
                    let original = TiffWriter::new(order, big).write(&[old]).unwrap();
                    let before = original.clone();
                    let mut new = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Long(vec![1])).with(tags::IMAGE_LENGTH, Value::Long(vec![1]));
                    new.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![vec![0]] });
                    let encoded = TiffWriter::default().write(&[new]).unwrap();
                    let err = preserve_tiff_metadata(&original, &encoded).unwrap_err().to_string();
                    assert!(err.contains("cannot be safely rewritten"), "{err}");
                    assert_eq!(original, before);
                }
            }
        }
    }

    #[test]
    fn overwrite_tiff_refuses_unreadable_xmp_before_returning_replacement() {
        let mut replacement = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Long(vec![2])).with(tags::IMAGE_LENGTH, Value::Long(vec![1]));
        replacement.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![vec![10, 20]] });
        let encoded = TiffWriter::default().write(&[replacement]).unwrap();
        for payload in [b"<broken>".as_slice(), &[0xff, 0xfe]] {
            let old = IfdBuilder::new().with(tags::XMP, Value::Byte(payload.to_vec()));
            let original = TiffWriter::default().write(&[old]).unwrap();
            assert!(preserve_tiff_metadata(&original, &encoded).is_err());
        }
    }

    #[test]
    fn overwrite_tiff_keeps_exif_gps_private_tags_and_new_strips() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for big in [false, true] {
                let exif = IfdBuilder::new()
                    .with(36867, Value::Ascii("2026:10:10 12:34:56".into()))
                    .with(40962, Value::Long(vec![99]))
                    .with(40963, Value::Short(vec![55]));
                let gps = IfdBuilder::new().with(1, Value::Ascii("N".into()));
                let mut old = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Long(vec![99])).with(65001, Value::Undefined(vec![4, 5, 6]));
                old.set(315, Value::Ascii("original artist".into()));
                old.set(tags::ORIENTATION, Value::Short(vec![6]));
                old.set(tags::IMAGE_LENGTH, Value::Long(vec![55]));
                let raw_ascii = b"Copyright \xa9\0second string\0\0";
                old.set(65002, Value::AsciiBytes(raw_ascii.to_vec()));
                old.set(33432, Value::AsciiBytes(raw_ascii.to_vec()));
                old.set(65003, Value::AsciiBytes(vec![0xe9, 0]));
                let xmp = concat!(
                    "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>",
                    "<r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/' xmlns:e='http://ns.adobe.com/exif/1.0/' ",
                    "xmlns:f='urn:foreign' t:Orientation='6' t:ImageWidth='99' e:PixelYDimension='55' f:Exact = 'a &amp; b'>",
                    "<t:Orientation>6</t:Orientation><t:ImageLength>55</t:ImageLength><e:PixelXDimension>99</e:PixelXDimension>",
                    "<f:History><r:Seq><r:li>camera original</r:li></r:Seq></f:History>",
                    "</r:Description></r:RDF>   \0\0"
                );
                old.set(tags::XMP, Value::Byte(xmp.as_bytes().to_vec()));
                let resource = |id: u16, data: &[u8]| {
                    let mut bytes = b"8BIM".to_vec();
                    bytes.extend_from_slice(&id.to_be_bytes());
                    bytes.extend_from_slice(&[0, 0]);
                    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
                    bytes.extend_from_slice(data);
                    if data.len() % 2 == 1 {
                        bytes.push(0);
                    }
                    bytes
                };
                let iptc = resource(1028, b"original IPTC");
                let mut resources = iptc.clone();
                resources.extend(resource(1033, b"old preview"));
                resources.extend(resource(1036, b"old preview"));
                resources.extend(resource(1024, b"old layer state"));
                old.set(34377, Value::Undefined(resources));
                old.set_child(tags::EXIF_IFD, exif).set_child(tags::GPS_IFD, gps);
                let original = TiffWriter::new(order, big).write(&[old]).unwrap();
                let mut new = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Long(vec![2])).with(tags::IMAGE_LENGTH, Value::Long(vec![1]));
                new.set(315, Value::Ascii("encoder artist".into()));
                new.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![vec![10, 20]] });
                let encoded = TiffWriter::new(order, big).write(&[new]).unwrap();
                let bytes = preserve_tiff_metadata(&original, &encoded).unwrap();
                let back = Tiff::parse(&bytes).unwrap();
                let root = &back.ifds[0];
                assert_eq!(root.string(315).as_deref(), Some("original artist"));
                assert_eq!(root.bytes(34377), Some(iptc.as_slice()));
                assert_eq!(root.u32(tags::IMAGE_WIDTH), Some(2));
                assert_eq!(root.u16(tags::ORIENTATION), Some(1));
                assert_eq!(root.bytes(65001), Some(&[4, 5, 6][..]));
                assert_eq!(root.exif.as_ref().unwrap().string(36867).as_deref(), Some("2026:10:10 12:34:56"));
                assert_eq!(root.exif.as_ref().unwrap().u32(40962), Some(2));
                assert_eq!(root.exif.as_ref().unwrap().u32(40963), Some(1));
                assert_eq!(root.u32(tags::IMAGE_LENGTH), Some(1));
                assert_eq!(back.order, order);
                assert_eq!(back.bigtiff, big);
                for (tag, expected) in [(33432, raw_ascii.as_slice()), (65002, raw_ascii.as_slice()), (65003, &[0xe9, 0][..])] {
                    let entry = root.get(tag).unwrap();
                    assert_eq!(entry.field_type(), lightcraft_tiff::FieldType::Ascii);
                    assert_eq!(entry.count(), expected.len());
                    assert_eq!(root.bytes(tag), Some(expected));
                }
                let expected_xmp = xmp
                    .replace("t:Orientation='6'", "t:Orientation='1'")
                    .replace("t:ImageWidth='99'", "t:ImageWidth='2'")
                    .replace("e:PixelYDimension='55'", "e:PixelYDimension='1'")
                    .replace("<t:Orientation>6", "<t:Orientation>1")
                    .replace("<t:ImageLength>55", "<t:ImageLength>1")
                    .replace("<e:PixelXDimension>99", "<e:PixelXDimension>2");
                assert_eq!(root.bytes(tags::XMP), Some(expected_xmp.as_bytes()));
                let twice = preserve_tiff_metadata(&bytes, &encoded).unwrap();
                let twice = Tiff::parse(&twice).unwrap();
                assert_eq!(twice.ifds[0].bytes(tags::XMP), Some(expected_xmp.as_bytes()));
                assert_eq!(twice.ifds[0].bytes(65002), Some(raw_ascii.as_slice()));
                assert_eq!(root.gps.as_ref().unwrap().string(1).as_deref(), Some("N"));
                let at = root.u64(tags::STRIP_OFFSETS).unwrap() as usize;
                assert_eq!(&bytes[at..at + 2], &[10, 20]);
            }
        }
    }
}
