//! Bounded PDF import through hayro 0.7.0 Device callbacks, with native editable layers.
//! Mapping adapted from VectorCraft crates/pdf/src/import.rs and pages.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. Copyright (c) 2026 ArtCraft Team
//! and VectorCraft contributors. MIT OR Apache-2.0; see licenses/vectorcraft-*.
//! The preflight is intentionally conservative: codecs without enforceable allocation
//! limits and encrypted streams are refused before hayro is allowed to decode anything.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use photocraft_doc::vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning};
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "import/device.rs"]
mod device;
#[path = "import/geometry.rs"]
mod geometry;
#[path = "import/pages.rs"]
mod pages;
#[path = "import/paint.rs"]
mod paint;
#[path = "import/preflight.rs"]
mod preflight;
pub use pages::{PageInfo, PdfInfo};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageBox {
    #[default]
    Crop,
    Media,
}

/// PDF-specific options supplement, rather than replace, the shared exchange boundary.
/// No font or external resource is read from disk. Embedded font outlines and the bundled
/// standard fourteen substitutes are used. Passwords never enter a journal or document.
#[derive(Clone, Debug, Default)]
pub struct PdfOptions {
    pub page_box: PageBox,
    pub ignore_rotation: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("invalid PDF: {0}")]
    Invalid(String),
    #[error("PDF exceeds the {0} limit")]
    Limit(&'static str),
    #[error("PDF import cancelled")]
    Cancelled,
    #[error("PDF feature cannot be imported safely: {0}")]
    Unsupported(&'static str),
    #[error("encrypted PDF import is unavailable: save a decrypted copy first")]
    Encrypted,
}
pub(super) type Result<T> = std::result::Result<T, ImportError>;

pub fn import(bytes: &[u8], options: &ImportOptions) -> Result<ImportedScene> {
    import_with(bytes, options, &PdfOptions::default(), &AtomicBool::new(false))
}

/// Fully detached import: preflight, decode, prepare bounded caches, then return a scene.
/// On errors/cancellation no partial scene escapes. Multiple selected pages are laid out
/// left-to-right in request order, each within its own editable page clipping group.
pub fn import_with(bytes: &[u8], options: &ImportOptions, pdf_options: &PdfOptions, cancel: &AtomicBool) -> Result<ImportedScene> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pdf = pages::open(bytes, options, cancel)?;
        pages::convert(&pdf, options, pdf_options, cancel)
    }))
    .map_err(|_| ImportError::Invalid("PDF decoder rejected malformed data".into()))?
}

pub fn info(bytes: &[u8], options: &ImportOptions, cancel: &AtomicBool) -> Result<PdfInfo> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pdf = pages::open(bytes, options, cancel)?;
        pages::info(&pdf, options, cancel)
    }))
    .map_err(|_| ImportError::Invalid("PDF decoder rejected malformed data".into()))?
}

pub(super) fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) { Err(ImportError::Cancelled) } else { Ok(()) }
}
pub(super) fn warn(warnings: &mut Vec<VectorWarning>, feature: &str, object: Option<&str>, message: &str) {
    if !warnings.iter().any(|w| w.feature == feature) {
        warnings.push(VectorWarning { feature: feature.into(), object: object.map(str::to_owned), message: message.into() });
    }
}
