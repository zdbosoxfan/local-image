//! Format identification and capability declarations.

use crate::image::{ChannelLayout, SampleType};

/// Supported (or known) raster file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    Tiff,
    WebP,
    Gif,
    Bmp,
    Tga,
    Ico,
    /// Netpbm family: PBM/PGM/PPM (P1–P6), PAM (P7) and PFM (`PF`/`Pf`).
    Pnm,
    Qoi,
    OpenExr,
    /// Radiance RGBE.
    Hdr,
    /// Encode-only behind the `avif` feature; see [`ASYMMETRIC_EXCEPTIONS`].
    Avif,
    /// HEIF/HEIC (HEVC-coded): read-only; see [`ASYMMETRIC_EXCEPTIONS`].
    Heif,
    /// JPEG XL (read-only, behind `jxl`).
    Jxl,
    /// Photoshop PSD / PSB merged composite (read-only).
    Psd,
}

/// What a format can hold **and** what this crate reads/writes for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCaps {
    /// Decoding is available in this build.
    pub read: bool,
    /// Encoding is available in this build.
    pub write: bool,
    /// Sample types stored without conversion.
    pub depths: &'static [SampleType],
    /// Layouts stored *and read back* without conversion.
    pub layouts: &'static [ChannelLayout],
    pub alpha: bool,
    pub icc: bool,
    pub exif: bool,
    pub xmp: bool,
    /// Physical resolution (DPI) is preserved.
    pub dpi: bool,
    /// Free-form key/value text is preserved.
    pub text: bool,
    /// The container can hold animation/multiple frames. This crate only
    /// decodes the first frame and encodes a single frame.
    pub animation: bool,
    /// Our encoder is lossy (quantization or lossy compression).
    pub lossy: bool,
}

/// Formats that are enabled but intentionally not symmetric, with the reason.
/// The test-suite asserts that every other enabled format is read+write.
pub const ASYMMETRIC_EXCEPTIONS: &[(Format, &str)] = &[
    (
        Format::Avif,
        "AVIF encode uses ravif (pure Rust) but decoding requires dav1d (C); read stays unsupported \
         until a pure-Rust AV1 decoder is viable. Only enabled with the non-default `avif` feature.",
    ),
    (Format::Jxl, "JPEG XL decoding uses jxl-oxide behind the jxl feature; no JPEG XL encoder is available."),
    (Format::Psd, "Photoshop merged composites are decoded here; layered PSD/PSB writing is provided by photocraft-psd."),
    (
        Format::Heif,
        "HEIC decode uses heic-rs (pure Rust, in the optional photocraft-heif crate behind the \
         non-default `heif` feature, which official builds enable), so iPhone and Mac photos open; \
         writing needs an HEVC encoder, and the mature ones (x265, libheif) are C, so write stays \
         unsupported. Without the feature HEIF is detected but neither read nor written.",
    ),
];

use ChannelLayout as L;
use SampleType as S;

const RGB_GRAY: &[ChannelLayout] = &[L::Gray, L::GrayA, L::Rgb, L::Rgba];
const ALL_LAYOUTS: &[ChannelLayout] = &[L::Gray, L::GrayA, L::Rgb, L::Rgba, L::Cmyk, L::CmykA];

impl Format {
    /// Every format known to the crate (enabled or not).
    pub const ALL: [Format; 16] = [
        Format::Png,
        Format::Jpeg,
        Format::Tiff,
        Format::WebP,
        Format::Gif,
        Format::Bmp,
        Format::Tga,
        Format::Ico,
        Format::Pnm,
        Format::Qoi,
        Format::OpenExr,
        Format::Hdr,
        Format::Avif,
        Format::Heif,
        Format::Jxl,
        Format::Psd,
    ];

    pub fn caps(self) -> FormatCaps {
        caps(self)
    }

    /// Readable or writable in this build.
    pub fn is_enabled(self) -> bool {
        let c = caps(self);
        c.read || c.write
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Tiff => "TIFF",
            Format::WebP => "WebP",
            Format::Gif => "GIF",
            Format::Bmp => "BMP",
            Format::Tga => "TGA",
            Format::Ico => "ICO",
            Format::Pnm => "Netpbm",
            Format::Qoi => "QOI",
            Format::OpenExr => "OpenEXR",
            Format::Hdr => "Radiance HDR",
            Format::Avif => "AVIF",
            Format::Heif => "HEIF",
            Format::Jxl => "JPEG XL",
            Format::Psd => "Photoshop",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Tiff => "image/tiff",
            Format::WebP => "image/webp",
            Format::Gif => "image/gif",
            Format::Bmp => "image/bmp",
            Format::Tga => "image/x-tga",
            Format::Ico => "image/x-icon",
            Format::Pnm => "image/x-portable-anymap",
            Format::Qoi => "image/qoi",
            Format::OpenExr => "image/x-exr",
            Format::Hdr => "image/vnd.radiance",
            Format::Avif => "image/avif",
            Format::Heif => "image/heif",
            Format::Jxl => "image/jxl",
            Format::Psd => "image/vnd.adobe.photoshop",
        }
    }

    /// Maximum encodable dimensions, if the format has a hard limit.
    pub fn max_dimensions(self) -> Option<(u32, u32)> {
        match self {
            Format::Jpeg | Format::Gif => Some((65535, 65535)),
            Format::WebP => Some((16384, 16384)),
            Format::Ico => Some((256, 256)),
            _ => None,
        }
    }
}

/// Capability table. `read`/`write` reflect the current build's features.
pub fn caps(format: Format) -> FormatCaps {
    let base = FormatCaps {
        read: true,
        write: true,
        depths: &[S::U8],
        layouts: &[L::Rgb, L::Rgba],
        alpha: true,
        icc: false,
        exif: false,
        xmp: false,
        dpi: false,
        text: false,
        animation: false,
        lossy: false,
    };
    match format {
        Format::Png => {
            FormatCaps { depths: &[S::U8, S::U16], layouts: RGB_GRAY, icc: true, exif: true, xmp: true, dpi: true, text: true, animation: true, ..base }
        }
        Format::Jpeg => FormatCaps { layouts: &[L::Gray, L::Rgb, L::Cmyk], alpha: false, icc: true, exif: true, xmp: true, dpi: true, lossy: true, ..base },
        Format::Tiff => {
            FormatCaps { depths: &[S::U8, S::U16, S::F32], layouts: &ChannelLayout::ALL, icc: true, exif: true, xmp: true, dpi: true, text: true, ..base }
        }
        Format::WebP => FormatCaps { icc: true, exif: true, xmp: true, animation: true, ..base },
        Format::Gif => FormatCaps { layouts: &[L::Rgba], animation: true, lossy: true, ..base },
        Format::Bmp => base,
        Format::Tga => FormatCaps { layouts: RGB_GRAY, ..base },
        Format::Ico => FormatCaps { layouts: &[L::Rgba], ..base },
        Format::Pnm => FormatCaps { depths: &[S::U8, S::U16, S::F32], layouts: ALL_LAYOUTS, ..base },
        Format::Qoi => base,
        Format::OpenExr => FormatCaps { depths: &[S::F16, S::F32], layouts: RGB_GRAY, ..base },
        Format::Hdr => FormatCaps { depths: &[S::F32], layouts: &[L::Rgb], alpha: false, lossy: true, ..base },
        Format::Avif => FormatCaps {
            read: false,
            write: cfg!(all(feature = "avif", not(target_arch = "wasm32"))),
            depths: &[S::U8, S::U16],
            exif: true,
            lossy: true,
            ..base
        },
        Format::Jxl => FormatCaps {
            read: cfg!(feature = "jxl"),
            write: false,
            depths: &[S::U8, S::U16, S::F32],
            layouts: RGB_GRAY,
            icc: true,
            exif: true,
            xmp: true,
            ..base
        },
        Format::Psd => FormatCaps { write: false, depths: &[S::U8, S::U16, S::F32], layouts: RGB_GRAY, icc: true, ..base },
        Format::Heif => {
            FormatCaps { read: cfg!(feature = "heif"), write: false, depths: &[S::U8, S::U16], icc: true, exif: true, xmp: true, lossy: true, ..base }
        }
    }
}

/// Identify a format from its leading bytes (magic numbers). TGA has no
/// magic and is recognized last, by header plausibility.
fn detect_raster(bytes: &[u8]) -> Option<Format> {
    let b = bytes;
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") || b.starts_with(b"II+\0") || b.starts_with(b"MM\0+") {
        return Some(Format::Tiff);
    }
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Some(Format::WebP);
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some(Format::Gif);
    }
    if b.starts_with(b"qoif") {
        return Some(Format::Qoi);
    }
    if b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some(Format::OpenExr);
    }
    if b.starts_with(b"#?RADIANCE") || b.starts_with(b"#?RGBE") {
        return Some(Format::Hdr);
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        if is_avif_ftyp(b) {
            return Some(Format::Avif);
        }
        if is_heif_ftyp(b) {
            return Some(Format::Heif);
        }
    }
    if b.len() >= 14 && b.starts_with(b"BM") {
        return Some(Format::Bmp);
    }
    if b.len() >= 6 && b[0..4] == [0, 0, 1, 0] && (b[4] != 0 || b[5] != 0) {
        return Some(Format::Ico);
    }
    if b.len() >= 3 && b[0] == b'P' && matches!(b[1], b'1'..=b'7' | b'F' | b'f') && b[2].is_ascii_whitespace() {
        return Some(Format::Pnm);
    }
    if looks_like_tga(b) {
        return Some(Format::Tga);
    }
    None
}

/// ISO BMFF ftyp major and compatible brands (skip the minor version).
/// Limit sniffing to the supplied prefix, with checked handling of malformed box sizes.
fn ftyp_brands(b: &[u8]) -> impl Iterator<Item = &[u8; 4]> {
    let size = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let end = if size == 0 { b.len().min(4096) } else { size.min(b.len()).min(4096) };
    b.get(8..end).unwrap_or_default().as_chunks::<4>().0.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, c)| c)
}

fn is_avif_ftyp(b: &[u8]) -> bool {
    ftyp_brands(b).any(|c| matches!(c, b"avif" | b"avis"))
}

fn is_heif_ftyp(b: &[u8]) -> bool {
    ftyp_brands(b).any(|c| matches!(c, b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"hevm" | b"hevs" | b"mif1" | b"msf1" | b"miaf"))
}

fn looks_like_tga(b: &[u8]) -> bool {
    if b.len() < 18 {
        return false;
    }
    if b.len() >= 26 && &b[b.len() - 18..] == b"TRUEVISION-XFILE.\0" {
        return true;
    }
    let cmap_type = b[1];
    let img_type = b[2];
    let cmap_len = u16::from_le_bytes([b[5], b[6]]);
    let cmap_depth = b[7];
    let w = u16::from_le_bytes([b[12], b[13]]);
    let h = u16::from_le_bytes([b[14], b[15]]);
    let depth = b[16];
    let desc = b[17];
    let type_ok = matches!(img_type, 1 | 2 | 3 | 9 | 10 | 11);
    let cmap_ok = match cmap_type {
        0 => cmap_len == 0 && cmap_depth == 0 && !matches!(img_type, 1 | 9),
        1 => matches!(img_type, 1 | 9) && cmap_len > 0 && matches!(cmap_depth, 15 | 16 | 24 | 32),
        _ => false,
    };
    let depth_ok = match img_type {
        1 | 9 => depth == 8,
        3 | 11 => matches!(depth, 8 | 16),
        _ => matches!(depth, 15 | 16 | 24 | 32),
    };
    type_ok && cmap_ok && depth_ok && w > 0 && h > 0 && desc & 0xC0 == 0
}

/// Look up a format by file extension (case-insensitive, with or without a
/// leading dot, or a full file name/path).
pub fn from_extension(ext: &str) -> Option<Format> {
    let ext = ext.rsplit(['.', '/', '\\']).next().unwrap_or(ext).to_ascii_lowercase();
    Format::ALL.into_iter().find(|f| f.extensions().contains(&ext.as_str()))
}

/// Container classification shared by raster and camera-raw callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    Raster(Format),
    RawTiffLike,
    RawOther,
}

/// Detect camera-raw containers as well as raster formats. A TIFF prefix may need
/// its SubIFDs to distinguish raw sensor data from an ordinary TIFF preview.
pub fn identify(b: &[u8]) -> Option<DetectedFormat> {
    if b.starts_with(b"FUJIFILMCCD-RAW")
        || b.starts_with(b"\0MRM")
        || b.starts_with(b"FOVb")
        || (b.len() >= 14 && b.starts_with(b"II") && &b[6..14] == b"HEAPCCDR")
    {
        return Some(DetectedFormat::RawOther);
    }
    if b.starts_with(b"IIRO") || b.starts_with(b"IIRS") || b.starts_with(b"MMOR") || b.starts_with(b"IIU\0") {
        return Some(DetectedFormat::RawTiffLike);
    }
    if b.starts_with(&[0xff, 0x0a]) || b.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Some(DetectedFormat::Raster(Format::Jxl));
    }
    if b.len() >= 6 && b.starts_with(b"8BPS") && matches!(u16::from_be_bytes([b[4], b[5]]), 1 | 2) {
        return Some(DetectedFormat::Raster(Format::Psd));
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        if ftyp_brands(b).any(|c| c == b"crx ") {
            return Some(DetectedFormat::RawOther);
        }
        if ftyp_brands(b).any(|c| c == b"jxl ") {
            return Some(DetectedFormat::Raster(Format::Jxl));
        }
    }
    let format = detect_raster(b)?;
    if format == Format::Tiff && tiff_is_raw(b) { Some(DetectedFormat::RawTiffLike) } else { Some(DetectedFormat::Raster(format)) }
}

/// Raster-only compatibility API. TIFF-structured raw files retain their TIFF
/// container classification; callers routing raw files should use [`identify`].
pub fn detect(b: &[u8]) -> Option<Format> {
    match identify(b)? {
        DetectedFormat::Raster(f) => Some(f),
        DetectedFormat::RawTiffLike if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") || b.starts_with(b"II+\0") || b.starts_with(b"MM\0+") => {
            Some(Format::Tiff)
        }
        DetectedFormat::RawTiffLike | DetectedFormat::RawOther => None,
    }
}

// TIFF raw policy is shared with the pixel decoder, including the main IFD chain.
fn tiff_is_raw(b: &[u8]) -> bool {
    crate::codecs::tiff_ifd::File::parse(b).is_some_and(|file| file.is_camera_raw())
}

// Define each extension once. Consumers select readable or writable formats;
// raw routing and folder scans share the same camera extension inventory.
macro_rules! extension_registry {
    ($( $format:ident => [$($ext:literal),+]),+; raw [$($raw:literal),+]) => {
        impl Format {
            /// Lowercase extensions, preferred writing extension first.
            pub fn extensions(self) -> &'static [&'static str] {
                match self { $(Self::$format => &[$($ext),+]),+ }
            }
        }
        pub const RAW_EXTENSIONS: &[&str] = &[$($raw),+];
        /// Known photo extensions; use capability filtering before opening.
        pub const PHOTO_EXTENSIONS: &[&str] = &[$($($ext),+,)+ $($raw),+];
    };
}
extension_registry! {
    Png => ["png", "apng"],
    Jpeg => ["jpg", "jpeg", "jpe", "jfif"],
    Tiff => ["tif", "tiff"],
    WebP => ["webp"],
    Gif => ["gif"],
    Bmp => ["bmp", "dib"],
    Tga => ["tga", "icb", "vda", "vst"],
    Ico => ["ico"],
    Pnm => ["pnm", "pbm", "pgm", "ppm", "pam", "pfm"],
    Qoi => ["qoi"],
    OpenExr => ["exr"],
    Hdr => ["hdr"],
    Avif => ["avif"],
    Heif => ["heic", "heif", "hif"],
    Jxl => ["jxl"],
    Psd => ["psd", "psb"];
    raw ["dng", "cr2", "cr3", "nef", "nrw", "arw", "arq", "raf", "orf", "rw2", "rwl", "raw", "pef", "srw", "x3f", "3fr", "iiq", "erf", "mos", "srf", "sr2", "kdc", "dcr", "mef", "mrw", "fff", "crw"]
}

/// Camera raw extension (case-insensitive, also accepts file names).
pub fn raw_from_extension(ext: &str) -> bool {
    let ext = ext.rsplit(['.', '/', '\\']).next().unwrap_or(ext);
    RAW_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known))
}

/// Extensions decoded by this build. Raw files use a separate camera decoder.
pub fn readable_extensions() -> Vec<&'static str> {
    Format::ALL.into_iter().filter(|f| f.caps().read).flat_map(Format::extensions).copied().collect()
}

/// Extensions encoded by this build; excludes read-only formats and camera raw.
pub fn writable_extensions() -> Vec<&'static str> {
    Format::ALL.into_iter().filter(|f| f.caps().write).flat_map(Format::extensions).copied().collect()
}

/// A format this build can read, optionally including camera raw files.
pub fn is_readable_extension(ext: &str, include_raw: bool) -> bool {
    (include_raw && raw_from_extension(ext)) || from_extension(ext).is_some_and(|f| f.caps().read)
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn registry_keeps_open_and_export_capabilities_separate() {
        let read = readable_extensions();
        let write = writable_extensions();
        for format in Format::ALL {
            for ext in format.extensions() {
                assert_eq!(from_extension(ext), Some(format));
                assert_eq!(from_extension(&format!("image.{}", ext.to_ascii_uppercase())), Some(format));
                assert_eq!(read.contains(ext), format.caps().read);
                assert_eq!(write.contains(ext), format.caps().write);
                assert!(PHOTO_EXTENSIONS.contains(ext));
            }
        }
        assert!(read.contains(&"psb"));
        assert!(!write.contains(&"psb"));
        assert!(!write.contains(&"jxl"));
        assert!(!write.contains(&"heic"));
        assert!(!read.contains(&"avif"));
        for ext in RAW_EXTENSIONS {
            assert!(raw_from_extension(&ext.to_ascii_uppercase()));
            assert!(is_readable_extension(ext, true));
            assert!(!is_readable_extension(ext, false));
            assert!(!write.contains(ext));
            assert!(PHOTO_EXTENSIONS.contains(ext));
        }
        let mut unique = PHOTO_EXTENSIONS.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), PHOTO_EXTENSIONS.len());
    }

    #[test]
    fn shared_detection_handles_extended_bmff_and_malformed_sizes() {
        let mut bmff = vec![0; 84];
        bmff[..4].copy_from_slice(&84u32.to_be_bytes());
        bmff[4..8].copy_from_slice(b"ftyp");
        bmff[8..12].copy_from_slice(b"mif1");
        bmff[80..84].copy_from_slice(b"avif");
        assert_eq!(identify(&bmff), Some(DetectedFormat::Raster(Format::Avif)));
        bmff[80..84].copy_from_slice(b"crx ");
        assert_eq!(identify(&bmff), Some(DetectedFormat::RawOther));
        for declared in [1u32, 8, 11] {
            bmff[..4].copy_from_slice(&declared.to_be_bytes());
            assert_eq!(identify(&bmff), None);
        }
        assert_eq!(identify(&[0xff, 0x0a]), Some(DetectedFormat::Raster(Format::Jxl)));
        assert_eq!(identify(b"8BPS\0\x02"), Some(DetectedFormat::Raster(Format::Psd)));
        assert_eq!(identify(b"8BPS\0\x03"), None);
    }

    #[test]
    fn shared_detection_keeps_raw_routing_and_tiff_compatibility() {
        let raw = b"II*\0\x10\0\0\0CR\x02\0";
        assert_eq!(identify(raw), Some(DetectedFormat::RawTiffLike));
        assert_eq!(detect(raw), Some(Format::Tiff));
        for raw in [b"FUJIFILMCCD-RAW".as_slice(), b"\0MRM", b"FOVb", b"IIRO"] {
            assert!(matches!(identify(raw), Some(DetectedFormat::RawOther | DetectedFormat::RawTiffLike)));
            assert_eq!(detect(raw), None);
        }
    }
}
