#[cfg(target_os = "linux")]
pub(super) fn wayland_clipboard_image() -> Option<(u32, u32, Vec<u8>)> {
    use std::io::Read;
    use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents, get_mime_types_ordered};
    const MAX_BYTES: u64 = 4 * 16384 * 16384;
    let offers = get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified).ok()?;
    image_from_offers(&offers, |mime| {
        let (pipe, _) = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Specific(mime)).ok()?;
        let mut bytes = Vec::new();
        pipe.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= MAX_BYTES).then_some(bytes)
    })
}

#[cfg(any(target_os = "linux", test))]
fn image_from_offers(offers: &[String], mut fetch: impl FnMut(&str) -> Option<Vec<u8>>) -> Option<(u32, u32, Vec<u8>)> {
    offers.iter().filter(|mime| mime.starts_with("image/")).find_map(|mime| {
        let bytes = fetch(mime)?;
        // TGA has no magic number, so its MIME type must also identify it.
        if photocraft_codecs::detect(&bytes)? == photocraft_codecs::Format::Tga && !mime.contains("tga") {
            return None;
        }
        let image = photocraft_codecs::decode(&bytes).ok()?;
        Some((image.width(), image.height(), image.to_rgba8()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};

    #[test]
    fn clipboard_images_decode_non_png_offers_without_fetching_text() {
        for (mime, format) in
            [("image/jpeg", photocraft_codecs::Format::Jpeg), ("image/bmp", photocraft_codecs::Format::Bmp), ("image/webp", photocraft_codecs::Format::WebP)]
        {
            let image = Image::from_u8(3, 2, ChannelLayout::Rgba, [255, 0, 0, 255].repeat(6)).unwrap();
            let encoded = photocraft_codecs::encode(&image, format, &EncodeOptions::default()).unwrap();
            let offers = ["text/plain".to_string(), "image/png".to_string(), mime.to_string()];
            let result = image_from_offers(&offers, |offered| {
                assert_ne!(offered, "text/plain");
                Some(if offered == mime { encoded.clone() } else { vec![0; 20] })
            })
            .unwrap();
            assert_eq!((result.0, result.1), (3, 2));
            assert_eq!(result.2.len(), 24);
            if format != photocraft_codecs::Format::Jpeg {
                assert_eq!(result.2, [255, 0, 0, 255].repeat(6));
            }
        }
    }

    #[test]
    fn clipboard_tga_requires_its_mime_type() {
        assert_eq!(image_from_offers(&["image/x-tga".into()], |_| Some(crate::services::tests::tga_1x1())), Some((1, 1, vec![255, 0, 0, 255])));
        assert!(image_from_offers(&["image/unknown".into()], |_| Some(crate::services::tests::tga_1x1())).is_none());
        assert!(image_from_offers(&["image/png".into()], |_| Some(vec![0; 8])).is_none());
        assert!(image_from_offers(&[], |_| panic!("no offers to fetch")).is_none());
    }
}
