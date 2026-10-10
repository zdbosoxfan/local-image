//! Equivalence + toolset GPU tests in one binary. `memory`, `fallback` and `quiesce` stay separate:
//! they assert on process-global GPU state (buffer-pool accounting, fatal-fault latch until
//! `reset_failures`, exit-time quiesce barrier) that parallel renders would disturb.
mod equivalence;
mod toolset;
