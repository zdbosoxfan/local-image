//! Original parent-space constraint arithmetic, following Affinity Designer 2 constraints.
//! Both pins with fixed size centre the unchanged size between the pinned margins. If
//! shrinking would make a stretched size non-positive, refuse the entire resize.
use crate::{EngineError, Result};
use photocraft_doc::vector_design::structure::Constraints;
use photocraft_geom::Affine;

pub const LIMIT: f64 = 1_000_000.0;
pub const MIN_SIZE: f64 = 0.0001;
pub type Box2 = [f64; 4]; // x, y, width, height
pub fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
pub fn finite(value: f64) -> Result<f64> {
    if value.is_finite() && value.abs() <= LIMIT { Ok(value) } else { Err(error("layout value must be finite and within 1000000 pixels")) }
}
pub fn validate_rect(rect: Box2) -> Result<Box2> {
    for value in rect {
        finite(value)?;
    }
    if rect[2] < MIN_SIZE || rect[3] < MIN_SIZE {
        return Err(error("layout size must be positive"));
    }
    finite(rect[0] + rect[2])?;
    finite(rect[1] + rect[3])?;
    Ok(rect)
}
fn axis(old: f64, new: f64, position: f64, size: f64, near: bool, far: bool, fixed: bool) -> Result<(f64, f64)> {
    let ratio = new / old;
    let next_size = if fixed {
        size
    } else if near && far {
        size + new - old
    } else {
        size * ratio
    };
    let next_position = match (near, far) {
        (true, true) if fixed => position + (new - old) / 2.0,
        (true, _) => position,
        (_, true) => new - (old - position - size) - next_size,
        _ => (position + size / 2.0) * ratio - next_size / 2.0,
    };
    finite(next_position)?;
    finite(next_size)?;
    if next_size < MIN_SIZE {
        return Err(error("parent is too small for the pinned margins"));
    }
    Ok((next_position, next_size))
}
/// Rectangles are in the same, unrotated parent coordinate system. A rotated child may
/// be resized via its parent-space bounds; anisotropic scaling can change its angle.
pub fn resize(old: Box2, new: Box2, child: Box2, c: Constraints) -> Result<Box2> {
    validate_rect(old)?;
    validate_rect(new)?;
    validate_rect(child)?;
    let (x, w) = axis(old[2], new[2], child[0] - old[0], child[2], c.left, c.right, c.fixed_width)?;
    let (y, h) = axis(old[3], new[3], child[1] - old[1], child[3], c.top, c.bottom, c.fixed_height)?;
    validate_rect([new[0] + x, new[1] + y, w, h])
}
pub fn mapping(old: Box2, new: Box2) -> Result<Affine> {
    validate_rect(old)?;
    validate_rect(new)?;
    let sx = new[2] / old[2];
    let sy = new[3] / old[3];
    let out = Affine { m: [sx, 0., 0., sy, new[0] - sx * old[0], new[1] - sy * old[1]] };
    validate_affine(out)?;
    Ok(out)
}
pub fn validate_affine(a: Affine) -> Result<()> {
    for value in a.m {
        finite(value)?;
    }
    if a.determinant().abs() < 1e-12 {
        return Err(error("layout transform is singular"));
    }
    Ok(())
}
/// The document Group model has no local transform. Until a parent basis is persisted,
/// engine resizing accepts only document-axis parents. Do not silently use a rotated AABB.
pub fn parent_basis(a: Affine) -> Result<()> {
    validate_affine(a)?;
    if a.m[1].abs() > 1e-12 || a.m[2].abs() > 1e-12 || a.m[0] <= 0. || a.m[3] <= 0. {
        return Err(error("constraint resize requires an unrotated, unsheared parent"));
    }
    Ok(())
}
