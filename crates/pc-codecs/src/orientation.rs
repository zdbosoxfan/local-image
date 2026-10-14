//! EXIF / TIFF orientation (tag 274): reading it, applying it to the pixels,
//! and rewriting it to 1 ("top-left", upright) once the pixels are upright.
//!
//! Decoders apply the orientation by default (like Photoshop and every photo
//! viewer), so a document is always edited upright. Encoders then write
//! Orientation = 1, so a viewer never rotates the already-upright pixels a
//! second time.
//!
//! Orientation values (TIFF 6.0, EXIF 2.3): 1 = upright, 2 = mirrored, 3 =
//! rotated 180°, 4 = flipped vertically, 5 = transposed, 6 = needs a 90°
//! clockwise turn, 7 = transverse, 8 = needs a 90° counter-clockwise turn.
//! Anything malformed, truncated or out of range reads as 1: an IFD0 that
//! starts inside the 8-byte header or is cut off before its next-IFD pointer,
//! or an Orientation entry that is not exactly one SHORT or LONG (the spec says
//! SHORT, but some cameras and tools write LONG). When IFD0 holds
//! several Orientation entries the first one wins (like libexif).

use std::borrow::Cow;

use crate::error::CodecError;
use crate::image::Image;

/// One metadata policy for all shared format writers.
pub(crate) fn encoded_exif(exif: &[u8], keep_orientation: bool) -> Cow<'_, [u8]> {
    if keep_orientation { Cow::Borrowed(exif) } else { upright_exif(exif) }
}
pub(crate) fn encoded_xmp(xmp: &str, keep_orientation: bool) -> Cow<'_, str> {
    if keep_orientation { Cow::Borrowed(xmp) } else { upright_xmp(xmp) }
}

pub use lightcraft_meta::upright_xmp;
/// Shared byte-preserving metadata helpers. Pixel orientation remains codec-local.
pub use lightcraft_tiff::exif::{orientation as exif_orientation, upright as upright_exif};

/// Source pixel index for output pixel `(0, y2)` and the step per output column, for
/// orientation `o` of a `w`×`h` source (values 2–8; see the module docs).
fn walk(o: u16, w: isize, h: isize, y2: isize) -> (isize, isize) {
    match o {
        2 => (y2 * w + w - 1, -1),
        3 => ((h - 1 - y2) * w + w - 1, -1),
        4 => ((h - 1 - y2) * w, 1),
        5 => (y2, w),
        6 => ((h - 1) * w + y2, -w),
        7 => ((h - 1) * w + (w - 1 - y2), -w),
        8 => (w - 1 - y2, w),
        _ => (y2 * w, 1),
    }
}

/// Output columns per block: a block's source pixels (for a 90° turn, `BLOCK`
/// rows × the band height) stay in cache while it is written.
const BLOCK: usize = 64;
/// Output rows per parallel band.
const BAND: usize = 64;

/// Writes the oriented pixels of one band of output rows, `N` bytes per pixel.
fn band<const N: usize>(src: &[[u8; N]], dst: &mut [[u8; N]], o: u16, (w, h): (usize, usize), ow: usize, y0: usize) {
    let (wi, hi) = (w as isize, h as isize);
    let rows = dst.len() / ow.max(1);
    for x0 in (0..ow).step_by(BLOCK) {
        let x1 = (x0 + BLOCK).min(ow);
        for r in 0..rows {
            let (base, step) = walk(o, wi, hi, (y0 + r) as isize);
            let Some(out) = dst.get_mut(r * ow + x0..r * ow + x1) else { return };
            let mut i = base + x0 as isize * step;
            for px in out {
                if let Some(s) = usize::try_from(i).ok().and_then(|i| src.get(i)) {
                    *px = *s;
                }
                i += step;
            }
        }
    }
}

/// Orients `src` (`N` bytes per pixel) into a new buffer, a band of rows per task.
fn orient_n<const N: usize>(src: &[u8], o: u16, (w, h): (usize, usize), ow: usize) -> Result<Vec<u8>, CodecError> {
    let mut out = alloc_zeroed(src.len())?;
    let (s, _) = src.as_chunks::<N>();
    let (d, _) = out.as_chunks_mut::<N>();
    let per = (BAND * ow).max(1);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        d.par_chunks_mut(per).enumerate().for_each(|(b, chunk)| band(s, chunk, o, (w, h), ow, b * BAND));
    }
    #[cfg(target_arch = "wasm32")]
    for (b, chunk) in d.chunks_mut(per).enumerate() {
        band(s, chunk, o, (w, h), ow, b * BAND);
    }
    Ok(out)
}

/// A zeroed buffer of `n` bytes, or an error (not an abort) when memory runs out.
fn alloc_zeroed(n: usize) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| CodecError::LimitExceeded(format!("cannot allocate {n} bytes to orient the image")))?;
    out.resize(n, 0);
    Ok(out)
}

/// Pixel-format-agnostic orientation: any byte size per pixel.
fn orient_bytes(src: &[u8], bpp: usize, o: u16, (w, h): (usize, usize), ow: usize) -> Result<Vec<u8>, CodecError> {
    match bpp {
        1 => orient_n::<1>(src, o, (w, h), ow),
        2 => orient_n::<2>(src, o, (w, h), ow),
        3 => orient_n::<3>(src, o, (w, h), ow),
        4 => orient_n::<4>(src, o, (w, h), ow),
        5 => orient_n::<5>(src, o, (w, h), ow),
        6 => orient_n::<6>(src, o, (w, h), ow),
        8 => orient_n::<8>(src, o, (w, h), ow),
        10 => orient_n::<10>(src, o, (w, h), ow),
        12 => orient_n::<12>(src, o, (w, h), ow),
        16 => orient_n::<16>(src, o, (w, h), ow),
        _ => orient_n::<20>(src, o, (w, h), ow),
    }
}

impl Image {
    /// The image turned upright for orientation `o` (1–8; anything else is
    /// treated as 1 and returns the image unchanged). Width and height swap
    /// for 5–8, as does the DPI, and the EXIF and XMP orientation are
    /// rewritten to 1 so the result is not rotated again on display or export.
    pub fn oriented(self, o: u16) -> Result<Image, CodecError> {
        if !(2..=8).contains(&o) {
            return Ok(self);
        }
        let (w, h) = (self.width() as usize, self.height() as usize);
        let swap = o >= 5;
        let (ow, oh) = if swap { (h, w) } else { (w, h) };
        let (layout, sample) = (self.layout(), self.sample_type());
        let bpp = layout.channels() * sample.bytes();
        // Every layout/sample pair is 1–20 bytes per pixel and has a fixed-size path.
        if !matches!(bpp, 1..=6 | 8 | 10 | 12 | 16 | 20) || w == 0 || h == 0 {
            return Err(CodecError::InvalidImage(format!("cannot orient {layout:?} {sample:?} {w}x{h}")));
        }
        let data = orient_bytes(self.data(), bpp, o, (w, h), ow)?;
        let mut meta = self.meta.clone();
        if let Some(e) = &mut meta.exif
            && let Cow::Owned(fixed) = upright_exif(e)
        {
            *e = fixed;
        }
        if let Some(x) = &mut meta.xmp
            && let Cow::Owned(fixed) = upright_xmp(x)
        {
            *x = fixed;
        }
        if swap {
            meta.dpi = meta.dpi.map(|(x, y)| (y, x));
        }
        let mut out = Image::from_raw(ow as u32, oh as u32, layout, sample, data)?.with_icc(self.icc).with_meta(meta);
        out.warnings = self.warnings;
        out.color_hint = self.color_hint;
        out.source_bit_depth = self.source_bit_depth;
        out.source_float = self.source_float;
        out.associated_alpha = self.associated_alpha;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_tiff::{ByteOrder as Order, directory::File};

    #[test]
    fn shared_directory_orientation_keeps_unknown_first_duplicate_and_patches_later_values() {
        use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value as TiffValue};
        for order in [Order::Little, Order::Big] {
            for big in [false, true] {
                let ifd = IfdBuilder::new().with(274, TiffValue::Short(vec![7])).with(275, TiffValue::Short(vec![6]));
                let mut bytes = TiffWriter::new(order, big).write(&[ifd]).expect("fixture");
                let file = File::parse(&bytes).expect("header");
                let first = file.first_ifd as usize + if big { 8 } else { 2 };
                let second = first + if big { 20 } else { 12 };
                let encode = |value: u16| if order == ByteOrder::Little { value.to_le_bytes() } else { value.to_be_bytes() };
                bytes[first + 2..first + 4].copy_from_slice(&encode(99));
                bytes[second..second + 2].copy_from_slice(&encode(274));
                assert_eq!(exif_orientation(&bytes), 1);
                let fixed = upright_exif(&bytes);
                assert!(matches!(fixed, Cow::Owned(_)));
                assert_eq!(&fixed[first..second], &bytes[first..second]);
                let file = File::parse(&fixed).expect("fixed header");
                let entry = file.entries_at(file.first_ifd).expect("table").nth(1).expect("duplicate");
                assert_eq!(file.uint(&entry, 0), Some(1));
            }
        }
    }

    #[test]
    fn shared_directory_orientation_preserves_decode_source_precision_hints() {
        use crate::image::{ChannelLayout, SampleType};
        let mut image = Image::new(1, 2, ChannelLayout::Rgba, SampleType::U16).expect("image");
        image.source_bit_depth = Some(12);
        image.source_float = Some(false);
        image.associated_alpha = true;
        let oriented = image.oriented(6).expect("orientation");
        assert_eq!((oriented.width(), oriented.height()), (2, 1));
        assert_eq!(oriented.source_bit_depth, Some(12));
        assert_eq!(oriented.source_float, Some(false));
        assert!(oriented.associated_alpha);
    }

    #[test]
    fn an_impossible_allocation_is_an_error_not_an_abort() {
        assert!(matches!(alloc_zeroed(usize::MAX), Err(CodecError::LimitExceeded(_))));
        assert!(matches!(orient_n::<1>(&[], 6, (0, 0), 0), Ok(v) if v.is_empty()));
        assert_eq!(alloc_zeroed(3).unwrap(), vec![0, 0, 0]);
    }
    #[test]
    fn j03_metadata_adapters_share_the_library_backend_policy() {
        for value in 1..=8 {
            let exif = lightcraft_tiff::exif::minimal_exif(value);
            assert_eq!(exif_orientation(&exif), lightcraft_tiff::exif::orientation(&exif));
            assert_eq!(upright_exif(&exif), lightcraft_tiff::exif::upright(&exif));
        }
        let packet = r#"<rdf:Description xmlns:o="http://ns.adobe.com/tiff/1.0/" o:Orientation='6'/>"#;
        assert_eq!(upright_xmp(packet), packet.replace("'6'", "'1'"));
        assert_eq!(upright_xmp(packet), lightcraft_meta::upright_xmp(packet));
    }
}
