//! Envelope map design adapted from VectorCraft crates/doc/src/live.rs and
//! crates/effects/src/warp.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Mesh coordinates are normalized to source bounds, as required by pc-doc, rather
//! than upstream's absolute coordinates. Four-sided top objects retain cubic boundaries.
//! Invalid data and exhausted fitting budgets are errors, never identity fallbacks.
use kurbo::{Affine, CubicBez, ParamCurve, Point, Rect};
use photocraft_doc::{Path, vector_design::live::EnvelopeSpec};
use photocraft_pathops::bounds;
#[path = "envelope/coons.rs"]
mod coons;
#[path = "envelope/fit.rs"]
mod fit;
#[path = "envelope/mesh.rs"]
mod mesh;
#[path = "envelope/warp.rs"]
mod warp;
pub use warp::WarpStyle;

pub const MAX_INPUT_NODES: usize = 8192;
pub const MAX_OUTPUT_NODES: usize = 65536;
pub const MAX_GRID_SIDE: u32 = 32;
pub const MAX_COORDINATE: f64 = 1_000_000.0;
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("envelope coordinates must be finite and within one million pixels")]
    Coordinate,
    #[error("invalid envelope bounds, options, style or mesh")]
    Invalid,
    #[error("top object requires a closed outline with four distinct ordered boundary corners")]
    TopObject,
    #[error("envelope fitting exceeds its work, depth or node budget")]
    Budget,
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Document pixels; numerical fitting uses kurbo and an independent sample guard.
    pub tolerance: f64,
    pub max_nodes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self { tolerance: 0.05, max_nodes: MAX_OUTPUT_NODES }
    }
}
impl Options {
    pub fn validate(self) -> Result<Self> {
        if !self.tolerance.is_finite() || !(0.000001..=10.0).contains(&self.tolerance) || !(2..=MAX_OUTPUT_NODES).contains(&self.max_nodes) {
            return Err(Error::Invalid);
        }
        Ok(self)
    }
}
pub fn validate_path(path: &Path) -> Result<()> {
    if path.subpaths.len() > MAX_INPUT_NODES || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_INPUT_NODES {
        return Err(Error::Budget);
    }
    if path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| !valid_point(Point::new(p.x, p.y))) {
        return Err(Error::Coordinate);
    }
    if path.inverted {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub(super) fn valid_point(p: Point) -> bool {
    [p.x, p.y].into_iter().all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
}
pub fn source_bounds(paths: &[Path]) -> Result<Rect> {
    if paths.is_empty() || paths.len() > 1024 {
        return Err(Error::Invalid);
    }
    let mut result: Option<Rect> = None;
    let mut nodes = 0;
    for p in paths {
        validate_path(p)?;
        nodes += p.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
        if nodes > MAX_INPUT_NODES {
            return Err(Error::Budget);
        }
        if let Some((x0, y0, x1, y1)) = bounds(p).map_err(|_| Error::Invalid)? {
            let b = Rect::new(x0, y0, x1, y1);
            result = Some(result.map_or(b, |r| r.union(b)));
        }
    }
    let mut r = result.ok_or(Error::Invalid)?;
    if ![r.x0, r.y0, r.x1, r.y1].into_iter().all(|v| v.is_finite()) {
        return Err(Error::Invalid);
    }
    // A flat source axis has a stable one-pixel frame. Identity stays exact for lines.
    if r.width() < 1e-9 {
        let x = (r.x0 + r.x1) * 0.5;
        r.x0 = (x - 0.5).clamp(-MAX_COORDINATE, MAX_COORDINATE - 1.);
        r.x1 = r.x0 + 1.;
    }
    if r.height() < 1e-9 {
        let y = (r.y0 + r.y1) * 0.5;
        r.y0 = (y - 0.5).clamp(-MAX_COORDINATE, MAX_COORDINATE - 1.);
        r.y1 = r.y0 + 1.;
    }
    Ok(r)
}
#[derive(Clone, Debug)]
enum Surface {
    Affine(Affine),
    Warp(WarpStyle, f64, f64, f64),
    Mesh(usize, usize, Vec<Point>),
    Coons(Box<coons::Coons>),
}
#[derive(Clone, Debug)]
pub struct Map {
    bounds: Rect,
    surface: Surface,
}
impl Map {
    pub fn new(spec: &EnvelopeSpec, bounds: Rect) -> Result<Self> {
        Self::with_tolerance(spec, bounds, Options::default().tolerance)
    }
    fn with_tolerance(spec: &EnvelopeSpec, bounds: Rect, tolerance: f64) -> Result<Self> {
        if !valid_point(Point::new(bounds.x0, bounds.y0)) || !valid_point(Point::new(bounds.x1, bounds.y1)) || bounds.width() < 1e-9 || bounds.height() < 1e-9 {
            return Err(Error::Invalid);
        }
        let surface = match spec {
            EnvelopeSpec::Warp { style, bend, horizontal, vertical } => {
                let style = WarpStyle::from_id(style).ok_or(Error::Invalid)?;
                if ![*bend, *horizontal, *vertical].into_iter().all(|v| v.is_finite() && (-100.0..=100.0).contains(&v)) {
                    return Err(Error::Invalid);
                }
                if *bend == 0.0 && *horizontal == 0.0 && *vertical == 0.0 {
                    Surface::Affine(Affine::IDENTITY)
                } else {
                    Surface::Warp(style, *bend / 100.0, *horizontal / 100.0, *vertical / 100.0)
                }
            }
            EnvelopeSpec::Mesh { rows, cols, points } => {
                if !(1..=MAX_GRID_SIDE).contains(rows) || !(1..=MAX_GRID_SIDE).contains(cols) || points.len() != ((*rows as usize + 1) * (*cols as usize + 1)) {
                    return Err(Error::Invalid);
                }
                if !points.iter().flatten().all(|v| v.is_finite() && v.abs() <= 16.0) {
                    return Err(Error::Coordinate);
                }
                let at = |r: usize, c: usize| {
                    let p = points[r * (*cols as usize + 1) + c];
                    Point::new(p[0], p[1])
                };
                let p = at(0, 0);
                let dx = at(0, *cols as usize) - p;
                let dy = at(*rows as usize, 0) - p;
                let affine = (0..=*rows as usize)
                    .all(|r| (0..=*cols as usize).all(|c| at(r, c).distance(p + dx * (c as f64 / *cols as f64) + dy * (r as f64 / *rows as f64)) < 1e-14));
                if affine {
                    let a = dx.x;
                    let b = dx.y * bounds.height() / bounds.width();
                    let c = dy.x * bounds.width() / bounds.height();
                    let d = dy.y;
                    Surface::Affine(Affine::new([
                        a,
                        b,
                        c,
                        d,
                        bounds.x0 + p.x * bounds.width() - a * bounds.x0 - c * bounds.y0,
                        bounds.y0 + p.y * bounds.height() - b * bounds.x0 - d * bounds.y0,
                    ]))
                } else {
                    Surface::Mesh(*rows as usize, *cols as usize, points.iter().map(|p| Point::new(p[0], p[1])).collect())
                }
            }
            EnvelopeSpec::TopObject { path } => {
                validate_path(path)?;
                Surface::Coons(Box::new(coons::Coons::new(path, (tolerance * 0.125).min(0.005))?))
            }
        };
        Ok(Self { bounds, surface })
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn point(&self, p: Point) -> Result<Point> {
        if !valid_point(p) {
            return Err(Error::Coordinate);
        }
        let u = (p.x - self.bounds.x0) / self.bounds.width();
        let v = (p.y - self.bounds.y0) / self.bounds.height();
        let q = match &self.surface {
            Surface::Affine(a) => *a * p,
            Surface::Warp(style, b, h, d) => {
                let (x, y) = warp::warp_point(*style, *b, *h, *d, 2.0 * u - 1.0, 2.0 * v - 1.0);
                Point::new(self.bounds.x0 + (x + 1.0) * self.bounds.width() / 2.0, self.bounds.y0 + (y + 1.0) * self.bounds.height() / 2.0)
            }
            Surface::Mesh(r, c, pts) => {
                let q = mesh::grid_eval(*r, *c, pts, u, v);
                Point::new(self.bounds.x0 + q.x * self.bounds.width(), self.bounds.y0 + q.y * self.bounds.height())
            }
            Surface::Coons(c) => c.eval(u, v),
        };
        if !valid_point(q) {
            return Err(Error::Coordinate);
        }
        Ok(q)
    }
    pub fn affine(&self) -> Option<Affine> {
        if let Surface::Affine(a) = self.surface { Some(a) } else { None }
    }
}
pub fn identity_mesh(rows: u32, cols: u32) -> Result<EnvelopeSpec> {
    if !(1..=MAX_GRID_SIDE).contains(&rows) || !(1..=MAX_GRID_SIDE).contains(&cols) {
        return Err(Error::Invalid);
    }
    Ok(EnvelopeSpec::Mesh { rows, cols, points: (0..=rows).flat_map(|r| (0..=cols).map(move |c| [c as f64 / cols as f64, r as f64 / rows as f64])).collect() })
}
/// Preserve contour order, PSD component operations, winding and explicit closedness.
/// Node growth is capped at 32× input nodes as well as the caller's global budget.
pub fn evaluate(paths: &[Path], spec: &EnvelopeSpec, options: Options) -> Result<Vec<Path>> {
    let options = options.validate()?;
    let map = Map::with_tolerance(spec, source_bounds(paths)?, options.tolerance)?;
    let input_nodes = paths.iter().flat_map(|p| &p.subpaths).map(|s| s.knots.len()).sum::<usize>();
    let mut budget = options.max_nodes.min(input_nodes.saturating_mul(32));
    let mut work = 0;
    paths.iter().map(|p| fit::path(p, &map, options.tolerance, &mut budget, &mut work)).collect()
}
pub(super) fn cubic(a: &photocraft_doc::Knot, b: &photocraft_doc::Knot) -> CubicBez {
    let p = |p: photocraft_geom::Point| Point::new(p.x, p.y);
    if a.out_ctrl == a.anchor && b.in_ctrl == b.anchor {
        let p0 = p(a.anchor);
        let p3 = p(b.anchor);
        CubicBez::new(p0, p0.lerp(p3, 1.0 / 3.0), p0.lerp(p3, 2.0 / 3.0), p3)
    } else {
        CubicBez::new(p(a.anchor), p(a.out_ctrl), p(b.in_ctrl), p(b.anchor))
    }
}
