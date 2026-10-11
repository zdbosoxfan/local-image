//! Repeat transforms adapted from VectorCraft crates/doc/src/pattern.rs, at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (file commit 5afc78a1002cc217f743f74cea9dcdf640318b0a).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; see licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Local Image keeps the original group children and uses radians. Grid spacing is the
//! gap between geometric source bounds, including negative overlap; it is not a pitch.

pub use photocraft_doc::vector_design::live::RepeatSpec;
use photocraft_doc::{Affine, Layer, LayerContent};
use photocraft_geom::Point;
use std::{f64::consts::TAU, fmt};

mod projection;
pub use projection::{Expansion, expand_layers, expand_scene, source_bounds};

pub const MAX_INSTANCES: u32 = 1_000;
pub const MAX_LAYERS: usize = 100_000;
pub const MAX_KNOTS: usize = 1_000_000;
pub const MAX_COORDINATE: f64 = 1_000_000.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepeatError(pub String);
impl fmt::Display for RepeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RepeatError {}
pub type Result<T> = std::result::Result<T, RepeatError>;
pub(crate) fn error(message: impl Into<String>) -> RepeatError {
    RepeatError(message.into())
}
pub fn number(v: f64) -> Result<()> {
    if !v.is_finite() || v.abs() > MAX_COORDINATE { Err(error("repeat input must be finite and bounded")) } else { Ok(()) }
}

/// Geometric source bounds (without stroke/effect outsets), in document pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 2],
    pub max: [f64; 2],
}
impl Bounds {
    pub fn new(min: [f64; 2], max: [f64; 2]) -> Result<Self> {
        for v in min.into_iter().chain(max) {
            number(v)?;
        }
        if min[0] > max[0] || min[1] > max[1] {
            return Err(error("inverted repeat bounds"));
        }
        Ok(Self { min, max })
    }
    pub fn width(self) -> f64 {
        self.max[0] - self.min[0]
    }
    pub fn height(self) -> f64 {
        self.max[1] - self.min[1]
    }
    pub fn center(self) -> Point {
        Point::new((self.min[0] + self.max[0]) / 2., (self.min[1] + self.max[1]) / 2.)
    }
    pub fn union(self, b: Self) -> Self {
        Self { min: [self.min[0].min(b.min[0]), self.min[1].min(b.min[1])], max: [self.max[0].max(b.max[0]), self.max[1].max(b.max[1])] }
    }
    pub fn transform(self, a: Affine) -> Result<Self> {
        let points = [[self.min[0], self.min[1]], [self.max[0], self.min[1]], [self.min[0], self.max[1]], self.max].map(|p| a.apply(Point::new(p[0], p[1])));
        Self::new(
            [points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min), points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min)],
            [points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max), points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max)],
        )
    }
}

/// Validate counts before multiplication/allocation; never clamp malformed saved parameters.
pub fn instance_count(spec: &RepeatSpec) -> Result<u32> {
    let n = match spec {
        RepeatSpec::Radial { count, center, angle } => {
            for v in center.iter().copied().chain([*angle]) {
                number(v)?;
            }
            *count
        }
        RepeatSpec::Grid { rows, cols, spacing } => {
            for v in spacing {
                number(*v)?;
            }
            if *rows == 0 || *cols == 0 {
                return Err(error("repeat rows and columns must be positive"));
            }
            rows.checked_mul(*cols).ok_or_else(|| error("repeat instance count overflow"))?
        }
        RepeatSpec::Mirror { origin, angle } => {
            for v in origin.iter().copied().chain([*angle]) {
                number(v)?;
            }
            2
        }
    };
    if n == 0 || n > MAX_INSTANCES {
        return Err(error("repeat instance limit exceeded (1–1000)"));
    }
    Ok(n)
}

/// Deterministic bottom-to-top instance transforms. Radial `angle` is the starting angle,
/// followed by a complete n-fold turn. Mirror includes the source then its reflection.
pub fn transforms(spec: &RepeatSpec, bounds: Bounds) -> Result<Vec<Affine>> {
    Bounds::new(bounds.min, bounds.max)?;
    let n = instance_count(spec)?;
    let out = match spec {
        RepeatSpec::Radial { center, angle, .. } => {
            let pre = Affine::translate(-center[0], -center[1]);
            let post = Affine::translate(center[0], center[1]);
            (0..n).map(|i| post.mul(&Affine::rotate(angle.rem_euclid(TAU) + TAU * f64::from(i) / f64::from(n))).mul(&pre)).collect()
        }
        RepeatSpec::Grid { rows, cols, spacing } => {
            let pitch = [bounds.width() + spacing[0], bounds.height() + spacing[1]];
            if !pitch.iter().all(|p| p.is_finite()) || (*cols > 1 && pitch[0] < 1e-3) || (*rows > 1 && pitch[1] < 1e-3) {
                return Err(error("repeat grid pitch must be at least 0.001 pixels"));
            }
            (0..*rows).flat_map(|row| (0..*cols).map(move |col| Affine::translate(f64::from(col) * pitch[0], f64::from(row) * pitch[1]))).collect()
        }
        RepeatSpec::Mirror { origin, angle } => {
            let (s, c) = (2. * angle.rem_euclid(TAU)).sin_cos();
            let reflection = Affine { m: [c, s, s, -c, 0., 0.] };
            vec![Affine::IDENTITY, Affine::translate(origin[0], origin[1]).mul(&reflection).mul(&Affine::translate(-origin[0], -origin[1]))]
        }
    };
    for a in &out {
        for v in a.m {
            if !v.is_finite() {
                return Err(error("repeat transform must be finite"));
            }
        }
        bounds.transform(*a)?;
    }
    Ok(out)
}

/// No renderer/cache is needed for shapes. Cache-only content requires an explicit bounds
/// provider first; absent caches are errors, never invisible successful instances.
fn plain_bounds(layer: &Layer) -> Result<Option<Bounds>> {
    let b = match &layer.content {
        LayerContent::Shape(s) => photocraft_pathops::bounds(&s.path).map_err(|e| error(e.to_string()))?,
        LayerContent::Group(_) => return Ok(None),
        _ => {
            let surface = layer.surface().ok_or_else(|| error("repeat source requires a bounds provider for this content"))?;
            let r = surface.content_bounds();
            if r.is_empty() {
                return Ok(None);
            }
            Some((f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)))
        }
    };
    b.map(|b| Bounds::new([b.0, b.1], [b.2, b.3])).transpose()
}
