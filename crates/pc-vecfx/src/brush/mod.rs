//! Adapted from VectorCraft crates/brush/src/lib.rs and doc/src/appearance.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Art is ordinary closed native geometry, painted with the stroke's paint. Projection
//! is shared by display and expansion; no upstream object tree or renderer is retained.

mod calli;
mod track;
mod warp;
use photocraft_doc::vector_design::appearance::{BrushRef, StrokeItem, WidthPoint, WidthProfile};
use photocraft_doc::{Path, PathOp, ShapeStroke, Subpath};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

pub const MAX_VERTICES: usize = 131_072;
pub const MAX_INPUT: usize = 8192;
pub const MAX_PIECES: usize = 4096;
pub const MAX_COORD: f64 = 1_000_000.0;
const REF_PREFIX: &str = "local-image:brush:v1:";
// The integrated appearance renderer bounds opaque BrushRef IDs to 4096 bytes.
const MAX_REF_BYTES: usize = 4096 - REF_PREFIX.len();
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("brush input must be finite and within its numeric limits")]
    Invalid,
    #[error("brush stroke or art has zero length or area")]
    Degenerate,
    #[error("brush geometry exceeds its piece or vertex budget")]
    Budget,
    #[error("brush reference is unknown or malformed")]
    Reference,
    #[error("brush geometry could not be normalized")]
    Geometry,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Brush {
    Calligraphic(Calligraphic),
    Art(ArtBrush),
    Scatter(Scatter),
    Pattern(PatternBrush),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calligraphic {
    /// Degrees counter-clockwise on a y-down page.
    pub angle: f64,
    /// Minor/major axis ratio; zero is the analytic flat nib.
    pub roundness: f64,
    pub size: f64,
}
impl Default for Calligraphic {
    fn default() -> Self {
        Self { angle: 45.0, roundness: 0.2, size: 1.0 }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ArtScale {
    #[default]
    Stretch,
    Proportional,
    BetweenGuides {
        start: f64,
        end: f64,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtBrush {
    pub art: Path,
    pub direction: Direction,
    pub scale: ArtScale,
    pub width: f64,
    pub flip_along: bool,
    pub flip_across: bool,
}
impl Default for ArtBrush {
    fn default() -> Self {
        Self {
            art: Path::new(vec![Subpath::polygon(&[(0., 0.), (5., -0.5), (10., 0.), (5., 0.5)])]),
            direction: Direction::default(),
            scale: ArtScale::default(),
            width: 1.,
            flip_along: false,
            flip_across: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scatter {
    pub art: Path,
    pub size: [f64; 2],
    pub spacing: [f64; 2],
    pub scatter: [f64; 2],
    pub rotation: [f64; 2],
    pub relative_to_path: bool,
}
impl Default for Scatter {
    fn default() -> Self {
        Self { art: tile(), size: [0.5, 1.5], spacing: [1., 1.5], scatter: [-1., 1.], rotation: [-90., 90.], relative_to_path: true }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternFit {
    #[default]
    Stretch,
    AddSpace,
    Approximate,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatternBrush {
    pub side: Path,
    pub outer_corner: Option<Path>,
    pub inner_corner: Option<Path>,
    pub start: Option<Path>,
    pub end: Option<Path>,
    pub scale: f64,
    pub spacing: f64,
    pub fit: PatternFit,
    pub flip_along: bool,
    pub flip_across: bool,
}
impl Default for PatternBrush {
    fn default() -> Self {
        Self {
            side: tile(),
            outer_corner: None,
            inner_corner: None,
            start: None,
            end: None,
            scale: 1.,
            spacing: 0.5,
            fit: PatternFit::default(),
            flip_along: false,
            flip_across: false,
        }
    }
}
fn tile() -> Path {
    Path::new(vec![Subpath::polygon(&[(0., -0.25), (1., -0.25), (1., 0.25), (0., 0.25)])])
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub brush: Brush,
    #[serde(default)]
    pub seed: u64,
}
impl Resource {
    /// Self-contained versioned resource in the existing opaque BrushRef slot. Native
    /// persistence already round-trips this slot. These could be interned in a future table;
    /// external names are never silently replaced by a preset.
    pub fn reference(&self) -> Result<BrushRef> {
        self.brush.validate()?;
        let data = serde_json::to_string(self).map_err(|_| Error::Reference)?;
        if data.len() > MAX_REF_BYTES {
            return Err(Error::Budget);
        }
        Ok(BrushRef { id: format!("{REF_PREFIX}{data}") })
    }
    pub fn resolve(reference: &BrushRef) -> Result<Self> {
        if reference.id.len() > MAX_REF_BYTES + REF_PREFIX.len() {
            return Err(Error::Budget);
        }
        let data = reference.id.strip_prefix(REF_PREFIX).ok_or(Error::Reference)?;
        let resource: Self = serde_json::from_str(data).map_err(|_| Error::Reference)?;
        resource.brush.validate()?;
        Ok(resource)
    }
}
pub fn presets() -> Vec<(&'static str, Brush)> {
    vec![
        ("Calligraphic", Brush::Calligraphic(Calligraphic::default())),
        ("Round", Brush::Calligraphic(Calligraphic { angle: 0., roundness: 1., size: 1. })),
        ("Art", Brush::Art(ArtBrush::default())),
        ("Scatter", Brush::Scatter(Scatter::default())),
        ("Pattern", Brush::Pattern(PatternBrush::default())),
    ]
}
fn bounded(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
fn range(v: [f64; 2], min: f64, max: f64) -> bool {
    bounded(v[0], min, max) && bounded(v[1], v[0], max)
}
pub fn validate_path(path: &Path) -> Result<()> {
    let count: usize = path.subpaths.iter().map(|s| s.knots.len()).sum();
    if count > MAX_INPUT || path.subpaths.len() > 256 {
        return Err(Error::Budget);
    }
    if path.inverted
        || path.subpaths.iter().any(|s| !matches!(s.op, PathOp::Combine | PathOp::Join))
        || path
            .subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
            .any(|p| !bounded(p.x, -MAX_COORD, MAX_COORD) || !bounded(p.y, -MAX_COORD, MAX_COORD))
    {
        return Err(Error::Invalid);
    }
    if count < 2 {
        return Err(Error::Degenerate);
    }
    Ok(())
}
fn validate_art(path: &Path) -> Result<()> {
    validate_path(path)?;
    if path.subpaths.iter().any(|s| !s.closed || s.knots.len() < 3) {
        return Err(Error::Invalid);
    }
    let (x0, y0, x1, y1) = path.control_bounds().ok_or(Error::Degenerate)?;
    if x1 - x0 <= 1e-6 || y1 - y0 <= 1e-6 {
        return Err(Error::Degenerate);
    }
    Ok(())
}
impl Brush {
    pub fn validate(&self) -> Result<()> {
        let valid = match self {
            Self::Calligraphic(c) => bounded(c.angle, -360., 360.) && bounded(c.roundness, 0., 1.) && bounded(c.size, 0.01, 100.),
            Self::Art(a) => {
                validate_art(&a.art)?;
                bounded(a.width, 0.01, 100.)
                    && match a.scale {
                        ArtScale::BetweenGuides { start, end } => bounded(start, 0., 1.) && bounded(end, start + 1e-6, 1.),
                        _ => true,
                    }
            }
            Self::Scatter(s) => {
                validate_art(&s.art)?;
                range(s.size, 0.01, 100.) && range(s.spacing, 0.01, 100.) && range(s.scatter, -100., 100.) && range(s.rotation, -360., 360.)
            }
            Self::Pattern(p) => {
                validate_art(&p.side)?;
                for art in [&p.outer_corner, &p.inner_corner, &p.start, &p.end].into_iter().flatten() {
                    validate_art(art)?;
                }
                bounded(p.scale, 0.01, 100.) && bounded(p.spacing, 0., 100.)
            }
        };
        if valid { Ok(()) } else { Err(Error::Invalid) }
    }
}
pub fn validate_profile(profile: &WidthProfile) -> Result<()> {
    if !(2..=MAX_INPUT).contains(&profile.points.len()) {
        return Err(Error::Invalid);
    }
    let mut last = -1.;
    for p in &profile.points {
        if !bounded(p.t, 0., 1.) || p.t <= last || !bounded(p.left, 0., 100_000.) || !bounded(p.right, 0., 100_000.) {
            return Err(Error::Invalid);
        }
        last = p.t;
    }
    if profile.points[0].t != 0. || profile.points[profile.points.len() - 1].t != 1. {
        return Err(Error::Invalid);
    }
    Ok(())
}
/// Width distances are absolute pixels, matching the shared width model.
pub fn pressure_profile(samples: &[(Point, f64)], width: f64) -> Result<WidthProfile> {
    if !bounded(width, 0.1, 1000.) || !(2..=MAX_INPUT).contains(&samples.len()) {
        return Err(Error::Invalid);
    }
    let mut points = Vec::new();
    let mut total = 0.;
    let mut previous: Option<Point> = None;
    for &(p, pressure) in samples {
        if !bounded(p.x, -MAX_COORD, MAX_COORD) || !bounded(p.y, -MAX_COORD, MAX_COORD) || !bounded(pressure, 0., 1.) {
            return Err(Error::Invalid);
        }
        if let Some(a) = previous {
            total += (p.x - a.x).hypot(p.y - a.y);
        }
        let half = width * pressure / 2.;
        if points.last().is_some_and(|q: &WidthPoint| q.t == total) {
            points.pop();
        }
        points.push(WidthPoint { t: total, left: half, right: half });
        previous = Some(p);
    }
    if total <= 1e-9 {
        return Err(Error::Degenerate);
    }
    for p in &mut points {
        p.t /= total;
    }
    let profile = WidthProfile { points };
    validate_profile(&profile)?;
    Ok(profile)
}
pub(crate) fn widths(stroke: &StrokeItem, t: f64) -> (f64, f64) {
    let Some(profile) = &stroke.profile else {
        let half = f64::from(stroke.base.width) / 2.;
        return (half, half);
    };
    let t = t.clamp(0., 1.);
    let i = profile.points.partition_point(|p| p.t < t).clamp(1, profile.points.len() - 1);
    let (a, b) = (profile.points[i - 1], profile.points[i]);
    let u = (t - a.t) / (b.t - a.t);
    (a.left + (b.left - a.left) * u, a.right + (b.right - a.right) * u)
}
/// The only brush evaluator. A failure never turns into successful empty geometry.
pub fn project(path: &Path, stroke: &StrokeItem) -> Result<Vec<Path>> {
    validate_path(path)?;
    validate_stroke(&stroke.base)?;
    if let Some(p) = &stroke.profile {
        validate_profile(p)?;
    }
    let resource = Resource::resolve(stroke.brush.as_ref().ok_or(Error::Reference)?)?;
    let tracks = track::tracks(path, 0.05)?;
    let out = match &resource.brush {
        Brush::Calligraphic(c) => calli::calligraphic(c, &tracks, stroke)?,
        Brush::Art(a) => warp::art(a, &tracks, stroke)?,
        Brush::Scatter(s) => warp::scatter(s, &tracks, stroke, resource.seed)?,
        Brush::Pattern(p) => warp::pattern(p, &tracks, stroke)?,
    };
    if out.is_empty() {
        return Err(Error::Degenerate);
    }
    let mut nodes = 0;
    for p in &out {
        nodes += p.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
        if nodes > MAX_VERTICES || out.len() > MAX_PIECES {
            return Err(Error::Budget);
        }
        if p.subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
            .any(|p| !bounded(p.x, -MAX_COORD, MAX_COORD) || !bounded(p.y, -MAX_COORD, MAX_COORD))
        {
            return Err(Error::Invalid);
        }
    }
    Ok(out)
}
pub fn validate_stroke(s: &ShapeStroke) -> Result<()> {
    if !bounded(f64::from(s.width), 0.1, 1000.)
        || !bounded(f64::from(s.opacity), 0., 1.)
        || !bounded(f64::from(s.miter_limit), 1., 1000.)
        || !bounded(f64::from(s.dash_offset), -MAX_COORD, MAX_COORD)
        || s.align != photocraft_doc::StrokeAlign::Center
        || !s.dashes.is_empty()
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub(crate) fn native(points: &[photocraft_pathops::geom::Point], closed: bool) -> Subpath {
    let pts: Vec<_> = points.iter().map(|p| (p.x, p.y)).collect();
    if closed { Subpath::polygon(&pts) } else { Subpath::polyline(&pts) }
}
pub(crate) fn normalize(path: Path) -> Result<Path> {
    photocraft_pathops::finish_compound(&path, None).map_err(|_| Error::Geometry)
}

/// One normalized brush stroke coverage for the StrokeProjection interface.
/// Pieces are united before opacity/blend is applied, matching ordinary expansion.
pub fn outline(path: &Path, stroke: &StrokeItem) -> Result<Path> {
    let pieces = project(path, stroke)?;
    let mut subpaths = Vec::new();
    for piece in pieces {
        for (i, mut subpath) in piece.subpaths.into_iter().enumerate() {
            if i == 0 {
                subpath.op = PathOp::Combine;
            }
            subpaths.push(subpath);
        }
    }
    let path = normalize(Path::new(subpaths))?;
    let count = path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
    if count > MAX_VERTICES {
        return Err(Error::Budget);
    }
    if path.is_empty() {
        return Err(Error::Degenerate);
    }
    Ok(path)
}
