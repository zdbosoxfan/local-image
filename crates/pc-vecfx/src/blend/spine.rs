//! Spine arc-length placement adapted from VectorCraft doc/blend.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Uses V1's curve-length metric and native exact curve tangents; lookup is bounded.
use super::{Result, correspondence::section, error, validate_path};
use photocraft_doc::Path;
use photocraft_geom::Point;
use photocraft_pathops::geom::{self, ParamCurve as _};

const LENGTH_SAMPLES: usize = 1024;
#[derive(Clone, Debug)]
pub struct Spine {
    segments: Vec<geom::SubPath>,
    tables: Vec<Vec<f64>>,
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
        let mut tables = Vec::new();
        let mut cumulative = vec![0.];
        let mut length = 0.;
        for i in 0..s.segment_count() {
            let part = section(s, i, 0., 1.);
            let metric = geom::PathData::single(part.clone()).length();
            if metric > 1e-12 {
                let mut table = vec![0.];
                if !part.segment_is_line(0) {
                    let mut total = 0.;
                    for j in 0..LENGTH_SAMPLES {
                        total += geom::PathData::single(section(&part, 0, j as f64 / LENGTH_SAMPLES as f64, (j + 1) as f64 / LENGTH_SAMPLES as f64)).length();
                        table.push(total);
                    }
                    // Keep the original native metric at the end of each segment.
                    for value in &mut table {
                        *value *= metric / total;
                    }
                }
                length += metric;
                segments.push(part);
                tables.push(table);
                cumulative.push(length);
            }
        }
        if segments.is_empty() || length <= 1e-9 || !length.is_finite() {
            return Err(error("blend spine has no finite positive length"));
        }
        Ok(Self { segments, tables, cumulative, length })
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
        let curve = part.segment(0);
        if part.segment_is_line(0) {
            let p = curve.p0.lerp(curve.p3, (along / segment_length).clamp(0., 1.));
            let direction = curve.p3 - curve.p0;
            return Ok((Point::new(p.x, p.y), direction.y.atan2(direction.x)));
        }
        let table = &self.tables[i];
        let j = table.partition_point(|v| *v < along).clamp(1, table.len() - 1);
        let local = ((along - table[j - 1]) / (table[j] - table[j - 1]).max(1e-12)).clamp(0., 1.);
        let t = (j - 1) as f64 / LENGTH_SAMPLES as f64 + local / LENGTH_SAMPLES as f64;
        let p = curve.eval(t);
        let u = 1. - t;
        let tangent = (curve.p1 - curve.p0) * (u * u) + (curve.p2 - curve.p1) * (2. * u * t) + (curve.p3 - curve.p2) * (t * t);
        // Retracted handles still have the same endpoint tangent as the native outline.
        let tangent = if tangent.hypot() > 1e-9 {
            tangent
        } else if t < 0.5 {
            [curve.p2, curve.p3].into_iter().map(|p| p - curve.p0).find(|v| v.hypot() > 1e-9).unwrap_or_default()
        } else {
            [curve.p1, curve.p0].into_iter().map(|p| curve.p3 - p).find(|v| v.hypot() > 1e-9).unwrap_or_default()
        };
        Ok((Point::new(p.x, p.y), tangent.y.atan2(tangent.x)))
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_spine_inverse_length_matches_curved_metric_and_tangent() {
        let path = Path::new(vec![photocraft_doc::Subpath {
            closed: false,
            op: Default::default(),
            knots: vec![
                photocraft_doc::Knot::smooth(Point::new(0., 0.), Point::new(0., 0.), Point::new(0., 100.)),
                photocraft_doc::Knot::smooth(Point::new(100., 0.), Point::new(100., 100.), Point::new(100., 0.)),
            ],
        }]);
        let spine = Spine::new(&path).unwrap();
        assert!((spine.length() - 200.).abs() < 1e-5);
        let (point, angle) = spine.at(0.5).unwrap();
        assert!((point.x - 50.).hypot(point.y - 75.) < 1e-6);
        assert!(angle.abs() < 1e-6);
        // This cubic has speed 300*(t*t + (1-t)*(1-t)), hence an analytic arc length.
        for fraction in [0.1, 0.25, 0.75, 0.9] {
            let (p, _) = spine.at(fraction).unwrap();
            let root = (1. - 4. * p.y / 300.).max(0.).sqrt();
            let t = if fraction < 0.5 { (1. - root) / 2. } else { (1. + root) / 2. };
            let measured = 300. * (t - t * t + 2. * t * t * t / 3.);
            assert!((measured - fraction * 200.).abs() < 1e-4);
        }
        assert_eq!(spine.at(0.).unwrap().0, Point::new(0., 0.));
        assert_eq!(spine.at(1.).unwrap().0, Point::new(100., 0.));
        assert!(spine.at(f64::NAN).is_err());
    }
}
