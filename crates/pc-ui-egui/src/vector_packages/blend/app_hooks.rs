//! Full-app hooks, routed through the normal context bar,
//! canvas event/capture/overlay and display_doc wrapper. State is passed explicitly so this
//! module does not need fields in UiState or the canvas/router.
use super::{State, engine, overlay, panel};
use crate::{
    PhotocraftApp,
    canvas::{ToolEvent, ViewXform},
    state::Tool,
};
use photocraft_doc::Document;
use photocraft_engine::{Result, Session};
use photocraft_geom::Point;
use std::sync::Arc;
pub fn synchronize(state: &mut State, app: &PhotocraftApp, ctx: &egui::Context) {
    let current = app.ui.tool.label();
    if state.host_tool.is_some_and(|previous| previous != current) {
        state.cancel();
        state.edit_spine = false;
    }
    state.host_tool = Some(current);
    let enabled = app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector;
    state.synchronize(&app.session, enabled);
    if crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.cancel();
    }
    if !ctx.input(|i| i.focused) {
        state.drag = None;
    }
}
pub fn event(state: &mut State, s: &mut Session, event: ToolEvent, mods: egui::Modifiers, zoom: f64) -> Result<bool> {
    if !state.edit_spine {
        return Ok(false);
    }
    let handled = match event {
        // egui handles and hit targets use logical points; zoom maps these to document pixels.
        ToolEvent::Down { x, y, .. } => state.down(s, Point::new(x, y), 6. / zoom.max(0.001))?,
        ToolEvent::Move { x, y, .. } => {
            let handled = state.drag.is_some();
            state.motion(s, Point::new(x, y), mods)?;
            handled
        }
        ToolEvent::Up { x, y } => {
            let handled = state.drag.is_some();
            state.up(s, Point::new(x, y), mods)?;
            handled
        }
    };
    Ok(handled)
}
pub fn project_display(state: &mut State, s: &Session, idx: usize, source: Arc<Document>, key: u64) -> Result<(Arc<Document>, u64)> {
    if s.active_index() == Some(idx)
        && state.fresh(s)
        && let Some(preview) = &state.preview
    {
        return Ok((preview.clone(), (1 << 47) | state.generation));
    }
    Ok((state.project_display(source)?, key))
}
pub fn paint(state: &State, app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    overlay(state, &app.session, painter, |p| xf.to_screen(p.x as f32, p.y as f32));
}
pub fn context(state: &mut State, ui: &mut egui::Ui) {
    if ui.button(li_i18n::t("Blend Options")).clicked() {
        state.open = true;
    }
}
pub fn window(state: &mut State, app: &mut PhotocraftApp, ctx: &egui::Context) -> Result<()> {
    if !state.open {
        return Ok(());
    }
    let mut open = state.open;
    let mut result = Ok(());
    egui::Window::new(li_i18n::t("Blend Options")).id(egui::Id::new("blend18 options")).default_pos(egui::pos2(860., 70.)).open(&mut open).show(ctx, |ui| {
        result = panel(ui, state, &mut app.session, |s| li_i18n::t(s).to_string());
    });
    state.open = open;
    if !open {
        state.cancel();
    }
    if state.edit_spine {
        app.ui.tool = Tool::PathSelection;
        state.host_tool = Some(app.ui.tool.label());
    }
    if result.is_err() {
        state.cancel();
    }
    result
}
/// Shared bounds-limited evaluator for the appearance renderer, scaled display and exports; register no second blend renderer.
pub fn project(source: &Document) -> Result<Document> {
    engine::evaluator::project_document(source).map_err(|e| photocraft_engine::EngineError::Other(e.to_string()))
}
