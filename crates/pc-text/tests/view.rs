#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

pub use photocraft_text::*;
#[path = "../src/view.rs"]
pub mod view;
