//! Pure vector projections and non-destructive vector effects; pc-vector remains the rasterizer.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
pub mod brush;
pub mod envelope;
pub mod repeat;
pub mod width;

pub mod blend;
pub mod sweep;

pub mod path_effects;
