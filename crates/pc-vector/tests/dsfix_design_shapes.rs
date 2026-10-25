#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

// Compile the detached drawing module without changing production crate registrations.
#[path = "../src/design_shapes.rs"]
pub mod design_shapes;
