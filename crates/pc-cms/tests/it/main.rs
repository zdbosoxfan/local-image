//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod bench;
mod cms;
mod lut_write_coverage;
mod profile_write_coverage;
mod regen;
