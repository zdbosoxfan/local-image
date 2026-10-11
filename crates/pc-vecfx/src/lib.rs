//! Pure vector projections and non-destructive vector effects; pc-vector remains the rasterizer.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
pub mod brush;
pub mod envelope;
pub mod repeat;
pub mod width;

pub mod blend;
pub mod sweep;

pub mod liquify;
pub mod live_paint;
pub mod path_effects;
pub mod puppet;

/// The authoritative advanced-stroke provider used by display and exchange adapters.
pub struct Projection;
impl photocraft_vector::appearance::StrokeProjection for Projection {
    fn outline(&self, path: &photocraft_doc::Path, stroke: &photocraft_doc::vector_design::appearance::StrokeItem) -> Result<photocraft_doc::Path, String> {
        if stroke.brush.is_some() { brush::outline(path, stroke).map_err(|e| e.to_string()) } else { width::outline(path, stroke).map_err(|e| e.to_string()) }
    }
}
