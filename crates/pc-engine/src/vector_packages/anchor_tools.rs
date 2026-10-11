//! Anchor-tool transactions and selected-curve hover helpers. Behavior adapted from
//! VectorCraft tools/src/draw2/{anchor,curvature}.rs and tools/src/draw2.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Notices: licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Existing node commands own insert/delete/convert/handle/bend geometry and corner remaps.
//! The new Curvature command replaces only the captured subpath and preserves knot identity.

use super::{node, pen};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{Document, Fill, LayerContent, LayerId, Path, ShapeStroke};
use photocraft_geom::Point;
use photocraft_vector::{curvature_fit, edit};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::OnceLock};

fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
pub fn valid_point(p: Point) -> Result<()> {
    if [p.x, p.y].iter().any(|v| !v.is_finite() || v.abs() > edit::MAX_COORDINATE) {
        return Err(error("anchor input must be finite and bounded"));
    }
    Ok(())
}

/// Guards and the complete selected set, captured at press. Selection changes never retarget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub targets: Vec<node::Target>,
    pub convert_live: bool,
}
impl Capture {
    pub fn begin(s: &Session, targets: Vec<node::Target>, convert_live: bool) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        validate_targets(&st.doc, &targets, convert_live)?;
        for target in &targets {
            cache_budget(&st.doc, target, None)?;
        }
        Ok(Self { document: st.doc.id.0, revision: st.revision, targets, convert_live })
    }
    pub fn validate(&self, s: &Session) -> Result<()> {
        pen::check_fresh(s, self.document, self.revision)?;
        validate_targets(&s.active().ok_or(EngineError::NoDocument)?.doc, &self.targets, self.convert_live)
    }
}

fn validate_targets(doc: &Document, targets: &[node::Target], convert_live: bool) -> Result<()> {
    if targets.is_empty() || targets.len() > 64 {
        return Err(error("select 1..64 curves"));
    }
    let mut seen = HashSet::new();
    for t in targets {
        let key = if t.name.is_empty() || t.name == "Work Path" || t.name.eq_ignore_ascii_case("work") { "work" } else { &t.name };
        if !seen.insert((key, t.layer)) {
            return Err(error("duplicate anchor target"));
        }
        let path = node::path(doc, t)?;
        edit::validate(path).map_err(error)?;
        pen::validate(path)?;
        for [s, k] in &t.anchors {
            if path.subpaths.get(*s).and_then(|sp| sp.knots.get(*k)).is_none() {
                return Err(error("selected anchor is missing"));
            }
        }
        if let Some(layer) = t.layer.and_then(|id| doc.layer(LayerId(id))) {
            if (layer.vector.live_shape.is_some() || matches!(&layer.content, LayerContent::Shape(sh) if sh.live.is_some())) && !convert_live {
                return Err(error("Convert to curves before editing this live shape"));
            }
            for c in &layer.vector.corners {
                if path.subpaths.get(c.subpath).and_then(|sp| sp.knots.get(c.knot)).is_none()
                    || !c.corner.radius.is_finite()
                    || !(0.0..=edit::MAX_COORDINATE).contains(&c.corner.radius)
                {
                    return Err(error("invalid corner reference"));
                }
            }
        }
    }
    Ok(())
}
fn cache_budget(doc: &Document, t: &node::Target, path: Option<&Path>) -> Result<()> {
    if let Some(layer) = t.layer.and_then(|id| doc.layer(LayerId(id)))
        && let LayerContent::Shape(sh) = &layer.content
    {
        let mut shape = sh.clone();
        if let Some(path) = path {
            shape.path = path.clone();
        }
        let compiled = photocraft_vector::CompiledShape::new(&shape, photocraft_vector::DEFAULT_TOLERANCE, doc.bounds());
        if compiled.bounds().is_some_and(|b| b.intersect(&doc.bounds()).size().area() > 64 * 1024 * 1024) {
            return Err(error("anchor cache exceeds the pixel budget"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NodeHit {
    pub target: usize,
    pub knot: [usize; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SegmentHit {
    pub target: usize,
    pub knot: [usize; 2],
    pub t: f64,
}
fn query_inputs(doc: &Document, targets: &[node::Target], p: Point, tolerance: f64) -> Result<()> {
    valid_point(p)?;
    if !tolerance.is_finite() || !(0.0..=edit::MAX_COORDINATE).contains(&tolerance) {
        return Err(error("invalid anchor hit tolerance"));
    }
    // Hover on live shapes is allowed; the gesture preflight produces the conversion prompt.
    if targets.is_empty() { Ok(()) } else { validate_targets(doc, targets, true) }
}
/// Selected targets, front first. Nodes win over segments for Pen auto add/delete.
pub fn node_under(doc: &Document, targets: &[node::Target], p: Point, tolerance: f64) -> Result<Option<NodeHit>> {
    query_inputs(doc, targets, p, tolerance)?;
    for (target, t) in targets.iter().enumerate() {
        if let Some(edit::Hit::Anchor(knot)) = node::hit(node::path(doc, t)?, p, tolerance, &[]) {
            return Ok(Some(NodeHit { target, knot }));
        }
    }
    Ok(None)
}
/// Exact V1 nearest point. Endpoints are excluded so hover never inserts coincident knots.
pub fn segment_under(doc: &Document, targets: &[node::Target], p: Point, tolerance: f64) -> Result<Option<SegmentHit>> {
    query_inputs(doc, targets, p, tolerance)?;
    if node_under(doc, targets, p, tolerance)?.is_some() {
        return Ok(None);
    }
    for (target, t) in targets.iter().enumerate() {
        if let Some((s, k, t, _, distance)) = photocraft_pathops::nearest(node::path(doc, t)?, p).map_err(|e| error(e.to_string()))?
            && distance <= tolerance
            && t > 0.0
            && t < 1.0
        {
            return Ok(Some(SegmentHit { target, knot: [s, k], t }));
        }
    }
    Ok(None)
}

#[derive(Clone, Debug)]
pub struct AnchorGesture {
    pub capture: Capture,
    pub target: usize,
    pub operation: node::Operation,
}
impl AnchorGesture {
    pub fn preview(&self, s: &Session) -> Result<Document> {
        self.capture.validate(s)?;
        let t = self.capture.targets.get(self.target).ok_or_else(|| error("missing captured target"))?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let (path, _) = node::evaluate(node::path(doc, t)?, &t.anchors, &self.operation)?;
        cache_budget(doc, t, Some(&path))?;
        Ok(node::prepare(doc, std::slice::from_ref(t), &self.operation, self.capture.convert_live)?.doc)
    }
    pub fn commit(self, s: &mut Session) -> Result<Vec<node::Target>> {
        self.capture.validate(s)?;
        let t = self.capture.targets.get(self.target).ok_or_else(|| error("missing captured target"))?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let (path, _) = node::evaluate(node::path(doc, t)?, &t.anchors, &self.operation)?;
        cache_budget(doc, t, Some(&path))?;
        let result=s.execute("path.node.edit", json!({"document":self.capture.document,"revision":self.capture.revision,"targets":[t],"operation":self.operation,"convertLive":self.capture.convert_live}))?;
        // The node command publishes provenance together with geometry; its response remaps
        // UI selection too, rather than leaving references to deleted/shifted knot indices.
        let remapped: Vec<node::Target> = serde_json::from_value(result["targets"].clone()).map_err(|e| error(e.to_string()))?;
        let mut targets = self.capture.targets;
        if let Some(target) = remapped.into_iter().next() {
            targets[self.target] = target;
        }
        Ok(targets)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurvaturePoint {
    pub at: [f64; 2],
    #[serde(default)]
    pub corner: bool,
    /// Old knot identity; newly placed points have none. Never infer identity from coordinates.
    #[serde(default)]
    pub origin: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Destination {
    NewShape { above: Option<LayerId> },
    Curve { target: node::Target, subpath: usize },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurvatureCommit {
    pub document: u64,
    pub revision: u64,
    pub destination: Destination,
    /// Complete selection captured at press. Older single-target requests may omit it.
    #[serde(default)]
    pub targets: Vec<node::Target>,
    pub points: Vec<CurvaturePoint>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub convert_live: bool,
    #[serde(default)]
    pub fill: Option<Fill>,
    #[serde(default)]
    pub stroke: Option<ShapeStroke>,
}
impl CurvatureCommit {
    pub fn path(&self) -> Result<Path> {
        curvature_fit::fit(&self.points.iter().map(|p| (Point::new(p.at[0], p.at[1]), p.corner)).collect::<Vec<_>>(), self.closed).map_err(error)
    }
    fn pen_request(&self, above: Option<LayerId>) -> Result<pen::Commit> {
        if !self.targets.is_empty() || self.points.iter().any(|p| p.origin.is_some()) {
            return Err(error("new Curvature points cannot refer to old knots"));
        }
        Ok(pen::Commit {
            document: self.document,
            revision: self.revision,
            target: pen::Target::NewShape { above },
            path: self.path()?,
            fill: self.fill.clone(),
            stroke: self.stroke.clone(),
        })
    }
}
fn check_new_cache(s: &Session, c: &CurvatureCommit, above: Option<LayerId>) -> Result<pen::Commit> {
    pen::check_fresh(s, c.document, c.revision)?;
    let request = c.pen_request(above)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    pen::check_target(doc, &request.target)?;
    let shape = photocraft_doc::ShapeLayer { path: request.path.clone(), fill: request.fill.clone(), stroke: request.stroke.clone(), ..Default::default() };
    let compiled = photocraft_vector::appearance::CompiledAppearance::new(&shape, None, &doc.patterns, doc.bounds()).map_err(error)?;
    if compiled.cache_bytes(doc.pixel_format()) > 512 << 20 {
        return Err(error("Curvature cache exceeds 512 MiB"));
    }
    Ok(request)
}

static RENDERER: OnceLock<node::RenderCache> = OnceLock::new();
/// 00 installs the same combined 06/08/11/17 projection used for node edits.
pub fn install_renderer(render: node::RenderCache) -> std::result::Result<(), node::RenderCache> {
    RENDERER.set(render)
}
fn render(doc: &Document, sh: &photocraft_doc::ShapeLayer, vector: &photocraft_doc::VectorData) -> Result<photocraft_doc::Surface> {
    if let Some(renderer) = RENDERER.get() {
        return renderer(doc, sh, vector);
    }
    let projected = super::corners::project_shape(sh, &vector.corners)?;
    let compiled =
        photocraft_vector::appearance::CompiledAppearance::new(&projected, vector.appearance.as_ref(), &doc.patterns, doc.bounds()).map_err(error)?;
    if compiled.cache_bytes(doc.pixel_format()) > 512 << 20 {
        return Err(error("Curvature cache exceeds 512 MiB"));
    }
    compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)
}
/// One detached document, with path/corner changes and bounds-limited caches prepared together.
pub fn preview_curvature(s: &Session, c: &CurvatureCommit) -> Result<Document> {
    pen::check_fresh(s, c.document, c.revision)?;
    let fitted = c.path()?;
    let Destination::Curve { target, subpath } = &c.destination else {
        let Destination::NewShape { above } = c.destination else {
            return Err(error("missing Curvature destination"));
        };
        return pen::preview_document(s, &check_new_cache(s, c, above)?);
    };
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    if c.targets.is_empty() {
        validate_targets(doc, std::slice::from_ref(target), c.convert_live)?;
    } else {
        validate_targets(doc, &c.targets, c.convert_live)?;
        if !c.targets.contains(target) {
            return Err(error("Curvature destination is outside the captured selection"));
        }
    }
    let base = node::path(doc, target)?;
    let old = base.subpaths.get(*subpath).ok_or_else(|| error("missing Curvature subpath"))?;
    let mut seen = HashSet::new();
    for origin in c.points.iter().filter_map(|p| p.origin) {
        if origin >= old.knots.len() || !seen.insert(origin) {
            return Err(error("invalid Curvature knot identity"));
        }
    }
    let mut path = base.clone();
    let removed = fitted.subpaths.is_empty();
    if let Some(replacement) = fitted.subpaths.first() {
        path.subpaths[*subpath] = photocraft_doc::Subpath { op: old.op, ..replacement.clone() };
    } else {
        path.subpaths.remove(*subpath);
    }
    pen::validate(&path)?;
    cache_budget(doc, target, Some(&path))?;
    let mut shown = (**doc).clone();
    if target.name == "layer" {
        let id = LayerId(target.layer.ok_or_else(|| error("missing Curvature layer"))?);
        let layer = shown.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
        layer.vector.corners.retain_mut(|corner| {
            if removed && corner.subpath > *subpath {
                corner.subpath -= 1;
                return true;
            }
            if corner.subpath != *subpath {
                return true;
            }
            if let Some(k) = c.points.iter().position(|p| p.origin == Some(corner.knot)) {
                corner.knot = k;
                true
            } else {
                false
            }
        });
        layer.vector.live_shape = None;
        if let LayerContent::Shape(sh) = &mut layer.content {
            sh.path = path;
            sh.live = None;
            sh.psd_raw = None;
        } else {
            layer.vector_mask.as_mut().ok_or_else(|| error("missing Curvature mask"))?.path = path;
        }
        let l = shown.layer(id).ok_or(EngineError::NoLayer(id))?;
        let cache = if let LayerContent::Shape(sh) = &l.content { Some(render(&shown, sh, &l.vector)?) } else { None };
        if let Some(cache) = cache
            && let Some(l) = shown.layer_mut(id)
            && let LayerContent::Shape(sh) = &mut l.content
        {
            sh.cache = Some(cache);
        }
    } else {
        *crate::vector_cmds::path_mut(&mut shown, &target.name, None)? = path;
    }
    Ok(shown)
}
fn run_curvature(s: &mut Session, value: &Value) -> Result<Value> {
    let c: CurvatureCommit = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
    if let Destination::NewShape { above } = c.destination {
        // Reuse Pen's public command implementation without nesting Session::execute (which
        // would journal twice once 00 registers this command).
        let request = check_new_cache(s, &c, above)?;
        let spec = pen::specs().into_iter().next().ok_or_else(|| error("missing Pen command"))?;
        return (spec.run)(s, &serde_json::to_value(request).map_err(|e| error(e.to_string()))?);
    }
    let prepared = preview_curvature(s, &c)?;
    let targets = curvature_selection(&c);
    pen::check_fresh(s, c.document, c.revision)?;
    s.edit("Curvature", move |doc, _| {
        *doc = prepared;
        Ok(())
    })?;
    Ok(json!({"points":c.points.len(),"targets":targets}))
}
fn curvature_selection(c: &CurvatureCommit) -> Vec<node::Target> {
    let Destination::Curve { target, subpath } = &c.destination else {
        return Vec::new();
    };
    let mut remapped = target.clone();
    remapped.anchors = target
        .anchors
        .iter()
        .filter_map(|[s, k]| {
            if s == subpath {
                c.points.iter().position(|p| p.origin == Some(*k)).map(|i| [*s, i])
            } else if c.points.is_empty() && s > subpath {
                Some([s - 1, *k])
            } else {
                Some([*s, *k])
            }
        })
        .collect();
    if c.targets.is_empty() { vec![remapped] } else { c.targets.iter().map(|t| if t == target { remapped.clone() } else { t.clone() }).collect() }
}
/// New paths reuse the already registered Pen commit. Existing paths use this package's
/// journaled command (00's exact registration fragment); no manual journal writes.
pub fn commit_curvature(s: &mut Session, c: CurvatureCommit) -> Result<Vec<node::Target>> {
    pen::check_fresh(s, c.document, c.revision)?;
    match c.destination {
        Destination::NewShape { above } => {
            s.execute("path.penCommit", serde_json::to_value(check_new_cache(s, &c, above)?).map_err(|e| error(e.to_string()))?)?;
            Ok(Vec::new())
        }
        Destination::Curve { .. } => {
            let result = s.execute("path.anchor.curvatureCommit", serde_json::to_value(c).map_err(|e| error(e.to_string()))?)?;
            serde_json::from_value(result["targets"].clone()).map_err(|e| error(e.to_string()))
        }
    }
}
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "path.anchor.curvatureCommit",
        label: "Curvature",
        menu: &[],
        shortcut: None,
        params: "{document,revision,destination,targets?,points:[{at:[x,y],corner?,origin?}],closed?,convertLive?,fill?,stroke?}",
        enabled: crate::vector_cmds::has_doc,
        run: run_curvature,
        journal: true,
    }]
}

#[cfg(test)]
mod tests;
