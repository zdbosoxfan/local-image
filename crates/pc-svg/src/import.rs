//! Bounded, offline SVG import into ordinary editable Local Image layers.
//! Mapping and XML text placeholders adapted from VectorCraft crates/svg/src/import.rs
//! and import/text.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Licensed under MIT OR Apache-2.0; see licenses/vectorcraft-*.

use photocraft_doc::vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "import/css.rs"]
mod css;
#[path = "import/geometry.rs"]
mod geometry;
#[path = "import/paint.rs"]
mod paint;
#[path = "import/preflight.rs"]
mod preflight;
#[path = "import/text.rs"]
mod text;
#[path = "import/tree.rs"]
mod tree;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("invalid SVG: {0}")]
    Invalid(String),
    #[error("SVG exceeds the {0} limit")]
    Limit(&'static str),
    #[error("SVG import cancelled")]
    Cancelled,
}
pub type Result<T> = std::result::Result<T, ImportError>;

/// Open, Place and Paste all consume this same detached exchange scene. Only bundled
/// fonts are used by default. Neither this function nor usvg may read linked files.
pub fn import(bytes: &[u8], options: &ImportOptions) -> Result<ImportedScene> {
    import_with(bytes, options, None, &AtomicBool::new(false))
}

/// A caller may explicitly supply its already populated pc-text engine. No font scanning
/// or resource fetching is initiated here. Cancellation never publishes partial layers.
pub fn import_with(bytes: &[u8], options: &ImportOptions, mut fonts: Option<&mut photocraft_text::TextEngine>, cancel: &AtomicBool) -> Result<ImportedScene> {
    cancelled(cancel)?;
    let source = preflight::source(bytes, options)?;
    let xml = preflight::xml(&source, options, cancel)?;
    let mut warnings = Vec::new();
    preflight::warnings(&xml, &mut warnings);
    let (prepared, slots) = text::prepare(&source, &xml, options, fonts.as_deref_mut(), &mut warnings)?;
    cancelled(cancel)?;
    let limit = options.limits.decompressed_bytes;
    let opt = usvg::Options {
        dpi: options.dpi,
        // usvg's default string resolver opens files, even with resources_dir=None.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(move |mime, data, _| {
                if data.len() as u64 > limit {
                    return None;
                }
                // Nested SVG/SVGZ images have their own expansion graph. They are deliberately
                // excluded rather than calling usvg's unbounded recursive default resolver.
                match mime {
                    "image/png" => Some(usvg::ImageKind::PNG(data)),
                    "image/jpeg" | "image/jpg" => Some(usvg::ImageKind::JPEG(data)),
                    "image/webp" => Some(usvg::ImageKind::WEBP(data)),
                    _ => None,
                }
            }),
        },
        ..Default::default()
    };
    let normalized = usvg::Tree::from_str(&prepared, &opt).map_err(|e| ImportError::Invalid(e.to_string()))?;
    cancelled(cancel)?;
    tree::convert(&normalized, &xml, slots, options, fonts, cancel, warnings)
}

pub(super) fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) { Err(ImportError::Cancelled) } else { Ok(()) }
}
pub(super) fn warn(warnings: &mut Vec<VectorWarning>, feature: &str, object: &str, message: &str) {
    // One warning per feature; preserve the first affected object's label for navigation.
    if !warnings.iter().any(|w| w.feature == feature) {
        warnings.push(VectorWarning { feature: feature.into(), object: (!object.is_empty()).then(|| object.into()), message: message.into() });
    }
}
