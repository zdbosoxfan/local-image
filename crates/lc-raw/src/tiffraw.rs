//! Reading the pixel data of a TIFF IFD (strips or tiles; uncompressed, lossless JPEG, Deflate), in parallel.

use crate::unpack::*;
use crate::{MAX_SAMPLES, RawData, RawError, Result, ljpeg};
use lightcraft_tiff::image::{Chunk, ImageInfo, chunk_bytes};
use lightcraft_tiff::{ByteOrder, tags::compression as comp};
use rayon::prelude::*;
use std::ops::Range;

thread_local! {
    // Standalone callers retain the established defensive limit. The app can
    // supply its shared, live memory headroom without adding platform code here.
    static DECODE_BUDGET: std::cell::Cell<usize> = const { std::cell::Cell::new(1 << 30) };
}

pub(crate) fn memory_budget() -> usize {
    DECODE_BUDGET.get()
}

pub(crate) fn with_memory_budget<T>(budget: usize, run: impl FnOnce() -> T) -> T {
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            DECODE_BUDGET.set(self.0);
        }
    }
    let previous = DECODE_BUDGET.replace(budget);
    let _restore = Restore(previous);
    run()
}

/// Bit packing for uncompressed integer data with bit depths other than 8/16.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // vendor decoders use the other variants
pub enum Packing {
    /// TIFF standard: MSB-first, rows start on byte boundaries.
    Msb,
    /// LSB-first little-endian bit stream (some vendor raws), rows byte aligned.
    Lsb,
    /// Each sample stored in 16 bits in file byte order, regardless of `bits`.
    Word16,
    /// MSB-first bit stream inside 32-bit words stored in file byte order ([`unpack_words32_msb`]); rows start on a
    /// word boundary.
    Words32Msb,
}

enum ChunkPx {
    U16(Vec<u16>),
    F32(Vec<f32>),
}

/// The checks [`read_image`] makes before decoding anything (sample format, size limits, data
/// present); returns the number of samples.
pub fn check_image(data: &[u8], info: &ImageInfo) -> Result<usize> {
    let (w, h) = (info.width as usize, info.height as usize);
    let cpp = info.samples_per_pixel as usize;
    let total = w.checked_mul(h).and_then(|v| v.checked_mul(cpp)).ok_or(RawError::Limit("image too large"))?;
    if total > MAX_SAMPLES || total == 0 {
        return Err(RawError::Limit("image too large"));
    }
    let bits = info.bits() as u32;
    let float = info.sample_format == 3;
    if float && !matches!(bits, 16 | 24 | 32) {
        return Err(RawError::Unsupported(format!("{bits}-bit floating point samples")));
    }
    if !float && !(1..=16).contains(&bits) {
        return Err(RawError::Unsupported(format!("{bits}-bit integer samples")));
    }
    let chunks = info.chunks(data.len() as u64);
    if chunks.is_empty() {
        return Err(RawError::Corrupt("no image data chunks".into()));
    }
    // every visible sample needs a chunk: a short offsets table would leave zero-filled (black) regions
    let (across, down) = info.grid();
    let planes = if info.planar == 2 { cpp } else { 1 };
    let expected = (across as usize).saturating_mul(down as usize).saturating_mul(planes);
    if chunks.len() < expected {
        return Err(RawError::Corrupt(format!("{} image data chunks, expected {expected}", chunks.len())));
    }
    // Plausibility bound before allocating: no supported coding stores more than ~2000 samples per byte
    // (Deflate of constant data is the extreme), so tiny files cannot trigger huge allocations.
    let available: u64 = chunks.iter().map(|c| chunk_bytes(data, c).map_or(0, |s| s.len() as u64)).sum();
    if available.saturating_mul(2048) < total as u64 {
        return Err(RawError::Corrupt(format!("{available} bytes of image data cannot hold {total} samples")));
    }
    Ok(total)
}

/// Read the samples of `info` in `mode`: [`Mode::Header`] only checks them ([`check_image`]) and
/// returns no samples (floating-point data as an empty `F32`).
pub(crate) fn read_image_in(mode: crate::Mode, data: &[u8], info: &ImageInfo, order: ByteOrder, packing: Packing) -> Result<RawData> {
    match mode {
        crate::Mode::Full => read_image(data, info, order, packing),
        crate::Mode::Header => {
            check_image(data, info)?;
            // a lossless-JPEG layout the decoder rejects (subsampled components, e.g. Sony's
            // lossless M/S sizes) must fail here too, as the full decode will
            if info.compression == 7
                && let Some(src) = info.chunks(data.len() as u64).first().and_then(|c| chunk_bytes(data, c))
            {
                ljpeg::frame_info(src)?;
            }
            Ok(if info.sample_format == 3 { RawData::F32(Vec::new()) } else { RawData::U16(Vec::new()) })
        }
    }
}

// Format/allocation safety ceilings, independent of the caller's live allowance.
// These never choose a batch size: all batch storage comes from memory_budget().
const MAX_OUTPUT_BYTES: usize = 2 << 30;
const MAX_CHUNK_WORKING_BYTES: usize = 1 << 30;

fn output_bytes(total: usize, float: bool) -> Result<usize> {
    total.checked_mul(if float { 4 } else { 2 }).filter(|&n| n <= MAX_OUTPUT_BYTES).ok_or(RawError::Limit("RAW output memory budget"))
}

/// Bound live pixel/codec allocations, not compressed input (owned by the caller).
/// Mirrors decode_chunk/unpack_chunk, including Vec growth and predictor row copies.
fn chunk_working_bytes(c: &Chunk, info: &ImageInfo, packing: Packing, cpp: usize) -> Result<usize> {
    let estimate = || -> Option<usize> {
        let row = (c.width as usize).checked_mul(cpp)?;
        let samples = row.checked_mul(c.height as usize).filter(|&n| n <= MAX_SAMPLES)?;
        let float = info.sample_format == 3;
        let decoded = samples.checked_mul(if float { 4 } else { 2 })?;
        let row_bytes = if float {
            row.checked_mul((info.bits() as usize).div_ceil(8))?
        } else {
            match (info.bits(), packing) {
                (8, _) => row,
                (16, _) | (_, Packing::Word16) => row.checked_mul(2)?,
                (_, Packing::Words32Msb) => row.checked_mul(info.bits() as usize)?.div_ceil(32).checked_mul(4)?,
                _ => row.checked_mul(info.bits() as usize)?.div_ceil(8),
            }
        };
        let rows = if float {
            row_bytes.checked_mul(if matches!(info.predictor, 3 | 34894 | 34895) { 2 } else { 1 })?
        } else if packing == Packing::Words32Msb && info.bits() != 8 && info.bits() != 16 {
            // unpack_words32_msb collects byte groups through an unindexed iterator.
            row_bytes.checked_mul(3)?
        } else {
            0
        };
        let working = match info.compression {
            comp::NONE => decoded.checked_add(rows)?,
            comp::ADOBE_DEFLATE | comp::DEFLATE => {
                // miniz grows geometrically; old + new buffers can coexist during realloc.
                // Three times the inflated limit also covers retained Vec capacity.
                let inflated = row_bytes.checked_mul(c.height as usize)?.checked_mul(3)?;
                decoded.checked_add(inflated)?.checked_add(rows)?.checked_add(std::mem::size_of::<miniz_oxide::inflate::core::DecompressorOxide>())?
            }
            comp::JPEG => {
                // ljpeg permits a frame up to max(2*n, 65536) samples and may resize it.
                // Include Huffman tables and header Vec growth on malformed repeated SOFs.
                samples
                    .checked_mul(6)?
                    .max(samples.checked_mul(2)?.max(1 << 16).checked_mul(2)?)
                    .checked_add(1 << 14)?
                    .checked_add(usize::try_from(c.len).ok()?.checked_mul(2)?)?
            }
            comp::LOSSY_JPEG => {
                // zune supports progressive/multiscan input too: padded coefficient planes,
                // MCU/upsampler rows, output and copies of embedded JPEG metadata.
                let width = (c.width as usize).checked_add(31)? / 32 * 32;
                let height = (c.height as usize).checked_add(31)? / 32 * 32;
                decoded
                    .checked_add(width.checked_mul(height)?.checked_mul(128 + 3)?)?
                    .checked_add(width.checked_mul(4096)?)?
                    .checked_add(usize::try_from(c.len).ok()?.checked_mul(8)?)?
                    .checked_add(1 << 16)?
            }
            _ => decoded.checked_add(rows)?,
        };
        (working <= MAX_CHUNK_WORKING_BYTES).then_some(working)
    };
    estimate().ok_or(RawError::Limit("RAW chunk memory budget"))
}

/// Plan all chunks before allocating output. A chunk must fit by itself: unlike the
/// fixed-budget PR, an oversized strip must never silently exceed the scoped allowance.
fn chunk_batches(chunks: &[Chunk], info: &ImageInfo, packing: Packing, cpp: usize, budget: usize) -> Result<Vec<Range<usize>>> {
    let mut batches = Vec::new();
    let (mut start, mut bytes) = (0, 0usize);
    for (i, c) in chunks.iter().enumerate() {
        let working = chunk_working_bytes(c, info, packing, cpp)?;
        if working > budget {
            return Err(RawError::Limit("RAW import memory budget"));
        }
        // Subtraction avoids overflow even with a usize::MAX caller allowance.
        if working > budget - bytes {
            batches.push(start..i);
            start = i;
            bytes = 0;
        }
        bytes += working;
    }
    if start < chunks.len() {
        batches.push(start..chunks.len());
    }
    Ok(batches)
}

fn output_buffer<T: Default + Clone>(total: usize) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(total).map_err(|_| RawError::Limit("RAW output allocation"))?;
    out.resize(total, T::default());
    Ok(out)
}

/// Decode chunks in parallel batches bounded by the caller's adaptive memory allowance.
/// When all chunks fit, this is one fully parallel batch; tight budgets can admit one at a time.
pub fn read_image(data: &[u8], info: &ImageInfo, order: ByteOrder, packing: Packing) -> Result<RawData> {
    let (w, h) = (info.width as usize, info.height as usize);
    let total = check_image(data, info)?;
    let cpp = info.samples_per_pixel as usize;
    let bits = info.bits() as u32;
    let float = info.sample_format == 3;
    let mut chunks = info.chunks(data.len() as u64);
    for c in &mut chunks {
        c.len = c.len.min((data.len() as u64).saturating_sub(c.offset));
    }
    let planar = info.planar == 2 && cpp > 1;
    let ccpp = if planar { 1 } else { cpp };
    // Capture the thread-local scope here, before Rayon dispatches on worker threads.
    let batch_bytes = memory_budget().checked_sub(output_bytes(total, float)?).ok_or(RawError::Limit("RAW import memory budget"))?;
    let batches = chunk_batches(&chunks, info, packing, ccpp, batch_bytes)?;
    let mut out_u16 = if float { Vec::new() } else { output_buffer::<u16>(total)? };
    let mut out_f32 = if float { output_buffer::<f32>(total)? } else { Vec::new() };
    let decode = |c: &Chunk| decode_chunk(data, info, c, order, packing, ccpp, bits, float).map(|p| (*c, p));
    let mut ok = 0usize;
    let mut first_err = None;
    for batch in batches {
        // Avoid dispatch overhead at the one-chunk minimum while retaining identical order.
        let decoded: Vec<_> =
            if batch.len() == 1 { chunks[batch].iter().map(decode).collect() } else { chunks[batch].par_iter().map(decode).collect() };
        for r in decoded {
            let (c, px) = match r {
                Ok(v) => v,
                Err(e) => {
                    first_err.get_or_insert(e);
                    continue;
                }
            };
            ok += 1;
            let (cw, ch) = (c.width as usize, c.height as usize);
            let (x0, y0) = (c.x as usize, c.y as usize);
            if x0 >= w || y0 >= h {
                continue;
            }
            let copy_w = cw.min(w - x0);
            let copy_h = ch.min(h - y0);
            for y in 0..copy_h {
                if !planar {
                    // Interleaved tiles/strips already have the destination channel order.
                    // Copy an entire clipped row rather than checking every sample separately.
                    let src = y * cw * cpp;
                    let dst = ((y0 + y) * w + x0) * cpp;
                    let len = copy_w * cpp;
                    match &px {
                        ChunkPx::U16(v) => {
                            if let (Some(out), Some(row)) = (out_u16.get_mut(dst..dst + len), v.get(src..src + len)) {
                                out.copy_from_slice(row);
                            }
                        }
                        ChunkPx::F32(v) => {
                            if let (Some(out), Some(row)) = (out_f32.get_mut(dst..dst + len), v.get(src..src + len)) {
                                out.copy_from_slice(row);
                            }
                        }
                    }
                    continue;
                }
                for x in 0..copy_w {
                    for s in 0..ccpp {
                        let src = (y * cw + x) * ccpp + s;
                        let dst_s = if planar { c.plane as usize } else { s };
                        if dst_s >= cpp {
                            continue;
                        }
                        let dst = ((y0 + y) * w + x0 + x) * cpp + dst_s;
                        match &px {
                            ChunkPx::U16(v) => {
                                if let (Some(o), Some(&s)) = (out_u16.get_mut(dst), v.get(src)) {
                                    *o = s;
                                }
                            }
                            ChunkPx::F32(v) => {
                                if let (Some(o), Some(&s)) = (out_f32.get_mut(dst), v.get(src)) {
                                    *o = s;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if ok == 0 {
        return Err(first_err.unwrap_or_else(|| RawError::Corrupt("no decodable chunks".into())));
    }
    Ok(if float { RawData::F32(out_f32) } else { RawData::U16(out_u16) })
}

#[allow(clippy::too_many_arguments)]
fn decode_chunk(data: &[u8], info: &ImageInfo, c: &Chunk, order: ByteOrder, packing: Packing, cpp: usize, bits: u32, float: bool) -> Result<ChunkPx> {
    let src = chunk_bytes(data, c).ok_or_else(|| RawError::Corrupt("chunk offset past end of file".into()))?;
    let (cw, ch) = (c.width as usize, c.height as usize);
    // rows of the chunk inside the image (an edge chunk may hang past the bottom)
    let vh = ch.min(info.height.saturating_sub(c.y) as usize);
    let n = cw.checked_mul(ch).and_then(|v| v.checked_mul(cpp)).ok_or(RawError::Limit("chunk too large"))?;
    if n > MAX_SAMPLES {
        return Err(RawError::Limit("chunk too large"));
    }
    match info.compression {
        comp::NONE => {
            // uncompressed: missing rows are filled with zeros, but a chunk holding a small fraction of what its
            // samples need is no picture: refused before the nominal chunk is allocated
            let need = (n as u64).saturating_mul(u64::from(bits)) / 8;
            if (src.len() as u64).saturating_mul(64) < need {
                return Err(RawError::Corrupt(format!("an uncompressed chunk of {n} samples has only {} bytes", src.len())));
            }
            unpack_chunk(src, order, packing, cw, ch, cpp, bits, float, 1)
        }
        comp::JPEG => {
            let f = ljpeg::decode(src, n.saturating_mul(2).max(1 << 16))?;
            if f.data.len() < n {
                // Some writers encode edge tiles smaller than nominal: accept a frame that covers the rows inside the image.
                if f.data.is_empty() || f.data.len() % (cw * cpp) != 0 || f.data.len() / (cw * cpp) < vh {
                    return Err(RawError::Corrupt(format!("lossless JPEG tile has {} samples, expected {n}", f.data.len())));
                }
            }
            let mut v = f.data;
            v.resize(n, 0);
            Ok(ChunkPx::U16(v))
        }
        comp::LOSSY_JPEG => {
            // lossy DNG: each chunk is a baseline (DCT) JPEG of 8-bit samples
            let (px, jw, jh, jc) = lossy_jpeg(src, cpp, cw, ch)?;
            let mut v = vec![0u16; n];
            for y in 0..ch.min(jh) {
                for x in 0..cw.min(jw) {
                    for k in 0..cpp {
                        v[(y * cw + x) * cpp + k] = px[(y * jw + x) * jc + k.min(jc - 1)] as u16;
                    }
                }
            }
            Ok(ChunkPx::U16(v))
        }
        comp::ADOBE_DEFLATE | comp::DEFLATE => {
            let bytes_per = if float { bits.div_ceil(8) as usize } else { 0 };
            let row_bytes = if float { cw * cpp * bytes_per } else { (cw * cpp * bits as usize).div_ceil(8) };
            let raw = inflate(src, row_bytes * ch)?;
            unpack_chunk(&raw, order, packing, cw, ch, cpp, bits, float, info.predictor)
        }
        other => Err(RawError::Unsupported(format!("TIFF compression {other}"))),
    }
}

/// Decode one baseline JPEG chunk to 8-bit samples: (samples, width, height, channels).
fn lossy_jpeg(src: &[u8], cpp: usize, tile_width: usize, tile_height: usize) -> Result<(Vec<u8>, usize, usize, usize)> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let cs = if cpp == 1 { ColorSpace::Luma } else { ColorSpace::RGB };
    let opts = DecoderOptions::default().set_max_width(tile_width).set_max_height(tile_height).set_strict_mode(false).jpeg_set_out_colorspace(cs);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(src), opts);
    d.decode_headers().map_err(|e| RawError::Corrupt(format!("lossy JPEG tile header: {e}")))?;
    let header = d.info().ok_or_else(|| RawError::Corrupt("missing JPEG tile header".into()))?;
    if header.width as usize > tile_width || header.height as usize > tile_height {
        return Err(RawError::Limit("JPEG dimensions exceed tile"));
    }
    let px = d.decode().map_err(|e| RawError::Corrupt(format!("lossy JPEG tile: {e}")))?;
    let info = d.info().ok_or_else(|| RawError::Corrupt("lossy JPEG tile without a header".into()))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let c = if cpp == 1 { 1 } else { 3 };
    if w == 0 || h == 0 || px.len() < w * h * c {
        return Err(RawError::Corrupt("lossy JPEG tile: short output".into()));
    }
    Ok((px, w, h, c))
}

#[allow(clippy::too_many_arguments)]
fn unpack_chunk(
    src: &[u8],
    order: ByteOrder,
    packing: Packing,
    cw: usize,
    ch: usize,
    cpp: usize,
    bits: u32,
    float: bool,
    predictor: u16,
) -> Result<ChunkPx> {
    let row_n = cw * cpp;
    let (factor, fp) = match predictor {
        1 => (0, false),
        2 => (1, false),
        3 => (1, true),
        34892 => (2, false),
        34893 => (4, false),
        34894 => (2, true),
        34895 => (4, true),
        p => return Err(RawError::Unsupported(format!("predictor {p}"))),
    };
    if float {
        let bp = bits.div_ceil(8) as usize;
        let row_bytes = row_n * bp;
        let mut out = vec![0f32; row_n * ch];
        let mut rowbuf = vec![0u8; row_bytes];
        for y in 0..ch {
            let s = src.get(y * row_bytes..).unwrap_or(&[]);
            let len = s.len().min(row_bytes);
            rowbuf[..len].copy_from_slice(&s[..len]);
            rowbuf[len..].fill(0);
            let big = if fp {
                undo_float_predictor(&mut rowbuf, row_n, bp, cpp * factor);
                true
            } else {
                if factor > 0 {
                    return Err(RawError::Unsupported("integer predictor on float data".into()));
                }
                order == ByteOrder::Big
            };
            for (i, o) in out[y * row_n..(y + 1) * row_n].iter_mut().enumerate() {
                let b = &rowbuf[i * bp..(i + 1) * bp];
                *o = match (bp, big) {
                    (2, true) => half::f16::from_be_bytes([b[0], b[1]]).to_f32(),
                    (2, false) => half::f16::from_le_bytes([b[0], b[1]]).to_f32(),
                    (3, true) => f24_to_f32(u32::from_be_bytes([0, b[0], b[1], b[2]])),
                    (3, false) => f24_to_f32(u32::from_le_bytes([b[0], b[1], b[2], 0])),
                    (4, true) => f32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                };
            }
        }
        return Ok(ChunkPx::F32(out));
    }
    if fp {
        return Err(RawError::Unsupported("floating-point predictor on integer data".into()));
    }
    let mut out = vec![0u16; row_n * ch];
    let (row_bytes, kind) = match (bits, packing) {
        (8, _) => (row_n, 0),
        (16, _) | (_, Packing::Word16) => (row_n * 2, 1),
        (_, Packing::Msb) => ((row_n * bits as usize).div_ceil(8), 2),
        (_, Packing::Lsb) => ((row_n * bits as usize).div_ceil(8), 3),
        (_, Packing::Words32Msb) => ((row_n * bits as usize).div_ceil(32) * 4, 4),
    };
    for y in 0..ch {
        let s = src.get(y * row_bytes..).unwrap_or(&[]);
        let s = &s[..s.len().min(row_bytes)];
        let row = &mut out[y * row_n..(y + 1) * row_n];
        match kind {
            0 => row.iter_mut().zip(s).for_each(|(o, &b)| *o = b as u16),
            1 => read_u16s(s, order, row),
            2 => unpack_msb(s, bits, row),
            4 => unpack_words32_msb(s, order, bits, row),
            _ => unpack_lsb(s, bits, row),
        }
        if factor > 0 {
            if bits == 8 {
                for i in cpp * factor..row.len() {
                    row[i] = (row[i] as u8).wrapping_add(row[i - cpp * factor] as u8) as u16;
                }
            } else {
                undo_diff_u16(row, cpp * factor);
            }
        }
    }
    Ok(ChunkPx::U16(out))
}

#[cfg(test)]
mod lossy_tests {
    use super::*;
    use lightcraft_tiff::image::Layout;

    /// A lossy-DNG-style image: two 16×8 tiles, each a baseline JPEG, the right one cut short
    /// by the image edge.
    #[test]
    fn lossy_jpeg_tiles_decode() {
        let (tw, th) = (16usize, 8usize);
        let tile = |shade: u8| {
            let px: Vec<u8> = (0..tw * th).flat_map(|i| [shade, (i % tw * 15) as u8, 200]).collect();
            let mut out = Vec::new();
            jpeg_encoder::Encoder::new(&mut out, 100).encode(&px, tw as u16, th as u16, jpeg_encoder::ColorType::Rgb).unwrap();
            out
        };
        let (a, b) = (tile(40), tile(220));
        let mut file = vec![0u8; 8];
        let oa = file.len() as u64;
        file.extend_from_slice(&a);
        let ob = file.len() as u64;
        file.extend_from_slice(&b);
        let info = ImageInfo {
            width: 24,
            height: 8,
            bits_per_sample: vec![8, 8, 8],
            samples_per_pixel: 3,
            compression: comp::LOSSY_JPEG,
            photometric: 34892,
            planar: 1,
            predictor: 1,
            sample_format: 1,
            new_subfile_type: 0,
            layout: Layout::Tiles { tile_width: tw as u32, tile_height: th as u32 },
            offsets: vec![oa, ob],
            byte_counts: vec![a.len() as u64, b.len() as u64],
        };
        let RawData::U16(v) = read_image(&file, &info, ByteOrder::Little, Packing::Msb).unwrap() else { panic!("integer samples") };
        assert_eq!(v.len(), 24 * 8 * 3);
        let px = |x: usize, y: usize| &v[(y * 24 + x) * 3..(y * 24 + x) * 3 + 3];
        assert!((px(2, 3)[0] as i32 - 40).abs() <= 3 && (px(2, 3)[2] as i32 - 200).abs() <= 3, "{:?}", px(2, 3));
        assert!((px(20, 3)[0] as i32 - 220).abs() <= 3, "second tile: {:?}", px(20, 3));
        assert!((px(10, 5)[1] as i32 - 150).abs() <= 6, "green ramp: {:?}", px(10, 5));
    }

    fn strip_info(compression: u16, offsets: Vec<u64>, byte_counts: Vec<u64>) -> ImageInfo {
        ImageInfo {
            width: 4,
            height: 4,
            bits_per_sample: vec![8],
            samples_per_pixel: 1,
            compression,
            photometric: 1,
            planar: 1,
            predictor: 1,
            sample_format: 1,
            new_subfile_type: 0,
            layout: Layout::Strips { rows_per_strip: 2 },
            offsets,
            byte_counts,
        }
    }

    /// A missing strip is an error, not a black band in a "successful" image.
    #[test]
    fn missing_chunks_are_errors() {
        let file = vec![9u8; 8 + 8];
        // both strips present
        let ok = strip_info(comp::NONE, vec![0, 8], vec![8, 8]);
        assert!(read_image(&file, &ok, ByteOrder::Little, Packing::Msb).is_ok());
        // the second strip has no offset
        let missing = strip_info(comp::NONE, vec![0], vec![8]);
        assert!(read_image(&file, &missing, ByteOrder::Little, Packing::Msb).is_err());
        // the header-only read rejects it too, as the full decode will
        assert!(read_image_in(crate::Mode::Header, &file, &missing, ByteOrder::Little, Packing::Msb).is_err());
        // the second strip starts past the end of the file
    }

    /// A lossless-JPEG tile may be coded only as tall as the part inside the image, never shorter.
    #[test]
    fn short_lossless_jpeg_tiles_are_errors() {
        let tile = |rows: usize| ljpeg::encode(&vec![300u16; 8 * rows], 8, rows, 1, 12, 1, 0);
        let decode = |height: u32, rows: usize| {
            let t = tile(rows);
            let info = ImageInfo {
                width: 8,
                height,
                bits_per_sample: vec![12],
                samples_per_pixel: 1,
                compression: comp::JPEG,
                photometric: 32803,
                planar: 1,
                predictor: 1,
                sample_format: 1,
                new_subfile_type: 0,
                layout: Layout::Tiles { tile_width: 8, tile_height: 8 },
                offsets: vec![0],
                byte_counts: vec![t.len() as u64],
            };
            read_image(&t, &info, ByteOrder::Little, Packing::Msb)
        };
        // an edge tile coded at its in-image height, and a full tile
        assert_eq!(decode(6, 6), Ok(RawData::U16(vec![300; 8 * 6])));
        assert_eq!(decode(8, 8), Ok(RawData::U16(vec![300; 8 * 8])));
        // half of the visible rows are missing
        assert!(decode(8, 4).is_err());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod hostile_tile_tests {
    use super::*;
    #[test]
    fn jpeg_dimensions_cannot_exceed_dng_tile() {
        let mut bytes = Vec::new();
        jpeg_encoder::Encoder::new(&mut bytes, 90).encode(&[128; 16 * 16 * 3], 16, 16, jpeg_encoder::ColorType::Rgb).unwrap();
        assert!(lossy_jpeg(&bytes, 3, 1, 1).is_err());
        assert!(lossy_jpeg(&bytes, 3, 16, 16).is_ok());
    }
}

#[cfg(test)]
mod sony_dji_memory_tests {
    use super::*;
    use lightcraft_tiff::image::Layout;
    #[test]
    fn sony_dji_chunk_streaming_preserves_rgb_and_scoped_memory_limits() {
        let words: Vec<u16> = (0..5 * 7 * 3).map(|i| 100 + i as u16 * 3).collect();
        let data: Vec<_> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        let info = ImageInfo {
            width: 5,
            height: 7,
            bits_per_sample: vec![16; 3],
            samples_per_pixel: 3,
            compression: comp::NONE,
            photometric: 34892,
            planar: 1,
            predictor: 1,
            sample_format: 1,
            new_subfile_type: 0,
            layout: Layout::Strips { rows_per_strip: 2 },
            offsets: vec![0, 60, 120, 180],
            byte_counts: vec![60, 60, 60, 30],
        };
        let read = || read_image(&data, &info, ByteOrder::Little, Packing::Word16);
        let parallel = read().unwrap();
        // Output210bytes+one60byte strip fits. Retaining all strips would need420.
        let streamed = with_memory_budget(280, read).unwrap();
        assert_eq!(parallel, streamed);
        assert_eq!(streamed, RawData::U16(words));
        assert!(matches!(with_memory_budget(269, read), Err(RawError::Limit(_))));
        assert_eq!(read().unwrap(), parallel, "scoped budget must be restored");
        let old = DECODE_BUDGET.get();
        let _ = std::panic::catch_unwind(|| with_memory_budget(12, || panic!("scope unwinding")));
        assert_eq!(DECODE_BUDGET.get(), old);
    }
}

#[cfg(test)]
mod batch_tests {
    // Ported from PR #13; the batch allowance now comes from the caller scope.
    use super::*;
    use lightcraft_tiff::image::Layout;

    fn tiled_info(width: u32, height: u32, tile: u32, planar: bool, float: bool) -> ImageInfo {
        let count = width.div_ceil(tile) as usize * height.div_ceil(tile) as usize * if planar { 3 } else { 1 };
        ImageInfo {
            width,
            height,
            bits_per_sample: vec![if float { 32 } else { 16 }; 3],
            samples_per_pixel: 3,
            compression: comp::DEFLATE,
            photometric: 34892,
            planar: if planar { 2 } else { 1 },
            predictor: if float { 3 } else { 1 },
            sample_format: if float { 3 } else { 1 },
            new_subfile_type: 0,
            layout: Layout::Tiles { tile_width: tile, tile_height: tile },
            offsets: vec![0; count],
            byte_counts: vec![4096; count],
        }
    }

    #[test]
    fn hundred_megapixel_rgb_hdr_has_bounded_tile_batches() {
        // Only plan this legal image; exercising the size guard must not allocate 1.2 GB in CI.
        let info = tiled_info(10_000, 10_000, 256, false, true);
        let total = check_image(&[0; 4096], &info).unwrap();
        assert_eq!(total, 300_000_000);
        let output = output_bytes(total, true).unwrap();
        assert!(output > 1 << 30); // the former combined 1 GiB guard rejected this image
        let chunks = info.chunks(4096);
        let budget = output / 4;
        let batches = chunk_batches(&chunks, &info, Packing::Msb, 3, budget).unwrap();
        assert!(batches.len() > 1);
        assert_eq!(batches.first().unwrap().start, 0);
        assert_eq!(batches.last().unwrap().end, chunks.len());
        for batch in batches {
            assert!(chunks[batch].iter().map(|c| chunk_working_bytes(c, &info, Packing::Msb, 3).unwrap()).sum::<usize>() <= budget);
        }
    }

    /// A small image whose uncompressed tile claims to be enormous (16384 x 16384 nominal, a few bytes of data) is
    /// refused before that tile is allocated; an edge tile stored at its full nominal size decodes.
    #[test]
    fn a_huge_nominal_uncompressed_tile_with_little_data_is_refused() {
        let info = |tile: u32, bytes: u64| ImageInfo {
            width: 8,
            height: 8,
            bits_per_sample: vec![16],
            samples_per_pixel: 1,
            compression: comp::NONE,
            photometric: 32803,
            planar: 1,
            predictor: 1,
            sample_format: 1,
            new_subfile_type: 0,
            layout: Layout::Tiles { tile_width: tile, tile_height: tile },
            offsets: vec![0],
            byte_counts: vec![bytes],
        };
        let small = vec![1u8; 2 * 16 * 16];
        assert!(read_image(&small, &info(16, small.len() as u64), ByteOrder::Little, Packing::Msb).is_ok());
        let data = vec![1u8; 4096];
        assert!(matches!(read_image(&data, &info(16384, data.len() as u64), ByteOrder::Little, Packing::Msb), Err(RawError::Corrupt(_))));
    }

    #[test]
    fn excessive_allocations_are_rejected_before_decoding() {
        assert!(output_bytes(MAX_OUTPUT_BYTES / 4 + 1, true).is_err());
        assert!(output_bytes(usize::MAX, true).is_err());
        assert!(output_buffer::<u8>(usize::MAX).is_err());
        let budget = memory_budget();
        let info = tiled_info(10_000, 10_000, 10_000, false, true);
        let chunks = info.chunks(4096);
        assert!(chunk_batches(&chunks, &info, Packing::Msb, 3, budget).is_err());
        // An overlarge nominal tile is rejected even when the actual image is tiny.
        let info = tiled_info(1, 1, u32::MAX, false, true);
        assert!(read_image(&[0; 4096], &info, ByteOrder::Little, Packing::Msb).is_err());
    }

    #[test]
    fn multiple_batches_preserve_float_predictor_planar_and_edge_pixels() {
        for planar in [false, true] {
            for float in [false, true] {
                let mut info = tiled_info(5, 3, 2, planar, float);
                let mut file = Vec::new();
                for chunk in info.chunks(0) {
                    let channels = if planar { 1 } else { 3 };
                    let mut encoded = Vec::new();
                    for y in 0..2 {
                        let values: Vec<u16> = (0..2)
                            .flat_map(|x| {
                                (0..channels).map(move |s| {
                                    let channel = if planar { chunk.plane as usize } else { s };
                                    ((chunk.y as usize + y) * 100 + (chunk.x as usize + x) * 10 + channel) as u16
                                })
                            })
                            .collect();
                        if float {
                            // TIFF floating predictor: byte planes followed by horizontal differencing.
                            let mut row = vec![0u8; values.len() * 4];
                            for (i, &value) in values.iter().enumerate() {
                                for (b, byte) in (value as f32 / 8.0).to_be_bytes().into_iter().enumerate() {
                                    row[b * values.len() + i] = byte;
                                }
                            }
                            for i in (channels..row.len()).rev() {
                                row[i] = row[i].wrapping_sub(row[i - channels]);
                            }
                            encoded.extend(row);
                        } else {
                            encoded.extend(values.iter().flat_map(|v| v.to_le_bytes()));
                        }
                    }
                    let z = miniz_oxide::deflate::compress_to_vec_zlib(&encoded, 6);
                    info.offsets[chunk.index] = file.len() as u64;
                    info.byte_counts[chunk.index] = z.len() as u64;
                    file.extend(z);
                }
                let chunks = info.chunks(file.len() as u64);
                // Force exactly two chunks per batch to exercise copying between batches.
                let budget = chunk_working_bytes(&chunks[0], &info, Packing::Msb, if planar { 1 } else { 3 }).unwrap() * 2;
                let decoded = with_memory_budget(output_bytes(5 * 3 * 3, float).unwrap() + budget, || {
                    read_image(&file, &info, ByteOrder::Little, Packing::Msb)
                })
                .unwrap();
                let expected: Vec<u16> = (0..3).flat_map(|y| (0..5).flat_map(move |x| (0..3).map(move |s| y * 100 + x * 10 + s))).collect();
                assert_eq!(decoded, if float { RawData::F32(expected.iter().map(|&v| v as f32 / 8.0).collect()) } else { RawData::U16(expected) });
                assert!(chunk_batches(&chunks, &info, Packing::Msb, if planar { 1 } else { 3 }, budget).unwrap().len() > 1);
            }
        }
    }

    #[test]
    fn adaptive_batches_pack_variable_strips_and_reject_oversized_chunks() {
        let mut info = tiled_info(5, 7, 2, false, false);
        info.compression = comp::NONE;
        info.layout = Layout::Strips { rows_per_strip: 2 };
        info.offsets = vec![0, 60, 120, 180];
        info.byte_counts = vec![60, 60, 60, 30];
        let chunks = info.chunks(210);
        // A final short strip can share the preceding batch; the planner covers all chunks once.
        assert_eq!(chunk_batches(&chunks, &info, Packing::Msb, 3, 90).unwrap(), vec![0..1, 1..2, 2..4]);
        assert_eq!(chunk_batches(&chunks, &info, Packing::Msb, 3, 210).unwrap(), vec![0..4]);
        assert_eq!(chunk_batches(&chunks, &info, Packing::Msb, 3, usize::MAX).unwrap(), vec![0..4]);
        assert!(chunk_batches(&chunks, &info, Packing::Msb, 3, 59).is_err());
        assert!(chunk_batches(&chunks, &info, Packing::Msb, 3, 0).is_err());
        assert!(chunk_batches(&[], &info, Packing::Msb, 3, 0).unwrap().is_empty());
    }

    #[test]
    fn adaptive_batches_budget_includes_float_predictor_and_packed_row_scratch() {
        let mut info = tiled_info(2, 1, 2, false, true);
        info.compression = comp::NONE;
        let chunk = info.chunks(4096)[0];
        // A full nominal tile plus two float row buffers (one predictor copy).
        assert_eq!(chunk_working_bytes(&chunk, &info, Packing::Msb, 3).unwrap(), 48 + 48);
        info.predictor = 1;
        assert_eq!(chunk_working_bytes(&chunk, &info, Packing::Msb, 3).unwrap(), 48 + 24);
        info.compression = comp::DEFLATE;
        info.predictor = 3;
        assert_eq!(
            chunk_working_bytes(&chunk, &info, Packing::Msb, 3).unwrap(),
            48 + 3 * 48 + 48 + std::mem::size_of::<miniz_oxide::inflate::core::DecompressorOxide>()
        );
        info.sample_format = 1;
        info.bits_per_sample = vec![12; 3];
        info.compression = comp::NONE;
        // 6 packed 12-bit samples round to 3 words; allow Vec collection growth.
        assert_eq!(chunk_working_bytes(&chunk, &info, Packing::Words32Msb, 3).unwrap(), 24 + 3 * 12);
    }

    #[test]
    fn adaptive_batches_keep_partial_decode_errors_and_nested_scopes() {
        let mut info = tiled_info(3, 2, 2, false, false);
        info.compression = comp::NONE;
        let file = vec![42u8; 24];
        info.offsets = vec![0, 24]; // second tile is missing; the established partial decode remains usable.
        info.byte_counts = vec![24, 24];
        let output = output_bytes(3 * 2 * 3, false).unwrap();
        let chunks = info.chunks(file.len() as u64);
        let working = chunk_working_bytes(&chunks[0], &info, Packing::Msb, 3).unwrap();
        let read = || read_image(&file, &info, ByteOrder::Little, Packing::Msb);
        let parallel = read().unwrap();
        assert_eq!(with_memory_budget(output + working, read).unwrap(), parallel);
        assert!(with_memory_budget(output + working - 1, read).is_err());
        let old = memory_budget();
        with_memory_budget(output + working, || {
            assert_eq!(memory_budget(), output + working);
            with_memory_budget(0, || assert!(read().is_err()));
            assert_eq!(read().unwrap(), parallel);
        });
        assert_eq!(memory_budget(), old);
        info.offsets = vec![24, 24];
        let failure = with_memory_budget(output + working, || read_image(&file, &info, ByteOrder::Little, Packing::Msb));
        assert!(matches!(failure, Err(RawError::Corrupt(_))));
    }

    #[test]
    fn adaptive_batches_lossless_jpeg_accounts_for_larger_frame_capacity() {
        let mut info = tiled_info(2, 2, 2, false, false);
        info.compression = comp::JPEG;
        // This accepted frame has twice the nominal tile samples; resize truncates only its length.
        let values: Vec<u16> = (0..24).collect();
        let file = ljpeg::encode(&values, 4, 2, 3, 16, 1, 0);
        info.byte_counts = vec![file.len() as u64];
        let chunk = info.chunks(file.len() as u64)[0];
        let working = chunk_working_bytes(&chunk, &info, Packing::Msb, 3).unwrap();
        assert!(working >= 2 * (1 << 16));
        let output = output_bytes(12, false).unwrap();
        let read = || read_image(&file, &info, ByteOrder::Little, Packing::Msb);
        assert_eq!(with_memory_budget(output + working, read).unwrap(), RawData::U16(values[..12].to_vec()));
        assert!(with_memory_budget(output + working - 1, read).is_err());
    }

    #[test]
    fn adaptive_batches_lossy_budget_uses_available_input_for_truncated_byte_counts() {
        let mut file = Vec::new();
        jpeg_encoder::Encoder::new(&mut file, 90).encode(&[128; 16 * 16 * 3], 16, 16, jpeg_encoder::ColorType::Rgb).unwrap();
        let mut info = tiled_info(16, 16, 16, false, false);
        info.compression = comp::LOSSY_JPEG;
        info.bits_per_sample = vec![8; 3];
        info.byte_counts = vec![file.len() as u64];
        let chunk = info.chunks(file.len() as u64)[0];
        let working = chunk_working_bytes(&chunk, &info, Packing::Msb, 3).unwrap();
        let allowance = output_bytes(16 * 16 * 3, false).unwrap() + working;
        let read = |info: &ImageInfo| read_image(&file, info, ByteOrder::Little, Packing::Msb);
        let full = read(&info).unwrap();
        assert_eq!(with_memory_budget(allowance, || read(&info)).unwrap(), full);
        assert!(with_memory_budget(allowance - 1, || read(&info)).is_err());
        // chunk_bytes already clips this malformed count to EOF; planning must agree.
        info.byte_counts[0] = u64::MAX;
        assert_eq!(with_memory_budget(allowance, || read(&info)).unwrap(), full);
    }
}
