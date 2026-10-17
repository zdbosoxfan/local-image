//! Attribute transfer and linked Graphic Styles, adapted in behaviour from VectorCraft
//! engine/cmd/style.rs and cmd/stylelib.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors (MIT OR Apache-2.0).
//! Detached preparation validates the complete set and budgets caches before one Session edit.
//! The host supplies persistence hooks; no process-global document/style sidecar is used.
use crate::{EngineError, Result, Session, commands::CommandSpec};
pub use photocraft_doc::vector_design::styles::{Attributes, GraphicStyle, Sample, StyleId, StyleLink};
use photocraft_doc::vector_design::{
    appearance::{Appearance, AppearanceItem, FillItem},
    styles::{LayerEffects, ObjectAttributes, TextObject},
};
use photocraft_doc::{Color, Document, Effects, Fill, Layer, LayerContent, LayerId};
use photocraft_vector::appearance::{self as renderer, CompiledAppearance, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, OnceLock},
};

mod text;
const MAX_TARGETS: usize = 256;
const CACHE_BUDGET: u128 = 512 << 20;
fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}

/// Detached view of the additive document list and per-layer links. Only the
/// native provider reads/writes these fields; this is also useful to file adapters via pc-doc.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub graphic_styles: Vec<GraphicStyle>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<LayerId, StyleLink>,
}
#[derive(Clone, Copy)]
pub struct Hooks {
    pub read: fn(&Document) -> Result<Store>,
    /// Called on the detached candidate inside Session::edit; failures publish nothing.
    pub write: fn(&mut Document, &Store) -> Result<()>,
}
static HOOKS: OnceLock<Hooks> = OnceLock::new();
pub fn install_hooks(hooks: Hooks) -> std::result::Result<(), Hooks> {
    HOOKS.set(hooks)
}
pub fn installed_hooks() -> Option<Hooks> {
    HOOKS.get().copied()
}
pub fn store(doc: &Document) -> Result<Store> {
    HOOKS.get().map_or_else(|| Ok(Store::default()), |h| (h.read)(doc))
}

pub fn sample(doc: &Document, id: LayerId, byte: Option<usize>) -> Result<Sample> {
    sample_layer_at(doc.layer(id).ok_or(EngineError::NoLayer(id))?, byte)
}
fn sample_layer_at(layer: &Layer, byte: Option<usize>) -> Result<Sample> {
    let (character, paragraph, object_text) = match &layer.content {
        LayerContent::Text(t) => {
            let at = byte.unwrap_or(0);
            if at > t.text.len() || !t.text.is_char_boundary(at) {
                return Err(error("invalid sampled text byte"));
            }
            let mut end = 0;
            let chars = t.char_runs();
            let chr = chars
                .iter()
                .find(|r| {
                    end += r.len;
                    at < end
                })
                .or_else(|| chars.last())
                .map(|r| r.style.clone());
            let mut end = 0;
            let paras = t.paragraph_runs();
            let para = paras
                .iter()
                .find(|r| {
                    end += r.len;
                    at < end
                })
                .or_else(|| paras.last())
                .map(|r| r.style.clone());
            (chr, para, Some(TextObject { shape: t.shape, orientation: t.orientation, antialias: t.antialias, warp: t.warp.clone() }))
        }
        _ => (None, None, None),
    };
    let appearance =
        match &layer.content {
            LayerContent::Shape(sh) => Some(layer.vector.appearance.clone().unwrap_or_else(|| renderer::legacy_appearance(sh))),
            LayerContent::Text(t) => Some(layer.vector.appearance.clone().unwrap_or_else(|| Appearance {
                items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(character.as_ref().map_or(t.color, |c| c.color))))],
            })),
            _ => layer.vector.appearance.clone(),
        };
    Ok(Sample {
        appearance,
        opacity: layer.opacity,
        blend: layer.blend,
        character,
        paragraph,
        effects: LayerEffects {
            enabled: layer.effects.enabled,
            items: layer.effects.items.clone(),
            reference: layer.effects.reference,
            psd_raw: layer.effects.psd_raw.as_ref().map(|r| (**r).clone()),
        },
        object: ObjectAttributes {
            fill_opacity: layer.fill_opacity,
            clipped: layer.clipped,
            blend_if: layer.blend_if.clone(),
            excluded_channels: layer.excluded_channels,
            text: object_text,
        },
    })
}
fn walk(layers: &[Layer], out: &mut Vec<LayerId>) {
    for l in layers {
        out.push(l.id);
        if let LayerContent::Group(g) = &l.content {
            walk(&g.children, out);
        }
    }
}
/// Select Same Graphic Style excludes both absent and locally modified links.
pub fn same_style(doc: &Document, store: &Store, reference: LayerId) -> Result<Vec<LayerId>> {
    let Some(link) = store.links.get(&reference).filter(|l| in_sync(doc, reference, l)) else { return Ok(Vec::new()) };
    let mut ids = Vec::new();
    walk(&doc.layers, &mut ids);
    Ok(ids.into_iter().filter(|id| store.links.get(id).is_some_and(|l| l.style == link.style && in_sync(doc, *id, l))).collect())
}
pub fn link(doc: &Document, id: LayerId, style: StyleId) -> Result<StyleLink> {
    let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let (character_runs, paragraph_runs) = match &layer.content {
        LayerContent::Text(t) => (t.char_runs(), t.paragraph_runs()),
        _ => (Vec::new(), Vec::new()),
    };
    Ok(StyleLink { style, applied: sample(doc, id, None)?, character_runs, paragraph_runs })
}
/// Per-layer reader. Absent and locally edited links produce no key.
pub fn style_key(layer: &Layer, baseline: Option<&StyleLink>) -> Result<Option<String>> {
    let Some(baseline) = baseline else {
        return Ok(None);
    };
    let (character_runs, paragraph_runs) = match &layer.content {
        LayerContent::Text(t) => (t.char_runs(), t.paragraph_runs()),
        _ => (Vec::new(), Vec::new()),
    };
    Ok((sample_layer_at(layer, None)? == baseline.applied && character_runs == baseline.character_runs && paragraph_runs == baseline.paragraph_runs)
        .then(|| baseline.style.0.to_string()))
}
pub fn in_sync(doc: &Document, id: LayerId, baseline: &StyleLink) -> bool {
    link(doc, id, baseline.style).is_ok_and(|current| current == *baseline)
}
/// Reuse the installed algorithms for profiles, arrows, dashes and brushes.
pub struct Projection;
impl StrokeProjection for Projection {
    fn outline(
        &self,
        path: &photocraft_doc::Path,
        stroke: &photocraft_doc::vector_design::appearance::StrokeItem,
    ) -> std::result::Result<photocraft_doc::Path, String> {
        if stroke.brush.is_some() {
            crate::vector_packages::brushes::BrushProjection.outline(path, stroke)
        } else {
            photocraft_vecfx::width::Projection.outline(path, stroke)
        }
    }
}
/// Called at the end of Session::edit, on its detached candidate. New style application
/// links remain in sync; local appearance/text/effect edits break links in the SAME undo step.
pub fn prune_links(doc: &mut Document) -> Result<()> {
    let Some(hooks) = installed_hooks() else {
        return Ok(());
    };
    let mut state = (hooks.read)(doc)?;
    let count = state.links.len();
    state.links.retain(|id, link| state.graphic_styles.iter().any(|s| s.id == link.style) && in_sync(doc, *id, link));
    if state.links.len() != count {
        (hooks.write)(doc, &state)?;
    }
    Ok(())
}
pub fn break_link(store: &mut Store, ids: &[LayerId]) {
    for id in ids {
        store.links.remove(id);
    }
}

/// Options replace only the chosen categories. Partial stacks retain the positions/order of
/// excluded items; full replacement is exactly the source stack, including hidden items.
fn transfer_stack(target: &Appearance, source: &Appearance, a: Attributes, append: bool) -> Appearance {
    let chosen = |item: &AppearanceItem| match item {
        AppearanceItem::Fill(_) | AppearanceItem::Mesh(_) => a.fill,
        AppearanceItem::Stroke(_) => a.stroke,
    };
    if a.fill && a.stroke && !append {
        return source.clone();
    }
    let incoming: Vec<_> = source.items.iter().filter(|i| chosen(i)).cloned().collect();
    if append {
        return Appearance { items: target.items.iter().cloned().chain(incoming).collect() };
    }
    let anchor = target.items.iter().position(chosen).unwrap_or_else(|| {
        if a.fill { target.items.iter().position(|i| matches!(i, AppearanceItem::Stroke(_))).unwrap_or(target.items.len()) } else { target.items.len() }
    });
    let mut items = Vec::new();
    for (index, item) in target.items.iter().enumerate() {
        if index == anchor {
            items.extend(incoming.iter().cloned());
        }
        if !chosen(item) {
            items.push(item.clone());
        }
    }
    if anchor == target.items.len() {
        items.extend(incoming);
    }
    Appearance { items }
}

fn transfer(doc: &Document, id: LayerId, source: &Sample, a: Attributes, append: bool) -> Result<Layer> {
    let a = if append { Attributes { fill: a.fill, stroke: a.stroke, effects: a.effects, ..Attributes::NONE } } else { a };
    let mut layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
    let before = sample(doc, id, None)?;
    if (a.fill || a.stroke)
        && let Some(src) = &source.appearance
    {
        let next = transfer_stack(&before.appearance.unwrap_or_default(), src, a, append);
        if !matches!(layer.content, LayerContent::Shape(_) | LayerContent::Text(_)) && !next.items.is_empty() {
            return Err(error("fill and stroke require a shape or text object"));
        }
        layer.vector.appearance = Some(next);
    }
    if a.opacity {
        layer.opacity = source.opacity;
        layer.blend = source.blend;
    }
    if a.effects {
        if append {
            layer.effects.enabled |= source.effects.enabled;
            layer.effects.items.extend(source.effects.items.clone());
            layer.effects.psd_raw = None;
        } else {
            layer.effects = Effects {
                enabled: source.effects.enabled,
                items: source.effects.items.clone(),
                reference: source.effects.reference,
                psd_raw: source.effects.psd_raw.clone().map(Arc::new),
            };
        }
    }
    if a.object {
        layer.fill_opacity = source.object.fill_opacity;
        layer.clipped = source.object.clipped;
        layer.blend_if = source.object.blend_if.clone();
        layer.excluded_channels = source.object.excluded_channels;
    }
    if let LayerContent::Text(t) = &mut layer.content {
        if a.character
            && let Some(chr) = &source.character
        {
            t.runs = t
                .char_runs()
                .into_iter()
                .map(|mut r| {
                    let color = r.style.color;
                    r.style = chr.clone();
                    if !a.fill {
                        r.style.color = color;
                    }
                    r
                })
                .collect();
            t.sync_summary();
        }
        if a.paragraph
            && let Some(para) = &source.paragraph
        {
            t.paragraphs = t
                .paragraph_runs()
                .into_iter()
                .map(|mut r| {
                    r.style = para.clone();
                    r
                })
                .collect();
        }
        if a.object
            && let Some(o) = &source.object.text
        {
            t.shape = o.shape;
            t.orientation = o.orientation;
            t.antialias = o.antialias;
            t.warp = o.warp.clone();
        }
    }
    validate_sample(&sample_layer(&layer)?)?;
    Ok(layer)
}
fn sample_layer(layer: &Layer) -> Result<Sample> {
    sample_layer_at(layer, None)
}
/// A strict serialization round-trip rejects NaN/Inf (serde emits null), then numeric fields
/// are bounded before layout/effects code sees them. No permissive numeric clamping.
pub fn validate_sample(s: &Sample) -> Result<()> {
    if s.effects.psd_raw.as_ref().is_some_and(|p| p.len() > 1 << 20) {
        return Err(error("style raw FX budget"));
    }

    if let Some(a) = &s.appearance {
        renderer::validate(a).map_err(error)?;
    }
    if !(0. ..=1.).contains(&s.opacity) || !(0. ..=1.).contains(&s.object.fill_opacity) || s.effects.items.len() > 64 {
        return Err(error("invalid style opacity/effects"));
    }
    let value = serde_json::to_value(s).map_err(|e| error(e.to_string()))?;
    let decoded: Sample = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
    if decoded != *s {
        return Err(error("style attributes must be finite"));
    }
    fn numbers(v: &Value, key: &str, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err(error("style nesting limit"));
        }
        match v {
            Value::Number(_) if matches!(key, "style_sheet" | "excluded_channels") => {}
            Value::Number(n) => {
                let n = n.as_f64().ok_or_else(|| error("invalid style number"))?;
                if !n.is_finite() || n.abs() > 1_000_000. {
                    return Err(error("style numbers exceed bounds"));
                }
                if matches!(key, "opacity" | "fill_opacity" | "alpha" | "spread" | "noise" | "jitter" | "range") && !(0. ..=1.).contains(&n) {
                    return Err(error("style fraction outside 0..1"));
                }
                if matches!(key, "size" | "soften" | "distance") && !(0. ..=5000.).contains(&n) {
                    return Err(error("style effect size outside bounds"));
                }
                if matches!(key, "size_pt" | "horizontal_scale" | "vertical_scale" | "scale") && !(0.001..=5000.).contains(&n) {
                    return Err(error("style size outside bounds"));
                }
            }
            Value::String(s) if s.len() > 4096 => return Err(error("style string limit")),
            Value::Array(a) => {
                if a.len() > 65_536 {
                    return Err(error("style array limit"));
                }
                for v in a {
                    numbers(v, key, depth + 1)?;
                }
            }
            Value::Object(m) => {
                for (k, v) in m {
                    if k != "psd_raw" {
                        numbers(v, k, depth + 1)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    numbers(&value, "", 0)?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Copy {
        sample: Box<Sample>,
        attributes: Attributes,
        #[serde(default)]
        append: bool,
    },
    Color {
        color: Color,
        #[serde(default)]
        stroke: bool,
    },
    Apply {
        style: StyleId,
        #[serde(default)]
        add: bool,
    },
    New {
        source: LayerId,
        name: String,
    },
    Redefine {
        style: StyleId,
        source: LayerId,
    },
    Rename {
        style: StyleId,
        name: String,
    },
    Duplicate {
        style: StyleId,
        name: String,
    },
    Delete {
        style: StyleId,
    },
    BreakLink,
    InstallStarters,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub document: u64,
    pub revision: u64,
    pub targets: Vec<LayerId>,
    pub operation: Operation,
}
pub fn capture(s: &Session, targets: Vec<LayerId>, operation: Operation) -> Result<Request> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    Ok(Request { document: st.doc.id.0, revision: st.revision, targets, operation })
}
pub fn fresh(s: &Session, request: &Request) -> bool {
    s.active().is_some_and(|st| st.doc.id.0 == request.document && st.revision == request.revision)
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub request: Request,
    pub document: Document,
    pub store: Store,
}
fn style(store: &Store, id: StyleId) -> Result<&GraphicStyle> {
    store.graphic_styles.iter().find(|s| s.id == id).ok_or_else(|| error("graphic style is missing"))
}
fn name(store: &Store, name: &str, except: Option<StyleId>) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > 256
        || name.chars().any(char::is_control)
        || store.graphic_styles.iter().any(|s| Some(s.id) != except && s.name == name)
    {
        return Err(error("style name must be nonempty, unique and at most 256 bytes"));
    }
    Ok(())
}
fn next_id(store: &Store) -> Result<StyleId> {
    store.graphic_styles.iter().map(|s| s.id.0).max().unwrap_or(0).checked_add(1).map(StyleId).ok_or_else(|| error("graphic style IDs exhausted"))
}
fn validate_targets(doc: &Document, ids: &[LayerId]) -> Result<()> {
    if ids.len() > MAX_TARGETS {
        return Err(error("style target budget exceeded"));
    }
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(error("duplicate style target"));
        }
        doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        let locks = doc.effective_locks(*id);
        if locks.all || locks.pixels {
            return Err(error("style target is locked"));
        }
    }
    Ok(())
}
/// Pure core, usable without the native data mirror.
pub fn prepare(doc: &Document, revision: u64, original: &Store, request: &Request, projection: &dyn StrokeProjection) -> Result<Preview> {
    if doc.id.0 != request.document || revision != request.revision {
        return Err(error("style gesture is stale"));
    }
    validate_targets(doc, &request.targets)?;
    if original.graphic_styles.len() > 1024 || original.links.len() > 100_000 {
        return Err(error("style library budget exceeded"));
    }
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(data.len());
            if self.0 > 64 << 20 {
                return Err(std::io::Error::other("style metadata exceeds 64 MiB"));
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(0), original).map_err(|e| error(e.to_string()))?;
    let mut unique = HashSet::new();
    for s in &original.graphic_styles {
        if s.id.0 == 0 || !unique.insert(s.id) {
            return Err(error("invalid graphic style IDs"));
        }
        validate_sample(&s.attributes)?;
        name(original, &s.name, Some(s.id))?;
    }
    let mut store = original.clone();
    store.links.retain(|id, link| original.graphic_styles.iter().any(|s| s.id == link.style) && in_sync(doc, *id, link));
    let mut edits = Vec::new();
    let mut links = Vec::new();
    let mut copy = |sample: &Sample, a: Attributes, append: bool, ids: &[LayerId], link: Option<StyleId>| -> Result<()> {
        if ids.is_empty() {
            return Err(error("select at least one style target"));
        }
        validate_sample(sample)?;
        validate_targets(doc, ids)?;
        if a == Attributes::NONE {
            return Ok(());
        }
        for id in ids {
            edits.push(transfer(doc, *id, sample, a, append)?);
            links.push((*id, link));
        }
        Ok(())
    };
    match &request.operation {
        Operation::Copy { sample, attributes, append } => copy(sample, *attributes, *append, &request.targets, None)?,
        Operation::Color { color, stroke } => {
            for id in &request.targets {
                let mut source = sample(doc, *id, None)?;
                let stack = source.appearance.as_mut().ok_or_else(|| error("colour target has no paint"))?;
                let paint = stack.items.iter_mut().rev().find_map(|item| match item {
                    AppearanceItem::Fill(f) if !stroke => Some(&mut f.paint),
                    AppearanceItem::Stroke(s) if *stroke => Some(&mut s.base.paint),
                    _ => None,
                });
                if let Some(paint) = paint {
                    *paint = Fill::Solid(*color);
                } else if *stroke {
                    stack.items.push(AppearanceItem::Stroke(photocraft_doc::vector_design::appearance::StrokeItem {
                        base: photocraft_doc::ShapeStroke { paint: Fill::Solid(*color), ..Default::default() },
                        ..Default::default()
                    }));
                } else {
                    stack.items.push(AppearanceItem::Fill(FillItem::new(Fill::Solid(*color))));
                }
                copy(&source, Attributes { fill: true, stroke: true, ..Attributes::NONE }, false, &[*id], None)?;
            }
            if request.targets.is_empty() {
                return Err(error("select at least one colour target"));
            }
        }
        Operation::Apply { style: id, add } => copy(&style(&store, *id)?.attributes, Attributes::ALL, *add, &request.targets, (!add).then_some(*id))?,
        Operation::New { source, name: n } => {
            name(&store, n, None)?;
            validate_targets(doc, &[*source])?;
            let attributes = sample(doc, *source, None)?;
            validate_sample(&attributes)?;
            let id = next_id(&store)?;
            store.graphic_styles.push(GraphicStyle { id, name: n.clone(), attributes: attributes.clone() });
            store.links.insert(*source, link(doc, *source, id)?);
        }
        Operation::Redefine { style: id, source } => {
            style(&store, *id)?;
            validate_targets(doc, &[*source])?;
            let attributes = sample(doc, *source, None)?;
            let mut ids: Vec<_> = store.links.iter().filter(|(_, l)| l.style == *id).map(|(id, _)| *id).collect();
            if !ids.contains(source) {
                ids.push(*source);
            }
            copy(&attributes, Attributes::ALL, false, &ids, Some(*id))?;
            let def = store.graphic_styles.iter_mut().find(|s| s.id == *id).ok_or_else(|| error("style disappeared"))?;
            def.attributes = attributes;
        }
        Operation::Rename { style: id, name: n } => {
            name(&store, n, Some(*id))?;
            style(&store, *id)?;
            for s in &mut store.graphic_styles {
                if s.id == *id {
                    s.name = n.clone();
                }
            }
        }
        Operation::Duplicate { style: id, name: n } => {
            name(&store, n, None)?;
            let mut next = style(&store, *id)?.clone();
            next.id = next_id(&store)?;
            next.name = n.clone();
            store.graphic_styles.push(next);
        }
        Operation::Delete { style: id } => {
            style(&store, *id)?;
            store.graphic_styles.retain(|s| s.id != *id);
            store.links.retain(|_, l| l.style != *id);
        }
        Operation::BreakLink => break_link(&mut store, &request.targets),
        Operation::InstallStarters => {
            for mut def in photocraft_doc::vector_design::styles::starter_library() {
                if store.graphic_styles.iter().any(|s| s.name == def.name) {
                    continue;
                }
                def.id = next_id(&store)?;
                store.graphic_styles.push(def);
            }
        }
    }
    if store.graphic_styles.len() > 1024 {
        return Err(error("style library budget exceeded"));
    }
    render_caches(doc, &mut edits, projection)?;
    let mut document = doc.clone();
    for (layer, (id, style_link)) in edits.into_iter().zip(links) {
        *document.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
        store.links.remove(&id);
        if let Some(style) = style_link {
            store.links.insert(id, link(&document, id, style)?);
        }
    }
    Ok(Preview { request: request.clone(), document, store })
}
fn render_caches(doc: &Document, edits: &mut [Layer], projection: &dyn StrokeProjection) -> Result<()> {
    // First compile ALL geometry, then budget ALL tile caches, and only then rasterize.
    let mut shapes = Vec::new();
    let mut native = Vec::new();
    let mut on_paths = Vec::new();
    for (i, l) in edits.iter().enumerate() {
        match &l.content {
            LayerContent::Shape(sh) => {
                crate::vector_api::validate_path(&sh.path)?;
                let mut shape = sh.clone();
                if !l.vector.corners.is_empty() {
                    shape = crate::vector_packages::corners::project_shape(&shape, &l.vector.corners)?;
                }
                shapes.push((i, shape));
            }
            LayerContent::Text(t) => {
                if let Some(path) = &l.vector.on_path {
                    text::layout(doc, t)?; // same text/transform limits, before path projection
                    let mut engine = photocraft_text::shared().lock().unwrap_or_else(|e| e.into_inner());
                    let layout = crate::vector_packages::path_text::project(&mut engine, doc, t, path)?;
                    let outlines = crate::vector_packages::path_text::on_path::validated_outlines(&layout, &t.transform).map_err(|e| error(e.0))?;
                    shapes.push((i, text::from_outlines(t, outlines.into_iter().map(|o| o.elements).collect())?));
                    if l.vector.appearance.is_none() {
                        on_paths.push((i, layout));
                    }
                    continue;
                }
                let mut layout = text::layout(doc, t)?;
                let solid = l.vector.appearance.as_ref().and_then(|a| match a.items.as_slice() {
                    [AppearanceItem::Fill(f)] if f.visible && f.blend == photocraft_doc::BlendMode::Normal => match f.paint {
                        Fill::Solid(mut color) => {
                            color.alpha *= f.opacity;
                            Some(color)
                        }
                        _ => None,
                    },
                    _ => None,
                });
                if let Some(color) = solid {
                    // Preserve native antialiasing/faux-bold for ordinary one-colour text;
                    // the explicit appearance remains editable and authoritative metadata.
                    for style in &mut layout.styles {
                        style.color = color;
                    }
                    native.push((i, layout));
                } else if l.vector.appearance.is_some() {
                    shapes.push((i, text::shape(t, &layout)?));
                } else {
                    native.push((i, layout));
                }
            }
            _ => {}
        }
    }
    let plans = shapes
        .iter()
        .map(|(i, sh)| CompiledAppearance::with_projection(sh, edits[*i].vector.appearance.as_ref(), &doc.patterns, doc.bounds(), projection).map_err(error))
        .collect::<Result<Vec<_>>>()?;
    let native_bytes =
        native.iter().map(|(i, layout)| if let LayerContent::Text(t) = &edits[*i].content { text::bytes(doc, t, layout) } else { 0 }).sum::<u128>();
    if native_bytes + plans.iter().map(|p| p.cache_bytes(doc.pixel_format())).sum::<u128>() > CACHE_BUDGET {
        return Err(error("style prepared caches exceed 512 MiB"));
    }
    for ((i, _), plan) in shapes.iter().zip(plans) {
        if on_paths.iter().any(|(native, _)| native == i) {
            continue;
        }
        let cache = plan.render(doc.pixel_format(), doc.bounds()).map_err(error)?;
        let layer = &mut edits[*i];
        match &mut layer.content {
            LayerContent::Shape(sh) => {
                if let Some(a) = layer.vector.appearance.as_ref() {
                    renderer::mirror_legacy(sh, a);
                }
                sh.cache = Some(cache);
                sh.psd_raw = None;
            }
            LayerContent::Text(t) => {
                t.cache = Some(cache);
                t.psd_raw = None;
            }
            _ => {}
        }
    }
    for (i, layout) in native {
        if let LayerContent::Text(t) = &mut edits[i].content {
            t.cache = Some(text::render(doc, t, &layout));
            t.psd_raw = None;
        }
    }
    for (i, layout) in on_paths {
        if let LayerContent::Text(t) = &mut edits[i].content {
            t.cache = Some(
                crate::vector_packages::path_text::on_path::rasterize(&layout, &t.transform, doc.pixel_format(), t.antialias, doc.bounds())
                    .map_err(|e| error(e.0))?
                    .surface,
            );
            t.psd_raw = None;
        }
    }
    Ok(())
}
/// Shared type/geometry edit and reload hook: render explicit style paint after updating text
/// runs/path geometry, before publishing the owning edit's candidate.
pub fn refresh(doc: &Document, layer: &mut Layer) -> Result<()> {
    render_caches(doc, std::slice::from_mut(layer), &Projection)
}
pub fn preview(s: &Session, request: &Request) -> Result<Preview> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    prepare(&st.doc, st.revision, &store(&st.doc)?, request, &Projection)
}
pub fn commit_with_hooks(s: &mut Session, request: &Request, hooks: Option<Hooks>) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let needs_store = !matches!(request.operation, Operation::Copy { .. } | Operation::Color { .. });
    if needs_store && hooks.is_none() {
        return Err(error("Graphic Styles requires package 00 native persistence hooks"));
    }
    let original = hooks.map_or_else(|| Ok(Store::default()), |h| (h.read)(&st.doc))?;
    let p = prepare(&st.doc, st.revision, &original, request, &Projection)?;
    if p.document == *st.doc && p.store == original {
        return Ok(());
    }
    s.edit("Graphic Styles", move |doc, _| {
        *doc = p.document;
        if let Some(h) = hooks {
            (h.write)(doc, &p.store)?;
        }
        Ok(())
    })
}
pub fn commit(s: &mut Session, request: &Request) -> Result<()> {
    // When the command is registered, production calls gain normal Session journaling.
    // The pure module remains executable through the authorized Session::edit boundary.
    if crate::commands::find("vector.styles.edit").is_some() {
        s.execute("vector.styles.edit", serde_json::to_value(request).map_err(|e| error(e.to_string()))?)?;
        Ok(())
    } else {
        commit_with_hooks(s, request, installed_hooks())
    }
}
fn command(s: &mut Session, value: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
    commit_with_hooks(s, &r, installed_hooks())?;
    Ok(json!({"applied":true}))
}
fn query(s: &mut Session, value: &Value) -> Result<Value> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let hooks = installed_hooks().ok_or_else(|| error("Graphic Styles requires package 00 native persistence hooks"))?;
    let store = (hooks.read)(&st.doc)?;
    if let Some(id) = value.get("reference").and_then(Value::as_u64) {
        Ok(json!({"layers":same_style(&st.doc, &store, LayerId(id))?}))
    } else {
        Ok(json!(store))
    }
}
fn preview_command(s: &mut Session, value: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
    let p = preview(s, &r)?;
    Ok(json!({"detached":true,"targets":p.request.targets}))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("vector.styles.edit", command as fn(&mut Session, &Value) -> Result<Value>, true),
        ("vector.styles.query", query, false),
        ("vector.styles.preview", preview_command, false),
    ]
    .into_iter()
    .map(|(id, run, journal)| CommandSpec {
        id,
        label: "Graphic Styles",
        menu: &[],
        shortcut: None,
        params: "Request {document,revision,targets,operation}; query {reference?}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
/// Read one visible object's colour. Solid paint retains its colour model; gradients use the
/// renderer's midpoint/opacity/offset/dither evaluator; raster pixels use document coordinates.
pub fn color_at(doc: &Document, id: LayerId, x: f64, y: f64, stroke: bool) -> Result<Color> {
    if !x.is_finite() || !y.is_finite() || x.abs() > 1_000_000. || y.abs() > 1_000_000. {
        return Err(error("invalid sample coordinates"));
    }
    let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    if let LayerContent::Shape(sh) = &l.content {
        let a = sample(doc, id, None)?.appearance.unwrap_or_default();
        let paint = a
            .items
            .iter()
            .rev()
            .find_map(|i| match i {
                AppearanceItem::Stroke(s) if stroke && s.visible => Some(&s.base.paint),
                AppearanceItem::Fill(f) if !stroke && f.visible => Some(&f.paint),
                _ => None,
            })
            .or_else(|| {
                a.items.iter().rev().find_map(|i| match i {
                    AppearanceItem::Stroke(s) if s.visible => Some(&s.base.paint),
                    _ => None,
                })
            });
        if let Some(Fill::Solid(c)) = paint {
            return Ok(*c);
        }
        if let Some(paint) = paint
            && !matches!(paint, Fill::Pattern { .. })
        {
            let b = sh.path.control_bounds().ok_or_else(|| error("sample requires bounded paint"))?;
            let c = photocraft_vector::Paint::from_fill(paint, b, (0., 0., f64::from(doc.size.width), f64::from(doc.size.height))).at(x, y);
            return Ok(Color::rgba(c[0], c[1], c[2], c[3]));
        }
    }
    let surface = l.surface().ok_or_else(|| error("sample has no rendered pixels"))?;
    let c = surface.rgba(x.floor() as i32, y.floor() as i32);
    Ok(Color::rgba(c[0], c[1], c[2], c[3]))
}
#[cfg(test)]
#[path = "styles/gpu.rs"]
mod gpu;
#[cfg(test)]
#[path = "styles/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "styles/native_acceptance.rs"]
pub mod native_acceptance;
