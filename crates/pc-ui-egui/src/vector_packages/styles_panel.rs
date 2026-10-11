//! Document Graphic Styles panel. Original Local Image UI; all edits pass through the
//! captured engine preparation boundary. Persistence hooks belong to package 00.
use egui::{Response, Ui};
use photocraft_doc::Document;
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::styles::{self as engine, Hooks, Operation, Request, Store, StyleId},
};

#[derive(Clone, Default)]
pub struct State {
    pub selected: Option<StyleId>,
    pub name: String,
    pub preview: Option<Document>,
    pub generation: u64,
    pending: Option<Request>,
    journal: bool,
    blocked: bool,
    document: Option<u64>,
}
impl State {
    pub fn cancel(&mut self) {
        self.blocked |= self.pending.is_some();
        self.pending = None;
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, session: &Session) {
        let document = session.active().map(|s| s.doc.id.0);
        if self.document != document {
            self.cancel();
            self.selected = None;
            self.name.clear();
            self.document = document;
        }
        if self.pending.as_ref().is_some_and(|r| !engine::fresh(session, r) || session.active().is_none_or(|s| s.selected_layers() != r.targets)) {
            self.cancel();
        }
    }
}
fn error(s: &str) -> EngineError {
    EngineError::Other(s.into())
}
fn unique(store: &Store, base: &str) -> String {
    (1..=1025).map(|n| format!("{base} {n}")).find(|n| store.graphic_styles.iter().all(|s| s.name != *n)).unwrap_or_else(|| base.into())
}
fn action(
    state: &mut State,
    session: &mut Session,
    hooks: Option<Hooks>,
    response: Response,
    operation: Option<Operation>,
    render_preview: bool,
) -> Result<()> {
    let pointer_pressed = response.ctx.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } if response.rect.contains(*pos)))
    });
    if (response.is_pointer_button_down_on() || pointer_pressed)
        && !state.blocked
        && state.pending.is_none()
        && let Some(operation) = operation.clone()
    {
        let st = session.active().ok_or(EngineError::NoDocument)?;
        let request = engine::capture(session, st.selected_layers(), operation)?;
        if render_preview {
            let store = hooks.map_or_else(|| Ok(Store::default()), |h| (h.read)(&st.doc))?;
            state.preview = Some(engine::prepare(&st.doc, st.revision, &store, &request, &engine::Projection)?.document);
            state.generation = state.generation.wrapping_add(1);
        }
        state.pending = Some(request);
    }
    if response.clicked() {
        if state.blocked {
            return Ok(());
        }
        if response.clicked_by(egui::PointerButton::Primary) && state.pending.is_none() {
            return Ok(());
        }
        // Keyboard activation is also captured at its event, before any edit.
        let request = if let Some(r) = state.pending.take() {
            r
        } else {
            engine::capture(
                session,
                session.active().ok_or(EngineError::NoDocument)?.selected_layers(),
                operation.ok_or_else(|| error("unavailable style action"))?,
            )?
        };
        state.cancel();
        if state.journal {
            engine::commit(session, &request)?;
        } else {
            engine::commit_with_hooks(session, &request, hooks)?;
        }
    }
    Ok(())
}
pub fn show(ui: &mut Ui, state: &mut State, session: &mut Session) -> Result<()> {
    state.journal = true;
    let result = show_with_hooks(ui, state, session, engine::installed_hooks());
    state.journal = false;
    result
}
/// Explicit provider form keeps the widget testable without touching core fields.
pub fn show_with_hooks(ui: &mut Ui, state: &mut State, session: &mut Session, hooks: Option<Hooks>) -> Result<()> {
    state.synchronize(session);
    if ui.input(|i| i.key_pressed(egui::Key::Escape) || !i.focused) {
        state.cancel();
    }
    let st = session.active().ok_or(EngineError::NoDocument)?;
    let store = hooks.map_or_else(|| Ok(Store::default()), |h| (h.read)(&st.doc))?;
    if state.selected.is_some_and(|id| store.graphic_styles.iter().all(|s| s.id != id)) {
        state.selected = None;
    }
    ui.heading(crate::i18n::t("Graphic Styles"));
    if hooks.is_none() {
        return Err(error("Graphic Styles requires the native document provider"));
    }
    for def in &store.graphic_styles {
        if ui.selectable_label(state.selected == Some(def.id), &def.name).clicked() {
            state.cancel();
            state.selected = Some(def.id);
            state.name = def.name.clone();
        }
    }
    let label = ui.label(crate::i18n::t("Style name"));
    if ui.text_edit_singleline(&mut state.name).labelled_by(label.id).changed() {
        state.cancel();
    }
    let source = session.active().and_then(|st| st.selected_layers().first().copied());
    // Build each operation before borrowing the widget state for its button.
    let new_name = if state.name.trim().is_empty() || store.graphic_styles.iter().any(|s| s.name == state.name) {
        unique(&store, "Graphic Style")
    } else {
        state.name.clone()
    };
    action(
        state,
        session,
        hooks,
        ui.add_enabled(source.is_some(), egui::Button::new(crate::i18n::t("New from selection"))),
        source.map(|source| Operation::New { source, name: new_name }),
        false,
    )?;
    let selected = state.selected;
    let add = ui.input(|i| i.modifiers.alt);
    action(
        state,
        session,
        hooks,
        ui.add_enabled(selected.is_some() && source.is_some(), egui::Button::new(crate::i18n::t("Apply style"))),
        selected.map(|style| Operation::Apply { style, add }),
        true,
    )?;
    action(
        state,
        session,
        hooks,
        ui.add_enabled(selected.is_some() && source.is_some(), egui::Button::new(crate::i18n::t("Redefine style"))),
        selected.zip(source).map(|(style, source)| Operation::Redefine { style, source }),
        false,
    )?;
    action(
        state,
        session,
        hooks,
        ui.add_enabled(source.is_some(), egui::Button::new(crate::i18n::t("Break Link"))),
        source.map(|_| Operation::BreakLink),
        false,
    )?;
    let name = state.name.clone();
    action(
        state,
        session,
        hooks,
        ui.add_enabled(selected.is_some(), egui::Button::new(crate::i18n::t("Rename style"))),
        selected.map(|style| Operation::Rename { style, name }),
        false,
    )?;
    let name = unique(&store, "Graphic Style Copy");
    action(
        state,
        session,
        hooks,
        ui.add_enabled(selected.is_some(), egui::Button::new(crate::i18n::t("Duplicate style"))),
        selected.map(|style| Operation::Duplicate { style, name }),
        false,
    )?;
    action(
        state,
        session,
        hooks,
        ui.add_enabled(selected.is_some(), egui::Button::new(crate::i18n::t("Delete style"))),
        selected.map(|style| Operation::Delete { style }),
        false,
    )?;
    action(state, session, hooks, ui.button(crate::i18n::t("Add starter library")), Some(Operation::InstallStarters), false)?;
    if ui.input(|i| i.pointer.any_released()) {
        state.cancel();
        state.blocked = false;
    }
    Ok(())
}
#[cfg(test)]
#[path = "styles_panel/tests.rs"]
mod tests;

fn app_key() -> egui::Id {
    egui::Id::new("vector-styles-38-panel")
}
pub fn panel(app: &mut crate::PhotocraftApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut state = ctx.data_mut(|d| d.get_temp::<State>(app_key()).unwrap_or_default());
    if let Err(e) = show(ui, &mut state, &mut app.session) {
        state.cancel();
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    ctx.data_mut(|d| d.insert_temp(app_key(), state));
}
pub fn frame(app: &crate::PhotocraftApp, ctx: &egui::Context) {
    let mut state = ctx.data_mut(|d| d.get_temp::<State>(app_key()).unwrap_or_default());
    state.synchronize(&app.session);
    if !app.ui.dialogs.is_empty() || !ctx.input(|i| i.focused) || app.current_module != crate::Module::Compositing {
        state.cancel();
    }
    ctx.data_mut(|d| d.insert_temp(app_key(), state));
}
pub fn display_doc(app: &crate::PhotocraftApp, idx: usize, ctx: &egui::Context) -> Option<(std::sync::Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) || !app.ui.dialogs.is_empty() || app.current_module != crate::Module::Compositing {
        return None;
    }
    let state = ctx.data_mut(|d| d.get_temp::<State>(app_key()))?;
    let pending = state.pending.as_ref()?;
    if !engine::fresh(&app.session, pending) || app.session.active().is_none_or(|s| s.selected_layers() != pending.targets) {
        return None;
    }
    Some((std::sync::Arc::new(state.preview?), state.generation))
}
