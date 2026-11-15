//! OpenEXR via the pure-Rust `exr` crate: F16/F32, Y/YA/RGB/RGBA channels,
//! lossless compression. Reads the first valid layer at full resolution;
//! pixel data is taken from the data window.

use std::io::Cursor;

use exr::prelude::{
    AnyChannel, AnyChannels, Blocks, Compression, Encoding, FlatSamples, Layer, LayerAttributes, LineOrder, ReadChannels, ReadLayers, WritableImage, read,
};
use half::f16;

use crate::Format;
use crate::error::CodecError;
use crate::fidelity::Plan;
use crate::image::{ChannelLayout, Image, SampleType};
use crate::options::{EncodeOptions, ExrCompression, Limits};

const F: Format = Format::OpenExr;

fn err(e: impl std::fmt::Display) -> CodecError {
    CodecError::malformed(F, e)
}

/// An `exr`-crate error, with its `NotSupported` kind kept distinct from malformed data
/// (the same split the TIFF codec makes), so unsupported features say so.
fn map_exr(e: exr::error::Error) -> CodecError {
    match e {
        exr::error::Error::NotSupported(message) => CodecError::unsupported(F, format!("not supported: {message}")),
        e => err(e),
    }
}

/// Strip an optional `layer.` prefix from a channel name.
fn base_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Refuses headers whose `preview` attributes declare more pixels than their value holds.
///
/// The `exr` crate sizes a preview's pixel buffer from its declared dimensions (up to 4 MiB at
/// once, zero-filled) before it finds the attribute too short, and in the non-pedantic mode
/// every reader here uses it then skips the attribute and goes on: thousands of tiny malformed
/// previews would each cost that allocation. This walks the raw headers (name, type, size,
/// value per attribute; an empty name ends a header, an empty header ends a multi-part list)
/// and checks each preview against its own size. Anything else odd is left to the crate.
pub(crate) fn check_previews(b: &[u8]) -> Result<(), CodecError> {
    if b.get(..4) != Some(&[0x76, 0x2f, 0x31, 0x01][..]) {
        return Ok(());
    }
    let Some(flags) = b.get(4..8).and_then(|v| <[u8; 4]>::try_from(v).ok()).map(u32::from_le_bytes) else { return Ok(()) };
    let multipart = flags & 0x1000 != 0;
    let cstr = |at: usize| -> Option<(&[u8], usize)> {
        let rest = b.get(at..)?;
        let n = rest.iter().position(|&c| c == 0)?;
        Some((rest.get(..n)?, at + n + 1))
    };
    let u32_at = |at: usize| b.get(at..at.checked_add(4)?).and_then(|v| <[u8; 4]>::try_from(v).ok()).map(u32::from_le_bytes);
    let mut at = 8;
    loop {
        loop {
            let Some((name, next)) = cstr(at) else { return Ok(()) };
            at = next;
            if name.is_empty() {
                break;
            }
            let Some((kind, next)) = cstr(at) else { return Ok(()) };
            let Some(size) = u32_at(next).and_then(|n| i32::try_from(n).ok()).and_then(|n| usize::try_from(n).ok()) else { return Ok(()) };
            let value = next + 4;
            let end = match value.checked_add(size) {
                Some(end) if end <= b.len() => end,
                _ => return Ok(()),
            };
            if kind == b"preview" {
                let declared =
                    u32_at(value).zip(u32_at(value + 4)).and_then(|(w, h)| u64::from(w).checked_mul(u64::from(h))?.checked_mul(4)).filter(|_| size >= 8);
                if declared.is_none_or(|n| n > (size - 8) as u64) {
                    return Err(err("a preview attribute declares more pixels than it holds"));
                }
            }
            at = end;
        }
        // A multi-part list ends with an empty header (a lone NUL).
        if !multipart || b.get(at) == Some(&0) {
            return Ok(());
        }
    }
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    check_previews(bytes)?;
    // Check declared sizes before any pixel allocation.
    let meta = exr::meta::MetaData::read_from_buffered(Cursor::new(bytes), false).map_err(err)?;
    let mut total: u64 = 0;
    // DWAA/DWAB parts are counted apart: they go past 2^16:1 (a flat 8192x4320 frame with 12
    // alpha-scheme mattes is about 30 KB, over 75000:1).
    let mut dwa: u64 = 0;
    for header in meta.headers.iter() {
        let size = header.layer_size;
        let (w, h) = (u32::try_from(size.0).unwrap_or(u32::MAX), u32::try_from(size.1).unwrap_or(u32::MAX));
        let bpp = header.channels.list.len().max(1) as u64 * 4;
        limits.check_bytes(w, h, bpp)?;
        let part = u64::from(w).saturating_mul(u64::from(h)).saturating_mul(bpp);
        total = total.saturating_add(part);
        if matches!(header.compression, Compression::DWAA(_) | Compression::DWAB(_)) {
            dwa = dwa.saturating_add(part);
        }
    }
    // The `exr` crate allocates a flat part's samples before reading its chunks, so the file
    // must be able to hold them. The other compressions stay far below 2^16:1 even against this
    // f32 budget. DWA's run-length channels are zlib-packed on top and reach about 132000:1, so
    // DWA parts get 2^18:1. Deep parts size their samples from the file's own counts.
    if !meta.requirements.has_deep_data {
        Limits::check_plausible(total.saturating_sub(dwa).saturating_add(dwa / 4), bytes.len(), 1 << 16)?;
    }

    // Deep parts are rejected by the `exr` crate's reader; walk their chunks ourselves and
    // composite the samples into the flat image (a warning says what was lost).
    if meta.requirements.has_deep_data {
        let deep = super::deep_exr::decode_deep(&meta, bytes, limits)?;
        return super::deep_exr::flatten(&deep);
    }

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_buffered(Cursor::new(bytes))
        .map_err(map_exr)?;
    let layer = &image.layer_data;
    let (w, h) = (layer.size.0, layer.size.1);
    let channels = &layer.channel_data.list;
    let find = |n: &str| channels.iter().find(|c| base_name(c.name.to_string().as_str()) == n);
    let (color, layout): (Vec<_>, ChannelLayout) = if let (Some(r), Some(g), Some(b)) = (find("R"), find("G"), find("B")) {
        match find("A") {
            Some(a) => (vec![r, g, b, a], ChannelLayout::Rgba),
            None => (vec![r, g, b], ChannelLayout::Rgb),
        }
    } else if let Some(y) = find("Y").or_else(|| (channels.len() == 1).then(|| &channels[0])) {
        match find("A") {
            Some(a) => (vec![y, a], ChannelLayout::GrayA),
            None => (vec![y], ChannelLayout::Gray),
        }
    } else {
        return Err(CodecError::unsupported(F, "no R/G/B or Y channels"));
    };
    let all_f16 = color.iter().all(|c| matches!(c.sample_data, FlatSamples::F16(_)));
    let sample = if all_f16 { SampleType::F16 } else { SampleType::F32 };
    let npx = w * h;
    if color.iter().any(|c| c.sample_data.len() != npx) {
        return Err(err("channel sample count mismatch (subsampled channels are unsupported)"));
    }
    let nc = color.len();
    let (w32, h32) = (w as u32, h as u32);
    let img = if all_f16 {
        let mut out = vec![f16::ZERO; npx * nc];
        for (ci, c) in color.iter().enumerate() {
            if let FlatSamples::F16(v) = &c.sample_data {
                for (i, s) in v.iter().enumerate() {
                    out[i * nc + ci] = *s;
                }
            }
        }
        Image::from_f16(w32, h32, layout, &out)?
    } else {
        let mut out = vec![0f32; npx * nc];
        for (ci, c) in color.iter().enumerate() {
            for (i, s) in c.sample_data.values_as_f32().enumerate() {
                out[i * nc + ci] = s;
            }
        }
        Image::from_f32(w32, h32, layout, &out)?
    };
    debug_assert_eq!(img.sample_type(), sample);
    Ok(img)
}

pub(crate) fn encode(src: &Image, plan: Plan, opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    let img = src.converted(plan.layout, plan.sample);
    let (w, h) = (img.width() as usize, img.height() as usize);
    let layout = img.layout();
    let names: &[&str] = match layout {
        ChannelLayout::Gray => &["Y"],
        ChannelLayout::GrayA => &["Y", "A"],
        ChannelLayout::Rgb => &["R", "G", "B"],
        ChannelLayout::Rgba => &["R", "G", "B", "A"],
        l => return Err(CodecError::encode(F, format!("unsupported layout {l:?}"))),
    };
    let nc = names.len();
    let mut list: Vec<AnyChannel<FlatSamples>> = Vec::with_capacity(nc);
    match img.sample_type() {
        SampleType::F16 => {
            let s = img.to_f16_samples().unwrap_or_default();
            for (ci, n) in names.iter().enumerate() {
                let v: Vec<f16> = s.iter().skip(ci).step_by(nc).copied().collect();
                list.push(AnyChannel::new(*n, FlatSamples::F16(v)));
            }
        }
        SampleType::F32 => {
            let s = img.to_f32_samples().unwrap_or_default();
            for (ci, n) in names.iter().enumerate() {
                let v: Vec<f32> = s.iter().skip(ci).step_by(nc).copied().collect();
                list.push(AnyChannel::new(*n, FlatSamples::F32(v)));
            }
        }
        s => return Err(CodecError::encode(F, format!("unsupported sample {s:?}"))),
    }
    let compression = match opts.exr_compression {
        ExrCompression::None => Compression::Uncompressed,
        ExrCompression::Rle => Compression::RLE,
        ExrCompression::Zip1 => Compression::ZIP1,
        ExrCompression::Zip16 => Compression::ZIP16,
        ExrCompression::Piz => Compression::PIZ,
    };
    let encoding = Encoding { compression, blocks: Blocks::ScanLines, line_order: LineOrder::Increasing };
    let channels = AnyChannels::sort(list.into_iter().collect());
    let layer = Layer::new((w, h), LayerAttributes::default(), encoding, channels);
    let image = exr::image::Image::from_layer(layer);
    let mut cursor = Cursor::new(Vec::new());
    image.write().to_buffered(&mut cursor).map_err(|e| CodecError::encode(F, e))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use exr::meta::attribute::Preview;
    use exr::prelude::{ImageAttributes, IntegerBounds};

    /// A 2x2 single-part file whose layer carries a 2x1 preview.
    fn with_preview() -> Vec<u8> {
        let mut attrs = LayerAttributes::named("rgb");
        attrs.preview = Some(Preview { size: exr::math::Vec2(2, 1), pixel_data: vec![7; 8] });
        let list = ["R", "G", "B"].into_iter().map(|n| AnyChannel::new(n, FlatSamples::F32(vec![0.5; 4]))).collect();
        let layer = Layer::new(
            (2, 2),
            attrs,
            Encoding { compression: Compression::Uncompressed, blocks: Blocks::ScanLines, line_order: LineOrder::Increasing },
            AnyChannels::sort(list),
        );
        let image = exr::image::Image::from_layers(ImageAttributes::new(IntegerBounds::new((0, 0), (2, 2))), vec![layer]);
        let mut out = Cursor::new(Vec::new());
        image.write().to_buffered(&mut out).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_preview_declaring_more_pixels_than_it_holds_is_refused_before_the_crate_reads_it() {
        let good = with_preview();
        assert_eq!(decode(&good, &Limits::default()).unwrap().dimensions(), (2, 2));
        // Patch the preview to 1024x1024 (4 MiB) while its value still holds 8 bytes.
        let tag = b"preview\0preview\0";
        let at = good.windows(tag.len()).position(|w| w == tag).unwrap() + tag.len() + 4;
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&1024u32.to_le_bytes());
        bad[at + 4..at + 8].copy_from_slice(&1024u32.to_le_bytes());
        for r in [decode(&bad, &Limits::default()).map(|_| ()), crate::decode_deep_exr(&bad, &Limits::default()).map(|_| ())] {
            assert!(matches!(&r, Err(CodecError::Malformed { message, .. }) if message.contains("preview")), "{r:?}");
        }
    }
}
