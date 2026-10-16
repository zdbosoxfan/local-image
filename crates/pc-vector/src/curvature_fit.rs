//! Interpolating Curvature spline, adapted from VectorCraft `tools/src/draw2.rs::catmull_rom`
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Clicks are interpolation constraints, never approximated samples. Smooth interior knots
//! have the same incoming/outgoing derivative; corner handles retract independently.

use photocraft_doc::{Knot, Path, Subpath};
use photocraft_geom::Point;

pub const MAX_POINTS: usize = 16_384;

/// Uniform Catmull–Rom converted directly to native cubics. Open endpoints and explicit
/// corners have retracted handles, as in the pinned tool. Coincident points remain distinct.
pub fn fit(points: &[(Point, bool)], closed: bool) -> Result<Path, String> {
    if points.len() > MAX_POINTS {
        return Err("Curvature exceeds the point budget".into());
    }
    if points.iter().any(|(p, _)| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > crate::edit::MAX_COORDINATE || p.y.abs() > crate::edit::MAX_COORDINATE) {
        return Err("Curvature points must be finite and bounded".into());
    }
    if closed && points.len() < 3 {
        return Err("a closed Curvature path needs at least three points".into());
    }
    if points.is_empty() {
        return Ok(Path::default());
    }
    let n = points.len();
    let knots = points
        .iter()
        .enumerate()
        .map(|(i, &(p, corner))| {
            if corner || n < 3 || (!closed && (i == 0 || i + 1 == n)) {
                return Knot::corner(p.x, p.y);
            }
            let (prev, next) = (points[(i + n - 1) % n].0, points[(i + 1) % n].0);
            let (x, y) = ((next.x - prev.x) / 6.0, (next.y - prev.y) / 6.0);
            if x.hypot(y) < 1e-9 { Knot::corner(p.x, p.y) } else { Knot::smooth(p, Point::new(p.x - x, p.y - y), Point::new(p.x + x, p.y + y)) }
        })
        .collect();
    let path = Path::new(vec![Subpath { knots, closed, ..Default::default() }]);
    crate::edit::validate(&path)?;
    Ok(path)
}

#[cfg(test)]
#[path = "curvature_fit/tests.rs"]
mod tests;
