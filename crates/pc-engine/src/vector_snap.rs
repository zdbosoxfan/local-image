//! Document-space snapping shared by vector tools. Grid rounding is adapted from
//! VectorCraft crates/geom/src/snap.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; see licenses/vectorcraft-LICENSE-{MIT,APACHE} and NOTICE.
//!
//! A cache belongs to a document revision and an immutable exclusion set. Queries do
//! not edit documents or rebuild targets. Distance wins, then kind priority, source,
//! coordinates and anchor index, using total ordering (no non-transitive epsilon ties).
//! Point/curve hits are atomic: an axis snap cannot pull the result off the geometry.

use crate::{EngineError, Result};
use photocraft_doc::{DocId, Document, LayerContent, LayerId, Path};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const LIMIT: f64 = 1e7;
const MAX_TARGETS: usize = 100_000;
const MAX_OBJECTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub nodes: bool,
    /// Enable once V1 Geometry is installed. Missing V1 is an error, never an
    /// approximate curve hit disguised as exact geometry.
    pub curves: bool,
    pub bounds: bool,
    pub guides: bool,
    pub grid: bool,
    pub pixel: bool,
    pub artboard: bool,
    pub document: bool,
    pub alignment: bool,
    pub spacing: bool,
    pub angle: bool,
    /// Physical display pixels. Document tolerance = pixels / (logical zoom * PPP).
    pub tolerance_px: f64,
    pub grid_step: f64,
    pub grid_origin: [f64; 2],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            nodes: true,
            curves: false,
            bounds: true,
            guides: true,
            grid: false,
            pixel: false,
            artboard: true,
            document: true,
            alignment: true,
            spacing: true,
            angle: true,
            tolerance_px: 8.0,
            grid_step: 18.0,
            grid_origin: [0.0; 2],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        bounded(self.tolerance_px, 0.0, 64.0)?;
        bounded(self.grid_step, 0.001, LIMIT)?;
        valid_point(self.grid_origin)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct View {
    pub zoom: f64,
    pub pixels_per_point: f64,
}
impl View {
    pub fn tolerance(self, physical_px: f64) -> Result<f64> {
        bounded(self.zoom, 0.01, 8192.0)?;
        bounded(self.pixels_per_point, 0.25, 16.0)?;
        bounded(physical_px, 0.0, 64.0)?;
        Ok(physical_px / (self.zoom * self.pixels_per_point))
    }
}
impl Default for View {
    fn default() -> Self {
        Self { zoom: 1.0, pixels_per_point: 1.0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Exclusions {
    pub layers: Vec<LayerId>,
    pub paths: Vec<String>,
    pub work_path: bool,
}

/// The exact V1 contract, with native Point and the tuple exposed by pc-pathops.
pub type Nearest = (usize, usize, f64, Point, f64);
pub type Bounds = (f64, f64, f64, f64);
#[derive(Clone, Copy)]
pub struct Geometry {
    pub nearest: fn(&Path, Point) -> Result<Option<Nearest>>,
    pub bounds: fn(&Path) -> Result<Option<Bounds>>,
}
static GEOMETRY: OnceLock<Geometry> = OnceLock::new();
/// Installs the V1 error/type adapter once during app/engine initialization.
pub fn install_geometry(geometry: Geometry) -> Result<()> {
    GEOMETRY.set(geometry).map_err(|_| error("vector snap geometry already installed"))
}
pub fn installed_geometry() -> Option<Geometry> {
    GEOMETRY.get().copied()
}

/// Enum order is the tie priority, after distance. Guides retain Move's tie precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Guide,
    Node,
    Curve,
    Artboard,
    Bounds,
    Alignment,
    Spacing,
    Angle,
    Grid,
    Pixel,
    Document,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Document,
    Guide(usize),
    Layer(LayerId),
    Mask(LayerId),
    SavedPath(usize),
    WorkPath,
    Grid,
    Pixel,
    Construction,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub kind: Kind,
    pub source: Source,
    pub subpath: usize,
    pub index: usize,
    pub t: Option<f64>,
}
impl Hit {
    fn new(kind: Kind, source: Source, index: usize) -> Self {
        Self { kind, source, subpath: 0, index, t: None }
    }
    fn key(&self) -> (Kind, Source, usize, usize) {
        (self.kind, self.source, self.subpath, self.index)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Guide {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub hit: Hit,
    /// Document-space gap for spacing measurement; UI formats using document units.
    pub value: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SnapResult {
    pub point: [f64; 2],
    pub delta: [f64; 2],
    pub guides: Vec<Guide>,
}
impl SnapResult {
    pub fn unchanged(point: [f64; 2]) -> Self {
        Self { point, delta: [0.0; 2], guides: Vec::new() }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Angle {
    pub origin: [f64; 2],
    pub step: f64,
    #[serde(default)]
    pub offset: f64,
}

#[derive(Clone, Debug)]
struct AxisTarget {
    axis: usize,
    pos: f64,
    span: [f64; 2],
    hit: Hit,
}
#[derive(Clone, Debug)]
struct Node {
    point: [f64; 2],
    hit: Hit,
}
#[derive(Clone, Debug)]
struct Curve {
    bounds: [f64; 4],
    path: Path,
    source: Source,
}
#[derive(Clone, Copy, Debug)]
struct Object {
    rect: [f64; 4],
    source: Source,
}
#[derive(Clone, Copy, Debug)]
struct Pair {
    axis: usize,
    left: Object,
    right: Object,
}

/// Built once at press; cloned paths are detached from subsequent preview geometry.
#[derive(Clone)]
pub struct Cache {
    document: DocId,
    revision: u64,
    exclusions: Exclusions,
    axes: Vec<AxisTarget>,
    nodes: Vec<Node>,
    curves: Vec<Curve>,
    pairs: Vec<Pair>,
    extent: [f64; 4],
    geometry: Option<Geometry>,
}
impl Cache {
    pub fn build(doc: &Document, revision: u64, exclusions: Exclusions, geometry: Option<Geometry>) -> Result<Self> {
        if doc.paths.len() > MAX_OBJECTS || doc.guides.vertical.len() + doc.guides.horizontal.len() > MAX_TARGETS {
            return Err(error("snap target budget exceeded"));
        }
        for (i, id) in exclusions.layers.iter().enumerate() {
            if doc.layer(*id).is_none() || exclusions.layers[..i].contains(id) {
                return Err(error("invalid or duplicate snap exclusion"));
            }
        }
        for (i, name) in exclusions.paths.iter().enumerate() {
            if !doc.paths.iter().any(|p| &p.name == name) || exclusions.paths[..i].contains(name) {
                return Err(error("invalid or duplicate path exclusion"));
            }
        }
        let extent = [0.0, 0.0, f64::from(doc.size.width), f64::from(doc.size.height)];
        valid_rect(extent)?;
        let mut cache =
            Self { document: doc.id, revision, exclusions, axes: Vec::new(), nodes: Vec::new(), curves: Vec::new(), pairs: Vec::new(), extent, geometry };
        cache.add_rect(extent, Kind::Document, Source::Document);
        for (axis, guides) in [(0, &doc.guides.vertical), (1, &doc.guides.horizontal)] {
            for (i, pos) in guides.iter().enumerate() {
                bounded(f64::from(*pos), -LIMIT, LIMIT)?;
                cache.axes.push(AxisTarget {
                    axis,
                    pos: f64::from(*pos),
                    span: [extent[1 - axis], extent[3 - axis]],
                    hit: Hit::new(Kind::Guide, Source::Guide(i), axis),
                });
            }
        }
        let walk = doc.walk();
        if walk.len() > MAX_OBJECTS {
            return Err(error("snap object budget exceeded"));
        }
        let mut objects = Vec::new();
        for (path, _, layer) in walk {
            // Hidden or moving ancestors exclude their descendants as well.
            if (1..=path.len()).any(|n| doc.layer_at(&path[..n]).is_some_and(|l| !l.visible || cache.exclusions.layers.contains(&l.id))) {
                continue;
            }
            let source = Source::Layer(layer.id);
            if let Some(board) = layer.artboard() {
                let r = board.rect;
                let rect = [f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)];
                valid_rect(rect)?;
                cache.add_rect(rect, Kind::Artboard, source);
            } else if let LayerContent::Shape(shape) = &layer.content {
                cache.add_path(&shape.path, source)?;
                // V1 supplies analytic bounds. Until installed, preserve the existing Move
                // visual bounds (explicitly documented); do not copy V1 curve algorithms.
                let rect = match geometry {
                    Some(g) => (g.bounds)(&shape.path)?.map(|(x0, y0, x1, y1)| [x0, y0, x1, y1]),
                    None => crate::snap::layer_rect(doc, layer.id),
                };
                if let Some(rect) = rect {
                    valid_rect(rect)?;
                    cache.add_rect(rect, Kind::Bounds, source);
                    objects.push(Object { rect, source });
                }
            } else if let Some(rect) = crate::snap::layer_rect(doc, layer.id) {
                valid_rect(rect)?;
                cache.add_rect(rect, Kind::Bounds, source);
                objects.push(Object { rect, source });
            }
            if let Some(mask) = layer.vector_mask.as_ref().filter(|m| m.enabled) {
                cache.add_path(&mask.path, Source::Mask(layer.id))?;
            }
            if cache.axes.len() + cache.nodes.len() > MAX_TARGETS {
                return Err(error("snap target budget exceeded"));
            }
        }
        for (i, path) in doc.paths.iter().enumerate() {
            if !cache.exclusions.paths.contains(&path.name) {
                cache.add_path(&path.path, Source::SavedPath(i))?;
            }
        }
        if !cache.exclusions.work_path
            && let Some(path) = &doc.work_path
        {
            cache.add_path(path, Source::WorkPath)?;
        }
        if cache.axes.len() + cache.nodes.len() > MAX_TARGETS {
            return Err(error("snap target budget exceeded"));
        }
        // Adjacent objects in axis order give a bounded O(n log n) cached spacing set.
        // A third object in the gap blocks it; overlapping objects are never a spacing pair.
        for axis in 0..2 {
            objects.sort_by(|a, b| a.rect[axis].total_cmp(&b.rect[axis]).then_with(|| a.source.cmp(&b.source)));
            for pair in objects.windows(2) {
                let (left, right) = (pair[0], pair[1]);
                let other = 1 - axis;
                if left.rect[axis + 2] <= right.rect[axis] && overlaps(left.rect, right.rect, other) {
                    cache.pairs.push(Pair { axis, left, right });
                }
            }
        }
        Ok(cache)
    }
    pub fn is_current(&self, doc: DocId, revision: u64) -> bool {
        self.document == doc && self.revision == revision
    }
    pub fn document(&self) -> DocId {
        self.document
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn exclusions(&self) -> &Exclusions {
        &self.exclusions
    }
    pub fn target_count(&self) -> usize {
        self.axes.len() + self.nodes.len() + self.curves.len()
    }
    fn add_rect(&mut self, rect: [f64; 4], kind: Kind, source: Source) {
        for axis in 0..2 {
            for (i, pos) in [rect[axis], (rect[axis] + rect[axis + 2]) * 0.5, rect[axis + 2]].into_iter().enumerate() {
                let kind = if kind == Kind::Bounds && i == 1 { Kind::Alignment } else { kind };
                self.axes.push(AxisTarget { axis, pos, span: [rect[1 - axis], rect[3 - axis]], hit: Hit::new(kind, source, i) });
            }
        }
    }
    fn add_path(&mut self, path: &Path, source: Source) -> Result<()> {
        let n: usize = path.subpaths.iter().map(|s| s.knots.len()).sum();
        if n > MAX_TARGETS.saturating_sub(self.nodes.len() + self.axes.len()) {
            return Err(error("snap target budget exceeded"));
        }
        for (s, sp) in path.subpaths.iter().enumerate() {
            for (i, k) in sp.knots.iter().enumerate() {
                for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
                    valid_point([p.x, p.y])?;
                }
                self.nodes.push(Node { point: [k.anchor.x, k.anchor.y], hit: Hit { kind: Kind::Node, source, subpath: s, index: i, t: None } });
            }
        }
        if n > 0
            && let Some((x0, y0, x1, y1)) = path.control_bounds()
        {
            self.curves.push(Curve { bounds: [x0, y0, x1, y1], path: path.clone(), source });
        }
        Ok(())
    }
    /// Query one point. A bypass suppresses every family, including pixel/angle snapping.
    pub fn point(&self, point: [f64; 2], settings: &Settings, view: View, angle: Option<Angle>, bypass: bool) -> Result<SnapResult> {
        valid_point(point)?;
        settings.validate()?;
        let tol = view.tolerance(settings.tolerance_px)?;
        if let Some(angle) = angle {
            valid_point(angle.origin)?;
            bounded(angle.step, std::f64::consts::PI / 180.0, std::f64::consts::PI)?;
            bounded(angle.offset, -std::f64::consts::TAU, std::f64::consts::TAU)?;
        }
        if bypass || !settings.enabled {
            return Ok(SnapResult::unchanged(point));
        }
        let mut result = SnapResult::unchanged(point);
        for axis in 0..2 {
            if let Some(c) = self.axis_best(axis, &[point[axis]], settings, tol) {
                result.point[axis] += c.delta;
                result.guides.push(self.axis_guide(&c.target, result.point, result.point));
            }
        }
        result.delta = subtract(result.point, point);
        let axis_dist = length(result.delta);
        let ray = angle.filter(|_| settings.angle).map(|angle| {
            let d = subtract(point, angle.origin);
            let theta = ((d[1].atan2(d[0]) - angle.offset) / angle.step).round() * angle.step + angle.offset;
            let unit = [theta.cos(), theta.sin()];
            let projection = d[0] * unit[0] + d[1] * unit[1];
            let projected = [angle.origin[0] + projection * unit[0], angle.origin[1] + projection * unit[1]];
            (angle, unit, projected)
        });
        let on_ray = |p: [f64; 2]| {
            ray.is_none_or(|(angle, unit, _)| {
                let d = subtract(p, angle.origin);
                (d[0] * unit[1] - d[1] * unit[0]).abs() <= 1e-8 && d[0] * unit[0] + d[1] * unit[1] >= -1e-8
            })
        };
        if !on_ray(result.point) {
            result = SnapResult::unchanged(point);
        }
        let mut best: Option<PointCandidate> = None;
        if settings.nodes {
            for n in self.nodes.iter().filter(|n| on_ray(n.point)) {
                consider_point(&mut best, PointCandidate { point: n.point, hit: n.hit.clone() }, point, tol);
            }
        }
        if settings.curves && !self.curves.is_empty() {
            let geometry = self.geometry.ok_or_else(|| error("nearest curve snapping requires the V1 geometry adapter"))?;
            for curve in self.curves.iter().filter(|curve| {
                let [x0, y0, x1, y1] = curve.bounds;
                point[0] >= x0 - tol && point[0] <= x1 + tol && point[1] >= y0 - tol && point[1] <= y1 + tol
            }) {
                if let Some((s, i, t, p, _distance)) = (geometry.nearest)(&curve.path, Point::new(point[0], point[1]))? {
                    valid_point([p.x, p.y])?;
                    bounded(t, 0.0, 1.0)?;
                    let Some(sp) = curve.path.subpaths.get(s) else {
                        return Err(error("invalid V1 nearest subpath"));
                    };
                    let count = if sp.closed { sp.knots.len() } else { sp.knots.len().saturating_sub(1) };
                    if i >= count {
                        return Err(error("invalid V1 nearest segment"));
                    }
                    if on_ray([p.x, p.y]) {
                        consider_point(
                            &mut best,
                            PointCandidate { point: [p.x, p.y], hit: Hit { kind: Kind::Curve, source: curve.source, subpath: s, index: i, t: Some(t) } },
                            point,
                            tol,
                        );
                    }
                }
            }
        }
        // A real target on the construction ray beats a zero-distance angle guide.
        // Otherwise a pointer already on a ray would never snap to a nearby grid/node.
        if best.is_none()
            && result.guides.is_empty()
            && let Some((_, _, p)) = ray
        {
            consider_point(&mut best, PointCandidate { point: p, hit: Hit::new(Kind::Angle, Source::Construction, 0) }, point, tol);
        }
        if let Some(c) = best {
            let dist = length(subtract(c.point, point));
            let axis_priority = result.guides.iter().map(|g| g.hit.kind).min();
            if result.guides.is_empty() || dist < axis_dist || (dist == axis_dist && axis_priority.is_none_or(|k| c.hit.kind < k)) {
                result.point = c.point;
                result.delta = subtract(c.point, point);
                let a = if c.hit.kind == Kind::Angle { angle.map_or(c.point, |a| a.origin) } else { c.point };
                result.guides = vec![Guide { a, b: c.point, hit: c.hit, value: None }];
            }
        }
        // Recreate axis guides from the FINAL point; X and Y adjustments must agree visually.
        for guide in &mut result.guides {
            if guide.a[0] == guide.b[0] && guide.hit.kind != Kind::Node && guide.hit.kind != Kind::Curve && guide.hit.kind != Kind::Angle {
                guide.a[1] = guide.a[1].min(result.point[1]);
                guide.b[1] = guide.b[1].max(result.point[1]);
            } else if guide.a[1] == guide.b[1] && guide.hit.kind != Kind::Node && guide.hit.kind != Kind::Curve && guide.hit.kind != Kind::Angle {
                guide.a[0] = guide.a[0].min(result.point[0]);
                guide.b[0] = guide.b[0].max(result.point[0]);
            }
        }
        valid_point(result.point)?;
        Ok(result)
    }
    /// Translate a bounding rectangle (including a union of multiple selected objects).
    pub fn rect(&self, rect: [f64; 4], settings: &Settings, view: View, bypass: bool) -> Result<SnapResult> {
        valid_rect(rect)?;
        settings.validate()?;
        let tol = view.tolerance(settings.tolerance_px)?;
        let mut result = SnapResult::unchanged([rect[0], rect[1]]);
        if bypass || !settings.enabled {
            return Ok(result);
        }
        let mut axis_hits: Vec<(usize, AxisCandidate)> = Vec::new();
        let mut spacing_hits: Vec<SpacingCandidate> = Vec::new();
        for axis in 0..2 {
            let anchors = [rect[axis], (rect[axis] + rect[axis + 2]) * 0.5, rect[axis + 2]];
            let best = self.axis_best(axis, &anchors, settings, tol);
            let spacing = if settings.spacing { self.spacing_best(axis, rect, tol) } else { None };
            if let Some(s) = spacing.filter(|s| {
                best.as_ref().is_none_or(|b| s.delta.abs() < b.delta.abs() || (s.delta.abs() == b.delta.abs() && Kind::Spacing < b.target.hit.kind))
            }) {
                result.delta[axis] = s.delta;
                spacing_hits.push(s);
            } else if let Some(c) = best {
                result.delta[axis] = c.delta;
                axis_hits.push((axis, c));
            }
        }
        let moved = translated(rect, result.delta);
        valid_rect(moved)?;
        result.point = [moved[0], moved[1]];
        for (_, c) in axis_hits {
            result.guides.push(self.axis_guide(&c.target, [moved[0], moved[1]], [moved[2], moved[3]]));
        }
        for c in spacing_hits {
            result.guides.extend(c.guides(moved));
        }
        Ok(result)
    }
    fn axis_best(&self, axis: usize, anchors: &[f64], settings: &Settings, tol: f64) -> Option<AxisCandidate> {
        let mut best: Option<AxisCandidate> = None;
        for (i, v) in anchors.iter().enumerate() {
            let mut consider = |target: AxisTarget| {
                let candidate = AxisCandidate { delta: target.pos - v, anchor: i, target };
                if candidate.delta.abs() <= tol && best.as_ref().is_none_or(|b| candidate.cmp(b).is_lt()) {
                    best = Some(candidate);
                }
            };
            for target in self.axes.iter().filter(|t| t.axis == axis && enabled(t.hit.kind, settings)) {
                consider(target.clone());
            }
            // Rectangle centres still align to objects and guides; grid and pixel targets use edges.
            if anchors.len() == 3 && i == 1 {
                continue;
            }
            for (on, step, origin, kind, source) in [
                (settings.grid, settings.grid_step, settings.grid_origin[axis], Kind::Grid, Source::Grid),
                (settings.pixel, 1.0, 0.0, Kind::Pixel, Source::Pixel),
            ] {
                if on {
                    let pos = ((v - origin) / step).round() * step + origin;
                    consider(AxisTarget { axis, pos, span: [self.extent[1 - axis], self.extent[3 - axis]], hit: Hit::new(kind, source, axis) });
                }
            }
        }
        best
    }
    fn axis_guide(&self, t: &AxisTarget, a: [f64; 2], b: [f64; 2]) -> Guide {
        let other = 1 - t.axis;
        let (lo, hi) = (t.span[0].min(a[other]), t.span[1].max(b[other]));
        let mut p = [t.pos, lo];
        let mut q = [t.pos, hi];
        if t.axis == 1 {
            p.swap(0, 1);
            q.swap(0, 1);
        }
        Guide { a: p, b: q, hit: t.hit.clone(), value: None }
    }
    fn spacing_best(&self, axis: usize, rect: [f64; 4], tol: f64) -> Option<SpacingCandidate> {
        let mut best: Option<SpacingCandidate> = None;
        let width = rect[axis + 2] - rect[axis];
        for pair in self.pairs.iter().filter(|p| p.axis == axis && overlaps(rect, p.left.rect, 1 - axis) && overlaps(rect, p.right.rect, 1 - axis)) {
            let gap = pair.right.rect[axis] - pair.left.rect[axis + 2];
            let positions =
                [pair.left.rect[axis] - gap - width, (pair.left.rect[axis + 2] + pair.right.rect[axis] - width) * 0.5, pair.right.rect[axis + 2] + gap];
            for (mode, pos) in positions.into_iter().enumerate() {
                if mode == 1 && gap < width {
                    continue;
                }
                let c = SpacingCandidate { pair: *pair, mode, delta: pos - rect[axis] };
                if c.delta.abs() <= tol && best.as_ref().is_none_or(|b| c.cmp(b).is_lt()) {
                    best = Some(c);
                }
            }
        }
        best
    }
}
#[derive(Clone)]
struct PointCandidate {
    point: [f64; 2],
    hit: Hit,
}
fn consider_point(best: &mut Option<PointCandidate>, c: PointCandidate, point: [f64; 2], tol: f64) {
    let dist = length(subtract(c.point, point));
    if dist <= tol
        && best.as_ref().is_none_or(|b| {
            dist.total_cmp(&length(subtract(b.point, point)))
                .then_with(|| c.hit.key().cmp(&b.hit.key()))
                .then_with(|| c.point[0].total_cmp(&b.point[0]))
                .then_with(|| c.point[1].total_cmp(&b.point[1]))
                .is_lt()
        })
    {
        *best = Some(c);
    }
}
struct AxisCandidate {
    delta: f64,
    anchor: usize,
    target: AxisTarget,
}
impl AxisCandidate {
    fn cmp(&self, b: &Self) -> std::cmp::Ordering {
        self.delta
            .abs()
            .total_cmp(&b.delta.abs())
            .then_with(|| self.target.hit.kind.cmp(&b.target.hit.kind))
            .then_with(|| self.target.pos.total_cmp(&b.target.pos))
            .then_with(|| self.target.hit.key().cmp(&b.target.hit.key()))
            .then_with(|| self.anchor.cmp(&b.anchor))
    }
}
struct SpacingCandidate {
    pair: Pair,
    mode: usize,
    delta: f64,
}
impl SpacingCandidate {
    fn cmp(&self, b: &Self) -> std::cmp::Ordering {
        self.delta
            .abs()
            .total_cmp(&b.delta.abs())
            .then_with(|| self.pair.left.source.cmp(&b.pair.left.source))
            .then_with(|| self.pair.right.source.cmp(&b.pair.right.source))
            .then_with(|| self.mode.cmp(&b.mode))
    }
    fn guides(&self, moved: [f64; 4]) -> Vec<Guide> {
        let axis = self.pair.axis;
        let mut objects = [self.pair.left.rect, self.pair.right.rect, moved];
        objects.sort_by(|a, b| a[axis].total_cmp(&b[axis]));
        let other = 1 - axis;
        let y = (moved[other] + moved[other + 2]) * 0.5;
        objects
            .windows(2)
            .map(|pair| {
                let (lo, hi) = (pair[0][axis + 2], pair[1][axis]);
                let mut a = [lo, y];
                let mut b = [hi, y];
                if axis == 1 {
                    a.swap(0, 1);
                    b.swap(0, 1);
                }
                Guide { a, b, hit: Hit::new(Kind::Spacing, self.pair.left.source, self.mode), value: Some(hi - lo) }
            })
            .collect()
    }
}
fn enabled(kind: Kind, s: &Settings) -> bool {
    match kind {
        Kind::Guide => s.guides,
        Kind::Bounds => s.bounds,
        Kind::Alignment => s.alignment,
        Kind::Artboard => s.artboard,
        Kind::Document => s.document,
        _ => false,
    }
}
fn overlaps(a: [f64; 4], b: [f64; 4], axis: usize) -> bool {
    a[axis] < b[axis + 2] && b[axis] < a[axis + 2]
}
pub fn translated(r: [f64; 4], d: [f64; 2]) -> [f64; 4] {
    [r[0] + d[0], r[1] + d[1], r[2] + d[0], r[3] + d[1]]
}
fn subtract(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn length(p: [f64; 2]) -> f64 {
    p[0].hypot(p[1])
}
fn bounded(v: f64, min: f64, max: f64) -> Result<()> {
    if !v.is_finite() || !(min..=max).contains(&v) { Err(error("snap numbers must be finite and bounded")) } else { Ok(()) }
}
fn valid_point(p: [f64; 2]) -> Result<()> {
    for v in p {
        bounded(v, -LIMIT, LIMIT)?;
    }
    Ok(())
}
fn valid_rect(r: [f64; 4]) -> Result<()> {
    valid_point([r[0], r[1]])?;
    valid_point([r[2], r[3]])?;
    if r[0] > r[2] || r[1] > r[3] { Err(error("snap rectangle is inverted")) } else { Ok(()) }
}
fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}

#[cfg(test)]
#[path = "vector_packages/snapping/tests.rs"]
mod tests;
