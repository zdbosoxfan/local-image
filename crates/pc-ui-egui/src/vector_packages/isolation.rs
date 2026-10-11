//! Isolation interaction and production hook fragments. Session view state is held outside
//! the document. Original GPL-3.0-or-later code, based on the official isolation workflows.
use crate::view_modes;
use egui::{Context, Key, PointerButton, Response, Ui};
use photocraft_doc::{DocId, Document, LayerId};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::isolation::{self as engine, Capture, Target},
};

#[derive(Clone, Debug, Default)]
pub struct State {
    pub scope: engine::State,
    capture: Option<PointerCapture>,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug)]
struct PointerCapture {
    document: Capture,
    target: Option<LayerId>,
    response: egui::Id,
}
impl State {
    pub fn cancel(&mut self) {
        self.capture = None;
    }
    pub fn synchronize(&mut self, session: &Session, vector_persona: bool) -> bool {
        if let Some(state) = engine::authoritative(session) {
            self.scope = state;
        }
        let changed = self.scope.synchronize(session, vector_persona);
        let stale = self.capture.is_some_and(|c| c.document.check(session, &self.scope).is_err());
        if changed || stale || !vector_persona {
            self.cancel();
        }
        changed || stale
    }
    pub fn exit(&mut self) -> bool {
        self.cancel();
        self.scope.exit()
    }
    pub fn enter(&mut self, session: &mut Session, id: LayerId) -> Result<()> {
        let capture = match self.capture.take() {
            Some(capture) if capture.target == Some(id) => capture.document,
            Some(_) => return Err(EngineError::Other("isolation gesture target changed".into())),
            None => Capture::new(session, &self.scope)?,
        };
        self.scope.enter(session, capture, id)?;
        engine::publish_view_state(session, &self.scope);
        Ok(())
    }
}
/// Pointer-down capture belongs to the same response used for double-click/thumbnail
/// release. The caller passes the hit from 32 (wireframe hit in Outline mode).
pub fn canvas(ui: &Ui, response: &Response, session: &mut Session, state: &mut State, hit: Option<LayerId>, enabled: bool) -> Result<bool> {
    if !enabled {
        state.cancel();
        return Ok(false);
    }
    if ui.input(|i| i.pointer.button_pressed(PointerButton::Primary) && i.pointer.interact_pos().is_some_and(|p| response.rect.contains(p))) {
        state.capture = Some(PointerCapture { document: Capture::new(session, &state.scope)?, target: hit, response: response.id });
    }
    if response.double_clicked_by(PointerButton::Primary) {
        let Some(capture) = state.capture.filter(|c| c.response == response.id) else { return Ok(true) };
        if let Err(e) = capture.document.check(session, &state.scope) {
            state.cancel();
            return Err(e);
        }
        if capture.target != hit {
            state.cancel();
            return Err(EngineError::Other("isolation gesture target changed".into()));
        }
        let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
        let outside = hit.is_none_or(|id| state.scope.current().is_some_and(|_| state.scope.scope(doc).is_ok_and(|s| !s.contains(doc, id))));
        if hit.is_some_and(|id| state.scope.current().is_some_and(|t| t.layer() == id)) {
            state.cancel();
            return Ok(true);
        }
        if outside {
            let changed = state.exit();
            engine::publish_view_state(session, &state.scope);
            return Ok(changed);
        }
        if let Some(id) = hit {
            let eligible = doc.layer(id).is_some_and(|layer| {
                layer.is_group()
                    || matches!(layer.content, photocraft_doc::LayerContent::Symbol(_))
                    || matches!(&layer.content,photocraft_doc::LayerContent::Shape(shape) if shape.path.subpaths.len()>1)
            });
            if !eligible {
                state.cancel();
                return Ok(false);
            }
            state.enter(session, id)?;
            return Ok(true);
        }
    }
    if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) && state.capture.is_some_and(|c| c.response == response.id) {
        state.cancel();
    }
    Ok(false)
}
/// Return true BEFORE the Layers panel selects or expands this row.
pub fn thumbnail(ui: &Ui, response: &Response, session: &mut Session, state: &mut State, id: LayerId) -> Result<bool> {
    if ui.input(|i| i.pointer.button_pressed(PointerButton::Primary) && i.pointer.interact_pos().is_some_and(|p| response.rect.contains(p))) {
        state.capture = Some(PointerCapture { document: Capture::new(session, &state.scope)?, target: Some(id), response: response.id });
    }
    if response.clicked() && ui.input(|i| i.modifiers.alt) {
        if state.capture.is_some_and(|c| c.response == response.id) {
            state.enter(session, id)?;
        }
        return Ok(true);
    }
    if ui.input(|i| i.pointer.button_released(PointerButton::Primary)) && state.capture.is_some_and(|c| c.response == response.id) {
        state.cancel();
    }
    Ok(false)
}
pub fn escape(ctx: &Context, state: &mut State) -> bool {
    if state.scope.current().is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        state.exit();
        true
    } else {
        false
    }
}
/// Breadcrumb labels are document object names; fixed labels go through the app translator.
pub fn bar(ui: &mut Ui, session: &mut Session, state: &mut State, text: impl Fn(&str) -> String) -> Result<bool> {
    if state.scope.current().is_none() {
        return Ok(false);
    }
    let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
    let mut depth = None;
    ui.horizontal(|ui| {
        ui.label(text("Isolation Mode"));
        if ui.button(text("Exit Isolation")).clicked() {
            depth = Some(0);
        }
        for (i, t) in state.scope.breadcrumbs().iter().enumerate() {
            ui.label("›");
            if ui.button(doc.layer(t.layer()).map_or("", |l| l.name.as_str())).clicked() {
                depth = Some(i + 1);
            }
        }
    });
    if let Some(depth) = depth {
        state.scope.navigate(depth)?;
        state.cancel();
        state.scope.restrict_selection(session)?;
        engine::publish_view_state(session, &state.scope);
        return Ok(true);
    }
    Ok(false)
}
/// All selection routes (Layers click/range, canvas, Select All, automation) use this query.
/// It changes only session selection fields, so view actions never alter doc revision.
pub fn select(session: &mut Session, state: &State, ids: &[LayerId]) -> Result<()> {
    let st = session.active_mut().ok_or(EngineError::NoDocument)?;
    let scope = state.scope.scope(&st.doc)?;
    if ids.iter().any(|id| !scope.selectable(&st.doc, *id)) {
        return Err(EngineError::Other("selection is outside isolation scope".into()));
    }
    let mut set = std::collections::HashSet::new();
    let ids: Vec<_> = ids.iter().copied().filter(|id| set.insert(*id)).collect();
    st.active_layer = ids.last().copied();
    st.selected_layers = ids;
    st.layer_anchor = st.active_layer;
    Ok(())
}

/// 00 stores this bridge in egui's temporary state and calls frame before any canvas or
/// accelerator dispatch. It is not serialized into prefs/documents.
#[derive(Clone, Debug, Default)]
pub struct Bridge {
    pub isolation: State,
    pub views: view_modes::State,
    owner: Option<(photocraft_engine::toolsets::Persona, crate::Module)>,
    document: Option<DocId>,
    view_generation: Option<u64>,
    scope_generation: Option<u64>,
}
fn key() -> egui::Id {
    egui::Id::new("vector-view-isolation-37")
}
pub fn bridge(ctx: &Context) -> Bridge {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}
pub fn store(ctx: &Context, bridge: Bridge) {
    ctx.data_mut(|d| d.insert_temp(key(), bridge));
}
/// Returns true when 00 must cancel all pending drawing/transform/selection gestures.
/// Persona/document switches are cancellation boundaries even with no active isolation.
pub fn frame(app: &mut crate::PhotocraftApp, ctx: &Context) -> bool {
    let mut b = bridge(ctx);
    let owner = (app.session.prefs().toolbar.persona, app.current_module);
    let document = app.session.active().map(|s| s.doc.id);
    let document_changed = b.document != document;
    let owner_changed = b.owner.is_some_and(|o| o != owner);
    if document_changed || owner_changed {
        engine::exit_session_isolation(&mut app.session);
        b.isolation.exit();
    }
    if let Some(state) = engine::authoritative_view(&app.session) {
        b.views = state;
    }
    let vector = owner.0 == photocraft_engine::toolsets::Persona::Vector && owner.1 == crate::Module::Compositing;
    let changed = b.isolation.synchronize(&app.session, vector) | b.views.synchronize(app.session.active().map(|s| s.doc.as_ref()));
    if owner_changed {
        b.isolation.exit();
    }
    let escaped = app.ui.dialogs.is_empty() && escape(ctx, &mut b.isolation);
    let toggled = view_modes::shortcuts(
        ctx,
        &mut b.views,
        vector
            && app.ui.dialogs.is_empty()
            && app.session.prefs().toolbar.vector_keyboard.preset == photocraft_engine::vector_toolbar_layout::Preset::Illustrator,
    );
    // Menus, row hooks and automation may have changed state after the previous frame.
    // Compare the last dispatched generations, not just the shortcuts consumed here.
    let generation_changed =
        b.view_generation.is_some_and(|g| g != b.views.generation) || b.scope_generation.is_some_and(|g| g != b.isolation.scope.generation());
    b.view_generation = Some(b.views.generation);
    b.scope_generation = Some(b.isolation.scope.generation());
    engine::publish_view_state(&mut app.session, &b.isolation.scope);
    engine::publish_view(&mut app.session, &b.views);
    b.owner = Some(owner);
    b.document = document;
    store(ctx, b);
    owner_changed || document_changed || changed || escaped || toggled || generation_changed
}
/// Scope and view preparation are evaluated from the same source document, including
/// detached gesture previews. Revision/source Arc/dependency generations join view_key in 09.
pub fn prepare(
    ctx: &Context,
    source: &Document,
    transform: photocraft_geom::Affine,
    clip: photocraft_geom::Rect,
    projection: &dyn view_modes::Geometry,
) -> Result<view_modes::Frame> {
    let b = bridge(ctx);
    view_modes::prepare(source, &b.views, b.isolation.scope.scope(source)?, transform, clip, projection)
}
/// New objects use the engine insertion-target API (document/group, symbol master local
/// coordinates, or package 35's compound provider); never silently insert at the document root.
pub fn insertion_target(ctx: &Context, doc: &Document) -> Result<engine::InsertionTarget> {
    bridge(ctx).isolation.scope.insertion_target(doc)
}
/// 32 currently exposes group-only Scope.isolation. This adapter supplies the native group
/// case; compound/symbol queries must use engine::Scope/select_all until 00 extends 32.
pub fn group_scope(ctx: &Context) -> Result<Option<LayerId>> {
    match bridge(ctx).isolation.scope.current() {
        Some(Target::Group { layer }) => Ok(Some(layer)),
        None => Ok(None),
        _ => Err(EngineError::Other("package 32 must use the compound/symbol isolation scope query".into())),
    }
}
#[cfg(test)]
#[path = "isolation/tests.rs"]
mod tests;

/// Store every menu/row/canvas change to the same authoritative session stores installed
/// by 00, before another frame or an automation query reads them.
pub fn store_session(session: &mut Session, ctx: &Context, b: Bridge) {
    engine::publish_view_state(session, &b.isolation.scope);
    engine::publish_view(session, &b.views);
    store(ctx, b);
}
