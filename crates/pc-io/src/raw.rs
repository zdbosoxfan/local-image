//! Camera raw files via `photocraft-raw`: developed to a 16-bit RGB document
//! in ProPhoto RGB (the embedded profile is the built-in ProPhoto-compatible
//! profile), or, for raw variants not decoded yet, the camera's embedded
//! JPEG preview with a warning.

use std::sync::Arc;

use photocraft_codecs::{self as codecs, ChannelLayout, Format, Image};
use photocraft_raw::{DevelopOptions, Limits, RawError};

use crate::flat::image_to_document;
use crate::{ImportResult, IoError};

/// `true` if `bytes` are a camera raw file `photocraft-raw` recognises.
pub fn is_raw(bytes: &[u8]) -> bool {
    photocraft_raw::is_raw(bytes)
}

/// The decode limits shared with the flat codecs.
fn limits() -> Limits {
    let l = codecs::Limits::default();
    Limits { max_width: l.max_width, max_height: l.max_height, max_pixels: l.max_pixels, max_alloc: l.max_alloc }
}

/// A raw file's embedded JPEG preview, turned upright. Its own EXIF orientation wins when it
/// has one; otherwise the raw's IFD0 orientation applies (TIFF-based raws record it there, and
/// their previews are usually stored as the sensor reads out). Developed raws are oriented by
/// `photocraft-raw` and carry no EXIF, so nothing is turned twice.
fn upright_preview(raw: &[u8], jpeg: &[u8]) -> Result<Image, IoError> {
    let img = codecs::decode_as_with(Format::Jpeg, jpeg, &codecs::DecodeOptions { keep_orientation: true, ..Default::default() })?;
    let own = img.meta.exif.as_deref().map_or(1, codecs::exif_orientation);
    let o = if own != 1 { own } else { codecs::exif_orientation(raw) };
    Ok(img.oriented(o)?)
}

/// Develops a raw file with the default settings: LightCraft's pipeline first, PhotoCraft's
/// developer for anything it can't decode.
pub fn import_raw(name: &str, bytes: &[u8]) -> Result<ImportResult, IoError> {
    if let Some(r) = import_lightcraft(name, bytes) {
        return r;
    }
    import_raw_with(name, bytes, &DevelopOptions { limits: limits(), ..Default::default() })
}

/// local-image: develops a camera raw with LightCraft's scene-referred pipeline (DNG colour
/// matrices and profiles, colour fitted to the embedded JPEG for ARW/NEF/RW2, highlight
/// reconstruction, camera defaults for white balance, sharpening, noise and lens profile) into a
/// 16-bit ProPhoto RGB document tagged with LightCraft's ProPhoto profile. `None` when LightCraft
/// doesn't recognise or fully decode the file.
pub fn import_lightcraft(name: &str, bytes: &[u8]) -> Option<Result<ImportResult, IoError>> {
    use lightcraft_pipeline::{OutputDepth, OutputSpace, RenderRequest};
    lightcraft_raw::probe(bytes)?;
    let (src, info) = lightcraft_engine::files::load_bytes(bytes, usize::MAX).ok()?;
    if !info.raw {
        // Only the embedded preview decoded: let PhotoCraft's path say so with its warning.
        return None;
    }
    let (temp, tint) = if info.relative_wb { (6500.0, 0.0) } else { (info.as_shot_temp, info.as_shot_tint) };
    let mut settings = lightcraft_develop::DevelopSettings::for_raw(temp, tint);
    if info.lens.is_some() {
        settings.optics.lens_profile = true;
    }
    let req = RenderRequest { space: OutputSpace::ProPhoto, depth: OutputDepth::U16, ..RenderRequest::fit(src.width, src.height) };
    let rendered = lightcraft_pipeline::render(&src, &info, &settings, &req);
    let deep = rendered.deep?;
    let lightcraft_pipeline::DeepSamples::U16(samples) = deep.samples else { return None };
    let result = (|| {
        let img = Image::from_u16(deep.width as u32, deep.height as u32, ChannelLayout::Rgb, &samples)?;
        let mut r = image_to_document(name, &img)?;
        r.document.icc_profile = Some(Arc::new(lightcraft_codecs::icc::write_named(lightcraft_codecs::NamedSpace::ProPhoto)));
        // The format's name as photographers write it (`Dng` → `DNG`, `Cr3` → `CR3`).
        let format = lightcraft_raw::probe(bytes).map(|f| format!("{f:?}").to_uppercase()).unwrap_or_else(|| "Camera raw".into());
        r.warnings.push(format!("{format} developed by LightCraft's pipeline (camera defaults, as-shot white balance) into 16-bit ProPhoto RGB"));
        Ok(r)
    })();
    Some(result)
}

/// Develops a raw file with explicit settings.
pub fn import_raw_with(name: &str, bytes: &[u8], opts: &DevelopOptions) -> Result<ImportResult, IoError> {
    let format = photocraft_raw::identify(bytes).map(|f| f.name()).unwrap_or("camera raw");
    match photocraft_raw::develop(bytes, opts) {
        Ok(dev) => {
            let img = Image::from_u16(dev.width, dev.height, ChannelLayout::Rgb, &dev.rgb)?;
            let mut r = image_to_document(name, &img)?;
            r.document.icc_profile = Some(Arc::new(photocraft_cms::Builtin::ProPhotoCompat.profile().to_bytes().to_vec()));
            // "Canon" + "Canon EOS 80D" reads as "Canon EOS 80D".
            let camera = match (dev.info.make.as_deref(), dev.info.model.as_deref()) {
                (Some(make), Some(model)) if model.to_ascii_lowercase().starts_with(&make.to_ascii_lowercase()) => model.to_string(),
                (make, model) => [make, model].into_iter().flatten().collect::<Vec<_>>().join(" "),
            };
            r.warnings.push(format!(
                "{format}{} developed with default settings ({} demosaic, as-shot white balance) into 16-bit {}",
                if camera.is_empty() { String::new() } else { format!(" from {camera}") },
                opts.demosaic.id(),
                photocraft_raw::OUTPUT_SPACE
            ));
            r.warnings.extend(dev.warnings);
            Ok(r)
        }
        Err(RawError::Unsupported(reason)) => match photocraft_raw::embedded_preview(bytes) {
            Some(p) => {
                let img = upright_preview(bytes, p.jpeg)?;
                let mut r = image_to_document(name, &img)?;
                r.warnings.insert(
                    0,
                    format!(
                        "{format}: {reason} is not supported yet; opened the camera's embedded {}x{} JPEG preview instead (8-bit, not the raw sensor data)",
                        p.width, p.height
                    ),
                );
                Ok(r)
            }
            None => Err(IoError::Raw(RawError::Unsupported(reason))),
        },
        Err(e) => Err(IoError::Raw(e)),
    }
}

/// The last full-size decode [`develop_with_settings`] made, by content hash. Coming back from
/// Develop with new settings re-develops a Develop layer from the same file: decoding a raw
/// (demosaic, profiles) was over half of that wait (1.3 s of 2.7 s for a 21 MP ARW on a Ryzen
/// 9800X3D, measured with `bench_develop_open`). One entry, and only up to
/// [`DECODE_CACHE_MAX_BYTES`], so it can't hold much memory.
static LAST_DECODE: std::sync::Mutex<Option<([u8; 32], Arc<lightcraft_raster::Rgb32f>, lightcraft_pipeline::SourceInfo)>> = std::sync::Mutex::new(None);

/// The largest decoded source (linear float RGB) [`LAST_DECODE`] keeps: about 64 MP.
const DECODE_CACHE_MAX_BYTES: usize = 768 << 20;

#[cfg(test)]
thread_local! {
    static DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn decoded_source(bytes: &[u8]) -> Result<(Arc<lightcraft_raster::Rgb32f>, lightcraft_pipeline::SourceInfo), IoError> {
    let key = *blake3::hash(bytes).as_bytes();
    if let Ok(mut last) = LAST_DECODE.lock() {
        if let Some((k, src, info)) = last.as_ref()
            && *k == key
        {
            return Ok((src.clone(), info.clone()));
        }
        // another photo: free its decode before this one is made (never two held at once)
        *last = None;
    }
    #[cfg(test)]
    DECODES.with(|d| d.set(d.get() + 1));
    let (src, info) = lightcraft_engine::files::load_bytes(bytes, usize::MAX).map_err(IoError::Unsupported)?;
    let src = Arc::new(src);
    if let Ok(mut last) = LAST_DECODE.lock() {
        *last = (src.width * src.height * 12 <= DECODE_CACHE_MAX_BYTES).then(|| (key, src.clone(), info.clone()));
    }
    Ok((src, info))
}

/// local-image: a Develop layer's source: `bytes` (a camera raw or any photo LightCraft decodes)
/// developed with `settings` (LightCraft `DevelopSettings` as JSON; missing fields take their
/// defaults) into 16-bit ProPhoto RGB, keeping the file's EXIF and XMP. Raw files stay raw: white
/// balance, highlight recovery and lens corrections work from the sensor data.
pub fn develop_with_settings(name: &str, bytes: &[u8], settings: &serde_json::Value) -> Result<ImportResult, IoError> {
    use lightcraft_pipeline::{OutputDepth, OutputSpace, RenderRequest};
    let settings: lightcraft_develop::DevelopSettings =
        serde_json::from_value(settings.clone()).map_err(|e| IoError::Unsupported(format!("develop settings: {e}")))?;
    let (src, info) = decoded_source(bytes)?;
    let req = RenderRequest { space: OutputSpace::ProPhoto, depth: OutputDepth::U16, ..RenderRequest::fit(src.width, src.height) };
    let rendered = lightcraft_pipeline::render(&src, &info, &settings, &req);
    let Some((width, height, lightcraft_pipeline::DeepSamples::U16(samples))) = rendered.deep.as_ref().map(|d| (d.width, d.height, &d.samples)) else {
        return Err(IoError::Unsupported(format!("{name}: the develop pipeline produced no 16-bit output")));
    };
    let img = Image::from_u16(width as u32, height as u32, ChannelLayout::Rgb, samples)?;
    let mut r = image_to_document(name, &img)?;
    r.document.icc_profile = Some(Arc::new(lightcraft_codecs::icc::write_named(lightcraft_codecs::NamedSpace::ProPhoto)));
    let meta = lightcraft_meta::embedded(bytes);
    r.document.metadata.exif = meta.exif.map(Arc::new);
    r.document.metadata.xmp = meta.xmp;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_raw::testgen::{DngSpec, mosaic, scene};

    fn dng(w: usize, h: usize, neutral: [f64; 3]) -> Vec<u8> {
        let mut spec = DngSpec::cfa(w, h, mosaic(&scene(w, h), w, [0, 1, 1, 2], 0, 65535));
        spec.as_shot_neutral = Some(neutral);
        spec.build()
    }

    fn pixels(r: &ImportResult) -> Vec<f32> {
        let l = &r.document.layers[0];
        let s = l.surface().expect("pixels");
        s.read_region(r.document.bounds())
    }

    /// Re-developing the same file with new settings reuses its decode, and gives exactly what a
    /// fresh decode gives; another file replaces it.
    #[test]
    fn redevelop_reuses_the_decode_and_matches_a_fresh_one() {
        let a = dng(40, 24, [0.6, 1.0, 0.8]);
        let b = dng(40, 24, [0.7, 1.0, 0.5]);
        let plain = serde_json::json!({});
        let brighter = serde_json::json!({ "light": { "exposure": 1.0 } });
        let decodes = || DECODES.with(std::cell::Cell::get);
        let n0 = decodes();
        let first = develop_with_settings("a.dng", &a, &plain).unwrap();
        let bright = develop_with_settings("a.dng", &a, &brighter).unwrap();
        let again = develop_with_settings("a.dng", &a, &plain).unwrap();
        assert_eq!(decodes() - n0, 1, "one decode for three develops of the same file");
        assert_eq!(pixels(&first), pixels(&again));
        assert_ne!(pixels(&first), pixels(&bright));
        let other = develop_with_settings("b.dng", &b, &plain).unwrap();
        assert_eq!(decodes() - n0, 2, "another file decodes");
        assert_ne!(pixels(&first), pixels(&other));
        // and the first one again: decoded afresh (one entry), same result
        let back = develop_with_settings("a.dng", &a, &plain).unwrap();
        assert_eq!(decodes() - n0, 3);
        assert_eq!(pixels(&first), pixels(&back));
    }
}
