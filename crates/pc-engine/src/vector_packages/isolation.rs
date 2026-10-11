//! Isolation scope and insertion targets: original GPL-3.0-or-later implementation.
//! Behavior informed by VectorCraft engine/cmd/object.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092
//! (MIT OR Apache-2.0), Illustrator isolation and Affinity soloing. Unlike that reference,
//! entering/exiting does not increment the document revision or record an edit.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{DocId, Document, Layer, LayerContent, LayerId, SymbolId};
use photocraft_geom::Affine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::OnceLock;

pub const DIM_ALPHA: f32 = 0.25;
fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Group { layer: LayerId },
    Compound { layer: LayerId },
    Symbol { layer: LayerId, master: SymbolId },
}
impl Target {
    pub fn layer(self) -> LayerId {
        match self {
            Self::Group { layer } | Self::Compound { layer } | Self::Symbol { layer, .. } => layer,
        }
    }
}
fn target(doc: &Document, id: LayerId) -> Result<Target> {
    let layer = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let path = doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
    if doc.effective_locks(id).all || doc.effective_locks(id).pixels || (1..=path.len()).any(|n| doc.layer_at(&path[..n]).is_none_or(|l| !l.visible)) {
        return Err(error("isolation target is hidden or locked"));
    }
    match &layer.content {
        LayerContent::Group(_) => Ok(Target::Group { layer: id }),
        LayerContent::Shape(s) if s.path.subpaths.len() > 1 => Ok(Target::Compound { layer: id }),
        LayerContent::Symbol(s) if doc.symbols.iter().any(|d| d.id == s.symbol) => Ok(Target::Symbol { layer: id, master: s.symbol }),
        _ => Err(error("isolate a group, compound path, clip group or symbol instance")),
    }
}
fn inside(doc: &Document, root: LayerId, id: LayerId, include_root: bool) -> bool {
    let (Some(a), Some(b)) = (doc.path_of(root), doc.path_of(id)) else { return false };
    b.starts_with(&a) && (include_root || b.len() > a.len())
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    document: Option<DocId>,
    stack: Vec<Target>,
    generation: u64,
}
impl State {
    pub fn current(&self) -> Option<Target> {
        self.stack.last().copied()
    }
    pub fn breadcrumbs(&self) -> &[Target] {
        &self.stack
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn exit(&mut self) -> bool {
        if self.stack.is_empty() {
            return false;
        }
        self.stack.clear();
        self.generation = self.generation.wrapping_add(1);
        true
    }
    /// Returns true when the caller must cancel pointer capture/previews. This also handles
    /// deletion/undo/hidden/locked containers. Ordinary edits inside a valid scope keep it.
    pub fn synchronize(&mut self, session: &Session, vector_persona: bool) -> bool {
        let document = session.active().map(|s| s.doc.id);
        let changed = self.document != document;
        let invalid = session.active().is_some_and(|s| self.stack.iter().any(|t| target(&s.doc, t.layer()).ok() != Some(*t)));
        let exited = if changed || !vector_persona || invalid { self.exit() } else { false };
        self.document = document;
        changed || exited
    }
    pub fn scope(&self, doc: &Document) -> Result<Scope> {
        if self.document.is_some_and(|id| id != doc.id) {
            return Err(error("isolation document changed"));
        }
        if let Some(t) = self.current()
            && target(doc, t.layer())? != t
        {
            return Err(error("isolation target changed"));
        }
        Ok(Scope { document: doc.id, target: self.current() })
    }
    pub fn enter(&mut self, session: &mut Session, capture: Capture, id: LayerId) -> Result<()> {
        capture.check(session, self)?;
        let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
        let next = target(doc, id)?;
        if let Some(t) = self.current()
            && !inside(doc, t.layer(), id, false)
        {
            return Err(error("nested isolation target is outside the current scope"));
        }
        self.document = Some(doc.id);
        self.stack.push(next);
        self.generation = self.generation.wrapping_add(1);
        self.restrict_selection(session)?;
        Ok(())
    }
    pub fn navigate(&mut self, depth: usize) -> Result<()> {
        if depth > self.stack.len() {
            return Err(error("invalid isolation breadcrumb"));
        }
        if depth != self.stack.len() {
            self.stack.truncate(depth);
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
    pub fn restrict_selection(&self, session: &mut Session) -> Result<()> {
        let st = session.active_mut().ok_or(EngineError::NoDocument)?;
        let scope = self.scope(&st.doc)?;
        let ids: Vec<_> = st.selected_layers().into_iter().filter(|id| scope.selectable(&st.doc, *id)).collect();
        st.active_layer = ids.last().copied();
        st.layer_anchor = st.active_layer;
        st.selected_layers = ids;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub document: DocId,
    pub revision: u64,
    pub generation: u64,
}
impl Capture {
    pub fn new(session: &Session, state: &State) -> Result<Self> {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        Ok(Self { document: st.doc.id, revision: st.revision, generation: state.generation })
    }
    pub fn check(self, session: &Session, state: &State) -> Result<()> {
        if self != Self::new(session, state)? {
            return Err(error("isolation gesture is stale"));
        }
        Ok(())
    }
}
/// Scope is a query, never a document field. Group targets exclude the enclosing group
/// itself. Compound/symbol instances are editable as one document object; their path nodes
/// and master children use the namespace-specific query below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub document: DocId,
    pub target: Option<Target>,
}
impl Scope {
    pub fn contains(&self, doc: &Document, id: LayerId) -> bool {
        self.document == doc.id
            && match self.target {
                None => doc.layer(id).is_some(),
                Some(Target::Group { layer }) => inside(doc, layer, id, false),
                Some(t) => t.layer() == id,
            }
    }
    pub fn selectable(&self, doc: &Document, id: LayerId) -> bool {
        if !self.contains(doc, id) {
            return false;
        }
        let Some(p) = doc.path_of(id) else { return false };
        let locks = doc.effective_locks(id);
        !locks.all && !locks.pixels && (1..=p.len()).all(|n| doc.layer_at(&p[..n]).is_some_and(|l| l.visible))
    }
    pub fn select_all(&self, doc: &Document) -> Vec<LayerId> {
        doc.walk().into_iter().map(|(_, _, l)| l.id).filter(|id| self.selectable(doc, *id)).collect()
    }
    pub fn master_layers<'a>(&self, doc: &'a Document) -> Result<Option<&'a [Layer]>> {
        if doc.id != self.document {
            return Err(error("isolation document changed"));
        }
        match self.target {
            Some(Target::Symbol { master, .. }) => {
                doc.symbols.iter().find(|d| d.id == master).map(|d| Some(d.layers.as_slice())).ok_or_else(|| error("missing isolated symbol master"))
            }
            _ => Ok(None),
        }
    }
    /// Dimming tests ancestor relationships too: a containing group must not dim its
    /// isolated descendants a second time. Apply this factor only to maximal outside roots.
    pub fn dimmed(&self, doc: &Document, id: LayerId) -> bool {
        self.target.is_some_and(|t| !inside(doc, t.layer(), id, true) && !inside(doc, id, t.layer(), true))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsertionTarget {
    pub container: Option<Target>,
    pub from_document: Affine,
}
impl State {
    pub fn insertion_target(&self, doc: &Document) -> Result<InsertionTarget> {
        let scope = self.scope(doc)?;
        let from_document = match scope.target {
            Some(Target::Symbol { layer, .. }) => {
                let Some(Layer { content: LayerContent::Symbol(s), .. }) = doc.layer(layer) else { return Err(error("symbol target changed")) };
                if s.transform.m.iter().any(|v| !v.is_finite()) {
                    return Err(error("invalid symbol transform"));
                }
                s.transform.inverse().ok_or_else(|| error("singular symbol transform"))?
            }
            _ => Affine::IDENTITY,
        };
        Ok(InsertionTarget { container: scope.target, from_document })
    }
}
/// Prepared objects are in insertion-target coordinates, not necessarily document space.
/// Drawing providers prepare bounded caches before this call. Compound objects need 35's
/// source-preserving insertion provider rather than flattening a new object's appearance.
pub type CompoundInsert = fn(&mut Session, Capture, LayerId, Layer) -> Result<LayerId>;
static COMPOUND_INSERT: OnceLock<CompoundInsert> = OnceLock::new();
pub fn install_compound_inserter(f: CompoundInsert) -> std::result::Result<(), CompoundInsert> {
    COMPOUND_INSERT.set(f)
}
pub fn insert_prepared(session: &mut Session, state: &State, capture: Capture, above: Option<LayerId>, layer: Layer) -> Result<LayerId> {
    capture.check(session, state)?;
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let target = state.insertion_target(&st.doc)?.container;
    // Entire new tree checked for finite paths, cache readiness and duplicate IDs before edit.
    fn validate(layers: &[Layer], ids: &mut std::collections::HashSet<LayerId>, depth: usize) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(error("insertion exceeds group depth budget"));
        }
        for l in layers {
            if l.id.0 == 0 || !ids.insert(l.id) {
                return Err(error("duplicate or zero insertion layer ID"));
            }
            if let LayerContent::Shape(s) = &l.content {
                crate::vector_api::validate_path(&s.path)?;
                if s.cache.is_none() {
                    return Err(error("insertion shape requires a prepared cache"));
                }
            }
            if let Some(children) = l.children() {
                validate(children, ids, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut ids: std::collections::HashSet<LayerId> = st.doc.walk().iter().map(|(_, _, l)| l.id).collect();
    // Existing master sources may legitimately have no raster cache; only the new tree
    // needs prepared caches. Reserve their IDs without revalidating unrelated artwork.
    let mut existing: Vec<_> = st.doc.symbols.iter().flat_map(|def| &def.layers).collect();
    while let Some(l) = existing.pop() {
        ids.insert(l.id);
        if let Some(children) = l.children() {
            existing.extend(children);
        }
    }
    validate(std::slice::from_ref(&layer), &mut ids, 0)?;
    let id = layer.id;
    let mut candidate = (*st.doc).clone();
    if let Some(Target::Group { layer: parent }) = target {
        candidate.layer_mut(parent).and_then(Layer::children_mut).ok_or(EngineError::NoLayer(parent))?.push(layer.clone());
        if candidate.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(error("insertion exceeds group depth budget"));
        }
    } else if target.is_none() {
        candidate.insert_above(above, layer.clone());
        if candidate.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(error("insertion exceeds group depth budget"));
        }
    }
    match target {
        Some(Target::Compound { layer: parent }) => {
            return COMPOUND_INSERT.get().ok_or_else(|| error("compound insertion requires package 35"))?(session, capture, parent, layer);
        }
        Some(Target::Symbol { master, .. }) => {
            let mut layers = st.doc.symbols.iter().find(|d| d.id == master).ok_or_else(|| error("missing isolated master"))?.layers.clone();
            if above.is_some() {
                return Err(error("symbol master insertion currently targets its top level"));
            }
            layers.push(layer);
            crate::vector_packages::symbols::update_master(
                session,
                crate::vector_packages::symbols::Capture { document: capture.document, revision: capture.revision },
                master,
                layers,
            )?;
        }
        Some(Target::Group { layer: parent }) => {
            let children = st.doc.layer(parent).and_then(Layer::children).ok_or(EngineError::NoLayer(parent))?;
            let index = match above {
                Some(a) => children.iter().position(|l| l.id == a).ok_or_else(|| error("insertion anchor is outside the isolated container"))? + 1,
                None => children.len(),
            };
            session.edit("Create Object", move |doc, active| {
                let children = doc.layer_mut(parent).and_then(Layer::children_mut).ok_or(EngineError::NoLayer(parent))?;
                children.insert(index, layer);
                *active = Some(id);
                Ok(())
            })?;
        }
        None => {
            if let Some(a) = above {
                if st.doc.layer(a).is_none() {
                    return Err(EngineError::NoLayer(a));
                }
                let locks = st.doc.effective_locks(a);
                if locks.all || locks.pixels {
                    return Err(error("insertion anchor is locked"));
                }
            }
            session.edit("Create Object", move |doc, active| {
                doc.insert_above(above, layer);
                *active = Some(id);
                Ok(())
            })?;
        }
    }
    Ok(id)
}
/// 00 installs authoritative per-session storage; absence fails instead of exposing fake
/// successful commands. All isolation commands are non-journaled view actions.
pub struct Hooks {
    /// Clear every per-document scope on document/persona transitions.
    pub exit_all: fn(&mut Session),
    pub read: fn(&Session) -> State,
    pub write: fn(&mut Session, State),
}
static HOOKS: OnceLock<Hooks> = OnceLock::new();
pub fn install_hooks(hooks: Hooks) -> std::result::Result<(), Hooks> {
    HOOKS.set(hooks)
}
pub fn exit_session_isolation(session: &mut Session) {
    if let Some(h) = HOOKS.get() {
        (h.exit_all)(session);
    }
}
pub fn authoritative(session: &Session) -> Option<State> {
    HOOKS.get().map(|h| (h.read)(session))
}
pub fn publish_view_state(session: &mut Session, state: &State) {
    if let Some(h) = HOOKS.get() {
        (h.write)(session, state.clone());
    }
}
fn command(session: &mut Session, value: &Value, enter: bool) -> Result<Value> {
    let hooks = HOOKS.get().ok_or_else(|| error("isolation storage requires package 00"))?;
    let mut state = (hooks.read)(session);
    if enter {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            capture: Capture,
            layer: LayerId,
        }
        let request: Request = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
        state.enter(session, request.capture, request.layer)?;
    } else {
        state.exit();
    }
    let scope = state.scope(&session.active().ok_or(EngineError::NoDocument)?.doc)?;
    (hooks.write)(session, state);
    Ok(json!(scope))
}
fn enter_command(s: &mut Session, v: &Value) -> Result<Value> {
    command(s, v, true)
}
fn exit_command(s: &mut Session, v: &Value) -> Result<Value> {
    command(s, v, false)
}
fn query_command(s: &mut Session, _: &Value) -> Result<Value> {
    let hooks = HOOKS.get().ok_or_else(|| error("isolation storage requires package 00"))?;
    Ok(json!((hooks.read)(s).scope(&s.active().ok_or(EngineError::NoDocument)?.doc)?))
}
pub fn specs() -> Vec<CommandSpec> {
    [
        ("vector.isolation.enter", enter_command as fn(&mut Session, &Value) -> Result<Value>),
        ("vector.isolation.exit", exit_command),
        ("vector.isolation.scope", query_command),
    ]
    .into_iter()
    .map(|(id, run)| CommandSpec {
        id,
        label: "Isolation Mode",
        menu: &[],
        shortcut: None,
        params: "enter: {capture:{document,revision,generation},layer}; exit/scope: {}",
        enabled: |s| if s.active().is_some() { Ok(()) } else { Err("no document open".into()) },
        run,
        journal: false,
    })
    .collect()
}
#[cfg(test)]
mod tests;

// View/session command data lives here until 00 registers the view_modes command slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Vector,
    Outline,
    Xray,
    PixelPreview,
}
#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub mode: ViewMode,
    pub layer_outlines: std::collections::HashSet<LayerId>,
    pub generation: u64,
    document: Option<DocId>,
}
impl ViewState {
    pub fn document(&self) -> Option<DocId> {
        self.document
    }
    pub fn layer_overridden(&self, doc: &Document, id: LayerId) -> bool {
        doc.path_of(id).is_some_and(|p| (1..=p.len()).any(|n| doc.layer_at(&p[..n]).is_some_and(|l| self.layer_outlines.contains(&l.id))))
    }
    pub fn set_mode(&mut self, mode: ViewMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.generation = self.generation.wrapping_add(1);
        true
    }
    pub fn toggle_outline(&mut self) {
        self.set_mode(if self.mode == ViewMode::Outline { ViewMode::Vector } else { ViewMode::Outline });
    }
    pub fn toggle_pixels(&mut self) {
        self.set_mode(if self.mode == ViewMode::PixelPreview { ViewMode::Vector } else { ViewMode::PixelPreview });
    }
    pub fn synchronize(&mut self, document: Option<&Document>) -> bool {
        let id = document.map(|d| d.id);
        if id != self.document {
            self.document = id;
            self.mode = ViewMode::Vector;
            self.layer_outlines.clear();
            self.generation = self.generation.wrapping_add(1);
            return true;
        }
        let before = self.layer_outlines.len();
        self.layer_outlines.retain(|id| document.is_some_and(|d| d.layer(*id).is_some()));
        if before != self.layer_outlines.len() {
            self.generation = self.generation.wrapping_add(1);
        }
        false
    }
    pub fn toggle_layer(&mut self, doc: &Document, id: LayerId) -> Result<()> {
        if doc.layer(id).is_none() {
            return Err(EngineError::NoLayer(id));
        }
        if self.document.is_some_and(|d| d != doc.id) {
            return Err(error("outline document changed"));
        }
        self.document = Some(doc.id);
        if !self.layer_outlines.remove(&id) {
            self.layer_outlines.insert(id);
        }
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn outlined(&self, doc: &Document, id: LayerId) -> bool {
        self.mode == ViewMode::Outline
            || self.mode == ViewMode::Xray
            || doc.path_of(id).is_some_and(|p| (1..=p.len()).any(|n| doc.layer_at(&p[..n]).is_some_and(|l| self.layer_outlines.contains(&l.id))))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ViewAction {
    Set { mode: ViewMode },
    ToggleOutline,
    TogglePixels,
    LayerOutline { layer: LayerId },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewChange {
    pub document: DocId,
    pub revision: u64,
    pub generation: u64,
    pub action: ViewAction,
}
/// The same non-journaled action boundary is usable by automation and the UI. All captured
/// state is checked before changing any view setting, selection, history or document data.
pub fn change_view(session: &Session, state: &mut ViewState, request: &ViewChange) -> Result<()> {
    let st = session.active().ok_or(EngineError::NoDocument)?;
    if request.document != st.doc.id || request.revision != st.revision || request.generation != state.generation {
        return Err(error("view gesture is stale"));
    }
    if state.document.is_some_and(|id| id != st.doc.id) {
        return Err(error("view document changed"));
    }
    match request.action {
        ViewAction::Set { mode } => {
            state.set_mode(mode);
        }
        ViewAction::ToggleOutline => state.toggle_outline(),
        ViewAction::TogglePixels => state.toggle_pixels(),
        ViewAction::LayerOutline { layer } => state.toggle_layer(&st.doc, layer)?,
    }
    state.document = Some(st.doc.id);
    Ok(())
}
pub struct ViewHooks {
    pub read: fn(&Session) -> ViewState,
    pub write: fn(&mut Session, ViewState),
}
static VIEW_HOOKS: OnceLock<ViewHooks> = OnceLock::new();
pub fn install_view_hooks(hooks: ViewHooks) -> std::result::Result<(), ViewHooks> {
    VIEW_HOOKS.set(hooks)
}
pub fn authoritative_view(session: &Session) -> Option<ViewState> {
    VIEW_HOOKS.get().map(|h| (h.read)(session))
}
pub fn publish_view(session: &mut Session, state: &ViewState) {
    if let Some(h) = VIEW_HOOKS.get() {
        (h.write)(session, state.clone());
    }
}
fn view_command(session: &mut Session, value: &Value) -> Result<Value> {
    let hooks = VIEW_HOOKS.get().ok_or_else(|| error("view storage requires package 00"))?;
    let request: ViewChange = serde_json::from_value(value.clone()).map_err(|e| error(e.to_string()))?;
    let mut state = (hooks.read)(session);
    change_view(session, &mut state, &request)?;
    let response = json!({"mode":state.mode,"generation":state.generation});
    (hooks.write)(session, state);
    Ok(response)
}
/// 00's new vector_packages::view_modes slot re-exports this as specs().
pub fn view_specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "vector.view",
        label: "Vector View",
        menu: &[],
        shortcut: None,
        params: "{document,revision,generation,action:{type:set|toggleOutline|togglePixels|layerOutline,mode?|layer?}}",
        enabled: |s| if s.active().is_some() { Ok(()) } else { Err("no document open".into()) },
        run: view_command,
        journal: false,
    }]
}
