//! Adapted from VectorCraft `crates/pathops/src/lib.rs` at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//!
//! Adapted calculation kernel; public document APIs are in the crate root.
#[path = "boolean.rs"]
mod boolean;
#[path = "edit.rs"]
mod edit;
#[path = "fit.rs"]
pub(crate) mod fit;
#[path = "offset.rs"]
mod offset;
#[path = "pathfinder.rs"]
mod pathfinder;
#[path = "planar.rs"]
mod planar;

pub use boolean::{BoolOp, DEFAULT_PRECISION, area, boolean, boolean_n, normalize, try_boolean, try_normalize, unite_all};
pub use edit::{
    AverageAxis, SimplifyOptions, add_anchor_points, average, join, remove_anchor, remove_redundant_points, simplify, simplify_with, smooth, split_into_grid,
};
pub use offset::{Cap, Join, offset_path, outline_stroke, stroke_region};
pub use pathfinder::{FaceMerger, PathfinderOp, Region, Shape, merge_regions, pathfinder, region_at, regions};
pub use planar::{BuilderArrangement, SHAPE_BUILDER_MAX_SEGMENTS, cut_out, encloses_area, interior_point, live_paint, shape_builder};
