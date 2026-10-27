//! Coverage tests, one module per source module; a single test binary per crate keeps link time down.
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

mod abr_map_coverage;
mod annotations_map_coverage;
mod comps_map_coverage;
mod develop_filter_coverage;
mod effects_map_coverage;
mod smart_map_coverage;
mod vector_map_coverage;
