//! Encoders: JPEG (`jpeg-encoder`), PNG (`png`), TIFF (`tiff`), lossless WebP (`image-webp`) and,
//! on native targets with the `avif` feature, AVIF (`ravif`/`rav1e`). Metadata (ICC/EXIF/XMP) is
//! embedded where the format allows.

use crate::{Error, Result};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};

/// Interleaved sample data. Integer samples are display-encoded (0..=MAX); float samples are
/// written as-is.
#[derive(Clone, Copy, Debug)]
pub enum Samples<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
    F32(&'a [f32]),
}

/// An image to encode: `channels` = 1 (gray), 2 (gray + alpha), 3 (RGB) or 4 (RGBA), straight alpha.
#[derive(Clone, Copy, Debug)]
pub struct EncodeImage<'a> {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub samples: Samples<'a>,
}

impl<'a> EncodeImage<'a> {
    pub fn rgba8(img: &'a Rgba8) -> Self {
        EncodeImage { width: img.width as u32, height: img.height as u32, channels: 4, samples: Samples::U8(bytemuck::cast_slice(&img.data)) }
    }
    pub fn new(width: u32, height: u32, channels: u8, samples: Samples<'a>) -> Self {
        EncodeImage { width, height, channels, samples }
    }

    fn validate(&self) -> Result<()> {
        if !(1..=4).contains(&self.channels) {
            return Err(Error::Encode(format!("unsupported channel count {}", self.channels)));
        }
        if self.width == 0 || self.height == 0 {
            return Err(Error::Encode("zero dimension".into()));
        }
        let need = self.width as u64 * self.height as u64 * self.channels as u64;
        let have = match self.samples {
            Samples::U8(s) => s.len(),
            Samples::U16(s) => s.len(),
            Samples::F32(s) => s.len(),
        } as u64;
        if have < need {
            return Err(Error::Encode(format!("sample buffer too short: {have} < {need}")));
        }
        Ok(())
    }
}

/// Metadata to embed.
#[derive(Clone, Copy, Debug, Default)]
pub struct EncodeMeta<'a> {
    pub icc: Option<&'a [u8]>,
    /// TIFF-structured EXIF (starting at `II`/`MM`, no `Exif\0\0` prefix).
    pub exif: Option<&'a [u8]>,
    pub xmp: Option<&'a str>,
    /// Print resolution in pixels per inch (JPEG JFIF density, PNG `pHYs`, TIFF X/YResolution).
    pub ppi: Option<u16>,
}

/// JPEG chroma subsampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChromaSubsampling {
    /// No subsampling (best quality).
    #[default]
    S444,
    S422,
    S420,
}

fn shared_image(img: &EncodeImage, meta: &EncodeMeta) -> Result<photocraft_codecs::Image> {
    use photocraft_codecs::{ChannelLayout as L, Image};
    img.validate()?;
    let layout = match img.channels {
        1 => L::Gray,
        2 => L::GrayA,
        3 => L::Rgb,
        _ => L::Rgba,
    };
    let n = img.width as usize * img.height as usize * img.channels as usize;
    let mut out = match img.samples {
        Samples::U8(s) => Image::from_u8(img.width, img.height, layout, s[..n].to_vec()),
        Samples::U16(s) => Image::from_u16(img.width, img.height, layout, &s[..n]),
        Samples::F32(s) => Image::from_f32(img.width, img.height, layout, &s[..n]),
    }
    .map_err(shared_error)?;
    out.icc = meta.icc.map(<[u8]>::to_vec);
    out.meta.exif = meta.exif.map(<[u8]>::to_vec);
    out.meta.xmp = meta.xmp.map(str::to_string);
    out.meta.dpi = meta.ppi.filter(|p| *p > 0).map(|p| (p as f32, p as f32));
    Ok(out)
}
// The historical borrowed Library API receives samples in stored camera order. Its callers
// decide when to orient pixels (export normalizes metadata before calling us).
fn shared_options() -> photocraft_codecs::EncodeOptions {
    photocraft_codecs::EncodeOptions { keep_orientation: true, ..Default::default() }
}
fn shared_error(e: photocraft_codecs::CodecError) -> Error {
    Error::Encode(e.to_string())
}

/// Shared baseline JPEG; alpha is dropped, inputs must be 8-bit.
pub fn encode_jpeg(img: &EncodeImage, quality: u8, subsampling: ChromaSubsampling, meta: &EncodeMeta) -> Result<Vec<u8>> {
    if !matches!(img.samples, Samples::U8(_)) {
        return Err(Error::Encode("JPEG needs 8-bit samples".into()));
    }
    // Preserve the Library's strict oversized-metadata errors.
    if meta.exif.is_some_and(|s| s.len() > 65_527)
        || meta.xmp.is_some_and(|s| s.len() + 29 > 65_533)
        || meta.icc.is_some_and(|s| s.len().div_ceil(65_519) > 255)
    {
        return Err(Error::Encode("JPEG metadata larger than its APP segment limit".into()));
    }
    let img = shared_image(img, meta)?;
    let sub = match subsampling {
        ChromaSubsampling::S444 => photocraft_codecs::ChromaSubsampling::S444,
        ChromaSubsampling::S422 => photocraft_codecs::ChromaSubsampling::S422,
        ChromaSubsampling::S420 => photocraft_codecs::ChromaSubsampling::S420,
    };
    photocraft_codecs::encode_jpeg_with_sampling(&img, &photocraft_codecs::EncodeOptions { jpeg_quality: quality, ..shared_options() }, sub)
        .map_err(shared_error)
}
/// Shared PNG, preserving integer sample depth.
pub fn encode_png(img: &EncodeImage, meta: &EncodeMeta) -> Result<Vec<u8>> {
    if matches!(img.samples, Samples::F32(_)) {
        return Err(Error::Encode("PNG needs 8- or 16-bit samples".into()));
    }
    photocraft_codecs::encode(&shared_image(img, meta)?, photocraft_codecs::Format::Png, &shared_options()).map_err(shared_error)
}
/// TIFF compression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TiffCompression {
    None,
    Lzw,
    #[default]
    Deflate,
    PackBits,
}
/// Shared TIFF, preserving sample depth, alpha and colour model.
pub fn encode_tiff(img: &EncodeImage, compression: TiffCompression, meta: &EncodeMeta) -> Result<Vec<u8>> {
    let comp = match compression {
        TiffCompression::None => photocraft_codecs::TiffCompression::None,
        TiffCompression::Lzw => photocraft_codecs::TiffCompression::Lzw,
        TiffCompression::Deflate => photocraft_codecs::TiffCompression::Deflate,
        TiffCompression::PackBits => photocraft_codecs::TiffCompression::PackBits,
    };
    photocraft_codecs::encode(
        &shared_image(img, meta)?,
        photocraft_codecs::Format::Tiff,
        &photocraft_codecs::EncodeOptions { tiff_compression: comp, ..shared_options() },
    )
    .map_err(shared_error)
}
/// Shared lossless WebP with ICC/EXIF/XMP.
pub fn encode_webp_lossless(img: &EncodeImage, meta: &EncodeMeta) -> Result<Vec<u8>> {
    if !matches!(img.samples, Samples::U8(_)) {
        return Err(Error::Encode("WebP needs 8-bit samples".into()));
    }
    photocraft_codecs::encode(&shared_image(img, meta)?, photocraft_codecs::Format::WebP, &shared_options()).map_err(shared_error)
}
/// Shared 10-bit AVIF (native `avif` builds only).
pub fn encode_avif(img: &EncodeImage, quality: u8, speed: u8, meta: &EncodeMeta) -> Result<Vec<u8>> {
    img.validate()?;
    if matches!(img.samples, Samples::F32(_)) {
        return Err(Error::Encode("AVIF needs integer samples".into()));
    }
    #[cfg(all(feature = "avif", not(target_arch = "wasm32")))]
    {
        photocraft_codecs::avif::encode_image(&shared_image(img, meta)?, quality, speed, &shared_options()).map_err(shared_error)
    }
    #[cfg(not(all(feature = "avif", not(target_arch = "wasm32"))))]
    {
        let _ = (quality, speed, meta);
        Err(Error::Encode("AVIF encoding is not available in this build".into()))
    }
}
