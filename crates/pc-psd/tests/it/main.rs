//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod builder;
#[cfg(feature = "corpus")]
mod corpus;
mod malformed;
mod merged_recovery;
mod proptests;
mod roundtrip;
mod tree;
