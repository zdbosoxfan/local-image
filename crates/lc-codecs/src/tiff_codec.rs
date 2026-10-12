//! TIFF decode via the `tiff` crate: 8/16/32-bit integer, 16/32/64-bit float, gray/RGB/CMYK/palette,
//! alpha (associated or not), ICC (34675), XMP (700), orientation (274). First image only.

use crate::convert::{Buf, Meta, Model, Raw, check_size, finish};
use crate::{DecodeOptions, Decoded, Error, Format, Result};
use tiff::ColorType;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

const F: Format = Format::Tiff;

fn err(e: impl std::fmt::Display) -> Error {
    Error::Malformed(F, e.to_string())
}

/// Present only the first IFD's WhiteIsZero tag as BlackIsZero to the decoder.
/// tiff 0.11 already inverts single-channel gray, but rejects WhiteIsZero with
/// extra samples. Reading stored samples lets us handle both without touching alpha.
struct StoredSamples<'a> {
    cursor: std::io::Cursor<&'a [u8]>,
    patch: Option<(u64, Vec<u8>)>,
}
impl<'a> StoredSamples<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        let patch = Self::photometric_patch(bytes)?;
        Ok(Self { cursor: std::io::Cursor::new(bytes), patch })
    }
    fn photometric_patch(bytes: &[u8]) -> Result<Option<(u64, Vec<u8>)>> {
        let little = match bytes.get(..2) {
            Some(b"II") => true,
            Some(b"MM") => false,
            _ => return Ok(None),
        };
        let read = |at: usize, len: usize| -> Option<u64> {
            let part = bytes.get(at..at.checked_add(len)?)?;
            Some(if little {
                part.iter().rev().fold(0, |v, b| (v << 8) | u64::from(*b))
            } else {
                part.iter().fold(0, |v, b| (v << 8) | u64::from(*b))
            })
        };
        let (ifd, count_size, entry_size, value_at) = match read(2, 2) {
            Some(42) => (read(4, 4), 2usize, 12usize, 8usize),
            Some(43) if read(4, 2) == Some(8) && read(6, 2) == Some(0) => (read(8, 8), 8, 20, 12),
            _ => return Ok(None),
        };
        let ifd = ifd.and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("invalid first IFD offset"))?;
        let count = read(ifd, count_size).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("invalid first IFD count"))?;
        let table_len = count.checked_mul(entry_size).filter(|len| *len <= 32 << 20).ok_or_else(|| err("IFD exceeds metadata limit"))?;
        let start = ifd.checked_add(count_size).ok_or_else(|| err("invalid IFD range"))?;
        let end = start.checked_add(table_len).filter(|end| *end <= bytes.len()).ok_or_else(|| err("truncated IFD"))?;
        let mut found = None;
        for at in (start..end).step_by(entry_size) {
            if read(at, 2) != Some(262) {
                continue;
            }
            if found.is_some() {
                return Err(err("duplicate PhotometricInterpretation"));
            }
            let len = match read(at + 2, 2) {
                Some(3) => 2,
                Some(4) => 4,
                _ => return Err(err("invalid PhotometricInterpretation type")),
            };
            if read(at + 4, if count_size == 8 { 8 } else { 4 }) != Some(1) {
                return Err(err("invalid PhotometricInterpretation count"));
            }
            let at = at + value_at;
            found = Some((at, len, read(at, len).ok_or_else(|| err("truncated PhotometricInterpretation"))?));
        }
        Ok(found.filter(|(_, _, value)| *value == 0).map(|(at, len, _)| {
            let mut replacement = vec![0; len];
            replacement[if little { 0 } else { len - 1 }] = 1;
            (at as u64, replacement)
        }))
    }
}
impl std::io::Read for StoredSamples<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let start = self.cursor.position();
        let n = std::io::Read::read(&mut self.cursor, out)?;
        if let Some((at, bytes)) = &self.patch {
            let end = start.saturating_add(n as u64);
            for pos in (*at).max(start)..at.saturating_add(bytes.len() as u64).min(end) {
                out[(pos - start) as usize] = bytes[(pos - *at) as usize];
            }
        }
        Ok(n)
    }
}
impl std::io::Seek for StoredSamples<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.cursor, pos)
    }
}

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded> {
    let mut limits = Limits::default();
    limits.decoding_buffer_size = opts.max_pixels.saturating_mul(4 * 8).min(usize::MAX as u64) as usize;
    limits.ifd_value_size = 32 << 20;
    let stored = StoredSamples::new(bytes)?;
    let white_is_zero = stored.patch.is_some();
    let mut d = Decoder::new(stored).map_err(err)?.with_limits(limits);
    let (w, h) = d.dimensions().map_err(err)?;
    check_size(F, w as u64, h as u64, opts)?;
    let ct = d.colortype().map_err(err)?;

    let icc = tag_bytes(&mut d, Tag::IccProfile);
    let xmp = tag_bytes(&mut d, Tag::from_u16_exhaustive(700)).map(|v| String::from_utf8_lossy(&v).trim_end_matches('\0').to_string());
    let orientation = d.get_tag_u32(Tag::Orientation).ok().map(|v| v as u16);
    let photometric = d.get_tag_u32(Tag::PhotometricInterpretation).ok();
    let planar = d.get_tag_u32(Tag::PlanarConfiguration).ok() == Some(2);
    let extra = d.get_tag_u16_vec(Tag::ExtraSamples).ok().unwrap_or_default();
    let colormap = d.get_tag_u16_vec(Tag::ColorMap).ok();
    let compression = d.get_tag_u32(Tag::Compression).ok();

    let (model, n_color, n_samples, bits) = match ct {
        ColorType::Gray(b) => (Model::Gray, 1, 1, b),
        ColorType::GrayA(b) => (Model::Gray, 1, 2, b),
        ColorType::RGB(b) => (Model::Rgb, 3, 3, b),
        ColorType::RGBA(b) => (Model::Rgb, 3, 4, b),
        ColorType::CMYK(b) => (Model::Cmyk, 4, 4, b),
        ColorType::CMYKA(b) => (Model::Cmyk, 4, 5, b),
        ColorType::Palette(b) => (Model::Rgb, 1, 1, b),
        // JPEG-compressed YCbCr is converted to RGB by the JPEG decoder.
        ColorType::YCbCr(b) if compression == Some(7) => (Model::Rgb, 3, 3, b),
        ColorType::Multiband { bit_depth, num_samples } if num_samples >= 1 && matches!(photometric, Some(0 | 1)) => {
            (Model::Gray, 1, num_samples as usize, bit_depth)
        }
        ColorType::Multiband { bit_depth, num_samples } if num_samples >= 3 => (Model::Rgb, 3, num_samples as usize, bit_depth),
        ColorType::Multiband { bit_depth, num_samples } if num_samples >= 1 => (Model::Gray, 1, num_samples as usize, bit_depth),
        other => return Err(Error::Unsupported(F, unsupported_name(other))),
    };
    let has_alpha = n_samples > n_color && !matches!(ct, ColorType::Palette(_));
    let premultiplied = has_alpha && extra.first() == Some(&1);

    let res = d.read_image().map_err(err)?;
    let (wu, hu) = (w as usize, h as usize);
    let n = wu * hu;
    let mut buf = match res {
        DecodingResult::U8(v) => Buf::U8(v),
        DecodingResult::U16(v) => Buf::U16(v),
        // Rare integer widths: keep 16 significant bits (plenty for display-referred data).
        DecodingResult::U32(v) => Buf::U16(v.into_iter().map(|x| (x >> 16) as u16).collect()),
        DecodingResult::U64(v) => Buf::U16(v.into_iter().map(|x| (x >> 48) as u16).collect()),
        DecodingResult::I8(v) => Buf::U8(v.into_iter().map(|x| (x.max(0) as u8) << 1).collect()),
        DecodingResult::I16(v) => Buf::U16(v.into_iter().map(|x| (x.max(0) as u16) << 1).collect()),
        DecodingResult::I32(v) => Buf::U16(v.into_iter().map(|x| (x.max(0) >> 15) as u16).collect()),
        DecodingResult::I64(v) => Buf::U16(v.into_iter().map(|x| (x.max(0) >> 47) as u16).collect()),
        DecodingResult::F16(v) => Buf::F32(v.into_iter().map(|x| x.to_f32()).collect()),
        DecodingResult::F32(v) => Buf::F32(v),
        DecodingResult::F64(v) => Buf::F32(v.into_iter().map(|x| x as f32).collect()),
    };
    let bit_depth = bits;

    if planar && n_samples > 1 {
        buf = interleave(buf, n, n_samples);
    }

    // Palette → 16-bit RGB.
    if let ColorType::Palette(b) = ct {
        let map = colormap.ok_or_else(|| err("palette image without ColorMap"))?;
        let entries = 1usize << b.min(16);
        if map.len() < 3 * entries {
            return Err(err("short ColorMap"));
        }
        let idx: Vec<usize> = match &buf {
            Buf::U8(v) => v.iter().map(|&i| i as usize).collect(),
            Buf::U16(v) => v.iter().map(|&i| i as usize).collect(),
            Buf::F32(_) => return Err(err("float palette")),
        };
        let mut out = Vec::with_capacity(n * 3);
        for &i in idx.iter().take(n) {
            let i = i.min(entries - 1);
            out.extend_from_slice(&[map[i], map[entries + i], map[2 * entries + i]]);
        }
        buf = Buf::U16(out);
        let raw = Raw { width: wu, height: hu, model: Model::Rgb, alpha: false, premultiplied: false, buf, bit_depth };
        let meta = Meta { icc, xmp, orientation, ..Default::default() };
        return finish(F, raw, meta, (w, h), opts);
    }

    // Match libtiff's setupMap/putagreytile convention: polarity maps stored
    // gray to associated intensity first, then finish unassociates before TRC.
    // Only colour changes: extra samples (including alpha) keep their exact values.
    if white_is_zero {
        match &mut buf {
            Buf::U8(v) => v.chunks_exact_mut(n_samples).for_each(|pixel| pixel[0] = 255 - pixel[0]),
            Buf::U16(v) => v.chunks_exact_mut(n_samples).for_each(|pixel| pixel[0] = 65535 - pixel[0]),
            Buf::F32(v) => v.chunks_exact_mut(n_samples).for_each(|pixel| pixel[0] = 1.0 - pixel[0]),
        }
    }

    // Drop extra samples beyond colour + one alpha.
    let keep = n_color + has_alpha as usize;
    if n_samples != keep {
        buf = select_channels(buf, n, n_samples, keep);
    }
    let raw = Raw { width: wu, height: hu, model, alpha: has_alpha, premultiplied, buf, bit_depth };
    let meta = Meta { icc, xmp, orientation, ..Default::default() };
    finish(F, raw, meta, (w, h), opts)
}

/// A BYTE/UNDEFINED tag as bytes (the decoder may surface unknown byte tags as unsigned values).
fn tag_bytes<R: std::io::Read + std::io::Seek>(d: &mut Decoder<R>, tag: Tag) -> Option<Vec<u8>> {
    use tiff::decoder::ifd::Value;
    fn byte(v: &Value) -> Option<u8> {
        match v {
            Value::Byte(b) => Some(*b),
            Value::Unsigned(u) => u8::try_from(*u).ok(),
            Value::Short(u) => u8::try_from(*u).ok(),
            Value::SignedByte(b) => Some(*b as u8),
            _ => None,
        }
    }
    match d.find_tag(tag).ok()?? {
        Value::List(v) => v.iter().map(byte).collect(),
        Value::Ascii(s) => Some(s.into_bytes()),
        v => byte(&v).map(|b| vec![b]),
    }
}

fn unsupported_name(ct: ColorType) -> &'static str {
    match ct {
        ColorType::Lab(_) => "CIE L*a*b* TIFF",
        ColorType::YCbCr(_) => "uncompressed YCbCr TIFF",
        _ => "colour type",
    }
}

fn interleave(buf: Buf, n: usize, s: usize) -> Buf {
    fn go<T: Copy + Default>(v: Vec<T>, n: usize, s: usize) -> Vec<T> {
        if v.len() < n * s {
            return v;
        }
        let mut out = vec![T::default(); n * s];
        for c in 0..s {
            for i in 0..n {
                out[i * s + c] = v[c * n + i];
            }
        }
        out
    }
    match buf {
        Buf::U8(v) => Buf::U8(go(v, n, s)),
        Buf::U16(v) => Buf::U16(go(v, n, s)),
        Buf::F32(v) => Buf::F32(go(v, n, s)),
    }
}

fn select_channels(buf: Buf, n: usize, s: usize, keep: usize) -> Buf {
    fn go<T: Copy>(v: Vec<T>, n: usize, s: usize, keep: usize) -> Vec<T> {
        if v.len() < n * s {
            return v;
        }
        let mut out = Vec::with_capacity(n * keep);
        for px in v.chunks_exact(s).take(n) {
            out.extend_from_slice(&px[..keep]);
        }
        out
    }
    match buf {
        Buf::U8(v) => Buf::U8(go(v, n, s, keep)),
        Buf::U16(v) => Buf::U16(go(v, n, s, keep)),
        Buf::F32(v) => Buf::F32(go(v, n, s, keep)),
    }
}

#[cfg(test)]
mod stored_samples_tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};

    #[test]
    fn white_is_zero_view_only_changes_the_checked_tag_and_handles_partial_reads() {
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&262u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        let original = bytes.clone();
        let mut view = StoredSamples::new(&bytes).unwrap();
        let mut actual = vec![];
        view.read_to_end(&mut actual).unwrap();
        let mut expected = bytes.clone();
        expected[18] = 1;
        assert_eq!(actual, expected);
        view.seek(SeekFrom::Start(17)).unwrap();
        let mut crossing = [0; 4];
        view.read_exact(&mut crossing).unwrap();
        assert_eq!(&crossing, &expected[17..21]);
        assert_eq!(bytes, original, "source bytes are never rewritten");
        for cut in 8..22 {
            assert!(StoredSamples::new(&bytes[..cut]).is_err());
        }
        bytes[14..18].copy_from_slice(&2u32.to_le_bytes());
        assert!(StoredSamples::new(&bytes).is_err(), "photometric array cannot be patched as a scalar");
        let mut huge = b"II+\0\x08\0\0\0".to_vec();
        huge.extend_from_slice(&16u64.to_le_bytes());
        huge.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(StoredSamples::new(&huge).is_err(), "count overflow rejected before traversal");
    }
}
