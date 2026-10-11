//! Captured live blend transactions on the existing ordinary layer tree.
//! Geometry provider adapts VectorCraft doc/blend.rs; see evaluator's copyright notice.
//! Original engine transaction code: GPL-3.0-or-later Local Image.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::live::{BlendSpec, LiveGroup};
use photocraft_doc::{Document, Layer, LayerId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

// Exact provider source, without competing edits to 00's engine manifest.
// 00 may replace this bridge with `pub use photocraft_vecfx::blend as evaluator`.
pub use photocraft_pathops::{from_geometry as evaluator_from_geometry, to_geometry as evaluator_geometry};
pub use photocraft_vecfx::blend as evaluator;
pub fn evaluator_nearest(path: &photocraft_doc::Path, point: photocraft_geom::Point) -> Result<Option<(usize, usize, f64, photocraft_geom::Point, f64)>> {
    evaluator::validate_path(path).map_err(|e| bad(e.to_string()))?;
    photocraft_pathops::nearest(path, point).map_err(|e| bad(e.to_string()))
}
pub fn evaluator_flatten(path: &photocraft_doc::Path) -> Result<Vec<photocraft_pathops::Polyline>> {
    evaluator::validate_spec(&BlendSpec { spine: Some(path.clone()), ..Default::default() }).map_err(|e| bad(e.to_string()))?;
    Ok(photocraft_pathops::flatten_path(path, 0.2))
}
fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn preflight(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() || ids.len() > evaluator::MAX_KEYS {
        return Err(bad("blend requires 1 to 32 targets"));
    }
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(bad("duplicate blend target"));
        }
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        for (p, _, layer) in doc.walk() {
            if p.starts_with(&path) {
                let locks = doc.effective_locks(layer.id);
                if locks.all || locks.pixels || locks.position || locks.artboard {
                    return Err(bad("blend target or descendant is locked"));
                }
            }
        }
        if path.iter().enumerate().any(|(i, _)| i + 1 < path.len() && doc.layer_at(&path[..=i]).is_some_and(|l| ids.contains(&l.id))) {
            return Err(bad("select a blend group or its child, not both"));
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(session: &Session, layers: Vec<LayerId>) -> Result<Self> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        preflight(&st.doc, &layers)?;
        Ok(Self { document: st.doc.id.0, revision: st.revision, layers })
    }
    pub fn check(&self, session: &Session) -> Result<()> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        if self.document != st.doc.id.0 || self.revision != st.revision {
            return Err(bad("blend gesture is stale; start again"));
        }
        preflight(&st.doc, &self.layers)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Create { spec: BlendSpec },
    Options { spec: BlendSpec },
    Expand,
    Release,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub operation: Operation,
}
fn siblings<'a>(doc: &'a mut Document, parent: &[usize]) -> Result<&'a mut Vec<Layer>> {
    if parent.is_empty() {
        return Ok(&mut doc.layers);
    }
    doc.layer_at_mut(parent).and_then(Layer::children_mut).ok_or_else(|| bad("blend parent is not a group"))
}
pub fn spec(session: &Session, id: LayerId) -> Result<BlendSpec> {
    let layer = session.active().ok_or(EngineError::NoDocument)?.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    match &layer.vector.live {
        Some(LiveGroup::Blend(spec)) if layer.is_group() => Ok(spec.clone()),
        _ => Err(bad("target is not a live blend")),
    }
}
pub fn spine(session: &Session, id: LayerId) -> Result<photocraft_doc::Path> {
    let spec = spec(session, id)?;
    evaluator::validate_spec(&spec).map_err(|e| bad(e.to_string()))?;
    if let Some(path) = spec.spine {
        return Ok(path);
    }
    let layer = session.active().ok_or(EngineError::NoDocument)?.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    evaluator::default_spine(layer.children().ok_or_else(|| bad("blend requires group keys"))?).map_err(|e| bad(e.to_string()))
}
fn detached(session: &Session, request: &Request) -> Result<(Document, LayerId)> {
    request.capture.check(session)?;
    let source = &session.active().ok_or(EngineError::NoDocument)?.doc;
    let mut doc = (**source).clone();
    match &request.operation {
        Operation::Create { spec } => {
            let ids = &request.capture.layers;
            if ids.len() < 2 {
                return Err(bad("create blend requires at least two keys"));
            }
            let paths = ids.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
            let first = &paths[0];
            let parent = &first[..first.len() - 1];
            if paths.iter().any(|p| &p[..p.len() - 1] != parent) {
                return Err(bad("blend keys must be siblings"));
            }
            if (1..=parent.len()).any(|n| doc.layer_at(&parent[..n]).is_some_and(|l| l.vector.live.is_some())) {
                return Err(bad("cannot create a nested blend in a live group"));
            }
            let mut indexes = paths.iter().map(|p| p[p.len() - 1]).collect::<Vec<_>>();
            indexes.sort_unstable();
            if indexes.windows(2).any(|w| w[1] != w[0] + 1) {
                return Err(bad("blend keys must be adjacent in paint order"));
            }
            let keys = indexes.iter().map(|i| siblings(&mut doc, parent).map(|s| s[*i].clone())).collect::<Result<Vec<_>>>()?;
            if keys.iter().any(|k| k.clipped) {
                return Err(bad("clipped keys must be blended inside their ordinary group"));
            }
            evaluator::evaluate(&keys, spec).map_err(|e| bad(e.to_string()))?;
            let max = evaluator::max_document_id(&doc);
            let id = LayerId(max.checked_add(1).ok_or_else(|| bad("blend ID overflow"))?);
            let mut group = Layer::group("Blend", keys);
            group.id = id;
            group.vector.live = Some(LiveGroup::Blend(spec.clone()));
            siblings(&mut doc, parent)?.splice(indexes[0]..=indexes[indexes.len() - 1], [group]);
            Ok((doc, id))
        }
        Operation::Options { spec } => {
            let [id] = request.capture.layers.as_slice() else { return Err(bad("blend options require one live group")) };
            self::spec(session, *id)?;
            let layer = doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?;
            evaluator::evaluate(layer.children().ok_or_else(|| bad("blend keys missing"))?, spec).map_err(|e| bad(e.to_string()))?;
            layer.vector.live = Some(LiveGroup::Blend(spec.clone()));
            Ok((doc, *id))
        }
        Operation::Expand => {
            let [id] = request.capture.layers.as_slice() else { return Err(bad("expand requires one live blend")) };
            self::spec(session, *id)?;
            let projected = evaluator::project_document(&doc).map_err(|e| bad(e.to_string()))?;
            *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = projected.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
            Ok((doc, *id))
        }
        Operation::Release => {
            let [id] = request.capture.layers.as_slice() else { return Err(bad("release requires one live blend")) };
            self::spec(session, *id)?;
            doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?.vector.live = None;
            Ok((doc, *id))
        }
    }
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub model: Document,
    pub display: Document,
    pub target: LayerId,
}
pub fn preview(session: &Session, request: &Request) -> Result<Preview> {
    let (model, target) = detached(session, request)?;
    let display = evaluator::project_document(&model).map_err(|e| bad(e.to_string()))?;
    Ok(Preview { model, display, target })
}
pub fn commit(session: &mut Session, request: &Request) -> Result<LayerId> {
    let mut prepared = preview(session, request)?;
    request.capture.check(session)?;
    let max = evaluator::max_document_id(&prepared.model);
    evaluator::prepare_caches(&mut prepared.model).map_err(|e| bad(e.to_string()))?;
    let id = prepared.target;
    session.edit("Live Blend", move |doc, active| {
        doc.layers = prepared.model.layers;
        *active = Some(id);
        photocraft_doc::ensure_ids_above(max);
        Ok(())
    })?;
    crate::layer_multi_cmds::set_selection(session, vec![id], Some(id), Some(id))?;
    Ok(id)
}
fn request(p: &Value, operation: Operation) -> Result<Request> {
    let capture = serde_json::from_value(p.get("capture").cloned().ok_or_else(|| bad("blend command requires captured document/revision/targets"))?)
        .map_err(|e| bad(e.to_string()))?;
    Ok(Request { capture, operation })
}
fn options(p: &Value) -> Result<BlendSpec> {
    serde_json::from_value(p.get("spec").cloned().ok_or_else(|| bad("blend spec required"))?).map_err(|e| bad(e.to_string()))
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let r = request(p, Operation::Create { spec: options(p)? })?;
    Ok(json!({"layer":commit(s, &r)?.0}))
}
fn update(s: &mut Session, p: &Value) -> Result<Value> {
    let r = request(p, Operation::Options { spec: options(p)? })?;
    Ok(json!({"layer":commit(s, &r)?.0}))
}
fn expand(s: &mut Session, p: &Value) -> Result<Value> {
    let r = request(p, Operation::Expand)?;
    Ok(json!({"layer":commit(s, &r)?.0}))
}
fn release(s: &mut Session, p: &Value) -> Result<Value> {
    let r = request(p, Operation::Release)?;
    Ok(json!({"layer":commit(s, &r)?.0}))
}
fn info(s: &mut Session, p: &Value) -> Result<Value> {
    let c: Capture = serde_json::from_value(p.get("capture").cloned().ok_or_else(|| bad("capture required"))?).map_err(|e| bad(e.to_string()))?;
    c.check(s)?;
    let [id] = c.layers.as_slice() else { return Err(bad("info requires one blend")) };
    let spec = spec(s, *id)?;
    let keys = s.active().ok_or(EngineError::NoDocument)?.doc.layer(*id).and_then(Layer::children).ok_or_else(|| bad("blend keys missing"))?;
    Ok(json!({"layer":id.0,"spec":spec,"keys":keys.iter().map(|l| l.id).collect::<Vec<_>>(),"spine":spine(s,*id)?}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("vector.blend.create", "Create Blend", true, create as fn(&mut Session, &Value) -> Result<Value>),
        ("vector.blend.options", "Blend Options", true, update),
        ("vector.blend.expand", "Expand Blend", true, expand),
        ("vector.blend.release", "Release Blend", true, release),
        ("vector.blend.info", "Blend Info", false, info),
    ]
    .into_iter()
    .map(|(id, label, journal, run)| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params: "{capture:{document,revision,layers},spec?:BlendSpec}",
        enabled: crate::vector_cmds::has_doc,
        journal,
        run,
    })
    .collect()
}
#[cfg(test)]
#[path = "blend/gpu.rs"]
mod gpu;
#[cfg(test)]
#[path = "blend/tests.rs"]
mod tests;
