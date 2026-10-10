//! Live-corner transaction boundary. Base paths remain authoritative; render/export consumers
//! use project_document/project_shape. Begin/preview do not publish document/history changes.

use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    vector_api::{self, ShapeEdit},
};
use photocraft_doc::vector_design::drawing::KnotCorner;
use photocraft_doc::{DocId, Document, Layer, LayerContent, LayerId, ShapeLayer};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};

// Package 00 owns pc-vector's registry. Compile the package-owned pure source here until that
// registration lands; this has no pc-vector -> pc-pathops dependency or duplicate evaluator.
#[path = "../../../pc-vector/src/live_corners.rs"]
pub mod geometry;

const MAX_CACHE_PIXELS: u64 = 64 * 1024 * 1024;
fn geometry_error(error: geometry::CornerError) -> EngineError {
    EngineError::Other(error.to_string())
}

/// Engine/display projection. Lower-level file adapters call the pure evaluator; metadata is untouched.
pub fn project_shape(shape: &ShapeLayer, corners: &[KnotCorner]) -> Result<ShapeLayer> {
    let mut projected = shape.clone();
    projected.path = geometry::evaluate(&shape.path, corners).map_err(geometry_error)?.path;
    // The path is the projection, not the parametric generator's base.
    projected.live = None;
    projected.psd_raw = None;
    Ok(projected)
}
fn cache(doc: &Document, shape: &ShapeLayer, corners: &[KnotCorner]) -> Result<photocraft_doc::Surface> {
    let projected = project_shape(shape, corners)?;
    let compiled = photocraft_vector::CompiledShape::new(&projected, photocraft_vector::DEFAULT_TOLERANCE, doc.bounds());
    let area = compiled.bounds().map_or(0, |b| b.intersect(&doc.bounds()).size().area());
    if area > MAX_CACHE_PIXELS {
        return Err(EngineError::Other("corner render cache exceeds the pixel budget".into()));
    }
    Ok(compiled.render(doc.pixel_format(), doc.bounds()))
}
fn project_layers(doc: &Document, layers: &mut [Layer]) -> Result<()> {
    for l in layers {
        match &mut l.content {
            LayerContent::Shape(shape) if !l.vector.corners.is_empty() => {
                if l.vector.appearance.is_some() {
                    return Err(EngineError::Other("corners with appearance require the appearance projection provider".into()));
                }
                let projected = project_shape(shape, &l.vector.corners)?;
                *shape = projected;
                shape.cache = Some(cache(doc, shape, &[])?);
            }
            LayerContent::Group(group) => project_layers(doc, &mut group.children)?,
            _ => {}
        }
    }
    Ok(())
}
/// Detached display document: transient projected paths let the normal CPU/GPU shape-split
/// compositor consume identical geometry. No Session::edit, persisted bake or undo step.
pub fn project_document(doc: &Document) -> Result<Document> {
    let mut out = doc.clone();
    project_layers(doc, &mut out.layers)?;
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct CornerGesture {
    pub document: DocId,
    pub revision: u64,
    base: Arc<Document>,
    display_base: OnceLock<Arc<Document>>,
    pub edits: Vec<ShapeEdit>,
}
impl CornerGesture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let edits = vector_api::snapshot_shapes(&st.doc, ids)?;
        for e in &edits {
            geometry::evaluate(&e.shape.path, &e.vector.corners).map_err(geometry_error)?;
            if e.vector.appearance.is_some() {
                return Err(EngineError::Other("corner edits require the appearance projection provider for appearance layers".into()));
            }
        }
        Ok(Self { document: st.doc.id, revision: st.revision, base: st.doc.clone(), display_base: OnceLock::new(), edits })
    }
    pub fn is_current(&self, s: &Session) -> bool {
        s.active().is_some_and(|st| st.doc.id == self.document && st.revision == self.revision)
    }
    /// Set full corner metadata for one captured layer. Every candidate validates before caches.
    pub fn set(&mut self, layer: LayerId, corners: Vec<KnotCorner>) -> Result<()> {
        let e = self.edits.iter_mut().find(|e| e.layer == layer).ok_or(EngineError::NoLayer(layer))?;
        let p = geometry::evaluate(&e.shape.path, &corners).map_err(geometry_error)?;
        e.vector.corners = p.corners;
        e.shape.cache = Some(cache(&self.base, &e.shape, &e.vector.corners)?);
        Ok(())
    }
    pub fn preview_document(&self) -> Result<Document> {
        if self.display_base.get().is_none() {
            let projected = Arc::new(project_document(&self.base)?);
            let _ = self.display_base.set(projected);
        }
        let base = self.display_base.get().ok_or_else(|| EngineError::Other("corner preview base is unavailable".into()))?;
        let mut doc = base.as_ref().clone();
        for e in &self.edits {
            let l = doc.layer_mut(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
            // Only captured targets change on a pointer frame. Their cache was prepared by set.
            l.content = LayerContent::Shape(project_shape(&e.shape, &e.vector.corners)?);
            l.vector = e.vector.clone();
        }
        Ok(doc)
    }
    /// One journaled command, revalidating captured identity/revision and the whole target set.
    pub fn commit(self, s: &mut Session) -> Result<Value> {
        s.execute("path.corner.set", self.params())
    }
    pub fn params(&self) -> Value {
        json!({"document": self.document.0, "revision": self.revision,
            "edits": self.edits.iter().map(|e| json!({"layer": e.layer.0, "corners": e.vector.corners})).collect::<Vec<_>>()})
    }
}
fn prepare(s: &Session, p: &Value) -> Result<CornerGesture> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if p.get("document").and_then(Value::as_u64) != Some(st.doc.id.0) || p.get("revision").and_then(Value::as_u64) != Some(st.revision) {
        return Err(EngineError::Other("stale corner edit: document or revision changed".into()));
    }
    let raw = p
        .get("edits")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 1024)
        .ok_or_else(|| EngineError::Other("corner edits must be a nonempty bounded array".into()))?;
    let ids: Vec<_> = raw
        .iter()
        .map(|e| e.get("layer").and_then(Value::as_u64).map(LayerId).ok_or_else(|| EngineError::Other("corner edit needs an integer layer id".into())))
        .collect::<Result<_>>()?;
    let mut gesture = CornerGesture::begin(s, &ids)?;
    // Decode and evaluate the ENTIRE set before rendering or changing any document.
    let metadata: Vec<Vec<KnotCorner>> = raw
        .iter()
        .map(|e| {
            let values = e
                .get("corners")
                .and_then(Value::as_array)
                .filter(|v| v.len() <= geometry::MAX_KNOTS)
                .ok_or_else(|| EngineError::Other("corner metadata must be a bounded array".into()))?;
            serde_json::from_value(Value::Array(values.clone())).map_err(|e| EngineError::Other(format!("invalid corner metadata: {e}")))
        })
        .collect::<Result<_>>()?;
    for (e, corners) in gesture.edits.iter_mut().zip(metadata) {
        e.vector.corners = geometry::evaluate(&e.shape.path, &corners).map_err(geometry_error)?.corners;
    }
    for e in &mut gesture.edits {
        e.shape.cache = Some(cache(&gesture.base, &e.shape, &e.vector.corners)?);
    }
    Ok(gesture)
}
fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let gesture = prepare(s, p)?;
    let result = json!({"layers": gesture.edits.iter().map(|e| e.layer.0).collect::<Vec<_>>(),
        "corners": gesture.edits.iter().map(|e| &e.vector.corners).collect::<Vec<_>>()});
    // All metadata stays with the unexpanded shape.path. The cache contains the projection.
    vector_api::apply_shape_edits(s, "Edit Corners", gesture.edits)?;
    Ok(result)
}
fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    let gesture = prepare(s, p)?;
    Ok(json!({"edits": gesture.edits.iter().map(|e| {
        let projection = geometry::evaluate(&e.shape.path, &e.vector.corners).map_err(geometry_error)?;
        Ok(json!({"layer": e.layer.0, "path": projection.path, "corners": projection.corners}))
    }).collect::<Result<Vec<_>>>()?}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [("path.corner.set", set as fn(&mut Session, &Value) -> Result<Value>, true), ("path.corner.preview", preview, false)]
        .into_iter()
        .map(|(id, run, journal)| CommandSpec {
            id,
            label: "Edit Corners",
            menu: &[],
            shortcut: None,
            params: "{document:id,revision:u64,edits:[{layer:id,corners:[{subpath,knot,corner:{radius,kind:Round|Inverted|Chamfer|Straight}}]}]}",
            enabled: crate::vector_cmds::has_doc,
            run,
            journal,
        })
        .collect()
}

#[cfg(test)]
#[path = "corners/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "corners/tests.rs"]
mod tests;
