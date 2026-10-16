//! Unified detached expansion and one-transaction publication. Dispatch semantics informed by
//! VectorCraft cmd/expand.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
use super::path_commands::{Capture, MAX_CACHE_BYTES, Prepared, bad, validate_path};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::{
    appearance::{AppearanceItem, StrokeItem},
    live::LiveGroup,
};
use photocraft_doc::{Document, Fill, Layer, LayerContent, Path, ShapeLayer};
use photocraft_vector::appearance::{self as renderer, CompiledAppearance, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub object: bool,
    pub fill: bool,
    pub stroke: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { object: true, fill: true, stroke: true }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    #[serde(default)]
    pub options: Options,
    #[serde(default)]
    pub appearance: bool,
}
/// Integrators use this provider for appearance previews of width and brush strokes too.
pub struct Projection;
impl StrokeProjection for Projection {
    fn outline(&self, path: &Path, stroke: &StrokeItem) -> std::result::Result<Path, String> {
        if stroke.brush.is_some() {
            super::brushes::geometry::outline(path, stroke).map_err(|e| e.to_string())
        } else {
            super::width::geometry::outline(path, stroke).map_err(|e| e.to_string())
        }
    }
}
pub(crate) fn clear_geometry_blocks(layer: &mut Layer) {
    layer.psd_blocks.retain(|(key, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&key));
    layer.psd_id = None;
}
/// Uses live-shape and corner owners in their prescribed order. Paint stays in source space.
pub fn project_shape_base(layer: &Layer) -> Result<ShapeLayer> {
    let LayerContent::Shape(source) = &layer.content else {
        return Err(bad("requires shape geometry"));
    };
    if layer.vector.live.is_some() || layer.vector.on_path.is_some() {
        return Err(bad("shape has incompatible live metadata"));
    }
    if layer.vector.clip.is_some() {
        return Err(bad("vector clip projection requires the integrated clip provider (35)"));
    }
    let mut shape = source.clone();
    if let Some(design) = &layer.vector.live_shape {
        shape.path = super::drawing::geometry::generate(design).map_err(|e| bad(e.to_string()))?;
    }
    if !layer.vector.corners.is_empty() {
        shape = super::corners::project_shape(&shape, &layer.vector.corners)?;
    }
    validate_path(&shape.path)?;
    shape.live = None;
    shape.psd_raw = None;
    Ok(shape)
}
pub(crate) fn refresh_tree(doc: &Document, layer: &mut Layer, bytes: &mut u128) -> Result<()> {
    if let LayerContent::Shape(sh) = &mut layer.content {
        validate_path(&sh.path)?;
        let compiled = CompiledAppearance::with_projection(sh, layer.vector.appearance.as_ref(), &doc.patterns, doc.bounds(), &Projection).map_err(bad)?;
        *bytes = bytes.checked_add(compiled.cache_bytes(doc.pixel_format())).ok_or_else(|| bad("cache budget overflow"))?;
        if *bytes > MAX_CACHE_BYTES {
            return Err(bad("Object command caches exceed 512 MiB"));
        }
        sh.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(bad)?);
        sh.psd_raw = None;
    }
    if let Some(children) = layer.children_mut() {
        for child in children {
            refresh_tree(doc, child, bytes)?;
        }
    }
    Ok(())
}
fn outline(path: &Path, stroke: &StrokeItem) -> Result<Path> {
    if stroke.start_arrow.is_some() || stroke.end_arrow.is_some() || stroke.dash_align || stroke.gradient_mode != Default::default() {
        return Err(bad("arrowheads, aligned dashes and stroke gradient expansion require the appearance owner (08)"));
    }
    if !matches!(stroke.base.paint, Fill::Solid(_)) {
        return Err(bad("stroke gradient/pattern expansion requires paint-coordinate baking (08)"));
    }
    if stroke.profile.is_some() || stroke.brush.is_some() {
        Projection.outline(path, stroke).map_err(bad)
    } else {
        photocraft_pathops::outline_stroke(path, &stroke.base, photocraft_vector::DEFAULT_TOLERANCE).map_err(|e| bad(e.to_string()))
    }
}
fn shape_expansion(layer: &Layer, options: Options, appearance: bool) -> Result<Layer> {
    let source = match &layer.content {
        LayerContent::Shape(sh) => sh,
        _ => return Err(bad("requires shape")),
    };
    if !options.object
        && !appearance
        && (layer.vector.live_shape.is_some() || !layer.vector.corners.is_empty() || source.live.is_some() || layer.vector.on_path.is_some())
    {
        return Err(bad("select Object to expand live shape/corner geometry"));
    }
    let mut sh = if options.object || appearance { project_shape_base(layer)? } else { source.clone() };
    let a = layer.vector.appearance.clone().unwrap_or_else(|| renderer::legacy_appearance(&sh));
    renderer::validate(&a).map_err(bad)?;
    let mut children = Vec::new();
    for item in &a.items {
        let (path, fill, stroke, opacity, blend, visible) = match item {
            AppearanceItem::Mesh(mesh) => {
                let mut child = Layer::new(layer.name.clone(), LayerContent::Shape(ShapeLayer { path: sh.path.clone(), ..Default::default() }));
                child.vector.appearance = Some(photocraft_doc::vector_design::appearance::Appearance { items: vec![AppearanceItem::Mesh(mesh.clone())] });
                children.push(child);
                continue;
            }
            AppearanceItem::Fill(f) => (sh.path.clone(), Some(f.paint.clone()), None, f.opacity, f.blend, f.visible),
            AppearanceItem::Stroke(st) => {
                if appearance || options.stroke {
                    (outline(&sh.path, st)?, Some(st.base.paint.clone()), None, st.base.opacity, st.blend, st.visible)
                } else {
                    if st.profile.is_some()
                        || st.brush.is_some()
                        || st.start_arrow.is_some()
                        || st.end_arrow.is_some()
                        || st.dash_align
                        || st.gradient_mode != Default::default()
                    {
                        return Err(bad("select Stroke to expand advanced strokes"));
                    }
                    let mut base = st.base.clone();
                    base.opacity = 1.;
                    (sh.path.clone(), None, Some(base), st.base.opacity, st.blend, st.visible)
                }
            }
        };
        // Invisible paints remain invisible ordinary paths, preserving the full paint order.
        let child_shape = ShapeLayer { path, fill, stroke, ..Default::default() };
        let mut child = Layer::new(layer.name.clone(), LayerContent::Shape(child_shape));
        child.opacity = opacity;
        child.blend = blend;
        child.visible = visible;
        children.push(child);
    }
    let mut out = layer.clone();
    if appearance || options.fill || options.stroke {
        out.content = LayerContent::Group(photocraft_doc::Group { children, expanded: true, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto });
        out.vector.appearance = None;
    } else {
        sh.cache = None;
        out.content = LayerContent::Shape(sh);
    }
    if options.object || appearance {
        out.vector.corners.clear();
        out.vector.live_shape = None;
    }
    clear_geometry_blocks(&mut out);
    Ok(out)
}
#[cfg(test)]
thread_local! {
    static PREPARE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct Availability {
    source: Weak<Document>,
    request: Request,
    reason: Option<String>,
}
thread_local! {
    // One entry per UI thread; no prepared raster caches or source document are retained.
    static AVAILABILITY: RefCell<Option<Availability>> = const { RefCell::new(None) };
}
/// Preserve the exact preparation failure reason, but compute it once per captured
/// document snapshot, revision, selection and options. Publication still prepares anew.
pub fn disabled_reason(s: &Session, request: &Request) -> Option<String> {
    if let Err(e) = request.capture.check(s) {
        return Some(e.to_string());
    }
    let source = match s.active() {
        Some(st) => &st.doc,
        None => return Some(EngineError::NoDocument.to_string()),
    };
    AVAILABILITY.with(|cache| {
        if let Some(entry) = cache.borrow().as_ref()
            && entry.request == *request
            && entry.source.ptr_eq(&Arc::downgrade(source))
        {
            return entry.reason.clone();
        }
        let reason = prepare(s, request).err().map(|e| e.to_string());
        *cache.borrow_mut() = Some(Availability { source: Arc::downgrade(source), request: request.clone(), reason: reason.clone() });
        reason
    })
}

fn expand_layer(doc: &Document, source: &Layer, options: Options, appearance: bool, depth: usize) -> Result<Layer> {
    if depth > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(bad("expansion exceeds group depth"));
    }
    if source.vector.clip.is_some() {
        return Err(bad("vector clip projection requires the integrated clip provider (35)"));
    }
    if matches!(source.content, LayerContent::Group(_))
        && (source.vector.appearance.is_some() || source.vector.live_shape.is_some() || !source.vector.corners.is_empty())
    {
        return Err(bad("group has incompatible shape appearance metadata"));
    }
    if !options.object && !appearance && (source.vector.live.is_some() || source.vector.on_path.is_some()) {
        return Err(bad("select Object to expand live or path-text geometry"));
    }
    let mut l = source.clone();
    if options.object || appearance {
        if !l.vector.effects.is_empty() {
            let scene = super::path_effects::project(&l, &l.vector.effects, doc.resolution_dpi)?;
            l = super::path_effects::projected_layer(&l, &scene, doc)?;
        }
        match &source.vector.live {
            Some(LiveGroup::Blend(_) | LiveGroup::Envelope(_) | LiveGroup::Repeat(_) | LiveGroup::LivePaint(_)) => {
                let mut selected = doc.clone();
                selected.layers = vec![source.clone()];
                let projected = crate::vector_integration::project_document(&selected)?;
                l = projected.layers.into_iter().next().ok_or_else(|| bad("repeat returned no layers"))?;
                if let Some(children) = l.children_mut() {
                    for child in children {
                        *child = child.duplicate();
                    }
                }
            }
            None => {}
        }
        if matches!(l.content, LayerContent::Symbol(_)) {
            let ex = super::symbols::expansion::expand(doc, &[l]).map_err(|e| bad(e.to_string()))?;
            l = super::symbols::prepare_expansion(doc, ex)?.into_iter().next().ok_or_else(|| bad("symbol returned no layers"))?;
        }
        if l.vector.on_path.is_some() {
            // The owner exposes a public command rather than a layer expander. Invoke it in a
            // detached session: all owner history/journals are discarded, never nested in ours.
            let mut scratch = Session::new();
            let mut isolated = doc.clone();
            isolated.layers = vec![l.clone()];
            scratch.add_document(isolated, None);
            let st = scratch.active().ok_or(EngineError::NoDocument)?;
            let params = json!({"document":st.doc.id.0,"revision":st.revision,"layers":[l.id.0]});
            scratch.execute("type.path.convert", params)?;
            l = scratch.active().ok_or(EngineError::NoDocument)?.doc.layer(l.id).ok_or(EngineError::NoLayer(l.id))?.clone();
        }
    }
    if let LayerContent::Group(group) = &mut l.content {
        let mut children = Vec::new();
        for child in &group.children {
            children.push(expand_layer(doc, child, options, appearance, depth + 1)?);
        }
        group.children = children;
        if options.object || appearance {
            l.vector.live = None;
        }
        Ok(l)
    } else if matches!(l.content, LayerContent::Shape(_)) {
        shape_expansion(&l, options, appearance)
    } else {
        Err(bad("this object's public path expander is unavailable (12/42/45)"))
    }
}
pub fn prepare(s: &Session, request: &Request) -> Result<Prepared> {
    #[cfg(test)]
    PREPARE_COUNT.with(|count| count.set(count.get() + 1));
    request.capture.check(s)?;
    if !request.appearance && !request.options.object && !request.options.fill && !request.options.stroke {
        return Err(bad("choose an Expand option"));
    }
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let mut doc = (**source).clone();
    let options = if request.appearance { Options::default() } else { request.options };
    // Prepare ALL target trees before publishing anything.
    for id in &request.capture.layers {
        let original = source.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        let depth = source.path_of(*id).ok_or(EngineError::NoLayer(*id))?.len();
        let l = expand_layer(source, original, options, request.appearance, depth)?;
        *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l;
    }
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(bad("expansion exceeds group depth"));
    }
    // Validate/expand every target before our own ordinary-path cache allocation. Owners
    // which return prepared projections retain their internal preflight/budget checks.
    let mut bytes = 0;
    for id in &request.capture.layers {
        let mut l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
        refresh_tree(&doc, &mut l, &mut bytes)?;
        *doc.layer_mut(*id).ok_or(EngineError::NoLayer(*id))? = l;
    }
    Ok(Prepared { document: doc, layers: request.capture.layers.clone(), original_points: 0, new_points: 0 })
}
fn read(v: &Value) -> Result<Request> {
    serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))
}
fn commit(s: &mut Session, v: &Value) -> Result<Value> {
    let r = read(v)?;
    let prepared = prepare(s, &r)?;
    super::path_commands::publish(s, &r.capture, if r.appearance { "Expand Appearance" } else { "Expand" }, prepared)
}
fn info(s: &mut Session, v: &Value) -> Result<Value> {
    let r = read(v)?;
    let reason = disabled_reason(s, &r);
    Ok(json!({"enabled":reason.is_none(),"reason":reason}))
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "object.expand",
            label: "Expand",
            menu: &[],
            shortcut: None,
            params: "{capture,options:{object,fill,stroke},appearance:false}",
            enabled: crate::vector_cmds::has_doc,
            run: commit,
            journal: true,
        },
        CommandSpec {
            id: "object.expand.info",
            label: "Expand Options",
            menu: &[],
            shortcut: None,
            params: "{capture,options,appearance}",
            enabled: crate::vector_cmds::has_doc,
            run: info,
            journal: false,
        },
    ]
}
#[cfg(test)]
mod tests;
