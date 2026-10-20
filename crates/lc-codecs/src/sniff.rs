//! Format detection from leading bytes.

pub use photocraft_codecs::{PHOTO_EXTENSIONS, RAW_EXTENSIONS, is_readable_extension, raw_from_extension, readable_extensions, writable_extensions};
use serde::{Deserialize, Serialize};

/// Image container formats LightCraft recognises.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    Jpeg,
    Png,
    /// Ordinary (non-camera-raw) TIFF, incl. BigTIFF.
    Tiff,
    WebP,
    Avif,
    /// HEIC / HEIF (ISO-BMFF with HEVC).
    Heif,
    /// JPEG XL (bare codestream or ISO-BMFF container).
    Jxl,
    Gif,
    Bmp,
    Tga,
    Ico,
    Pnm,
    Qoi,
    OpenExr,
    Hdr,
    /// Photoshop PSD / PSB (merged composite).
    Psd,
    /// TIFF-structured camera raw (DNG, CR2, NEF, ARW, PEF, ORF, RW2, SRW, 3FR, IIQ, ERF, …):
    /// route to `lightcraft-raw`.
    RawTiffLike,
    /// Non-TIFF camera raw containers (CR3, RAF, CRW, MRW, X3F): route to `lightcraft-raw`.
    RawOther,
}

impl Format {
    /// Canonical lowercase file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Tiff => "tif",
            Format::WebP => "webp",
            Format::Avif => "avif",
            Format::Heif => "heic",
            Format::Jxl => "jxl",
            Format::Gif => "gif",
            Format::Bmp => "bmp",
            Format::Tga => "tga",
            Format::Ico => "ico",
            Format::Pnm => "pnm",
            Format::Qoi => "qoi",
            Format::OpenExr => "exr",
            Format::Hdr => "hdr",
            Format::Psd => "psd",
            Format::RawTiffLike => "dng",
            Format::RawOther => "raw",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
            Format::Tiff => "image/tiff",
            Format::WebP => "image/webp",
            Format::Avif => "image/avif",
            Format::Heif => "image/heic",
            Format::Jxl => "image/jxl",
            Format::Gif => "image/gif",
            Format::Bmp => "image/bmp",
            Format::Tga => "image/x-tga",
            Format::Ico => "image/x-icon",
            Format::Pnm => "image/x-portable-anymap",
            Format::Qoi => "image/qoi",
            Format::OpenExr => "image/x-exr",
            Format::Hdr => "image/vnd.radiance",
            Format::Psd => "image/vnd.adobe.photoshop",
            Format::RawTiffLike | Format::RawOther => "image/x-raw",
        }
    }

    /// Whether [`crate::decode`] can decode this format in this build.
    pub fn can_decode(self) -> bool {
        self.raster().is_some_and(|f| f.caps().read)
    }

    /// Shared raster format, or `None` for a camera-raw container.
    pub fn raster(self) -> Option<photocraft_codecs::Format> {
        use photocraft_codecs::Format as F;
        Some(match self {
            Self::Jpeg => F::Jpeg,
            Self::Png => F::Png,
            Self::Tiff => F::Tiff,
            Self::WebP => F::WebP,
            Self::Avif => F::Avif,
            Self::Heif => F::Heif,
            Self::Jxl => F::Jxl,
            Self::Gif => F::Gif,
            Self::Bmp => F::Bmp,
            Self::Psd => F::Psd,
            Self::Tga => F::Tga,
            Self::Ico => F::Ico,
            Self::Pnm => F::Pnm,
            Self::Qoi => F::Qoi,
            Self::OpenExr => F::OpenExr,
            Self::Hdr => F::Hdr,
            Self::RawTiffLike | Self::RawOther => return None,
        })
    }

    /// Camera raw formats (decode with `lightcraft-raw`).
    pub fn is_raw(self) -> bool {
        matches!(self, Format::RawTiffLike | Format::RawOther)
    }
}

impl From<photocraft_codecs::Format> for Format {
    fn from(format: photocraft_codecs::Format) -> Self {
        use photocraft_codecs::Format as F;
        match format {
            F::Jpeg => Self::Jpeg,
            F::Png => Self::Png,
            F::Tiff => Self::Tiff,
            F::WebP => Self::WebP,
            F::Avif => Self::Avif,
            F::Heif => Self::Heif,
            F::Jxl => Self::Jxl,
            F::Gif => Self::Gif,
            F::Bmp => Self::Bmp,
            F::Psd => Self::Psd,
            F::Tga => Self::Tga,
            F::Ico => Self::Ico,
            F::Pnm => Self::Pnm,
            F::Qoi => Self::Qoi,
            F::OpenExr => Self::OpenExr,
            F::Hdr => Self::Hdr,
        }
    }
}

/// Identify a shared raster format or route camera raw data to `lightcraft-raw`.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    use photocraft_codecs::DetectedFormat;
    Some(match photocraft_codecs::identify(bytes)? {
        DetectedFormat::Raster(f) => f.into(),
        DetectedFormat::RawTiffLike => Format::RawTiffLike,
        DetectedFormat::RawOther => Format::RawOther,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_adapter_and_shared_capabilities_agree() {
        for shared in photocraft_codecs::Format::ALL {
            let adapter = Format::from(shared);
            assert_eq!(adapter.raster(), Some(shared));
            assert_eq!(adapter.can_decode(), shared.caps().read);
        }
        assert!(Format::RawTiffLike.is_raw());
        assert!(Format::RawOther.is_raw());
        assert_eq!(Format::RawOther.raster(), None);
        assert!(!Format::RawOther.can_decode());
        assert_eq!(sniff(&[0xff, 0x0a]), Some(Format::Jxl));
        assert_eq!(sniff(b"qoif"), Some(Format::Qoi));
        assert_eq!(sniff(b"#?RADIANCE"), Some(Format::Hdr));
    }

    #[test]
    fn magic_numbers() {
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0\0\x10JFIF"), Some(Format::Jpeg));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(sniff(b"GIF89a......"), Some(Format::Gif));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(sniff(b"\0\0\0\x1cftypavif\0\0\0\0avifmif1miaf"), Some(Format::Avif));
        assert_eq!(sniff(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"), Some(Format::Heif));
        assert_eq!(sniff(b"\0\0\0\x18ftypcrx \0\0\0\x01crx isom"), Some(Format::RawOther));
        assert_eq!(sniff(&[0xFF, 0x0A, 0xFA, 0x7F]), Some(Format::Jxl));
        assert_eq!(sniff(b"8BPS\0\x01\0\0\0\0\0\0"), Some(Format::Psd));
        assert_eq!(sniff(b"FUJIFILMCCD-RAW 0201"), Some(Format::RawOther));
        assert_eq!(sniff(b"II*\0\x08\0\0\0\0\0"), Some(Format::Tiff));
        assert_eq!(sniff(b"II*\0\x10\0\0\0CR\x02\0"), Some(Format::RawTiffLike));
        assert_eq!(sniff(b"IIRO\x08\0\0\0"), Some(Format::RawTiffLike));
        assert_eq!(sniff(b"nope"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn dng_detected() {
        // II*, IFD at 8 with a single DNGVersion entry.
        let mut b = b"II*\0\x08\0\0\0".to_vec();
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&0xC612u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&[1, 4, 0, 0]);
        b.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(sniff(&b), Some(Format::RawTiffLike));
    }

    #[test]
    fn sony_linear_ycbcr_subifd_is_raw_but_ordinary_tiff_is_not() {
        fn fixture(compression: u32, pattern: bool) -> Vec<u8> {
            let mut b = b"II*\0\x08\0\0\0".to_vec();
            let entry = |b: &mut Vec<u8>, tag: u16, typ: u16, count: u32, value: u32| {
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&typ.to_le_bytes());
                b.extend_from_slice(&count.to_le_bytes());
                b.extend_from_slice(&value.to_le_bytes());
            };
            b.extend_from_slice(&2u16.to_le_bytes());
            entry(&mut b, 0x010f, 2, 5, 38);
            entry(&mut b, 0x014a, 4, 1, 44);
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(b"SONY\0\0");
            b.extend_from_slice(&(if pattern { 4u16 } else { 3 }).to_le_bytes());
            entry(&mut b, 0x0103, 3, 1, compression);
            entry(&mut b, 0x0106, 3, 1, 6);
            entry(&mut b, 0x0115, 3, 1, 3);
            if pattern {
                entry(&mut b, 0x828e, 1, 4, u32::MAX);
            }
            b.extend_from_slice(&0u32.to_le_bytes());
            b
        }
        let mut raw = fixture(7, true);
        assert_eq!(sniff(&raw), Some(Format::RawTiffLike));
        assert_eq!(sniff(&fixture(7, false)), Some(Format::Tiff));
        assert_eq!(sniff(&fixture(1, true)), Some(Format::Tiff));
        assert_eq!(sniff(&raw[..38]), Some(Format::Tiff));
        raw[38..42].copy_from_slice(b"TEST");
        assert_eq!(sniff(&raw), Some(Format::Tiff));
    }
}
