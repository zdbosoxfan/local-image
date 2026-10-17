//! Width-profile transactions, informed by VectorCraft crates/engine/src/tests_widthtool.rs
//! and crates/doc/src/appearance.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! Source paths stay editable. The appearance renderer draws the filled width projection.

use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
use photocraft_doc::vector_design::appearance::{AppearanceItem, StrokeItem};
use photocraft_doc::{Document, LayerContent, LayerId};
pub use photocraft_vecfx::width::{self as geometry, WidthPoint, WidthProfile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub layer: LayerId,
    pub item: usize,
    pub profile: Option<WidthProfile>,
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
pub fn stroke(s: &Session, id: LayerId, item: Option<usize>) -> Result<(usize, StrokeItem)> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let edits = vector_api::snapshot_shapes(&st.doc, &[id])?;
    let e = &edits[0];
    let a = e.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(&e.shape));
    let i = item.or_else(|| a.items.iter().rposition(|i| matches!(i, AppearanceItem::Stroke(_)))).ok_or_else(|| error("select a stroked shape"))?;
    match a.items.get(i) {
        Some(AppearanceItem::Stroke(stroke)) => Ok((i, stroke.clone())),
        _ => Err(error("selected appearance item is not a stroke")),
    }
}
pub fn check_fresh(s: &Session, c: &Commit) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != c.document || st.revision != c.revision {
        return Err(error("width gesture is stale; start again"));
    }
    Ok(())
}
fn appearance_request(s: &Session, c: &Commit) -> Result<super::appearance::Commit> {
    check_fresh(s, c)?;
    if c.edits.len() > 64 {
        return Err(error("width exceeds the target budget"));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids: Vec<_> = c.edits.iter().map(|e| e.layer).collect();
    let shapes = vector_api::snapshot_shapes(&st.doc, &ids)?;
    let mut edits = Vec::new();
    for (shape, edit) in shapes.iter().zip(&c.edits) {
        if let Some(p) = &edit.profile {
            geometry::validate_profile(p).map_err(error)?;
        }
        if !shape.vector.corners.is_empty() {
            return Err(error("width with live corners requires the combined geometry projection provider"));
        }
        let mut a = shape.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(&shape.shape));
        match a.items.get_mut(edit.item) {
            Some(AppearanceItem::Stroke(stroke)) => stroke.profile = edit.profile.clone(),
            _ => return Err(error("selected appearance item is not a stroke")),
        }
        photocraft_vector::appearance::validate(&a).map_err(error)?;
        edits.push(super::appearance::Edit { layer: edit.layer, appearance: a });
    }
    Ok(super::appearance::Commit { document: c.document, revision: c.revision, edits })
}
pub fn prepare(s: &Session, c: &Commit) -> Result<Vec<vector_api::ShapeEdit>> {
    super::appearance::prepare(s, &appearance_request(s, c)?, &geometry::Projection)
}
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    let edits = prepare(s, c)?;
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
    bases: Vec<StrokeItem>,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId], item: Option<usize>) -> Result<Self> {
        if ids.len() > 64 {
            return Err(error("width exceeds the target budget"));
        }
        let st = s.active().ok_or(EngineError::NoDocument)?;
        vector_api::snapshot_shapes(&st.doc, ids)?;
        let mut edits = Vec::new();
        let mut bases = Vec::new();
        for id in ids {
            let (item, base) = stroke(s, *id, item)?;
            edits.push(Edit { layer: *id, item, profile: base.profile.clone() });
            bases.push(base);
        }
        let g = Self { request: Commit { document: st.doc.id.0, revision: st.revision, edits }, bases };
        appearance_request(s, &g.request)?;
        Ok(g)
    }
    pub fn request(&self) -> &Commit {
        &self.request
    }
    pub fn is_fresh(&self, s: &Session) -> bool {
        check_fresh(s, &self.request).is_ok()
    }
    pub fn profile(&self, target: usize) -> Result<WidthProfile> {
        let e = self.request.edits.get(target).ok_or_else(|| error("width target is absent"))?;
        let half = f64::from(self.bases[target].base.width) / 2.0;
        Ok(e.profile.as_ref().filter(|p| !p.points.is_empty()).cloned().unwrap_or_else(|| geometry::uniform(half)))
    }
    pub fn set_profile(&mut self, target: usize, p: Option<WidthProfile>) -> Result<()> {
        if let Some(p) = &p {
            geometry::validate_profile(p).map_err(error)?;
        }
        let e = self.request.edits.get_mut(target).ok_or_else(|| error("width target is absent"))?;
        e.profile = p;
        Ok(())
    }
    pub fn preset(&mut self, id: &str) -> Result<()> {
        let mut next = self.clone();
        for i in 0..next.request.edits.len() {
            let p = if id == "uniform" { None } else { Some(geometry::preset(id, f64::from(next.bases[i].base.width) / 2.0).map_err(error)?) };
            next.set_profile(i, p)?;
        }
        *self = next;
        Ok(())
    }
    pub fn set_point(&mut self, target: usize, index: Option<usize>, point: WidthPoint) -> Result<usize> {
        let mut p = self.profile(target)?;
        let i = geometry::set_point(&mut p, index, point).map_err(error)?;
        self.set_profile(target, Some(p))?;
        Ok(i)
    }
    pub fn remove(&mut self, target: usize, index: usize) -> Result<()> {
        let mut p = self.profile(target)?;
        if index >= p.points.len() {
            return Err(error("width point is absent"));
        }
        p.points.remove(index);
        self.set_profile(target, if p.points.is_empty() { None } else { Some(p) })
    }
    pub fn commit(self, s: &mut Session) -> Result<()> {
        check_fresh(s, &self.request)?;
        s.execute("shape.width.set", serde_json::to_value(self.request).map_err(|e| error(e.to_string()))?)?;
        Ok(())
    }
}
fn request(v: &Value) -> Result<Commit> {
    serde_json::from_value(v.clone()).map_err(|e| error(format!("invalid width request: {e}")))
}
fn set(s: &mut Session, v: &Value) -> Result<Value> {
    let c = request(v)?;
    let edits = prepare(s, &c)?;
    vector_api::apply_shape_edits(s, "Edit Width Profile", edits)?;
    Ok(json!({"layers":c.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>()}))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let c = request(v)?;
    prepare(s, &c)?;
    Ok(json!({"detached":true,"layers":c.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>()}))
}
fn info(s: &mut Session, v: &Value) -> Result<Value> {
    let ids = v
        .get("layers")
        .and_then(Value::as_array)
        .ok_or_else(|| error("width info needs layers"))?
        .iter()
        .map(|v| v.as_u64().map(LayerId).ok_or_else(|| error("invalid width layer id")))
        .collect::<Result<Vec<_>>>()?;
    let item = v.get("item").map(|i| i.as_u64().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| error("invalid stroke item"))).transpose()?;
    serde_json::to_value(Gesture::begin(s, &ids, item)?.request()).map_err(|e| error(e.to_string()))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "shape.width.set",
            label: "Edit Width Profile",
            menu: &[],
            shortcut: None,
            params: "{document,revision,edits:[{layer,item,profile}]}",
            enabled: crate::vector_cmds::has_doc,
            run: set,
            journal: true,
        },
        CommandSpec {
            id: "shape.width.preview",
            label: "Preview Width Profile",
            menu: &[],
            shortcut: None,
            params: "{document,revision,edits:[{layer,item,profile}]}",
            enabled: crate::vector_cmds::has_doc,
            run: preview,
            journal: false,
        },
        CommandSpec {
            id: "shape.width.info",
            label: "Width Profile",
            menu: &[],
            shortcut: None,
            params: "{layers:[id],item?:index}",
            enabled: crate::vector_cmds::has_doc,
            run: info,
            journal: false,
        },
    ]
}
#[cfg(test)]
#[path = "width/tests.rs"]
mod tests;
