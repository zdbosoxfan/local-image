//! Captured path-effect transactions, informed by VectorCraft cmd/effectcmd.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Native field registration/projection routing belongs to 00; no hidden production storage.
use crate::{EngineError, Result, Session, commands::CommandSpec, vector_api};
pub use photocraft_doc::vector_design::effects::*;
use photocraft_doc::{Document, Layer, LayerContent, LayerId, ShapeLayer, Surface};
pub use photocraft_vecfx::path_effects as geometry;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
};
fn bad(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
fn geo<T>(r: geometry::Result<T>) -> Result<T> {
    r.map_err(|e| bad(e.to_string()))
}
/// Register once at engine bootstrap after 00 adds VectorData.effects. Both functions operate
/// on the document-owned Layer; write may not mutate external state or caches.
#[derive(Clone, Copy)]
pub struct Access {
    pub read: fn(&Layer) -> Result<Vec<PathEffect>>,
    pub write: fn(&mut Layer, Vec<PathEffect>) -> Result<()>,
}
static ACCESS: OnceLock<Access> = OnceLock::new();
pub fn install_access(a: Access) -> Result<()> {
    ACCESS.set(a).map_err(|_| bad("path-effect access already installed"))
}
pub fn access() -> Result<Access> {
    ACCESS.get().copied().ok_or_else(|| bad("package 00 must register native path-effect storage"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
}
impl Capture {
    pub fn begin(s: &Session) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        Ok(Self { document: st.doc.id.0, revision: st.revision })
    }
    pub fn check(self, s: &Session) -> Result<()> {
        if self == Self::begin(s)? { Ok(()) } else { Err(bad("path-effect gesture is stale; start again")) }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub layer: LayerId,
    pub effects: Vec<PathEffect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub edits: Vec<Edit>,
}
#[derive(Clone, Debug)]
pub struct Gesture {
    pub request: Request,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId], a: Access) -> Result<Self> {
        if ids.len() > 64 {
            return Err(bad("path effects require at most 64 targets"));
        }
        let st = s.active().ok_or(EngineError::NoDocument)?;
        vector_api::snapshot_shapes(&st.doc, ids)?;
        let edits = ids
            .iter()
            .map(|id| {
                let layer = st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
                Ok(Edit { layer: *id, effects: (a.read)(layer)? })
            })
            .collect::<Result<Vec<_>>>()?;
        let g = Self { request: Request { capture: Capture::begin(s)?, edits } };
        validate(&st.doc, &g.request)?;
        Ok(g)
    }
    pub fn update(&mut self, s: &Session, mut f: impl FnMut(&mut Vec<PathEffect>) -> Result<()>) -> Result<()> {
        self.request.capture.check(s)?;
        let mut next = self.request.clone();
        for edit in &mut next.edits {
            f(&mut edit.effects)?;
        }
        validate(&s.active().ok_or(EngineError::NoDocument)?.doc, &next)?;
        self.request = next;
        Ok(())
    }
}
fn validate(doc: &Document, r: &Request) -> Result<()> {
    if r.edits.is_empty() || r.edits.len() > 64 {
        return Err(bad("path effects require 1 to 64 targets"));
    }
    let ids = r.edits.iter().map(|e| e.layer).collect::<Vec<_>>();
    let snapshots = vector_api::snapshot_shapes(doc, &ids)?;
    for (source, e) in snapshots.iter().zip(&r.edits) {
        let locks = doc.effective_locks(e.layer);
        if locks.position || locks.artboard {
            return Err(bad("path effect target is locked"));
        }
        geo(geometry::validate_path(&source.shape.path))?;
        let a = source.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(&source.shape));
        geo(geometry::validate_stack(&e.effects, a.items.len()))?;
        photocraft_vector::appearance::validate(&a).map_err(bad)?;
        // Preserve node/live/corner metadata: the providers project it before effects.
        super::expand::project_shape_base(doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?)?;
    }
    Ok(())
}
pub fn project(layer: &Layer, effects: &[PathEffect], dpi: f32) -> Result<Vec<geometry::PaintPath>> {
    let base = super::expand::project_shape_base(layer)?;
    geo(geometry::project(&base, layer.vector.appearance.as_ref(), effects, geometry::Options { dpi: f64::from(dpi), ..Default::default() }))
}
/// Geometry-cache key uses source/provider inputs and effects, never whole-document revision.
/// Patterns/render scale are owned by 08/09's paint/view keys rather than this geometry cache.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    layer: LayerId,
    path: photocraft_doc::Path,
    vector: photocraft_doc::VectorData,
    fill: Option<photocraft_doc::Fill>,
    stroke: Option<photocraft_doc::ShapeStroke>,
    effects: Vec<PathEffect>,
    dpi: f32,
}
#[derive(Clone, Default)]
pub struct ProjectionCache {
    entries: VecDeque<(Key, Arc<Vec<geometry::PaintPath>>, Vec<LayerId>)>,
    pub hits: u64,
    pub misses: u64,
}
impl ProjectionCache {
    pub fn get(&mut self, layer: &Layer, effects: &[PathEffect], dpi: f32) -> Result<Arc<Vec<geometry::PaintPath>>> {
        let LayerContent::Shape(sh) = &layer.content else { return Err(bad("path effects require shapes")) };
        let key = Key {
            layer: layer.id,
            path: sh.path.clone(),
            vector: layer.vector.clone(),
            fill: sh.fill.clone(),
            stroke: sh.stroke.clone(),
            effects: effects.to_vec(),
            dpi,
        };
        if let Some(i) = self.entries.iter().position(|(k, _, _)| *k == key) {
            let entry = self.entries.remove(i).ok_or_else(|| bad("projection cache entry disappeared"))?;
            let value = entry.1.clone();
            self.entries.push_back(entry);
            self.hits += 1;
            return Ok(value);
        }
        let scene = Arc::new(project(layer, effects, dpi)?);
        self.misses += 1;
        // 8 entries × bounded 65536 nodes; detached cache contains no full-document surfaces.
        if self.entries.len() == 8 {
            self.entries.pop_front();
        }
        let ids = scene.iter().map(|_| LayerId::fresh()).collect();
        self.entries.push_back((key, scene.clone(), ids));
        Ok(scene)
    }
    fn projected(&mut self, layer: &Layer, effects: &[PathEffect], doc: &Document) -> Result<Layer> {
        let scene = self.get(layer, effects, doc.resolution_dpi)?;
        let ids = &self.entries.back().ok_or_else(|| bad("projection cache is empty"))?.2;
        projected_layer_with_ids(layer, &scene, doc, Some(ids))
    }
}
fn render(plan: &geometry::render::Compiled<'_>, doc: &Document) -> Result<Surface> {
    if plan.cache_bytes(doc.pixel_format()) > geometry::render::MAX_CACHE_BYTES {
        return Err(bad("path-effect cache exceeds 512 MiB"));
    }
    let mut out = Surface::new(doc.pixel_format());
    let b = plan.bounds();
    for y in (b.y0..b.y1).step_by(photocraft_geom::TILE_SIZE as usize) {
        for x in (b.x0..b.x1).step_by(photocraft_geom::TILE_SIZE as usize) {
            let r = photocraft_geom::Rect::new(
                x,
                y,
                x.saturating_add(photocraft_geom::TILE_SIZE).min(b.x1),
                y.saturating_add(photocraft_geom::TILE_SIZE).min(b.y1),
            );
            let rgba = geo(plan.render_rgba(r))?;
            if rgba.iter().all(|p| p[3] == 0.) {
                continue;
            }
            let mut values = vec![0.; rgba.len() * doc.pixel_format().channels()];
            for (p, v) in rgba.iter().zip(values.chunks_exact_mut(doc.pixel_format().channels())) {
                photocraft_raster::from_rgba_into(&doc.pixel_format(), *p, v);
            }
            out.write_region(r, &values);
        }
    }
    Ok(out)
}
/// Refresh one source-edit cache without baking its path. 00 calls this from node/live-shape/
/// appearance edits and reload; the same project() feeds scaled view and exporters.
pub fn refresh(doc: &Document, layer: &mut Layer, a: Access) -> Result<()> {
    let effects = (a.read)(layer)?;
    let scene = project(layer, &effects, doc.resolution_dpi)?;
    let plan = geo(geometry::render::Compiled::new(&scene, &doc.patterns, doc.bounds(), &super::expand::Projection))?;
    let cache = render(&plan, doc)?;
    if let LayerContent::Shape(sh) = &mut layer.content {
        sh.cache = Some(cache);
        sh.psd_raw = None;
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub request: Request,
    pub layers: Vec<Layer>,
    pub preview: Document,
}
pub fn prepare(s: &Session, r: &Request, a: Access) -> Result<Prepared> {
    r.capture.check(s)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    validate(doc, r)?;
    // Evaluate the ENTIRE target set before allocating pixels.
    let scenes = r
        .edits
        .iter()
        .map(|e| project(doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?, &e.effects, doc.resolution_dpi))
        .collect::<Result<Vec<_>>>()?;
    let plans = scenes
        .iter()
        .map(|scene| geo(geometry::render::Compiled::new(scene, &doc.patterns, doc.bounds(), &super::expand::Projection)))
        .collect::<Result<Vec<_>>>()?;
    let bytes = plans.iter().map(|p| p.cache_bytes(doc.pixel_format()) + p.expanded_cache_bytes(doc.pixel_format())).sum::<u128>();
    if bytes > geometry::render::MAX_CACHE_BYTES {
        return Err(bad("path-effect prepared caches exceed 512 MiB"));
    }
    let mut layers = Vec::new();
    let mut preview = (**doc).clone();
    for ((edit, scene), plan) in r.edits.iter().zip(&scenes).zip(&plans) {
        let cache = render(plan, doc)?;
        let mut source = doc.layer(edit.layer).ok_or(EngineError::NoLayer(edit.layer))?.clone();
        (a.write)(&mut source, edit.effects.clone())?;
        if let LayerContent::Shape(sh) = &mut source.content {
            sh.cache = Some(cache.clone());
            sh.psd_raw = None;
        }
        let mut display = projected_layer(&source, scene, doc)?;
        // Rendering consumers see ordinary child paths; source remains non-destructive.
        (a.write)(&mut display, Vec::new())?;
        *preview.layer_mut(edit.layer).ok_or(EngineError::NoLayer(edit.layer))? = display;
        layers.push(source);
    }
    Ok(Prepared { request: r.clone(), layers, preview })
}
/// Paint operations become ordinary child shapes under original layer identity/transparency.
/// Strokes remain ordinary stroke paths; package 36 may outline them using 08 after this step.
pub fn projected_layer(source: &Layer, scene: &[geometry::PaintPath], doc: &Document) -> Result<Layer> {
    projected_layer_with_ids(source, scene, doc, None)
}
fn projected_layer_with_ids(source: &Layer, scene: &[geometry::PaintPath], doc: &Document, ids: Option<&[LayerId]>) -> Result<Layer> {
    let mut out = source.clone();
    let preflight = geo(geometry::render::Compiled::new(scene, &doc.patterns, doc.bounds(), &super::expand::Projection))?;
    if preflight.expanded_cache_bytes(doc.pixel_format()) > geometry::render::MAX_CACHE_BYTES {
        return Err(bad("expanded path-effect caches exceed 512 MiB"));
    }
    let mut children = Vec::new();
    let mut bytes = 0u128;
    for (i, p) in scene.iter().enumerate() {
        let sh = ShapeLayer { path: p.path.clone(), ..Default::default() };
        let mut item = p.item.clone();
        let blend = match &mut item {
            photocraft_doc::vector_design::appearance::AppearanceItem::Mesh(mesh) => {
                let blend = mesh.blend;
                if blend != photocraft_doc::BlendMode::Dissolve {
                    mesh.blend = photocraft_doc::BlendMode::Normal;
                }
                if blend == photocraft_doc::BlendMode::Dissolve { photocraft_doc::BlendMode::Normal } else { blend }
            }
            photocraft_doc::vector_design::appearance::AppearanceItem::Fill(f) => {
                let b = f.blend;
                if b == photocraft_doc::BlendMode::Dissolve {
                    photocraft_doc::BlendMode::Normal
                } else {
                    f.blend = photocraft_doc::BlendMode::Normal;
                    b
                }
            }
            photocraft_doc::vector_design::appearance::AppearanceItem::Stroke(s) => {
                let b = s.blend;
                if b == photocraft_doc::BlendMode::Dissolve {
                    photocraft_doc::BlendMode::Normal
                } else {
                    s.blend = photocraft_doc::BlendMode::Normal;
                    b
                }
            }
        };
        let appearance = photocraft_doc::vector_design::appearance::Appearance { items: vec![item] };
        let plan =
            photocraft_vector::appearance::CompiledAppearance::with_projection(&sh, Some(&appearance), &doc.patterns, doc.bounds(), &super::expand::Projection)
                .map_err(bad)?;
        bytes += plan.cache_bytes(doc.pixel_format());
        if bytes > geometry::render::MAX_CACHE_BYTES {
            return Err(bad("expanded path-effect caches exceed 512 MiB"));
        }
        let mut child = Layer::new(source.name.clone(), LayerContent::Shape(sh));
        if let Some(id) = ids.and_then(|ids| ids.get(i)) {
            child.id = *id;
        }
        child.blend = blend;
        if let LayerContent::Shape(sh) = &mut child.content {
            photocraft_vector::appearance::mirror_legacy(sh, &appearance);
            sh.cache = Some(plan.render(doc.pixel_format(), doc.bounds()).map_err(bad)?);
        }
        child.vector.appearance = Some(appearance);
        children.push(child);
    }
    out.content = LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto });
    out.vector.corners.clear();
    out.vector.live_shape = None;
    out.vector.appearance = None;
    out.vector.effects.clear();
    super::expand::clear_geometry_blocks(&mut out);
    Ok(out)
}
/// Detached projection for display/export. Does not mutate source paths, revision or history.
pub fn project_document(doc: &Document, a: Access, cache: &mut ProjectionCache) -> Result<Document> {
    fn walk(doc: &Document, layers: &mut [Layer], a: Access, cache: &mut ProjectionCache) -> Result<()> {
        for layer in layers {
            let effects = (a.read)(layer)?;
            if !effects.is_empty() {
                let mut out = cache.projected(layer, &effects, doc)?;
                (a.write)(&mut out, Vec::new())?;
                *layer = out;
            } else if let Some(children) = layer.children_mut() {
                walk(doc, children, a, cache)?;
            }
        }
        Ok(())
    }
    let mut out = doc.clone();
    walk(doc, &mut out.layers, a, cache)?;
    Ok(out)
}
pub fn commit_with_access(s: &mut Session, r: &Request, a: Access) -> Result<()> {
    let prepared = prepare(s, r, a)?;
    r.capture.check(s)?;
    s.edit("Edit Path Effects", move |doc, _| {
        for layer in prepared.layers {
            let id = layer.id;
            *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
        }
        Ok(())
    })
}
pub fn commit(s: &mut Session, r: &Request) -> Result<()> {
    s.execute("vector.pathEffects.set", serde_json::to_value(r).map_err(|e| bad(e.to_string()))?)?;
    Ok(())
}
fn set(s: &mut Session, v: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    commit_with_access(s, &r, access()?)?;
    Ok(json!({"layers":r.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>() }))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    let p = prepare(s, &r, access()?)?;
    Ok(json!({"detached":true,"layers":p.request.edits.iter().map(|e|e.layer.0).collect::<Vec<_>>() }))
}
fn info(s: &mut Session, v: &Value) -> Result<Value> {
    let ids = v
        .get("layers")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("path effects info requires layers"))?
        .iter()
        .map(|v| v.as_u64().map(LayerId).ok_or_else(|| bad("invalid layer id")))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!(Gesture::begin(s, &ids, access()?)?.request))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("vector.pathEffects.set", set as fn(&mut Session, &Value) -> Result<Value>, true),
        ("vector.pathEffects.preview", preview, false),
        ("vector.pathEffects.info", info, false),
    ]
    .into_iter()
    .map(|(id, run, journal)| CommandSpec {
        id,
        label: "Edit Path Effects",
        menu: &[],
        shortcut: None,
        params: "Request {capture:{document,revision},edits:[{layer,effects}]}; info {layers}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
#[cfg(test)]
#[path = "path_effects/gpu.rs"]
pub mod gpu;
#[cfg(test)]
#[path = "path_effects/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "path_effects/native.rs"]
mod native;
