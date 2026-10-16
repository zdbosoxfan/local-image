//! Filled swept tips for Blob Brush and vector Eraser. Original GPL-3.0-or-later code.
//! A segment sweeps a homothetic ellipse with linearly interpolated diameter: its
//! region is the convex hull of the endpoint tips, including stationary pressure changes.
//! A 96-sided tip has <0.072% relative area error; V1 resolves overlaps/loops.
use photocraft_doc::{Path, Subpath};
use photocraft_geom::Point;
use photocraft_pathops::{self as ops, BoolOp};
use serde::{Deserialize, Serialize};

pub const MAX_SAMPLES: usize = 1024;
pub const MAX_KNOTS: usize = 131_072;
pub const MAX_COORD: f64 = 1_000_000.;
const SIDES: usize = 96;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tip {
    pub size: f64,
    /// Degrees counter-clockwise on the y-down document, as in package 17.
    pub angle: f64,
    pub roundness: f64,
    /// 0 = fixed; 1 = full pressure modulation of diameter.
    pub pressure: f64,
}
impl Default for Tip {
    fn default() -> Self {
        Self { size: 10., angle: 0., roundness: 1., pressure: 0. }
    }
}
impl Tip {
    pub fn validate(self) -> Result<()> {
        for (v, lo, hi) in [(self.size, 0.1, 1000.), (self.angle, -360., 360.), (self.roundness, 0.01, 1.), (self.pressure, 0., 1.)] {
            if !v.is_finite() || !(lo..=hi).contains(&v) {
                return Err("tip values must be finite and within their bounds".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub point: Point,
    pub pressure: f64,
}
impl Sample {
    pub fn validate(self) -> Result<()> {
        point(self.point)?;
        if self.pressure.is_finite() && (0.0..=1.0).contains(&self.pressure) { Ok(()) } else { Err("pressure must be finite and 0..1".into()) }
    }
}
fn point(p: Point) -> Result<()> {
    if [p.x, p.y].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) { Ok(()) } else { Err("sweep coordinate exceeds its finite bounds".into()) }
}
pub fn validate_path(path: &Path) -> Result<()> {
    if path.subpaths.len() > MAX_SAMPLES || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_KNOTS {
        return Err("sweep geometry budget exceeded".into());
    }
    for p in path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        point(p)?;
    }
    Ok(())
}
fn tip_polygon(s: Sample, tip: Tip) -> Vec<Point> {
    let radius = tip.size * (1. - tip.pressure + tip.pressure * s.pressure) / 2.;
    let a = -tip.angle.to_radians();
    (0..SIDES)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / SIDES as f64;
            let (x, y) = (radius * t.cos(), radius * tip.roundness * t.sin());
            Point::new(s.point.x + a.cos() * x - a.sin() * y, s.point.y + a.sin() * x + a.cos() * y)
        })
        .collect()
}
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn hull(mut points: Vec<Point>) -> Path {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    let mut out = Vec::new();
    for p in &points {
        while out.len() >= 2 && cross(out[out.len() - 2], out[out.len() - 1], *p) <= 0. {
            out.pop();
        }
        out.push(*p);
    }
    let lower = out.len();
    for p in points.iter().rev().skip(1) {
        while out.len() > lower && cross(out[out.len() - 2], out[out.len() - 1], *p) <= 0. {
            out.pop();
        }
        out.push(*p);
    }
    out.pop();
    if out.len() < 3 { Path::default() } else { Path::new(vec![Subpath::polygon(&out.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>())]) }
}
/// Includes endpoint caps and the entire continuous sweep between events. Zero-pressure
/// input is a legitimate empty region; callers must not publish an empty edit for it.
pub fn outline(samples: &[Sample], tip: Tip) -> Result<Path> {
    tip.validate()?;
    if samples.is_empty() || samples.len() > MAX_SAMPLES {
        return Err("sweep needs 1..1024 samples".into());
    }
    for s in samples {
        s.validate()?;
    }
    let mut paths = Vec::new();
    if samples.len() == 1 {
        paths.push(hull(tip_polygon(samples[0], tip)));
    } else {
        for pair in samples.windows(2) {
            let mut points = tip_polygon(pair[0], tip);
            points.extend(tip_polygon(pair[1], tip));
            paths.push(hull(points));
        }
    }
    paths.retain(|p| !p.is_empty());
    // Balanced union avoids repeatedly accumulating every preceding contour.
    while paths.len() > 1 {
        paths = paths
            .chunks(2)
            .map(|p| {
                let result = if p.len() == 2 { ops::boolean(&p[0], &p[1], BoolOp::Union).map_err(|e| e.to_string())? } else { p[0].clone() };
                validate_path(&result)?;
                Ok(result)
            })
            .collect::<Result<Vec<_>>>()?;
    }
    let path = paths.pop().unwrap_or_default();
    validate_path(&path)?;
    Ok(path)
}
pub fn rectangle(a: Point, b: Point) -> Result<Path> {
    point(a)?;
    point(b)?;
    if a.x == b.x || a.y == b.y {
        return Ok(Path::default());
    }
    Ok(Path::new(vec![Subpath::polygon(&[
        (a.x.min(b.x), a.y.min(b.y)),
        (a.x.max(b.x), a.y.min(b.y)),
        (a.x.max(b.x), a.y.max(b.y)),
        (a.x.min(b.x), a.y.max(b.y)),
    ])]))
}

#[cfg(test)]
#[path = "../tests/sweep/suite.rs"]
mod tests;
