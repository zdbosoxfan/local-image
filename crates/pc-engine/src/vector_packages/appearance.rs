//! Captured, detached appearance previews and atomic multi-shape publication.
//! Uses vector_api's prepared-cache transaction boundary. Preview never edits Session.
use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    vector_api::{self, ShapeEdit},
};
use photocraft_doc::vector_design::appearance::Appearance;
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_vector::appearance::{self as renderer, CompiledAppearance, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub layer: LayerId,
    pub appearance: Appearance,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub document: u64,
    pub revision: u64,
    pub edits: Vec<Edit>,
}
fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}

pub fn check_fresh(s: &Session, c: &Commit) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != c.document || st.revision != c.revision {
        return Err(error("appearance gesture is stale; start again"));
    }
    Ok(())
}
/// Validate the entire set before any render; providers cannot publish a partial cache set.
pub fn prepare(s: &Session, c: &Commit, projection: &dyn StrokeProjection) -> Result<Vec<ShapeEdit>> {
    check_fresh(s, c)?;
    if c.edits.len() > 64 {
        return Err(error("appearance exceeds the target budget"));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids: Vec<_> = c.edits.iter().map(|e| e.layer).collect();
    let mut edits = vector_api::snapshot_shapes(&st.doc, &ids)?;
    for (e, request) in edits.iter().zip(&c.edits) {
        renderer::validate(&request.appearance).map_err(error)?;
        vector_api::validate_path(&e.shape.path)?;
    }
    let format = st.doc.pixel_format();
    let canvas = st.doc.bounds();
    let compiled = edits
        .iter()
        .zip(&c.edits)
        .map(|(e, request)| CompiledAppearance::with_projection(&e.shape, Some(&request.appearance), &st.doc.patterns, canvas, projection).map_err(error))
        .collect::<Result<Vec<_>>>()?;
    let bytes: u128 = compiled.iter().map(|p| p.cache_bytes(format)).sum();
    if bytes > 512 << 20 {
        return Err(error("appearance prepared caches exceed the 512 MiB budget"));
    }
    for ((e, request), compiled) in edits.iter_mut().zip(&c.edits).zip(compiled) {
        let cache = compiled.render(format, canvas).map_err(error)?;
        renderer::mirror_legacy(&mut e.shape, &request.appearance);
        e.shape.cache = Some(cache);
        e.shape.psd_raw = None;
        e.vector.appearance = Some(request.appearance.clone());
    }
    Ok(edits)
}
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    preview_with_projection(s, c, &super::styles::Projection)
}
pub fn preview_with_projection(s: &Session, c: &Commit, p: &dyn StrokeProjection) -> Result<Document> {
    let edits = prepare(s, c, p)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    for e in edits {
        let layer = doc.layer_mut(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        layer.content = LayerContent::Shape(e.shape);
        layer.vector = e.vector;
    }
    Ok(doc)
}

#[derive(Clone, Debug)]
pub struct Gesture {
    request: Commit,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if ids.len() > 64 {
            return Err(error("appearance exceeds the target budget"));
        }
        let shapes = vector_api::snapshot_shapes(&st.doc, ids)?;
        Ok(Self {
            request: Commit {
                document: st.doc.id.0,
                revision: st.revision,
                edits: shapes
                    .into_iter()
                    .map(|e| Edit { layer: e.layer, appearance: e.vector.appearance.unwrap_or_else(|| renderer::legacy_appearance(&e.shape)) })
                    .collect(),
            },
        })
    }
    pub fn is_fresh(&self, s: &Session) -> bool {
        check_fresh(s, &self.request).is_ok()
    }
    pub fn request(&self) -> &Commit {
        &self.request
    }
    /// Update all captured targets. Failure leaves the gesture unchanged.
    pub fn update(&mut self, mut f: impl FnMut(&mut Appearance) -> Result<()>) -> Result<()> {
        let mut next = self.request.clone();
        for e in &mut next.edits {
            f(&mut e.appearance)?;
            renderer::validate(&e.appearance).map_err(error)?;
        }
        self.request = next;
        Ok(())
    }
    pub fn commit(self, s: &mut Session) -> Result<()> {
        check_fresh(s, &self.request)?;
        s.execute("shape.appearance.set", serde_json::to_value(&self.request).map_err(|e| error(e.to_string()))?)?;
        Ok(())
    }
}

fn request(v: &Value) -> Result<Commit> {
    serde_json::from_value(v.clone()).map_err(|e| error(format!("invalid appearance request: {e}")))
}
fn set(s: &mut Session, v: &Value) -> Result<Value> {
    let c = request(v)?;
    let edits = prepare(s, &c, &super::styles::Projection)?;
    vector_api::apply_shape_edits(s, "Edit Appearance", edits)?;
    Ok(json!({"layers":c.edits.iter().map(|e| e.layer.0).collect::<Vec<_>>() }))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let c = request(v)?;
    let edits = prepare(s, &c, &super::styles::Projection)?;
    Ok(json!({"layers":edits.iter().map(|e|e.layer.0).collect::<Vec<_>>(),"detached":true}))
}
fn info(s: &mut Session, v: &Value) -> Result<Value> {
    let ids = v
        .get("layers")
        .and_then(Value::as_array)
        .ok_or_else(|| error("appearance info needs layers"))?
        .iter()
        .map(|v| v.as_u64().map(LayerId).ok_or_else(|| error("invalid appearance layer id")))
        .collect::<Result<Vec<_>>>()?;
    serde_json::to_value(Gesture::begin(s, &ids)?.request()).map_err(|e| error(e.to_string()))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "shape.appearance.set",
            label: "Edit Appearance",
            menu: &[],
            shortcut: None,
            params: "{document,revision,edits:[{layer,appearance}]}",
            enabled: crate::vector_cmds::has_doc,
            run: set,
            journal: true,
        },
        CommandSpec {
            id: "shape.appearance.preview",
            label: "Preview Appearance",
            menu: &[],
            shortcut: None,
            params: "{document,revision,edits:[{layer,appearance}]}",
            enabled: crate::vector_cmds::has_doc,
            run: preview,
            journal: false,
        },
        CommandSpec {
            id: "shape.appearance.info",
            label: "Appearance",
            menu: &[],
            shortcut: None,
            params: "{layers:[id]}",
            enabled: crate::vector_cmds::has_doc,
            run: info,
            journal: false,
        },
    ]
}
#[cfg(test)]
mod tests;
