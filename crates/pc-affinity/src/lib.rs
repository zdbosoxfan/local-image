//! Reader for native Affinity documents (`.af`, and `.afdesign`/`.afphoto`/`.afpub` from
//! Affinity 1 and 2): the archive container, the tagged object stream in `doc.dat`, and the
//! subset of the document model VectorCraft imports.
//!
//! Layouts follow the MIT-licensed afread reference and were checked against public documents
//! saved by Affinity 1.x to 3.x (container versions 8 to 12); see tests/fixtures/SOURCES.md for provenance
//! and the exact supported subset. Nothing here writes Affinity files.
//! Adapted from VectorCraft crates/affinity/src/lib.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![expect(clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed"))]

use std::fmt;

mod access;
pub mod container;
mod geometry;
pub mod model;
pub mod paint;
pub mod preview;
mod raster;
mod shapes;
pub mod stream;
#[cfg(any(test, feature = "synth"))]
pub mod synth;
mod text;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod zstd_security_tests;

pub use container::{Archive, Limits, MAGIC, is_affinity};
pub use model::{Document, read};
pub use preview::{MAX_PREVIEW_BYTES, MAX_PREVIEW_DIMENSION, Preview, preview};

/// Why a file could not be read. Messages are short reasons; callers add advice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Malformed(&'static str),
    Unsupported(&'static str),
    Limit(&'static str),
    Cancelled,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Affinity import cancelled"),
            Self::Malformed(s) => write!(f, "damaged Affinity file: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported Affinity file: {s}"),
            Self::Limit(s) => write!(f, "Affinity file exceeds a safety limit: {s}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

pub mod import;
pub use import::{import, import_preview, import_with};
