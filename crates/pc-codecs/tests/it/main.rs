//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod camera_raw;
mod common;
mod decode_warnings;
mod deep_exr;
mod detect_caps;
mod fidelity;
#[cfg(all(feature = "corpus", feature = "heif"))]
mod heif;
mod limits;
mod malformed;
mod metadata;
mod orientation;
mod png_jpeg;
mod pnm_headers;
mod roundtrip;
mod tga;
mod tiff_bench;
mod tiff_exif;
mod tiff_lab;
mod tiff_pages;
mod tiff_photoshop;
mod tiff_regressions;
mod webp_lossy;
