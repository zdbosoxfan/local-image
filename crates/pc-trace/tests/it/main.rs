//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::allow_attributes_without_reason, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]

mod benchmark;
mod common;
mod engine;
mod gpu_parity;
mod regenerate;

#[path = "../centerline/benchmark.rs"]
mod centerline_benchmark;
#[path = "../centerline/routing.rs"]
mod centerline_routing;

#[path = "../cutout/benchmark.rs"]
mod cutout_benchmark;
