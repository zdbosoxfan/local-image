//! Export: encode a rendered image to JPEG / PNG / TIFF / WebP / AVIF (or copy the original / write a
//! DNG, each with the edits in XMP) in sRGB, Display P3, Adobe RGB
//! (1998) compatible, ProPhoto RGB or Rec. 2020 with an embedded ICC profile we generate from the
//! published primaries and curves, optional output sharpening and a JPEG file-size limit. Pure (bytes in, bytes out) so the desktop
//! app, CLI, MCP and the web build share it; writing the file is the caller's job.

use li_raster::Rgba8;
use li_raster::resample::ResampleChoice;
pub use lightcraft_codecs::TiffCompression;
use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, NamedSpace, Samples, encode, icc};
use lightcraft_meta::{DateTime, Gps, Metadata};
pub use lightcraft_pipeline::{DeepImage, DeepSamples, OutputDepth, OutputSpace};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod lanczos_tests;
mod options;
mod prepared;
#[cfg(test)]
mod tests;
mod watermark;

pub use options::*;
pub use prepared::*;
pub use watermark::*;

/// Unsharp mask on an 8-bit image (separable box-blur approximation of a small Gaussian).
pub fn output_sharpen(img: &mut Rgba8, target: SharpenFor, amount: SharpenAmount) {
    let src: Vec<[f32; 3]> = img.data.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if let Some(out) = unsharp(img.width, img.height, &src, target, amount) {
        for (p, v) in img.data.iter_mut().zip(out) {
            for c in 0..3 {
                p[c] = v[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// [`output_sharpen`] on a high-bit-depth image (float images are sharpened in linear light).
pub fn output_sharpen_deep(img: &mut DeepImage, target: SharpenFor, amount: SharpenAmount) {
    let (w, h) = (img.width, img.height);
    match &mut img.samples {
        DeepSamples::U16(v) => {
            let src: Vec<[f32; 3]> = v.as_chunks::<3>().0.iter().map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect();
            if let Some(out) = unsharp(w, h, &src, target, amount) {
                for (d, s) in v.iter_mut().zip(out.as_flattened()) {
                    *d = s.round().clamp(0.0, 65535.0) as u16;
                }
            }
        }
        DeepSamples::F32(v) => {
            let src: Vec<[f32; 3]> = v.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2]]).collect();
            if let Some(out) = unsharp(w, h, &src, target, amount) {
                for (d, s) in v.iter_mut().zip(out.as_flattened()) {
                    *d = s.max(0.0);
                }
            }
        }
    }
}

/// The sharpened values of `src` (`w × h`), or `None` when nothing is to be done.
fn unsharp(w: usize, h: usize, src: &[[f32; 3]], target: SharpenFor, amount: SharpenAmount) -> Option<Vec<[f32; 3]>> {
    let (radius, base) = match target {
        SharpenFor::None => return None,
        SharpenFor::Screen => (1usize, 0.35f32),
        SharpenFor::Matte => (2, 0.6),
        SharpenFor::Glossy => (1, 0.5),
    };
    let k = base
        * match amount {
            SharpenAmount::Low => 0.6,
            SharpenAmount::Standard => 1.0,
            SharpenAmount::High => 1.5,
        };
    if w < 3 || h < 3 {
        return None;
    }
    let r = radius as isize;
    let box_pass = |inp: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
        let mut out = vec![[0.0; 3]; inp.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 3];
                for d in -r..=r {
                    let (sx, sy) = if horizontal {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    } else {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    };
                    let p = inp[sy * w + sx];
                    acc = [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]];
                }
                let n = (2 * r + 1) as f32;
                out[y * w + x] = acc.map(|v| v / n);
            }
        }
        out
    };
    let blur = box_pass(&box_pass(src, true), false);
    Some(src.iter().zip(&blur).map(|(s, b)| std::array::from_fn(|c| s[c] + (s[c] - b[c]) * k)).collect())
}

/// Encode a rendered display-referred image according to `o` (its pixels must already be in
/// `o.effective_space()`). Resizing to `long_edge` is the
/// caller's job (render at that size); sharpening is applied here.
pub fn encode_image(img: &Rgba8, o: &ExportOptions) -> Result<Vec<u8>, String> {
    encode_with_metadata(img, o, None)
}

/// Like [`encode_image`], embedding `meta` (already filtered by the policy) as EXIF + XMP.
pub fn encode_with_metadata(img: &Rgba8, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    let mut img = img.clone();
    output_sharpen(&mut img, o.sharpen, o.sharpen_amount);
    let space = o.effective_space();
    if let Some(wm) = &o.watermark {
        let wm = Watermark { color: srgb8_in(space, wm.color), target: Some(space), ..wm.clone() };
        draw_watermark(&mut img, &wm);
    }
    let profile = icc::write_named(named_space(space));
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let e = EncodeImage::rgba8(&img);
    let r = match o.format {
        ExportFormat::Jpeg => {
            // 4:4:4 normally; 4:2:0 when targeting a file size (much smaller at equal visual quality).
            let sub = if o.limit_kb.is_some() { ChromaSubsampling::S420 } else { o.jpeg_chroma.unwrap_or(ChromaSubsampling::S444) };
            let jpeg = |q: u8| encode::encode_jpeg(&e, q, sub, &meta);
            match o.limit_kb {
                Some(kb) => {
                    let limit = kb as usize * 1024;
                    // Binary search for the highest quality that fits.
                    let (mut lo, mut hi) = (1u8, o.quality);
                    let mut best = None;
                    while lo <= hi {
                        let mid = lo + (hi - lo) / 2;
                        let b = jpeg(mid).map_err(|e| e.to_string())?;
                        if b.len() <= limit {
                            best = Some(b);
                            lo = mid + 1;
                        } else if mid == 1 {
                            break;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    return best.ok_or_else(|| format!("cannot fit within {kb} KB"));
                }
                None => jpeg(o.quality),
            }
        }
        ExportFormat::Png => encode::encode_png(&e, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&e, o.tiff_compression, &meta),
        ExportFormat::Webp => encode::encode_webp_lossless(&e, &meta),
        ExportFormat::Avif => encode::encode_avif(&e, o.quality, 8, &meta),
        f @ (ExportFormat::Original | ExportFormat::Dng) => return Err(format!("{f:?} export does not encode pixels")),
    };
    r.map_err(|e| e.to_string())
}

/// The codecs' name of an output space (for its ICC profile).
pub fn named_space(s: OutputSpace) -> NamedSpace {
    match s {
        OutputSpace::Srgb => NamedSpace::Srgb,
        OutputSpace::DisplayP3 => NamedSpace::DisplayP3,
        OutputSpace::AdobeRgb => NamedSpace::AdobeRgb,
        OutputSpace::ProPhoto => NamedSpace::ProPhoto,
        OutputSpace::Rec2020 => NamedSpace::Rec2020,
    }
}

/// An 8-bit sRGB colour expressed in `space` (encoded with its curve).
pub fn srgb8_in(space: OutputSpace, c: [u8; 3]) -> [u8; 3] {
    if space == OutputSpace::Srgb {
        return c;
    }
    let lin = c.map(li_color::transfer::decode_srgb8);
    let m = li_color::SRGB.to_space(&space.rgb_space()).to_f32();
    let t = space.trc();
    std::array::from_fn(|i| {
        let v = m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2];
        (t.encode(v) * 255.0 + 0.5) as u8
    })
}

/// Encode a render according to `o`: its high-bit-depth samples when it has them (16-bit PNG/TIFF,
/// 10-bit AVIF, 32-bit float linear TIFF), else its 8-bit image.
pub fn encode_rendered(r: &lightcraft_pipeline::Rendered, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    if let Some(source) = &o.source_encoding {
        return encode_in_source_space(r, o, source, meta);
    }
    match &r.deep {
        Some(d) if o.effective_depth() != OutputDepth::U8 => encode_deep(d, o, meta),
        _ => encode_with_metadata(&r.image, o, meta),
    }
}

/// The render still uses the export pipeline, delivered as linear Rec.2020 floats to avoid
/// an 8-bit/sRGB intermediate. Convert those pixels back to the original profile's primaries
/// and per-channel curves before encoding, including nonstandard RGB matrix/TRC profiles.
fn encode_in_source_space(
    r: &lightcraft_pipeline::Rendered,
    o: &ExportOptions,
    source: &SourceEncoding,
    meta: Option<&Metadata>,
) -> Result<Vec<u8>, String> {
    let deep = r.deep.as_ref().ok_or("source-profile export needs a float render")?;
    let DeepSamples::F32(samples) = &deep.samples else { return Err("source-profile export needs linear floats".into()) };
    let matrix = source.space.to_working().inverse().ok_or("the original's colour matrix is singular")?;
    let curves = source.space.trc.as_ref().ok_or("the original's encoding curves are unknown")?;
    let mut converted = Vec::with_capacity(samples.len());
    let depth = o.effective_depth();
    let float = depth == OutputDepth::F32Linear;
    for pixel in samples.as_chunks::<3>().0 {
        let linear = matrix.apply([pixel[0] as f64, pixel[1] as f64, pixel[2] as f64]);
        for (i, value) in linear.into_iter().enumerate() {
            let encoded = curves[i].from_linear(value as f32);
            converted.push(if float { encoded } else { encoded.clamp(0.0, 1.0) });
        }
    }
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&source.profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let u16s;
    let u8s;
    let samples = if float {
        Samples::F32(&converted)
    } else if depth == OutputDepth::U16 {
        u16s = converted.iter().map(|v| (v * 65535.0 + 0.5) as u16).collect::<Vec<_>>();
        Samples::U16(&u16s)
    } else {
        u8s = converted.iter().map(|v| (v * 255.0 + 0.5) as u8).collect::<Vec<_>>();
        Samples::U8(&u8s)
    };
    let image = EncodeImage::new(deep.width as u32, deep.height as u32, 3, samples);
    match o.format {
        ExportFormat::Jpeg => encode::encode_jpeg(&image, o.quality, o.jpeg_chroma.unwrap_or(ChromaSubsampling::S444), &meta),
        ExportFormat::Png => encode::encode_png(&image, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&image, o.tiff_compression, &meta),
        ExportFormat::Webp => encode::encode_webp_lossless(&image, &meta),
        _ => return Err("this format cannot preserve the original colour profile".into()),
    }
    .map_err(|e| e.to_string())
}

/// Encode a high-bit-depth image (see [`encode_rendered`]).
pub fn encode_deep(img: &DeepImage, o: &ExportOptions, meta: Option<&Metadata>) -> Result<Vec<u8>, String> {
    let mut img = img.clone();
    output_sharpen_deep(&mut img, o.sharpen, o.sharpen_amount);
    if let Some(wm) = &o.watermark {
        let wm = Watermark { color: srgb8_in(img.space, wm.color), target: Some(img.space), ..wm.clone() };
        draw_watermark_deep(&mut img, &wm);
    }
    let profile = match img.samples {
        DeepSamples::F32(_) => icc::write_matrix_trc(&img.space.rgb_space(), &lightcraft_codecs::Trc::Linear),
        DeepSamples::U16(_) => icc::write_named(named_space(img.space)),
    };
    let exif = meta.map(lightcraft_meta::write_exif);
    let xmp = meta.map(|m| lightcraft_meta::write_xmp(m, None));
    let meta = EncodeMeta { icc: Some(&profile), exif: exif.as_deref(), xmp: xmp.as_deref(), ppi: Some(o.ppi) };
    let (w, h) = (img.width as u32, img.height as u32);
    let e = match &img.samples {
        DeepSamples::U16(v) => EncodeImage::new(w, h, 3, Samples::U16(v)),
        DeepSamples::F32(v) => EncodeImage::new(w, h, 3, Samples::F32(v)),
    };
    let r = match o.format {
        ExportFormat::Png => encode::encode_png(&e, &meta),
        ExportFormat::Tiff => encode::encode_tiff(&e, o.tiff_compression, &meta),
        ExportFormat::Avif => encode::encode_avif(&e, o.quality, 8, &meta),
        f => return Err(format!("{f:?} export is 8-bit only")),
    };
    r.map_err(|e| e.to_string())
}

/// Parse a shutter speed such as `1/250`, `0.5` or `2"` into seconds.
fn parse_shutter(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches(['s', '"']).trim();
    match s.split_once('/') {
        Some((n, d)) => Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
    .filter(|v: &f64| v.is_finite() && *v > 0.0)
}

/// The metadata to embed for `photo` under `o.metadata` / `o.remove_location`. `None` = embed nothing.
pub fn export_metadata(photo: &lightcraft_catalog::Photo, o: &ExportOptions) -> Option<Metadata> {
    let m = &photo.meta;
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
    // copyright info (also under "copyright only"): notice, creator, status, usage terms, info URL
    let mut out = Metadata {
        copyright: text(&m.copyright),
        copyright_marked: m.copyright_status.marked(),
        usage_terms: text(&m.usage_terms),
        copyright_url: text(&m.copyright_url),
        artist: text(&m.creator),
        creators: m.creators.clone(),
        software: Some("Local Image".into()),
        ..Default::default()
    };
    match o.metadata {
        MetadataPolicy::None => return None,
        MetadataPolicy::Copyright => return Some(out),
        MetadataPolicy::All | MetadataPolicy::AllExceptCamera => {}
    }
    out.title = text(&m.title);
    out.caption = text(&m.caption);
    out.alt_text = text(&m.alt_text);
    out.extended_description = text(&m.extended_description);
    if !o.remove_location {
        out.sublocation = text(&m.location);
        out.city = text(&m.city);
        out.state = text(&m.state);
        out.country = text(&m.country);
    }
    out.keywords = m.keywords.clone();
    if o.include_people_names {
        add_people_keywords(&mut out.keywords, &m.regions);
    }
    out.capture_time = photo.captured.as_deref().and_then(DateTime::parse_iso);
    out.rating = (photo.rating > 0).then_some(photo.rating as i8);
    // Pixels are exported upright: orientation is baked in.
    out.orientation = Some(lightcraft_meta::Orientation::Normal);
    if !o.remove_location {
        out.gps = m.gps.map(|(latitude, longitude)| Gps { latitude, longitude, altitude: None });
    }
    if o.metadata == MetadataPolicy::All {
        out.serial_number = m.serial_number.clone();
        out.sony = m.sony.clone();
        // All DJI telemetry may reveal location, including retained RTK/custom properties.
        // The existing Remove Location control must also suppress that namespace.
        if !o.remove_location {
            out.dji = m.dji.clone();
        }
        out.model = text(&m.camera);
        out.lens_model = text(&m.lens);
        out.focal_length = m.focal_mm.map(f64::from);
        out.f_number = m.aperture.map(f64::from);
        out.exposure_time = parse_shutter(&m.shutter);
        out.iso = m.iso;
    }
    Some(out)
}

/// Names leave the face store only as explicit export metadata; never include IDs or vectors.
fn add_people_keywords(keywords: &mut Vec<String>, regions: &[lightcraft_meta::Region]) {
    for name in regions
        .iter()
        .filter(|r| r.kind == lightcraft_meta::RegionKind::Face)
        .filter_map(|r| r.name.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        let keyword = format!("People|{name}");
        if !keywords.iter().any(|k| k.eq_ignore_ascii_case(&keyword)) {
            keywords.push(keyword);
        }
    }
}

/// One exported file.
pub struct Exported {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// Files written next to it, named like it with another extension: `(extension, bytes)` (the
    /// XMP sidecar of an `Original` export).
    pub sidecars: Vec<(&'static str, Vec<u8>)>,
}

/// Output size of photo `p` under `o` (its cropped full size when `o.resize` is `None`).
pub fn output_size(p: &lightcraft_catalog::Photo, o: &ExportOptions) -> (usize, usize) {
    let (w, h) = lightcraft_pipeline::native_output_size(p.width.max(1) as usize, p.height.max(1) as usize, &p.develop);
    match &o.resize {
        Some(r) => r.apply(w, h),
        None => ((w.round() as usize).max(1), (h.round() as usize).max(1)),
    }
}

/// Render photo `id` at the requested size and encode it (or copy / convert its original for
/// [`ExportFormat::Original`] / [`ExportFormat::Dng`]).
pub fn export_photo(session: &mut crate::Session, id: lightcraft_catalog::PhotoId, o: &ExportOptions, seq: usize) -> Result<Exported, String> {
    prepare_export(session, id, o, seq)?.run()
}

/// How an exported DNG stores its raw data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DngCompression {
    /// Lossless JPEG (the usual DNG compression).
    #[default]
    Lossless,
    /// Deflate (zip) tiles with prediction.
    Deflate,
    Uncompressed,
}

/// The destination of a batch: a folder (with `ExportOptions::subfolder` and the conflict policy
/// applied to each file together with its sidecars), or one exact file path (single photo; an
/// ordinary file already there is replaced). Either way a catalogued original (or its sidecar)
/// is never written over: see [`crate::originals`].
#[derive(Clone, Debug, Default)]
pub struct Destination {
    pub dir: String,
    pub exact: Option<String>,
}

/// Where each of `ids` would be exported to under `o` and `to`, before any conflict policy:
/// the folder ([`ExportOptions::same_folder`]: the photo's own; plus the subfolder) and the file
/// name ([`ExportOptions::file_name_for`]). `None` for a photo with no folder of its own. Used to
/// preview names and to ask about existing files before an export starts.
pub fn planned_paths(
    catalog: &lightcraft_catalog::Catalog,
    ids: &[lightcraft_catalog::PhotoId],
    o: &ExportOptions,
    to: &Destination,
) -> Vec<Option<String>> {
    let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{}/{b}", a.trim_end_matches('/')) };
    let in_sub = |d: &str| if o.subfolder.is_empty() { d.to_string() } else { join(d, &o.subfolder) };
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let p = catalog.photo(*id)?;
            if let Some(exact) = to.exact.as_deref().filter(|_| ids.len() == 1) {
                return Some(exact.to_string());
            }
            let dir = match (&p.source, o.same_folder) {
                (_, false) => in_sub(&to.dir),
                (lightcraft_catalog::Source::File { path }, true) => in_sub(&std::path::Path::new(path).parent()?.to_string_lossy()),
                (lightcraft_catalog::Source::Demo { .. }, true) => return None,
            };
            Some(join(&dir, &o.file_name_for(p, i + 1)))
        })
        .collect()
}

/// Export `ids` in order ([`prepare_batch`] + [`run_batch`]), stopping at the first error.
pub fn export_batch(
    session: &mut crate::Session,
    ids: &[lightcraft_catalog::PhotoId],
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Vec<serde_json::Value>, String> {
    let items = prepare_batch(session, ids, o)?;
    run_batch(items, o, to, write, exists, true, &mut |_, _| true)
}

/// Write an exported or rendered file on disk: its folder is created if needed, and the file is
/// replaced atomically ([`li_fs::write_atomic_nosync`]: a temp file
/// renamed into place), so a failure part-way leaves any previous file intact and no truncated
/// one. Not synced to disk (issue #134): an export can always be made again from the original.
/// The native writer behind exports, renders and screenshots (app, CLI, MCP).
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_with(path, bytes, false)
}

/// [`write_file`], synced to disk: for a written file that becomes a library photo (Edit a Copy)
/// and so can't simply be exported again once the catalog points at it.
pub fn write_file_durable(path: &str, bytes: &[u8]) -> Result<(), String> {
    write_with(path, bytes, true)
}

fn write_with(path: &str, bytes: &[u8], durable: bool) -> Result<(), String> {
    use li_fs::{write_atomic, write_atomic_nosync};
    let p = std::path::Path::new(path);
    if let Some(dir) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if durable { write_atomic(p, bytes) } else { write_atomic_nosync(p, bytes) }.map_err(|e| format!("{path}: {e}"))
}

/// The model emits sRGB8; preserve the requested output primaries, curve and container depth.
/// Restoration has 8-bit source precision even in 16/32-bit exports.
/// Resize a full-size export render to `size` with `kernel` — the editor's Image Size engine
/// (`li-raster` f64 taps, edges repeated; Preserve Details' enlargement sharpening from
/// `photocraft-algo`), in the render's own encoding: display-encoded for 8/16-bit, linear for
/// 32-bit float, as the editor resizes a document of that depth. 8-bit results are dithered like
/// the Develop renderer's own 8-bit output; `deep` is kept only when the export wants it.
pub(crate) fn resample_rendered(r: &mut lightcraft_pipeline::Rendered, kernel: ResampleChoice, size: (usize, usize), keep_deep: bool) {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let (sw, sh) = r.deep.as_ref().map_or((r.image.width, r.image.height), |d| (d.width, d.height));
    if (sw, sh) == (w, h) || sw == 0 || sh == 0 {
        return;
    }
    let enlarging = w * h > sw * sh;
    let filter = kernel.resolve(enlarging);
    let mut out = match r.deep.as_ref().map(|d| &d.samples) {
        Some(DeepSamples::U16(v)) => li_raster::resample::resize_interleaved(v, sw, sh, 3, false, w, h, filter),
        Some(DeepSamples::F32(v)) => li_raster::resample::resize_interleaved(v, sw, sh, 3, false, w, h, filter),
        None => {
            let rgb: Vec<u8> = r.image.data.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
            li_raster::resample::resize_interleaved(&rgb, sw, sh, 3, false, w, h, filter)
        }
    };
    if filter == li_raster::resample::Resample::PreserveDetails && enlarging {
        out = preserve_details(out, (sw, sh), (w, h));
    }
    let space = r.deep.as_ref().map(|d| d.space);
    let linear = matches!(r.deep.as_ref().map(|d| &d.samples), Some(DeepSamples::F32(_)));
    let deep = space.map(|space| DeepImage {
        width: w,
        height: h,
        space,
        samples: if linear {
            DeepSamples::F32(out.clone())
        } else {
            DeepSamples::U16(out.iter().map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16).collect())
        },
    });
    r.image = match (&deep, linear) {
        (Some(d), true) => d.to_rgba8(),
        _ => {
            let mut img = Rgba8::new(w, h);
            for (i, (p, c)) in img.data.iter_mut().zip(out.as_chunks::<3>().0).enumerate() {
                let (x, y) = (i % w, i / w);
                *p = [
                    lightcraft_pipeline::finish::dither8(c[0], x, y, 0),
                    lightcraft_pipeline::finish::dither8(c[1], x, y, 1),
                    lightcraft_pipeline::finish::dither8(c[2], x, y, 2),
                    255,
                ];
            }
            img
        }
    };
    r.deep = deep.filter(|_| keep_deep);
}

/// Preserve Details' light sharpening of an enlargement (`photocraft-algo`, as Image Size does).
fn preserve_details(rgb: Vec<f32>, from: (usize, usize), to: (usize, usize)) -> Vec<f32> {
    use photocraft_doc::{ColorMode, PixelFormat, SampleType, Surface};
    let rect = photocraft_geom::Rect::new(0, 0, to.0 as i32, to.1 as i32);
    let mut s = Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::F32, false));
    s.write_region(rect, &rgb);
    let (sx, sy) = (to.0 as f64 / from.0 as f64, to.1 as f64 / from.1 as f64);
    photocraft_algo::resample::preserve_details_sharpen(&s, sx, sy, rect).read_region(rect)
}

fn enhanced_samples(image: &mut Rgba8, opts: &ExportOptions) -> Option<DeepImage> {
    let (space, depth) = if opts.source_encoding.is_some() {
        (OutputSpace::Rec2020, OutputDepth::F32Linear)
    } else {
        (opts.effective_space(), opts.effective_depth())
    };
    let matrix = li_color::SRGB.to_space(&space.rgb_space());
    let trc = space.trc();
    let mut values = Vec::with_capacity(image.data.len() * 3);
    let linear = depth == OutputDepth::F32Linear;
    for pixel in &mut image.data {
        let rgb = matrix.apply([pixel[0], pixel[1], pixel[2]].map(|v| li_color::transfer::decode_srgb8(v) as f64));
        for c in 0..3 {
            let encoded = trc.encode(rgb[c] as f32).clamp(0.0, 1.0);
            values.push(if linear { rgb[c] as f32 } else { encoded });
            pixel[c] = (encoded * 255.0).round() as u8;
        }
    }
    let samples = match depth {
        OutputDepth::U8 => return None,
        OutputDepth::U16 => DeepSamples::U16(values.into_iter().map(|v| (v * 65535.0).round() as u16).collect()),
        OutputDepth::F32Linear => DeepSamples::F32(values),
    };
    Some(DeepImage { width: image.width, height: image.height, space, samples })
}

/// The path of the sidecar with extension `ext` of the exported file `main`.
fn sidecar_path(main: &str, ext: &str) -> String {
    std::path::Path::new(main).with_extension(ext).to_string_lossy().to_string()
}

/// Run prepared exports in order: pick each one's path, and hand the bytes (and sidecars) to
/// `write`. `exists` tells whether a path is taken. The conflict policy applies to a file and its
/// sidecars as one: with Unique both get the same free name, with Skip the photo is skipped when
/// either is taken. A path that is a catalogued original (or its sidecar) is refused whatever the
/// policy. `progress(done, next file)` is called before each photo; returning false cancels the
/// rest. Returns one JSON object per photo: `{path, photo, width, height, bytes, sidecars}`,
/// `{skipped: path}` or (unless `stop_on_error`) `{photo, file, error}`.
pub fn run_batch(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
) -> Result<Vec<serde_json::Value>, String> {
    run_batch_with_ctl(items, o, to, write, exists, stop_on_error, progress, &lightcraft_sysmem::JobCtl::new())
}

/// The background export uses this control for cancellation within a restoration tile/job.
///
/// The batch runs as an ordered pipeline: a feeder admits photos in
/// order, each with a heavy-job permit sized from its pixels ([`lightcraft_sysmem::heavy_jobs`]),
/// render threads develop them (the GPU one at a time), encode threads encode them, and this
/// thread names, checks and writes them strictly in input order (`progress`, conflict handling,
/// the originals guard and `write` stay here). Stage threads are plain OS threads in the Utility
/// class joined by bounded channels; rayon is only used inside a stage. How many photos are in
/// flight comes from the compute profile and the free memory, never a constant.
#[allow(clippy::too_many_arguments)]
pub fn run_batch_with_ctl(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &lightcraft_sysmem::JobCtl,
) -> Result<Vec<serde_json::Value>, String> {
    let permits = lightcraft_sysmem::heavy_jobs();
    let shape = pipeline::Shape::auto(items.len(), permits.max_jobs(), lightcraft_sysmem::profile().threads);
    run_batch_shaped(items, o, to, write, exists, stop_on_error, progress, ctl, permits, shape)
}

/// What the write stage does at one position of a batch.
enum Slot {
    /// Rendered by the pipeline, written into `dir`.
    Run { dir: String, has_subfolder: bool },
    /// Reported as this photo's error (or ends the batch with `stop_on_error`).
    Fail(String),
    /// Ends the batch with this error when reached.
    Abort(String),
}

/// [`run_batch_with_ctl`] with explicit permits and pipeline shape (tests compare shapes).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_batch_shaped(
    items: Vec<PreparedExport>,
    o: &ExportOptions,
    to: &Destination,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &lightcraft_sysmem::JobCtl,
    permits: &lightcraft_sysmem::HeavyJobs,
    shape: pipeline::Shape,
) -> Result<Vec<serde_json::Value>, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{}/{b}", a.trim_end_matches('/')) };
    let in_sub = |d: &str| if o.subfolder.is_empty() { d.to_string() } else { join(d, &o.subfolder) };
    let batch_dir = in_sub(&to.dir);
    let single = items.len() == 1;

    // Each photo's folder first (pure, in order); an error is reported when the batch reaches it,
    // as before, and nothing after a batch-ending error is rendered.
    let mut slots = Vec::with_capacity(items.len());
    let mut heads = Vec::with_capacity(items.len());
    let mut work = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let name = item.file_name.clone();
        let has_subfolder = item.subfolder.is_some();
        let dir = match (o.same_folder && !has_subfolder, &item.source_dir) {
            (false, _) => Ok(batch_dir.clone()),
            (true, Some(d)) => Ok(in_sub(d)),
            (true, None) => Err(format!("{name}: a generated demo photo has no folder of its own; choose a specific folder")),
        };
        let slot = match (dir, &item.subfolder) {
            (Err(e), _) => Slot::Fail(e),
            (Ok(dir), None) => Slot::Run { dir, has_subfolder },
            (Ok(dir), Some(sub)) => match crate::smart_sort::plan::sanitize(sub) {
                Ok(sub) => Slot::Run { dir: join(&dir, &sub), has_subfolder },
                Err(e) => Slot::Abort(e),
            },
        };
        heads.push((item.photo, name, item.guard.clone()));
        let abort = matches!(slot, Slot::Abort(_));
        if matches!(slot, Slot::Run { .. }) {
            work.push((i, item));
        }
        slots.push(slot);
        if abort {
            break; // the batch ends here: later photos are never rendered
        }
    }

    let flow = pipeline::Flow::default();
    if cfg!(target_arch = "wasm32") {
        // no threads in the browser: each photo is rendered and encoded when the writer reaches it
        let mut work: std::collections::HashMap<usize, PreparedExport> = work.into_iter().collect();
        let mut next = |i: usize| work.remove(&i).map(|item| item.run_with_ctl(ctl));
        return write_stage(&mut next, &slots, &heads, o, to, single, write, exists, stop_on_error, progress, ctl, &flow, &join);
    }
    let stop = AtomicBool::new(false);
    let stopped = || stop.load(Ordering::Relaxed) || ctl.is_cancelled();
    std::thread::scope(|sc| {
        let outputs = pipeline::spawn(sc, work, shape, permits, &flow, &stopped, ctl);
        let r = {
            // this photo's result, keeping those that finish before it (with their permits) until
            // their turn; `None` once the pipeline ended without it
            let mut early = std::collections::BTreeMap::new();
            let mut next = move |i: usize| loop {
                if let Some((r, _permit)) = early.remove(&i) {
                    return Some(r);
                }
                match outputs.recv() {
                    Ok((seq, r, permit)) => {
                        early.insert(seq, (r, permit));
                    }
                    Err(_) if stopped() => return None,
                    Err(_) => return Some(Err("the export stopped unexpectedly: its worker threads could not run".to_string())),
                }
            };
            write_stage(&mut next, &slots, &heads, o, to, single, write, exists, stop_on_error, progress, ctl, &flow, &join)
            // `next` goes here with the receiver and the results held back: an encoder waiting
            // to hand over a photo sees the channel closed
        };
        // whatever ended the batch (done, cancelled, an error): the other stages stop now
        stop.store(true, Ordering::Relaxed);
        flow.wake();
        r
    })
}

type Head = (lightcraft_catalog::PhotoId, String, std::sync::Arc<crate::originals::OriginalGuard>);

/// The write stage of [`run_batch_shaped`], on the caller's thread: takes each photo's result in
/// input order (`next(position)`) and names, checks and writes it.
#[allow(clippy::too_many_arguments)]
fn write_stage(
    next: &mut dyn FnMut(usize) -> Option<Result<Exported, String>>,
    slots: &[Slot],
    heads: &[Head],
    o: &ExportOptions,
    to: &Destination,
    single: bool,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<(), String>,
    exists: &dyn Fn(&str) -> bool,
    stop_on_error: bool,
    progress: &mut dyn FnMut(usize, &str) -> bool,
    ctl: &lightcraft_sysmem::JobCtl,
    flow: &pipeline::Flow,
    join: &dyn Fn(&str, &str) -> String,
) -> Result<Vec<serde_json::Value>, String> {
    use serde_json::json;
    let mut taken = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, (slot, (photo, name, guard))) in slots.iter().zip(heads).enumerate() {
        if ctl.is_cancelled() || !progress(i, name) {
            break;
        }
        let photo = *photo;
        let (dir, has_subfolder) = match slot {
            Slot::Run { dir, has_subfolder } => (dir, *has_subfolder),
            Slot::Fail(err) => {
                if stop_on_error {
                    return Err(err.clone());
                }
                out.push(json!({"photo": photo.0, "file": name, "error": err}));
                flow.written(i + 1);
                continue;
            }
            Slot::Abort(err) => return Err(err.clone()),
        };
        // the pipeline ended without it: cancelled
        let Some(result) = next(i) else { break };
        let e = match result {
            Ok(e) => e,
            Err(err) if stop_on_error => return Err(err),
            Err(err) => {
                out.push(json!({"photo": photo.0, "file": name, "error": err}));
                flow.written(i + 1);
                continue;
            }
        };
        // the exported file and its sidecars
        let group = |main: &str| std::iter::once(main.to_string()).chain(e.sidecars.iter().map(|(x, _)| sidecar_path(main, x))).collect::<Vec<_>>();
        let path = match to.exact.as_deref().filter(|_| single && !has_subfolder) {
            Some(p) => Ok(p.to_string()),
            None => {
                let path = join(dir, &e.file_name);
                let busy = |main: &str| group(main).iter().any(|p| taken.contains(p) || exists(p));
                if !busy(&path) {
                    Ok(path)
                } else {
                    match o.conflict {
                        Conflict::Overwrite if !group(&path).iter().any(|p| taken.contains(p)) => Ok(path),
                        Conflict::Skip => {
                            out.push(json!({"skipped": path}));
                            flow.written(i + 1);
                            continue;
                        }
                        Conflict::Unique | Conflict::Overwrite => {
                            let (stem, ext) = e.file_name.rsplit_once('.').map_or((e.file_name.as_str(), None), |(a, b)| (a, Some(b)));
                            let name = |n: usize| join(dir, &ext.map_or(format!("{stem}-{n}"), |x| format!("{stem}-{n}.{x}")));
                            (2..1_000_000).map(name).find(|p| !busy(p)).ok_or_else(|| format!("{path}: no free file name"))
                        }
                    }
                }
            }
        };
        if ctl.is_cancelled() {
            break;
        }
        let file = path.clone().unwrap_or_else(|_| name.clone());
        let written = path.and_then(|path| {
            let files = group(&path);
            // never over an original, whatever the conflict policy or the exact path said
            for f in &files {
                guard.check(std::path::Path::new(f))?;
            }
            write(&path, &e.bytes)?;
            let mut sidecars = Vec::new();
            for ((_, bytes), sc) in e.sidecars.iter().zip(files.iter().skip(1)) {
                write(sc, bytes)?;
                sidecars.push(sc.clone());
            }
            Ok((path, files, sidecars))
        });
        match written {
            Ok((path, files, sidecars)) => {
                taken.extend(files);
                out.push(json!({"path": path, "photo": photo.0, "width": e.width, "height": e.height, "bytes": e.bytes.len(), "sidecars": sidecars}));
            }
            Err(err) if stop_on_error => return Err(err),
            Err(err) => out.push(json!({"photo": photo.0, "file": file, "error": err})),
        }
        flow.written(i + 1);
    }
    Ok(out)
}

/// The bounded, ordered batch pipeline behind exports (and the GPU gate Build Previews shares).
pub mod pipeline;

/// The photo whose sidecar packet an Original/DNG export writes: with people names as keywords when
/// asked, and without any location (GPS, sublocation, city, state, country) under "Remove location
/// info" (upstream LightCraft db7e378). Borrowed when nothing changes.
fn photo_for_sidecar(
    photo: &lightcraft_catalog::Photo,
    include_people_names: bool,
    remove_location: bool,
) -> std::borrow::Cow<'_, lightcraft_catalog::Photo> {
    let mut photo = std::borrow::Cow::Borrowed(photo);
    if include_people_names {
        let copy = photo.to_mut();
        add_people_keywords(&mut copy.meta.keywords, &copy.meta.regions);
    }
    let m = &photo.meta;
    if remove_location && (m.dji.is_some() || m.gps.is_some() || [&m.location, &m.city, &m.state, &m.country].iter().any(|s| !s.trim().is_empty())) {
        let m = &mut photo.to_mut().meta;
        m.gps = None;
        m.dji = None;
        m.location.clear();
        m.city.clear();
        m.state.clear();
        m.country.clear();
    }
    photo
}
/// Original files cannot be scrubbed while preserving their bytes. Include textual XMP/IPTC places.
fn original_embeds_location(bytes: &[u8]) -> bool {
    let m = lightcraft_meta::extract(bytes);
    m.dji.is_some()
        || m.gps.is_some()
        || [&m.sublocation, &m.city, &m.state, &m.country].into_iter().any(|s| s.as_deref().is_some_and(|s| !s.trim().is_empty()))
}
