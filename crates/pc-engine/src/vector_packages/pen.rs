//! Atomic Pen commit and detached preview boundary.
//! Gesture behavior adapted from VectorCraft `crates/tools/src/pen.rs` at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Licensed under MIT OR Apache-2.0; see licenses/vectorcraft-*.

use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, ShapeStroke};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod gesture;
pub use gesture::{Gesture, Mode, Modifiers, Outcome, constrain};

pub const MAX_COORDINATE: f64 = 1_000_000.0;
pub const MAX_KNOTS: usize = 16_384;

/// Captured destination. `above` is the insertion anchor, never a live selection lookup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Target {
    NewShape { above: Option<LayerId> },
    Shape { layer: LayerId },
    Work,
    VectorMask { layer: LayerId },
}

/// Native serde Path/paint schema, with mandatory freshness guards for automation too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub document: u64,
    pub revision: u64,
    pub target: Target,
    pub path: Path,
    #[serde(default)]
    pub fill: Option<Fill>,
    #[serde(default)]
    pub stroke: Option<ShapeStroke>,
}

fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}

pub fn validate(path: &Path) -> Result<()> {
    vector_api::validate_path(path)?;
    let count: usize = path.subpaths.iter().map(|s| s.knots.len()).sum();
    if count > MAX_KNOTS || path.subpaths.len() > MAX_KNOTS {
        return Err(error("pen path exceeds the node budget"));
    }
    if path
        .subpaths
        .iter()
        .flat_map(|s| &s.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err(error("pen coordinates exceed the document coordinate limit"));
    }
    Ok(())
}

pub fn check_fresh(s: &Session, document: u64, revision: u64) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != document || st.revision != revision {
        return Err(error("pen gesture is stale; start again"));
    }
    Ok(())
}

/// Preflight is shared by Begin, Preview and Commit and precedes cache preparation.
pub fn check_target(doc: &Document, target: &Target) -> Result<()> {
    match target {
        Target::Shape { layer } => {
            let edits = vector_api::snapshot_shapes(doc, &[*layer])?;
            if edits[0].vector.appearance.is_some() {
                return Err(error("pen edits on appearance stacks require the appearance renderer"));
            }
        }
        Target::VectorMask { layer } => {
            let l = doc.layer(*layer).ok_or(EngineError::NoLayer(*layer))?;
            let locks = doc.effective_locks(*layer);
            if locks.all || locks.pixels || l.vector_mask.is_none() {
                return Err(error("pen requires an unlocked vector mask"));
            }
        }
        Target::NewShape { above: Some(id) } => {
            doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            // A locked reference layer is fine; its containing group must be editable.
            let mut path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
            path.pop();
            if !path.is_empty() && doc.layer_at(&path).is_some_and(|p| doc.effective_locks(p.id).all) {
                return Err(error("pen destination group is locked"));
            }
        }
        Target::NewShape { above: None } | Target::Work => {}
    }
    Ok(())
}

fn bounded_numbers(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_f64().is_some_and(|n| n.is_finite() && n.abs() <= MAX_COORDINATE),
        Value::Array(a) => a.iter().all(bounded_numbers),
        Value::Object(o) => o.values().all(bounded_numbers),
        _ => true,
    }
}

pub fn check_paint(c: &Commit) -> Result<()> {
    let paint = serde_json::to_value((&c.fill, &c.stroke)).map_err(|e| error(&e.to_string()))?;
    // serde_json turns nonfinite floats into null; null is valid only for the two
    // top-level optional paints, never inside a paint record.
    fn no_nulls(v: &Value) -> bool {
        match v {
            Value::Null => false,
            Value::Array(a) => a.iter().all(no_nulls),
            Value::Object(o) => o.values().all(no_nulls),
            _ => true,
        }
    }
    if paint.as_array().is_some_and(|a| a.iter().filter(|p| !p.is_null()).any(|p| !no_nulls(p))) {
        return Err(error("pen paint parameters must be finite"));
    }
    if !bounded_numbers(&paint)
        || c.stroke.as_ref().is_some_and(|s| {
            s.width < 0.0
                || s.width > 5000.0
                || !(0.0..=1.0).contains(&s.opacity)
                || s.miter_limit < 1.0
                || s.dashes.len() > 1024
                || s.dashes.iter().any(|d| *d < 0.0)
        })
    {
        return Err(error("pen paint parameters must be finite and bounded"));
    }
    if c.fill.iter().chain(c.stroke.iter().map(|s| &s.paint)).any(|f| matches!(f, Fill::Pattern { .. })) {
        return Err(error("pen pattern paint requires the appearance renderer"));
    }
    Ok(())
}

fn prepare(s: &Session, c: &Commit) -> Result<(Document, Option<LayerId>)> {
    check_fresh(s, c.document, c.revision)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    check_target(doc, &c.target)?;
    validate(&c.path)?;
    check_paint(c)?;
    let mut preview = (**doc).clone();
    let id = apply(&mut preview, c)?;
    Ok((preview, id))
}

/// Prepare a detached document with bounds-limited pc-vector caches. Nothing is published.
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    prepare(s, c).map(|p| p.0)
}

/// Refresh only the gesture's detached target. Clone the document once at begin;
/// subsequent pointer changes reuse it and copy only the original edited layer.
/// `layer` is the preview insertion ID returned by the first update.
pub fn update_preview(s: &Session, c: &Commit, preview: &mut Option<Document>, layer: &mut Option<LayerId>) -> Result<()> {
    check_fresh(s, c.document, c.revision)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    check_target(source, &c.target)?;
    validate(&c.path)?;
    check_paint(c)?;
    if preview.is_none() {
        let mut doc = (**source).clone();
        *layer = apply(&mut doc, c)?;
        *preview = Some(doc);
        return Ok(());
    }
    let doc = preview.as_mut().ok_or_else(|| error("missing pen preview"))?;
    if doc.id != source.id {
        return Err(error("stale pen preview"));
    }
    match c.target {
        Target::NewShape { .. } => {
            let id = layer.ok_or_else(|| error("missing pen preview layer"))?;
            let mut shape = ShapeLayer { path: c.path.clone(), fill: c.fill.clone(), stroke: c.stroke.clone(), ..Default::default() };
            crate::vector_cmds::refresh_shape(doc, &mut shape);
            doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.content = LayerContent::Shape(shape);
        }
        Target::Shape { layer: id } | Target::VectorMask { layer: id } => {
            let original = source.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
            *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = original;
            apply(doc, c)?;
        }
        Target::Work => {
            doc.work_path = Some(c.path.clone());
        }
    }
    Ok(())
}

fn apply(doc: &mut Document, c: &Commit) -> Result<Option<LayerId>> {
    match c.target {
        Target::NewShape { above } => {
            let mut shape = ShapeLayer { path: c.path.clone(), fill: c.fill.clone(), stroke: c.stroke.clone(), ..Default::default() };
            crate::vector_cmds::refresh_shape(doc, &mut shape);
            Ok(Some(doc.insert_above(above, Layer::new(doc.next_layer_name("Curve"), LayerContent::Shape(shape)))))
        }
        Target::Shape { layer } => {
            let mut edit = vector_api::snapshot_shapes(doc, &[layer])?.into_iter().next().ok_or_else(|| error("pen shape disappeared"))?;
            // Preserve corners for unchanged knots and remap indices when prepending.
            let offsets: Vec<_> = edit
                .shape
                .path
                .subpaths
                .iter()
                .enumerate()
                .map(|(si, old)| {
                    c.path.subpaths.get(si).map_or(0, |new| {
                        // Closing drags may change an endpoint's handles after
                        // prepending. Its retained anchors still identify the offset.
                        let same_anchors = |offset| {
                            new.knots.get(offset..offset + old.knots.len()).is_some_and(|knots| knots.iter().zip(&old.knots).all(|(a, b)| a.anchor == b.anchor))
                        };
                        let offset = new.knots.len().saturating_sub(old.knots.len());
                        if !same_anchors(0) && same_anchors(offset) { offset } else { 0 }
                    })
                })
                .collect();
            edit.vector.corners.retain_mut(|corner| {
                let Some(old) = edit.shape.path.subpaths.get(corner.subpath) else { return false };
                let Some(new) = c.path.subpaths.get(corner.subpath) else { return false };
                // Preserve identity by index/contiguous old sequence. Equal duplicate
                // knots are not interchangeable: a coordinate search loses their metadata.
                let Some(k) = old.knots.get(corner.knot) else { return false };
                let i = corner.knot + offsets[corner.subpath];
                if new.knots.get(i) != Some(k) {
                    return false;
                }
                corner.knot = i;
                true
            });
            edit.shape.path = c.path.clone();
            edit.shape.live = None;
            edit.shape.psd_raw = None;
            edit.vector.live_shape = None;
            crate::vector_cmds::refresh_shape(doc, &mut edit.shape);
            let l = doc.layer_mut(layer).ok_or(EngineError::NoLayer(layer))?;
            l.content = LayerContent::Shape(edit.shape);
            l.vector = edit.vector;
            Ok(Some(layer))
        }
        Target::Work => {
            doc.work_path = Some(c.path.clone());
            Ok(None)
        }
        Target::VectorMask { layer } => {
            let l = doc.layer_mut(layer).ok_or(EngineError::NoLayer(layer))?;
            let mask = l.vector_mask.as_mut().ok_or_else(|| error("vector mask disappeared"))?;
            mask.path = c.path.clone();
            Ok(Some(layer))
        }
    }
}

fn commit(s: &mut Session, p: &Value) -> Result<Value> {
    let c: Commit = serde_json::from_value(p.clone()).map_err(|e| crate::vector_cmds::bad("path.penCommit", e.to_string()))?;
    if c.path.subpaths.iter().all(|p| p.knots.len() < 2) {
        return Err(error("a finished pen curve needs at least two nodes"));
    }
    // Caches and all validation finish before the sole edit/history publication.
    let (prepared, id) = prepare(s, &c)?;
    check_fresh(s, c.document, c.revision)?;
    check_target(&s.active().ok_or(EngineError::NoDocument)?.doc, &c.target)?;
    s.edit("Pen", move |doc, active| {
        *doc = prepared;
        if let Some(id) = id {
            *active = Some(id);
        }
        Ok(())
    })?;
    Ok(json!({"layer": id.map(|id| id.0)}))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "path.penCommit",
        label: "Pen",
        menu: &[],
        shortcut: None,
        params: "{document:u64,revision:u64,target:{kind:newShape|shape|work|vectorMask,above?:id,layer?:id},path:serde Path,fill?:serde Fill,stroke?:serde ShapeStroke}",
        enabled: crate::vector_cmds::has_doc,
        run: commit,
        journal: true,
    }]
}

#[cfg(test)]
#[path = "pen/tests.rs"]
mod tests;
