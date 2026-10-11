//! Captured Live Paint / Vector Flood Fill transactions. GPL-3.0-or-later Local Image.
//! Make/Merge/Expand/Release and face/edge paint follow VectorCraft buildcmds.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092, MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Shared LiveGroup data and display/export dispatch are integrated by package 00.
use crate::{EngineError, Result, Session, commands::CommandSpec};
pub use evaluator::{Gap, Graph, LivePaintSpec, Signature};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, ShapeLayer, ShapeStroke};
use photocraft_geom::Point;
use photocraft_pathops as ops;
pub use photocraft_vecfx::live_paint as evaluator;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Arc,
};
fn error(e: impl ToString) -> EngineError {
    EngineError::Other(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let mut layers = if ids.is_empty() {
            let selected = st.selected_layers();
            if selected.is_empty() {
                let mut out = Vec::new();
                let mut pending = st.doc.layers.iter().rev().collect::<Vec<_>>();
                while let Some(layer) = pending.pop() {
                    if !layer.visible || layer.vector.live.is_some() {
                        continue;
                    }
                    if matches!(layer.content, LayerContent::Shape(_)) {
                        out.push(layer.id);
                        if out.len() > evaluator::MAX_SOURCES {
                            break;
                        }
                    } else if let Some(children) = layer.children() {
                        pending.extend(children.iter().rev());
                    }
                }
                out
            } else {
                selected
            }
        } else {
            ids.to_vec()
        };
        preflight(&st.doc, &layers)?;
        layers.sort_by_key(|id| st.doc.path_of(*id));
        Ok(Self { document: st.doc.id.0, revision: st.revision, layers })
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id.0 != self.document || st.revision != self.revision {
            return Err(error("paint gesture is stale; start again"));
        }
        preflight(&st.doc, &self.layers)
    }
}
fn preflight(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() || ids.len() > evaluator::MAX_SOURCES {
        return Err(error("region paint requires 1 to 128 targets"));
    }
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(error("duplicate region paint target"));
        }
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        for (p, _, l) in doc.walk().into_iter().filter(|(p, _, _)| p.starts_with(&path) || path.starts_with(p)) {
            if path.starts_with(&p) && !l.visible {
                return Err(error("paint target has a hidden ancestor"));
            }
            let locks = doc.effective_locks(l.id);
            if locks.all || locks.pixels || locks.position || locks.artboard {
                return Err(error("paint target, ancestor or source is locked"));
            }
            if p != path && ids.contains(&l.id) {
                return Err(error("select a paint group or its sources, not both"));
            }
            if path.starts_with(&p) && p != path && l.vector.live.is_some() {
                return Err(error("select the live paint group to paint its regions"));
            }
        }
        let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        if !layer.visible || layer.clipped || layer.mask.is_some() || layer.vector_mask.is_some() || layer.vector.clip.is_some() {
            return Err(error("paint targets must be visible and have projected clipping/masks"));
        }
        if !matches!(layer.content, LayerContent::Shape(_)) && evaluator::read_spec(layer).map_err(error)?.is_none() {
            return Err(error("paint targets must be curves or live paint groups"));
        }
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Insertion {
    #[default]
    Inside,
    InBetween,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillMode {
    AddOnTop,
    #[default]
    SmartRefill,
    Knockout,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloodOptions {
    #[serde(default)]
    pub insertion: Insertion,
    #[serde(default)]
    pub mode: FillMode,
    #[serde(default)]
    pub visible_boundaries: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Make {
        #[serde(default)]
        gap: Gap,
    },
    Paint {
        points: Vec<[f64; 2]>,
        paint: Option<Fill>,
        #[serde(default)]
        edges: bool,
        #[serde(default = "default_width")]
        width: f32,
        #[serde(default = "default_tolerance")]
        tolerance: f64,
    },
    Flood {
        points: Vec<[f64; 2]>,
        paint: Fill,
        #[serde(default)]
        options: FloodOptions,
    },
    Options {
        gap: Gap,
    },
    Merge,
    Expand,
    Release,
}
fn default_width() -> f32 {
    1.
}
fn default_tolerance() -> f64 {
    4.
}
impl Action {
    pub fn command(&self) -> &'static str {
        match self {
            Self::Make { .. } => "livePaint.make",
            Self::Paint { .. } => "livePaint.paint",
            Self::Flood { .. } => "vector.floodFill",
            Self::Options { .. } => "livePaint.options",
            Self::Merge => "livePaint.merge",
            Self::Expand => "livePaint.expand",
            Self::Release => "livePaint.release",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Make { .. } => "Make Live Paint",
            Self::Paint { .. } => "Live Paint Bucket",
            Self::Flood { .. } => "Vector Flood Fill",
            Self::Options { .. } => "Live Paint Gap Options",
            Self::Merge => "Merge Live Paint",
            Self::Expand => "Expand Live Paint",
            Self::Release => "Release Live Paint",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub action: Action,
}
#[derive(Clone, Debug)]
pub struct Gesture {
    pub capture: Capture,
    pub sources: Vec<Layer>,
    pub spec: LivePaintSpec,
    pub group: Option<LayerId>,
    pub graph: Arc<Graph>,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let capture = Capture::begin(s, ids)?;
        Self::from_capture(s, capture)
    }
    fn from_capture(s: &Session, capture: Capture) -> Result<Self> {
        capture.check(s)?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let mut sources = Vec::new();
        let mut spec = LivePaintSpec::default();
        let mut group = None;
        // Existing live children retain their stable source slots. New paths are appended,
        // including paths selected below the live group in the document stack.
        for id in &capture.layers {
            let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            if let Some(saved) = evaluator::read_spec(layer).map_err(error)? {
                if group.is_some() {
                    return Err(error("merge one live paint group with new paths at a time"));
                }
                group = Some(*id);
                spec = saved;
                sources.extend(layer.children().ok_or_else(|| error("live paint sources missing"))?.iter().cloned());
            }
        }
        for id in &capture.layers {
            let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            if evaluator::read_spec(layer).map_err(error)?.is_none() {
                sources.push(layer.clone());
            }
        }
        let graph = Arc::new(Graph::new(&sources, spec.gap).map_err(error)?);
        Ok(Self { capture, sources, spec, group, graph })
    }
    pub fn is_current(&self, s: &Session) -> bool {
        self.capture.check(s).is_ok()
    }
    pub fn request(&self, action: Action) -> Request {
        Request { capture: self.capture.clone(), action }
    }
    pub fn preview(&self, s: &Session, action: Action) -> Result<Preview> {
        prepare_gesture(s, &self.request(action), self)
    }
    pub fn commit(&self, s: &mut Session, action: Action) -> Result<Vec<LayerId>> {
        let cmd = action.command();
        let params = serde_json::to_value(self.request(action)).map_err(error)?;
        let value = s.execute(cmd, params)?;
        serde_json::from_value(value["layers"].clone()).map_err(error)
    }
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub model: Document,
    pub display: Document,
    pub layers: Vec<LayerId>,
    pub changed: bool,
}
fn points(raw: &[[f64; 2]]) -> Result<Vec<Point>> {
    if raw.is_empty() || raw.len() > evaluator::MAX_POINTS {
        return Err(error("invalid paint point count"));
    }
    raw.iter()
        .map(|p| {
            let p = Point::new(p[0], p[1]);
            evaluator::point(p).map_err(error)?;
            Ok(p)
        })
        .collect()
}
fn siblings<'a>(doc: &'a mut Document, parent: &[usize]) -> Result<&'a mut Vec<Layer>> {
    if parent.is_empty() {
        return Ok(&mut doc.layers);
    }
    doc.layer_at_mut(parent).and_then(Layer::children_mut).ok_or_else(|| error("paint insertion parent is not a group"))
}
fn next_id(doc: &Document, next: &mut u64) -> Result<LayerId> {
    *next = (*next).max(photocraft_vecfx::blend::max_document_id(doc)).checked_add(1).ok_or_else(|| error("paint ID overflow"))?;
    Ok(LayerId(*next))
}
fn make_group(doc: &mut Document, g: &Gesture, spec: &LivePaintSpec) -> Result<LayerId> {
    let paths = g.capture.layers.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    let first = &paths[0];
    let parent = first[..first.len() - 1].to_vec();
    if paths.iter().any(|p| p[..p.len() - 1] != parent) {
        return Err(error("live paint targets must be siblings"));
    }
    let indexes = paths.iter().map(|p| p[p.len() - 1]).collect::<BTreeSet<_>>();
    let top = *indexes.last().ok_or_else(|| error("paint targets missing"))?;
    let id = if let Some(id) = g.group { id } else { next_id(doc, &mut 0)? };
    let mut group = if let Some(old) = g.group { doc.layer(old).ok_or(EngineError::NoLayer(old))?.clone() } else { Layer::group("Live Paint", Vec::new()) };
    group.id = id;
    let LayerContent::Group(content) = &mut group.content else { return Err(error("live paint target must be a group")) };
    content.children = g.sources.clone();
    evaluator::write_spec(&mut group, spec).map_err(error)?;
    // Preserve unrelated siblings, and insert where the front-most selected target stood.
    let siblings = siblings(doc, &parent)?;
    let at = top - indexes.iter().filter(|i| **i < top).count();
    for i in indexes.iter().rev() {
        siblings.remove(*i);
    }
    siblings.insert(at, group);
    Ok(id)
}
fn filled(layer: &Layer) -> Result<Appearance> {
    let LayerContent::Shape(shape) = &layer.content else { return Err(error("flood source must be a shape")) };
    let mut appearance = layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape));
    if appearance.items.iter().any(|i| matches!(i, AppearanceItem::Mesh(_))) {
        return Err(error("Flood Fill requires ordinary fill/stroke paints"));
    }
    appearance.items.retain(|i| matches!(i, AppearanceItem::Fill(_)));
    Ok(appearance)
}
fn flood(doc: &mut Document, g: &Gesture, raw: &[[f64; 2]], paint: &Fill, options: &FloodOptions) -> Result<Vec<LayerId>> {
    evaluator::validate_fill(paint).map_err(error)?;
    if g.group.is_some() {
        return Err(error("expand a live paint group before destructive flood fill"));
    }
    let mut faces = g.graph.touched(&points(raw)?).map_err(error)?;
    if options.visible_boundaries {
        faces = g.graph.visible_faces(&faces, &g.sources).map_err(error)?;
    }
    if faces.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_target: BTreeMap<LayerId, BTreeSet<usize>> = BTreeMap::new();
    for i in faces {
        let region = &g.graph.faces[i].region;
        let target = region.sources.iter().rev().filter_map(|i| g.sources.get(*i)).find(|l| evaluator::fill(l).is_some()).unwrap_or(&g.sources[0]);
        let appearance = filled(target)?;
        let duplicate = appearance.items.iter().any(|f| matches!(f, AppearanceItem::Fill(f) if f.visible && f.paint == *paint));
        if options.mode != FillMode::SmartRefill || !duplicate {
            by_target.entry(target.id).or_default().insert(i);
        }
    }
    let mut out = Vec::new();
    let mut next = photocraft_vecfx::blend::max_document_id(doc);
    for (id, faces) in by_target {
        let original = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
        let LayerContent::Shape(shape) = &original.content else { return Err(error("flood target must be a shape")) };
        let area = g.graph.merge(&faces).map_err(error)?;
        let remainder = if shape.path.subpaths.iter().any(|s| s.closed) {
            ops::boolean(&shape.path, &area, ops::BoolOp::Difference).map_err(error)?
        } else {
            shape.path.clone()
        };
        let mut appearance = filled(&original)?;
        let opaque = matches!(paint, Fill::Solid(c) if c.alpha == 1.);
        if opaque || options.mode == FillMode::Knockout {
            appearance.items.clear();
        }
        appearance.items.push(AppearanceItem::Fill(FillItem::new(paint.clone())));
        photocraft_vector::appearance::validate(&appearance).map_err(error)?;
        // Split the original's fill from its unchanged stroke, avoiding double alpha
        // when a transparent new face inherits the old fill stack.
        let mut base_fill = original.clone();
        base_fill.id = next_id(doc, &mut next)?;
        base_fill.opacity = 1.;
        base_fill.fill_opacity = 1.;
        base_fill.effects = Default::default();
        base_fill.blend = photocraft_doc::BlendMode::Normal;
        base_fill.vector.appearance = Some(filled(&original)?);
        if let LayerContent::Shape(s) = &mut base_fill.content {
            s.path = remainder;
            s.live = None;
            s.psd_raw = None;
            s.cache = None;
            photocraft_vector::appearance::mirror_legacy(s, base_fill.vector.appearance.as_ref().ok_or_else(|| error("flood appearance missing"))?);
        }
        let mut base_stroke = original.clone();
        base_stroke.id = next_id(doc, &mut next)?;
        base_stroke.opacity = 1.;
        base_stroke.fill_opacity = 1.;
        base_stroke.effects = Default::default();
        base_stroke.blend = photocraft_doc::BlendMode::Normal;
        let mut stroke_app = original.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape));
        stroke_app.items.retain(|i| matches!(i, AppearanceItem::Stroke(_)));
        base_stroke.vector.appearance = Some(stroke_app.clone());
        if let LayerContent::Shape(s) = &mut base_stroke.content {
            s.live = None;
            s.psd_raw = None;
            s.cache = None;
            photocraft_vector::appearance::mirror_legacy(s, &stroke_app);
        }
        let mut face = original.clone();
        face.id = next_id(doc, &mut next)?;
        face.name = "Flood Fill".into();
        face.vector = Default::default();
        face.vector.appearance = Some(appearance.clone());
        face.content = LayerContent::Shape(ShapeLayer { path: area, ..Default::default() });
        if let LayerContent::Shape(s) = &mut face.content {
            photocraft_vector::appearance::mirror_legacy(s, &appearance);
        }
        let face_id = face.id;
        out.push(face_id);
        let mut group = original.clone();
        group.vector = Default::default();
        let children = if options.insertion == Insertion::Inside {
            face.opacity = 1.;
            face.fill_opacity = 1.;
            face.effects = Default::default();
            face.blend = photocraft_doc::BlendMode::Normal;
            vec![base_fill, face.clone(), base_stroke]
        } else {
            vec![base_fill, base_stroke]
        };
        group.content = Layer::group("Flood Fill", children).content;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = group;
        if options.insertion == Insertion::InBetween {
            let path = doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
            siblings(doc, &path[..path.len() - 1])?.insert(path[path.len() - 1] + 1, face);
        }
    }
    Ok(out)
}
pub fn prepare(s: &Session, request: &Request) -> Result<Preview> {
    let g = Gesture::from_capture(s, request.capture.clone())?;
    prepare_gesture(s, request, &g)
}
fn prepare_gesture(s: &Session, request: &Request, g: &Gesture) -> Result<Preview> {
    request.capture.check(s)?;
    let mut model = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let mut spec = g.spec.clone();
    let layers = match &request.action {
        Action::Flood { points, paint, options } => flood(&mut model, g, points, paint, options)?,
        Action::Make { gap } => {
            if g.group.is_some() {
                return Err(error("targets already include a live paint group; use Merge"));
            }
            spec.gap = *gap;
            Graph::new(&g.sources, *gap).map_err(error)?;
            vec![make_group(&mut model, g, &spec)?]
        }
        Action::Paint { points: raw, paint, edges, width, tolerance } => {
            if let Some(p) = paint {
                evaluator::validate_fill(p).map_err(error)?;
            }
            if !width.is_finite() || !(0. ..=5000.).contains(width) || !tolerance.is_finite() || !(0.01..=1000.).contains(tolerance) {
                return Err(error("invalid edge paint width/tolerance"));
            }
            let pts = points(raw)?;
            if *edges {
                let mut hits = BTreeSet::new();
                for p in edge_samples(&pts, *tolerance)? {
                    if let Some(i) = g.graph.edge_at(p, *tolerance).map_err(error)? {
                        hits.insert(i);
                    }
                }
                for i in hits {
                    spec.set_edge(g.graph.edges[i].signature.clone(), paint.clone().map(|p| ShapeStroke { paint: p, width: *width, ..Default::default() }))
                        .map_err(error)?;
                }
            } else {
                for i in g.graph.touched(&pts).map_err(error)? {
                    spec.set_face(g.graph.faces[i].signature.clone(), paint.clone()).map_err(error)?;
                }
            }
            if spec == g.spec {
                Vec::new()
            } else if let Some(id) = g.group {
                if g.capture.layers.len() != 1 {
                    return Err(error("paint one live group at a time"));
                }
                evaluator::write_spec(model.layer_mut(id).ok_or(EngineError::NoLayer(id))?, &spec).map_err(error)?;
                vec![id]
            } else {
                vec![make_group(&mut model, g, &spec)?]
            }
        }
        Action::Merge => {
            if g.group.is_none() || g.capture.layers.len() < 2 {
                return Err(error("Merge requires a live group and new paths"));
            }
            // Existing signatures remain authoritative. New split faces inherit paint by
            // containment of a strict interior point in the old source arrangement.
            let id = g.group.ok_or_else(|| error("live group missing"))?;
            let old = model.layer(id).ok_or(EngineError::NoLayer(id))?;
            let old_sources = old.children().ok_or_else(|| error("live sources missing"))?;
            let old_graph = Graph::new(old_sources, spec.gap).map_err(error)?;
            let old_spec = spec.clone();
            for (i, f) in g.graph.faces.iter().enumerate() {
                let geo = ops::to_geometry(&f.region.path);
                if let Some(p) = ops::kernel::interior_point(&geo)
                    && let Some(old_i) = old_graph.face_at(Point::new(p.x, p.y)).map_err(error)?
                {
                    let paint = old_graph.face_paint(old_i, old_sources, &old_spec).map_err(error)?;
                    spec.set_face(g.graph.faces[i].signature.clone(), paint).map_err(error)?;
                }
            }
            // An added path may split an existing painted edge into several branches.
            // Transfer only along the same editable source; new paths keep their strokes.
            for edge in &g.graph.edges {
                let geo = ops::to_geometry(&edge.path);
                let Some(curve) = geo.to_bezpath().segments().next() else { continue };
                let p = ops::geom::ParamCurve::eval(&curve, 0.5);
                for (i, old_edge) in old_graph.edges.iter().enumerate().filter(|(_, e)| e.source == edge.source) {
                    if ops::nearest(&old_edge.path, Point::new(p.x, p.y)).map_err(error)?.is_some_and(|(_, _, _, _, d)| d < 0.01) {
                        let stroke = old_graph.edge_paint(i, old_sources, &old_spec).map_err(error)?;
                        spec.set_edge(edge.signature.clone(), stroke).map_err(error)?;
                        break;
                    }
                }
            }
            vec![make_group(&mut model, g, &spec)?]
        }
        Action::Options { gap } => {
            let id = single_group(g)?;
            spec.gap = *gap;
            Graph::new(&g.sources, *gap).map_err(error)?;
            evaluator::write_spec(model.layer_mut(id).ok_or(EngineError::NoLayer(id))?, &spec).map_err(error)?;
            vec![id]
        }
        Action::Expand => {
            let id = single_group(g)?;
            let projected = evaluator::project_document(&model).map_err(error)?;
            *model.layer_mut(id).ok_or(EngineError::NoLayer(id))? = projected.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
            vec![id]
        }
        Action::Release => {
            let id = single_group(g)?;
            let path = model.path_of(id).ok_or(EngineError::NoLayer(id))?;
            let children = model.layer(id).and_then(Layer::children).ok_or_else(|| error("live sources missing"))?.to_vec();
            let ids = children.iter().map(|l| l.id).collect();
            siblings(&mut model, &path[..path.len() - 1])?.splice(path[path.len() - 1]..=path[path.len() - 1], children);
            ids
        }
    };
    request.capture.check(s)?;
    // Cache budget is checked before allocations, and only missing/generated caches rebuild.
    photocraft_vecfx::blend::prepare_caches(&mut model).map_err(error)?;
    let display = evaluator::project_document(&model).map_err(error)?;
    let changed = model.layers != s.active().ok_or(EngineError::NoDocument)?.doc.layers;
    Ok(Preview { model, display, layers, changed })
}
fn single_group(g: &Gesture) -> Result<LayerId> {
    if g.capture.layers.len() != 1 {
        return Err(error("operation requires one live paint group"));
    }
    g.group.ok_or_else(|| error("select a live paint group"))
}
fn edge_samples(points: &[Point], tolerance: f64) -> Result<Vec<Point>> {
    let mut out = vec![points[0]];
    for pair in points.windows(2) {
        let n = ((pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y) / (tolerance * 0.5)).ceil().max(1.) as usize;
        if n > evaluator::MAX_POINTS || out.len() + n > evaluator::MAX_POINTS {
            return Err(error("edge gesture exceeds sampling budget"));
        }
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push(Point::new(pair[0].x + (pair[1].x - pair[0].x) * t, pair[0].y + (pair[1].y - pair[0].y) * t));
        }
    }
    Ok(out)
}
pub fn commit(s: &mut Session, request: &Request) -> Result<Vec<LayerId>> {
    let preview = prepare(s, request)?;
    if !preview.changed {
        return Ok(preview.layers);
    }
    request.capture.check(s)?;
    let ids = preview.layers;
    let selected = ids.last().copied();
    let max = photocraft_vecfx::blend::max_document_id(&preview.model);
    s.edit(request.action.label(), move |doc, active| {
        doc.layers = preview.model.layers;
        *active = selected;
        photocraft_doc::ensure_ids_above(max);
        Ok(())
    })?;
    // Structural edit selection belongs to the edit's history target. Standalone
    // set_selection would add another revision and clear the edit's canvas damage.
    crate::layer_multi_cmds::reselect(s, ids.clone(), selected);
    Ok(ids)
}
fn run(s: &mut Session, p: &Value, command: &str) -> Result<Value> {
    let request: Request = serde_json::from_value(p.clone()).map_err(error)?;
    if request.action.command() != command {
        return Err(error("paint action does not match command"));
    }
    Ok(json!({"layers":commit(s, &request)?}))
}
fn preview_command(s: &mut Session, p: &Value) -> Result<Value> {
    let request: Request = serde_json::from_value(p.clone()).map_err(error)?;
    let preview = prepare(s, &request)?;
    Ok(json!({"layers":preview.layers,"changed":preview.changed}))
}
fn info(s: &mut Session, p: &Value) -> Result<Value> {
    let capture: Capture = serde_json::from_value(p["capture"].clone()).map_err(error)?;
    let g = Gesture::from_capture(s, capture)?;
    Ok(json!({"group":g.group,"faces":g.graph.faces.iter().map(|f|json!({"signature":f.signature,"area":f.area,"path":f.region.path})).collect::<Vec<_>>(),
        "edges":g.graph.edges.iter().map(|e|json!({"signature":e.signature,"source":e.source,"path":e.path})).collect::<Vec<_>>(),"spec":g.spec}))
}
fn select(s: &mut Session, p: &Value) -> Result<Value> {
    let capture: Capture = serde_json::from_value(p["capture"].clone()).map_err(error)?;
    let g = Gesture::from_capture(s, capture)?;
    let raw: Vec<[f64; 2]> = serde_json::from_value(p["points"].clone()).map_err(error)?;
    let pts = points(&raw)?;
    if p.get("edges").and_then(Value::as_bool).unwrap_or(false) {
        let tolerance = p.get("tolerance").and_then(Value::as_f64).unwrap_or(4.);
        if !tolerance.is_finite() || !(0.01..=1000.).contains(&tolerance) {
            return Err(error("invalid edge selection tolerance"));
        }
        let mut edges = BTreeSet::new();
        for pt in edge_samples(&pts, tolerance)? {
            if let Some(i) = g.graph.edge_at(pt, tolerance).map_err(error)? {
                edges.insert(i);
            }
        }
        return Ok(json!({"edges":edges.iter().map(|i|&g.graph.edges[*i].signature).collect::<Vec<_>>()}));
    }
    let faces = g.graph.touched(&pts).map_err(error)?;
    Ok(json!({"faces":faces.iter().map(|i|&g.graph.faces[*i].signature).collect::<Vec<_>>()}))
}
fn sample(s: &mut Session, p: &Value) -> Result<Value> {
    let capture: Capture = serde_json::from_value(p["capture"].clone()).map_err(error)?;
    let g = Gesture::from_capture(s, capture)?;
    let raw: [f64; 2] = serde_json::from_value(p["point"].clone()).map_err(error)?;
    let point = Point::new(raw[0], raw[1]);
    if p.get("edges").and_then(Value::as_bool).unwrap_or(false) {
        let tolerance = p.get("tolerance").and_then(Value::as_f64).unwrap_or(4.);
        let stroke =
            g.graph.edge_at(point, tolerance).map_err(error)?.map(|i| g.graph.edge_paint(i, &g.sources, &g.spec)).transpose().map_err(error)?.flatten();
        Ok(json!({"stroke":stroke}))
    } else {
        let paint = g.graph.face_at(point).map_err(error)?.map(|i| g.graph.face_paint(i, &g.sources, &g.spec)).transpose().map_err(error)?.flatten();
        Ok(json!({"paint":paint}))
    }
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    type Handler = fn(&mut Session, &Value) -> Result<Value>;
    let edits: [(&str, &str, Handler); 7] = [
        ("livePaint.make", "Make Live Paint", |s, p| run(s, p, "livePaint.make")),
        ("livePaint.paint", "Live Paint Bucket", |s, p| run(s, p, "livePaint.paint")),
        ("vector.floodFill", "Vector Flood Fill", |s, p| run(s, p, "vector.floodFill")),
        ("livePaint.options", "Live Paint Gap Options", |s, p| run(s, p, "livePaint.options")),
        ("livePaint.merge", "Merge Live Paint", |s, p| run(s, p, "livePaint.merge")),
        ("livePaint.expand", "Expand Live Paint", |s, p| run(s, p, "livePaint.expand")),
        ("livePaint.release", "Release Live Paint", |s, p| run(s, p, "livePaint.release")),
    ];
    let mut out = edits
        .into_iter()
        .map(|(id, label, handler)| CommandSpec {
            id,
            label,
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision,layers},action:Action}",
            enabled: crate::vector_cmds::has_doc,
            journal: true,
            run: handler,
        })
        .collect::<Vec<_>>();
    for (id, handler) in [
        ("livePaint.preview", preview_command as fn(&mut Session, &Value) -> Result<Value>),
        ("livePaint.info", info),
        ("livePaint.select", select),
        ("livePaint.sample", sample),
    ] {
        out.push(CommandSpec {
            id,
            label: "Region Paint Query",
            menu: &[],
            shortcut: None,
            params: "{capture,...}",
            enabled: crate::vector_cmds::has_doc,
            journal: false,
            run: handler,
        });
    }
    out
}
#[cfg(test)]
#[path = "live_paint/gpu.rs"]
mod gpu;
#[cfg(test)]
#[path = "live_paint/tests.rs"]
mod tests;
