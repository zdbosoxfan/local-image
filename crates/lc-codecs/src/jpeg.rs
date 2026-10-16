//! Library colour/thumbnail adapters over the shared JPEG decoder and preview selector.

use crate::convert::{from_shared, shared_error, shared_limits};
use crate::{DecodeOptions, Decoded, Format, Result, Thumbnail, ThumbnailSource};

const F: Format = Format::Jpeg;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded> {
    let (image, source) = photocraft_codecs::decode_jpeg_scaled(bytes, &shared_limits(opts), opts.max_size).map_err(|e| shared_error(F, bytes, e))?;
    from_shared(F, image, source, opts)
}

pub(crate) fn embedded_thumbnail(bytes: &[u8], max_edge: u32, min_edge: u32) -> Option<Thumbnail> {
    let preview = photocraft_codecs::jpeg_embedded_preview(bytes, max_edge, min_edge)?;
    let decoded = decode(preview.bytes, &DecodeOptions::fit(max_edge, max_edge)).ok()?;
    let source = match preview.source {
        photocraft_codecs::JpegPreviewSource::ExifThumbnail => ThumbnailSource::ExifThumbnail,
        photocraft_codecs::JpegPreviewSource::MpfPreview => ThumbnailSource::MpfPreview,
    };
    Some(Thumbnail {
        image: decoded.to_srgb8(),
        orientation: preview.orientation,
        source,
        source_width: preview.source_width,
        source_height: preview.source_height,
    })
}
