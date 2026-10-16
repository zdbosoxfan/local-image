//! Shared transaction boundary for vector packages. Preview operates on detached shape edits;
//! commit validates all targets before publishing one Session::edit/history state. Commands
//! using this helper must be registered with journal=true; Session::execute owns journaling.
//! New appearance/live geometry renderers supply their prepared cache explicitly. The legacy
//! path-only command below uses pc-vector. Preview must never call Session::edit.

use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{Document, LayerContent, LayerId, ShapeLayer, VectorData};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Fully prepared shape and metadata; coordinates/cache are in document pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeEdit {
    pub layer: LayerId,
    pub shape: ShapeLayer,
    pub vector: VectorData,
}

/// Snapshot selected shapes for a tool's transient Begin/Preview state. Duplicates, missing
/// layers, non-shapes and inherited pixel/all locks are errors, before any expensive rendering.
fn shape_targets<'a>(doc: &'a Document, ids: &[LayerId]) -> Result<Vec<&'a photocraft_doc::Layer>> {
    if ids.is_empty() {
        return Err(EngineError::Other("select at least one shape".into()));
    }
    let mut seen = HashSet::new();
    ids.iter()
        .map(|id| {
            if !seen.insert(*id) {
                return Err(EngineError::Other("duplicate shape target".into()));
            }
            let locks = doc.effective_locks(*id);
            if locks.all || locks.pixels {
                return Err(EngineError::Other("shape target is locked".into()));
            }
            let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
            if !matches!(layer.content, LayerContent::Shape(_)) {
                return Err(EngineError::Other("vector edit requires shape layers".into()));
            }
            Ok(layer)
        })
        .collect()
}
pub fn snapshot_shapes(doc: &Document, ids: &[LayerId]) -> Result<Vec<ShapeEdit>> {
    shape_targets(doc, ids)?
        .into_iter()
        .map(|l| {
            let LayerContent::Shape(shape) = &l.content else {
                return Err(EngineError::Other("vector edit requires shape layers".into()));
            };
            Ok(ShapeEdit { layer: l.id, shape: shape.clone(), vector: l.vector.clone() })
        })
        .collect()
}

/// Publish prepared edits atomically. The renderer is package-owned; it must build a cache
/// before this call. None is rejected rather than committing invisible editable geometry.
/// Tool code must also compare its captured document revision before publishing stale previews.
pub fn apply_shape_edits(s: &mut Session, label: &str, edits: Vec<ShapeEdit>) -> Result<()> {
    let ids: Vec<_> = edits.iter().map(|e| e.layer).collect();
    shape_targets(&s.active().ok_or(EngineError::NoDocument)?.doc, &ids)?;
    for edit in &edits {
        validate_path(&edit.shape.path)?;
        if edit.shape.cache.is_none() {
            return Err(EngineError::Other("vector edit requires a prepared render cache".into()));
        }
    }
    s.edit(label, move |doc, _| {
        for edit in edits {
            let layer = doc.layer_mut(edit.layer).ok_or(EngineError::NoLayer(edit.layer))?;
            layer.content = LayerContent::Shape(edit.shape);
            layer.vector = edit.vector;
        }
        Ok(())
    })
}

pub fn validate_path(path: &photocraft_doc::Path) -> Result<()> {
    if path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]).any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(EngineError::Other("vector coordinates must be finite".into()));
    }
    Ok(())
}

fn apply_paths(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("edits").and_then(Value::as_array).ok_or_else(|| crate::vector_cmds::bad("shape.applyPaths", "edits must be an array"))?;
    let ids: Vec<_> = raw
        .iter()
        .map(|e| e.get("layer").and_then(Value::as_u64).map(LayerId).ok_or_else(|| crate::vector_cmds::bad("shape.applyPaths", "each edit needs a layer id")))
        .collect::<Result<_>>()?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut edits = snapshot_shapes(doc, &ids)?;
    for (edit, raw) in edits.iter_mut().zip(raw) {
        edit.shape.path =
            serde_json::from_value(raw.get("path").cloned().ok_or_else(|| crate::vector_cmds::bad("shape.applyPaths", "each edit needs a serde Path"))?)
                .map_err(|e| crate::vector_cmds::bad("shape.applyPaths", e.to_string()))?;
        validate_path(&edit.shape.path)?;
        // A geometry replacement clears parametric metadata tied to the old knot indices.
        // Paint, constraints and clip stay with their layer.
        edit.shape.live = None;
        edit.shape.psd_raw = None;
        edit.vector.corners.clear();
        edit.vector.live_shape = None;
        if edit.vector.appearance.is_some() {
            return Err(EngineError::Other("appearance path edits require the appearance renderer".into()));
        }
        crate::vector_cmds::refresh_shape(doc, &mut edit.shape);
    }
    apply_shape_edits(s, "Edit Vector Paths", edits)?;
    Ok(json!({ "layers": ids.iter().map(|id| id.0).collect::<Vec<_>>() }))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "shape.applyPaths",
        label: "Edit Vector Paths",
        menu: &[],
        shortcut: None,
        params: "{edits:[{layer:id,path:serde Path},…]}",
        enabled: crate::vector_cmds::has_doc,
        run: apply_paths,
        journal: true,
    }]
}

#[cfg(test)]
mod tests;
