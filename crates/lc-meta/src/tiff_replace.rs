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
    // Compressed pixel byte order belongs to the encoder, not to the previous TIFF header.
    TiffWriter::new(target.order, target.bigtiff).write(&[root])
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
    fn overwrite_tiff_keeps_exif_gps_private_tags_and_new_strips() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for big in [false, true] {
                let exif = IfdBuilder::new().with(36867, Value::Ascii("2026:10:10 12:34:56".into())).with(40962, Value::Long(vec![99]));
                let gps = IfdBuilder::new().with(1, Value::Ascii("N".into()));
                let mut old = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Long(vec![99])).with(65001, Value::Undefined(vec![4, 5, 6]));
                old.set(315, Value::Ascii("original artist".into()));
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
                let encoded = TiffWriter::default().write(&[new]).unwrap();
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
                assert_eq!(root.gps.as_ref().unwrap().string(1).as_deref(), Some("N"));
                let at = root.u64(tags::STRIP_OFFSETS).unwrap() as usize;
                assert_eq!(&bytes[at..at + 2], &[10, 20]);
            }
        }
    }
}
