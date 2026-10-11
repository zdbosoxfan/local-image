//! Gradient mesh captured transactions. Behaviour informed by VectorCraft
//! engine/cmd/live.rs (object.mesh.*), pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors (MIT OR Apache-2.0).
//! Native appearance schema and paint dispatch are supplied by packages 00/08.
//! No mesh is stored in a process-global sidecar or committed as flattened geometry.
use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    vector_api::{self, ShapeEdit},
};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, ShapeLayer};
use photocraft_raster::Surface;
use photocraft_vector::{
    appearance::CompiledAppearance,
    mesh_paint::{self, CompiledMesh, Mesh, MeshPoint},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointData {
    pub position: [f64; 2],
    #[serde(default)]
    pub handles: [[f64; 2]; 4],
    pub rgba: [f32; 4],
    #[serde(default = "one")]
    pub opacity: f32,
}
fn one() -> f32 {
    1.
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshData {
    pub rows: usize,
    pub cols: usize,
    pub points: Vec<PointData>,
    #[serde(default = "yes")]
    pub dither: bool,
    #[serde(default)]
    pub seed: u32,
}
impl MeshData {
    pub fn geometry(&self) -> Mesh {
        Mesh {
            rows: self.rows,
            cols: self.cols,
            points: self.points.iter().map(|p| MeshPoint { position: p.position, handles: p.handles, rgba: p.rgba, opacity: p.opacity }).collect(),
            dither: self.dither,
            seed: self.seed,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.points.len() > 2601 {
            return Err(error("mesh point budget exceeded"));
        }
        self.geometry().validate().map_err(error)
    }
    pub fn update(&mut self, f: impl FnOnce(&mut Mesh) -> std::result::Result<(), String>) -> Result<()> {
        self.validate()?;
        let mut next = self.geometry();
        f(&mut next).map_err(error)?;
        next.validate().map_err(error)?;
        *self = next.into();
        Ok(())
    }
}
impl From<Mesh> for MeshData {
    fn from(m: Mesh) -> Self {
        Self {
            rows: m.rows,
            cols: m.cols,
            points: m.points.into_iter().map(|p| PointData { position: p.position, handles: p.handles, rgba: p.rgba, opacity: p.opacity }).collect(),
            dither: m.dither,
            seed: m.seed,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Flat,
    ToCenter,
    ToEdge,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub rows: usize,
    pub cols: usize,
    pub appearance: Appearance,
    pub highlight: f32,
}
impl Default for Options {
    fn default() -> Self {
        Self { rows: 4, cols: 4, appearance: Appearance::Flat, highlight: 100. }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub layer: LayerId,
    pub mesh: MeshData,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub document: u64,
    pub revision: u64,
    pub edits: Vec<Edit>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
    pub options: Options,
}
/// This explicit provider boundary is also injectable for detached integration fixtures.
/// `write` must store editable native paint; `render` must render the entire resulting stack.
#[derive(Clone, Copy)]
pub struct Hooks {
    pub read: fn(&Layer) -> Result<Option<MeshData>>,
    pub write: fn(&mut ShapeEdit, &MeshData) -> Result<()>,
    pub render: fn(&Document, &ShapeEdit) -> Result<Surface>,
    pub cache_bytes: fn(&Document, &ShapeEdit) -> Result<u128>,
}
/// 09 installs its existing scaled projection entrypoint after serial registration.
/// This stores only a provider function, never document/mesh state.
#[derive(Clone, Copy)]
pub struct ViewHooks {
    pub project: fn(&Document, f64) -> Result<Document>,
}
static VIEW_HOOKS: std::sync::OnceLock<ViewHooks> = std::sync::OnceLock::new();
pub fn install_view_hooks(hooks: ViewHooks) -> std::result::Result<(), ViewHooks> {
    VIEW_HOOKS.set(hooks)
}
pub fn view_hooks() -> Option<ViewHooks> {
    VIEW_HOOKS.get().copied()
}
pub const NATIVE: Hooks = Hooks { read: read_native, write: write_native, render: render_native, cache_bytes: cache_bytes_native };
/// JSON bridge keeps 43 compiled before the owner adds the additive mesh paint variant.
/// It uses the real native Appearance model; absent schema is an error, never success.
pub fn read_native(layer: &Layer) -> Result<Option<MeshData>> {
    let Some(a) = &layer.vector.appearance else {
        return Ok(None);
    };
    let value = serde_json::to_value(a).map_err(|e| error(e.to_string()))?;
    let Some(items) = value.get("items").and_then(Value::as_array) else {
        return Ok(None);
    };
    for item in items.iter().rev() {
        if item.get("kind").and_then(Value::as_str) == Some("mesh") && item.get("visible").and_then(Value::as_bool) != Some(false) {
            let mesh: MeshData =
                serde_json::from_value(item.get("paint").cloned().ok_or_else(|| error("mesh paint missing"))?).map_err(|e| error(e.to_string()))?;
            mesh.validate()?;
            return Ok(Some(mesh));
        }
    }
    Ok(None)
}
pub fn write_native(edit: &mut ShapeEdit, mesh: &MeshData) -> Result<()> {
    mesh.validate()?;
    let a = edit.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(&edit.shape));
    let mut value = serde_json::to_value(a).map_err(|e| error(e.to_string()))?;
    if value.get("items").is_none() {
        value["items"] = json!([]);
    }
    let items = value.get_mut("items").and_then(Value::as_array_mut).ok_or_else(|| error("invalid appearance stack"))?;
    if let Some(item) = items.iter_mut().rev().find(|i| i["kind"] == "mesh" && i.get("visible").and_then(Value::as_bool) != Some(false)) {
        item["paint"] = json!(mesh);
    } else if let Some(item) = items.iter_mut().rev().find(|i| i["kind"] == "fill" && i.get("visible").and_then(Value::as_bool) != Some(false)) {
        item["kind"] = json!("mesh");
        item["paint"] = json!(mesh);
    } else {
        items.push(json!({"kind":"mesh","paint":mesh}));
    }
    let appearance = serde_json::from_value(value).map_err(|e| error(format!("gradient mesh requires package 00/08 native appearance schema: {e}")))?;
    photocraft_vector::appearance::mirror_legacy(&mut edit.shape, &appearance);
    edit.vector.appearance = Some(appearance);
    edit.shape.psd_raw = None;
    Ok(())
}
fn projected_shape(edit: &ShapeEdit) -> Result<ShapeLayer> {
    if edit.vector.corners.is_empty() { Ok(edit.shape.clone()) } else { crate::vector_packages::corners::project_shape(&edit.shape, &edit.vector.corners) }
}
fn cache_bytes_native(doc: &Document, edit: &ShapeEdit) -> Result<u128> {
    let a = edit.vector.appearance.as_ref().ok_or_else(|| error("mesh appearance not stored"))?;
    let shape = projected_shape(edit)?;
    Ok(CompiledAppearance::with_projection(&shape, Some(a), &doc.patterns, doc.bounds(), &crate::vector_packages::styles::Projection)
        .map_err(error)?
        .cache_bytes(doc.pixel_format()))
}
fn render_native(doc: &Document, edit: &ShapeEdit) -> Result<Surface> {
    let a = edit.vector.appearance.as_ref().ok_or_else(|| error("mesh appearance not stored"))?;
    let shape = projected_shape(edit)?;
    let compiled =
        CompiledAppearance::with_projection(&shape, Some(a), &doc.patterns, doc.bounds(), &crate::vector_packages::styles::Projection).map_err(error)?;
    if compiled.cache_bytes(doc.pixel_format()) > 256 << 20 {
        return Err(error("mesh stack exceeds 256 MiB"));
    }
    compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)
}
pub fn fresh(s: &Session, c: &Commit) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != c.document || st.revision != c.revision {
        return Err(error("gradient mesh gesture is stale"));
    }
    Ok(())
}
fn snapshot(s: &Session, c: &Commit) -> Result<Vec<ShapeEdit>> {
    fresh(s, c)?;
    if c.edits.is_empty() || c.edits.len() > 64 {
        return Err(error("mesh needs 1..64 targets"));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids = c.edits.iter().map(|e| e.layer).collect::<Vec<_>>();
    let edits = vector_api::snapshot_shapes(&st.doc, &ids)?;
    for (e, r) in edits.iter().zip(&c.edits) {
        r.mesh.validate()?;
        vector_api::validate_path(&e.shape.path)?;
    }
    Ok(edits)
}
pub fn prepare_with(s: &Session, c: &Commit, hooks: Hooks) -> Result<Vec<ShapeEdit>> {
    let mut edits = snapshot(s, c)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    // Compile every target and bound all allocations before any provider/render invocation.
    let compiled = edits
        .iter()
        .zip(&c.edits)
        .map(|(e, r)| CompiledMesh::new(&r.mesh.geometry(), &e.shape.path, st.doc.bounds()).map_err(error))
        .collect::<Result<Vec<_>>>()?;
    if compiled.iter().map(|m| m.cache_bytes(st.doc.pixel_format())).sum::<u128>() > 256 << 20 {
        return Err(error("mesh prepared cache set exceeds 256 MiB"));
    }
    for (e, r) in edits.iter_mut().zip(&c.edits) {
        (hooks.write)(e, &r.mesh)?;
    }
    let bytes = edits.iter().map(|e| (hooks.cache_bytes)(&st.doc, e)).collect::<Result<Vec<_>>>()?.into_iter().sum::<u128>();
    if bytes > 256 << 20 {
        return Err(error("mesh complete appearance cache set exceeds 256 MiB"));
    }
    for e in &mut edits {
        e.shape.cache = Some((hooks.render)(&st.doc, e)?);
        e.shape.psd_raw = None;
    }
    Ok(edits)
}
pub fn prepare(s: &Session, c: &Commit) -> Result<Vec<ShapeEdit>> {
    prepare_with(s, c, NATIVE)
}
pub fn preview_document_with(s: &Session, c: &Commit, hooks: Hooks) -> Result<Document> {
    let edits = prepare_with(s, c, hooks)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    for e in edits {
        let layer = doc.layer_mut(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        layer.content = LayerContent::Shape(e.shape);
        layer.vector = e.vector;
    }
    Ok(doc)
}
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    preview_document_with(s, c, NATIVE)
}
#[derive(Clone, Debug)]
pub struct Gesture {
    request: Commit,
}
impl Gesture {
    pub fn begin_with(s: &Session, ids: &[LayerId], create: Option<&Options>, hooks: Hooks) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if ids.is_empty() || ids.len() > 64 {
            return Err(error("mesh needs 1..64 targets"));
        }
        let shapes = vector_api::snapshot_shapes(&st.doc, ids)?;
        let mut edits = Vec::new();
        for e in shapes {
            let mesh = if let Some(options) = create {
                vector_api::validate_path(&e.shape.path)?;
                let frame = e.shape.path.control_bounds().ok_or_else(|| error("mesh needs a nonempty shape"))?;
                let source = match &e.vector.appearance {
                    Some(a) => a.items.iter().rev().find_map(|i| match i {
                        photocraft_doc::vector_design::appearance::AppearanceItem::Fill(f) if f.visible => Some(&f.paint),
                        _ => None,
                    }),
                    None => e.shape.fill.as_ref(),
                }
                .ok_or_else(|| error("mesh creation needs a visible fill"))?;
                if matches!(source, Fill::Pattern { .. }) {
                    return Err(error("mesh creation from pattern needs package 08 paint sampler"));
                }
                let canvas = st.doc.bounds();
                let paint = photocraft_vector::Paint::from_fill(
                    source,
                    frame,
                    (f64::from(canvas.x0), f64::from(canvas.y0), f64::from(canvas.x1), f64::from(canvas.y1)),
                );
                let appearance = match options.appearance {
                    Appearance::Flat => mesh_paint::Appearance::Flat,
                    Appearance::ToCenter => mesh_paint::Appearance::ToCenter,
                    Appearance::ToEdge => mesh_paint::Appearance::ToEdge,
                };
                Mesh::regular([frame.0, frame.1, frame.2, frame.3], options.rows, options.cols, appearance, options.highlight, |p| paint.at(p[0], p[1]))
                    .map_err(error)?
                    .into()
            } else {
                (hooks.read)(st.doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?)?.ok_or_else(|| error("select a gradient mesh"))?
            };
            mesh.validate()?;
            edits.push(Edit { layer: e.layer, mesh });
        }
        Ok(Self { request: Commit { document: st.doc.id.0, revision: st.revision, edits } })
    }
    pub fn begin(s: &Session, ids: &[LayerId], create: Option<&Options>) -> Result<Self> {
        Self::begin_with(s, ids, create, NATIVE)
    }
    pub fn request(&self) -> &Commit {
        &self.request
    }
    pub fn is_fresh(&self, s: &Session) -> bool {
        fresh(s, &self.request).is_ok()
    }
    pub fn update(&mut self, mut f: impl FnMut(LayerId, &mut Mesh) -> std::result::Result<(), String>) -> Result<()> {
        let mut next = self.request.clone();
        for e in &mut next.edits {
            e.mesh.update(|m| f(e.layer, m))?;
        }
        self.request = next;
        Ok(())
    }
    pub fn commit(self, s: &mut Session) -> Result<()> {
        fresh(s, &self.request)?;
        s.execute("object.mesh.set", json!(self.request))?;
        Ok(())
    }
}
fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(|e| error(format!("invalid mesh request: {e}")))
}
fn set(s: &mut Session, v: &Value) -> Result<Value> {
    let c: Commit = parse(v)?;
    let edits = prepare(s, &c)?;
    vector_api::apply_shape_edits(s, "Edit Gradient Mesh", edits)?;
    Ok(json!({"layers":c.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>()}))
}
fn create(s: &mut Session, v: &Value) -> Result<Value> {
    let r: Create = parse(v)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != r.document || st.revision != r.revision {
        return Err(error("gradient mesh creation is stale"));
    }
    let g = Gesture::begin(s, &r.layers, Some(&r.options))?;
    set(s, &json!(g.request()))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let c: Commit = parse(v)?;
    prepare(s, &c)?;
    Ok(json!({"detached":true,"layers":c.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>()}))
}
fn info(s: &mut Session, v: &Value) -> Result<Value> {
    let ids: Vec<LayerId> = parse(v.get("layers").ok_or_else(|| error("mesh info requires layers"))?)?;
    let g = Gesture::begin(s, &ids, None)?;
    Ok(json!(g.request()))
}
/// 00 adds this to the aggregate registration; public to keep the isolated compiled slot useful.
pub fn specs() -> Vec<CommandSpec> {
    [
        ("object.mesh.create", "Create Gradient Mesh…", create as fn(&mut Session, &Value) -> Result<Value>, true),
        ("object.mesh.set", "Edit Gradient Mesh", set, true),
        ("object.mesh.preview", "Preview Gradient Mesh", preview, false),
        ("object.mesh.info", "Gradient Mesh", info, false),
    ]
    .into_iter()
    .map(|(id, label, run, journal)| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params: "captured {document,revision,edits:[{layer,mesh}]}; create {document,revision,layers,options}; info {layers}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
#[cfg(test)]
mod tests;
