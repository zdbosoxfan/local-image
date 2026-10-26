//! Single integration-test binary for this crate (one link instead of one per file).
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

mod agent_tasks;
mod mcp;
mod smart_filter_authorization;
