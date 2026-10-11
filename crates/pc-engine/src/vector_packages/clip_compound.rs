//! Object clipping/compound command semantics adapted from VectorCraft cmd/object.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Local Image uses ordinary groups with authoritative vector masks, not a new tree.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{Document, Fill, FillRule, Layer, LayerContent, LayerId, Path, PathOp, ShapeLayer, ShapeStroke, VectorMask};
use photocraft_vector::appearance::{self, CompiledAppearance};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub layers: Vec<LayerId>,
}
impl Capture {
    pub fn begin(s: &Session, layers: &[LayerId]) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        targets(&st.doc, layers)?;
        Ok(Self { document: st.doc.id.0, revision: st.revision, layers: layers.to_vec() })
    }
    pub fn selected(s: &Session) -> Result<Self> {
        Self::begin(s, &s.active().ok_or(EngineError::NoDocument)?.selected_layers())
    }
    pub fn check(&self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        if self.document != st.doc.id.0 || self.revision != st.revision {
            return Err(bad("clipping/compound gesture is stale"));
        }
        targets(&st.doc, &self.layers)
    }
    pub fn params(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| bad(e.to_string()))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrawMode {
    #[default]
    Normal,
    Behind,
    Inside,
}
#[derive(Clone, Debug)]
pub enum Action {
    MakeClip,
    ReleaseClip,
    MakeCompound,
    ReleaseCompound,
    FillRule(FillRule),
    MoveInside(LayerId),
    MoveOutside,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    pub document: Document,
    pub selection: Vec<LayerId>,
    capture: Capture,
}
fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn path_valid(path: &Path) -> Result<()> {
    if path.subpaths.len() > 65_536 || path.subpaths.iter().map(|p| p.knots.len()).sum::<usize>() > 65_536 {
        return Err(bad("path exceeds the clipping/compound node budget"));
    }
    if path
        .subpaths
        .iter()
        .flat_map(|s| &s.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.)
    {
        return Err(bad("path coordinates must be finite and within 1000000 pixels"));
    }
    Ok(())
}
fn targets(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.len() > 256 {
        return Err(bad("clipping/compound target budget exceeded"));
    }
    let mut seen = BTreeSet::new();
    for &id in ids {
        if !seen.insert(id) {
            return Err(bad("duplicate target"));
        }
        let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        let locks = doc.effective_locks(id);
        if locks.all || locks.pixels || locks.position {
            return Err(bad(format!("layer {} is locked", layer.name)));
        }
        validate_layer(layer)?;
    }
    let paths = ids.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    for (i, a) in paths.iter().enumerate() {
        if paths.iter().enumerate().any(|(j, b)| i != j && b.starts_with(a)) {
            return Err(bad("target set contains an ancestor and descendant"));
        }
    }
    Ok(())
}
fn validate_layer(l: &Layer) -> Result<()> {
    if !l.opacity.is_finite() || !l.fill_opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity) || !(0.0..=1.0).contains(&l.fill_opacity) {
        return Err(bad("invalid layer opacity"));
    }
    if let LayerContent::Shape(sh) = &l.content {
        path_valid(&sh.path)?;
        appearance::validate(&l.vector.appearance.clone().unwrap_or_else(|| appearance::legacy_appearance(sh))).map_err(bad)?;
    }
    if let Some(m) = &l.vector_mask {
        path_valid(&m.path)?;
        if !m.density.is_finite() || !(0.0..=1.0).contains(&m.density) || !m.feather.is_finite() || !(0.0..=1_000_000.0).contains(&m.feather) {
            return Err(bad("invalid vector mask density or feather"));
        }
    }
    if let Some(ch) = l.children() {
        for child in ch {
            if child.locks.all || child.locks.pixels || child.locks.position {
                return Err(bad("clipping/compound contains a locked descendant"));
            }
            validate_layer(child)?;
        }
    }
    Ok(())
}
fn validate_tree(doc: &Document) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (path, _, l) in doc.walk() {
        if path.len() > photocraft_doc::MAX_GROUP_DEPTH + 1 || !seen.insert(l.id) {
            return Err(bad("clipping/compound exceeds the depth limit or repeats a layer ID"));
        }
    }
    Ok(())
}
fn ordered(doc: &Document, ids: &[LayerId]) -> Result<Vec<LayerId>> {
    if ids.is_empty() {
        return Err(bad("select at least one object"));
    }
    let mut paths = ids.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    let parent = &paths[0][..paths[0].len() - 1];
    if paths.iter().any(|p| &p[..p.len() - 1] != parent) {
        return Err(bad("selected objects must have the same parent"));
    }
    paths.sort();
    paths.iter().map(|p| doc.layer_at(p).map(|l| l.id).ok_or_else(|| bad("missing object"))).collect()
}
fn one(ids: &[LayerId]) -> Result<LayerId> {
    if let [id] = ids { Ok(*id) } else { Err(bad("select exactly one object")) }
}
fn list_mut<'a>(doc: &'a mut Document, parent: &[usize]) -> Result<&'a mut Vec<Layer>> {
    if parent.is_empty() {
        Ok(&mut doc.layers)
    } else {
        doc.layer_at_mut(parent).and_then(Layer::children_mut).ok_or_else(|| bad("insertion parent disappeared"))
    }
}
fn replace(doc: &mut Document, id: LayerId, layers: Vec<Layer>) -> Result<()> {
    let path = doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
    let (&index, parent) = path.split_last().ok_or_else(|| bad("missing insertion position"))?;
    list_mut(doc, parent)?.splice(index..=index, layers);
    Ok(())
}
fn shape_path(doc: &Document, id: LayerId) -> Result<Path> {
    let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let path = match &layer.content {
        LayerContent::Shape(sh) if layer.vector.corners.is_empty() => sh.path.clone(),
        LayerContent::Shape(sh) => super::corners::project_shape(sh, &layer.vector.corners)?.path,
        LayerContent::Text(t) => {
            if !doc.resolution_dpi.is_finite() || !(1.0..=9600.0).contains(&doc.resolution_dpi) {
                return Err(bad("invalid document DPI"));
            }
            super::path_text::on_path::validate_text(t).map_err(|e| bad(e.to_string()))?;
            if let Some(options) = &layer.vector.on_path {
                let mut engine = photocraft_text::shared().lock().map_err(|_| bad("text engine unavailable"))?;
                let layout = super::path_text::project(&mut engine, doc, t, options)?;
                let outlines = super::path_text::on_path::validated_outlines(&layout, &t.transform).map_err(|e| bad(e.to_string()))?;
                outlines::path(&outlines)
            } else {
                crate::type_extra_cmds::text_path(doc, t)
            }
        }
        _ => return Err(bad("clipping object must be a shape or text")),
    };
    path_valid(&path)?;
    if path.is_empty() || path.inverted {
        return Err(bad("clipping object must have a nonempty, non-inverted path"));
    }
    Ok(path)
}
fn render_layers(doc: &Document, layers: &mut [Layer]) -> Result<()> {
    // All geometry/paint and the total bounded cache cost are validated before allocation.
    for layer in layers.iter() {
        validate_layer(layer)?;
    }
    let compiled = layers
        .iter()
        .map(|l| match &l.content {
            LayerContent::Shape(sh) => {
                let projected;
                let sh = if l.vector.corners.is_empty() {
                    sh
                } else {
                    projected = super::corners::project_shape(sh, &l.vector.corners)?;
                    &projected
                };
                CompiledAppearance::with_projection(sh, l.vector.appearance.as_ref(), &doc.patterns, doc.bounds(), &Projection).map(Some).map_err(bad)
            }
            _ => Ok(None),
        })
        .collect::<Result<Vec<_>>>()?;
    if compiled.iter().flatten().map(|p| p.cache_bytes(doc.pixel_format())).sum::<u128>() > 512 << 20 {
        return Err(bad("prepared shapes exceed the 512 MiB cache budget"));
    }
    for (layer, compiled) in layers.iter_mut().zip(compiled) {
        if let (LayerContent::Shape(sh), Some(compiled)) = (&mut layer.content, compiled) {
            sh.cache = Some(compiled.render(doc.pixel_format(), doc.bounds()).map_err(bad)?);
            sh.psd_raw = None;
            if let Some(a) = &layer.vector.appearance {
                appearance::mirror_legacy(sh, a);
            }
        }
    }
    Ok(())
}
fn render_layer(doc: &Document, layer: &mut Layer) -> Result<()> {
    render_layers(doc, std::slice::from_mut(layer))
}
fn make_clip(doc: &mut Document, ids: &[LayerId]) -> Result<LayerId> {
    let ids = ordered(doc, ids)?;
    if ids.len() < 2 {
        return Err(bad("select a clipping path and at least one content object"));
    }
    let top = *ids.last().ok_or_else(|| bad("empty clipping selection"))?;
    let path = shape_path(doc, top)?;
    // Consume the top path into the ordinary group's vector mask. Release reconstructs
    // an unpainted path from this authoritative geometry, including later mask edits.
    let children = ids[..ids.len() - 1].iter().map(|id| doc.layer(*id).cloned().ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    let mut group = Layer::group("Clipping Group", children);
    group.vector_mask = Some(VectorMask::new(path));
    let gid = group.id;
    doc.insert_above(Some(top), group);
    for id in ids {
        doc.remove(id).ok_or(EngineError::NoLayer(id))?;
    }
    Ok(gid)
}
/// Nearest ordinary group with a vector mask, including the object itself.
pub fn clip_parent(doc: &Document, id: LayerId) -> Option<LayerId> {
    let path = doc.path_of(id)?;
    (1..=path.len()).rev().filter_map(|n| doc.layer_at(&path[..n])).find(|l| l.is_group() && l.vector_mask.is_some()).map(|l| l.id)
}
fn release_clip(doc: &mut Document, id: LayerId) -> Result<Vec<LayerId>> {
    let group = doc.layer(id).cloned().ok_or(EngineError::NoLayer(id))?;
    if group.mask.is_some()
        || group.vector.live.is_some()
        || group.artboard().is_some()
        || group.effects != (photocraft_doc::Effects { enabled: true, ..Default::default() })
        || group.opacity != 1.0
        || group.fill_opacity != 1.0
        || group.blend != photocraft_doc::BlendMode::PassThrough
        || group.clipped
    {
        return Err(bad("release requires an ordinary clip group without additional group appearance"));
    }
    let mask = group.vector_mask.ok_or_else(|| bad("select a group with a vector mask"))?;
    let mut layers = group.content;
    let LayerContent::Group(ref mut g) = layers else { return Err(bad("select a clip group")) };
    let mut mask_layer = Layer::new("Released Clipping Path", LayerContent::Shape(ShapeLayer { path: mask.path, ..Default::default() }));
    mask_layer.vector.appearance = Some(Default::default());
    render_layer(doc, &mut mask_layer)?;
    let mut children = std::mem::take(&mut g.children);
    children.push(mask_layer);
    let ids = children.iter().map(|l| l.id).collect();
    replace(doc, id, children)?;
    Ok(ids)
}
fn compound(doc: &mut Document, ids: &[LayerId]) -> Result<LayerId> {
    let ids = ordered(doc, ids)?;
    if ids.len() < 2 {
        return Err(bad("select at least two shape layers"));
    }
    for &id in &ids {
        if !matches!(doc.layer(id).map(|l| &l.content), Some(LayerContent::Shape(_))) {
            return Err(bad("compound paths require shape layers"));
        }
    }
    let mut bottom = doc.layer(ids[0]).cloned().ok_or(EngineError::NoLayer(ids[0]))?;
    let mut path = Path::default();
    for &id in &ids {
        let LayerContent::Shape(sh) = &doc.layer(id).ok_or(EngineError::NoLayer(id))?.content else { return Err(bad("shape disappeared")) };
        // Ordinary contours are lossless. Bake ordered PSD operations only when needed.
        let corners = &doc.layer(id).ok_or(EngineError::NoLayer(id))?.vector.corners;
        let projected = if corners.is_empty() { sh.clone() } else { super::corners::project_shape(sh, corners)? };
        let part = if projected.path.inverted
            || projected.path.fill_rule == FillRule::EvenOdd
            || projected.path.subpaths.iter().any(|s| !matches!(s.op, PathOp::Combine | PathOp::Join))
        {
            photocraft_pathops::finish_compound(&projected.path, None).map_err(|e| bad(e.to_string()))?
        } else {
            projected.path
        };
        path.subpaths.extend(part.subpaths);
    }
    for (i, subpath) in path.subpaths.iter_mut().enumerate() {
        // One native component: Join is required for a fill rule to span the contours.
        subpath.op = if i == 0 { PathOp::Combine } else { PathOp::Join };
    }
    path_valid(&path)?;
    let LayerContent::Shape(sh) = &mut bottom.content else { return Err(bad("missing bottom shape")) };
    sh.path = path;
    sh.live = None;
    bottom.vector.live_shape = None;
    bottom.vector.corners.clear();
    bottom.name = "Compound Path".into();
    render_layer(doc, &mut bottom)?;
    let id = bottom.id;
    let top = *ids.last().ok_or_else(|| bad("empty compound selection"))?;
    // Use a fresh temporary ID during insertion; never publish duplicate IDs.
    let mut inserted = bottom;
    inserted.id = LayerId::fresh();
    let temporary = inserted.id;
    doc.insert_above(Some(top), inserted);
    for id in ids {
        doc.remove(id).ok_or(EngineError::NoLayer(id))?;
    }
    doc.layer_mut(temporary).ok_or(EngineError::NoLayer(temporary))?.id = id;
    Ok(id)
}
fn release_compound(doc: &mut Document, id: LayerId) -> Result<Vec<LayerId>> {
    let layer = doc.layer(id).cloned().ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Shape(sh) = &layer.content else { return Err(bad("select a compound shape")) };
    if sh.path.subpaths.len() < 2 || sh.path.inverted {
        return Err(bad("select a non-inverted compound path with at least two subpaths"));
    }
    if sh.path.subpaths.len() > 256 {
        return Err(bad("compound release exceeds the 256-object budget"));
    }
    let projected = if layer.vector.corners.is_empty() { sh.clone() } else { super::corners::project_shape(sh, &layer.vector.corners)? };
    let sh = &projected;
    // Duplicate only the envelope/paint, not the complete compound for each contour.
    let mut template = layer.clone();
    let LayerContent::Shape(shape) = &mut template.content else { return Err(bad("shape disappeared")) };
    shape.path = Path::default();
    shape.cache = None;
    shape.psd_raw = None;
    let mut children = Vec::new();
    for subpath in &sh.path.subpaths {
        let mut child = template.duplicate();
        let LayerContent::Shape(shape) = &mut child.content else { return Err(bad("shape disappeared")) };
        let mut subpath = subpath.clone();
        subpath.op = PathOp::Combine;
        shape.path = Path { subpaths: vec![subpath], fill_rule: sh.path.fill_rule, inverted: false };
        shape.live = None;
        child.vector.corners.clear();
        child.vector.live_shape = None;
        children.push(child);
    }
    render_layers(doc, &mut children)?;
    let ids = children.iter().map(|l| l.id).collect();
    replace(doc, id, children)?;
    Ok(ids)
}
fn move_inside(doc: &mut Document, ids: &[LayerId], target: LayerId) -> Result<LayerId> {
    if ids.is_empty() || ids.contains(&target) {
        return Err(bad("move inside needs contents and a distinct target"));
    }
    targets(doc, &[target])?;
    let destination = doc.path_of(target).ok_or(EngineError::NoLayer(target))?;
    for id in ids {
        let path = doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
        if destination.starts_with(&path) {
            return Err(bad("cannot move an ancestor inside its descendant"));
        }
    }
    let gid = if doc.layer(target).is_some_and(Layer::is_group) {
        target
    } else {
        let path = shape_path(doc, target)?;
        let parent = doc.layer(target).cloned().ok_or(EngineError::NoLayer(target))?;
        let mut group = Layer::group(parent.name.clone(), vec![parent]);
        group.vector_mask = Some(VectorMask::new(path));
        let gid = group.id;
        replace(doc, target, vec![group])?;
        gid
    };
    let mut ordered = ids.iter().map(|id| Ok((doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?, *id))).collect::<Result<Vec<_>>>()?;
    ordered.sort();
    let mut children = Vec::new();
    for (_, id) in ordered {
        children.push(doc.remove(id).ok_or(EngineError::NoLayer(id))?);
    }
    doc.layer_mut(gid).and_then(Layer::children_mut).ok_or_else(|| bad("missing destination group"))?.extend(children);
    Ok(gid)
}
pub fn prepare(s: &Session, c: &Capture, action: &Action) -> Result<Prepared> {
    c.check(s)?;
    if c.layers.is_empty() {
        return Err(bad("select at least one object"));
    }
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let selection = match *action {
        Action::MakeClip => vec![make_clip(&mut doc, &c.layers)?],
        Action::ReleaseClip => release_clip(&mut doc, one(&c.layers)?)?,
        Action::MakeCompound => vec![compound(&mut doc, &c.layers)?],
        Action::ReleaseCompound => release_compound(&mut doc, one(&c.layers)?)?,
        Action::FillRule(rule) => {
            // Preflight every target type before rendering any of them.
            let mut layers = Vec::new();
            for id in &c.layers {
                let layer = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
                if !matches!(layer.content, LayerContent::Shape(_)) && !(layer.is_group() && layer.vector_mask.is_some()) {
                    return Err(bad("fill rule requires shapes or clip groups"));
                }
            }
            for id in &c.layers {
                let mut layer = doc.layer(*id).cloned().ok_or(EngineError::NoLayer(*id))?;
                if let LayerContent::Shape(sh) = &mut layer.content {
                    sh.path.fill_rule = rule;
                    sh.live = None;
                    layer.vector.live_shape = None;
                } else if let Some(m) = &mut layer.vector_mask {
                    m.path.fill_rule = rule;
                }
                layers.push(layer);
            }
            render_layers(&doc, &mut layers)?;
            for layer in layers {
                let id = layer.id;
                *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
            }
            c.layers.clone()
        }
        Action::MoveInside(target) => {
            move_inside(&mut doc, &c.layers, target)?;
            c.layers.clone()
        }
        Action::MoveOutside => {
            let ids = ordered(&doc, &c.layers)?;
            let first = doc.path_of(ids[0]).ok_or(EngineError::NoLayer(ids[0]))?;
            let parent = doc.layer_at(&first[..first.len() - 1]).ok_or_else(|| bad("object is already outside"))?.id;
            targets(&doc, &[parent])?;
            let mut above = parent;
            for id in &ids {
                let layer = doc.remove(*id).ok_or(EngineError::NoLayer(*id))?;
                doc.insert_above(Some(above), layer);
                above = *id;
            }
            ids
        }
    };
    validate_tree(&doc)?;
    Ok(Prepared { document: doc, selection, capture: c.clone() })
}
/// Capture the exact insertion target at pointer-down. 37 supplies its isolation target
/// as `target`; callers must not resolve a different active object on release.
pub fn prepare_insert(s: &Session, c: &Capture, mode: DrawMode, target: Option<LayerId>, mut layer: Layer) -> Result<Prepared> {
    c.check(s)?;
    if c.layers != target.into_iter().collect::<Vec<_>>() {
        return Err(bad("insertion target differs from capture"));
    }
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    if doc.layer(layer.id).is_some() {
        return Err(bad("insertion layer already exists"));
    }
    // Validate the destination before preparing any new surface.
    if mode == DrawMode::Inside {
        let target = target.ok_or_else(|| bad("Draw Inside needs one selected object"))?;
        if !doc.layer(target).is_some_and(Layer::is_group) {
            shape_path(&doc, target)?;
        }
        let depth = doc.path_of(target).ok_or(EngineError::NoLayer(target))?.len();
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(bad("Draw Inside exceeds the group depth limit"));
        }
    }
    render_layer(&doc, &mut layer)?;
    let id = layer.id;
    let selection = match mode {
        DrawMode::Normal => {
            doc.insert_above(target, layer);
            vec![id]
        }
        DrawMode::Behind => {
            if let Some(target) = target {
                let path = doc.path_of(target).ok_or(EngineError::NoLayer(target))?;
                let (&index, parent) = path.split_last().ok_or_else(|| bad("missing draw target"))?;
                list_mut(&mut doc, parent)?.insert(index, layer);
            } else {
                doc.layers.insert(0, layer);
            }
            vec![id]
        }
        DrawMode::Inside => {
            let target = target.ok_or_else(|| bad("Draw Inside needs one selected object"))?;
            // Place temporarily at root, then use the exact same nesting implementation.
            doc.layers.push(layer);
            vec![move_inside(&mut doc, &[id], target)?]
        }
    };
    validate_tree(&doc)?;
    Ok(Prepared { document: doc, selection, capture: c.clone() })
}
fn publish(s: &mut Session, p: Prepared, label: &str) -> Result<Value> {
    p.capture.check(s)?;
    let ids = p.selection.clone();
    s.edit(label, move |doc, active| {
        *doc = p.document;
        *active = p.selection.last().copied();
        Ok(())
    })?;
    // Match Session's existing selection store; keep history's current selection coherent.
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    st.selected_layers = ids.clone();
    st.layer_anchor = ids.last().copied();
    st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected: ids.clone() });
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>(),"layer":ids.last().map(|id|id.0)}))
}
fn parse(p: &Value) -> Result<Capture> {
    serde_json::from_value(p.clone()).map_err(|e| bad(format!("invalid clipping/compound capture: {e}")))
}
fn run(s: &mut Session, p: &Value, action: Action, label: &str) -> Result<Value> {
    publish(s, prepare(s, &parse(p)?, &action)?, label)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveRequest {
    capture: Capture,
    target: LayerId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleRequest {
    capture: Capture,
    rule: FillRule,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawRequest {
    pub capture: Capture,
    pub mode: DrawMode,
    pub target: Option<LayerId>,
    pub layer: LayerId,
    pub path: Path,
    pub fill: Option<Fill>,
    pub stroke: Option<ShapeStroke>,
}
pub fn preview_draw(s: &Session, request: &DrawRequest) -> Result<Prepared> {
    if request.path.is_empty() {
        return Err(bad("draw path must not be empty"));
    }
    let mut layer = Layer::new(
        "Shape",
        LayerContent::Shape(ShapeLayer { path: request.path.clone(), fill: request.fill.clone(), stroke: request.stroke.clone(), ..Default::default() }),
    );
    layer.id = request.layer;
    if layer.id.0 == 0 {
        return Err(bad("invalid inserted layer ID"));
    }
    prepare_insert(s, &request.capture, request.mode, request.target, layer)
}
/// Edit Mask targets the group's authoritative vector-mask path (DirectSelection PathRef::Layer).
pub fn edit_selection(s: &mut Session, c: &Capture, mask: bool) -> Result<Vec<LayerId>> {
    c.check(s)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let groups: BTreeSet<_> =
        c.layers.iter().map(|id| clip_parent(doc, *id).ok_or_else(|| bad("select a clip group or its contents"))).collect::<Result<_>>()?;
    let mut ids = Vec::new();
    for group in groups {
        targets(doc, &[group])?;
        if mask {
            ids.push(group);
        } else {
            ids.extend(doc.layer(group).and_then(Layer::children).ok_or_else(|| bad("missing clip contents"))?.iter().map(|l| l.id));
        }
    }
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    st.active_layer = ids.last().copied();
    st.selected_layers = ids.clone();
    st.layer_anchor = st.active_layer;
    Ok(ids)
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    macro_rules! spec {
        ($id:expr, $label:expr, $run:expr, $journal:expr) => {
            CommandSpec {
                id: $id,
                label: $label,
                menu: &[],
                shortcut: None,
                params: "captured document/revision/layers; see integration/35.md",
                enabled: crate::vector_cmds::has_doc,
                run: $run,
                journal: $journal,
            }
        };
    }
    vec![
        spec!("object.clippingMask.make", "Make Clipping Mask", |s, p| run(s, p, Action::MakeClip, "Make Clipping Mask"), true),
        spec!("object.clippingMask.release", "Release Clipping Mask", |s, p| run(s, p, Action::ReleaseClip, "Release Clipping Mask"), true),
        spec!("object.compoundPath.make", "Make Compound Path", |s, p| run(s, p, Action::MakeCompound, "Make Compound Path"), true),
        spec!("object.compoundPath.release", "Release Compound Path", |s, p| run(s, p, Action::ReleaseCompound, "Release Compound Path"), true),
        spec!(
            "object.moveInside",
            "Move Inside",
            |s, p| {
                let r: MoveRequest = serde_json::from_value(p.clone()).map_err(|e| bad(e.to_string()))?;
                publish(s, prepare(s, &r.capture, &Action::MoveInside(r.target))?, "Move Inside")
            },
            true
        ),
        spec!("object.moveOutside", "Move Outside", |s, p| run(s, p, Action::MoveOutside, "Move Outside"), true),
        spec!(
            "object.fillRule",
            "Fill Rule",
            |s, p| {
                let r: RuleRequest = serde_json::from_value(p.clone()).map_err(|e| bad(e.to_string()))?;
                publish(s, prepare(s, &r.capture, &Action::FillRule(r.rule))?, "Change Fill Rule")
            },
            true
        ),
        spec!(
            "object.draw.insert",
            "Draw Object",
            |s, p| {
                let r: DrawRequest = serde_json::from_value(p.clone()).map_err(|e| bad(e.to_string()))?;
                publish(s, preview_draw(s, &r)?, "Draw Object")
            },
            true
        ),
        spec!(
            "object.clippingMask.editMask",
            "Edit Mask",
            |s, p| Ok(json!({"layers":edit_selection(s,&parse(p)?,true)?.iter().map(|id|id.0).collect::<Vec<_>>()})),
            false
        ),
        spec!(
            "object.clippingMask.editContents",
            "Edit Contents",
            |s, p| Ok(json!({"layers":edit_selection(s,&parse(p)?,false)?.iter().map(|id|id.0).collect::<Vec<_>>()})),
            false
        ),
    ]
}

#[path = "clip_compound/outlines.rs"]
mod outlines;
struct Projection;
impl photocraft_vector::appearance::StrokeProjection for Projection {
    fn outline(&self, path: &Path, stroke: &photocraft_doc::vector_design::appearance::StrokeItem) -> std::result::Result<Path, String> {
        if stroke.brush.is_some() { super::brushes::BrushProjection.outline(path, stroke) } else { photocraft_vecfx::width::Projection.outline(path, stroke) }
    }
}
#[cfg(test)]
#[path = "clip_compound/tests.rs"]
mod tests;
