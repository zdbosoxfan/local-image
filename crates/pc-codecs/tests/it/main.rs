//! Single integration-test binary for this crate (one link instead of one per file).
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
mod roundtrip;
mod tiff_bench;
mod tiff_pages;
mod tiff_photoshop;
mod tiff_regressions;
mod webp_lossy;
