//! Destructive vector liquify/Puppet Warp transactions; raster tools remain separate.
//! Behavior adapted from VectorCraft tools/distort/{liquify,puppet}.rs and engine/cmd/distortcmds.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    vector_api::{self, ShapeEdit},
};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, Path};
pub use photocraft_vecfx::{liquify, puppet};
use photocraft_vector::appearance::CompiledAppearance;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
const MAX_TARGETS: usize = 256;
const MAX_CACHE_BYTES: u128 = 256 << 20;
fn error(e: impl ToString) -> EngineError {
    EngineError::Other(e.to_string())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
    /// With no selection, live shapes outside the stroke need no conversion.
    #[serde(default)]
    pub automatic: bool,
}
impl Capture {
    pub fn begin(s: &Session, layers: Vec<LayerId>) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let out = Self { document: st.doc.id.0, revision: st.revision, layers, automatic: st.selected_layers().is_empty() };
        out.validate(s)?;
        Ok(out)
    }
    pub fn validate(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if self.document != st.doc.id.0 || self.revision != st.revision {
            return Err(error("distort gesture is stale; start again"));
        }
        validate_targets(&st.doc, &self.layers, self.automatic)
    }
}
fn validate_targets(doc: &Document, ids: &[LayerId], automatic: bool) -> Result<()> {
    if ids.is_empty() || ids.len() > MAX_TARGETS {
        return Err(error("distort needs 1..256 shape targets"));
    }
    let edits = vector_api::snapshot_shapes(doc, ids)?;
    let mut nodes = 0;
    for e in edits {
        let locks = doc.effective_locks(e.layer);
        if locks.position {
            return Err(error("distort target is locked"));
        }
        let l = doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        if l.vector.live.is_some() || l.vector.on_path.is_some() {
            return Err(error("expand live artwork before distorting"));
        }
        if !automatic && (l.vector.live_shape.is_some() || e.shape.live.is_some()) {
            return Err(error("Convert to curves before distorting this live shape"));
        }
        liquify::validate_path(&e.shape.path, liquify::MAX_NODES).map_err(error)?;
        nodes += liquify::node_count(&e.shape.path);
        if nodes > 100_000 {
            return Err(error("distort selection exceeds total node budget"));
        }
    }
    Ok(())
}
/// Selected groups resolve to their ordinary path children. With no selection capture all
/// visible, unlocked paths once; each dab only deforms geometry actually reached by its brush.
pub fn target_layers(s: &Session) -> Result<Vec<LayerId>> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    fn collect(doc: &Document, l: &Layer, automatic: bool, visible: bool, out: &mut Vec<LayerId>) -> Result<()> {
        let visible = visible && l.visible;
        if automatic && !visible {
            return Ok(());
        }
        let locks = doc.effective_locks(l.id);
        if automatic && (locks.all || locks.pixels || locks.position) {
            return Ok(());
        }
        match &l.content {
            LayerContent::Shape(_) => {
                if !out.contains(&l.id) {
                    out.push(l.id);
                }
            }
            LayerContent::Text(_) if !automatic => return Err(error("Convert text to curves before distorting")),
            LayerContent::Group(g) => {
                if l.vector.live.is_some() {
                    if automatic {
                        return Ok(());
                    }
                    return Err(error("expand live artwork before distorting"));
                }
                for child in &g.children {
                    collect(doc, child, automatic, visible, out)?;
                }
            }
            _ if !automatic => return Err(error("distort needs selected vector paths")),
            _ => {}
        }
        Ok(())
    }
    let selected = st.selected_layers();
    let mut out = Vec::new();
    if selected.is_empty() {
        for l in &st.doc.layers {
            collect(&st.doc, l, true, true, &mut out)?;
        }
    } else {
        for id in selected {
            collect(&st.doc, st.doc.layer(id).ok_or(EngineError::NoLayer(id))?, false, true, &mut out)?;
        }
    }
    Ok(out)
}
/// Auto-target prompts name only live shapes reached by the current brush.
pub fn conversion_targets(s: &Session, at: [f64; 2], options: liquify::Options) -> Vec<LayerId> {
    let Some(st) = s.active() else {
        return Vec::new();
    };
    let selected = st.selected_layers();
    if !selected.is_empty() {
        return selected;
    }
    let bb = options.brush_bounds(photocraft_pathops::geom::Point::new(at[0], at[1]));
    target_layers(s)
        .unwrap_or_default()
        .into_iter()
        .filter(|id| {
            let Some(l) = st.doc.layer(*id) else {
                return false;
            };
            let LayerContent::Shape(sh) = &l.content else {
                return false;
            };
            if sh.live.is_none() && l.vector.live_shape.is_none() {
                return false;
            }
            photocraft_pathops::bounds(&sh.path)
                .ok()
                .flatten()
                .is_some_and(|(x0, y0, x1, y1)| photocraft_pathops::geom::Rect::new(x0, y0, x1, y1).inflate(1e-6, 1e-6).intersect(bb).area() > 0.)
        })
        .collect()
}
fn bases(s: &Session, c: &Capture) -> Result<Vec<ShapeEdit>> {
    c.validate(s)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut edits = vector_api::snapshot_shapes(doc, &c.layers)?;
    for e in &mut edits {
        let l = doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        if c.automatic && (e.shape.live.is_some() || e.vector.live_shape.is_some()) {
            e.shape = super::expand::project_shape_base(l)?;
        }
        // Bake evaluated corners before topology-changing distortions, then clear their indices.
        if !e.vector.corners.is_empty() {
            e.shape = super::corners::project_shape(&e.shape, &e.vector.corners)?;
        }
    }
    Ok(edits)
}
fn render(s: &Session, c: &Capture, paths: &[Path]) -> Result<Prepared> {
    let mut edits = bases(s, c)?;
    if edits.len() != paths.len() {
        return Err(error("distort result target set changed"));
    }
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    for (e, path) in edits.iter().zip(paths) {
        let l = doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        if c.automatic && e.shape.path != *path && (l.vector.live_shape.is_some() || matches!(&l.content,LayerContent::Shape(sh) if sh.live.is_some())) {
            return Err(error("Convert to curves before distorting this live shape"));
        }
    }
    edits = edits
        .into_iter()
        .zip(paths)
        .filter_map(|(mut e, path)| {
            if e.shape.path == *path {
                return None;
            }
            e.shape.path = path.clone();
            e.shape.live = None;
            e.shape.psd_raw = None;
            e.vector.corners.clear();
            e.vector.live_shape = None;
            Some(e)
        })
        .collect();
    for e in &edits {
        liquify::validate_path(&e.shape.path, liquify::MAX_NODES).map_err(error)?;
    }
    // Compile all and validate the aggregate allocation BEFORE rendering any target cache.
    let compiled = edits
        .iter()
        .map(|e| {
            CompiledAppearance::with_projection(&e.shape, e.vector.appearance.as_ref(), &doc.patterns, doc.bounds(), &super::expand::Projection).map_err(error)
        })
        .collect::<Result<Vec<_>>>()?;
    if compiled.iter().map(|p| p.cache_bytes(doc.pixel_format())).sum::<u128>() > MAX_CACHE_BYTES {
        return Err(error("distort caches exceed 256 MiB"));
    }
    let mut detached = (**doc).clone();
    let mut changed = Vec::new();
    for (mut e, compiled) in edits.into_iter().zip(compiled) {
        e.shape.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
        let l = detached.layer_mut(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
        l.content = LayerContent::Shape(e.shape.clone());
        l.vector = e.vector.clone();
        super::expand::clear_geometry_blocks(l);
        changed.push(e);
    }
    Ok(Prepared { capture: c.clone(), document: Arc::new(detached), edits: changed })
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub capture: Capture,
    pub document: Arc<Document>,
    pub edits: Vec<ShapeEdit>,
}
fn publish(s: &mut Session, prepared: Prepared, label: &str) -> Result<Value> {
    prepared.capture.validate(s)?;
    let layers: Vec<_> = prepared.edits.iter().map(|e| e.layer.0).collect();
    if !layers.is_empty() {
        // One transaction preserves the rest of each layer while clearing stale PSD geometry.
        let changes = prepared.edits;
        s.edit(label, move |doc, _| {
            for e in changes {
                let l = doc.layer_mut(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
                l.content = LayerContent::Shape(e.shape);
                l.vector = e.vector;
                super::expand::clear_geometry_blocks(l);
            }
            Ok(())
        })?;
    }
    Ok(json!({"layers":layers}))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiquifyRequest {
    pub capture: Capture,
    #[serde(default)]
    pub options: liquify::Options,
    pub samples: Vec<liquify::Sample>,
    #[serde(default)]
    pub seed: u64,
}
pub fn prepare_liquify(s: &Session, r: &LiquifyRequest) -> Result<Prepared> {
    let edits = bases(s, &r.capture)?;
    let paths = edits.iter().map(|e| liquify::apply(&e.shape.path, &r.samples, r.options, r.seed ^ e.layer.0).map_err(error)).collect::<Result<Vec<_>>>()?;
    render(s, &r.capture, &paths)
}
/// Growing strokes apply each dab prefix once. Release uses the same checked request as replay.
#[derive(Clone, Debug)]
pub struct LiquifyGesture {
    pub request: LiquifyRequest,
    /// Exact changed live targets from the last attempted dab, including interpolated dabs.
    pub conversion_layers: Vec<LayerId>,
    live_shapes: Vec<bool>,
    strokes: Vec<liquify::Stroke>,
}
impl LiquifyGesture {
    pub fn begin(s: &Session, ids: Vec<LayerId>, options: liquify::Options, seed: u64) -> Result<Self> {
        let capture = Capture::begin(s, ids)?;
        let edits = bases(s, &capture)?;
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let live_shapes = capture
            .layers
            .iter()
            .map(|id| doc.layer(*id).is_some_and(|l| l.vector.live_shape.is_some() || matches!(&l.content, LayerContent::Shape(sh) if sh.live.is_some())))
            .collect();
        let strokes = edits.into_iter().map(|e| liquify::Stroke::new(e.shape.path, options, seed ^ e.layer.0).map_err(error)).collect::<Result<Vec<_>>>()?;
        Ok(Self { request: LiquifyRequest { capture, options, samples: Vec::new(), seed }, strokes, live_shapes, conversion_layers: Vec::new() })
    }
    pub fn push(&mut self, s: &Session, sample: liquify::Sample) -> Result<Prepared> {
        self.request.capture.validate(s)?;
        let mut next = self.strokes.clone();
        for stroke in &mut next {
            stroke.push(sample).map_err(error)?;
        }
        let paths = next.iter().map(|stroke| stroke.path().map_err(error)).collect::<Result<Vec<_>>>()?;
        self.conversion_layers = paths
            .iter()
            .zip(&self.strokes)
            .zip(&self.live_shapes)
            .enumerate()
            .filter_map(|(i, ((path, stroke), live))| {
                (self.request.capture.automatic && *live && path != stroke.base()).then_some(self.request.capture.layers[i])
            })
            .collect();
        if !self.conversion_layers.is_empty() {
            return Err(error("Convert to curves before distorting this live shape"));
        }
        let preview = render(s, &self.request.capture, &paths)?;
        self.strokes = next;
        self.request.samples.push(sample);
        Ok(preview)
    }
    pub fn commit(self, s: &mut Session) -> Result<Value> {
        self.request.capture.validate(s)?;
        s.execute("object.liquify", serde_json::to_value(&self.request).map_err(error)?)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuppetRequest {
    pub capture: Capture,
    #[serde(default)]
    pub options: puppet::Options,
    /// Authoritative rest paths checked against the gesture's starting document in `PuppetGesture`.
    /// For automation, each request warps the current geometry; no arbitrary path replacement.
    pub pins: Vec<puppet::Pin>,
}
pub fn prepare_puppet(s: &Session, r: &PuppetRequest) -> Result<Prepared> {
    let edits = bases(s, &r.capture)?;
    let paths: Vec<_> = edits.iter().map(|e| e.shape.path.clone()).collect();
    let rig = puppet::Rig::new(&paths, r.options).map_err(error)?;
    let deformed = rig.deform(&r.pins).map_err(error)?;
    let paths = paths.iter().map(|p| rig.warp(p, &r.pins, &deformed).map_err(error)).collect::<Result<Vec<_>>>()?;
    render(s, &r.capture, &paths)
}
#[derive(Clone, Debug)]
pub struct PuppetGesture {
    pub capture: Capture,
    pub rig: puppet::Rig,
    paths: Vec<Path>,
}
impl PuppetGesture {
    pub fn begin(s: &Session, ids: Vec<LayerId>, options: puppet::Options) -> Result<Self> {
        let capture = Capture::begin(s, ids)?;
        let paths = bases(s, &capture)?.into_iter().map(|e| e.shape.path).collect::<Vec<_>>();
        let rig = puppet::Rig::new(&paths, options).map_err(error)?;
        Ok(Self { capture, rig, paths })
    }
    pub fn preview(&self, s: &Session, pins: &[puppet::Pin]) -> Result<Prepared> {
        self.capture.validate(s)?;
        let deformed = self.rig.deform(pins).map_err(error)?;
        let paths = self.paths.iter().map(|p| self.rig.warp(p, pins, &deformed).map_err(error)).collect::<Result<Vec<_>>>()?;
        render(s, &self.capture, &paths)
    }
    pub fn commit(&self, s: &mut Session, pins: Vec<puppet::Pin>) -> Result<Value> {
        self.capture.validate(s)?;
        s.execute("object.puppetWarp", serde_json::to_value(PuppetRequest { capture: self.capture.clone(), options: self.rig.options, pins }).map_err(error)?)
    }
}
/// Conversion is a separately confirmed, single undoable edit; it never occurs on a brush dab.
pub fn prepare_conversion(s: &Session, c: &Capture) -> Result<Document> {
    let guard = super::path_commands::Capture { document: c.document, revision: c.revision, layers: c.layers.clone() };
    guard.check(s)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut ids = Vec::new();
    fn leaves(l: &Layer, ids: &mut Vec<LayerId>) -> Result<()> {
        match &l.content {
            LayerContent::Shape(_) | LayerContent::Text(_) => ids.push(l.id),
            LayerContent::Group(g) if l.vector.live.is_none() => {
                for child in &g.children {
                    leaves(child, ids)?;
                }
            }
            _ => return Err(error("conversion requires shape or text artwork")),
        }
        Ok(())
    }
    for id in &c.layers {
        leaves(source.layer(*id).ok_or(EngineError::NoLayer(*id))?, &mut ids)?;
    }
    if ids.len() > MAX_TARGETS {
        return Err(error("conversion exceeds target budget"));
    }
    for (i, id) in ids.iter().enumerate() {
        let locks = source.effective_locks(*id);
        if locks.all || locks.pixels || locks.position || ids[..i].contains(id) {
            return Err(error("conversion target is locked or repeated"));
        }
    }
    let mut scratch = Session::new();
    scratch.add_document((**source).clone(), None);
    for id in &ids {
        let l = source.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        if let LayerContent::Text(t) = &l.content {
            super::path_text::on_path::validate_text(t).map_err(error)?;
            if l.vector.on_path.is_some() {
                scratch.execute(
                    "type.path.convert",
                    json!({"document":source.id.0,"revision":scratch.active().ok_or(EngineError::NoDocument)?.revision,"layers":[id.0]}),
                )?;
            } else {
                scratch.execute("type.convertToShape", json!({"layer":id.0}))?;
            }
        } else {
            let shape = super::expand::project_shape_base(l)?;
            scratch.edit("Convert to curves", move |doc, _| {
                let l = doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?;
                l.content = LayerContent::Shape(shape);
                l.vector.live_shape = None;
                l.vector.corners.clear();
                Ok(())
            })?;
        }
    }
    let mut detached = (*scratch.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let mut bytes = 0;
    for id in ids {
        let mut l = detached.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
        super::expand::clear_geometry_blocks(&mut l);
        super::expand::refresh_tree(source, &mut l, &mut bytes)?;
        *detached.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
    }
    Ok(detached)
}
fn convert(s: &mut Session, v: &Value) -> Result<Value> {
    let c: Capture = decode(v)?;
    let detached = prepare_conversion(s, &c)?;
    let guard = super::path_commands::Capture { document: c.document, revision: c.revision, layers: c.layers.clone() };
    guard.check(s)?;
    s.edit("Convert to curves", move |doc, _| {
        *doc = detached;
        Ok(())
    })?;
    Ok(json!({"layers":c.layers}))
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(error)
}
fn liquify_commit(s: &mut Session, v: &Value) -> Result<Value> {
    let r: LiquifyRequest = decode(v)?;
    publish(s, prepare_liquify(s, &r)?, r.options.kind.label())
}
fn puppet_commit(s: &mut Session, v: &Value) -> Result<Value> {
    let r = decode(v)?;
    publish(s, prepare_puppet(s, &r)?, "Puppet Warp")
}
fn liquify_preview(s: &mut Session, v: &Value) -> Result<Value> {
    let p = prepare_liquify(s, &decode(v)?)?;
    Ok(json!({"detached":true,"paths":p.edits.iter().map(|e| (&e.layer,&e.shape.path)).collect::<Vec<_>>() }))
}
fn puppet_preview(s: &mut Session, v: &Value) -> Result<Value> {
    let p = prepare_puppet(s, &decode(v)?)?;
    Ok(json!({"detached":true,"paths":p.edits.iter().map(|e| (&e.layer,&e.shape.path)).collect::<Vec<_>>() }))
}
fn puppet_info(s: &mut Session, v: &Value) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Info {
        capture: Capture,
        #[serde(default)]
        options: puppet::Options,
    }
    let r: Info = decode(v)?;
    let g = PuppetGesture::begin(s, r.capture.layers.clone(), r.options)?;
    r.capture.validate(s)?;
    Ok(json!({"pins":g.rig.automatic_pins(),"vertices":g.rig.mesh.verts.len(),"triangles":g.rig.mesh.tris.len()}))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("object.distort.convert", "Convert to curves", convert as fn(&mut Session, &Value) -> Result<Value>, true),
        ("object.liquify", "Warp", liquify_commit as fn(&mut Session, &Value) -> Result<Value>, true),
        ("object.liquify.preview", "Liquify Preview", liquify_preview, false),
        ("object.puppetWarp", "Puppet Warp", puppet_commit, true),
        ("object.puppetWarp.preview", "Puppet Warp Preview", puppet_preview, false),
        ("object.puppetWarp.info", "Puppet Warp Pins", puppet_info, false),
    ]
    .into_iter()
    .map(|(id, label, run, journal)| CommandSpec {
        id,
        label,
        run,
        journal,
        menu: &[],
        shortcut: None,
        params: match id {
            "object.distort.convert" => "{document,revision,layers}",
            "object.liquify" | "object.liquify.preview" => "{capture:{document,revision,layers,automatic?},options?,samples,seed?}",
            "object.puppetWarp.info" => "{capture:{document,revision,layers,automatic?},options?}",
            _ => "{capture:{document,revision,layers,automatic?},options?,pins}",
        },
        enabled: crate::vector_cmds::has_doc,
    })
    .collect()
}
#[cfg(test)]
#[path = "distort_tools/tests.rs"]
mod tests;
