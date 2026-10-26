//! Constructive tool transactions, using V1's fallible planar/edit/offset APIs.
//! Face selection adapted from VectorCraft crates/tools/src/builder.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092. Copyright (c) 2026 ArtCraft Team
//! and the VectorCraft contributors. MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! Previews are detached. All geometry, paint and bounded caches succeed before one edit.

use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, Path, ShapeLayer, Subpath};
use photocraft_geom::Point;
use photocraft_pathops::{
    self as ops,
    geom::{ParamCurve, Shape as _},
};
use photocraft_vector::appearance::{CompiledAppearance, NoProjection, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};

pub const MAX_TARGETS: usize = 128;
pub const MAX_POINTS: usize = 4096;
pub const MAX_COORD: f64 = 1_000_000.0;
const MAX_KNOTS: usize = 8192;
const MAX_OUTPUTS: usize = 4096;
const MAX_CACHE_BYTES: u128 = 512 << 20;

fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn geometry(e: ops::PathOpsError) -> EngineError {
    error(e.to_string())
}
fn bounded(v: f64, lo: f64, hi: f64) -> Result<()> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(error("constructive option must be finite and within its bounds")) }
}
fn validate_path(path: &Path) -> Result<()> {
    vector_api::validate_path(path)?;
    let count: usize = path.subpaths.iter().map(|s| s.knots.len()).sum();
    if count > MAX_KNOTS
        || path.subpaths.len() > MAX_KNOTS
        || path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| p.x.abs() > MAX_COORD || p.y.abs() > MAX_COORD)
    {
        return Err(error("constructive path exceeds the geometry budget"));
    }
    Ok(())
}
fn points(raw: &[[f64; 2]], min: usize) -> Result<Vec<Point>> {
    if raw.len() < min || raw.len() > MAX_POINTS {
        return Err(error("constructive stroke has an invalid point count"));
    }
    raw.iter()
        .map(|p| {
            bounded(p[0], -MAX_COORD, MAX_COORD)?;
            bounded(p[1], -MAX_COORD, MAX_COORD)?;
            Ok(Point::new(p[0], p[1]))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    #[default]
    Round,
    Miter,
    Bevel,
}
impl JoinKind {
    fn geometry(self) -> ops::Join {
        match self {
            Self::Round => ops::Join::Round,
            Self::Miter => ops::Join::Miter,
            Self::Bevel => ops::Join::Bevel,
        }
    }
}
/// Operations carry parameters, never caller-supplied output paths or caches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Builder { faces: Vec<usize>, remove: bool, style: LayerId },
    Knife { points: Vec<[f64; 2]> },
    LineCut { from: [f64; 2], to: [f64; 2] },
    Scissors { layer: LayerId, subpath: usize, segment: usize, t: f64 },
    Contour { delta: f64, join: JoinKind, miter: f64 },
    Smooth { points: Vec<[f64; 2]>, radius: f64, amount: f64 },
    Erase { points: Vec<[f64; 2]>, width: f64 },
    Join { points: Vec<[f64; 2]>, tolerance: f64 },
}
impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Builder { remove: true, .. } => "Remove Faces",
            Self::Builder { .. } => "Merge Faces",
            Self::Knife { .. } => "Knife",
            Self::LineCut { .. } => "Line Cut",
            Self::Scissors { .. } => "Scissors",
            Self::Contour { .. } => "Contour",
            Self::Smooth { .. } => "Smooth Paths",
            Self::Erase { .. } => "Path Eraser",
            Self::Join { .. } => "Join Paths",
        }
    }
    fn command(&self) -> &'static str {
        match self {
            Self::Builder { .. } => "path.builder.commit",
            Self::Knife { .. } | Self::LineCut { .. } | Self::Scissors { .. } => "path.cut.commit",
            Self::Erase { .. } => "path.erase.commit",
            _ => "path.constructive.commit",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
    pub action: Action,
}

#[derive(Clone, Debug)]
struct FaceMap {
    arrangement: ops::BuilderArrangement,
    hits: Vec<(ops::geom::BezPath, ops::geom::Rect)>,
}

#[derive(Clone, Debug)]
pub struct Gesture {
    pub document: u64,
    pub revision: u64,
    base: Arc<Document>,
    inputs: Vec<Layer>,
    builder: Arc<OnceLock<std::result::Result<FaceMap, String>>>,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if ids.len() > MAX_TARGETS {
            return Err(error("constructive selection exceeds the target budget"));
        }
        vector_api::snapshot_shapes(&st.doc, ids)?;
        let mut ordered = ids
            .iter()
            .map(|id| Ok((st.doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?, st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone())))
            .collect::<Result<Vec<_>>>()?;
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        let mut inputs: Vec<Layer> = ordered.into_iter().map(|(_, l)| l).collect();
        let mut knots = 0;
        for layer in &mut inputs {
            let LayerContent::Shape(shape) = &mut layer.content else {
                return Err(error("shape target required"));
            };
            validate_path(&shape.path)?;
            if !layer.vector.corners.is_empty() {
                *shape = super::corners::project_shape(shape, &layer.vector.corners)?;
            }
            validate_path(&shape.path)?;
            knots += shape.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
        }
        if knots > MAX_KNOTS {
            return Err(error("constructive selection exceeds the knot budget"));
        }
        Ok(Self { document: st.doc.id.0, revision: st.revision, base: st.doc.clone(), inputs, builder: Default::default() })
    }
    pub fn is_current(&self, s: &Session) -> bool {
        s.active().is_some_and(|st| st.doc.id.0 == self.document && st.revision == self.revision)
    }
    pub fn ids(&self) -> Vec<LayerId> {
        self.inputs.iter().map(|l| l.id).collect()
    }
    fn shape(&self, index: usize) -> Result<&ShapeLayer> {
        match self.inputs.get(index).map(|l| &l.content) {
            Some(LayerContent::Shape(sh)) => Ok(sh),
            _ => Err(error("invalid constructive source")),
        }
    }
    fn filled_path(&self, path: &Path) -> Result<Path> {
        let b = self.base.bounds();
        ops::finish_compound(path, Some(ops::geom::Rect::new(b.x0 as f64, b.y0 as f64, b.x1 as f64, b.y1 as f64))).map_err(geometry)
    }
    fn builder_shapes(&self) -> Result<Vec<ops::Shape>> {
        self.inputs
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let sh = self.shape(i)?;
                let mut path = sh.path.clone();
                let filled = l.vector.appearance.as_ref().map_or(sh.fill.is_some(), |a| {
                    a.items.iter().any(|p| matches!(p, photocraft_doc::vector_design::appearance::AppearanceItem::Fill(f) if f.visible && f.opacity > 0.0))
                });
                if filled {
                    for sp in path.subpaths.iter_mut().filter(|sp| !sp.closed) {
                        if ops::kernel::encloses_area(&ops::to_geometry(&Path::new(vec![sp.clone()])).subpaths[0]) {
                            sp.closed = true;
                            if let Some(k) = sp.knots.first_mut() {
                                k.in_ctrl = k.anchor;
                            }
                            if let Some(k) = sp.knots.last_mut() {
                                k.out_ctrl = k.anchor;
                            }
                        }
                    }
                }
                if path.inverted {
                    path = self.filled_path(&path)?;
                }
                Ok(ops::Shape::new(path, i as u64))
            })
            .collect()
    }
    fn face_map(&self) -> Result<&FaceMap> {
        self.builder
            .get_or_init(|| {
                let arrangement = self.builder_shapes().and_then(|s| ops::shape_builder(&s, false).map_err(geometry)).map_err(|e| e.to_string())?;
                let hits = arrangement
                    .regions
                    .iter()
                    .map(|r| {
                        let bez = ops::to_geometry(&r.path).to_bezpath();
                        let bounds = bez.bounding_box();
                        (bez, bounds)
                    })
                    .collect();
                Ok(FaceMap { arrangement, hits })
            })
            .as_ref()
            .map_err(|e| error(e.clone()))
    }
    pub fn arrangement(&self) -> Result<&ops::BuilderArrangement> {
        Ok(&self.face_map()?.arrangement)
    }
    pub fn face_at(&self, p: Point) -> Result<Option<usize>> {
        bounded(p.x, -MAX_COORD, MAX_COORD)?;
        bounded(p.y, -MAX_COORD, MAX_COORD)?;
        let q = ops::geom::Point::new(p.x, p.y);
        Ok(self.face_map()?.hits.iter().position(|(bez, bounds)| bounds.contains(q) && bez.winding(q) != 0))
    }
    /// Faces crossed by a pointer segment, in encounter order, including its endpoints.
    pub fn faces_along(&self, a: Point, b: Point) -> Result<Vec<usize>> {
        for p in [a, b] {
            bounded(p.x, -MAX_COORD, MAX_COORD)?;
            bounded(p.y, -MAX_COORD, MAX_COORD)?;
        }
        let start = ops::geom::Point::new(a.x, a.y);
        let end = ops::geom::Point::new(b.x, b.y);
        let bounds = ops::geom::Rect::from_points(start, end);
        let len = start.distance(end);
        if len == 0.0 {
            return Ok(self.face_at(a)?.into_iter().collect());
        }
        let mut crossings: Vec<f64> = Vec::new();
        for (path, hit_bounds) in &self.face_map()?.hits {
            if hit_bounds.x0 > bounds.x1 || hit_bounds.x1 < bounds.x0 || hit_bounds.y0 > bounds.y1 || hit_bounds.y1 < bounds.y0 {
                continue;
            }
            for segment in path.segments() {
                crossings.extend(segment.intersect_line((start, end).into()).iter().map(|hit| hit.line_t));
            }
        }
        crossings.retain(|t| t.is_finite() && *t > 0.0 && *t < 1.0);
        crossings.sort_by(f64::total_cmp);
        let mut probes = vec![a];
        let mut from = 0.0;
        for to in crossings.into_iter().chain([1.0]) {
            // Coincident crossings are one place; every intervening stretch gets a probe.
            if (to - from) * len > 1e-9 {
                let t = (from + to) * 0.5;
                probes.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
                from = to;
            }
        }
        probes.push(b);
        let mut faces = Vec::new();
        let mut seen = BTreeSet::new();
        for probe in probes {
            if let Some(face) = self.face_at(probe)?
                && seen.insert(face)
            {
                faces.push(face);
            }
        }
        Ok(faces)
    }
    pub fn face_style(&self, face: usize) -> Result<LayerId> {
        let i = self.arrangement()?.regions.get(face).and_then(ops::Region::top).unwrap_or(0);
        self.inputs.get(i).map(|l| l.id).ok_or_else(|| error("face has no selected style source"))
    }
    pub fn nearest(&self, p: Point, tolerance: f64) -> Result<Option<(LayerId, usize, usize, f64, Point)>> {
        bounded(tolerance, 0.0, 1000.0)?;
        bounded(p.x, -MAX_COORD, MAX_COORD)?;
        bounded(p.y, -MAX_COORD, MAX_COORD)?;
        let mut best = None;
        let mut distance = tolerance;
        for (i, l) in self.inputs.iter().enumerate().rev() {
            if let Some((sp, seg, t, q, d)) = ops::nearest(&self.shape(i)?.path, p).map_err(geometry)?
                && (d < distance || (best.is_none() && d <= distance))
            {
                distance = d;
                best = Some((l.id, sp, seg, t, q));
            }
        }
        Ok(best)
    }
    /// Outward normal of the picked curve. Open paths use their left-hand normal.
    pub fn contour_normal(&self, p: Point, tolerance: f64) -> Result<Option<Point>> {
        let Some((id, sp, seg, t, q)) = self.nearest(p, tolerance)? else {
            return Ok(None);
        };
        let index = self.inputs.iter().position(|l| l.id == id).ok_or(EngineError::NoLayer(id))?;
        let path = &self.shape(index)?.path;
        let raw = ops::to_geometry(path);
        let c = raw.subpaths[sp].segment(seg);
        let a = c.eval((t - 1e-4).max(0.0));
        let b = c.eval((t + 1e-4).min(1.0));
        let d = b - a;
        let len = d.hypot();
        if len <= 1e-12 {
            return Ok(None);
        }
        let mut normal = Point::new(-d.y / len, d.x / len);
        if path.subpaths[sp].closed && ops::contains(path, Point::new(q.x + normal.x * 0.1, q.y + normal.y * 0.1)).map_err(geometry)? {
            normal = Point::new(-normal.x, -normal.y);
        }
        Ok(Some(normal))
    }
    pub fn request(&self, action: Action) -> Request {
        Request { document: self.document, revision: self.revision, layers: self.ids(), action }
    }
    pub fn preview(&self, action: &Action) -> Result<Prepared> {
        self.preview_with_projection(action, &NoProjection)
    }
    pub fn preview_with_projection(&self, action: &Action, projection: &dyn StrokeProjection) -> Result<Prepared> {
        let plan = self.plan(action)?;
        self.prepare(plan, projection)
    }
    pub fn commit(&self, s: &mut Session, action: Action) -> Result<Value> {
        if !self.is_current(s) {
            return Err(error("constructive gesture is stale; start again"));
        }
        // Reject a no-op before dispatch so an empty drag has no journal entry either.
        if self.plan(&action)?.remove.is_empty() {
            return Ok(json!({"layers":[],"changed":false}));
        }
        let id = action.command();
        s.execute(id, serde_json::to_value(self.request(action)).map_err(|e| error(e.to_string()))?)
    }
    fn same_parent(&self) -> Result<()> {
        let mut parent = None;
        for l in &self.inputs {
            let mut path = self.base.path_of(l.id).ok_or(EngineError::NoLayer(l.id))?;
            path.pop();
            if parent.as_ref().is_some_and(|p| p != &path) {
                return Err(error("merge/join targets must share a parent"));
            }
            parent = Some(path);
        }
        Ok(())
    }
    fn plan(&self, action: &Action) -> Result<Plan> {
        let mut plan = Plan::default();
        match action {
            Action::Builder { faces, remove, style } => {
                self.same_parent()?;
                if faces.is_empty() || faces.len() > MAX_OUTPUTS {
                    return Err(error("select a bounded set of faces"));
                }
                let map = self.arrangement()?;
                let mut chosen = BTreeSet::new();
                for &i in faces {
                    if i >= map.regions.len() || !chosen.insert(i) {
                        return Err(error("invalid or duplicate builder face"));
                    }
                }
                let owner = self.inputs.iter().position(|l| l.id == *style).ok_or(EngineError::NoLayer(*style))?;
                let shapes = self.builder_shapes()?;
                if !remove {
                    let refs = faces.iter().map(|&i| &map.regions[i]).collect::<Vec<_>>();
                    plan.add(owner, ops::merge_regions(&shapes, &refs).map_err(geometry)?);
                }
                for (i, face) in map.regions.iter().enumerate() {
                    if !chosen.contains(&i) {
                        plan.add(face.top().unwrap_or(0), face.path.clone());
                    }
                }
                for line in &map.lines {
                    let raw = ops::to_geometry(&line.path);
                    let between = if let Some(sp) = raw.subpaths.first().filter(|sp| sp.segment_count() > 0) {
                        let c = sp.segment(sp.segment_count() / 2);
                        let mid = c.eval(0.5);
                        let a = c.eval(0.49);
                        let b = c.eval(0.51);
                        let d = b - a;
                        let len = d.hypot();
                        if len <= 1e-12 {
                            false
                        } else {
                            let mut sides = [None; 2];
                            for (index, side) in [1.0, -1.0].into_iter().enumerate() {
                                let q = Point::new(mid.x - d.y / len * 0.001 * side, mid.y + d.x / len * 0.001 * side);
                                sides[index] = self.face_at(q)?;
                            }
                            matches!(sides, [Some(a), Some(b)] if a != b && chosen.contains(&a) && chosen.contains(&b))
                        }
                    } else {
                        false
                    };
                    if !between {
                        plan.add(line.key as usize, line.path.clone());
                    }
                }
                plan.remove = self.ids();
            }
            Action::Knife { .. } | Action::LineCut { .. } => {
                let stroke = match action {
                    Action::Knife { points: raw } => points(raw, 2)?,
                    Action::LineCut { from, to } => points(&[*from, *to], 2)?,
                    _ => return Err(error("invalid cut")),
                };
                if stroke.windows(2).all(|p| p[0] == p[1]) {
                    return Err(error("cut needs two different points"));
                }
                let sources = self.builder_shapes()?;
                for (i, l) in self.inputs.iter().enumerate() {
                    let source = sources[i].path.clone();
                    let mut knife = stroke.clone();
                    if matches!(action, Action::LineCut { .. }) {
                        let (a, b) = (stroke[0], stroke[1]);
                        let (dx, dy) = (b.x - a.x, b.y - a.y);
                        let len = dx.hypot(dy);
                        let reach = source.control_bounds().map_or(1.0, |(x0, y0, x1, y1)| {
                            [Point::new(x0, y0), Point::new(x1, y1), Point::new(x0, y1), Point::new(x1, y0)]
                                .iter()
                                .map(|p| (p.x - a.x).hypot(p.y - a.y))
                                .fold(1.0, f64::max)
                        }) + 1.0;
                        knife = vec![Point::new(a.x - dx / len * reach, a.y - dy / len * reach), Point::new(a.x + dx / len * reach, a.y + dy / len * reach)];
                    }
                    let cutter = Path::new(vec![polyline(&knife)]);
                    let before = ops::shape_builder(&[ops::Shape::new(source.clone(), 0)], false).map_err(geometry)?;
                    let arrangement = ops::shape_builder(&[ops::Shape::new(source.clone(), 0), ops::Shape::new(cutter, 1)], false).map_err(geometry)?;
                    let faces: Vec<_> = arrangement.regions.iter().filter(|r| r.sources.contains(&0)).collect();
                    let lines: Vec<_> = arrangement.lines.iter().filter(|l| l.key == 0).collect();
                    let open_count = source.subpaths.iter().filter(|s| !s.closed && s.knots.len() >= 2).count();
                    if faces.len() > before.regions.len() || lines.len() > open_count {
                        plan.remove.push(l.id);
                        for face in faces {
                            plan.add(i, face.path.clone());
                        }
                        for line in lines {
                            plan.add(i, line.path.clone());
                        }
                    }
                }
            }
            Action::Scissors { layer, subpath, segment, t } => {
                bounded(*t, 0.0, 1.0)?;
                let i = self.inputs.iter().position(|l| l.id == *layer).ok_or(EngineError::NoLayer(*layer))?;
                let path = &self.shape(i)?.path;
                let source = path.subpaths.get(*subpath).ok_or_else(|| error("invalid scissors subpath"))?;
                let segments = if source.closed { source.knots.len() } else { source.knots.len().saturating_sub(1) };
                if *segment >= segments {
                    return Err(error("invalid scissors segment"));
                }
                if !source.closed && ((*segment == 0 && *t == 0.0) || (*segment + 2 == source.knots.len() && *t == 1.0)) {
                    return Ok(plan);
                }
                let pieces = ops::split_at(&Path::new(vec![source.clone()]), 0, *segment, *t).map_err(geometry)?;
                plan.remove.push(*layer);
                // Unrelated compound contours stay together with the first cut piece.
                let mut pieces = pieces.into_iter();
                if let Some(mut first) = pieces.next() {
                    first.fill_rule = path.fill_rule;
                    first.inverted = path.inverted;
                    let mut cut = first.subpaths.clone();
                    if let Some(sp) = cut.first_mut() {
                        sp.op = source.op;
                    }
                    first.subpaths = path.subpaths.clone();
                    first.subpaths.splice(*subpath..=*subpath, cut);
                    plan.add(i, first);
                }
                for mut piece in pieces {
                    piece.fill_rule = path.fill_rule;
                    piece.inverted = path.inverted;
                    for sp in &mut piece.subpaths {
                        sp.op = source.op;
                    }
                    plan.add(i, piece);
                }
            }
            Action::Contour { delta, join, miter } => {
                bounded(*delta, -100_000.0, 100_000.0)?;
                bounded(*miter, 1.0, 1000.0)?;
                if *delta == 0.0 {
                    return Ok(plan);
                }
                for (i, l) in self.inputs.iter().enumerate() {
                    let path = &self.shape(i)?.path;
                    let source = if path.inverted { self.filled_path(path)? } else { path.clone() };
                    plan.replace(i, l.id, ops::offset_path(&source, *delta, join.geometry(), *miter).map_err(geometry)?);
                }
            }
            Action::Smooth { points: raw, radius, amount } => {
                let stroke = points(raw, 1)?;
                bounded(*radius, 0.01, 1000.0)?;
                bounded(*amount, 0.0, 1.0)?;
                if *amount == 0.0 {
                    return Ok(plan);
                }
                for (i, l) in self.inputs.iter().enumerate() {
                    let path = &self.shape(i)?.path;
                    let smoothed = ops::smooth(path, *amount).map_err(geometry)?;
                    let mut result = path.clone();
                    for (sp, next) in result.subpaths.iter_mut().zip(smoothed.subpaths) {
                        for (knot, next) in sp.knots.iter_mut().zip(next.knots) {
                            if distance(&stroke, knot.anchor) <= *radius {
                                *knot = next;
                            }
                        }
                    }
                    if result != *path {
                        plan.replace(i, l.id, result);
                    }
                }
            }
            Action::Erase { points: raw, width } => {
                let stroke = points(raw, 1)?;
                bounded(*width, 0.01, 1000.0)?;
                let brush = if stroke.len() == 1 {
                    photocraft_vector::shapes::ellipse(stroke[0].x - width / 2.0, stroke[0].y - width / 2.0, *width, *width)
                } else {
                    let path = Path::new(vec![polyline(&stroke)]);
                    ops::outline_stroke(
                        &path,
                        &photocraft_doc::ShapeStroke {
                            width: *width as f32,
                            cap: photocraft_doc::LineCap::Round,
                            join: photocraft_doc::LineJoin::Round,
                            ..Default::default()
                        },
                        0.01,
                    )
                    .map_err(geometry)?
                };
                for (i, l) in self.inputs.iter().enumerate() {
                    let path = &self.shape(i)?.path;
                    if let Some(pieces) = erase(path, &brush)? {
                        plan.remove.push(l.id);
                        for piece in pieces {
                            plan.add(i, piece);
                        }
                    }
                }
            }
            Action::Join { points: raw, tolerance } => {
                self.same_parent()?;
                let stroke = points(raw, 1)?;
                bounded(*tolerance, 0.0, 1000.0)?;
                let mut joining = Vec::new();
                let mut rest = BTreeMap::new();
                let mut ends = 0;
                for (i, l) in self.inputs.iter().enumerate() {
                    let path = &self.shape(i)?.path;
                    let mut untouched = path.clone();
                    untouched.subpaths.clear();
                    for sp in &path.subpaths {
                        let hit = !sp.closed
                            && sp.knots.len() >= 2
                            && [sp.knots.first(), sp.knots.last()].iter().flatten().any(|k| distance(&stroke, k.anchor) <= *tolerance);
                        if hit {
                            ends += [sp.knots.first(), sp.knots.last()].iter().flatten().filter(|k| distance(&stroke, k.anchor) <= *tolerance).count();
                            joining.push(Path::new(vec![sp.clone()]));
                        } else {
                            untouched.subpaths.push(sp.clone());
                        }
                    }
                    if untouched.subpaths.len() != path.subpaths.len() {
                        plan.remove.push(l.id);
                        rest.insert(i, untouched);
                    }
                }
                if ends < 2 {
                    return Ok(Plan::default());
                }
                let owner = rest.keys().next().copied().ok_or_else(|| error("join has no source"))?;
                let merged = ops::join(&joining, *tolerance).map_err(geometry)?;
                if let Some(path) = rest.get_mut(&owner) {
                    path.subpaths.extend(merged.subpaths);
                }
                for (i, path) in rest {
                    plan.add(i, path);
                }
            }
        }
        if plan.outputs.len() > MAX_OUTPUTS {
            return Err(error("constructive output exceeds the face budget"));
        }
        for (_, path) in &plan.outputs {
            validate_path(path)?;
        }
        Ok(plan)
    }
    fn prepare(&self, plan: Plan, projection: &dyn StrokeProjection) -> Result<Prepared> {
        let mut outputs = Vec::new();
        let mut reused = BTreeSet::new();
        for (source, path) in plan.outputs {
            let mut layer = self.inputs.get(source).cloned().ok_or_else(|| error("invalid output style source"))?;
            let anchor = layer.id;
            if !reused.insert(anchor) {
                layer.id = LayerId::fresh();
            }
            let LayerContent::Shape(shape) = &mut layer.content else {
                return Err(error("shape source required"));
            };
            shape.path = path;
            shape.live = None;
            shape.psd_raw = None;
            shape.cache = None;
            layer.vector.corners.clear();
            layer.vector.live_shape = None;
            layer.psd_blocks.retain(|(k, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&k));
            if let Some(a) = &layer.vector.appearance {
                photocraft_vector::appearance::mirror_legacy(shape, a);
            }
            outputs.push((anchor, layer));
        }
        let compiled = outputs
            .iter()
            .map(|(_, layer)| {
                let LayerContent::Shape(sh) = &layer.content else {
                    return Err(error("invalid prepared output"));
                };
                CompiledAppearance::with_projection(sh, layer.vector.appearance.as_ref(), &self.base.patterns, self.base.bounds(), projection).map_err(error)
            })
            .collect::<Result<Vec<_>>>()?;
        if compiled.iter().map(|c| c.cache_bytes(self.base.pixel_format())).sum::<u128>() > MAX_CACHE_BYTES {
            return Err(error("constructive caches exceed the 512 MiB budget"));
        }
        let caches = compiled.iter().map(|c| c.render(self.base.pixel_format(), self.base.bounds()).map_err(error)).collect::<Result<Vec<_>>>()?;
        drop(compiled);
        for ((_, layer), cache) in outputs.iter_mut().zip(caches) {
            if let LayerContent::Shape(sh) = &mut layer.content {
                sh.cache = Some(cache);
            }
        }
        Ok(Prepared { base: self.base.clone(), remove: plan.remove, outputs })
    }
}
#[derive(Default)]
struct Plan {
    remove: Vec<LayerId>,
    outputs: Vec<(usize, Path)>,
}
impl Plan {
    fn add(&mut self, source: usize, path: Path) {
        if !path.subpaths.is_empty() {
            self.outputs.push((source, path));
        }
    }
    fn replace(&mut self, source: usize, id: LayerId, path: Path) {
        self.remove.push(id);
        self.add(source, path);
    }
}
#[derive(Clone, Debug)]
pub struct Prepared {
    base: Arc<Document>,
    remove: Vec<LayerId>,
    outputs: Vec<(LayerId, Layer)>,
}
impl Prepared {
    pub fn changed(&self) -> bool {
        !self.remove.is_empty()
    }
    pub fn output_paths(&self) -> Vec<&Path> {
        self.outputs.iter().filter_map(|(_, l)| if let LayerContent::Shape(sh) = &l.content { Some(&sh.path) } else { None }).collect()
    }
    fn publish(&self, doc: &mut Document) -> Result<Vec<LayerId>> {
        let mut last = BTreeMap::new();
        let mut ids = Vec::new();
        let mut retained = BTreeSet::new();
        for (anchor, layer) in &self.outputs {
            if layer.id == *anchor {
                *doc.layer_mut(*anchor).ok_or(EngineError::NoLayer(*anchor))? = layer.clone();
                retained.insert(*anchor);
            } else {
                doc.insert_above(Some(last.get(anchor).copied().unwrap_or(*anchor)), layer.clone());
            }
            last.insert(*anchor, layer.id);
            ids.push(layer.id);
        }
        for id in &self.remove {
            if !retained.contains(id) {
                doc.remove(*id).ok_or(EngineError::NoLayer(*id))?;
            }
        }
        Ok(ids)
    }
    pub fn document(&self) -> Result<Document> {
        let mut doc = (*self.base).clone();
        self.publish(&mut doc)?;
        Ok(doc)
    }
}
fn polyline(points: &[Point]) -> Subpath {
    Subpath::polyline(&points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>())
}
/// Distance to the continuous scrub, not just the event samples.
pub fn distance(points: &[Point], q: Point) -> f64 {
    points
        .windows(2)
        .map(|p| {
            let (dx, dy) = (p[1].x - p[0].x, p[1].y - p[0].y);
            let d = dx * dx + dy * dy;
            let t = if d > 0.0 { ((q.x - p[0].x) * dx + (q.y - p[0].y) * dy) / d } else { 0.0 }.clamp(0.0, 1.0);
            (q.x - p[0].x - t * dx).hypot(q.y - p[0].y - t * dy)
        })
        .fold(points.first().map_or(f64::INFINITY, |p| (q.x - p.x).hypot(q.y - p.y)), f64::min)
}
/// Eraser boundaries come from V1's arrangement. V1 cut_out retains cubic subsegments exactly;
/// this avoids the upstream fixed 24-sample mask, which can miss narrow erased intervals.
fn erase(path: &Path, brush: &Path) -> Result<Option<Vec<Path>>> {
    let mut changed = false;
    let mut output = Vec::new();
    let brush_outline = ops::to_geometry(brush).to_bezpath();
    for original in &path.subpaths {
        let source = Path::new(vec![original.clone()]);
        let arrangement = ops::shape_builder(&[ops::Shape::new(source.clone(), 0), ops::Shape::new(brush.clone(), 1)], true).map_err(geometry)?;
        let pieces = arrangement.lines.iter().chain(&arrangement.edges).filter(|p| p.key == 0);
        let mut raw = ops::to_geometry(&source);
        for piece in pieces {
            let geometry_piece = ops::to_geometry(&piece.path);
            for sp in &geometry_piece.subpaths {
                let n = sp.segment_count();
                if n == 0 {
                    continue;
                }
                let mid = sp.segment(n / 2).eval(0.5);
                if brush_outline.winding(mid) != 0
                    && let Some(cut) = ops::kernel::cut_out(&raw, sp, false, 0.001)
                {
                    changed = true;
                    raw = cut;
                }
            }
        }
        if raw == ops::to_geometry(&source) {
            output.push(source);
        } else {
            output.extend(raw.subpaths.into_iter().map(|sp| {
                let mut result = ops::from_geometry(&ops::geom::PathData::single(sp));
                if let Some(sp) = result.subpaths.first_mut() {
                    sp.op = original.op;
                }
                result
            }));
        }
    }
    for piece in &mut output {
        piece.fill_rule = path.fill_rule;
        piece.inverted = path.inverted;
    }
    Ok(changed.then_some(if path.subpaths.len() == 1 {
        output
    } else {
        let mut result = path.clone();
        result.subpaths = output.into_iter().flat_map(|p| p.subpaths).collect();
        if result.subpaths.is_empty() { Vec::new() } else { vec![result] }
    }))
}
fn decode(s: &Session, p: &Value) -> Result<(Gesture, Action)> {
    let request: Request = serde_json::from_value(p.clone()).map_err(|e| error(format!("invalid constructive request: {e}")))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != request.document || st.revision != request.revision {
        return Err(error("constructive gesture is stale; start again"));
    }
    Ok((Gesture::begin(s, &request.layers)?, request.action))
}
fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    let (gesture, action) = decode(s, p)?;
    let prepared = gesture.preview(&action)?;
    if !prepared.changed() {
        return Ok(json!({"layers":[],"changed":false}));
    }
    vector_api::snapshot_shapes(&s.active().ok_or(EngineError::NoDocument)?.doc, &gesture.ids())?;
    let ids = s.edit(action.label(), |doc, active| {
        let ids = prepared.publish(doc)?;
        *active = ids.first().copied();
        Ok(ids)
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = ids.clone();
        st.layer_anchor = ids.first().copied();
        st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected: ids.clone() });
    }
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>(),"changed":true}))
}
fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    let (gesture, action) = decode(s, p)?;
    let prepared = gesture.preview(&action)?;
    Ok(json!({"paths":prepared.output_paths(),"changed":prepared.changed(),"detached":true}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("path.builder.commit", true),
        ("path.cut.commit", true),
        ("path.erase.commit", true),
        ("path.constructive.commit", true),
        ("path.constructive.preview", false),
    ]
    .into_iter()
    .map(|(id, journal)| CommandSpec {
        id,
        label: "Constructive Paths",
        menu: &[],
        shortcut: None,
        params: "{document,revision,layers:[id],action:{kind,...}}",
        enabled: crate::vector_cmds::has_doc,
        run: if journal { apply } else { preview },
        journal,
    })
    .collect()
}
#[cfg(test)]
#[path = "constructive/tests.rs"]
mod tests;
