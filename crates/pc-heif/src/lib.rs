//! `photocraft-heif`: the optional HEIF / HEIC (iPhone and Mac photo) decoder.
//!
//! A thin wrapper around `heic-rs`, a pure-Rust HEVC still-picture decoder: single pictures and
//! grid-tiled photos, 8- and 10-bit (10-bit decodes to 16-bit), alpha auxiliary images, ICC,
//! EXIF and XMP. Its API is plain data ([`Info`], [`Decoded`], [`Error`]) so the crate knows
//! nothing about the rest of PhotoCraft; `photocraft-codecs` adapts it behind its `heif` feature.
//!
//! HEIF records orientation in the container (`irot`/`imir`, plus a `clap` crop), not in EXIF.
//! [`Options::apply_transforms`] applies them; with it off, the pixels come back as coded.
//!
//! Never panics: heic-rs is young, so every call into it runs under `catch_unwind` and a panic
//! inside it becomes [`Error::Malformed`].

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod census;
mod pps;
use pps::check_parameter_sets;
use std::borrow::Cow;

use heic_rs::PixelLayout;
use heic_rs::context::Context;

/// What a HEIF file declares, read from the container alone (no pixel is decoded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    /// Size after the container transforms (rotation, mirror, crop): what a viewer shows.
    pub width: u32,
    pub height: u32,
    /// Size as coded, before the transforms.
    pub coded_width: u32,
    pub coded_height: u32,
    /// The primary image has an alpha auxiliary image.
    pub has_alpha: bool,
    /// More than 8 bits per sample: it decodes to 16-bit.
    pub sixteen_bit: bool,
}

/// Decode settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Refuse images with more pixels than this (checked before decoding).
    pub max_pixels: u64,
    /// Apply the container's rotation, mirror and crop (`irot`/`imir`/`clap`).
    pub apply_transforms: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_pixels: 1 << 28, apply_transforms: true }
    }
}

/// A decoded image: interleaved RGB or RGBA samples, row-major, no padding. 16-bit samples are
/// native-endian `u16`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// Four channels (RGBA) instead of three (RGB).
    pub has_alpha: bool,
    /// Two bytes per sample instead of one.
    pub sixteen_bit: bool,
    pub data: Vec<u8>,
    /// The primary image's ICC profile.
    pub icc: Option<Vec<u8>>,
    /// EXIF as a TIFF structure (without HEIF's offset header). Its Orientation tag only mirrors
    /// what the container says; it is returned verbatim.
    pub exif: Option<Vec<u8>>,
    /// The XMP packet.
    pub xmp: Option<String>,
}

/// Why a file could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A valid file using something this decoder does not handle (image sequences, overlays…).
    Unsupported(String),
    /// The image exceeds a pixel or decoder-work limit, or carries an oversized metadata box.
    Limit(String),
    /// Broken or truncated data (or a decoder bug, reported the same way).
    Malformed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(m) => write!(f, "unsupported HEIF: {m}"),
            Error::Limit(m) => write!(f, "limit exceeded: {m}"),
            Error::Malformed(m) => write!(f, "malformed HEIF: {m}"),
        }
    }
}

impl std::error::Error for Error {}

const SEQUENCE: &str = "this file holds an image sequence; only HEIF still images can be opened";

fn err(e: heic_rs::Error) -> Error {
    match e {
        heic_rs::Error::Unsupported(what) => Error::Unsupported(what.to_string()),
        // The primary item is neither an HEVC picture nor a grid of them.
        heic_rs::Error::MissingBox("hvcC") => Error::Unsupported("the image is an overlay, an identity derivation or not HEVC-coded".into()),
        heic_rs::Error::PixelLimit { .. } | heic_rs::Error::BoxTooLarge { .. } => Error::Limit(e.to_string()),
        e => Error::Malformed(e.to_string()),
    }
}

/// Runs `f`, turning a heic-rs panic into an error. Fuzzing found two out-of-range slices in
/// heic-rs 0.1.1 on malformed files: in its box parser (`boxes.rs:130`) and during reconstruction
/// (`hevc/decode/recon.rs:70`). The codecs fuzz target also calls heic-rs directly, so its panics
/// stay visible there.
fn guarded<T>(f: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err(Error::Malformed("the HEIF decoder failed on this file".into())))
}

/// Reads the container's declared size, depth and alpha without decoding pixels, so callers can
/// check their limits first.
pub fn probe(bytes: &[u8]) -> Result<Info, Error> {
    guarded(|| probe_unguarded(bytes))
}

fn probe_unguarded(bytes: &[u8]) -> Result<Info, Error> {
    let (ctx, mut visits) = bounded_context(bytes)?;
    let info = probe_with_context(bytes, &ctx, &mut visits)?;
    Ok(Info {
        width: info.width,
        height: info.height,
        coded_width: info.coded_width,
        coded_height: info.coded_height,
        has_alpha: info.has_alpha,
        sixteen_bit: info.bit_depth > 8,
    })
}

/// Opens the container only after bounding its metadata and the primary item's extent chain.
fn bounded_context(bytes: &[u8]) -> Result<(Context<'_>, usize), Error> {
    census::cap(bytes)?;
    let ctx = Context::open(bytes).map_err(open_error)?;
    let mut visits = 0;
    item_visits(&ctx, ctx.meta.primary, 0, &mut visits)?;
    Ok((ctx, visits))
}

fn open_error(error: heic_rs::Error) -> Error {
    match error {
        heic_rs::Error::MissingBox("meta") => Error::Unsupported(SEQUENCE.into()),
        error => err(error),
    }
}

/// Checks every reconstruction used while probing, including repeated grid references.
fn probe_with_context(bytes: &[u8], ctx: &Context<'_>, visits: &mut usize) -> Result<heic_rs::ImageInfo, Error> {
    let id = ctx.meta.primary;
    // `heic_rs::probe` reads an item without `ispe` from its parameter sets
    check_item_parameter_sets(ctx, id)?;
    let info = heic_rs::probe(bytes).map_err(open_error)?;
    let mut alpha_items = Vec::new();
    if let Some(alpha) = ctx.alpha_item(id).map_err(err)? {
        item_visits(ctx, alpha, 0, visits)?;
        alpha_items.push(alpha);
        if let Some((_, tiles)) = ctx.grid(alpha).map_err(err)? {
            for &tile in &tiles {
                item_visits(ctx, tile, 0, visits)?;
            }
            alpha_items.extend(tiles);
        }
    }
    let mut coded = match ctx.grid(id).map_err(err)? {
        Some((_, tiles)) => tiles,
        None => vec![id],
    };
    for &item in &coded {
        if item != id {
            item_visits(ctx, item, 0, visits)?;
        }
    }
    coded.sort_unstable();
    coded.dedup();
    alpha_items.sort_unstable();
    alpha_items.dedup();
    for &item in coded.iter().chain(&alpha_items) {
        check_item_parameter_sets(ctx, item)?;
    }
    let mut reconstructed = 0_usize;
    for item in coded {
        reconstructed = reconstructed.saturating_add(ctx.item_data(item).map_err(err)?.len());
        if reconstructed > bytes.len() {
            return Err(Error::Limit(format!("the container holds {reconstructed} bytes of item data, over the {} bytes of the file itself", bytes.len())));
        }
    }
    Ok(info)
}

/// [`check_parameter_sets`] of a coded item's `hvcC`, if it has one.
fn check_item_parameter_sets(ctx: &Context<'_>, id: u32) -> Result<(), Error> {
    ctx.props(id).map_err(err)?.hvcc.as_ref().map_or(Ok(()), |h| check_parameter_sets(&h.parameter_sets()))
}

/// Counts the item extents a reconstruction follows, before heic-rs visits them.
fn item_visits(ctx: &Context<'_>, id: u32, depth: u32, visits: &mut usize) -> Result<(), Error> {
    use heic_rs::meta::iloc::Construction;

    if depth > MAX_ITEM_CHAIN {
        return Err(Error::Limit(format!("items built from items nest deeper than {MAX_ITEM_CHAIN} levels")));
    }
    let Some(location) = ctx.meta.location(id) else {
        return Ok(());
    };
    *visits = visits.saturating_add(location.extents.len().max(1));
    if *visits > MAX_ITEM_VISITS {
        return Err(Error::Limit(format!("reconstructing the image takes over {MAX_ITEM_VISITS} item extent reads")));
    }
    if location.construction == Construction::Item {
        for extent in &location.extents {
            if let Ok(source) = u32::try_from(extent.index)
                && source != id
            {
                item_visits(ctx, source, depth + 1, visits)?;
            }
        }
    }
    Ok(())
}

// A normal grid needs one visit per tile; repeated references share this limit too.
const MAX_ITEM_VISITS: usize = 4096;
// Matches heic-rs's own maximum item-construction recursion depth.
const MAX_ITEM_CHAIN: u32 = 8;
// Each transform can rewrite the full decoded image; phone photos use at most one of each kind.
const MAX_TRANSFORMS: usize = 12;
// Bound sample work even when a caller opts out of the default pixel ceiling.
const MAX_OUTPUT_SAMPLES: u64 = 1 << 30;

/// Decodes the primary image, with its metadata.
pub fn decode(bytes: &[u8], options: &Options) -> Result<Decoded, Error> {
    guarded(|| decode_unguarded(bytes, options))
}

fn decode_unguarded(bytes: &[u8], options: &Options) -> Result<Decoded, Error> {
    let (ctx, mut visits) = bounded_context(bytes)?;
    let properties = ctx.props(ctx.meta.primary).map_err(err)?;
    if properties.transforms.len() > MAX_TRANSFORMS {
        return Err(Error::Limit(format!("the image carries {} transform properties, over the {MAX_TRANSFORMS}-property cap", properties.transforms.len())));
    }
    let info = probe_with_context(bytes, &ctx, &mut visits)?;
    let sixteen_bit = info.bit_depth > 8;
    let pixel_layout = match (info.has_alpha, sixteen_bit) {
        (true, true) => PixelLayout::Rgba16,
        (true, false) => PixelLayout::Rgba8,
        (false, true) => PixelLayout::Rgb16,
        (false, false) => PixelLayout::Rgb8,
    };
    let channels = if info.has_alpha { 4 } else { 3 };
    let max_pixels = options.max_pixels.min(MAX_OUTPUT_SAMPLES / channels);
    let raw = heic_rs::DecodeOptions::default()
        .with_layout(pixel_layout)
        .with_max_pixels(Some(max_pixels))
        .with_transforms(options.apply_transforms)
        .with_alpha(info.has_alpha);
    let decoded = heic_rs::decode(bytes, &raw).map_err(err)?;
    let (icc, exif, xmp) = metadata(bytes);
    Ok(Decoded { width: decoded.width, height: decoded.height, has_alpha: info.has_alpha, sixteen_bit, data: decoded.data, icc, exif, xmp })
}

/// The primary image's ICC profile, EXIF (TIFF structure) and XMP packet. Metadata is a
/// courtesy: a broken metadata item never fails a decode whose pixels came out fine.
fn metadata(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<String>) {
    let Ok(ctx) = heic_rs::context::Context::open(bytes) else {
        return (None, None, None);
    };
    let id = ctx.meta.primary;
    let icc = ctx.props(id).ok().and_then(|p| ctx.icc(&p).map(<[u8]>::to_vec));
    let mut visits = 0;
    let exif_item = ctx.exif_item(id);
    let exif =
        if exif_item.is_none_or(|item| item_visits(&ctx, item, 0, &mut visits).is_ok()) { ctx.exif(id).ok().flatten().map(<[u8]>::to_vec) } else { None };
    let xmp_item = ctx.xmp_item(id).filter(|&item| item_visits(&ctx, item, 0, &mut visits).is_ok());
    let xmp = xmp_item.and_then(|x| ctx.item_data(x).ok()).and_then(|d| match d {
        Cow::Borrowed(b) => String::from_utf8(b.to_vec()).ok(),
        Cow::Owned(b) => String::from_utf8(b).ok(),
    });
    (icc, exif, xmp.map(|x| x.trim_end_matches('\0').to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Found by the `decode_heif` fuzz target: a malformed box makes heic-rs 0.1.1 slice out of
    /// range (`boxes.rs:130`). It must be an error, not a crash.
    const HEIC_RS_BOX_PANIC: [u8; 72] = [
        0x00, 0x00, 0x00, 0x24, 0x66, 0x74, 0x79, 0x70, 0x68, 0x65, 0x69, 0x63, 0x00, 0x00, 0x00, 0x00, 0x6d, 0x69, 0x66, 0x31, 0x4d, 0x69, 0x50, 0x72, 0x6d,
        0x69, 0x61, 0x66, 0x4d, 0x69, 0x48, 0x42, 0x72, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6d, 0x65, 0x74, 0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x10, 0x75, 0x75, 0x69, 0x64, 0x00, 0x00, 0x00, 0x00, 0x03, 0x08, 0x08, 0x08, 0x00, 0x03, 0x01, 0x03, 0x70, 0x00, 0xa8, 0x00,
    ];

    #[test]
    fn a_heic_rs_panic_is_a_malformed_error() {
        for apply_transforms in [true, false] {
            let r = decode(&HEIC_RS_BOX_PANIC, &Options { apply_transforms, ..Default::default() });
            assert!(matches!(r, Err(Error::Malformed(_))), "{r:?}");
        }
    }

    #[test]
    fn image_sequence_is_unsupported_with_a_clear_message() {
        // An ftyp and nothing else: a HEIF image sequence would have a moov here instead of a meta.
        let bytes = b"\0\0\0\x18ftypmsf1\0\0\0\0msf1hevc";
        for r in [probe(bytes).map(|_| ()), decode(bytes, &Options::default()).map(|_| ())] {
            assert!(matches!(&r, Err(Error::Unsupported(m)) if m.contains("sequence")), "{r:?}");
        }
    }

    #[test]
    fn garbage_is_an_error() {
        for bytes in [&b""[..], b"\0\0\0\x18ftypheic\0\0\0\0mif1heic", &[0xFF; 64]] {
            assert!(probe(bytes).is_err());
            assert!(decode(bytes, &Options::default()).is_err());
        }
    }

    #[test]
    fn errors_display_their_kind() {
        assert!(Error::Unsupported("x".into()).to_string().contains("unsupported"));
        assert!(Error::Limit("x".into()).to_string().contains("limit"));
        assert!(Error::Malformed("x".into()).to_string().contains("malformed"));
    }
}

#[cfg(test)]
mod tests_hardening;
