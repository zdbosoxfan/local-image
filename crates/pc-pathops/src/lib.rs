//! Constructive geometry on Local Image's editable `Path` and `Knot` model.
//!
//! Calculation algorithms are adapted from VectorCraft at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! The transient kurbo adapter never changes the persisted document schema.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod adapter;
pub mod geom;
pub mod kernel;
pub use adapter::*;
pub use kernel::{BoolOp, Cap, DEFAULT_PRECISION, Join, PathfinderOp, SimplifyOptions};
pub use photocraft_vector::{Polyline, flatten_path, flatten_subpath};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathOpsError {
    #[error("input contains NaN or infinite coordinates")]
    NonFinite,
    #[error("input path could not be closed")]
    OpenPath,
    #[error("the shapes are too degenerate to combine")]
    Degenerate,
    #[error("inverted paths need a finite clipping rectangle")]
    NeedsClip,
    #[error("invalid geometry option or segment index")]
    InvalidOption,
}
