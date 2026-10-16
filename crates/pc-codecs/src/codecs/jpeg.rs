//! JPEG: decode via `zune-jpeg` (gray, YCbCr, CMYK, YCCK), encode via
//! `jpeg-encoder` (gray, RGB, CMYK). Metadata (APP0 JFIF density, APP1
//! EXIF/XMP, APP2 multi-segment ICC, APP14 Adobe) is parsed by our own marker
//! scanner so behaviour does not depend on decoder internals.

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

use crate::Format;
use crate::error::CodecError;
use crate::fidelity::Plan;
use crate::image::{ChannelLayout, DecodeWarning, Image, Metadata, SampleType};
use crate::options::{EncodeOptions, Limits};
use crate::orientation::{encoded_exif, encoded_xmp};

const F: Format = Format::Jpeg;
const EXIF_HEADER: &[u8] = b"Exif\0\0";
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ICC_HEADER: &[u8] = b"ICC_PROFILE\0";

/// Header metadata collected by the one JPEG marker/entropy-data walker.
#[derive(Default, Debug)]
pub(crate) struct JpegMeta {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<String>,
    pub dpi: Option<(f32, f32)>,
    pub adobe_transform: Option<u8>,
    /// (width, height, components) from the first SOFn.
    pub frame: Option<(u32, u32, u8)>,
    precision: u8,
    sof: u8,
    first_scan_components: u8,
    exif_offset: usize,
    mpf: Option<(usize, Vec<u8>)>,
    end: DataEnd,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum DataEnd {
    #[default]
    Complete,
    Truncated,
    Empty,
}

/// Walk marker segments and entropy-coded scans together. Segment lengths prevent an embedded
/// preview's EOI from being mistaken for the main image's end; stuffed bytes, restart markers and
/// fill bytes inside scans do not terminate a scan. Only metadata preceding the first SOS is used.
pub(crate) fn scan_metadata(b: &[u8]) -> JpegMeta {
    let mut m = JpegMeta::default();
    if !b.starts_with(&[0xFF, 0xD8]) {
        m.end = DataEnd::Empty;
        return m;
    }
    let mut icc_parts: Vec<(u8, u8, &[u8])> = Vec::new();
    let mut i = 2usize;
    let mut saw_scan = false;
    let mut scan_data = false;
    let cut = |has_data: bool| if has_data { DataEnd::Truncated } else { DataEnd::Empty };
    m.end = loop {
        let Some(&byte) = b.get(i) else { break cut(scan_data) };
        if byte != 0xFF {
            // The Library historically accepted padding between header segments.
            if saw_scan {
                break DataEnd::Complete;
            }
            i += 1;
            continue;
        }
        let Some(&marker) = b.get(i + 1) else { break cut(scan_data) };
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD9 {
            break DataEnd::Complete;
        }
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let Some(len) = b.get(i + 2..i + 4).and_then(|s| <[u8; 2]>::try_from(s).ok()).map(u16::from_be_bytes) else {
            break cut(scan_data);
        };
        let len = usize::from(len);
        if len < 2 {
            break cut(scan_data);
        }
        let Some(seg) = b.get(i + 4..i + 2 + len) else { break cut(scan_data) };
        if !saw_scan {
            match marker {
                0xE0 if seg.len() >= 12 && seg.starts_with(b"JFIF\0") => {
                    let x = u16::from_be_bytes([seg[8], seg[9]]) as f32;
                    let y = u16::from_be_bytes([seg[10], seg[11]]) as f32;
                    if x > 0.0 && y > 0.0 {
                        m.dpi = match seg[7] {
                            1 => Some((x, y)),
                            2 => Some((x * 2.54, y * 2.54)),
                            _ => None,
                        };
                    }
                }
                0xE1 if seg.starts_with(EXIF_HEADER) && m.exif.is_none() => {
                    m.exif = Some(seg[EXIF_HEADER.len()..].to_vec());
                    m.exif_offset = i + 4 + EXIF_HEADER.len();
                }
                0xE1 if seg.starts_with(XMP_HEADER) && m.xmp.is_none() => {
                    m.xmp = Some(String::from_utf8_lossy(&seg[XMP_HEADER.len()..]).trim_end_matches('\0').to_owned());
                }
                0xE2 if seg.len() >= 14 && seg.starts_with(ICC_HEADER) => {
                    icc_parts.push((seg[12], seg[13], &seg[14..]));
                }
                0xE2 if seg.starts_with(b"MPF\0") && m.mpf.is_none() => {
                    m.mpf = Some((i + 8, seg[4..].to_vec()));
                }
                0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) && seg.len() >= 6 && m.frame.is_none() => {
                    let h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                    let w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                    m.frame = Some((w, h, seg[5]));
                    m.precision = seg[0];
                    m.sof = marker;
                }
                0xEE if seg.len() >= 12 && seg.starts_with(b"Adobe") => m.adobe_transform = Some(seg[11]),
                _ => {}
            }
        }
        i += 2 + len;
        if marker == 0xDA {
            if !saw_scan {
                m.first_scan_components = seg.first().copied().unwrap_or(0);
            }
            saw_scan = true;
            let start = i;
            while let Some(p) = b.get(i..).and_then(|r| r.iter().position(|&v| v == 0xFF)) {
                i += p;
                match b.get(i + 1) {
                    Some(0x00 | 0xD0..=0xD7) => i += 2,
                    Some(0xFF) => i += 1,
                    _ => break,
                }
            }
            // If the scan runs to EOF, account for every remaining byte before classifying it.
            if !b.get(i..).is_some_and(|r| r.first() == Some(&0xFF)) {
                i = b.len();
            }
            scan_data |= i > start;
        }
    };
    if !icc_parts.is_empty() {
        icc_parts.sort_by_key(|p| p.0);
        let total = icc_parts[0].1 as usize;
        let seqs_ok = icc_parts.len() == total && icc_parts.iter().enumerate().all(|(k, p)| p.0 as usize == k + 1 && p.1 as usize == total);
        if seqs_ok || icc_parts.len() == 1 {
            m.icc = Some(icc_parts.iter().flat_map(|p| p.2.iter().copied()).collect());
        }
    }
    m
}

fn err(e: impl std::fmt::Display) -> CodecError {
    CodecError::malformed(F, e)
}

pub(crate) fn dimensions(bytes: &[u8]) -> Result<(u32, u32), CodecError> {
    scan_metadata(bytes).frame.map(|(w, h, _)| (w, h)).ok_or_else(|| err("no frame header"))
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    decode_jpeg_scaled(bytes, limits, None).map(|(image, _)| image)
}

/// Decode JPEG samples in stored orientation, optionally using DCT scaling for a fit box. The
/// returned source dimensions always describe the original frame; callers can then fit exactly.
/// Limits apply to the original frame as well as the scaled output.
pub fn decode_jpeg_scaled(bytes: &[u8], limits: &Limits, max_size: Option<(u32, u32)>) -> Result<(Image, (u32, u32)), CodecError> {
    std::panic::catch_unwind(|| decode_jpeg_scaled_inner(bytes, limits, max_size)).unwrap_or_else(|_| Err(err("JPEG decoder panicked")))
}

fn decode_jpeg_scaled_inner(bytes: &[u8], limits: &Limits, max_size: Option<(u32, u32)>) -> Result<(Image, (u32, u32)), CodecError> {
    let meta = scan_metadata(bytes);
    if meta.end == DataEnd::Empty {
        return Err(err("the file ends before any image data"));
    }
    let (w, h, nc) = meta.frame.ok_or_else(|| err("no frame header"))?;
    limits.check_bytes(w, h, u64::from(nc.max(1)) * if meta.precision > 8 { 2 } else { 1 })?;
    let scale = max_size.filter(|&(mw, mh)| mw > 0 && mh > 0 && (w >= mw.saturating_mul(2) || h >= mh.saturating_mul(2)));
    // zune-jpeg 0.5 misdecodes subsampled non-interleaved sequential scans. jpeg-decoder also
    // supplies DCT scaling and 12/16-bit lossless JPEG. Full-resolution CMYK/YCCK keeps
    // the zune conversion, measured slightly more accurate on the shared colour test.
    let non_interleaved = matches!(meta.sof, 0xC0 | 0xC1) && meta.first_scan_components < nc;
    let mut img = if scale.is_some() || meta.precision > 8 || non_interleaved {
        decode_jpeg_decoder(bytes, limits, meta.precision, scale.map(|(mw, mh)| scaled_request(w, h, mw, mh)))?
    } else {
        decode_zune(bytes, limits)?
    };
    img.source_bit_depth = Some(meta.precision);
    img.source_float = Some(false);
    img.orientation = meta.exif.as_deref().map(lightcraft_tiff::exif::orientation).unwrap_or(1);
    img.icc = meta.icc;
    img.meta = Metadata { exif: meta.exif, xmp: meta.xmp, dpi: meta.dpi, ..Default::default() };
    if meta.end == DataEnd::Truncated {
        img.warnings.push(DecodeWarning::Truncated { format: F });
    }
    Ok((img, (w, h)))
}

fn decode_zune(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    let options = DecoderOptions::default().set_strict_mode(false).set_max_width(65535).set_max_height(65535);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    dec.decode_headers().map_err(err)?;
    let (w, h) = dec.dimensions().ok_or_else(|| err("no dimensions"))?;
    let (w, h) = (w as u32, h as u32);
    let input = dec.input_colorspace().ok_or_else(|| err("no colorspace"))?;
    let (out_cs, layout) = match input {
        ColorSpace::Luma | ColorSpace::LumaA => (ColorSpace::Luma, ChannelLayout::Gray),
        ColorSpace::CMYK => (ColorSpace::CMYK, ChannelLayout::Cmyk),
        ColorSpace::YCCK => (ColorSpace::YCCK, ChannelLayout::Cmyk),
        _ => (ColorSpace::RGB, ChannelLayout::Rgb),
    };
    limits.check(w, h, layout, SampleType::U8)?;
    dec.set_options(dec.options().jpeg_set_out_colorspace(out_cs));
    let mut px = dec.decode().map_err(err)?;
    let expected = w as usize * h as usize * layout.channels();
    if px.len() < expected {
        return Err(err("short pixel buffer"));
    }
    px.truncate(expected);
    match input {
        // Adobe-style CMYK is stored inverted (255 = no ink).
        ColorSpace::CMYK => px.iter_mut().for_each(|v| *v = 255 - *v),
        ColorSpace::YCCK => {
            for p in px.as_chunks_mut::<4>().0 {
                let (y, cb, cr) = (p[0] as f32, p[1] as f32 - 128.0, p[2] as f32 - 128.0);
                let c = y + 1.402 * cr;
                let m = y - 0.344_136 * cb - 0.714_136 * cr;
                let yy = y + 1.772 * cb;
                p[0] = c.round().clamp(0.0, 255.0) as u8;
                p[1] = m.round().clamp(0.0, 255.0) as u8;
                p[2] = yy.round().clamp(0.0, 255.0) as u8;
                p[3] = 255 - p[3];
            }
        }
        _ => {}
    }
    Image::from_raw(w, h, layout, SampleType::U8, px)
}

fn decode_jpeg_decoder(bytes: &[u8], limits: &Limits, precision: u8, scale: Option<(u16, u16)>) -> Result<Image, CodecError> {
    use jpeg_decoder::PixelFormat;
    let mut dec = jpeg_decoder::Decoder::new(bytes);
    dec.set_max_decoding_buffer_size(limits.alloc_usize());
    dec.read_info().map_err(err)?;
    if let Some((w, h)) = scale {
        dec.scale(w, h).map_err(err)?;
    }
    let info = dec.info().ok_or_else(|| err("no dimensions"))?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    let (layout, sample) = match info.pixel_format {
        PixelFormat::L8 => (ChannelLayout::Gray, SampleType::U8),
        PixelFormat::L16 => (ChannelLayout::Gray, SampleType::U16),
        PixelFormat::RGB24 => (ChannelLayout::Rgb, SampleType::U8),
        PixelFormat::CMYK32 => (ChannelLayout::Cmyk, SampleType::U8),
    };
    limits.check(w, h, layout, sample)?;
    // jpeg-decoder writes lossless L16 samples in native byte order, matching Image's storage.
    let mut px = dec.decode().map_err(err)?;
    if sample == SampleType::U16 && (9..16).contains(&precision) {
        // Lossless JPEG retains its declared sample range. Image's integer storage is normalized
        // to the whole u16 range; expand without losing the source precision recorded above.
        let max = (1u32 << precision) - 1;
        for bytes in px.as_chunks_mut::<2>().0 {
            let value = u32::from(u16::from_ne_bytes(*bytes));
            *bytes = (((value * u32::from(u16::MAX) + max / 2) / max).min(u32::from(u16::MAX)) as u16).to_ne_bytes();
        }
    }
    Image::from_raw(w, h, layout, sample, px)
}

fn scaled_request(w: u32, h: u32, mw: u32, mh: u32) -> (u16, u16) {
    let s = (mw as f64 / w as f64).min(mh as f64 / h as f64).min(1.0);
    let tw = ((w as f64 * s).ceil() as u32).clamp(1, u16::MAX as u32);
    let th = ((h as f64 * s).ceil() as u32).clamp(1, u16::MAX as u32);
    (tw as u16, th as u16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpegPreviewSource {
    ExifThumbnail,
    MpfPreview,
}

/// Borrowed embedded JPEG preview. Its samples and metadata can be decoded by the same shared
/// decoder; orientation and source dimensions belong to the main image.
#[derive(Debug, Clone, Copy)]
pub struct JpegPreview<'a> {
    pub bytes: &'a [u8],
    pub orientation: u16,
    pub source: JpegPreviewSource,
    pub source_width: u32,
    pub source_height: u32,
}

/// Select the smallest aspect-matching embedded preview covering `max_edge`, or the largest one
/// covering `min_edge`. EXIF thumbnail and MPF offsets are read by the shared TIFF directory API.
pub fn jpeg_embedded_preview(bytes: &[u8], max_edge: u32, min_edge: u32) -> Option<JpegPreview<'_>> {
    use super::tiff_ifd::File;
    let meta = scan_metadata(bytes);
    let (width, height, _) = meta.frame?;
    let orientation = meta.exif.as_deref().map(lightcraft_tiff::exif::orientation).unwrap_or(1);
    let mut candidates = Vec::new();
    if let Some(exif) = meta.exif.as_deref()
        && let Some(tiff) = File::parse(exif)
        && let Some(ifd0) = tiff.ifd(tiff.first_ifd)
        && ifd0.next != 0
        && ifd0.next != ifd0.at
        && let Some(ifd1) = tiff.ifd(ifd0.next)
        && let (Some(off), Some(len)) = (tiff.tag_uint(&ifd1, 0x0201), tiff.tag_uint(&ifd1, 0x0202))
        && let (Ok(off), Ok(len)) = (usize::try_from(off), usize::try_from(len))
        && off.checked_add(len).is_some_and(|end| end <= exif.len())
        && let Some(start) = meta.exif_offset.checked_add(off)
        && let Some(end) = start.checked_add(len)
        && let Some(data) = bytes.get(start..end)
    {
        candidates.push((data, JpegPreviewSource::ExifThumbnail));
    }
    if let Some((base, data)) = &meta.mpf
        && let Some(tiff) = File::parse(data)
        && let Some(ifd) = tiff.ifd(tiff.first_ifd)
        && let Some(list) = ifd.get(0xB002).and_then(|e| tiff.data(e))
    {
        for rec in list.as_chunks::<16>().0.iter().skip(1).take(15) {
            let read = |at: usize| {
                let a = [rec[at], rec[at + 1], rec[at + 2], rec[at + 3]];
                if tiff.le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
            };
            let (size, off) = (read(4) as usize, read(8) as usize);
            if size > 0
                && off > 0
                && let Some(start) = base.checked_add(off)
                && let Some(end) = start.checked_add(size)
                && let Some(data) = bytes.get(start..end)
            {
                candidates.push((data, JpegPreviewSource::MpfPreview));
            }
        }
    }
    let mut best: Option<(u32, &[u8], JpegPreviewSource)> = None;
    for (data, source) in candidates {
        let preview = scan_metadata(data);
        let Some((pw, ph, _)) = preview.frame else { continue };
        let long = pw.max(ph);
        let aspect_ok = width > 0 && height > 0 && ph > 0 && ((pw as f64 / ph as f64) / (width as f64 / height as f64) - 1.0).abs() < 0.02;
        let better = match best {
            None => true,
            Some((edge, ..)) if edge >= max_edge => long >= max_edge && long < edge,
            Some((edge, ..)) => long > edge,
        };
        if long >= min_edge && aspect_ok && better {
            best = Some((long, data, source));
        }
    }
    let (_, data, source) = best?;
    Some(JpegPreview { bytes: data, orientation, source, source_width: width, source_height: height })
}

pub(crate) fn encode(src: &Image, plan: Plan, opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    let sampling = if opts.jpeg_chroma_subsampling { crate::ChromaSubsampling::S420 } else { crate::ChromaSubsampling::S444 };
    encode_with_sampling(src, plan, opts, sampling)
}

pub(crate) fn encode_with_sampling(src: &Image, plan: Plan, opts: &EncodeOptions, sampling: crate::ChromaSubsampling) -> Result<Vec<u8>, CodecError> {
    let img = src.converted(plan.layout, plan.sample);
    let (w, h) = img.dimensions();
    let (w16, h16) = match (u16::try_from(w), u16::try_from(h)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            return Err(CodecError::encode(F, "JPEG dimensions are limited to 65535"));
        }
    };
    let ct = match img.layout() {
        ChannelLayout::Gray => jpeg_encoder::ColorType::Luma,
        ChannelLayout::Rgb => jpeg_encoder::ColorType::Rgb,
        ChannelLayout::Cmyk => jpeg_encoder::ColorType::Cmyk,
        l => return Err(CodecError::encode(F, format!("unsupported layout {l:?}"))),
    };
    let sampling = if img.layout() == ChannelLayout::Cmyk { crate::ChromaSubsampling::S444 } else { sampling };
    // RGB/gray use the established parallel baseline encoder on both export routes.
    if img.layout() != ChannelLayout::Cmyk && sampling != crate::ChromaSubsampling::S422 {
        let mut segments = Vec::new();
        let density = if opts.embed_metadata { img.meta.dpi } else { None };
        let density = density.filter(|(x, y)| *x >= 1.0 && *y >= 1.0);
        let d = density.map_or([0, 0, 1, 0, 1], |(x, y)| {
            let [xh, xl] = (x.round().min(65535.0) as u16).to_be_bytes();
            let [yh, yl] = (y.round().min(65535.0) as u16).to_be_bytes();
            [1, xh, xl, yh, yl]
        });
        segments.push((0xE0, [&b"JFIF\0\x01\x02"[..], &d, &[0, 0]].concat()));
        if opts.embed_metadata {
            if let Some(seg) = img.meta.exif.as_deref().and_then(|e| exif_segment_with(e, opts.keep_orientation)) {
                segments.push((0xE1, seg));
            }
            if let Some(seg) = img.meta.xmp.as_deref().and_then(|x| xmp_segment_with(x, opts.keep_orientation)) {
                segments.push((0xE1, seg));
            }
        }
        if opts.embed_icc
            && let Some(icc) = &img.icc
        {
            segments.extend(icc_segments(icc)?.into_iter().map(|seg| (0xE2, seg)));
        }
        return Ok(crate::jpeg_par::encode(img.data(), w as usize, h as usize, img.layout().channels(), opts.jpeg_quality, sampling, &segments));
    }
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, opts.jpeg_quality.clamp(1, 100));
    // CMYK is always written 4:4:4: subsampled 4-component JPEGs are not
    // decoded consistently across implementations.
    let factor = match sampling {
        crate::ChromaSubsampling::S444 => jpeg_encoder::SamplingFactor::R_4_4_4,
        crate::ChromaSubsampling::S422 => jpeg_encoder::SamplingFactor::R_4_2_2,
        crate::ChromaSubsampling::S420 => jpeg_encoder::SamplingFactor::R_4_2_0,
    };
    enc.set_sampling_factor(factor);
    let e = |e: jpeg_encoder::EncodingError| CodecError::encode(F, e);
    if opts.embed_metadata {
        if let Some((x, y)) = img.meta.dpi
            && x >= 1.0
            && y >= 1.0
        {
            enc.set_density(jpeg_encoder::PixelDensity {
                unit: jpeg_encoder::PixelDensityUnit::Inches,
                density: (x.round().min(65535.0) as u16, y.round().min(65535.0) as u16),
            });
        }
        // Metadata too large for its one segment is left out (and reported by the fidelity
        // warnings) rather than failing the whole export.
        if let Some(seg) = img.meta.exif.as_deref().and_then(|e| exif_segment_with(e, opts.keep_orientation)) {
            enc.add_app_segment(1, seg).map_err(e)?;
        }
        if let Some(seg) = img.meta.xmp.as_deref().and_then(|x| xmp_segment_with(x, opts.keep_orientation)) {
            enc.add_app_segment(1, seg).map_err(e)?;
        }
    }
    if opts.embed_icc
        && let Some(icc) = &img.icc
    {
        for segment in icc_segments(icc)? {
            enc.add_app_segment(2, segment).map_err(e)?;
        }
    }
    enc.encode(img.data(), w16, h16, ct).map_err(e)?;
    Ok(out)
}

/// ICC JPEG APP2 payloads (ICC embedding technical note); used by every JPEG mode.
pub(crate) fn icc_segments(icc: &[u8]) -> Result<Vec<Vec<u8>>, CodecError> {
    let count = icc.len().div_ceil(65_519);
    if count > 255 {
        return Err(CodecError::encode(F, "ICC profile too large for JPEG"));
    }
    Ok(icc.chunks(65_519).enumerate().map(|(i, chunk)| [&b"ICC_PROFILE\0"[..], &[i as u8 + 1, count as u8], chunk].concat()).collect())
}

/// The most data one APP segment holds (its 16-bit length counts its own two bytes).
const MAX_SEGMENT: usize = 65533;

/// XMP properties (local names, any namespace prefix) that only describe the layered document:
/// every document ever placed in it, the text of every type layer. A flat image doesn't need them,
/// and they alone can outgrow a segment.
const LAYERED_ONLY_XMP: [&str; 2] = ["DocumentAncestors", "TextLayers"];

/// The EXIF APP1 segment, or `None` when it doesn't fit in one segment.
pub(crate) fn exif_segment(exif: &[u8]) -> Option<Vec<u8>> {
    exif_segment_with(exif, false)
}
fn exif_segment_with(exif: &[u8], keep_orientation: bool) -> Option<Vec<u8>> {
    let seg = [EXIF_HEADER, encoded_exif(exif, keep_orientation).as_ref()].concat();
    (seg.len() <= MAX_SEGMENT).then_some(seg)
}

/// The XMP APP1 segment. When the packet doesn't fit in one segment, the layered-document
/// properties are left out; `None` when it still doesn't fit.
pub(crate) fn xmp_segment(xmp: &str) -> Option<Vec<u8>> {
    xmp_segment_with(xmp, false)
}
fn xmp_segment_with(xmp: &str, keep_orientation: bool) -> Option<Vec<u8>> {
    let xmp = encoded_xmp(xmp, keep_orientation);
    let seg = |x: &str| [XMP_HEADER, x.as_bytes()].concat();
    if XMP_HEADER.len() + xmp.len() <= MAX_SEGMENT {
        return Some(seg(&xmp));
    }
    let trimmed = LAYERED_ONLY_XMP.iter().fold(xmp.into_owned(), |x, name| remove_element(&x, name));
    (XMP_HEADER.len() + trimmed.len() <= MAX_SEGMENT).then(|| seg(&trimmed))
}

/// `xmp` without its `<prefix:name …>…</prefix:name>` and `<prefix:name …/>` elements. Unbalanced
/// markup is kept as is.
fn remove_element(xmp: &str, name: &str) -> String {
    let local = format!(":{name}");
    let mut out = String::with_capacity(xmp.len());
    let mut rest = xmp;
    while let Some(j) = rest.find(&local) {
        let head = rest.get(..j).unwrap_or_default();
        let prefix_len = head.bytes().rev().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')).count();
        let lt = j - prefix_len;
        let after = rest.get(j + local.len()..).unwrap_or_default();
        // An opening tag of exactly this name (not a closing tag, an attribute or a longer name).
        let opening = prefix_len > 0 && lt > 0 && rest.get(lt - 1..lt) == Some("<");
        if !opening || !after.starts_with(['>', '/', ' ', '\t', '\r', '\n']) {
            out.push_str(rest.get(..j + local.len()).unwrap_or_default());
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        let end = if after.get(..gt).is_some_and(|s| s.ends_with('/')) {
            gt + 1
        } else {
            let close = format!("</{}{local}>", rest.get(lt..j).unwrap_or_default());
            let Some(c) = after.find(&close) else { break };
            c + close.len()
        };
        out.push_str(rest.get(..lt - 1).unwrap_or_default());
        rest = after.get(end..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod j06_tests {
    use super::*;

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut out, 95);
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
        encoder.encode(&[50u8, 100, 150].repeat(w as usize * h as usize), w, h, jpeg_encoder::ColorType::Rgb).unwrap();
        out
    }

    fn segment(bytes: &mut Vec<u8>, marker: u8, payload: &[u8]) {
        bytes.extend_from_slice(&[0xFF, marker]);
        bytes.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(payload);
    }

    #[test]
    fn j06_jpeg_scaled_keeps_source_dimensions_and_guards_source_pixels() {
        assert_eq!(scaled_request(6000, 4000, 256, 256), (256, 171));
        let bytes = jpeg(96, 64);
        let (decoded, source) = decode_jpeg_scaled(&bytes, &Limits::default(), Some((24, 24))).unwrap();
        assert_eq!(source, (96, 64));
        assert_eq!(decoded.dimensions(), (24, 16));
        let limits = Limits { max_pixels: 1000, ..Default::default() };
        assert!(matches!(decode_jpeg_scaled(&bytes, &limits, Some((24, 24))), Err(CodecError::LimitExceeded(_))));
        let mut oracle = jpeg_decoder::Decoder::new(bytes.as_slice());
        oracle.scale(24, 16).unwrap();
        assert_eq!(decoded.data(), oracle.decode().unwrap());
    }

    #[test]
    fn j06_jpeg_embedded_exif_preview_preserves_main_orientation() {
        let preview = jpeg(16, 12);
        // IFD0 carries orientation 6 and points to IFD1; IFD1 describes the embedded preview.
        let mut exif = b"II\x2a\0\x08\0\0\0".to_vec();
        exif.extend_from_slice(&1u16.to_le_bytes());
        exif.extend_from_slice(&0x0112u16.to_le_bytes());
        exif.extend_from_slice(&3u16.to_le_bytes());
        exif.extend_from_slice(&1u32.to_le_bytes());
        exif.extend_from_slice(&6u32.to_le_bytes());
        exif.extend_from_slice(&26u32.to_le_bytes());
        exif.extend_from_slice(&2u16.to_le_bytes());
        for (tag, value) in [(0x0201u16, 56u32), (0x0202, preview.len() as u32)] {
            exif.extend_from_slice(&tag.to_le_bytes());
            exif.extend_from_slice(&4u16.to_le_bytes());
            exif.extend_from_slice(&1u32.to_le_bytes());
            exif.extend_from_slice(&value.to_le_bytes());
        }
        exif.extend_from_slice(&0u32.to_le_bytes());
        exif.extend_from_slice(&preview);
        let primary = jpeg(64, 48);
        let mut bytes = primary[..2].to_vec();
        segment(&mut bytes, 0xE1, &[EXIF_HEADER, &exif].concat());
        bytes.extend_from_slice(&primary[2..]);
        let chosen = jpeg_embedded_preview(&bytes, 16, 16).unwrap();
        assert_eq!(chosen.bytes, preview);
        assert_eq!(chosen.orientation, 6);
        assert_eq!(chosen.source, JpegPreviewSource::ExifThumbnail);
        assert_eq!((chosen.source_width, chosen.source_height), (64, 48));
        assert!(jpeg_embedded_preview(&bytes, 32, 32).is_none());
    }

    #[test]
    fn j06_jpeg_mpf_preview_selects_smallest_adequate_then_largest_fallback() {
        let primary = jpeg(96, 64);
        let previews = [jpeg(24, 16), jpeg(48, 32), jpeg(30, 30)];
        let count = previews.len() + 1;
        let mpf_len = 26 + count * 16;
        // MPF is inserted immediately after SOI; its TIFF header is at file offset 10.
        let primary_len = primary.len() + 8 + mpf_len;
        let mut mpf = b"II\x2a\0\x08\0\0\0".to_vec();
        mpf.extend_from_slice(&1u16.to_le_bytes());
        mpf.extend_from_slice(&0xB002u16.to_le_bytes());
        mpf.extend_from_slice(&7u16.to_le_bytes());
        mpf.extend_from_slice(&((count * 16) as u32).to_le_bytes());
        mpf.extend_from_slice(&26u32.to_le_bytes());
        mpf.extend_from_slice(&0u32.to_le_bytes());
        mpf.extend_from_slice(&[0; 16]); // Primary image is never a preview candidate.
        let mut at = primary_len;
        for preview in &previews {
            mpf.extend_from_slice(&0u32.to_le_bytes());
            mpf.extend_from_slice(&(preview.len() as u32).to_le_bytes());
            mpf.extend_from_slice(&((at - 10) as u32).to_le_bytes());
            mpf.extend_from_slice(&0u32.to_le_bytes());
            at += preview.len();
        }
        let mut bytes = primary[..2].to_vec();
        segment(&mut bytes, 0xE2, &[b"MPF\0".as_slice(), &mpf].concat());
        bytes.extend_from_slice(&primary[2..]);
        for preview in &previews {
            bytes.extend_from_slice(preview);
        }
        for (edge, expected) in [(20, 0), (40, 1), (80, 1)] {
            let chosen = jpeg_embedded_preview(&bytes, edge, 20).unwrap();
            assert_eq!(chosen.source, JpegPreviewSource::MpfPreview);
            assert_eq!(chosen.bytes, previews[expected]);
        }
    }

    #[test]
    fn j06_jpeg_scanner_ignores_embedded_end_markers_and_bad_icc_sets() {
        let main = jpeg(16, 16);
        let mut bytes = main[..2].to_vec();
        segment(&mut bytes, 0xE1, &[EXIF_HEADER, &[0xFF, 0xD9]].concat());
        segment(&mut bytes, 0xE2, &[ICC_HEADER, &[1, 2], b"one"].concat());
        segment(&mut bytes, 0xE2, &[ICC_HEADER, &[1, 2], b"duplicate"].concat());
        bytes.extend_from_slice(&main[2..main.len() - 2]);
        let metadata = scan_metadata(&bytes);
        assert_eq!(metadata.end, DataEnd::Truncated);
        assert!(metadata.icc.is_none());
        let decoded = decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(decoded.warnings, vec![DecodeWarning::Truncated { format: F }]);
    }

    #[test]
    fn j06_jpeg_cmyk_quality_matches_or_improves_previous_backend() {
        let w = 40usize;
        let h = 32usize;
        let source: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 5) as u8, (y * 6) as u8, ((x + y) * 3) as u8, 50]
            })
            .collect();
        for color in [jpeg_encoder::ColorType::Cmyk, jpeg_encoder::ColorType::CmykAsYcck] {
            let mut bytes = Vec::new();
            let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, 95);
            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
            encoder.encode(&source, w as u16, h as u16, color).unwrap();
            let shared = decode(&bytes, &Limits::default()).unwrap();
            // Reproduce the preceding pc-codecs CMYK/YCCK path for a small quality comparison.
            let mut old = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(&bytes), DecoderOptions::default().set_strict_mode(false));
            old.decode_headers().unwrap();
            let input = old.input_colorspace().unwrap();
            old.set_options(old.options().jpeg_set_out_colorspace(input));
            let mut old = old.decode().unwrap();
            if input == ColorSpace::CMYK {
                old.iter_mut().for_each(|v| *v = 255 - *v);
            } else {
                for p in old.as_chunks_mut::<4>().0 {
                    let (y, cb, cr) = (p[0] as f32, p[1] as f32 - 128.0, p[2] as f32 - 128.0);
                    p[0] = (y + 1.402 * cr).round().clamp(0.0, 255.0) as u8;
                    p[1] = (y - 0.344_136 * cb - 0.714_136 * cr).round().clamp(0.0, 255.0) as u8;
                    p[2] = (y + 1.772 * cb).round().clamp(0.0, 255.0) as u8;
                    p[3] = 255 - p[3];
                }
            }
            let mse = |pixels: &[u8]| source.iter().zip(pixels).map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2)).sum::<f64>() / source.len() as f64;
            let max_diff = shared.data().iter().zip(&old).map(|(&a, &b)| a.abs_diff(b)).max().unwrap();
            eprintln!("J06 {color:?}: shared MSE={:.6}, previous MSE={:.6}, max sample difference={max_diff}", mse(shared.data()), mse(&old));
            assert_eq!(shared.data(), old, "full-resolution CMYK/YCCK must preserve the more accurate path");
            let mut dct = jpeg_decoder::Decoder::new(bytes.as_slice());
            let dct = dct.decode().unwrap();
            eprintln!("J06 {color:?}: jpeg-decoder MSE={:.6}", mse(&dct));
            assert!(mse(shared.data()) <= mse(&dct), "shared CMYK quality regressed");
        }
    }

    #[test]
    fn j06_jpeg_lossless_12bit_expands_native_samples_to_normalized_u16() {
        // A valid SOF3 constant grayscale image. A zero difference reconstructs the initial
        // predictor 2^(precision-1), i.e. 2048 at 12 bits. ITU T.81 A.3.1 / H.1.2.1.
        let mut bytes = vec![0xFF, 0xD8];
        let mut huffman = vec![0, 1];
        huffman.extend_from_slice(&[0; 15]);
        huffman.push(0);
        segment(&mut bytes, 0xC4, &huffman);
        segment(&mut bytes, 0xC3, &[12, 0, 1, 0, 1, 1, 1, 0x11, 0]);
        segment(&mut bytes, 0xDA, &[1, 1, 0, 1, 0, 0]);
        bytes.extend_from_slice(&[0x7F, 0xFF, 0xD9]);
        let decoded = decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(decoded.layout(), ChannelLayout::Gray);
        assert_eq!(decoded.sample_type(), SampleType::U16);
        assert_eq!(decoded.to_u16_samples().unwrap(), [32776]);
        assert_eq!(decoded.source_bit_depth, Some(12));
    }
}
