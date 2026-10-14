//! Camera raw import through the shared Library/Develop pipeline.
use crate::flat::image_to_document;
use crate::{ImportResult, IoError};
use photocraft_codecs::{ChannelLayout, Image};
use std::sync::Arc;

/// Recognise the same raw containers as Library, independently of the file extension.
pub fn is_raw(bytes: &[u8]) -> bool {
    lightcraft_raw::probe(bytes).is_some()
}

/// Develop with Library's camera defaults; unsupported variants use its upright preview.
pub fn import_raw(name: &str, bytes: &[u8]) -> Result<ImportResult, IoError> {
    let format = format!("{:?}", lightcraft_raw::probe(bytes).ok_or(lightcraft_raw::RawError::NotRaw)?).to_uppercase();
    let (src, info) = decoded_source(bytes, &Default::default()).map_err(|e| {
        let error = lightcraft_raw::decode(bytes).err().unwrap_or_else(|| lightcraft_raw::RawError::Corrupt(e.to_string()));
        IoError::Raw(match error {
            lightcraft_raw::RawError::Unsupported(reason) => lightcraft_raw::RawError::Unsupported(format!("{format}: {reason}")),
            other => other,
        })
    })?;
    if !info.raw {
        let preview = lightcraft_engine::files::embedded_preview_srgb(bytes, usize::MAX)
            .ok_or_else(|| IoError::Raw(lightcraft_raw::RawError::Unsupported(format!("{format}: no embedded preview"))))?;
        let img = Image::from_u8(preview.width as u32, preview.height as u32, ChannelLayout::Rgba, preview.data.iter().flatten().copied().collect())?;
        let mut result = image_to_document(name, &img)?;
        let reason = lightcraft_raw::decode(bytes).err().map(|e| e.to_string()).unwrap_or_else(|| "unsupported raw variant".into());
        result.warnings.insert(
            0,
            format!(
                "{format}: {reason}; opened the camera's embedded {}x{} JPEG preview instead (8-bit, not the raw sensor data)",
                preview.width, preview.height
            ),
        );
        return Ok(result);
    }
    let settings = camera_defaults(&info);
    let mut result = render_source(name, bytes, &src, &info, &settings)?;
    result.warnings.push(format!("{format} developed by LightCraft's pipeline (camera defaults, as-shot white balance) into 16-bit ProPhoto RGB"));
    Ok(result)
}

/// Default development shared with Library's raw open route.
fn camera_defaults(info: &lightcraft_pipeline::SourceInfo) -> lightcraft_develop::DevelopSettings {
    let (temp, tint) = if info.relative_wb { (6500.0, 0.0) } else { (info.as_shot_temp, info.as_shot_tint) };
    let mut settings = lightcraft_develop::DevelopSettings::for_raw(temp, tint);
    settings.optics.lens_profile = info.lens.is_some();
    settings
}

/// The last full-size decode [`develop_with_settings`] made, by content hash. Coming back from
/// Develop with new settings re-develops a Develop layer from the same file: decoding a raw
/// (demosaic, profiles) was over half of that wait (1.3 s of 2.7 s for a 21 MP ARW on a Ryzen
/// 9800X3D, measured with `bench_develop_open`). One entry, and only up to
/// [`DECODE_CACHE_MAX_BYTES`], so it can't hold much memory.
static LAST_DECODE: std::sync::Mutex<Option<(([u8; 32], u64), Arc<lightcraft_raster::Rgb32f>, lightcraft_pipeline::SourceInfo)>> = std::sync::Mutex::new(None);

/// The largest decoded source (linear float RGB) [`LAST_DECODE`] keeps: about 64 MP.
const DECODE_CACHE_MAX_BYTES: usize = 768 << 20;

#[cfg(test)]
thread_local! {
    static DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn decoded_source(
    bytes: &[u8],
    opts: &lightcraft_engine::files::RawOptions,
) -> Result<(Arc<lightcraft_raster::Rgb32f>, lightcraft_pipeline::SourceInfo), IoError> {
    let key = (*blake3::hash(bytes).as_bytes(), opts.key());
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
    let (src, info) = lightcraft_engine::files::load_bytes_with(bytes, usize::MAX, opts).map_err(IoError::Unsupported)?;
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
    let settings: lightcraft_develop::DevelopSettings =
        serde_json::from_value(settings.clone()).map_err(|e| IoError::Unsupported(format!("develop settings: {e}")))?;
    let (src, info) = decoded_source(bytes, &lightcraft_engine::files::RawOptions::of(&settings))?;
    render_source(name, bytes, &src, &info, &settings)
}

fn render_source(
    name: &str,
    bytes: &[u8],
    src: &Arc<lightcraft_raster::Rgb32f>,
    info: &lightcraft_pipeline::SourceInfo,
    settings: &lightcraft_develop::DevelopSettings,
) -> Result<ImportResult, IoError> {
    use lightcraft_pipeline::{OutputDepth, OutputSpace, RenderRequest};
    let req = RenderRequest { space: OutputSpace::ProPhoto, depth: OutputDepth::U16, ..RenderRequest::fit(src.width, src.height) };
    let rendered = lightcraft_engine::media::develop(src, info, settings, &req, None, true);
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
    use lightcraft_raw::testgen::{DngSpec, mosaic, scene};

    static CACHE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let _test = CACHE_TEST.lock().unwrap();
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

    #[test]
    fn develop_raw_options_are_used_and_part_of_the_decode_cache_key() {
        let _test = CACHE_TEST.lock().unwrap();
        let bytes = dng(52, 36, [0.6, 1.0, 0.8]);
        let a: lightcraft_develop::DevelopSettings = serde_json::from_value(serde_json::json!({})).unwrap();
        let b: lightcraft_develop::DevelopSettings = serde_json::from_value(serde_json::json!({"raw": {"demosaic": "rcd", "highlights": "clip"}})).unwrap();
        let oa = lightcraft_engine::files::RawOptions::of(&a);
        let ob = lightcraft_engine::files::RawOptions::of(&b);
        assert_ne!(oa.key(), ob.key());
        let (src_a, _) = decoded_source(&bytes, &oa).unwrap();
        let (src_b, info_b) = decoded_source(&bytes, &ob).unwrap();
        assert!(!Arc::ptr_eq(&src_a, &src_b), "different raw options decode again");
        let (expected, _) = lightcraft_engine::files::load_bytes_with(&bytes, usize::MAX, &ob).unwrap();
        assert_eq!(src_b.data, expected.data);
        let req = lightcraft_pipeline::RenderRequest {
            space: lightcraft_pipeline::OutputSpace::ProPhoto,
            depth: lightcraft_pipeline::OutputDepth::U16,
            ..lightcraft_pipeline::RenderRequest::fit(src_b.width, src_b.height)
        };
        let expected = lightcraft_engine::media::develop(&src_b, &info_b, &b, &req, None, true).deep.unwrap();
        let lightcraft_pipeline::DeepSamples::U16(samples) = expected.samples else { panic!("U16") };
        let img = Image::from_u16(expected.width as u32, expected.height as u32, ChannelLayout::Rgb, &samples).unwrap();
        let expected = image_to_document("options.dng", &img).unwrap();
        let actual = develop_with_settings("options.dng", &bytes, &serde_json::to_value(&b).unwrap()).unwrap();
        assert_eq!(pixels(&actual), pixels(&expected));
        let (back, _) = decoded_source(&bytes, &oa).unwrap();
        assert_eq!(back.data, src_a.data);
    }
    #[test]
    fn raw_import_matches_library_defaults_and_explicit_develop() {
        let _test = CACHE_TEST.lock().unwrap();
        // A Nikon TIFF/EP raw, not a DNG-only route.
        let (w, h) = (64, 40);
        let bytes = lightcraft_raw::testgen::tiff_ep("NIKON CORPORATION", w, h, &mosaic(&scene(w, h), w, [0, 1, 1, 2], 0, 65535), [0, 1, 1, 2], 16, vec![]);
        assert_eq!(lightcraft_raw::probe(&bytes), Some(lightcraft_raw::RawFormat::Nef));
        let (src, info) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX).unwrap();
        assert!(info.raw);
        let settings = camera_defaults(&info);
        let actual = import_raw("camera.nef", &bytes).unwrap();
        let explicit = develop_with_settings("camera.nef", &bytes, &settings.to_json()).unwrap();
        assert_eq!(pixels(&actual), pixels(&explicit));
        let expected = render_source("camera.nef", &bytes, &Arc::new(src), &info, &settings).unwrap();
        assert_eq!(pixels(&actual), pixels(&expected));
        assert_eq!(actual.document.icc_profile, explicit.document.icc_profile);
    }

    #[test]
    #[ignore = "requires a hardware GPU; run explicitly with --ignored --nocapture"]
    fn raw_shared_real_gpu_parity() {
        let _test = CACHE_TEST.lock().unwrap();
        assert!(lightcraft_gpu::available(), "hardware GPU is required");
        let adapter = lightcraft_gpu::adapter_name().unwrap();
        assert!(!adapter.to_lowercase().contains("llvmpipe"), "software adapter is not a GPU validation");
        eprintln!("shared raw GPU parity: {adapter}");
        let bytes = dng(96, 64, [0.6, 1.0, 0.8]);
        let (src, info) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX).unwrap();
        let settings = camera_defaults(&info);
        let req = lightcraft_pipeline::RenderRequest {
            space: lightcraft_pipeline::OutputSpace::ProPhoto,
            depth: lightcraft_pipeline::OutputDepth::U16,
            ..lightcraft_pipeline::RenderRequest::fit(src.width, src.height)
        };
        let rendered = lightcraft_gpu::render(&Arc::new(src), &info, &settings, &req, None).expect("must actually render on the GPU");
        let deep = rendered.deep.unwrap();
        let lightcraft_pipeline::DeepSamples::U16(samples) = deep.samples else { panic!("16-bit output") };
        let img = Image::from_u16(deep.width as u32, deep.height as u32, ChannelLayout::Rgb, &samples).unwrap();
        let expected = image_to_document("gpu.dng", &img).unwrap();
        let t = std::time::Instant::now();
        let actual = import_raw("gpu.dng", &bytes).unwrap();
        eprintln!("shared import path {}x{}: {:.2} ms", deep.width, deep.height, t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(pixels(&actual), pixels(&expected));
    }
}
