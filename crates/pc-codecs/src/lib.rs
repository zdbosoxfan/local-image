//! `photocraft-codecs`: symmetric, depth- and metadata-preserving codecs for
//! flat raster image formats.
//!
//! * Every format we **write** we can also **read** (see
//!   [`ASYMMETRIC_EXCEPTIONS`] for the documented, feature-gated exception).
//! * Bit depth (8/16-bit integer, 16/32-bit float), ICC profiles and basic
//!   metadata (EXIF, XMP, DPI, text) are preserved where the format allows.
//! * [`caps`] declares per-format capabilities and [`fidelity_warnings`]
//!   tells the UI exactly what an export will lose.
//! * The API is I/O free (`&[u8]` in, `Vec<u8>` out) and builds for
//!   `wasm32-unknown-unknown`. Decoders enforce [`Limits`] before allocating.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![expect(clippy::allow_attributes_without_reason, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]

pub mod avif;
pub mod avif_input;
mod codecs;
mod error;
mod fidelity;
mod format;
mod image;
pub mod jpeg_par;
mod options;
pub use options::{DESKTOP_PHOTO_BYTES, DESKTOP_PHOTO_PIXELS};
pub mod orientation;
pub mod web;

pub use crate::codecs::jpeg::{JpegPreview, JpegPreviewSource, decode_jpeg_scaled, jpeg_embedded_preview};
pub use crate::codecs::png::encode_indexed as encode_png_indexed;
pub use crate::codecs::tiff::{PhotoshopTags as TiffPhotoshopTags, photoshop_tags as tiff_photoshop_tags, writes_little_endian as tiff_writes_little_endian};
pub use crate::codecs::tiff_ifd::{TiffInfo, TiffPage, TiffPageKind};
pub use crate::error::CodecError;
pub use crate::fidelity::{FidelityWarning, fidelity_warnings, fidelity_warnings_with};
pub use crate::format::{
    ASYMMETRIC_EXCEPTIONS, DetectedFormat, Format, FormatCaps, PHOTO_EXTENSIONS, RAW_EXTENSIONS, caps, detect, from_extension, identify, is_readable_extension,
    raw_from_extension, readable_extensions, writable_extensions,
};
pub use crate::image::{ChannelLayout, ColorHint, DecodeWarning, DeepChannel, DeepImage, Image, Metadata, SampleType};
pub use crate::options::{DecodeOptions, EncodeOptions, ExrCompression, Limits, PngCompression, TiffCompression};
pub use half::f16;

/// Decode TIFF device samples for a colour-managed caller, keeping associated alpha
/// until that caller removes the transfer curve. The ordinary decoder stays straight.
fn decode_tiff_for_color(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    crate::codecs::tiff::decode_for_color(bytes, limits)
}

use crate::codecs::{exr, heif, jpeg, png, pnm, tiff, via_image, webp};

/// Detect the format and decode with default [`Limits`].
pub fn decode(bytes: &[u8]) -> Result<Image, CodecError> {
    decode_with(bytes, &DecodeOptions::default())
}

/// Detect the format and decode.
pub fn decode_with(bytes: &[u8], opts: &DecodeOptions) -> Result<Image, CodecError> {
    let format = detect(bytes).ok_or(CodecError::UnknownFormat)?;
    decode_as_with(format, bytes, opts)
}

/// Decode as a specific format with default [`Limits`].
pub fn decode_as(format: Format, bytes: &[u8]) -> Result<Image, CodecError> {
    decode_as_with(format, bytes, &DecodeOptions::default())
}

/// Decode as a specific format.
pub fn decode_as_with(format: Format, bytes: &[u8], opts: &DecodeOptions) -> Result<Image, CodecError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode_as_inner(format, bytes, opts)))
        .unwrap_or_else(|_| Err(CodecError::InvalidImage("Image decoder rejected malformed data".into())))
}
fn decode_as_inner(format: Format, bytes: &[u8], opts: &DecodeOptions) -> Result<Image, CodecError> {
    // HEIF without the `heif` feature: its decoder stub says what is missing from this build.
    if !caps(format).read && format != Format::Heif {
        return Err(CodecError::unsupported(format, "decoding is not available for this format"));
    }
    let l = &opts.limits;
    let img = match format {
        Format::Png => png::decode(bytes, l),
        Format::Jpeg => jpeg::decode(bytes, l),
        Format::Tiff => tiff::decode(bytes, l),
        Format::WebP => webp::decode(bytes, l),
        Format::Pnm => pnm::decode(bytes, l),
        Format::OpenExr => exr::decode(bytes, l),
        Format::Psd => codecs::psd::decode(bytes, l),
        #[cfg(feature = "jxl")]
        Format::Jxl => codecs::jxl::decode(bytes, l),
        #[cfg(not(feature = "jxl"))]
        Format::Jxl => Err(CodecError::unsupported(format, "built without the jxl feature")),
        Format::Heif => heif::decode(bytes, l, opts.keep_orientation),
        Format::Gif | Format::Bmp | Format::Tga | Format::Ico | Format::Qoi | Format::Hdr | Format::Avif => via_image::decode(format, bytes, l),
    }?;
    // Turn the pixels upright, like Photoshop: a TIFF records it in the decoded page's own
    // directory, the others in their EXIF block. The metadata is rewritten to Orientation = 1
    // on the way. HEIF keeps it in its container, and its decoder has already applied it.
    let o = match format {
        Format::Tiff => img.orientation,
        Format::Heif => 1,
        _ => img.meta.exif.as_deref().map_or(1, lightcraft_tiff::exif::orientation),
    };
    finish_decode(img, o, opts)
}

/// Decode stored samples for the Library's colour interpreter. The same bounded decoders
/// retain associated samples until TRC/ICC conversion; HEIF applies its container transform.
pub fn decode_as_for_color(format: Format, bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match format {
        Format::Tiff => decode_tiff_for_color(bytes, limits),
        #[cfg(feature = "jxl")]
        Format::Jxl => codecs::jxl::decode_for_color(bytes, limits),
        _ => decode_as_inner(format, bytes, &DecodeOptions { limits: *limits, keep_orientation: format != Format::Heif }),
    }))
    .unwrap_or_else(|_| Err(CodecError::InvalidImage("Image decoder rejected malformed data".into())))
}

/// Stored dimensions without decoding pixels. Used to report a rejected source's dimensions.
pub fn dimensions(format: Format, bytes: &[u8]) -> Result<(u32, u32), CodecError> {
    if format == Format::Jpeg {
        return codecs::jpeg::dimensions(bytes);
    }
    if format == Format::Psd {
        let header = bytes.get(..26).ok_or_else(|| CodecError::malformed(format, "truncated header"))?;
        let w = u32::from_be_bytes([header[18], header[19], header[20], header[21]]);
        let h = u32::from_be_bytes([header[14], header[15], header[16], header[17]]);
        return Ok((w, h));
    }
    if format == Format::WebP {
        return image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).map(|d| d.dimensions()).map_err(|e| CodecError::malformed(format, e));
    }
    if format == Format::Tiff {
        let info = tiff_info(bytes)?;
        let page = info.default_page().and_then(|i| info.pages.get(i)).ok_or_else(|| CodecError::malformed(format, "no image directory"))?;
        return Ok((page.width, page.height));
    }
    if format == Format::Png {
        let mut decoder = ::png::Decoder::new(std::io::Cursor::new(bytes));
        return decoder.read_header_info().map(|i| i.size()).map_err(|e| CodecError::malformed(format, e));
    }
    codecs::via_image::dimensions(format, bytes)
}

/// Lists the pages (image directories) of a TIFF or BigTIFF file without decoding pixels: the
/// main IFD chain and its SubIFDs, each with its size, kind (page, reduced-resolution copy or
/// mask) and storage. [`TiffInfo::default_page`] is the page [`decode`] opens.
pub fn tiff_info(bytes: &[u8]) -> Result<TiffInfo, CodecError> {
    crate::codecs::tiff_ifd::tiff_info(bytes)
}

/// Decodes page `page` (an index into [`tiff_info`]'s `pages`) of a TIFF or BigTIFF file,
/// applying that page's orientation unless `opts.keep_orientation`.
pub fn decode_tiff_page(bytes: &[u8], page: usize, opts: &DecodeOptions) -> Result<Image, CodecError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode_tiff_page_inner(bytes, page, opts)))
        .unwrap_or_else(|_| Err(CodecError::InvalidImage("Image decoder rejected malformed data".into())))
}
fn decode_tiff_page_inner(bytes: &[u8], page: usize, opts: &DecodeOptions) -> Result<Image, CodecError> {
    let img = tiff::decode_page(bytes, Some(page), &opts.limits)?;
    let orientation = img.orientation;
    finish_decode(img, orientation, opts)
}

/// The Orientation (1–8) of the page [`decode`] opens from a TIFF or BigTIFF file; 1 when it is
/// absent, malformed or out of range.
pub fn tiff_orientation(bytes: &[u8]) -> u16 {
    tiff::orientation(bytes, None)
}

/// The Orientation (1–8) of page `page` (an index into [`tiff_info`]'s `pages`); 1 when it is
/// absent, malformed or out of range, or the page does not exist.
pub fn tiff_page_orientation(bytes: &[u8], page: usize) -> u16 {
    tiff::orientation(bytes, Some(page))
}

/// The limits check and the orientation, shared by every decoder.
fn finish_decode(mut img: Image, o: u16, opts: &DecodeOptions) -> Result<Image, CodecError> {
    let l = &opts.limits;
    // Final guard for decoders whose header we could not pre-inspect.
    l.check(img.width(), img.height(), img.layout(), img.sample_type())?;
    img.orientation = o;
    if opts.keep_orientation {
        return Ok(img);
    }
    // Orientations 5–8 swap width and height: with asymmetric limits a stored landscape
    // that fit can turn into a portrait that does not, so check the upright size too.
    if (5..=8).contains(&o) {
        l.check(img.height(), img.width(), img.layout(), img.sample_type())?;
    }
    let mut img = img.oriented(o)?;
    img.orientation = 1;
    Ok(img)
}

/// Deep OpenEXR samples (`deepscanline`/`deeptile`): the structured per-pixel sample lists
/// (colour, alpha and Z per sample), for callers that composite deep data themselves —
/// [`decode_as_with`] instead flattens a deep file into a normal [`Image`] with a
/// [`DecodeWarning::DeepFlattened`]. `Err(Unsupported)` when the file has no deep part.
pub fn decode_deep_exr(bytes: &[u8], limits: &Limits) -> Result<DeepImage, CodecError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cur = std::io::Cursor::new(bytes);
        let meta = ::exr::meta::MetaData::read_from_buffered(&mut cur, false).map_err(|e| CodecError::malformed(Format::OpenExr, e))?;
        codecs::deep_exr::decode_deep(&meta, bytes, limits)
    }))
    .unwrap_or_else(|_| Err(CodecError::malformed(Format::OpenExr, "image decoder panicked")))
}

/// Encode `image` as `format`. Conversions follow the same plan that
/// [`fidelity_warnings_with`] reports; fatal conditions (unsupported
/// format, oversize image) return an error.
pub fn encode(image: &Image, format: Format, opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    if !caps(format).write {
        return Err(CodecError::unsupported(format, "encoding is not available in this build"));
    }
    if image.width() == 0 || image.height() == 0 {
        return Err(CodecError::InvalidImage("cannot encode an empty image".into()));
    }
    if let Some((mw, mh)) = format.max_dimensions()
        && (image.width() > mw || image.height() > mh)
    {
        return Err(CodecError::encode(format, format!("dimensions exceed {mw}x{mh}")));
    }
    let plan = fidelity::plan(image, format, opts);
    match format {
        Format::Png => png::encode(image, plan, opts),
        Format::Jpeg => jpeg::encode(image, plan, opts),
        Format::Tiff => tiff::encode(image, plan, opts),
        Format::WebP => webp::encode(image, plan, opts),
        Format::Pnm => pnm::encode(image, plan, opts),
        Format::OpenExr => exr::encode(image, plan, opts),
        Format::Heif | Format::Jxl | Format::Psd => Err(CodecError::unsupported(format, "encoding is not available in this build")),
        Format::Gif | Format::Bmp | Format::Tga | Format::Ico | Format::Qoi | Format::Hdr | Format::Avif => via_image::encode(format, image, plan, opts),
    }
}

/// Baseline JPEG with an explicit sampling mode (including Library's 4:2:2).
pub fn encode_jpeg_with_sampling(image: &Image, opts: &EncodeOptions, sampling: ChromaSubsampling) -> Result<Vec<u8>, CodecError> {
    if image.width() == 0 || image.height() == 0 {
        return Err(CodecError::InvalidImage("cannot encode an empty image".into()));
    }
    jpeg::encode_with_sampling(image, fidelity::plan(image, Format::Jpeg, opts), opts, sampling)
}

pub use options::ChromaSubsampling;
