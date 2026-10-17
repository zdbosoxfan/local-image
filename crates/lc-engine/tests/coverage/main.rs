//! Coverage tests, one module per source module; a single test binary per crate keeps link time down.

// Use the same complete compute-device lifetime guard as GPU integration tests.
#[path = "../../../lc-gpu/tests/support/mod.rs"]
mod gpu_support;

mod devices_coverage;
mod enhance_coverage;
mod files_coverage;
mod import_coverage;
mod lens_db_coverage;
mod library_coverage;
mod media_coverage;
mod memory_coverage;
mod merge_coverage;
mod preset_import_coverage;
mod preset_luminar_coverage;
mod rename_coverage;
mod segment_coverage;
mod sidecar_coverage;
mod smart_sort_bursts_coverage;
mod smart_sort_examples_coverage;
mod smart_sort_plan_coverage;
mod smart_sort_sessions_coverage;
mod smart_sort_store_coverage;
mod smart_sort_tokens_coverage;
