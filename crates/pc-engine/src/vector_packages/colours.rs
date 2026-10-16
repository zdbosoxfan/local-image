//! Document swatch transactions, informed by VectorCraft `engine/src/cmd/swatch.rs`
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! 00 installs Access for additive document/paint storage. No palette is hidden in metadata.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::appearance::AppearanceItem;
pub use photocraft_doc::vector_design::swatches::*;
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId, SymbolId};
use photocraft_vector::appearance::{CompiledAppearance, StrokeProjection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::OnceLock;

pub(super) fn bad(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
/// Injected only by 00 after registering Document.swatches and optional paint references.
/// write participates in the same Session::edit as the pixels; it must not mutate external state.
#[derive(Clone, Copy)]
pub struct Access {
    pub read: fn(&Document) -> Result<SwatchBook>,
    pub write: fn(&mut Document, SwatchBook) -> Result<()>,
}
static ACCESS: OnceLock<Access> = OnceLock::new();
pub fn install_access(access: Access) -> Result<()> {
    ACCESS.set(access).map_err(|_| bad("document colour access already installed"))
}
pub fn access() -> Result<Access> {
    ACCESS.get().copied().ok_or_else(|| bad("package 00 must register document colour storage"))
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
        if self == Self::begin(s)? { Ok(()) } else { Err(bad("colour gesture is stale; start again")) }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    SetColor { targets: Vec<PaintTarget>, color: photocraft_doc::Color },
    Move { id: SwatchId, group: Option<u64>, before: Option<SwatchId> },
    RenameGroup { id: u64, name: String },
    Add { swatch: Swatch },
    Edit { swatch: Swatch },
    Delete { ids: Vec<SwatchId> },
    Merge { into: SwatchId, ids: Vec<SwatchId> },
    Group { group: SwatchGroup, ids: Vec<SwatchId> },
    Ungroup { id: u64 },
    DeleteGroup { id: u64 },
    AddUsed { global: bool, group: Option<u64> },
    Assign { targets: Vec<PaintTarget>, swatch: SwatchId, tint: f32 },
    AssignPaint { targets: Vec<PaintTarget>, swatch: SwatchId },
    Import { palette: SwatchBook, replace: bool },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capture: Capture,
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub document: Document,
    pub book: SwatchBook,
    pub warnings: Vec<photocraft_doc::vector_design::exchange::VectorWarning>,
}

fn tree(doc: &Document, symbol: Option<SymbolId>) -> Result<&[Layer]> {
    match symbol {
        None => Ok(&doc.layers),
        Some(id) => doc.symbols.iter().find(|d| d.id == id).map(|d| d.layers.as_slice()).ok_or_else(|| bad("missing symbol master")),
    }
}
fn tree_mut(doc: &mut Document, symbol: Option<SymbolId>) -> Result<&mut [Layer]> {
    match symbol {
        None => Ok(&mut doc.layers),
        Some(id) => doc.symbols.iter_mut().find(|d| d.id == id).map(|d| d.layers.as_mut_slice()).ok_or_else(|| bad("missing symbol master")),
    }
}
fn find(layers: &[Layer], id: LayerId) -> Option<&Layer> {
    for l in layers {
        if l.id == id {
            return Some(l);
        }
        if let LayerContent::Group(g) = &l.content
            && let Some(found) = find(&g.children, id)
        {
            return Some(found);
        }
    }
    None
}
fn find_mut(layers: &mut [Layer], id: LayerId) -> Option<&mut Layer> {
    for l in layers {
        if l.id == id {
            return Some(l);
        }
        if let LayerContent::Group(g) = &mut l.content
            && let Some(found) = find_mut(&mut g.children, id)
        {
            return Some(found);
        }
    }
    None
}
fn locked(layers: &[Layer], id: LayerId, inherited: bool) -> Option<bool> {
    for l in layers {
        let lock = inherited || l.locks.all || l.locks.pixels;
        if l.id == id {
            return Some(lock);
        }
        if let LayerContent::Group(g) = &l.content
            && let Some(v) = locked(&g.children, id, lock)
        {
            return Some(v);
        }
    }
    None
}
pub fn layer<'a>(doc: &'a Document, t: &PaintTarget) -> Result<&'a Layer> {
    find(tree(doc, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))
}
fn paint<'a>(l: &'a Layer, slot: &PaintSlot) -> Result<std::borrow::Cow<'a, Fill>> {
    if let (LayerContent::Text(t), PaintSlot::Fill) = (&l.content, slot) {
        return Ok(std::borrow::Cow::Owned(Fill::Solid(t.color)));
    }
    match slot {
        PaintSlot::Fill => match &l.content {
            LayerContent::Shape(sh) => sh.fill.as_ref(),
            LayerContent::Fill(f) => Some(f),
            _ => None,
        },
        PaintSlot::Stroke => match &l.content {
            LayerContent::Shape(sh) => sh.stroke.as_ref().map(|s| &s.paint),
            _ => None,
        },
        PaintSlot::Appearance(i) => l.vector.appearance.as_ref().and_then(|a| a.items.get(*i)).and_then(|it| match it {
            AppearanceItem::Fill(f) => Some(&f.paint),
            AppearanceItem::Stroke(s) => Some(&s.base.paint),
            AppearanceItem::Mesh(_) => None,
        }),
    }
    .map(std::borrow::Cow::Borrowed)
    .ok_or_else(|| bad("missing paint slot"))
}
fn paint_mut<'a>(l: &'a mut Layer, slot: &PaintSlot) -> Result<&'a mut Fill> {
    match slot {
        PaintSlot::Fill => match &mut l.content {
            LayerContent::Shape(sh) => sh.fill.as_mut(),
            LayerContent::Fill(f) => Some(f),
            _ => None,
        },
        PaintSlot::Stroke => match &mut l.content {
            LayerContent::Shape(sh) => sh.stroke.as_mut().map(|s| &mut s.paint),
            _ => None,
        },
        PaintSlot::Appearance(i) => l.vector.appearance.as_mut().and_then(|a| a.items.get_mut(*i)).and_then(|it| match it {
            AppearanceItem::Fill(f) => Some(&mut f.paint),
            AppearanceItem::Stroke(s) => Some(&mut s.base.paint),
            AppearanceItem::Mesh(_) => None,
        }),
    }
    .ok_or_else(|| bad("missing paint slot"))
}
fn set_paint(l: &mut Layer, slot: &PaintSlot, paint: Fill) -> Result<()> {
    if let (LayerContent::Text(text), PaintSlot::Fill) = (&mut l.content, slot) {
        let Fill::Solid(color) = paint else {
            return Err(bad("Text swatches require a solid colour"));
        };
        text.color = color;
        for run in &mut text.runs {
            run.style.color = color;
        }
        text.psd_raw = None;
        return Ok(());
    }
    *paint_mut(l, slot)? = paint;
    Ok(())
}
pub fn color(doc: &Document, t: &PaintTarget) -> Result<photocraft_doc::Color> {
    match (paint(layer(doc, t)?, &t.paint)?.as_ref(), t.stop) {
        (Fill::Solid(c), None) => Ok(*c),
        (Fill::Gradient { stops, .. }, Some(i)) => stops.get(i).map(|s| s.1).ok_or_else(|| bad("missing gradient stop")),
        _ => Err(bad("target must be a solid paint or gradient stop")),
    }
}
pub fn validate_target(doc: &Document, t: &PaintTarget) -> Result<()> {
    if locked(tree(doc, t.symbol)?, t.layer, false).ok_or(EngineError::NoLayer(t.layer))? {
        return Err(bad("colour target is locked"));
    }
    color(doc, t)?;
    Ok(())
}
pub(super) fn set_color(doc: &mut Document, t: &PaintTarget, value: photocraft_doc::Color) -> Result<()> {
    validate_color(value).map_err(bad)?;
    let l = find_mut(tree_mut(doc, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))?;
    if let (LayerContent::Text(text), PaintSlot::Fill, None) = (&mut l.content, &t.paint, t.stop) {
        text.color = value;
        for run in &mut text.runs {
            run.style.color = value;
        }
        text.psd_raw = None;
        return Ok(());
    }
    match (paint_mut(l, &t.paint)?, t.stop) {
        (Fill::Solid(c), None) => {
            *c = value;
        }
        (Fill::Gradient { stops, .. }, Some(i)) => {
            stops.get_mut(i).ok_or_else(|| bad("missing gradient stop"))?.1 = value;
        }
        _ => return Err(bad("target must be solid or gradient stop")),
    }
    if let LayerContent::Shape(sh) = &mut l.content {
        sh.psd_raw = None;
    }
    // Native PSD vector blocks are no longer authoritative after a colour change.
    l.psd_blocks.retain(|(k, _)| !matches!(k, b"SoCo" | b"GdFl" | b"vscg" | b"vstk"));
    Ok(())
}
#[derive(Clone, Debug)]
pub struct UsedColor {
    pub target: PaintTarget,
    pub color: photocraft_doc::Color,
    pub reference: Option<SwatchRef>,
}
/// Visits authoritative paints once: appearance if present, otherwise legacy fill/stroke.
/// Pattern paints are explicit warnings. Groups traverse; selected symbol instances address
/// their master (so recolouring a symbol remains linked and affects every instance).
pub fn used_colors(
    doc: &Document,
    book: &SwatchBook,
    selection: Option<&[LayerId]>,
) -> Result<(Vec<UsedColor>, Vec<photocraft_doc::vector_design::exchange::VectorWarning>)> {
    super::symbols::validation::document(doc).map_err(|e| bad(e.to_string()))?;
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    fn walk(
        doc: &Document,
        book: &SwatchBook,
        l: &Layer,
        symbol: Option<SymbolId>,
        out: &mut Vec<UsedColor>,
        warnings: &mut Vec<photocraft_doc::vector_design::exchange::VectorWarning>,
        seen: &mut HashSet<(Option<u64>, u64)>,
    ) -> Result<()> {
        if !seen.insert((symbol.map(|s| s.0), l.id.0)) {
            return Ok(());
        }
        if let LayerContent::Group(g) = &l.content {
            for c in &g.children {
                walk(doc, book, c, symbol, out, warnings, seen)?;
            }
        }
        if let LayerContent::Symbol(s) = &l.content {
            for c in tree(doc, Some(s.symbol))? {
                walk(doc, book, c, Some(s.symbol), out, warnings, seen)?;
            }
        }
        let slots = if matches!(l.content, LayerContent::Text(_)) {
            vec![PaintSlot::Fill]
        } else if let Some(a) = &l.vector.appearance {
            (0..a.items.len()).map(PaintSlot::Appearance).collect::<Vec<_>>()
        } else {
            match &l.content {
                LayerContent::Shape(sh) => sh.fill.iter().map(|_| PaintSlot::Fill).chain(sh.stroke.iter().map(|_| PaintSlot::Stroke)).collect(),
                LayerContent::Fill(_) => vec![PaintSlot::Fill],
                LayerContent::Text(_) => vec![PaintSlot::Fill],
                LayerContent::Raster(_) | LayerContent::Smart(_) => {
                    warnings.push(warning("recolor-raster", "Raster pixels are not recoloured"));
                    Vec::new()
                }
                _ => Vec::new(),
            }
        };
        for slot in slots {
            let p = paint(l, &slot)?;
            let colors = match p.as_ref() {
                Fill::Solid(c) => vec![(None, *c)],
                Fill::Gradient { stops, .. } => stops.iter().enumerate().map(|(i, s)| (Some(i), s.1)).collect(),
                Fill::Pattern { .. } => {
                    warnings.push(warning("recolor-pattern", "Pattern paints are not recoloured"));
                    Vec::new()
                }
            };
            for (stop, c) in colors {
                let target = PaintTarget { symbol, layer: l.id, paint: slot.clone(), stop };
                let reference = book.links.iter().find(|l| l.target == target).map(|l| l.reference);
                let c = reference.map_or(Ok(c), |r| book.resolve(r)).map_err(bad)?;
                validate_color(c).map_err(bad)?;
                out.push(UsedColor { target, color: c, reference });
            }
        }
        Ok(())
    }
    if let Some(ids) = selection {
        if ids.is_empty() || ids.len() > 100_000 {
            return Err(bad("select 1..100000 objects"));
        }
        let mut unique = HashSet::new();
        for id in ids {
            if !unique.insert(*id) {
                return Err(bad("duplicate selection target"));
            }
            if locked(&doc.layers, *id, false).ok_or(EngineError::NoLayer(*id))? {
                return Err(bad("selected colour object is locked"));
            }
            walk(doc, book, doc.layer(*id).ok_or(EngineError::NoLayer(*id))?, None, &mut out, &mut warnings, &mut seen)?;
        }
    } else {
        for l in &doc.layers {
            walk(doc, book, l, None, &mut out, &mut warnings, &mut seen)?;
        }
        for s in &doc.symbols {
            for l in &s.layers {
                walk(doc, book, l, Some(s.id), &mut out, &mut warnings, &mut seen)?;
            }
        }
    }
    if out.len() > MAX_LINKS {
        return Err(bad("colour extraction budget exceeded"));
    }
    warnings.sort_by(|a, b| a.feature.cmp(&b.feature));
    warnings.dedup_by(|a, b| a.feature == b.feature);
    Ok((out, warnings))
}

fn resolve_swatches(book: &mut SwatchBook) -> Result<()> {
    let colors: Vec<_> = book
        .swatches
        .iter()
        .flat_map(|w| w.stops.iter().map(move |s| (w.id, s.stop, s.reference)))
        .map(|(id, stop, r)| Ok((id, stop, book.resolve(r).map_err(bad)?)))
        .collect::<Result<_>>()?;
    for (id, i, c) in colors {
        if let Some(w) = book.swatches.iter_mut().find(|w| w.id == id)
            && let Fill::Gradient { stops, .. } = &mut w.paint
        {
            stops[i].1 = c;
        }
    }
    Ok(())
}
/// All validation precedes cache preparation. Caches stay bounds-limited and aggregate bytes
/// are checked before rendering. 08's projection provider can handle brush/profile strokes.
pub fn finish(mut document: Document, mut book: SwatchBook, changed: &[PaintTarget], projection: &dyn StrokeProjection) -> Result<Preview> {
    book.validate().map_err(bad)?;
    for link in &book.links {
        color(&document, &link.target)?;
    }
    resolve_swatches(&mut book)?;
    let mut keys = HashSet::new();
    let mut targets = Vec::new();
    for t in changed {
        validate_target(&document, t)?;
        if keys.insert((t.symbol.map(|s| s.0), t.layer.0)) {
            targets.push(t.clone());
        }
    }
    let render_shape = |l: &Layer| -> Result<Option<photocraft_doc::ShapeLayer>> {
        let LayerContent::Shape(sh) = &l.content else {
            return Ok(None);
        };
        // Keep the authoritative vector envelope (including clipping) on the
        // layer. The display/export projection applies it to this refreshed
        // base paint cache; swatch edits must not bake or discard that envelope.
        let mut shape = sh.clone();
        if let Some(live) = &l.vector.live_shape {
            shape.path = photocraft_vector::design_shapes::generate(live).map_err(|e| bad(e.to_string()))?;
        }
        Ok(Some(super::corners::project_shape(&shape, &l.vector.corners)?))
    };
    let mut bytes = 0u128;
    for t in &targets {
        let l = layer(&document, t)?;
        if let Some(sh) = render_shape(l)? {
            let compiled =
                CompiledAppearance::with_projection(&sh, l.vector.appearance.as_ref(), &document.patterns, document.bounds(), projection).map_err(bad)?;
            bytes += compiled.cache_bytes(document.pixel_format());
            if bytes > 512 << 20 {
                return Err(bad("colour cache aggregate budget exceeded"));
            }
        }
    }
    for t in &targets {
        let mut l = layer(&document, t)?.clone();
        let projected = render_shape(&l)?;
        if let (LayerContent::Shape(sh), Some(projected)) = (&mut l.content, projected) {
            let compiled = CompiledAppearance::with_projection(&projected, l.vector.appearance.as_ref(), &document.patterns, document.bounds(), projection)
                .map_err(bad)?;
            let cache = compiled.render(document.pixel_format(), document.bounds()).map_err(bad)?;
            if let Some(a) = &l.vector.appearance {
                photocraft_vector::appearance::mirror_legacy(sh, a);
            }
            sh.cache = Some(cache);
        }
        if let LayerContent::Text(text) = &mut l.content {
            crate::vector_integration::refresh_text(&document, text, &l.vector)?;
        }
        *find_mut(tree_mut(&mut document, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))? = l;
    }
    // Refresh affected instances from the already-resolved master cache and appearance.
    let affected: HashSet<_> = targets.iter().filter_map(|t| t.symbol).flat_map(|s| super::symbols::dependents(&document, s)).collect();
    fn refresh(doc: &Document, ls: &mut [Layer], set: &HashSet<u64>) -> Result<()> {
        for l in ls {
            match &mut l.content {
                LayerContent::Symbol(s) if set.contains(&s.symbol.0) => {
                    s.cache = Some(super::symbols::render_instance(doc, s)?);
                }
                LayerContent::Group(g) => refresh(doc, &mut g.children, set)?,
                _ => {}
            }
        }
        Ok(())
    }
    let snapshot = document.clone();
    refresh(&snapshot, &mut document.layers, &affected)?;
    for s in &mut document.symbols {
        refresh(&snapshot, &mut s.layers, &affected)?;
    }
    Ok(Preview { document, book, warnings: Vec::new() })
}
fn unlink(book: &mut SwatchBook, ids: &[SwatchId]) {
    book.links.retain(|l| !ids.contains(&l.reference.swatch));
    for w in &mut book.swatches {
        w.stops.retain(|s| !ids.contains(&s.reference.swatch));
    }
}
pub fn prepare(s: &Session, access: Access, r: &Request) -> Result<Preview> {
    prepare_with_projection(s, access, r, &super::styles::Projection)
}
pub fn prepare_with_projection(s: &Session, access: Access, r: &Request, projection: &dyn StrokeProjection) -> Result<Preview> {
    r.capture.check(s)?;
    let mut document = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    let mut book = (access.read)(&document)?;
    book.validate().map_err(bad)?;
    // Resolve every old link first, so deletion/global-off keeps the pixels even if a cache
    // was stale on load. Validate targets that change; unrelated locked art is left alone.
    let mut changed = Vec::new();
    for l in &book.links {
        let c = book.resolve(l.reference).map_err(bad)?;
        if color(&document, &l.target)? != c {
            validate_target(&document, &l.target)?;
            set_color(&mut document, &l.target, c)?;
            changed.push(l.target.clone());
        }
    }
    resolve_swatches(&mut book)?;
    let mut warnings = Vec::new();
    match &r.action {
        Action::SetColor { targets, color } => {
            validate_color(*color).map_err(bad)?;
            if targets.is_empty() || targets.len() > MAX_LINKS {
                return Err(bad("colour assignment needs bounded targets"));
            }
            let mut unique = HashSet::new();
            for t in targets {
                if !unique.insert(t) || locked(tree(&document, t.symbol)?, t.layer, false) != Some(false) {
                    return Err(bad("colour target is duplicate, missing or locked"));
                }
                if t.stop.is_some() {
                    validate_target(&document, t)?;
                } else {
                    paint(layer(&document, t)?, &t.paint)?;
                }
            }
            for t in targets {
                if t.stop.is_some() {
                    set_color(&mut document, t, *color)?;
                    book.links.retain(|l| l.target != *t);
                } else {
                    let l = find_mut(tree_mut(&mut document, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))?;
                    set_paint(l, &t.paint, Fill::Solid(*color))?;
                    set_color(&mut document, t, *color)?;
                    book.links.retain(|l| !(l.target.layer == t.layer && l.target.symbol == t.symbol && l.target.paint == t.paint));
                }
                changed.push(t.clone());
            }
        }
        Action::Move { id, group, before } => move_swatch(&mut book, *id, *group, *before).map_err(bad)?,
        Action::RenameGroup { id, name } => {
            book.groups.iter_mut().find(|g| g.id == *id).ok_or_else(|| bad("missing group"))?.name = name.clone();
        }
        Action::Add { swatch } => {
            book.swatches.push(swatch.clone());
        }
        Action::Edit { swatch } => {
            let old = book.swatch(swatch.id).ok_or_else(|| bad("missing swatch"))?;
            if matches!(old.paint, Fill::Solid(_)) != matches!(swatch.paint, Fill::Solid(_)) {
                return Err(bad("replace swatches only with the same paint kind"));
            }
            if old.global && !swatch.global {
                unlink(&mut book, &[swatch.id]);
            }
            *book.swatches.iter_mut().find(|w| w.id == swatch.id).ok_or_else(|| bad("missing swatch"))? = swatch.clone();
        }
        Action::Delete { ids } => {
            if ids.is_empty() || ids.iter().any(|id| book.swatch(*id).is_none()) {
                return Err(bad("missing swatch deletion target"));
            }
            unlink(&mut book, ids);
            book.swatches.retain(|w| !ids.contains(&w.id));
        }
        Action::Merge { into, ids } => {
            let keep = book.swatch(*into).cloned().ok_or_else(|| bad("missing merge swatch"))?;
            let Fill::Solid(base) = keep.paint else {
                return Err(bad("merge requires solid swatches"));
            };
            if ids.is_empty() || ids.contains(into) || ids.iter().any(|id| !book.swatch(*id).is_some_and(|w| matches!(w.paint, Fill::Solid(_)))) {
                return Err(bad("invalid merge target set"));
            }
            for l in &mut book.links {
                if ids.contains(&l.reference.swatch) {
                    validate_target(&document, &l.target)?;
                    l.reference.swatch = *into;
                    set_color(&mut document, &l.target, tint(base, l.reference.tint).map_err(bad)?)?;
                    changed.push(l.target.clone());
                }
            }
            for w in &mut book.swatches {
                for l in &mut w.stops {
                    if ids.contains(&l.reference.swatch) {
                        if let Fill::Gradient { stops, .. } = &mut w.paint {
                            stops[l.stop].1 = tint(base, l.reference.tint).map_err(bad)?;
                        }
                        l.reference.swatch = *into;
                    }
                }
            }
            if !keep.global {
                unlink(&mut book, &[*into]);
            }
            book.swatches.retain(|w| !ids.contains(&w.id));
        }
        Action::Group { group, ids } => {
            for id in ids {
                if book.swatch(*id).is_none() {
                    return Err(bad("missing swatch group member"));
                }
            }
            if let Some(g) = book.groups.iter_mut().find(|g| g.id == group.id) {
                *g = group.clone();
            } else {
                book.groups.push(group.clone());
            }
            for w in &mut book.swatches {
                if ids.contains(&w.id) {
                    w.group = Some(group.id);
                }
            }
        }
        Action::Ungroup { id } => {
            if !book.groups.iter().any(|g| g.id == *id) {
                return Err(bad("missing swatch group"));
            }
            book.groups.retain(|g| g.id != *id);
            for w in &mut book.swatches {
                if w.group == Some(*id) {
                    w.group = None;
                }
            }
        }
        Action::DeleteGroup { id } => {
            if !book.groups.iter().any(|g| g.id == *id) {
                return Err(bad("missing swatch group"));
            }
            let ids: Vec<_> = book.swatches.iter().filter(|w| w.group == Some(*id)).map(|w| w.id).collect();
            unlink(&mut book, &ids);
            book.swatches.retain(|w| w.group != Some(*id));
            book.groups.retain(|g| g.id != *id);
        }
        Action::AddUsed { global, group } => {
            let (used, warn) = used_colors(&document, &book, None)?;
            warnings = warn;
            for u in used {
                let id = match book.swatches.iter().find(|w| w.paint == Fill::Solid(u.color) && (!*global || w.global)) {
                    Some(w) => w.id,
                    None => {
                        let id = SwatchId(book.next_id().map_err(bad)?);
                        let [r, g, b, _] = u.color.to_rgba8();
                        let name = book.free_name(&format!("#{r:02X}{g:02X}{b:02X}"));
                        book.swatches.push(Swatch { id, name, paint: Fill::Solid(u.color), global: *global, spot: false, group: *group, stops: Vec::new() });
                        id
                    }
                };
                if *global && u.reference.is_none() {
                    validate_target(&document, &u.target)?;
                    book.links.push(PaintLink { target: u.target, reference: SwatchRef { swatch: id, tint: 100. } });
                }
            }
        }
        Action::Assign { targets, swatch, tint } => {
            let reference = SwatchRef { swatch: *swatch, tint: *tint };
            let c = book.resolve(reference).map_err(bad)?;
            let mut unique = HashSet::new();
            if targets.is_empty() {
                return Err(bad("assignment needs targets"));
            }
            for t in targets {
                if !unique.insert(t) {
                    return Err(bad("duplicate colour target"));
                }
                if t.stop.is_some() {
                    validate_target(&document, t)?;
                } else {
                    if locked(tree(&document, t.symbol)?, t.layer, false) != Some(false) {
                        return Err(bad("colour target is missing or locked"));
                    }
                    paint(layer(&document, t)?, &t.paint)?;
                }
            }
            for t in targets {
                if t.stop.is_none() {
                    let l = find_mut(tree_mut(&mut document, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))?;
                    set_paint(l, &t.paint, Fill::Solid(c))?;
                    book.links.retain(|l| !(l.target.layer == t.layer && l.target.symbol == t.symbol && l.target.paint == t.paint));
                }
                set_color(&mut document, t, c)?;
                book.links.retain(|l| l.target != *t);
                book.links.push(PaintLink { target: t.clone(), reference });
                changed.push(t.clone());
            }
        }
        Action::AssignPaint { targets, swatch } => {
            let w = book.swatch(*swatch).cloned().ok_or_else(|| bad("missing swatch"))?;
            let mut unique = HashSet::new();
            if targets.is_empty() {
                return Err(bad("assignment needs targets"));
            }
            for t in targets {
                if !unique.insert(t) || (t.stop.is_some() && !matches!(w.paint, Fill::Solid(_))) {
                    return Err(bad("paint assignment needs unique whole paints or solid stop colours"));
                }
                if t.stop.is_some() {
                    validate_target(&document, t)?;
                }
                if locked(tree(&document, t.symbol)?, t.layer, false) != Some(false) {
                    return Err(bad("colour target is missing or locked"));
                }
                paint(layer(&document, t)?, &t.paint)?;
            }
            for t in targets {
                if let (Some(_), Fill::Solid(c)) = (t.stop, &w.paint) {
                    set_color(&mut document, t, *c)?;
                    book.links.retain(|l| l.target != *t);
                    if w.global {
                        book.links.push(PaintLink { target: t.clone(), reference: SwatchRef { swatch: w.id, tint: 100. } });
                    }
                    changed.push(t.clone());
                    continue;
                }
                let l = find_mut(tree_mut(&mut document, t.symbol)?, t.layer).ok_or(EngineError::NoLayer(t.layer))?;
                set_paint(l, &t.paint, w.paint.clone())?;
                if let LayerContent::Shape(sh) = &mut l.content {
                    sh.psd_raw = None;
                }
                l.psd_blocks.retain(|(k, _)| !matches!(k, b"SoCo" | b"GdFl" | b"vscg" | b"vstk"));
                book.links.retain(|l| !(l.target.layer == t.layer && l.target.symbol == t.symbol && l.target.paint == t.paint));
                if w.global {
                    book.links.push(PaintLink { target: t.clone(), reference: SwatchRef { swatch: w.id, tint: 100. } });
                }
                for stop in &w.stops {
                    let mut target = t.clone();
                    target.stop = Some(stop.stop);
                    book.links.push(PaintLink { target, reference: stop.reference });
                }
                let mut marker = t.clone();
                if matches!(w.paint, Fill::Gradient { .. }) {
                    marker.stop = Some(0);
                }
                changed.push(marker);
            }
        }
        Action::Import { palette, replace } => {
            palette.validate().map_err(bad)?;
            if !palette.links.is_empty() {
                return Err(bad("imported palette cannot reference document layers"));
            }
            if *replace {
                book = palette.clone();
            } else {
                let mut remap = std::collections::HashMap::new();
                for g in &palette.groups {
                    let id = book.next_id().map_err(bad)?;
                    remap.insert(g.id, id);
                    book.groups.push(SwatchGroup { id, name: book.free_name(&g.name) });
                }
                for w in &palette.swatches {
                    let id = book.next_id().map_err(bad)?;
                    remap.insert(w.id.0, id);
                    let mut w = w.clone();
                    w.id = SwatchId(id);
                    w.name = book.free_name(&w.name);
                    w.group = w.group.and_then(|id| remap.get(&id).copied());
                    book.swatches.push(w);
                }
                let imported: HashSet<_> = remap.values().copied().collect();
                for w in &mut book.swatches {
                    if imported.contains(&w.id.0) {
                        for l in &mut w.stops {
                            l.reference.swatch = SwatchId(*remap.get(&l.reference.swatch.0).ok_or_else(|| bad("missing imported swatch reference"))?);
                        }
                    }
                }
            }
        }
    }
    book.validate().map_err(bad)?;
    for l in &book.links {
        let c = book.resolve(l.reference).map_err(bad)?;
        if color(&document, &l.target)? != c {
            validate_target(&document, &l.target)?;
            set_color(&mut document, &l.target, c)?;
            changed.push(l.target.clone());
        }
    }
    let mut preview = finish(document, book, &changed, projection)?;
    preview.warnings = warnings;
    (access.write)(&mut preview.document, preview.book.clone())?;
    Ok(preview)
}
pub(super) fn publish(s: &mut Session, capture: Capture, preview: Preview, label: &str) -> Result<()> {
    capture.check(s)?;
    if preview.document.id.0 != capture.document {
        return Err(bad("preview belongs to another document"));
    }
    s.edit(label, move |d, _| {
        *d = preview.document;
        Ok(())
    })
}
pub fn commit(s: &mut Session, access: Access, request: &Request) -> Result<()> {
    let p = prepare(s, access, request)?;
    publish(s, request.capture, p, "Edit Document Swatches")
}
fn run(s: &mut Session, v: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    commit(s, access()?, &r)?;
    Ok(json!({"changed":true}))
}
fn list(s: &mut Session, _: &Value) -> Result<Value> {
    serde_json::to_value((access()?.read)(&s.active().ok_or(EngineError::NoDocument)?.doc)?).map_err(|e| bad(e.to_string()))
}
fn preview_query(s: &mut Session, v: &Value) -> Result<Value> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    let p = prepare(s, access()?, &r)?;
    Ok(json!({"detached":true,"palette":p.book,"warnings":p.warnings}))
}
/// 00 appends these specs once after installing Access. UI calls the public detached preview.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "swatch.document.edit",
            label: "Edit Document Swatches",
            menu: &[],
            shortcut: None,
            params: "{capture:{document,revision},action:Action}",
            enabled: crate::vector_cmds::has_doc,
            run,
            journal: true,
        },
        CommandSpec {
            id: "swatch.document.list",
            label: "Document Swatches",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: crate::vector_cmds::has_doc,
            run: list,
            journal: false,
        },
        CommandSpec {
            id: "swatch.document.preview",
            label: "Preview Document Swatches",
            menu: &[],
            shortcut: None,
            params: "{capture,action}",
            enabled: crate::vector_cmds::has_doc,
            run: preview_query,
            journal: false,
        },
    ]
}
#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
#[path = "colours/native.rs"]
mod native;

#[cfg(test)]
#[path = "colours/shared_swatches_tests.rs"]
mod shared_swatches_tests;
