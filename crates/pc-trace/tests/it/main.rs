//! Single integration-test binary for this crate (one link instead of one per file).
mod benchmark;
mod common;
mod engine;
mod gpu_parity;
mod regenerate;

#[path = "../centerline/benchmark.rs"]
mod centerline_benchmark;
#[path = "../centerline/routing.rs"]
mod centerline_routing;
