//! photocraft-testkit
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed"))]

#[cfg(not(target_arch = "wasm32"))]
pub mod perf;

pub mod vector;
pub mod vector_geometry;
