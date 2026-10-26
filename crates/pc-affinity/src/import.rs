//! Local Image scene adapter for the pinned Affinity reader. No filesystem or implicit resources.
use crate::{Error, Limits, model};
use photocraft_doc::{
    DesignUnits, Size,
    vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning},
};
use std::sync::atomic::{AtomicBool, Ordering};
mod effects;
mod paint;
mod scene;
mod text;

pub(crate) fn cancelled(cancel: Option<&AtomicBool>) -> Result<(), Error> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) { Err(Error::Cancelled) } else { Ok(()) }
}
fn policy(bytes: &[u8], options: &ImportOptions) -> Result<Limits, Error> {
    if bytes.len() > options.limits.bytes {
        return Err(Error::Limit("input bytes"));
    }
    if !options.dpi.is_finite() || !(1.0..=100_000.0).contains(&options.dpi) {
        return Err(Error::Malformed("invalid DPI"));
    }
    let total = usize::try_from(options.limits.decompressed_bytes).map_err(|_| Error::Limit("decompressed bytes"))?;
    Ok(Limits {
        max_entry: total.min(256 << 20),
        max_total: total,
        max_objects: options.limits.objects,
        max_values: (total / 48).min(crate::stream::MAX_VALUES),
        max_depth: options.limits.depth.saturating_mul(3).saturating_add(6).min(crate::stream::MAX_DEPTH),
        max_segments: options.limits.segments,
        max_pixels: options.limits.pixels,
    })
}
pub fn import(bytes: &[u8], options: &ImportOptions) -> Result<ImportedScene, Error> {
    import_with(bytes, options, None, &AtomicBool::new(false))
}
/// Native import only. Cancellation/limit/corruption errors never turn into a successful empty scene.
/// Geometry is converted from stored document DPI to options.dpi, retaining physical dimensions.
pub fn import_with(
    bytes: &[u8],
    options: &ImportOptions,
    fonts: Option<&mut photocraft_text::TextEngine>,
    cancel: &AtomicBool,
) -> Result<ImportedScene, Error> {
    cancelled(Some(cancel))?;
    let mut limits = policy(bytes, options)?;
    let mut archive = crate::Archive::open_with(bytes, limits, Some(cancel))?;
    let data = archive.read("doc.dat")?;
    let stream = crate::stream::parse_with(&data, limits, Some(cancel))?;
    let stream_depth = limits.max_depth;
    limits.max_depth = options.limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH);
    archive.limits = limits;
    // Stream parsing and layer recursion have different nesting conventions.
    let mut reader = model::Reader::new(&stream, &mut archive);
    reader.stream_depth = stream_depth;
    let native = reader.document()?;
    let bytes = reader.budget.owned as u64;
    scene::convert(native, options, fonts, cancel, bytes.saturating_add(archive.extracted() as u64))
}
/// Explicit read-only recovery, intended for Open after a native unsupported-layout error.
/// A preview remains at thumbnail dimensions and is visibly identified as flattened.
/// Callers must never recover cancellation, allocation-limit or CRC/malformed failures.
pub fn import_preview(bytes: &[u8], options: &ImportOptions, cancel: &AtomicBool, reason: &str) -> Result<ImportedScene, Error> {
    policy(bytes, options)?;
    cancelled(Some(cancel))?;
    if reason.len() > 4096 {
        return Err(Error::Limit("preview reason bytes"));
    }
    let preview = crate::preview(bytes)?;
    if preview.png.len() as u64 > options.limits.decompressed_bytes {
        return Err(Error::Limit("encoded preview bytes"));
    }
    if u64::from(preview.width) * u64::from(preview.height) > options.limits.pixels {
        return Err(Error::Limit("preview pixels"));
    }
    let image = model::Image {
        profile: None,
        width: preview.width,
        height: preview.height,
        pixels: model::Pixels::Encoded(preview.png.to_vec()),
        transform: model::Affine::IDENTITY,
    };
    let size = Size::new(preview.width, preview.height);
    let mut state = scene::State::new(options, None, cancel, size, preview.png.len() as u64);
    let surface = state.image(&image, photocraft_doc::Affine::IDENTITY)?;
    Ok(ImportedScene {
        size,
        resolution_dpi: options.dpi,
        units: DesignUnits::Pixels,
        layers: vec![photocraft_doc::Layer::new("Affinity preview", photocraft_doc::LayerContent::Raster(surface))],
        symbols: vec![],
        warnings: vec![VectorWarning {
            feature: "preview-fallback".into(),
            object: Some("Affinity preview".into()),
            message: format!("Flattened embedded preview at its own dimensions: {reason}"),
        }],
    })
}
