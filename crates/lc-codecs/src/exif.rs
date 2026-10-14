//! Compatibility facade: EXIF directory handling lives in the shared TIFF library.
pub use lightcraft_tiff::exif::minimal_exif;
pub(crate) use lightcraft_tiff::exif::summarize;
