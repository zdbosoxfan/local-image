//! Coverage tests, one module per source module; a single test binary per crate keeps link time down.
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod adjust_coverage;
mod doc_model_coverage;
mod slices_coverage;
mod vector_text_coverage;
