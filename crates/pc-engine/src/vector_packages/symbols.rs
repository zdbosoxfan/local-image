//! V8 Symbols: ordinary layer templates, detached preparation and one undoable publication.
//! Original GPL-3.0-or-later implementation; Affinity Symbols and SVG 1.1 reference semantics.
//! Core activation is the atomic package-00 patch in integration/15.md.
use crate::{EngineError, Result, Session, commands::CommandSpec};
pub use photocraft_doc::vector_design::{symbol_expansion as expansion, symbol_validation as validation};
use photocraft_doc::{Affine, DocId, Document, Layer, LayerContent, LayerId, SymbolDef, SymbolId, SymbolInstance};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod render;
pub use render::{prepare_expansion, refresh_layer_symbols, render_instance, transform_cached_instance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
}
impl Capture {
    pub fn new(s: &Session) -> Result<Self> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        Ok(Self { document: st.doc.id, revision: st.revision })
    }
    pub fn check(self, s: &Session) -> Result<()> {
        if self == Self::new(s)? { Ok(()) } else { Err(bad("symbol gesture is stale")) }
    }
    pub fn params(self) -> Value {
        json!({"document":self.document.0,"revision":self.revision})
    }
}
pub(super) fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
fn valid(doc: &Document) -> Result<()> {
    validation::document(doc).map_err(|e| bad(e.to_string()))
}
fn capture(p: &Value) -> Result<Capture> {
    Ok(Capture { document: DocId(number(p, "document")?), revision: number(p, "revision")? })
}
fn number(p: &Value, key: &str) -> Result<u64> {
    p.get(key).and_then(Value::as_u64).ok_or_else(|| bad(format!("{key} required")))
}
fn ids(p: &Value) -> Result<Vec<LayerId>> {
    serde_json::from_value::<Vec<u64>>(p.get("layers").cloned().ok_or_else(|| bad("layers required"))?)
        .map(|v| v.into_iter().map(LayerId).collect())
        .map_err(|e| bad(e.to_string()))
}
fn name(p: &Value) -> Result<String> {
    let name = p.get("name").and_then(Value::as_str).ok_or_else(|| bad("symbol name required"))?.trim();
    if name.is_empty() || name.len() > 1024 { Err(bad("symbol name must contain 1–1024 bytes")) } else { Ok(name.into()) }
}
fn symbol(p: &Value) -> Result<SymbolId> {
    Ok(SymbolId(number(p, "symbol")?))
}
fn transform(p: &Value) -> Result<Affine> {
    let a = match p.get("transform") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?,
        None => Affine::IDENTITY,
    };
    validation::affine(a).map_err(|e| bad(e.to_string()))?;
    Ok(a)
}
fn target_locks(doc: &Document, ids: &[LayerId], position: bool) -> Result<()> {
    if ids.is_empty() {
        return Err(bad("select at least one layer"));
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.0) {
            return Err(bad("duplicate target layer ID"));
        }
        let l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        let locks = doc.effective_locks(*id);
        if locks.all || locks.pixels || (position && locks.position) {
            return Err(bad(format!("layer {} is locked", l.name)));
        }
        if let Some(children) = l.children() {
            for child in children {
                target_locks(doc, &[child.id], position)?;
            }
        }
    }
    Ok(())
}
fn targets(doc: &Document, ids: &[LayerId], position: bool) -> Result<()> {
    target_locks(doc, ids, position)?;
    for a in ids {
        for b in ids {
            if a != b && doc.path_of(*b).zip(doc.path_of(*a)).is_some_and(|(bp, ap)| bp.starts_with(&ap)) {
                return Err(bad("target set contains ancestor and descendant"));
            }
        }
    }
    Ok(())
}
fn selection(doc: &Document, ids: &[LayerId]) -> Result<(Vec<Layer>, LayerId)> {
    targets(doc, ids, true)?;
    let mut paths = ids.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    paths.sort();
    let layers: Vec<Layer> = paths.iter().map(|path| doc.layer_at(path).cloned().ok_or_else(|| bad("missing selection"))).collect::<Result<_>>()?;
    let top = layers.last().ok_or_else(|| bad("empty selection"))?.id;
    Ok((layers, top))
}
fn publish(s: &mut Session, c: Capture, label: &str, doc: Document, active: Option<LayerId>) -> Result<()> {
    c.check(s)?;
    valid(&doc)?;
    s.edit(label, move |target, selection| {
        *target = doc;
        if let Some(id) = active {
            *selection = Some(id);
        }
        Ok(())
    })
}
/// All transitive dependents, including the edited master, for renderer/scaled cache keys.
pub fn dependents(doc: &Document, changed: SymbolId) -> BTreeSet<u64> {
    fn references(layers: &[Layer], set: &BTreeSet<u64>) -> bool {
        layers.iter().any(|l| match &l.content {
            LayerContent::Symbol(s) => set.contains(&s.symbol.0),
            LayerContent::Group(g) => references(&g.children, set),
            _ => false,
        })
    }
    let mut set = BTreeSet::from([changed.0]);
    loop {
        let previous = set.len();
        for def in &doc.symbols {
            if references(&def.layers, &set) {
                set.insert(def.id.0);
            }
        }
        if previous == set.len() {
            return set;
        }
    }
}
fn refresh_dependents(doc: &mut Document, changed: SymbolId) -> Result<()> {
    valid(doc)?;
    let affected = dependents(doc, changed);
    fn refresh(snapshot: &Document, layers: &mut [Layer], affected: &BTreeSet<u64>) -> Result<()> {
        for layer in layers {
            match &mut layer.content {
                LayerContent::Symbol(instance) if affected.contains(&instance.symbol.0) => instance.cache = Some(render_instance(snapshot, instance)?),
                LayerContent::Group(g) => refresh(snapshot, &mut g.children, affected)?,
                _ => {}
            }
        }
        Ok(())
    }
    let snapshot = doc.clone();
    refresh(&snapshot, &mut doc.layers, &affected)?;
    for def in &mut doc.symbols {
        refresh(&snapshot, &mut def.layers, &affected)?;
    }
    Ok(())
}
/// Creates a master and replaces the captured selection with its first instance.
pub fn create_from_selection(s: &mut Session, c: Capture, ids: &[LayerId], name: String) -> Result<(SymbolId, LayerId)> {
    c.check(s)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    let (art, top) = selection(&doc, ids)?;
    let id = SymbolId(doc.symbols.iter().map(|d| d.id.0).max().unwrap_or(0).checked_add(1).ok_or_else(|| bad("symbol ID space exhausted"))?);
    if name.trim().is_empty() || name.len() > 1024 {
        return Err(bad("invalid symbol name"));
    }
    doc.symbols.push(SymbolDef { id, name: name.clone(), layers: art.iter().map(Layer::duplicate).collect() });
    let instance = SymbolInstance { symbol: id, transform: Affine::IDENTITY, cache: None };
    let mut layer = Layer::new(name, LayerContent::Symbol(instance));
    let layer_id = layer.id;
    // Match established layer.group: group at the topmost selected layer, preserve paint order.
    doc.insert_above(Some(top), layer.clone());
    for id in ids {
        doc.remove(*id).ok_or(EngineError::NoLayer(*id))?;
    }
    valid(&doc)?;
    if let LayerContent::Symbol(instance) = &mut layer.content {
        instance.cache = Some(render_instance(&doc, instance)?);
    }
    *doc.layer_mut(layer_id).ok_or(EngineError::NoLayer(layer_id))? = layer;
    publish(s, c, "Create Symbol", doc, Some(layer_id))?;
    Ok((id, layer_id))
}
/// Master edits accept ordinary detached layers, so all object types retain existing semantics.
/// Callers must capture document/revision before opening an editor. IDs must be unique globally.
pub fn master_preview(s: &Session, c: Capture, id: SymbolId, layers: Vec<Layer>) -> Result<Document> {
    c.check(s)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    let def = doc.symbols.iter_mut().find(|d| d.id == id).ok_or_else(|| bad("missing symbol"))?;
    def.layers = layers;
    valid(&doc)?;
    refresh_dependents(&mut doc, id)?;
    Ok(doc)
}
pub fn update_master(s: &mut Session, c: Capture, id: SymbolId, layers: Vec<Layer>) -> Result<()> {
    let doc = master_preview(s, c, id, layers)?;
    publish(s, c, "Edit Symbol Master", doc, None)
}
pub fn place_preview(s: &Session, c: Capture, id: SymbolId, a: Affine, above: Option<LayerId>) -> Result<(Document, LayerId)> {
    c.check(s)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    validation::affine(a).map_err(|e| bad(e.to_string()))?;
    let name = doc.symbols.iter().find(|d| d.id == id).ok_or_else(|| bad("missing symbol"))?.name.clone();
    if let Some(above) = above {
        let layer = doc.layer(above).ok_or(EngineError::NoLayer(above))?;
        let locks = doc.effective_locks(above);
        if locks.all || locks.pixels {
            return Err(bad(format!("layer {} is locked", layer.name)));
        }
    }
    let mut instance = SymbolInstance { symbol: id, transform: a, cache: None };
    instance.cache = Some(render_instance(&doc, &instance)?);
    let layer = Layer::new(name, LayerContent::Symbol(instance));
    let layer_id = layer.id;
    doc.insert_above(above, layer);
    valid(&doc)?;
    Ok((doc, layer_id))
}
fn place(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    let above = match p.get("above") {
        None | Some(Value::Null) => None,
        Some(v) => Some(LayerId(v.as_u64().ok_or_else(|| bad("above must be a layer ID"))?)),
    };
    let (doc, id) = place_preview(s, c, symbol(p)?, transform(p)?, above)?;
    publish(s, c, "Place Symbol", doc, Some(id))?;
    Ok(json!({"layer":id.0}))
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let (symbol, layer) = create_from_selection(s, capture(p)?, &ids(p)?, name(p)?)?;
    Ok(json!({"symbol":symbol.0,"layer":layer.0}))
}
fn update(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids = ids(p)?;
    targets(&st.doc, &ids, false)?;
    let (art, _) = selection(&st.doc, &ids)?;
    let id = symbol(p)?;
    update_master(s, c, id, art.iter().map(Layer::duplicate).collect())?;
    Ok(json!({"symbol":id.0}))
}
fn edit_master(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let id = symbol(p)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let def = doc.symbols.iter().find(|d| d.id == id).ok_or_else(|| bad("missing symbol"))?;
    let mut draft = (**doc).clone();
    draft.layers = def.layers.clone();
    let entries = p.get("edits").and_then(Value::as_array).ok_or_else(|| bad("master edits required"))?;
    let ids = entries.iter().map(|e| number(e, "layer").map(LayerId)).collect::<Result<Vec<_>>>()?;
    target_locks(&draft, &ids, false)?;
    for (entry, id) in entries.iter().zip(&ids) {
        let l = draft.layer_mut(*id).ok_or(EngineError::NoLayer(*id))?;
        if let Some(v) = entry.get("visible") {
            l.visible = v.as_bool().ok_or_else(|| bad("visible must be boolean"))?;
        }
        if let Some(v) = entry.get("fill") {
            let LayerContent::Shape(sh) = &mut l.content else {
                return Err(bad("master fill needs shape layer"));
            };
            sh.fill = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
        }
        if let Some(v) = entry.get("path") {
            let LayerContent::Shape(sh) = &mut l.content else {
                return Err(bad("master path needs shape layer"));
            };
            sh.path = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
            sh.live = None;
            sh.psd_raw = None;
            l.vector.live_shape = None;
        }
        if let Some(v) = entry.get("text") {
            let LayerContent::Text(t) = &mut l.content else {
                return Err(bad("master text needs type layer"));
            };
            let text = v.as_str().filter(|t| t.len() <= 1_000_000).ok_or_else(|| bad("master text exceeds budget"))?;
            let len = t.text.len();
            crate::type_cmds::replace_text(t, 0, len, text);
        }
    }
    update_master(s, c, symbol(p)?, draft.layers)?;
    Ok(json!({"symbol":id.0}))
}
fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    let id = symbol(p)?;
    let name = name(p)?;
    doc.symbols.iter_mut().find(|d| d.id == id).ok_or_else(|| bad("missing symbol"))?.name = name;
    publish(s, c, "Rename Symbol", doc, None)?;
    Ok(json!({"symbol":id.0}))
}
pub fn detach_prepared(doc: &Document, layer: &Layer) -> Result<Layer> {
    if !matches!(layer.content, LayerContent::Symbol(_)) {
        return Err(bad("target is not a symbol instance"));
    }
    let ex = expansion::expand(doc, std::slice::from_ref(layer)).map_err(|e| bad(e.to_string()))?;
    prepare_expansion(doc, ex)?.pop().ok_or_else(|| bad("empty expansion"))
}
fn detach(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let ids = ids(p)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    targets(&doc, &ids, true)?;
    let replacements = ids.iter().map(|id| detach_prepared(&doc, doc.layer(*id).ok_or(EngineError::NoLayer(*id))?)).collect::<Result<Vec<_>>>()?;
    for layer in replacements {
        let id = layer.id;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
    }
    publish(s, c, "Detach Symbols", doc, None)?;
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let id = symbol(p)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    if !doc.symbols.iter().any(|d| d.id == id) {
        return Err(bad("missing symbol"));
    }
    fn detach_all(snapshot: &Document, layers: &mut [Layer], id: SymbolId) -> Result<()> {
        for l in layers {
            match &mut l.content {
                LayerContent::Symbol(s) if s.symbol == id => *l = detach_prepared(snapshot, l)?,
                LayerContent::Group(g) => detach_all(snapshot, &mut g.children, id)?,
                _ => {}
            }
        }
        Ok(())
    }
    let snapshot = doc.clone();
    let root_ids: Vec<_> =
        doc.walk().into_iter().filter(|(_, _, l)| matches!(&l.content,LayerContent::Symbol(s) if s.symbol==id)).map(|(_, _, l)| l.id).collect();
    if !root_ids.is_empty() {
        targets(&doc, &root_ids, true)?;
    }
    detach_all(&snapshot, &mut doc.layers, id)?;
    for def in &mut doc.symbols {
        if def.id != id {
            detach_all(&snapshot, &mut def.layers, id)?;
        }
    }
    doc.symbols.retain(|d| d.id != id);
    // Other templates changed by detachment retain pixel-equivalent caches. Their geometry
    // generation changes with this one revision; no unnecessary re-render of ordinary layers.
    publish(s, c, "Delete Symbol", doc, None)?;
    Ok(json!({"symbol":id.0}))
}
fn set_instances(s: &mut Session, p: &Value) -> Result<Value> {
    let c = capture(p)?;
    c.check(s)?;
    let ids = ids(p)?;
    let a = transform(p)?;
    let mut doc = (*s.active().ok_or(EngineError::NoDocument)?.doc).clone();
    valid(&doc)?;
    targets(&doc, &ids, true)?;
    let replacements = ids
        .iter()
        .map(|id| {
            let mut l = doc.layer(*id).ok_or(EngineError::NoLayer(*id))?.clone();
            let LayerContent::Symbol(instance) = &mut l.content else {
                return Err(bad("target is not a symbol instance"));
            };
            instance.transform = a;
            instance.cache = Some(render_instance(&doc, instance)?);
            Ok(l)
        })
        .collect::<Result<Vec<_>>>()?;
    for l in replacements {
        let id = l.id;
        *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = l;
    }
    publish(s, c, "Transform Symbols", doc, None)?;
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}
fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    valid(doc)?;
    Ok(
        json!({"symbols":doc.symbols.iter().map(|d|json!({"symbol":d.id.0,"name":d.name,"layers":d.layers.iter().map(|l|l.id.0).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
    )
}

/// Copies the transitive master closure for ordinary duplicated layers in a scratch document.
/// Every imported master gets a new destination SymbolId and fresh template LayerIds; identical
/// numeric IDs in unrelated documents never accidentally link the copies to destination art.
pub fn transfer_definitions(source: &Document, destination: &Document, copies: &mut Document) -> Result<()> {
    valid(source)?;
    valid(destination)?;
    fn refs(layers: &[Layer], ids: &mut BTreeSet<u64>) {
        for l in layers {
            match &l.content {
                LayerContent::Symbol(s) => {
                    ids.insert(s.symbol.0);
                }
                LayerContent::Group(g) => refs(&g.children, ids),
                _ => {}
            }
        }
    }
    let mut needed = BTreeSet::new();
    refs(&copies.layers, &mut needed);
    loop {
        let before = needed.len();
        for def in &source.symbols {
            if needed.contains(&def.id.0) {
                refs(&def.layers, &mut needed);
            }
        }
        if before == needed.len() {
            break;
        }
    }
    let mut next = destination.symbols.iter().map(|d| d.id.0).max().unwrap_or(0);
    let mut ids = std::collections::BTreeMap::new();
    for id in &needed {
        next = next.checked_add(1).ok_or_else(|| bad("symbol ID space exhausted"))?;
        ids.insert(*id, SymbolId(next));
    }
    fn remap(layers: &mut [Layer], ids: &std::collections::BTreeMap<u64, SymbolId>) -> Result<()> {
        for l in layers {
            match &mut l.content {
                LayerContent::Symbol(s) => s.symbol = *ids.get(&s.symbol.0).ok_or_else(|| bad("missing copied symbol master"))?,
                LayerContent::Group(g) => remap(&mut g.children, ids)?,
                _ => {}
            }
        }
        Ok(())
    }
    let mut layers = copies.layers.clone();
    remap(&mut layers, &ids)?;
    let masters = source
        .symbols
        .iter()
        .filter(|d| needed.contains(&d.id.0))
        .map(|d| {
            let mut layers = d.layers.iter().map(Layer::duplicate).collect::<Vec<_>>();
            remap(&mut layers, &ids)?;
            Ok(SymbolDef { id: ids[&d.id.0], name: d.name.clone(), layers })
        })
        .collect::<Result<Vec<_>>>()?;
    validation::validate(&masters, &layers).map_err(|e| bad(e.to_string()))?;
    copies.layers = layers;
    copies.symbols = masters;
    Ok(())
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    [
        ("symbol.create", "Create Symbol", true, create as fn(&mut Session, &Value) -> Result<Value>),
        ("symbol.place", "Place Symbol", true, place),
        ("symbol.update", "Edit Symbol Master", true, update),
        ("symbol.editMaster", "Edit Symbol Master", true, edit_master),
        ("symbol.rename", "Rename Symbol", true, rename),
        ("symbol.delete", "Delete Symbol", true, delete),
        ("symbol.detach", "Detach Symbols", true, detach),
        ("symbol.setInstances", "Transform Symbols", true, set_instances),
        ("symbol.list", "Symbols", false, list),
    ]
    .into_iter()
    .map(|(id, label, journal, run)| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params: "{document:id,revision:n,symbol?:id,layers?:[id],name?:string,transform?:Affine,above?:id}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal,
    })
    .collect()
}
#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod tests;
