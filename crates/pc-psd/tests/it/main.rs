//! Single integration-test binary for this crate (one link instead of one per file).
mod builder;
#[cfg(feature = "corpus")]
mod corpus;
mod malformed;
mod proptests;
mod roundtrip;
mod tree;
