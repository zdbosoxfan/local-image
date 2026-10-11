//! Package 24 dialog/canvas adapter. State is transient; routing is owned by package 00.
use egui::{Context, Event, Key, PointerButton, Pos2, Rect, Ui};
use photocraft_doc::{
    Document, LayerId,
    vector_design::exchange::{ImportedScene, VectorWarning},
};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::affinity::{self, Place},
};
use std::sync::Arc;

#[derive(Default)]
pub struct State {
    pub source: Vec<u8>,
    pub name: String,
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub warnings: Vec<VectorWarning>,
    pub error: Option<String>,
    request: Option<Place>,
    scene: Option<ImportedScene>,
    drag: Option<(Pos2, [i32; 2])>,
    context: Option<(crate::state::Tool, crate::Module)>,
    warning_scope: Option<(u64, Vec<LayerId>)>,
}
impl State {
    pub fn cancel(&mut self) {
        self.preview = None;
        self.request = None;
        self.scene = None;
        self.drag = None;
        self.context = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, session: &Session, enabled: bool) {
        if !enabled || self.request.as_ref().is_some_and(|r| affinity::check(session, r).is_err()) {
            self.cancel();
        }
    }
    /// The app calls this before drawing/input, including document/module switches.
    /// The host passes enabled=false when a persona or modal transition interrupts placement.
    pub fn synchronize_context(&mut self, session: &Session, tool: crate::state::Tool, module: crate::Module, enabled: bool) {
        self.synchronize(session, enabled);
        if self.pending() {
            let current = (tool, module);
            if self.context.is_some_and(|previous| previous != current) {
                self.cancel();
            } else {
                self.context = Some(current);
            }
        }
    }
    /// The host must use its bounded file reader before calling this method.
    pub fn set_source(&mut self, bytes: &[u8], name: &str) -> Result<()> {
        if bytes.len() > photocraft_doc::vector_design::exchange::ImportOptions::default().limits.bytes || name.len() > 4096 {
            return Err(EngineError::Other("Affinity source exceeds bounds".into()));
        }
        self.cancel();
        self.source = bytes.to_vec();
        self.name = name.into();
        self.warnings.clear();
        self.warning_scope = None;
        self.error = None;
        Ok(())
    }
    pub fn stage(&mut self, session: &Session) -> Result<()> {
        if self.source.len() > photocraft_doc::vector_design::exchange::ImportOptions::default().limits.bytes {
            return Err(EngineError::Other("Affinity input exceeds byte limit".into()));
        }
        self.cancel();
        self.warning_scope = None;
        let mut request = affinity::capture(session, self.source.clone())?;
        if !self.name.is_empty() {
            request.name = self.name.clone();
        }
        let scene = affinity::decode(session, &request)?;
        let (preview, _) = affinity::preview(session, &request, &scene)?;
        self.warnings = scene.warnings.clone();
        self.request = Some(request);
        self.scene = Some(scene);
        self.preview = Some(Arc::new(preview));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn place(&mut self, session: &mut Session) -> Result<()> {
        if self.request.is_none() {
            self.stage(session)?;
        }
        let request = self.request.as_ref().ok_or_else(|| EngineError::Other("missing Affinity placement".into()))?;
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing Affinity scene".into()))?;
        affinity::commit(session, request, scene)?;
        let mut stack: Vec<_> = scene.layers.iter().collect();
        let mut ids = Vec::new();
        while let Some(layer) = stack.pop() {
            ids.push(layer.id);
            if let photocraft_doc::LayerContent::Group(group) = &layer.content {
                stack.extend(&group.children);
            }
        }
        self.warning_scope = Some((request.document, ids));
        self.cancel();
        Ok(())
    }
    pub fn open(&mut self, session: &mut Session) -> Result<()> {
        self.warnings = affinity::open_bytes(session, &self.source, 72., if self.name.is_empty() { "Affinity" } else { &self.name })?;
        if let Some(active) = session.active() {
            self.warning_scope = Some((active.doc.id.0, active.doc.walk().into_iter().map(|(_, _, layer)| layer.id).collect()));
        }
        self.cancel();
        Ok(())
    }
    pub fn pending(&self) -> bool {
        self.request.is_some()
    }
    fn motion(&mut self, session: &Session, at: Pos2) -> Result<()> {
        let Some((origin, base)) = self.drag else { return Ok(()) };
        let Some(request) = &mut self.request else { return Ok(()) };
        let delta = [at.x - origin.x, at.y - origin.y];
        if delta.iter().any(|d| !d.is_finite() || d.abs() > 1_000_000.0) {
            return Err(EngineError::Other("invalid Affinity placement pointer".into()));
        }
        request.offset = [base[0].saturating_add(delta[0].round() as i32), base[1].saturating_add(delta[1].round() as i32)];
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing Affinity preview scene".into()))?;
        self.preview = Some(Arc::new(affinity::preview(session, request, scene)?.0));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Controls are hosted by the existing bounded binary File/Open and Place routes.
/// Bytes arrive from the host's bounded Open/Place file reader; never resolve linked resources.
pub fn controls(ui: &mut Ui, state: &mut State, session: &mut Session) {
    ui.label(tl!("Affinity document"));
    if ui.add(egui::TextEdit::singleline(&mut state.name).hint_text(tl!("Document name")).char_limit(4096)).changed() {
        state.cancel();
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.add_enabled(!state.source.is_empty(), egui::Button::new(tl!("Open Affinity"))).clicked() {
            action = Some(0);
        }
        if ui.add_enabled(session.active().is_some() && !state.source.is_empty(), egui::Button::new(tl!("Preview Affinity"))).clicked() {
            action = Some(1);
        }
        if ui.add_enabled(session.active().is_some() && !state.source.is_empty(), egui::Button::new(tl!("Place Affinity"))).clicked() {
            action = Some(2);
        }
        if ui.add_enabled(state.pending(), egui::Button::new(tl!("Cancel"))).clicked() {
            state.cancel();
        }
    });
    if let Some(action) = action {
        let result = match action {
            0 => state.open(session),
            1 => state.stage(session),
            _ => state.place(session),
        };
        state.error = result.err().map(|e| e.to_string());
    }
    if let Some(e) = &state.error {
        ui.colored_label(egui::Color32::RED, e);
    }
    warning_controls(ui, state, session);
}
/// Clicking a warning selects its affected layer in the ordinary Layers panel.
/// Warnings about definitions or omitted objects have no fabricated layer target.
pub fn warning_controls(ui: &mut Ui, state: &State, session: &mut Session) {
    for warning in &state.warnings {
        let target = warning_target(state, session, warning);
        let label = warning.object.as_ref().map_or_else(|| warning.feature.clone(), |o| format!("{}: {o}", warning.feature));
        if ui.add_enabled(target.is_some(), egui::Button::new(label)).on_hover_text(&warning.message).clicked()
            && let Some(id) = target
            && let Some(active) = session.active_mut()
        {
            active.active_layer = Some(id);
            active.selected_layers = vec![id];
        }
    }
}
fn warning_target(state: &State, session: &Session, warning: &VectorWarning) -> Option<LayerId> {
    let object = warning.object.as_ref()?;
    let (document, ids) = state.warning_scope.as_ref()?;
    let active = session.active()?;
    if active.doc.id.0 != *document {
        return None;
    }
    active.doc.walk().into_iter().filter(|(_, _, l)| &l.name == object && ids.contains(&l.id)).min_by_key(|(_, _, l)| l.is_group()).map(|(_, _, l)| l.id)
}

/// Offscreen/full app input entrypoint. to_doc supplies the established canvas affine.
/// Every pointer frame remains detached; a captured release commits once.
pub fn input(state: &mut State, session: &mut Session, ctx: &Context, canvas: Rect, to_doc: impl Fn(Pos2) -> Pos2, enabled: bool) -> Result<()> {
    state.synchronize(session, enabled);
    if !state.pending() {
        return Ok(());
    }
    let (events, focused) = ctx.input(|i| (i.events.clone(), i.focused));
    if !focused {
        state.cancel();
        return Ok(());
    }
    for event in events {
        match event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if canvas.contains(pos) => {
                if let Some(request) = &state.request {
                    state.drag = Some((to_doc(pos), request.offset));
                }
            }
            Event::PointerMoved(pos) if state.drag.is_some() => {
                state.motion(session, to_doc(pos)).inspect_err(|error| {
                    state.cancel();
                    state.error = Some(error.to_string());
                })?;
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } if state.drag.is_some() => {
                state.motion(session, to_doc(pos)).inspect_err(|error| {
                    state.cancel();
                    state.error = Some(error.to_string());
                })?;
                state.place(session).inspect_err(|error| {
                    state.cancel();
                    state.error = Some(error.to_string());
                })?;
            }
            Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) | Event::PointerGone => state.cancel(),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "affinity/tests.rs"]
mod tests;
