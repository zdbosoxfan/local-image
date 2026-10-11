//! Spine arc-length placement adapted from VectorCraft doc/blend.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Uses V1's curve-length metric and native exact curve tangents; lookup is bounded.
use super::{Result, correspondence::section, error, validate_path};
use photocraft_doc::Path;
use photocraft_geom::Point;
use photocraft_pathops::geom::{self, ParamCurve as _};

#[derive(Clone, Debug)]
pub struct Spine {
    segments: Vec<geom::SubPath>,
    cumulative: Vec<f64>,
    length: f64,
}
impl Spine {
    pub fn new(path: &Path) -> Result<Self> {
        validate_path(path)?;
        if path.subpaths.len() != 1 || path.subpaths[0].knots.len() < 2 || path.subpaths[0].knots.len() > 128 || path.inverted {
            return Err(error("spine requires one non-inverted path with 2 to 128 knots"));
        }
        let geometry = photocraft_pathops::to_geometry(path);
        let s = &geometry.subpaths[0];
        let mut segments = Vec::new();
        let mut cumulative = vec![0.];
        let mut length = 0.;
        for i in 0..s.segment_count() {
            let part = section(s, i, 0., 1.);
            let metric = geom::PathData::single(part.clone()).length();
            if metric > 1e-12 {
                length += metric;
                segments.push(part);
                cumulative.push(length);
            }
        }
        if segments.is_empty() || length <= 1e-9 || !length.is_finite() {
            return Err(error("blend spine has no finite positive length"));
        }
        Ok(Self { segments, cumulative, length })
    }
    pub fn length(&self) -> f64 {
        self.length
    }
    pub fn at(&self, fraction: f64) -> Result<(Point, f64)> {
        if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
            return Err(error("invalid spine fraction"));
        }
        let distance = fraction * self.length;
        let i = self.cumulative.partition_point(|v| *v <= distance).saturating_sub(1).min(self.segments.len() - 1);
        let part = &self.segments[i];
        let along = distance - self.cumulative[i];
        let segment_length = self.cumulative[i + 1] - self.cumulative[i];
        if part.segment_is_line(0) {
            let curve = part.segment(0);
            let p = curve.p0.lerp(curve.p3, (along / segment_length).clamp(0., 1.));
            let direction = curve.p3 - curve.p0;
            return Ok((Point::new(p.x, p.y), direction.y.atan2(direction.x)));
        }
        let t = if along <= 0. {
            0.
        } else if along >= segment_length {
            1.
        } else {
            // Native curve length rather than chord length: bounded 32-step inverse lookup.
            let (mut lo, mut hi) = (0., 1.);
            for _ in 0..32 {
                let mid = (lo + hi) * 0.5;
                if geom::PathData::single(section(part, 0, 0., mid)).length() < along {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) * 0.5
        };
        let p = part.segment(0).eval(t);
        let bez = geom::PathData::single(part.clone()).to_bezpath();
        let segments = geom::bez::segments(bez.elements());
        let segment = segments.first().ok_or_else(|| error("spine segment is empty"))?;
        let tangent = geom::bez::tangent(segment, t);
        Ok((Point::new(p.x, p.y), tangent.y.atan2(tangent.x)))
    }
}
