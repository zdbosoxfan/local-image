//! Package 02: detached live geometry and atomic, journaled shape commands.
//! Generator inclusion is the package-owned bootstrap until 00 adds pc-vector::design_shapes.
//! Replace the path module with `pub use photocraft_vector::design_shapes as geometry` at integration.
pub use photocraft_vector::design_shapes as geometry;

use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use photocraft_doc::vector_design::drawing::DesignShape;
use photocraft_doc::{Color, DocId, Document, Fill, Layer, LayerContent, LayerId, ShapeLayer};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
}
impl Capture {
    pub fn new(s: &Session) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        Ok(Self { document: st.doc.id, revision: st.revision })
    }
    pub fn check(self, s: &Session) -> Result<()> {
        let now = Self::new(s)?;
        if self == now { Ok(()) } else { Err(EngineError::Other("live shape gesture is stale".into())) }
    }
}
fn geom_error(e: geometry::ShapeError) -> EngineError {
    EngineError::Other(e.to_string())
}
fn design(p: &Value) -> Result<DesignShape> {
    serde_json::from_value(p.get("design").cloned().ok_or_else(|| EngineError::Other("design shape required".into()))?)
        .map_err(|e| EngineError::Other(e.to_string()))
}
fn capture(p: &Value) -> Result<Capture> {
    let get = |name| p.get(name).and_then(Value::as_u64).ok_or_else(|| EngineError::Other(format!("{name} capture required")));
    Ok(Capture { document: DocId(get("document")?), revision: get("revision")? })
}
fn ids(p: &Value) -> Result<Vec<LayerId>> {
    serde_json::from_value::<Vec<u64>>(p.get("layers").cloned().ok_or_else(|| EngineError::Other("layers required".into()))?)
        .map(|v| v.into_iter().map(LayerId).collect())
        .map_err(|e| EngineError::Other(e.to_string()))
}

/// Fully prepares base geometry and a bounds-limited cache without touching Session/history.
pub fn prepare(doc: &Document, design: &DesignShape, mut shape: ShapeLayer) -> Result<ShapeLayer> {
    shape.path = geometry::generate(design).map_err(geom_error)?;
    shape.live = None;
    shape.psd_raw = None;
    crate::vector_cmds::refresh_shape(doc, &mut shape);
    Ok(shape)
}
fn default_shape(design: &DesignShape) -> ShapeLayer {
    if geometry::stroke_only(&design.parameters) {
        ShapeLayer { stroke: Some(Default::default()), ..Default::default() }
    } else {
        ShapeLayer { fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() }
    }
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    capture(p)?.check(s)?;
    let design = design(p)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let above = match p.get("above") {
        None | Some(Value::Null) => None,
        Some(value) => Some(LayerId(value.as_u64().ok_or_else(|| EngineError::Other("above layer ID required".into()))?)),
    };
    if let Some(id) = above {
        let locks = doc.effective_locks(id);
        if locks.all || locks.pixels {
            return Err(EngineError::Other("creation target is locked".into()));
        }
        if doc.layer(id).is_none() {
            return Err(EngineError::NoLayer(id));
        }
    }
    let mut base = default_shape(&design);
    if let Some(fill) = p.get("fill") {
        base.fill = serde_json::from_value(fill.clone()).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    if let Some(stroke) = p.get("stroke") {
        base.stroke = serde_json::from_value(stroke.clone()).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    super::pen::check_paint(&super::pen::Commit {
        document: doc.id.0,
        revision: capture(p)?.revision,
        target: super::pen::Target::Work,
        path: Default::default(),
        fill: base.fill.clone(),
        stroke: base.stroke.clone(),
    })?;
    let shape = prepare(doc, &design, base)?;
    let mut layer = Layer::new("Live Shape", LayerContent::Shape(shape));
    layer.vector.live_shape = Some(design);
    let id = layer.id;
    crate::vector_integration::insert_layer(s, above, layer, "Create Live Shape")?;
    Ok(json!({"layer":id.0}))
}
fn set(s: &mut Session, p: &Value) -> Result<Value> {
    capture(p)?.check(s)?;
    let raw = p.get("edits").and_then(Value::as_array).ok_or_else(|| EngineError::Other("edits required".into()))?;
    let ids: Vec<_> = raw
        .iter()
        .map(|v| v.get("layer").and_then(Value::as_u64).map(LayerId).ok_or_else(|| EngineError::Other("layer required".into())))
        .collect::<Result<_>>()?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut edits = vector_api::snapshot_shapes(doc, &ids)?;
    // Preflight the entire target set and parameter payload before rendering any cache.
    let designs: Vec<_> = raw.iter().map(design).collect::<Result<_>>()?;
    for (edit, d) in edits.iter().zip(&designs) {
        geometry::generate(d).map_err(geom_error)?;
        if edit.vector.appearance.is_some() || !edit.vector.corners.is_empty() {
            return Err(EngineError::Other("live shape update requires appearance/corner projection".into()));
        }
    }
    for (edit, d) in edits.iter_mut().zip(designs) {
        edit.shape = prepare(doc, &d, edit.shape.clone())?;
        edit.vector.live_shape = Some(d);
    }
    vector_api::apply_shape_edits(s, "Edit Live Shapes", edits)?;
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    capture(p)?.check(s)?;
    let ids = ids(p)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut edits = vector_api::snapshot_shapes(doc, &ids)?;
    for edit in &mut edits {
        edit.vector.live_shape = None;
        edit.shape.live = None;
        edit.shape.psd_raw = None;
        // Geometry is already authoritative. Preserve its paint/cache/corner metadata exactly.
        if edit.shape.cache.is_none() {
            crate::vector_cmds::refresh_shape(doc, &mut edit.shape);
        }
    }
    vector_api::apply_shape_edits(s, "Convert to Curves", edits)?;
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    capture(p)?.check(s)?;
    let d = design(p)?;
    let path = geometry::generate(&d).map_err(geom_error)?;
    Ok(json!({"path":path,"design":d}))
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("shape.design.create", "Create Live Shape", true, create as fn(&mut Session, &Value) -> Result<Value>),
        ("shape.design.set", "Edit Live Shapes", true, set),
        ("shape.design.convert", "Convert to Curves", true, convert),
        ("shape.design.preview", "Preview Live Shape", false, preview),
    ]
    .into_iter()
    .map(|(id, label, journal, run)| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params: if id == "shape.design.create" {
            "{document:id,revision:n,design:DesignShape,above?:id,fill?:Fill|null,stroke?:ShapeStroke|null}"
        } else {
            "{document:id,revision:n,design:DesignShape | edits:[{layer,design}] | layers:[id]}"
        },
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
#[cfg(test)]
#[path = "drawing/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "../../../pc-vector/tests/design_shapes/geometry.rs"]
mod shape_tests;
#[cfg(test)]
#[path = "drawing/tests.rs"]
mod tests;
