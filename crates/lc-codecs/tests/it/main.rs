//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod bench;
mod corpus;
mod fuzz;
mod roundtrip;
