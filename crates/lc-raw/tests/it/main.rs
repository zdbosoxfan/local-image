//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod corpus;
mod dng;
mod ljpeg_opcodes_dngwrite_coverage;
mod raw_speed;
mod rawler_contract;
mod robust;
mod vendor_robust;
