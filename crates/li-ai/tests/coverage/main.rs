//! Coverage tests, one module per source module; a single test binary per crate keeps link time down.
#![expect(clippy::let_underscore_must_use, clippy::unused_result_ok, reason = "baseline 2026-10-25: unreviewed")]

mod arch_coverage;
mod browser_coverage;
mod browser_oct9_coverage;
mod catalog_coverage;
mod cloud_coverage;
mod comfy_coverage;
mod curated_graphs_coverage;
mod custom_coverage;
mod download_coverage;
mod download_trash_inventory_coverage;
mod family_coverage;
mod identify_coverage;
mod inventory_coverage;
mod mock_coverage;
mod mock_hub_coverage;
mod ops_coverage;
