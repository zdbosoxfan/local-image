//! Live envelopes: detached group projection, captured gestures and one atomic edit.
//! Sources remain native shape/type children. Display/export use the same pure evaluator.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{
    Document, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer,
    vector_design::live::{EnvelopeSpec, LiveGroup},
};
use photocraft_pathops::geom::Rect;
pub use photocraft_vecfx::envelope as geometry;
use photocraft_vector::appearance::CompiledAppearance;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
#[path = "envelope/outlines.rs"]
mod outlines;
const MAX_CACHE_BYTES: u128 = 256 << 20;
fn error(e: impl ToString) -> EngineError {
    EngineError::Other(e.to_string())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub group: LayerId,
    /// Every descendant captured at Begin, including nested groups.
    pub sources: Vec<LayerId>,
    pub spec: EnvelopeSpec,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub document: u64,
    pub revision: u64,
    pub edits: Vec<Edit>,
}
fn descendants(layer: &Layer, depth: usize, out: &mut Vec<LayerId>) -> Result<()> {
    if depth > photocraft_doc::MAX_GROUP_DEPTH || out.len() >= 1024 {
        return Err(error("envelope target budget exceeded"));
    }
    out.push(layer.id);
    if let Some(c) = layer.children() {
        for l in c {
            descendants(l, depth + 1, out)?;
        }
    }
    Ok(())
}
fn targets(doc: &Document, id: LayerId, editing: bool) -> Result<Vec<LayerId>> {
    let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Group(g) = &layer.content else { return Err(error("envelope needs a group")) };
    if g.children.is_empty() {
        return Err(error("envelope needs source children"));
    }
    if layer.vector.live.as_ref().is_some_and(|v| !matches!(v, LiveGroup::Envelope(_))) {
        return Err(error("expand the existing live group first"));
    }
    let mut ids = Vec::new();
    descendants(layer, 0, &mut ids)?;
    for id in &ids {
        let locks = doc.effective_locks(*id);
        if editing && (locks.all || locks.pixels || locks.position) {
            return Err(error("envelope source or group is locked"));
        }
        let l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        if *id != layer.id && l.vector.live.is_some() {
            return Err(error("nested live groups require expansion before enveloping"));
        }
        if !matches!(l.content, LayerContent::Group(_) | LayerContent::Shape(_) | LayerContent::Text(_)) {
            return Err(error("envelope supports path and type sources"));
        }
        if l.vector.clip.is_some() || l.vector_mask.is_some() || l.mask.is_some() {
            return Err(error("envelope masked sources require the clipping projection provider"));
        }
    }
    Ok(ids)
}
pub fn check_fresh(s: &Session, c: &Commit) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != c.document || st.revision != c.revision {
        return Err(error("stale envelope: document or revision changed"));
    }
    Ok(())
}
fn preflight(doc: &Document, c: &Commit, editing: bool) -> Result<()> {
    if c.edits.is_empty() || c.edits.len() > 32 {
        return Err(error("invalid envelope group count"));
    }
    let mut seen = HashSet::new();
    for e in &c.edits {
        let ids = targets(doc, e.group, editing)?;
        if ids != e.sources {
            return Err(error("envelope source set changed"));
        }
        for id in ids {
            if !seen.insert(id) {
                return Err(error("duplicate or overlapping envelope targets"));
            }
        }
    }
    Ok(())
}
#[derive(Clone)]
struct Piece {
    source: LayerId,
    shape: ShapeLayer,
    vector: photocraft_doc::VectorData,
}
fn collect(doc: &Document, l: &Layer, out: &mut Vec<Piece>) -> Result<()> {
    match &l.content {
        LayerContent::Group(g) => {
            for l in &g.children {
                collect(doc, l, out)?;
            }
        }
        LayerContent::Shape(sh) => {
            let mut shape = sh.clone();
            if let Some(d) = &l.vector.live_shape {
                shape.path = crate::vector_packages::drawing::geometry::generate(d).map_err(error)?;
            }
            if !l.vector.corners.is_empty() {
                shape = crate::vector_packages::corners::project_shape(&shape, &l.vector.corners)?;
            }
            let mut vector = l.vector.clone();
            vector.live_shape = None;
            vector.corners.clear();
            out.push(Piece { source: l.id, shape, vector });
        }
        LayerContent::Text(t) => {
            let mut checked = t.clone();
            if l.vector.on_path.is_none() {
                checked.warp = None;
            }
            crate::vector_packages::path_text::on_path::validate_text(&checked).map_err(error)?;
            if t.warp
                .as_ref()
                .is_some_and(|w| ![w.value, w.horizontal_distortion, w.vertical_distortion].into_iter().all(|v| v.is_finite() && (-100.0..=100.0).contains(&v)))
            {
                return Err(error("invalid source text warp"));
            }
            let mut engine = photocraft_text::shared().lock().map_err(error)?;
            let paints = if let Some(p) = &l.vector.on_path {
                let layout = crate::vector_packages::path_text::project(&mut engine, doc, t, p)?;
                crate::vector_packages::path_text::on_path::validated_outlines(&layout, &t.transform).map_err(error)?
            } else {
                let layout = engine.layout(t, doc.resolution_dpi);
                if layout.glyphs.len() > 8192 {
                    return Err(error("envelope glyph budget exceeded"));
                }
                let warp = photocraft_text::render::layout_warp(&layout, t.warp.as_ref());
                let mut paints = Vec::new();
                let mut one = layout.clone();
                one.glyphs.clear();
                one.decorations.clear();
                for g in &layout.glyphs {
                    let style = layout.styles.get(g.style as usize).ok_or_else(|| error("invalid glyph style"))?;
                    let face = layout.faces.get(g.face as usize).ok_or_else(|| error("invalid glyph face"))?;
                    let radius = f64::from(face.size_px * photocraft_text::render::FAUX_BOLD_RADIUS);
                    let offsets: &[(f64, f64)] = if style.faux_bold || face.embolden {
                        &[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)]
                    } else {
                        &[(0.0, 0.0)]
                    };
                    one.glyphs.clear();
                    one.glyphs.push(*g);
                    for &(x, y) in offsets {
                        let post = t.transform.mul(&photocraft_geom::Affine::translate(x * radius, y * radius));
                        for elements in photocraft_text::render::outlines(&one, &post, warp.as_ref()) {
                            paints.push(crate::vector_packages::path_text::on_path::PaintedOutline { color: style.color, elements });
                        }
                    }
                }
                for d in &layout.decorations {
                    let mut elements = Vec::new();
                    let corners = [(d.x0, d.y0), (d.x1, d.y0), (d.x1, d.y1), (d.x0, d.y1)];
                    for (i, (x, y)) in corners.into_iter().enumerate() {
                        let (x, y) = warp.as_ref().map_or((f64::from(x), f64::from(y)), |w| w.apply(f64::from(x), f64::from(y)));
                        let p = t.transform.apply(photocraft_geom::Point::new(x, y));
                        elements.push(if i == 0 {
                            photocraft_text::render::PathEl::MoveTo([p.x, p.y])
                        } else {
                            photocraft_text::render::PathEl::LineTo([p.x, p.y])
                        });
                    }
                    elements.push(photocraft_text::render::PathEl::Close);
                    paints.push(crate::vector_packages::path_text::on_path::PaintedOutline { color: layout.styles[d.style as usize].color, elements });
                }
                paints
            };
            for (color, path) in outlines::paths(&paints) {
                let mut vector = l.vector.clone();
                vector.on_path = None;
                vector.live_shape = None;
                vector.corners.clear();
                out.push(Piece { source: l.id, shape: ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() }, vector });
            }
        }
        _ => return Err(error("unsupported envelope source")),
    }
    if out.len() > 1024 {
        return Err(error("envelope piece budget exceeded"));
    }
    Ok(())
}
fn paths(pieces: &[Piece]) -> Vec<Path> {
    pieces.iter().map(|p| p.shape.path.clone()).collect()
}
fn pieces(doc: &Document, id: LayerId) -> Result<Vec<Piece>> {
    let mut out = Vec::new();
    collect(doc, doc.layer(id).ok_or(EngineError::NoLayer(id))?, &mut out)?;
    Ok(out)
}
pub fn source_bounds(doc: &Document, id: LayerId) -> Result<Rect> {
    geometry::source_bounds(&paths(&pieces(doc, id)?)).map_err(error)
}
fn replace_projection(layer: &mut Layer, projected: &[Piece], next_id: &mut usize, used: &mut HashSet<LayerId>) -> Result<()> {
    match &mut layer.content {
        LayerContent::Group(g) => {
            for l in &mut g.children {
                replace_projection(l, projected, next_id, used)?;
            }
        }
        LayerContent::Shape(_) => {
            let p = projected.iter().find(|p| p.source == layer.id).ok_or_else(|| error("missing envelope shape projection"))?;
            layer.content = LayerContent::Shape(p.shape.clone());
            layer.vector = p.vector.clone();
        }
        LayerContent::Text(_) => {
            let mut children = Vec::new();
            for p in projected.iter().filter(|p| p.source == layer.id) {
                // Transient IDs are deterministic and disjoint from captured source IDs. Expand
                // creates fresh native IDs later; repeated previews never allocate global IDs.
                let mut child = layer.clone();
                loop {
                    let id = LayerId(u64::MAX - *next_id as u64);
                    *next_id += 1;
                    if used.insert(id) {
                        child.id = id;
                        break;
                    }
                }
                child.name = format!("{} outline", layer.name);
                child.content = LayerContent::Shape(p.shape.clone());
                child.vector = p.vector.clone();
                child.vector.constraints = None;
                child.opacity = 1.0;
                child.fill_opacity = 1.0;
                child.clipped = false;
                child.mask = None;
                child.vector_mask = None;
                child.blend_if = Default::default();
                child.excluded_channels = 0;
                child.psd_id = None;
                child.link_group = None;
                child.fill_cache = None;
                child.blend = photocraft_color::BlendMode::Normal;
                child.effects = Default::default();
                child.psd_blocks.clear();
                children.push(child);
            }
            layer.content = LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None });
            layer.vector.on_path = None;
            layer.psd_blocks.retain(|(k, _)| k != b"TySh");
        }
        _ => return Err(error("unsupported envelope projection")),
    }
    Ok(())
}
fn project_group(doc: &Document, id: LayerId, spec: &EnvelopeSpec, bytes: &mut u128, next_id: &mut usize, used: &mut HashSet<LayerId>) -> Result<Layer> {
    let mut projected = pieces(doc, id)?;
    let output = geometry::evaluate(&paths(&projected), spec, geometry::Options::default()).map_err(error)?;
    for (piece, path) in projected.iter_mut().zip(output) {
        piece.shape.path = path;
        piece.shape.live = None;
        piece.shape.psd_raw = None;
        piece.shape.cache = None;
    }
    // All target paths/options validate before cache allocation. Appearance's existing
    // provider rejects unsupported stroke projections instead of rendering them incorrectly.
    let compiled = projected
        .iter()
        .map(|p| CompiledAppearance::new(&p.shape, p.vector.appearance.as_ref(), &doc.patterns, doc.bounds()).map_err(error))
        .collect::<Result<Vec<_>>>()?;
    let additional = compiled.iter().map(|c| c.cache_bytes(doc.pixel_format())).sum::<u128>();
    *bytes += additional;
    if *bytes > MAX_CACHE_BYTES {
        return Err(error("envelope caches exceed 256 MiB"));
    }
    for (p, c) in projected.iter_mut().zip(compiled) {
        p.shape.cache = Some(c.render(doc.pixel_format(), doc.bounds()).map_err(error)?);
    }
    let mut group = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
    group.vector.live = None;
    replace_projection(&mut group, &projected, next_id, used)?;
    Ok(group)
}
fn prepare(doc: &Document, c: &Commit, editing: bool) -> Result<Vec<Layer>> {
    preflight(doc, c, editing)?;
    // Evaluate the entire target set first, before rendering the first group.
    for e in &c.edits {
        let p = pieces(doc, e.group)?;
        geometry::evaluate(&paths(&p), &e.spec, geometry::Options::default()).map_err(error)?;
    }
    let mut bytes = 0;
    let mut next_id = 0;
    let mut used = doc.walk().into_iter().map(|(_, _, l)| l.id).collect::<HashSet<_>>();
    c.edits.iter().map(|e| project_group(doc, e.group, &e.spec, &mut bytes, &mut next_id, &mut used)).collect()
}
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    check_fresh(s, c)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let projected = prepare(source, c, true)?;
    // Preserve other committed envelopes in the detached preview. The captured groups
    // are replaced below, so do not evaluate their previous live parameters a second time.
    let mut background = source.as_ref().clone();
    for e in &c.edits {
        background.layer_mut(e.group).ok_or(EngineError::NoLayer(e.group))?.vector.live = None;
    }
    let mut doc = project_document(&background)?;
    for group in projected {
        let id = group.id;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = group;
    }
    Ok(doc)
}
/// Committed display/export projection, revision-keyed by callers. No source mutation.
pub fn project_document(source: &Document) -> Result<Document> {
    let edits = source
        .walk()
        .into_iter()
        .filter_map(|(_, _, l)| {
            if let Some(LiveGroup::Envelope(spec)) = &l.vector.live { Some(Edit { group: l.id, sources: Vec::new(), spec: spec.clone() }) } else { None }
        })
        .map(|mut e| {
            e.sources = targets(source, e.group, false)?;
            Ok(e)
        })
        .collect::<Result<Vec<_>>>()?;
    if edits.is_empty() {
        return Ok(source.clone());
    }
    let c = Commit { document: source.id.0, revision: 0, edits };
    let projected = prepare(source, &c, false)?;
    let mut doc = source.clone();
    for g in projected {
        let id = g.id;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = g;
    }
    Ok(doc)
}
#[derive(Clone, Debug)]
pub struct Gesture {
    pub request: Commit,
    base: Arc<Document>,
}
impl Gesture {
    pub fn begin(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let edits = ids
            .iter()
            .map(|id| {
                let sources = targets(&st.doc, *id, true)?;
                let layer = st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
                let spec =
                    if let Some(LiveGroup::Envelope(spec)) = &layer.vector.live { spec.clone() } else { geometry::identity_mesh(2, 2).map_err(error)? };
                Ok(Edit { group: *id, sources, spec })
            })
            .collect::<Result<Vec<_>>>()?;
        let request = Commit { document: st.doc.id.0, revision: st.revision, edits };
        preflight(&st.doc, &request, true)?;
        Ok(Self { request, base: st.doc.clone() })
    }
    pub fn is_current(&self, s: &Session) -> bool {
        check_fresh(s, &self.request).is_ok()
    }
    pub fn update(&mut self, spec: EnvelopeSpec) -> Result<()> {
        for e in &self.request.edits {
            let paths = paths(&pieces(&self.base, e.group)?);
            geometry::evaluate(&paths, &spec, geometry::Options::default()).map_err(error)?;
        }
        for e in &mut self.request.edits {
            e.spec = spec.clone();
        }
        Ok(())
    }
    pub fn bounds(&self) -> Result<Rect> {
        source_bounds(&self.base, self.request.edits.first().ok_or_else(|| error("empty envelope gesture"))?.group)
    }
    pub fn commit(self, s: &mut Session) -> Result<Value> {
        s.execute("vector.envelope.set", serde_json::to_value(self.request).map_err(error)?)
    }
}
fn decode(v: &Value) -> Result<Commit> {
    serde_json::from_value(v.clone()).map_err(error)
}
fn set(s: &mut Session, v: &Value) -> Result<Value> {
    let c = decode(v)?;
    check_fresh(s, &c)?;
    let source = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let _prepared = prepare(&source, &c, true)?;
    let ids = c.edits.iter().map(|e| e.group.0).collect::<Vec<_>>();
    s.edit("Edit Envelope", move |doc, _| {
        for e in c.edits {
            doc.layer_mut(e.group).ok_or(EngineError::NoLayer(e.group))?.vector.live = Some(LiveGroup::Envelope(e.spec));
        }
        Ok(())
    })?;
    Ok(json!({"groups":ids}))
}
fn preview(s: &mut Session, v: &Value) -> Result<Value> {
    let c = decode(v)?;
    let doc = preview_document(s, &c)?;
    Ok(
        json!({"detached":true,"groups":c.edits.iter().map(|e|e.group.0).collect::<Vec<_>>(),"paths":doc.walk().into_iter().filter_map(|(_,_,l)|if let LayerContent::Shape(sh)=&l.content{Some(json!({"layer":l.id.0,"path":sh.path}))}else{None}).collect::<Vec<_>>()}),
    )
}
fn fresh_ids(l: &mut Layer, original: &HashSet<LayerId>) {
    // Allocate only on successful Expand's prepared clone, before the atomic edit.
    if !original.contains(&l.id) {
        l.id = photocraft_doc::Layer::new("", LayerContent::Shape(Default::default())).id;
    }
    if let Some(c) = l.children_mut() {
        for l in c {
            fresh_ids(l, original);
        }
    }
}
fn expand(s: &mut Session, v: &Value) -> Result<Value> {
    let c = decode(v)?;
    check_fresh(s, &c)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    for e in &c.edits {
        if doc.layer(e.group).and_then(|l| l.vector.live.as_ref()) != Some(&LiveGroup::Envelope(e.spec.clone())) {
            return Err(error("Expand requires the captured live envelope"));
        }
    }
    let original = doc.walk().into_iter().map(|(_, _, l)| l.id).collect::<HashSet<_>>();
    let mut groups = prepare(doc, &c, true)?;
    for l in &mut groups {
        fresh_ids(l, &original);
    }
    s.edit("Expand Envelope", move |doc, _| {
        for l in groups {
            let id = l.id;
            *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
        }
        Ok(())
    })?;
    Ok(json!({"expanded":true}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("vector.envelope.set", set as fn(&mut Session, &Value) -> Result<Value>, true),
        ("vector.envelope.preview", preview, false),
        ("vector.envelope.expand", expand, true),
    ]
    .into_iter()
    .map(|(id, run, journal)| CommandSpec {
        id,
        label: "Envelope",
        menu: &[],
        shortcut: None,
        params: "{document,revision,edits:[{group,sources:[id],spec:EnvelopeSpec}]}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
#[cfg(test)]
#[path = "envelope/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "envelope/tests.rs"]
mod tests;
