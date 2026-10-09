//! Read TIFF metadata without cloning the sensor payload into an EXIF buffer.
use crate::Metadata;
use lightcraft_tiff::{Tiff, tags};

/// Metadata and the embedded XMP packet. TIFF raws share one parsed directory tree; other
/// containers retain the metadata library's existing extraction and precedence rules.
pub fn metadata_with_xmp(bytes: &[u8]) -> (Metadata, Option<String>) {
    if let Ok(tiff) = Tiff::parse(bytes) {
        let xmp = tiff.ifds.first().and_then(|ifd| ifd.bytes(tags::XMP)).map(|b| String::from_utf8_lossy(b).into_owned());
        return (lightcraft_meta::from_tiff(&tiff), xmp);
    }
    (lightcraft_meta::extract(bytes), lightcraft_meta::embedded(bytes).xmp)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod library_scale_tests {
    use super::*;
    use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value};

    #[test]
    fn metadata_and_xmp_match_the_container_extractor() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut root = IfdBuilder::new();
            root.set(tags::MAKE, Value::Ascii("Sony".into()));
            root.set(tags::ORIENTATION, Value::Short(vec![6]));
            root.set(tags::RATING, Value::Short(vec![1]));
            root.set(tags::IMAGE_DESCRIPTION, Value::Ascii("EXIF caption".into()));
            // XMP wins for editable fields; EXIF retains orientation and camera facts.
            let xmp = lightcraft_meta::write_xmp(
                &Metadata {
                    caption: Some("Conference speaker".into()),
                    rating: Some(4),
                    orientation: Some(crate::Orientation::Rotate270),
                    keywords: vec!["Event".into()],
                    ..Default::default()
                },
                None,
            );
            root.set(tags::XMP, Value::Byte(xmp.as_bytes().to_vec()));
            let mut bytes = TiffWriter { order, bigtiff: false }.write(&[root]).unwrap();
            bytes.resize(16 << 20, 0);
            let (actual, packet) = metadata_with_xmp(&bytes);
            assert_eq!(actual, lightcraft_meta::extract(&bytes));
            assert_eq!(packet, Some(xmp));
        }
        assert_eq!(metadata_with_xmp(b"not an image"), (Metadata::default(), None));
    }
}
