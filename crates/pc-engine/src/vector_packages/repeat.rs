//! Live repeat transactions: detached evaluation, strict target/revision preflight and
//! a single undoable publication. Sources/parameters remain authoritative in native files.
use crate::{EngineError, Result, Session, commands::CommandSpec};
pub use evaluator::RepeatSpec;
use photocraft_doc::{DocId, Document, Layer, LayerContent, LayerId, vector_design::live::LiveGroup};
pub use photocraft_vecfx::repeat as evaluator;
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod render;
pub use render::project_document;
pub mod native;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
    pub targets: Vec<LayerId>,
}
impl Capture {
    pub fn new(s: &Session, targets: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        validate_targets(&st.doc, targets)?;
        Ok(Self { document: st.doc.id, revision: st.revision, targets: targets.to_vec() })
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if st.doc.id != self.document || st.revision != self.revision {
            return Err(bad("repeat gesture is stale"));
        }
        validate_targets(&st.doc, &self.targets)
    }
    pub fn params(&self) -> Value {
        json!({"document":self.document.0,"revision":self.revision,"layers":self.targets.iter().map(|id| id.0).collect::<Vec<_>>()})
    }
}
fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn validate_targets(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.is_empty() {
        return Err(bad("select at least one repeat source or group"));
    }
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(bad("duplicate repeat target"));
        }
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        if paths.iter().any(|p: &Vec<usize>| path.starts_with(p) || p.starts_with(&path)) {
            return Err(bad("repeat targets include ancestor and descendant"));
        }
        paths.push(path);
        fn subtree(doc: &Document, l: &Layer) -> Result<()> {
            let locks = doc.effective_locks(l.id);
            if locks.all || locks.pixels || locks.position {
                return Err(bad("repeat target or source is locked"));
            }
            if let Some(children) = l.children() {
                for child in children {
                    subtree(doc, child)?;
                }
            }
            Ok(())
        }
        subtree(doc, doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)?;
    }
    Ok(())
}
fn repeat_group(doc: &Document, id: LayerId) -> Result<&Layer> {
    let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    if l.children().is_none() || !matches!(l.vector.live, Some(LiveGroup::Repeat(_))) {
        return Err(bad("target must be a live repeat group"));
    }
    Ok(l)
}
fn read_capture(p: &Value) -> Result<Capture> {
    let number = |key| p.get(key).and_then(Value::as_u64).ok_or_else(|| bad(format!("{key} required")));
    let targets: Vec<u64> = serde_json::from_value(p.get("layers").cloned().ok_or_else(|| bad("layers required"))?).map_err(|e| bad(e.to_string()))?;
    Ok(Capture { document: DocId(number("document")?), revision: number("revision")?, targets: targets.into_iter().map(LayerId).collect() })
}
fn read_spec(p: &Value) -> Result<RepeatSpec> {
    let spec = serde_json::from_value(p.get("spec").cloned().ok_or_else(|| bad("repeat spec required"))?).map_err(|e| bad(e.to_string()))?;
    evaluator::instance_count(&spec).map_err(|e| bad(e.to_string()))?;
    Ok(spec)
}
fn publish(s: &mut Session, capture: &Capture, label: &str, document: Document, active: Option<LayerId>) -> Result<()> {
    capture.check(s)?;
    s.edit(label, move |doc, selected| {
        *doc = document;
        if let Some(id) = active {
            *selected = Some(id);
        }
        Ok(())
    })
}

/// Prepare parameter changes without touching revision, history, caches or the journal.
pub fn preview(s: &Session, capture: &Capture, spec: &RepeatSpec) -> Result<Document> {
    let detached = set_document(s, capture, spec)?;
    project_document(&detached)
}
fn set_document(s: &Session, capture: &Capture, spec: &RepeatSpec) -> Result<Document> {
    capture.check(s)?;
    evaluator::instance_count(spec).map_err(|e| bad(e.to_string()))?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    for id in &capture.targets {
        repeat_group(&doc, *id)?;
    }
    for id in &capture.targets {
        doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?.vector.live = Some(LiveGroup::Repeat(spec.clone()));
    }
    Ok(doc)
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let capture = read_capture(p)?;
    capture.check(s)?;
    let spec = read_spec(p)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let mut paths = capture.targets.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    paths.sort();
    let first = paths.first().ok_or_else(|| bad("empty repeat selection"))?;
    let parent = &first[..first.len() - 1];
    let start = first[first.len() - 1];
    for (i, path) in paths.iter().enumerate() {
        if path.len() != first.len() || &path[..path.len() - 1] != parent || path[path.len() - 1] != start + i {
            return Err(bad("repeat sources must be consecutive siblings"));
        }
    }
    let siblings = if parent.is_empty() {
        &mut doc.layers
    } else {
        doc.layer_at_mut(parent).and_then(Layer::children_mut).ok_or_else(|| bad("repeat parent must be a group"))?
    };
    let sources = siblings.drain(start..start + paths.len()).collect();
    let mut group = Layer::group("Repeat", sources);
    group.vector.live = Some(LiveGroup::Repeat(spec));
    let id = group.id;
    siblings.insert(start, group);
    // Complete render/budget validation before making the editable source tree visible.
    project_document(&doc)?;
    publish(s, &capture, "Create Repeat", doc, Some(id))?;
    Ok(json!({"layer":id.0}))
}
fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let capture = read_capture(p)?;
    let spec = read_spec(p)?;
    let doc = set_document(s, &capture, &spec)?;
    project_document(&doc)?;
    publish(s, &capture, "Edit Repeat", doc, None)?;
    Ok(json!({"instances":evaluator::instance_count(&spec).map_err(|e|bad(e.to_string()))?}))
}
fn expand(s: &mut Session, p: &Value) -> Result<Value> {
    let capture = read_capture(p)?;
    capture.check(s)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    for id in &capture.targets {
        repeat_group(&doc, *id)?;
    }
    let projected = project_document(&doc)?;
    for id in &capture.targets {
        let mut l = projected.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
        if let Some(children) = l.children_mut() {
            for c in children {
                *c = c.duplicate();
            }
        }
        *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l;
    }
    publish(s, &capture, "Expand Repeat", doc, None)?;
    Ok(json!({"layers":capture.targets.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("vector.repeat.create", "Create Repeat", create as fn(&mut Session, &Value) -> Result<Value>, "{document,revision,layers:[id],spec:RepeatSpec}"),
        ("vector.repeat.set", "Edit Repeat", set, "{document,revision,layers:[id],spec:RepeatSpec}"),
        ("vector.repeat.expand", "Expand Repeat", expand, "{document,revision,layers:[id]}"),
    ]
    .into_iter()
    .map(|(id, label, run, params)| CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: crate::vector_cmds::has_doc, run, journal: true })
    .collect()
}

#[cfg(test)]
mod tests;
