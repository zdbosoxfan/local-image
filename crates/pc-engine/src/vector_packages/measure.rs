//! Design measurement queries. Distance/angle conventions adapted from VectorCraft
//! crates/tools/src/xform/measure.rs @ d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! See licenses/vectorcraft-LICENSE-{MIT,APACHE} and vectorcraft-NOTICE.
//! All geometry is detached; queries never render, edit, journal or persist readings.
use super::{
    corners, layout,
    snapping::query::{Cache, Exclusions, Geometry, Settings, SnapResult, View},
};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::{drawing::ShapeParameters, structure::DesignUnits};
use photocraft_doc::{DocId, Document, LayerContent, LayerId, LiveShape, Path, Subpath};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

const LIMIT: f64 = 1e7;
const MAX_OBJECTS: usize = 4096;
const MAX_KNOTS: usize = 100_000;
fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
fn point(p: [f64; 2]) -> Result<()> {
    if p.iter().all(|v| v.is_finite() && v.abs() <= LIMIT) { Ok(()) } else { Err(error("measurement coordinates must be finite and bounded")) }
}
/// Optional persistence contract for 00. A 1:100 drawing has factor=100.
/// Absence in old documents means 1:1; readings themselves must never be serialized.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingScale {
    pub factor: f64,
}
impl Default for DrawingScale {
    fn default() -> Self {
        Self { factor: 1. }
    }
}
impl DrawingScale {
    pub fn validate(self) -> Result<()> {
        if self.factor.is_finite() && (1e-6..=1e6).contains(&self.factor) { Ok(()) } else { Err(error("drawing scale must be between 0.000001 and 1000000")) }
    }
    pub fn assign(measured_pixels: f64, real_length: f64, units: DesignUnits, dpi: f64) -> Result<Self> {
        if !measured_pixels.is_finite() || !(1e-9..=LIMIT * 3.).contains(&measured_pixels) || !real_length.is_finite() || !(1e-9..=LIMIT).contains(&real_length)
        {
            return Err(error("assign scale requires a positive finite measurement and real length"));
        }
        let scale = Self { factor: layout::to_pixels(real_length, units, dpi)? / measured_pixels };
        scale.validate()?;
        Ok(scale)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    pub unit: DesignUnits,
    pub dpi: f64,
    pub scale: DrawingScale,
}
impl Units {
    pub fn document(doc: &Document, scale: DrawingScale) -> Self {
        Self { unit: doc.design_units, dpi: f64::from(doc.resolution_dpi), scale }
    }
    pub fn length(self, pixels: f64) -> Result<f64> {
        self.scale.validate()?;
        let result = layout::from_pixels(pixels, self.unit, self.dpi)? * self.scale.factor;
        if result.is_finite() { Ok(result) } else { Err(error("measurement overflow")) }
    }
    pub fn area(self, pixels: f64) -> Result<f64> {
        let factor = self.length(1.)?;
        let value = pixels * factor * factor;
        if value.is_finite() { Ok(value) } else { Err(error("area overflow")) }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn new(doc: &Document, revision: u64, layers: Vec<LayerId>) -> Result<Self> {
        let capture = Self { document: doc.id, revision, layers };
        capture.check(doc, revision)?;
        Ok(capture)
    }
    /// Read-only measurements permit locked targets. Scale edits additionally check locks.
    pub fn check(&self, doc: &Document, revision: u64) -> Result<()> {
        if self.document != doc.id || self.revision != revision {
            return Err(error("stale measurement"));
        }
        let mut ids = HashSet::new();
        if self.layers.len() > MAX_OBJECTS || self.layers.iter().any(|id| !ids.insert(*id) || doc.layer(*id).is_none()) {
            return Err(error("invalid measurement targets"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Distance {
    pub distance: f64,
    pub angle: f64,
    pub dx: f64,
    pub dy: f64,
}
/// Angle is counterclockwise in degrees, while document coordinates have y down.
pub fn distance(a: [f64; 2], b: [f64; 2], units: Units) -> Result<Distance> {
    point(a)?;
    point(b)?;
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    Ok(Distance {
        distance: units.length(dx.hypot(dy))?,
        angle: if dx == 0. && dy == 0. { 0. } else { (-dy).atan2(dx).to_degrees() },
        dx: units.length(dx)?,
        dy: units.length(dy)?,
    })
}
pub fn geometry() -> Geometry {
    Geometry {
        nearest: |p, q| photocraft_pathops::nearest(p, q).map_err(|e| error(e.to_string())),
        bounds: |p| photocraft_pathops::bounds(p).map_err(|e| error(e.to_string())),
    }
}
#[derive(Clone)]
pub struct Gesture {
    pub capture: Capture,
    pub start: [f64; 2],
    cache: Cache,
}
impl Gesture {
    pub fn begin(doc: &Document, revision: u64, raw: [f64; 2], settings: &Settings, view: View, bypass: bool) -> Result<Self> {
        let cache = Cache::build(doc, revision, Exclusions::default(), Some(geometry()))?;
        let snap = cache.point(raw, settings, view, None, bypass)?;
        // Capture every shape/path owner read by the detached snap query, including guides
        // through document/revision. No locks are required for a read-only gesture.
        let ids = doc.walk().iter().map(|(_, _, l)| l.id).collect();
        Ok(Self { capture: Capture::new(doc, revision, ids)?, start: snap.point, cache })
    }
    pub fn endpoint(&self, raw: [f64; 2], settings: &Settings, view: View, shift: bool, bypass: bool) -> Result<SnapResult> {
        let snap = self.cache.point(raw, settings, view, None, bypass)?;
        if !shift {
            return Ok(snap);
        }
        // Shift is a hard constraint, independent of enabled snapping and Ctrl bypass.
        // Keep an exact snapped node only when it lies on the selected 45-degree ray.
        let d = [raw[0] - self.start[0], raw[1] - self.start[1]];
        let step = std::f64::consts::FRAC_PI_4;
        let theta = (d[1].atan2(d[0]) / step).round() * step;
        let unit = [theta.cos(), theta.sin()];
        let s = [snap.point[0] - self.start[0], snap.point[1] - self.start[1]];
        if (s[0] * unit[1] - s[1] * unit[0]).abs() <= 1e-10 * s[0].hypot(s[1]).max(1.) {
            return Ok(snap);
        }
        // VC Measure rotates the raw vector onto the constrained ray without
        // changing its length (geom::constrain_angle), unlike 07's angle-guide projection.
        let length = d[0].hypot(d[1]);
        let p = [self.start[0] + length * unit[0], self.start[1] + length * unit[1]];
        point(p)?;
        Ok(SnapResult { point: p, delta: [p[0] - raw[0], p[1] - raw[1]], guides: Vec::new() })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerimeterMode {
    #[default]
    FullEdges,
    CuspedSegments,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub area: f64,
    pub including_holes: f64,
    pub perimeter: f64,
    pub segment: Option<f64>,
}
fn path_valid(path: &Path) -> Result<()> {
    if path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_KNOTS {
        return Err(error("measurement knot budget exceeded"));
    }
    for k in path.subpaths.iter().flat_map(|s| &s.knots) {
        for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
            point([p.x, p.y])?;
        }
    }
    Ok(())
}
fn length(path: &Path) -> f64 {
    photocraft_pathops::to_geometry(path).length()
}
/// One segment, or the contiguous run between cusp knots (smooth knots do not split it).
pub fn segment_length(path: &Path, subpath: usize, segment: usize, mode: PerimeterMode) -> Result<f64> {
    path_valid(path)?;
    if mode == PerimeterMode::FullEdges {
        return Ok(length(path));
    }
    let sp = path.subpaths.get(subpath).ok_or_else(|| error("missing hovered subpath"))?;
    let count = if sp.knots.len() < 2 {
        0
    } else if sp.closed {
        sp.knots.len()
    } else {
        sp.knots.len() - 1
    };
    if segment >= count {
        return Err(error("missing hovered segment"));
    }
    let mut first = segment;
    for _ in 1..count {
        if !sp.knots[first].smooth || (!sp.closed && first == 0) {
            break;
        }
        first = (first + count - 1) % count;
    }
    let mut result = 0.;
    for offset in 0..count {
        let i = (first + offset) % count;
        let a = sp.knots[i];
        let b = sp.knots[(i + 1) % sp.knots.len()];
        let part = Subpath { closed: false, knots: vec![a, b], op: Default::default() };
        result += length(&Path::new(vec![part]));
        if !b.smooth || (!sp.closed && i + 1 == count) {
            break;
        }
    }
    Ok(result)
}
/// Hole-aware fill area uses V1's normalized arrangement. Perimeter measures the drawn
/// curves (open curves are not artificially closed). Closed boolean components use their
/// resulting boundaries, so subtraction includes the inner perimeter.
pub fn path_area(path: &Path) -> Result<Area> {
    path_valid(path)?;
    let normalized = photocraft_pathops::finish_compound(path, None).map_err(|e| error(e.to_string()))?;
    let contours = photocraft_pathops::to_geometry(&normalized);
    let signed: Vec<_> = contours.subpaths.iter().map(|s| s.signed_area()).collect();
    let outer_sign = signed.iter().max_by(|a, b| a.abs().total_cmp(&b.abs())).copied().unwrap_or(0.).signum();
    let including_holes = signed.iter().filter(|a| a.signum() == outer_sign).map(|a| a.abs()).sum();
    let area = photocraft_pathops::area(path).map_err(|e| error(e.to_string()))?;
    // Fill implicitly closes open contours with a line, per V1. Length measures
    // their actual curves, without adding that invisible closing edge.
    let perimeter = if path.subpaths.iter().all(|s| s.closed) { contours.length() } else { length(path) };
    Ok(Area { area, including_holes, perimeter, segment: None })
}
#[derive(Clone)]
struct Object {
    id: LayerId,
    path: Path,
    area: Area,
    circle: Option<f64>,
    visible: bool,
    bounds: Option<(f64, f64, f64, f64)>,
}
/// Once per revision, not per pointer frame. No raster cache preparation or layer writes.
#[derive(Clone)]
pub struct AreaCache {
    pub capture: Capture,
    objects: Vec<Object>,
}
impl AreaCache {
    pub fn build(doc: &Document, revision: u64) -> Result<Self> {
        let walk = doc.walk();
        if walk.len() > MAX_OBJECTS {
            return Err(error("measurement object budget exceeded"));
        }
        let mut objects = Vec::new();
        let mut total_knots = 0;
        for (indices, _, layer) in &walk {
            let LayerContent::Shape(shape) = &layer.content else { continue };
            let projected = corners::project_shape(shape, &layer.vector.corners)?;
            let path = projected.path;
            total_knots += path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            if total_knots > MAX_KNOTS {
                return Err(error("measurement scene knot budget exceeded"));
            }
            let mut area = path_area(&path)?;
            let circle = if layer.vector.corners.is_empty() {
                let rect = match (&layer.vector.live_shape, &shape.live) {
                    (Some(d), _) if matches!(d.parameters, ShapeParameters::Ellipse) && d.angle.is_finite() => {
                        let generated = super::drawing::geometry::generate(d).map_err(|e| error(e.to_string()))?;
                        (generated == shape.path).then_some(d.rect)
                    }
                    (None, Some(live @ LiveShape::Ellipse { rect })) if photocraft_vector::shapes::live_path(live) == shape.path => Some(*rect),
                    _ => None,
                };
                rect.filter(|r| r[2] == r[3] && r[2] > 0. && r.iter().all(|v| v.is_finite())).map(|r| r[2] / 2.)
            } else {
                None
            };
            if let Some(r) = circle {
                area.area = std::f64::consts::PI * r * r;
                area.including_holes = area.area;
                area.perimeter = std::f64::consts::TAU * r;
            }
            let bounds = photocraft_pathops::bounds(&path).map_err(|e| error(e.to_string()))?;
            let visible = (1..=indices.len()).all(|n| doc.layer_at(&indices[..n]).is_some_and(|l| l.visible));
            objects.push(Object { id: layer.id, path, area, circle, visible, bounds });
        }
        Ok(Self { capture: Capture::new(doc, revision, objects.iter().map(|o| o.id).collect())?, objects })
    }
    pub fn current(&self, doc: &Document, revision: u64) -> bool {
        self.capture.document == doc.id && self.capture.revision == revision
    }
    pub fn hover(&self, p: [f64; 2], tolerance: f64) -> Result<Option<LayerId>> {
        point(p)?;
        if !tolerance.is_finite() || !(0.0..=6400.).contains(&tolerance) {
            return Err(error("invalid area hit tolerance"));
        }
        for o in self.objects.iter().rev().filter(|o| o.visible) {
            if !o.bounds.is_some_and(|(x0, y0, x1, y1)| p[0] >= x0 - tolerance && p[0] <= x1 + tolerance && p[1] >= y0 - tolerance && p[1] <= y1 + tolerance) {
                continue;
            }
            let q = Point::new(p[0], p[1]);
            if photocraft_pathops::contains(&o.path, q).map_err(|e| error(e.to_string()))?
                || photocraft_pathops::nearest(&o.path, q).map_err(|e| error(e.to_string()))?.is_some_and(|n| n.4 <= tolerance)
            {
                return Ok(Some(o.id));
            }
        }
        Ok(None)
    }
    pub fn read(
        &self,
        doc: &Document,
        revision: u64,
        ids: &[LayerId],
        hovered: Option<(LayerId, [f64; 2])>,
        mode: PerimeterMode,
        units: Units,
    ) -> Result<Area> {
        if !self.current(doc, revision) {
            return Err(error("stale area measurement"));
        }
        Capture::new(doc, revision, ids.to_vec())?;
        let mut result = Area::default();
        let mut leaves = HashSet::new();
        for id in ids {
            let layer = doc.layer(*id).ok_or_else(|| error("missing area object"))?;
            fn collect(l: &photocraft_doc::Layer, ids: &mut HashSet<LayerId>) -> Result<()> {
                if l.vector.live.is_some() {
                    return Err(error("live groups require the shared measurement projection"));
                }
                match &l.content {
                    LayerContent::Shape(_) => {
                        ids.insert(l.id);
                    }
                    LayerContent::Group(g) => {
                        for l in &g.children {
                            collect(l, ids)?;
                        }
                    }
                    _ => return Err(error("area target requires vector geometry")),
                }
                Ok(())
            }
            collect(layer, &mut leaves)?;
        }
        for o in &self.objects {
            if leaves.remove(&o.id) {
                result.area += o.area.area;
                result.including_holes += o.area.including_holes;
                result.perimeter += o.area.perimeter;
            }
        }
        if !leaves.is_empty() {
            return Err(error("missing projected area target"));
        }
        if let Some((id, p)) = hovered {
            point(p)?;
            if doc.layer(id).is_none() {
                return Err(error("missing hovered target"));
            }
            let o = self.objects.iter().find(|o| o.id == id).ok_or_else(|| error("invalid hovered object"))?;
            if let Some((s, i, _, _, _)) = photocraft_pathops::nearest(&o.path, Point::new(p[0], p[1])).map_err(|e| error(e.to_string()))? {
                result.segment =
                    Some(if mode == PerimeterMode::FullEdges || o.circle.is_some() { o.area.perimeter } else { segment_length(&o.path, s, i, mode)? });
            }
        }
        Ok(Area {
            area: units.area(result.area)?,
            including_holes: units.area(result.including_holes)?,
            perimeter: units.length(result.perimeter)?,
            segment: result.segment.map(|v| units.length(v)).transpose()?,
        })
    }
}
/// Prepared metadata-only edit for 00's document/artboard persistence hook. No caches
/// change. The setter must only update the requested scale field; Session::edit is atomic.
#[derive(Clone, Debug)]
pub struct ScaleEdit {
    pub capture: Capture,
    pub artboard: Option<LayerId>,
    pub scale: DrawingScale,
}
impl ScaleEdit {
    pub fn commit(&self, s: &mut Session, set: impl FnOnce(&mut Document, Option<LayerId>, DrawingScale) -> Result<()>) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        self.capture.check(&st.doc, st.revision)?;
        self.scale.validate()?;
        if let Some(id) = self.artboard
            && (!self.capture.layers.contains(&id)
                || st.doc.layer(id).and_then(|l| l.artboard()).is_none()
                || st.doc.effective_locks(id).all
                || st.doc.effective_locks(id).artboard)
        {
            return Err(error("invalid or locked drawing-scale artboard"));
        }
        s.edit("Drawing Scale", |doc, _| set(doc, self.artboard, self.scale))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Query {
    capture: Capture,
    #[serde(default)]
    a: Option<[f64; 2]>,
    #[serde(default)]
    b: Option<[f64; 2]>,
    #[serde(default)]
    units: Option<DesignUnits>,
    #[serde(default)]
    scale: DrawingScale,
    #[serde(default)]
    hovered: Option<(LayerId, [f64; 2])>,
    #[serde(default)]
    perimeter_mode: PerimeterMode,
}
fn run_distance(s: &mut Session, p: &Value) -> Result<Value> {
    let q: Query = serde_json::from_value(p.clone()).map_err(|e| error(e.to_string()))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    q.capture.check(&st.doc, st.revision)?;
    let mut units = Units::document(&st.doc, q.scale);
    units.unit = q.units.unwrap_or(units.unit);
    Ok(json!(distance(q.a.ok_or_else(|| error("a required"))?, q.b.ok_or_else(|| error("b required"))?, units)?))
}
fn run_area(s: &mut Session, p: &Value) -> Result<Value> {
    let q: Query = serde_json::from_value(p.clone()).map_err(|e| error(e.to_string()))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    q.capture.check(&st.doc, st.revision)?;
    let mut units = Units::document(&st.doc, q.scale);
    units.unit = q.units.unwrap_or(units.unit);
    Ok(json!(AreaCache::build(&st.doc, st.revision)?.read(&st.doc, st.revision, &q.capture.layers, q.hovered, q.perimeter_mode, units)?))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("vector.measure.distance", "Measure Distance", run_distance as fn(&mut Session, &Value) -> Result<Value>),
        ("vector.measure.area", "Measure Area", run_area),
    ]
    .into_iter()
    .map(|(id, label, run)| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params: "{capture:{document,revision,layers},a?,b?,units?,scale?:{factor},hovered?:[layer,[x,y]],perimeterMode?:FullEdges|CuspedSegments}",
        enabled: crate::vector_cmds::has_doc,
        journal: false,
        run,
    })
    .collect()
}
#[cfg(test)]
#[path = "measure/tests.rs"]
mod tests;
