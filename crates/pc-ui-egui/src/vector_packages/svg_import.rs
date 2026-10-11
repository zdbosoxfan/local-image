//! Package 13 dialog/canvas adapter. State is transient; routing is owned by package 00.
use egui::{Context, Event, Key, PointerButton, Pos2, Rect, Ui};
use photocraft_doc::{
    Document, LayerId,
    vector_design::exchange::{ImportedScene, VectorWarning},
};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::svg_import::{self, Place},
};
use std::sync::Arc;

#[derive(Default)]
pub struct State {
    pub source: String,
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub warnings: Vec<VectorWarning>,
    pub error: Option<String>,
    request: Option<Place>,
    scene: Option<ImportedScene>,
    drag: Option<(Pos2, [i32; 2])>,
    context: Option<(crate::state::Tool, crate::Module)>,
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
        if !enabled || self.request.as_ref().is_some_and(|r| svg_import::check(session, r).is_err()) {
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
    pub fn stage(&mut self, session: &Session) -> Result<()> {
        self.cancel();
        let request = svg_import::capture(session, self.source.clone())?;
        let scene = svg_import::decode(session, &request)?;
        let (preview, _) = svg_import::preview(session, &request, &scene)?;
        self.warnings = scene.warnings.clone();
        self.request = Some(request);
        self.scene = Some(scene);
        self.preview = Some(Arc::new(preview));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn place(&mut self, session: &mut Session, paste: bool) -> Result<()> {
        if self.request.is_none() {
            self.stage(session)?;
        }
        let request = self.request.as_ref().ok_or_else(|| EngineError::Other("missing SVG placement".into()))?;
        let response = session
            .execute(if paste { "file.svg.paste" } else { "file.svg.place" }, serde_json::to_value(request).map_err(|e| EngineError::Other(e.to_string()))?)?;
        self.warnings = serde_json::from_value(response.get("warnings").cloned().unwrap_or_default()).map_err(|e| EngineError::Other(e.to_string()))?;
        self.cancel();
        Ok(())
    }
    pub fn open(&mut self, session: &mut Session) -> Result<()> {
        let response = session.execute("file.svg.import", serde_json::json!({"svg":self.source}))?;
        self.warnings = serde_json::from_value(response.get("warnings").cloned().unwrap_or_default()).map_err(|e| EngineError::Other(e.to_string()))?;
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
            return Err(EngineError::Other("invalid SVG placement pointer".into()));
        }
        request.offset = [base[0].saturating_add(delta[0].round() as i32), base[1].saturating_add(delta[1].round() as i32)];
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing SVG preview scene".into()))?;
        self.preview = Some(Arc::new(svg_import::preview(session, request, scene)?.0));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

/// Controls can be hosted by the existing File/Open and Place dialog routes. Clipboard
/// SVG calls the same State::place(...,true) adapter, after filling source with bounded text.
pub fn controls(ui: &mut Ui, state: &mut State, session: &mut Session) {
    if ui.add(egui::TextEdit::multiline(&mut state.source).hint_text(tl!("SVG source")).char_limit(64 << 20).desired_rows(4)).changed() {
        state.cancel();
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(tl!("Open SVG")).clicked() {
            action = Some(0);
        }
        if ui.add_enabled(session.active().is_some(), egui::Button::new(tl!("Preview SVG"))).clicked() {
            action = Some(1);
        }
        if ui.add_enabled(session.active().is_some(), egui::Button::new(tl!("Place SVG"))).clicked() {
            action = Some(2);
        }
        if ui.add_enabled(session.active().is_some(), egui::Button::new(tl!("Paste SVG"))).clicked() {
            action = Some(3);
        }
        if ui.add_enabled(state.pending(), egui::Button::new(tl!("Cancel"))).clicked() {
            state.cancel();
        }
    });
    if let Some(action) = action {
        let result = match action {
            0 => state.open(session),
            1 => state.stage(session),
            2 => state.place(session, false),
            _ => state.place(session, true),
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
        let target = warning_target(session, warning);
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
fn warning_target(session: &Session, warning: &VectorWarning) -> Option<LayerId> {
    let object = warning.object.as_ref()?;
    session.active()?.doc.walk().into_iter().filter(|(_, _, l)| &l.name == object).min_by_key(|(_, _, l)| l.is_group()).map(|(_, _, l)| l.id)
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
                state.place(session, false).inspect_err(|error| {
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    struct Host {
        state: State,
        session: Session,
        canvas: Rect,
    }
    fn host() -> Host {
        let mut session = Session::new();
        session.execute("file.new", serde_json::json!({"width":128,"height":128,"background":"transparent"})).unwrap();
        Host { state: State::default(), session, canvas: Rect::NOTHING }
    }
    fn harness() -> Harness<'static, Host> {
        Harness::builder().with_size(egui::vec2(700.0, 500.0)).build_ui_state(
            |ui, h| {
                controls(ui, &mut h.state, &mut h.session);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 200.0), egui::Sense::click_and_drag());
                h.canvas = rect;
                input(&mut h.state, &mut h.session, ui.ctx(), rect, |p| p - rect.min.to_vec2(), true).unwrap();
            },
            host(),
        )
    }
    fn source() -> String {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><rect id="badge" data-name="Visible badge" width="32" height="32" fill="url(#g)"/></svg>"##.into()
    }
    #[test]
    fn svg_import_ui_type_place_warning_click_and_undo() {
        let mut h = harness();
        h.get_by_role(egui::accesskit::Role::MultilineTextInput).click();
        h.event(Event::Text(source()));
        h.run_steps(2);
        assert!(h.state().state.source.contains("badge"));
        let before = h.state().session.active().unwrap().doc.layer_count();
        h.get_by_label("Place SVG").click();
        h.run_steps(2);
        assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
        let s = h.state().session.active().unwrap();
        assert_eq!(s.doc.layer_count(), before + 2);
        assert!(s.doc.layer(s.active_layer.unwrap()).unwrap().is_group());
        h.get_by_label("gradient-projection: Visible badge").click();
        h.run_steps(2);
        let s = h.state().session.active().unwrap();
        assert_eq!(s.doc.layer(s.active_layer.unwrap()).unwrap().name, "Visible badge");
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), before);
    }
    #[test]
    fn svg_import_ui_drag_preview_release_and_escape() {
        let mut h = harness();
        h.state_mut().state.source = source();
        h.run_steps(2);
        h.get_by_label("Preview SVG").click();
        h.run_steps(2);
        assert!(h.state().state.pending(), "preview button: {:?}", h.state().state.error);
        let rev = h.state().session.active().unwrap().revision;
        let count = h.state().session.active().unwrap().doc.layer_count();
        let a = h.state().canvas.min + egui::vec2(20.0, 20.0);
        let b = a + egui::vec2(25.0, 15.0);
        h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(Event::PointerMoved(b));
        h.step();
        assert_eq!(h.state().session.active().unwrap().revision, rev);
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), count);
        assert!(h.state().state.preview.is_some());
        h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.step();
        assert_eq!(h.state().session.active().unwrap().revision, rev + 1);
        assert!(!h.state().state.pending());
        h.get_by_label("Preview SVG").click();
        h.run_steps(2);
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.step();
        assert!(!h.state().state.pending());
        assert_eq!(h.state().session.active().unwrap().revision, rev + 1);
    }
    #[test]
    fn svg_import_ui_text_warning_selects_editable_text_with_source_label() {
        let mut h = harness();
        h.get_by_role(egui::accesskit::Role::MultilineTextInput).click();
        h.event(Event::Text(r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><text id="title" data-name="Title" x="2" y="20" font-family="Missing SVG Fixture Font">Hi</text></svg>"#.into()));
        h.run_steps(2);
        let before = h.state().session.active().unwrap().doc.layer_count();
        h.get_by_label("Place SVG").click();
        h.run_steps(2);
        assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
        h.get_by_label("font-substitution: Title").click();
        h.run_steps(2);
        let st = h.state().session.active().unwrap();
        assert!(matches!(&st.doc.layer(st.active_layer.unwrap()).unwrap().content, photocraft_doc::LayerContent::Text(t) if t.text == "Hi"));
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), before);
    }
}
