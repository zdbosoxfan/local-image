//! Flat raster formats via `photocraft-codecs`.

use std::sync::Arc;

use photocraft_codecs::{self as codecs, ChannelLayout, Format, Image, SampleType as CSample};
use photocraft_color::{BlendMode, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent};
use photocraft_geom::{Rect, Size, TILE_SIZE};
use photocraft_raster::Surface;

use crate::{ExportOptions, ExportResult, ImportResult, IoError, XmpEmbed};

/// Bytes per band when converting or exporting a band of rows at a time.
const BAND_BYTES: usize = 32 << 20;

/// Decodes a flat image into a single-layer document; a TIFF with Photoshop layer data opens
/// layered (see [`crate::tiff_layers`]).
pub fn import_flat(name: &str, bytes: &[u8]) -> Result<ImportResult, IoError> {
    if codecs::detect(bytes) == Some(Format::Tiff) {
        return import_tiff_page(name, bytes, None);
    }
    let img = codecs::decode(bytes)?;
    let mut r = image_to_document(name, &img)?;
    // OpenEXR and Radiance HDR hold linear, scene-referred values (Rec. 709 primaries unless
    // stated otherwise): tag them linear sRGB so they display and convert correctly.
    let d = &mut r.document;
    if d.icc_profile.is_none() && d.mode == ColorMode::Rgb && matches!(codecs::detect(bytes), Some(Format::OpenExr | Format::Hdr)) {
        d.icc_profile = Some(photocraft_cms::Builtin::LinearSrgb.profile().to_bytes());
    }
    Ok(r)
}

/// Opens one page of a TIFF or BigTIFF file: `None` is the page Photoshop opens (the first
/// full-resolution one), `Some(i)` an index into [`codecs::tiff_info`]'s pages (every IFD and
/// SubIFD). A page with Photoshop layer data opens layered.
pub fn import_tiff_page(name: &str, bytes: &[u8], page: Option<usize>) -> Result<ImportResult, IoError> {
    // The orientation is applied after the layer check: rotating the composite but not the
    // layers would misalign them, so a layered TIFF keeps its stored orientation.
    let keep = codecs::DecodeOptions { keep_orientation: true, ..Default::default() };
    let (img, orientation) = match page {
        None => (codecs::decode_with(bytes, &keep)?, codecs::tiff_orientation(bytes)),
        Some(p) => (codecs::decode_tiff_page(bytes, p, &keep)?, codecs::tiff_page_orientation(bytes, p)),
    };
    if let Some(layers) = img.meta.photoshop_layers.as_deref().filter(|l| !l.is_empty()) {
        let mut r = crate::tiff_layers::import_layered(name, &img, layers)?;
        if orientation != 1 {
            r.warnings.push("the TIFF's orientation tag was ignored so the layers stay aligned with the image".to_string());
        }
        return Ok(r);
    }
    image_to_document(name, &img.oriented(orientation)?)
}

/// A decoded flat image as a single-layer document.
pub(crate) fn image_to_document(name: &str, img: &Image) -> Result<ImportResult, IoError> {
    // What the decoder noticed (frames or pages left out, data ending early) comes first.
    let mut warnings: Vec<String> = img.warnings.iter().map(ToString::to_string).collect();
    let (mode, target_layout) = match img.layout() {
        ChannelLayout::Gray | ChannelLayout::GrayA => (ColorMode::Grayscale, ChannelLayout::GrayA),
        ChannelLayout::Rgb | ChannelLayout::Rgba => (ColorMode::Rgb, ChannelLayout::Rgba),
        ChannelLayout::Cmyk | ChannelLayout::CmykA => (ColorMode::Cmyk, ChannelLayout::CmykA),
    };
    let (depth, csample) = match img.sample_type() {
        CSample::U8 => (SampleType::U8, CSample::U8),
        CSample::U16 => (SampleType::U16, CSample::U16),
        CSample::F16 => {
            warnings.push("16-bit float samples are stored as 32-bit float".to_string());
            (SampleType::F32, CSample::F32)
        }
        CSample::F32 => (SampleType::F32, CSample::F32),
    };
    let (w, h) = img.dimensions();
    let mut doc = Document::new(name, Size::new(w, h), mode, depth);
    let fmt = PixelFormat::new(mode, depth, true);
    let mut s = Surface::new(fmt);
    if img.layout() == target_layout && img.sample_type() == csample {
        s.write_interleaved(Rect::new(0, 0, w as i32, h as i32), img.data());
    } else {
        // Converted a band of rows at a time: no second full-size copy of the image.
        let row = img.data().len() / (h.max(1) as usize);
        let band = (BAND_BYTES / row.max(1)).max(1);
        for (i, rows) in img.data().chunks(row.max(1) * band).enumerate() {
            let n = (rows.len() / row.max(1)) as u32;
            let part = Image::from_raw(w, n, img.layout(), img.sample_type(), rows.to_vec())?.convert(target_layout, csample);
            let y0 = (i * band) as i32;
            s.write_interleaved(Rect::new(0, y0, w as i32, y0 + n as i32), part.data());
        }
    }
    s.prune();
    let mut bg = Layer::new("Background", LayerContent::Raster(s));
    if !img.layout().has_alpha() {
        bg.locks.transparency = true;
        bg.locks.position = true;
    }
    doc.layers.push(bg);
    doc.icc_profile = img.icc.clone().map(Arc::new);
    doc.metadata.exif = img.meta.exif.clone().map(Arc::new);
    doc.metadata.xmp = img.meta.xmp.clone();
    if let Some((x, _)) = img.meta.dpi {
        doc.resolution_dpi = x;
    }
    if !img.meta.text.is_empty() {
        warnings.push(format!("{} text metadata entries are not kept in the document", img.meta.text.len()));
    }
    Ok(ImportResult { document: doc, warnings })
}

/// `Some(surface)` when the document is exactly one visible, unmasked,
/// normal, fully opaque raster layer: its pixels can be written natively
/// (keeping CMYK / depth exactly) instead of going through the compositor.
pub(crate) fn single_layer(doc: &Document) -> Option<&Surface> {
    let [l] = &doc.layers[..] else { return None };
    let ok = l.visible
        && l.opacity >= 1.0
        && l.fill_opacity >= 1.0
        && l.mask.is_none()
        && l.vector_mask.as_ref().is_none_or(|mask| !mask.enabled)
        && l.effects.items.is_empty()
        && l.effects.psd_raw.is_none()
        && matches!(l.blend, BlendMode::Normal | BlendMode::PassThrough)
        && photocraft_compose::channel_weights(l, doc.mode).is_none()
        && !photocraft_compose::blend_if_active(l, doc.mode);
    match (&l.content, ok) {
        (LayerContent::Raster(s), true) if s.format() == doc.pixel_format() => Some(s),
        _ => None,
    }
}

pub(crate) fn layout_for(mode: ColorMode, alpha: bool) -> ChannelLayout {
    match (mode, alpha) {
        (ColorMode::Grayscale, false) => ChannelLayout::Gray,
        (ColorMode::Grayscale, true) => ChannelLayout::GrayA,
        (ColorMode::Cmyk, false) => ChannelLayout::Cmyk,
        (ColorMode::Cmyk, true) => ChannelLayout::CmykA,
        (_, false) => ChannelLayout::Rgb,
        (_, true) => ChannelLayout::Rgba,
    }
}

pub(crate) fn csample(s: SampleType) -> CSample {
    match s {
        SampleType::U8 => CSample::U8,
        SampleType::U16 => CSample::U16,
        SampleType::F32 => CSample::F32,
    }
}

/// Renders the document to a flat codec image (native pixels when possible).
pub fn document_to_image(doc: &Document, warnings: &mut Vec<String>) -> Result<Image, IoError> {
    let (w, h) = (doc.size.width, doc.size.height);
    let canvas = doc.bounds();
    let fmt = doc.pixel_format();
    let n = (w as usize) * (h as usize);
    // Lab has no flat-format layout here: Lab documents always go through the composite.
    let native = single_layer(doc).filter(|_| fmt.mode != ColorMode::Lab);
    let mut icc = doc.icc_profile.as_ref().map(|i| i.to_vec());
    let img = if let Some(s) = native {
        // Native path: keep model and depth. The surface's encoded samples are the codec's raw
        // native-endian samples, copied a band of rows at a time (alpha dropped when opaque).
        let ch = fmt.channels();
        let opaque = opaque_surface(s, canvas);
        let layout = layout_for(fmt.mode, !opaque);
        let bps = fmt.sample.bytes();
        let keep = if opaque { (ch - 1) * bps } else { ch * bps };
        let mut data = try_buffer(n, keep)?;
        let rows = (BAND_BYTES / (w as usize * ch * bps).max(1)).max(1) as i32;
        let mut y = 0;
        while y < h as i32 {
            let y1 = y.saturating_add(rows).min(h as i32);
            let band = s.to_interleaved(Rect::new(0, y, w as i32, y1));
            if opaque {
                for px in band.chunks_exact(ch * bps) {
                    data.extend_from_slice(&px[..keep]);
                }
            } else {
                data.extend_from_slice(&band);
            }
            y = y1;
        }
        Image::from_raw(w, h, layout, csample(fmt.sample), data)?
    } else {
        let count = doc.layer_count();
        warnings.push(format!("{count} layer(s) flattened; layers, masks and blend modes are not kept"));
        // The compositor works in RGB; write RGB/gray.
        let gray = fmt.mode == ColorMode::Grayscale;
        if fmt.mode == ColorMode::Cmyk || fmt.mode == ColorMode::Lab {
            // The compositor renders CMYK/Lab documents in sRGB (CMYK through the built-in
            // profile), so the file is tagged sRGB.
            warnings.push(format!("{:?} composite written as sRGB RGB (colour-managed conversion)", fmt.mode));
            icc = Some(photocraft_cms::Builtin::Srgb.profile().to_bytes().to_vec());
        }
        let cs = csample(fmt.sample);
        let colors = if gray { 1 } else { 3 };
        // Rendered and quantised in bands (no full-size float composite), with alpha; the alpha
        // is dropped afterwards, in place, when every pixel turned out opaque.
        let mut data = try_buffer(n, (colors + 1) * cs.bytes())?;
        let mut opaque = true;
        let _ = photocraft_compose::render_bands(doc, canvas, 0, |band| -> Result<(), ()> {
            opaque &= band.px.iter().all(|p| p[3] >= 1.0);
            let parts = crate::pixels::par_map(crate::pixels::bands(band.px.len()), |range| {
                let mut out = Vec::with_capacity(range.len() * (colors + 1) * cs.bytes());
                let mut put = |v: f32| {
                    let v = if v.is_nan() { 0.0 } else { v };
                    match cs {
                        CSample::U8 => out.push((v.clamp(0.0, 1.0) * 255.0).round() as u8),
                        CSample::U16 => out.extend_from_slice(&((v.clamp(0.0, 1.0) * 65535.0).round() as u16).to_ne_bytes()),
                        CSample::F16 | CSample::F32 => out.extend_from_slice(&v.to_ne_bytes()),
                    }
                };
                for p in &band.px[range] {
                    if gray {
                        put(photocraft_color::convert::rgb_to_gray([p[0], p[1], p[2]]));
                    } else {
                        put(p[0]);
                        put(p[1]);
                        put(p[2]);
                    }
                    put(p[3]);
                }
                out
            });
            for part in parts {
                data.extend_from_slice(&part);
            }
            Ok(())
        });
        if opaque {
            drop_alpha(&mut data, colors * cs.bytes(), cs.bytes());
        }
        let layout = layout_for(if gray { ColorMode::Grayscale } else { ColorMode::Rgb }, !opaque);
        Image::from_raw(w, h, layout, cs, data)?
    };
    let meta = codecs::Metadata {
        exif: doc.metadata.exif.as_ref().map(|e| e.to_vec()),
        xmp: doc.metadata.xmp.clone(),
        dpi: Some((doc.resolution_dpi, doc.resolution_dpi)),
        ..Default::default()
    };
    Ok(img.with_icc(icc).with_meta(meta))
}

/// Flatten for replacement in the source format, profile, model and sample depth.
/// The source metadata belongs to the photo, rather than the temporary Develop document.
/// Camera raws and formats without a read/write pair are deliberately rejected.
pub fn export_over_original(doc: &Document, original: &[u8], jpeg_quality: u8) -> Result<ExportResult, IoError> {
    use photocraft_cms::{ColorSpace, Intent, Profile, Transform};
    if crate::raw::is_raw(original) {
        return Err(IoError::Unsupported("Raw files can't be overwritten; save a copy instead".into()));
    }
    let original_psd = if crate::is_psd(original) {
        let result = crate::import("original", original)?;
        if !result.warnings.is_empty() {
            return Err(IoError::Unsupported("Animated, multi-page or damaged originals can't be overwritten; save a copy instead".into()));
        }
        Some(result.document)
    } else {
        None
    };
    let format = if original_psd.is_some() { Format::Tiff } else { codecs::detect(original).ok_or_else(|| IoError::UnknownFormat("original".into()))? };
    if !matches!(format, Format::Jpeg | Format::Png | Format::Tiff | Format::WebP) || !format.caps().read || !format.caps().write {
        return Err(IoError::Unsupported("This format can't be overwritten; save a copy instead".into()));
    }
    let source = if let Some(old) = &original_psd {
        if !matches!(old.mode, ColorMode::Rgb | ColorMode::Grayscale | ColorMode::Cmyk) {
            return Err(IoError::Unsupported("This PSD colour mode can't be overwritten; save a copy instead".into()));
        }
        let has_alpha = photocraft_psd::PsdFile::from_bytes(original)?.merged_has_alpha();
        Image::new(1, 1, layout_for(old.mode, has_alpha), csample(old.depth))?.with_icc(old.icc_profile.as_ref().map(|p| p.to_vec())).with_meta(
            codecs::Metadata {
                exif: old.metadata.exif.as_ref().map(|p| p.to_vec()),
                xmp: old.metadata.xmp.clone(),
                dpi: Some((old.resolution_dpi, old.resolution_dpi)),
                ..Default::default()
            },
        )
    } else {
        codecs::decode(original)?
    };
    if !source.warnings.is_empty() {
        return Err(IoError::Unsupported("Animated, multi-page or damaged originals can't be overwritten; save a copy instead".into()));
    }
    let mut warnings = Vec::new();
    let mut img = document_to_image(doc, &mut warnings)?;
    if img.layout().has_alpha() && !source.layout().has_alpha() {
        img = matte_over_white(&img)?;
    }
    let space = |l: ChannelLayout| {
        if l.is_gray() {
            ColorSpace::Gray
        } else if l.is_cmyk() {
            ColorSpace::Cmyk
        } else {
            ColorSpace::Rgb
        }
    };
    let profile = |icc: &Option<Vec<u8>>, layout: ChannelLayout| -> Result<Profile, IoError> {
        let cs = space(layout);
        match icc {
            Some(bytes) => {
                let p = Profile::parse(bytes).map_err(|e| IoError::Unsupported(e.to_string()))?;
                if p.color_space != cs {
                    return Err(IoError::Unsupported("profile does not match the photo's colour model".into()));
                }
                Ok(p)
            }
            None => photocraft_cms::builtin::default_for(cs).cloned().ok_or_else(|| IoError::Unsupported("unknown colour space".into())),
        }
    };
    let src = profile(&img.icc, img.layout())?;
    let dst = profile(&source.icc, source.layout())?;
    let layout = source.layout();
    let sample = source.sample_type();
    if !src.same_colors(&dst) || !img.layout().same_model(layout) {
        let t = Transform::new(&src, &dst, Intent::RelativeColorimetric, true).map_err(|e| IoError::Unsupported(e.to_string()))?;
        let ss = img.layout().channels();
        let ds = layout.channels();
        img = map_bands(&img, layout, sample, |vals| {
            let mut out = vec![1.0; vals.len() / ss * ds];
            t.convert_f32(&vals, ss, &mut out, ds, true);
            out
        })?;
    } else {
        img = img.convert(layout, sample);
    }
    img.icc = source.icc;
    img.meta = source.meta;
    if let Some(exif) = &mut img.meta.exif {
        drop_exif_thumbnail(exif);
    }
    // The replacement is flattened: stale private layer data must not override the new pixels.
    img.meta.photoshop_layers = None;
    if let Some(old) = original_psd {
        let mut flat = image_to_document(&doc.name, &img)?.document;
        flat.metadata.psd_resources = old.metadata.psd_resources.into_iter().filter(|(id, _, _)| !matches!(id, 1033 | 1036)).collect();
        let name = if original.get(4..6) == Some(&[0, 2]) { "psb" } else { "psd" };
        let mut out = crate::export(&flat, name, &ExportOptions::default())?;
        out.warnings.extend(warnings);
        admit_overwrite_size(out.bytes.len() as u64)?;
        return Ok(out);
    }
    let opts = codecs::EncodeOptions {
        tiff_bigtiff: original.starts_with(b"II+\0") || original.starts_with(b"MM\0+"),
        jpeg_quality: jpeg_quality.clamp(1, 100),
        embed_icc: true,
        embed_metadata: true,
        ..Default::default()
    };
    let bytes = codecs::encode(&img, format, &opts)?;
    let bytes = if format == Format::Tiff {
        lightcraft_meta::preserve_tiff_metadata(original, &bytes).map_err(|e| IoError::Unsupported(e.to_string()))?
    } else if format == Format::Jpeg {
        preserve_jpeg_metadata(original, &bytes, img.dimensions())?
    } else {
        bytes
    };
    admit_overwrite_size(bytes.len() as u64)?;
    Ok(ExportResult { bytes, warnings })
}

fn admit_overwrite_size(size: u64) -> Result<(), IoError> {
    if size > photocraft_format::read::MAX_FILE_BYTES {
        return Err(IoError::Unsupported(
            "The edited file exceeds the app's reopen limit. The original was not changed; reduce its size or export a smaller copy.".into(),
        ));
    }
    Ok(())
}

/// IFD1 describes the old thumbnail. Leave camera/EXIF/GPS entries and their original offsets
/// intact, but unlink that directory so fast thumbnail loaders decode the replacement pixels.
fn drop_exif_thumbnail(exif: &mut [u8]) {
    let little = exif.starts_with(b"II");
    if !little && !exif.starts_with(b"MM") {
        return;
    }
    let read = |at: usize, n: usize| -> Option<u64> {
        let b = exif.get(at..at.checked_add(n)?)?;
        Some(if little { b.iter().rev().fold(0, |v, b| (v << 8) | u64::from(*b)) } else { b.iter().fold(0, |v, b| (v << 8) | u64::from(*b)) })
    };
    let (header, count_size, entry_size, offset_size) = match read(2, 2) {
        Some(42) => (4, 2, 12, 4),
        Some(43) => (8, 8, 20, 8),
        _ => return,
    };
    let Some(at) = read(header, offset_size).and_then(|v| usize::try_from(v).ok()) else { return };
    let Some(count) = read(at, count_size).and_then(|v| usize::try_from(v).ok()) else { return };
    let Some(next) = count.checked_mul(entry_size).and_then(|v| at.checked_add(count_size)?.checked_add(v)) else { return };
    if let Some(bytes) = exif.get_mut(next..next.saturating_add(offset_size)) {
        bytes.fill(0);
    }
}

/// Patch dimensions in the original Exif allocation. Rebuilding its TIFF tree would
/// relocate opaque MakerNotes (including their vendor-relative/absolute pointers).
/// Only dimension entries change; old out-of-line payloads and all offsets remain.
fn update_exif_dimensions(exif: &mut [u8], width: u32, height: u32) -> Result<(), IoError> {
    fn number(bytes: &[u8], at: usize, n: usize, little: bool) -> Option<u64> {
        let b = bytes.get(at..at.checked_add(n)?)?;
        Some(if little { b.iter().rev().fold(0, |v, b| (v << 8) | u64::from(*b)) } else { b.iter().fold(0, |v, b| (v << 8) | u64::from(*b)) })
    }
    fn put(bytes: &mut [u8], value: u64, little: bool) {
        let n = bytes.len();
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = (value >> (8 * if little { i } else { n - i - 1 })) as u8;
        }
    }
    let invalid = || IoError::Unsupported("invalid JPEG Exif directory; cannot update dimensions safely".into());
    let little = exif.starts_with(b"II");
    if !little && !exif.starts_with(b"MM") {
        return Err(invalid());
    }
    let (header, count_size, entry_size, offset_size) = match number(exif, 2, 2, little) {
        Some(42) => (4, 2, 12, 4),
        Some(43) if number(exif, 4, 2, little) == Some(8) && number(exif, 6, 2, little) == Some(0) => (8, 8, 20, 8),
        _ => return Err(invalid()),
    };
    let first = number(exif, header, offset_size, little).and_then(|v| usize::try_from(v).ok()).ok_or_else(invalid)?;
    let mut pending = vec![(first, false)];
    let mut visited = std::collections::BTreeSet::new();
    let mut changes = Vec::new();
    while let Some((at, is_exif)) = pending.pop() {
        if at == 0 || !visited.insert(at) {
            continue;
        }
        let count = number(exif, at, count_size, little).and_then(|v| usize::try_from(v).ok()).ok_or_else(invalid)?;
        let start = at.checked_add(count_size).ok_or_else(invalid)?;
        let end = count.checked_mul(entry_size).and_then(|n| start.checked_add(n)).ok_or_else(invalid)?;
        exif.get(start..end.checked_add(offset_size).ok_or_else(invalid)?).ok_or_else(invalid)?;
        for entry in (start..end).step_by(entry_size) {
            let tag = number(exif, entry, 2, little).ok_or_else(invalid)?;
            let typ = number(exif, entry + 2, 2, little).ok_or_else(invalid)?;
            let count = number(exif, entry + 4, entry_size - 4 - offset_size, little).ok_or_else(invalid)?;
            let slot = entry + entry_size - offset_size;
            if !is_exif && tag == 34665 {
                let size = match typ {
                    4 | 13 => 4,
                    16 | 18 if offset_size == 8 => 8,
                    _ => return Err(invalid()),
                };
                if count != 1 {
                    return Err(invalid());
                }
                let child = number(exif, slot, size, little).and_then(|v| usize::try_from(v).ok()).ok_or_else(invalid)?;
                pending.push((child, true));
            }
            let dimension = match (is_exif, tag) {
                (false, 256) | (true, 40962) => width,
                (false, 257) | (true, 40963) => height,
                _ => continue,
            };
            changes.push((entry, slot, typ == 3 && count == 1 && dimension <= u16::MAX as u32, dimension));
        }
    }
    for (entry, slot, short, dimension) in changes {
        let size = if short { 2 } else { 4 };
        put(&mut exif[entry + 2..entry + 4], if short { 3 } else { 4 }, little);
        put(&mut exif[entry + 4..slot], 1, little);
        // Keep unused inline padding exactly as the camera wrote it.
        put(&mut exif[slot..slot + size], u64::from(dimension), little);
    }
    Ok(())
}

const JPEG_XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const JPEG_EXTENDED_XMP: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const MAX_EXTENDED_XMP: usize = 16 * 1024 * 1024;

/// RFC 1321 MD5, solely for Adobe's Extended XMP content GUID. This is a file-format
/// checksum, never an authentication or security primitive. No new dependency is needed.
fn extended_xmp_guid(bytes: &[u8]) -> String {
    const SHIFT: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut compress = |block: &[u8]| {
        let mut words = [0u32; 16];
        for (word, chunk) in words.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = b.wrapping_add(a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(words[g]).rotate_left(SHIFT[(i / 16) * 4 + i % 4]));
            (a, b, c, d) = (d, next, b, c);
        }
        for (total, value) in state.iter_mut().zip([a, b, c, d]) {
            *total = total.wrapping_add(value);
        }
    };
    let (chunks, rest) = bytes.as_chunks::<64>();
    for chunk in chunks {
        compress(chunk);
    }
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let size = if rest.len() < 56 { 64 } else { 128 };
    tail[size - 8..size].copy_from_slice(&(bytes.len() as u64).wrapping_mul(8).to_le_bytes());
    for block in tail[..size].as_chunks::<64>().0 {
        compress(block);
    }
    state.into_iter().flat_map(u32::to_le_bytes).map(|byte| format!("{byte:02X}")).collect()
}

struct ExtendedXmp {
    old_guid: String,
    new_guid: String,
    segments: Vec<Vec<u8>>,
}

/// Reassemble only a complete, coherent Extended XMP stream. The size budget is
/// checked before allocation, and gaps/overlaps/GUID mismatches never yield output.
fn normalize_extended_xmp(segments: &[&[u8]], width: u32, height: u32) -> Result<Option<ExtendedXmp>, IoError> {
    let invalid = || IoError::Unsupported("invalid or oversized JPEG Extended XMP stream".into());
    let mut chunks = Vec::new();
    let mut identity: Option<(&[u8], usize)> = None;
    for segment in segments {
        if segment[1] != 0xe1 {
            continue;
        }
        let Some(payload) = segment[4..].strip_prefix(JPEG_EXTENDED_XMP) else { continue };
        let header = payload.get(..40).ok_or_else(invalid)?;
        let guid = &header[..32];
        let len = u32::from_be_bytes(header[32..36].try_into().map_err(|_| invalid())?) as usize;
        let offset = u32::from_be_bytes(header[36..40].try_into().map_err(|_| invalid())?) as usize;
        let data = &payload[40..];
        if !guid.iter().all(u8::is_ascii_hexdigit) || len == 0 || len > MAX_EXTENDED_XMP || data.is_empty() || chunks.len() >= 4096 {
            return Err(invalid());
        }
        if offset.checked_add(data.len()).is_none_or(|end| end > len) {
            return Err(invalid());
        }
        match identity {
            Some((previous, size)) if !previous.eq_ignore_ascii_case(guid) || size != len => return Err(invalid()),
            None => identity = Some((guid, len)),
            _ => {}
        }
        chunks.push((offset, data));
    }
    let Some((guid, len)) = identity else { return Ok(None) };
    chunks.sort_by_key(|(offset, _)| *offset);
    let mut end = 0;
    for (offset, data) in &chunks {
        if *offset != end {
            return Err(invalid());
        }
        end += data.len();
    }
    if end != len {
        return Err(invalid());
    }
    let packet: Vec<u8> = chunks.into_iter().flat_map(|(_, data)| data.iter().copied()).collect();
    let old_guid = std::str::from_utf8(guid).map_err(|_| invalid())?.to_owned();
    if !extended_xmp_guid(&packet).eq_ignore_ascii_case(&old_guid) {
        return Err(invalid());
    }
    let packet = std::str::from_utf8(&packet).map_err(|_| invalid())?;
    let packet = lightcraft_meta::normalize_xmp_geometry(packet, width, height).map_err(|e| IoError::Unsupported(e.to_string()))?;
    if packet.len() > MAX_EXTENDED_XMP {
        return Err(invalid());
    }
    let new_guid = extended_xmp_guid(packet.as_bytes());
    let chunk_size = u16::MAX as usize - 2 - JPEG_EXTENDED_XMP.len() - 40;
    let mut segments = Vec::new();
    for (index, chunk) in packet.as_bytes().chunks(chunk_size).enumerate() {
        let len = 2 + JPEG_EXTENDED_XMP.len() + 40 + chunk.len();
        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&(len as u16).to_be_bytes());
        segment.extend_from_slice(JPEG_EXTENDED_XMP);
        segment.extend_from_slice(new_guid.as_bytes());
        segment.extend_from_slice(&(packet.len() as u32).to_be_bytes());
        segment.extend_from_slice(&((index * chunk_size) as u32).to_be_bytes());
        segment.extend_from_slice(chunk);
        segments.push(segment);
    }
    Ok(Some(ExtendedXmp { old_guid, new_guid, segments }))
}

/// Keep opaque APP metadata (including IPTC, extended XMP and private camera blocks) that
/// the raster codec doesn't model. Use the new encoder's JFIF/Adobe and pixel segments.
fn preserve_jpeg_metadata(original: &[u8], encoded: &[u8], (width, height): (u32, u32)) -> Result<Vec<u8>, IoError> {
    fn segments(bytes: &[u8]) -> Result<(Vec<&[u8]>, &[u8]), IoError> {
        let mut at = 2;
        let mut entries = Vec::new();
        while let Some(header) = bytes.get(at..at + 4) {
            if header[0] != 0xff {
                break;
            }
            if header[1] == 0xda {
                return Ok((entries, &bytes[at..]));
            }
            let len = usize::from(u16::from_be_bytes([header[2], header[3]]));
            if len < 2 {
                break;
            }
            let end = at.checked_add(2 + len).ok_or_else(|| IoError::Unsupported("JPEG segment overflow".into()))?;
            let entry = bytes.get(at..end).ok_or_else(|| IoError::Unsupported("truncated JPEG metadata".into()))?;
            entries.push(entry);
            at = end;
        }
        Err(IoError::Unsupported("invalid JPEG metadata stream".into()))
    }
    let is_metadata = |s: &[u8]| matches!(s[1], 0xe1..=0xed | 0xef | 0xfe);
    let (old, _) = segments(original)?;
    let (new, pixels) = segments(encoded)?;
    let extended = normalize_extended_xmp(&old, width, height)?;
    let main_count = old.iter().filter(|segment| segment[1] == 0xe1 && segment[4..].starts_with(JPEG_XMP)).count();
    if extended.is_some() && main_count != 1 {
        return Err(IoError::Unsupported("Extended XMP requires one linked main XMP packet".into()));
    }
    let mut wrote_extended = false;
    let mut out = vec![0xff, 0xd8];
    for segment in new.into_iter().filter(|s| !is_metadata(s)) {
        out.extend_from_slice(segment);
    }
    for segment in old.into_iter().filter(|s| is_metadata(s) && !(s[1] == 0xe2 && s[4..].starts_with(b"MPF\0"))) {
        if segment[1] == 0xe1 && segment[4..].starts_with(JPEG_EXTENDED_XMP) {
            if !wrote_extended {
                if let Some(extended) = &extended {
                    for segment in &extended.segments {
                        out.extend_from_slice(segment);
                    }
                }
                wrote_extended = true;
            }
            continue;
        }
        let mut segment = segment.to_vec();
        if segment[1] == 0xe1 && segment[4..].starts_with(b"Exif\0\0") {
            let upright = codecs::upright_exif(&segment[10..]);
            let mut payload = upright.into_owned();
            update_exif_dimensions(&mut payload, width, height)?;
            drop_exif_thumbnail(&mut payload);
            segment[10..].copy_from_slice(&payload);
        } else if segment[1] == 0xe1 && segment[4..].starts_with(JPEG_XMP) {
            let begin = 4 + JPEG_XMP.len();
            let packet = std::str::from_utf8(&segment[begin..]).map_err(|_| IoError::Unsupported("non-UTF-8 JPEG XMP".into()))?;
            let packet = match &extended {
                Some(extended) => lightcraft_meta::normalize_xmp_geometry_with_extended_guid(packet, width, height, &extended.old_guid, &extended.new_guid),
                None => {
                    if lightcraft_meta::xmp_has_extended_guid(packet).map_err(|e| IoError::Unsupported(e.to_string()))? {
                        return Err(IoError::Unsupported("JPEG XMP references missing Extended XMP chunks".into()));
                    }
                    lightcraft_meta::normalize_xmp_geometry(packet, width, height)
                }
            }
            .map_err(|e| IoError::Unsupported(e.to_string()))?;
            segment.truncate(begin);
            segment.extend_from_slice(packet.as_bytes());
            let len = u16::try_from(segment.len() - 2).map_err(|_| IoError::Unsupported("JPEG XMP too large".into()))?;
            segment[2..4].copy_from_slice(&len.to_be_bytes());
        }
        out.extend_from_slice(&segment);
    }
    out.extend_from_slice(pixels);
    Ok(out)
}

/// An empty buffer with room for `pixels × bytes_per_pixel` bytes, or an error (not an abort)
/// when that much memory can't be had.
pub(crate) fn try_buffer(pixels: usize, bytes_per_pixel: usize) -> Result<Vec<u8>, IoError> {
    let len = pixels.checked_mul(bytes_per_pixel).ok_or_else(|| IoError::Unsupported("image too large".into()))?;
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(|_| IoError::Unsupported(format!("not enough memory for a {} MB image", len >> 20)))?;
    Ok(v)
}

/// Remove the trailing `alpha` bytes of every `color + alpha`-byte pixel, in place.
fn drop_alpha(data: &mut Vec<u8>, color: usize, alpha: usize) {
    let stride = color + alpha;
    let n = data.len() / stride;
    for i in 0..n {
        data.copy_within(i * stride..i * stride + color, i * color);
    }
    data.truncate(n * color);
    data.shrink_to_fit();
}

/// Whether every pixel of `s` over `r` is fully opaque (read from the tiles, no copy).
fn opaque_surface(s: &Surface, r: Rect) -> bool {
    let fmt = s.format();
    let (ch, bps) = (fmt.channels(), fmt.sample.bytes());
    let a = ch - 1;
    let alpha_ok = |px: &[u8]| photocraft_color::read_sample(px, fmt.sample, a) >= 1.0;
    let mut default = vec![0u8; ch * bps];
    photocraft_raster::encode_pixel(&fmt, &s.default_pixel(), &mut default);
    r.tiles().all(|tc| {
        let tr = tc.rect().intersect(&r);
        match s.tile(tc) {
            None => alpha_ok(&default),
            Some(t) => (tr.y0..tr.y1).all(|y| {
                let row = ((y - tc.rect().y0) as usize * TILE_SIZE as usize + (tr.x0 - tc.rect().x0) as usize) * ch * bps;
                t.bytes()[row..row + tr.width() as usize * ch * bps].chunks_exact(ch * bps).all(alpha_ok)
            }),
        }
    })
}

/// Flattens and encodes as `format`.
pub fn export_flat(doc: &Document, format: Format, opts: &ExportOptions) -> Result<ExportResult, IoError> {
    use photocraft_cms::{Builtin, Intent};
    if let Some(r) = export_mode_specific(doc, format, opts)? {
        return Ok(r);
    }
    let mut warnings = Vec::new();
    let mut img = document_to_image(doc, &mut warnings)?;
    if opts.xmp == XmpEmbed::None {
        // Export As's Metadata: None: the packet lists the text of every type layer and one id
        // per placed document (#647).
        img.meta.xmp = None;
    }
    if img.layout().has_alpha() && !format.caps().alpha {
        // Flattened over white, as saving a transparent document without transparency does.
        img = matte_over_white(&img)?;
        warnings.push(format!("transparency composited over white for {format:?}"));
    }
    if img.layout().is_cmyk() && !format.caps().layouts.iter().any(|l| l.is_cmyk()) {
        img = cmyk_image_to_srgb(&img)?;
        warnings.push(format!("CMYK converted to sRGB for {format:?} through the document's colour profile"));
    }
    if matches!(format, Format::OpenExr | Format::Hdr) {
        // OpenEXR and Radiance HDR store linear light (read back as linear sRGB, see [`import_flat`]).
        if let Some(linear) = convert_rgb(&img, Builtin::LinearSrgb.profile(), Intent::RelativeColorimetric, false, CSample::F32)? {
            img = linear;
        }
    } else if !format.caps().icc {
        // Untagged files read back as sRGB: convert to it (as Quick Export does) rather than
        // write values that only mean something under the dropped profile.
        if let Some(srgb) = convert_rgb(&img, Builtin::Srgb.profile(), Intent::Perceptual, true, img.sample_type())? {
            img = srgb;
            warnings.push(format!("colours converted to sRGB; {format:?} can't embed the document's colour profile"));
        }
    }
    for w in codecs::fidelity_warnings_with(&img, format, &opts.encode) {
        if w.is_fatal() {
            return Err(IoError::Unsupported(w.to_string()));
        }
        warnings.push(w.to_string());
    }
    let bytes = codecs::encode(&img, format, &opts.encode)?;
    Ok(ExportResult { bytes, warnings })
}

/// `img` with every band of rows mapped by `f` (normalized samples in, normalized `layout`
/// samples out) and stored as `sample`, so no full-size float copy is made. Profile and metadata
/// are kept.
fn map_bands(img: &Image, layout: ChannelLayout, sample: CSample, f: impl Fn(Vec<f32>) -> Vec<f32>) -> Result<Image, IoError> {
    let (w, h) = img.dimensions();
    let row = img.data().len() / (h.max(1) as usize);
    let band = row.max(1) * (BAND_BYTES / row.max(1)).max(1);
    let mut data = try_buffer(img.pixel_count(), layout.channels() * sample.bytes())?;
    for rows in img.data().chunks(band) {
        let n = (rows.len() / row.max(1)) as u32;
        let vals = f(Image::from_raw(w, n, img.layout(), img.sample_type(), rows.to_vec())?.to_normalized());
        data.extend_from_slice(Image::from_normalized(w, n, layout, sample, &vals)?.data());
    }
    Ok(Image::from_raw(w, h, layout, sample, data)?.with_icc(img.icc.clone()).with_meta(img.meta.clone()))
}

/// Straight-alpha pixels composited over white (no ink for CMYK), without the alpha channel.
fn matte_over_white(img: &Image) -> Result<Image, IoError> {
    let layout = img.layout();
    let white = if layout.is_cmyk() { 0.0 } else { 1.0 };
    map_bands(img, layout.without_alpha(), img.sample_type(), |vals| {
        let mut out = Vec::with_capacity(vals.len() / layout.channels() * layout.color_channels());
        for px in vals.chunks_exact(layout.channels()) {
            if let Some((&a, color)) = px.split_last() {
                let a = a.clamp(0.0, 1.0);
                out.extend(color.iter().map(|&c| c * a + white * (1.0 - a)));
            }
        }
        out
    })
}

/// RGB pixels converted from their profile (sRGB when untagged) to `dst`, unclamped, untagged and
/// stored as `sample`. `None` when the image isn't RGB or already holds `dst`'s colours.
fn convert_rgb(img: &Image, dst: &photocraft_cms::Profile, intent: photocraft_cms::Intent, bpc: bool, sample: CSample) -> Result<Option<Image>, IoError> {
    use photocraft_cms::{Builtin, ColorSpace, Profile, Transform};
    if !img.layout().is_rgb() {
        return Ok(None);
    }
    let src =
        img.icc.as_ref().and_then(|b| Profile::parse(b).ok()).filter(|p| p.color_space == ColorSpace::Rgb).unwrap_or_else(|| Builtin::Srgb.profile().clone());
    if src.same_colors(dst) {
        return Ok(None);
    }
    let t = Transform::new(&src, dst, intent, bpc).map_err(|e| IoError::Unsupported(e.to_string()))?;
    let stride = img.layout().channels();
    let out = map_bands(img, img.layout(), sample, |mut vals| {
        t.apply(&mut vals, stride);
        vals
    })?;
    Ok(Some(out.with_icc(None)))
}

/// Colour-managed CMYK → sRGB for formats that cannot store CMYK (the document's embedded
/// CMYK profile when it parses, else the built-in coated CMYK; relative colorimetric + BPC).
fn cmyk_image_to_srgb(img: &Image) -> Result<Image, IoError> {
    use photocraft_cms::{Builtin, ColorSpace, Intent, Profile, Transform};
    let src = img
        .icc
        .as_ref()
        .and_then(|b| Profile::parse(b).ok())
        .filter(|p| p.color_space == ColorSpace::Cmyk)
        .unwrap_or_else(|| Builtin::CoatedCmyk.profile().clone());
    let dst = Builtin::Srgb.profile();
    let t = Transform::new(&src, dst, Intent::RelativeColorimetric, true).map_err(|e| IoError::Unsupported(e.to_string()))?;
    let alpha = img.layout().has_alpha();
    let (ss, ds) = (if alpha { 5 } else { 4 }, if alpha { 4 } else { 3 });
    let vals = img.to_normalized();
    let mut out = vec![0.0f32; img.pixel_count() * ds];
    t.convert_f32(&vals, ss, &mut out, ds, true);
    let (w, h) = img.dimensions();
    let layout = if alpha { ChannelLayout::Rgba } else { ChannelLayout::Rgb };
    let sample = match img.sample_type() {
        CSample::F16 => CSample::F32,
        s => s,
    };
    Ok(Image::from_normalized(w, h, layout, sample, &out)?.with_icc(Some(dst.to_bytes().to_vec())).with_meta(img.meta.clone()))
}

/// Indexed Color → PNG-8 with its colour table; Duotone → the inks rendered as RGB.
fn export_mode_specific(doc: &Document, format: Format, opts: &ExportOptions) -> Result<Option<ExportResult>, IoError> {
    match doc.mode {
        ColorMode::Indexed if format == Format::Png => {
            let Some(table) = doc.color_table.as_ref().filter(|t| !t.colors.is_empty() && t.colors.len() <= 256) else { return Ok(None) };
            let mut idx = try_buffer(doc.size.area() as usize, 1)?;
            let _ = photocraft_compose::render_bands(doc, doc.bounds(), 0, |band| -> Result<(), ()> {
                idx.extend(band.px.iter().map(|p| match table.transparent {
                    Some(t) if p[3] < 0.5 => t,
                    _ => table.nearest([p[0], p[1], p[2]]) as u8,
                }));
                Ok(())
            });
            let bytes = codecs::encode_png_indexed(doc.size.width, doc.size.height, &idx, &table.colors, table.transparent)?;
            Ok(Some(ExportResult { bytes, warnings: vec![format!("written as an 8-bit palette PNG ({} colours)", table.colors.len())] }))
        }
        ColorMode::Duotone => {
            let Some(d) = doc.duotone.as_ref() else { return Ok(None) };
            let mut shown = doc.clone();
            shown.layers.push(photocraft_doc::Layer::new("Duotone", photocraft_doc::LayerContent::Adjustment(d.display_adjustment())));
            shown.mode = ColorMode::Rgb;
            shown.icc_profile = None;
            shown.duotone = None;
            let mut r = export_flat(&shown, format, opts)?;
            r.warnings.retain(|w| !w.contains("flattened"));
            r.warnings.push(format!("Duotone ({} inks) written as RGB", d.inks.len()));
            Ok(Some(r))
        }
        _ => Ok(None),
    }
}

#[test]
fn overwrite_output_must_fit_the_reopen_limit() {
    let limit = photocraft_format::read::MAX_FILE_BYTES;
    assert!(admit_overwrite_size(limit).is_ok());
    assert!(admit_overwrite_size(limit + 1).unwrap_err().to_string().contains("reopen limit"));
    assert!(admit_overwrite_size(u64::MAX).is_err());
}

#[cfg(test)]
mod overwrite_metadata_tests {
    use super::extended_xmp_guid;

    #[test]
    fn extended_xmp_guid_matches_rfc1321_and_padding_vectors() {
        // All seven RFC 1321 known-answer vectors, including multi-block input.
        for (input, expected) in [
            ("", "D41D8CD98F00B204E9800998ECF8427E"),
            ("a", "0CC175B9C0F1B6A831C399E269772661"),
            ("abc", "900150983CD24FB0D6963F7D28E17F72"),
            ("message digest", "F96B697D7CB7938D525A2F31AAF161D0"),
            ("abcdefghijklmnopqrstuvwxyz", "C3FCD3D76192E4007DFB496CCA67E13B"),
            ("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "D174AB98D277D9F5A5611C2C9F419D9F"),
            ("12345678901234567890123456789012345678901234567890123456789012345678901234567890", "57EDF4A22BE3C955AC49DA2E2107B67A"),
        ] {
            assert_eq!(extended_xmp_guid(input.as_bytes()), expected);
        }
        // Independent hashlib-generated fixture answers around MD5 padding boundaries.
        for (len, expected) in [
            (55, "EF1772B6DFF9A122358552954AD0DF65"),
            (56, "3B0C8AC703F828B04C6C197006D17218"),
            (63, "B06521F39153D618550606BE297466D5"),
            (64, "014842D480B571495A4A0363793F7367"),
            (65, "C743A45E0D2E6A95CB859ADAE0248435"),
        ] {
            assert_eq!(extended_xmp_guid(&vec![b'a'; len]), expected);
        }
    }
}
