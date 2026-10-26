//! Coverage tests, one module per source module; a single test binary per crate keeps link time down.
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod journal_coverage;
mod keywords_dates_query_coverage;
mod local_coverage;
mod model_coverage;
mod rules_coverage;
