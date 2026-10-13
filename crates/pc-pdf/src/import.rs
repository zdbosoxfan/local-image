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

/// A compatibility limitation with an explicit, independently contained image route.
/// This is not admission to the editable decoder, nor proof the rest of a file is safe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFallbackReason {
    ImageCodec,
    CalculatorFunction,
}
impl ImportError {
    pub fn raster_fallback_reason(&self) -> Option<RasterFallbackReason> {
        match self {
            Self::Unsupported("JPEG 2000 image decoder" | "JBIG2 image decoder") => Some(RasterFallbackReason::ImageCodec),
            Self::Unsupported("PostScript calculator functions") => Some(RasterFallbackReason::CalculatorFunction),
            _ => None,
        }
    }
}

/// The bounded header recognition shared by native Open/Place and preflight.
pub fn header_offset(bytes: &[u8]) -> Option<usize> {
    let offset = bytes[..bytes.len().min(1029)].windows(5).position(|s| s == b"%PDF-")?;
    bytes.get(offset + 5..offset + 8).filter(|v| v[0].is_ascii_digit() && v[1] == b'.' && v[2].is_ascii_digit()).map(|_| offset)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod fallback_boundary_tests {
    use super::*;
    #[test]
    fn shared_header_window_and_fallback_refusals_are_explicit() {
        for offset in [0, 1024] {
            let mut bytes = vec![b' '; offset];
            bytes.extend_from_slice(b"%PDF-1.7\n");
            assert_eq!(header_offset(&bytes), Some(offset));
        }
        let mut late = vec![b' '; 1025];
        late.extend_from_slice(b"%PDF-1.7\n");
        assert_eq!(header_offset(&late), None);
        assert_eq!(header_offset(b"%PDF-x.y"), None);
        assert!(ImportError::Unsupported("JPEG 2000 image decoder").raster_fallback_reason().is_some());
        assert!(ImportError::Unsupported("JBIG2 image decoder").raster_fallback_reason().is_some());
        assert!(ImportError::Unsupported("PostScript calculator functions").raster_fallback_reason().is_some());
        for error in [
            ImportError::Invalid("bad xref".into()),
            ImportError::Limit("pixel"),
            ImportError::Cancelled,
            ImportError::Encrypted,
            ImportError::Unsupported("knockout transparency groups"),
            ImportError::Unsupported("external file streams"),
            ImportError::Unsupported("stream codec has no bounded decoder"),
            ImportError::Unsupported("binary resource reused as page content"),
        ] {
            assert_eq!(error.raster_fallback_reason(), None, "{error}");
        }
    }
}
