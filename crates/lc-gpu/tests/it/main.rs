//! Equivalence + toolset GPU tests in one binary. `memory`, `fallback` and `quiesce` stay separate:
//! they assert on process-global GPU state (buffer-pool accounting, fatal-fault latch until
//! `reset_failures`, exit-time quiesce barrier) that parallel renders would disturb.
mod color_range_bounds;
mod equivalence;
#[path = "../support/mod.rs"]
mod support;
mod toolset;
