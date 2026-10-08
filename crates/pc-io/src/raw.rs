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

/// local-image: a Develop layer's source: `bytes` (a camera raw or any photo LightCraft decodes)
/// developed with `settings` (LightCraft `DevelopSettings` as JSON; missing fields take their
/// defaults) into 16-bit ProPhoto RGB, keeping the file's EXIF and XMP. Raw files stay raw: white
/// balance, highlight recovery and lens corrections work from the sensor data.
pub fn develop_with_settings(name: &str, bytes: &[u8], settings: &serde_json::Value) -> Result<ImportResult, IoError> {
    use lightcraft_pipeline::{OutputDepth, OutputSpace, RenderRequest};
    let settings: lightcraft_develop::DevelopSettings =
        serde_json::from_value(settings.clone()).map_err(|e| IoError::Unsupported(format!("develop settings: {e}")))?;
    let (src, info) = lightcraft_engine::files::load_bytes(bytes, usize::MAX).map_err(IoError::Unsupported)?;
    let req = RenderRequest { space: OutputSpace::ProPhoto, depth: OutputDepth::U16, ..RenderRequest::fit(src.width, src.height) };
    let rendered = lightcraft_pipeline::render(&src, &info, &settings, &req);
    let Some(lightcraft_pipeline::DeepSamples::U16(samples)) = rendered.deep.as_ref().map(|d| &d.samples) else {
        return Err(IoError::Unsupported(format!("{name}: the develop pipeline produced no 16-bit output")));
    };
    let deep = rendered.deep.as_ref().expect("checked above");
    let img = Image::from_u16(deep.width as u32, deep.height as u32, ChannelLayout::Rgb, samples)?;
    let mut r = image_to_document(name, &img)?;
    r.document.icc_profile = Some(Arc::new(lightcraft_codecs::icc::write_named(lightcraft_codecs::NamedSpace::ProPhoto)));
    let meta = lightcraft_meta::embedded(bytes);
    r.document.metadata.exif = meta.exif.map(Arc::new);
    r.document.metadata.xmp = meta.xmp;
    Ok(r)
}
