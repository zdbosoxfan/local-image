//! Production hooks for 00's serial canvas/router lane. State is kept separately from
//! source documents. Display projection is shared by GPU, CPU fallback, Navigator and export.
use super::*;
use crate::{PhotocraftApp, canvas::ViewXform, state::Tool};

#[derive(Clone, Debug, Default)]
pub struct Bridge {
    pub state: State,
    owner: Option<(Tool, photocraft_engine::toolsets::Persona, crate::Module)>,
    cache: Option<(u64, u64, u64, Arc<Document>, Arc<Document>)>,
}
fn key() -> egui::Id {
    egui::Id::new("vector-repeat-19")
}
pub fn bridge(ctx: &egui::Context) -> Bridge {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}
pub fn store(ctx: &egui::Context, b: Bridge) {
    ctx.data_mut(|d| d.insert_temp(key(), b));
}
pub fn open(ctx: &egui::Context) {
    let mut b = bridge(ctx);
    b.state.open = true;
    store(ctx, b);
    ctx.request_repaint();
}
pub fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    let owner = (app.ui.tool, app.session.prefs().toolbar.persona, app.current_module);
    let keyboard = crate::shortcuts::canvas_keyboard_available(ctx);
    let cancel = ctx.input(|i| {
        !i.focused
            || (keyboard && i.key_pressed(Key::Escape))
            || i.events.iter().any(|e| matches!(e, egui::Event::PointerGone | egui::Event::WindowFocused(false)))
    });
    if cancel || b.owner.is_some_and(|previous| previous != owner) || !app.ui.dialogs.is_empty() {
        b.state.cancel();
    }
    b.owner = Some(owner);
    b.state.synchronize(&app.session, true);
    if b.state.open {
        let mut open = true;
        egui::Window::new(crate::i18n::t("Repeat")).id(key().with("panel")).open(&mut open).default_pos(egui::pos2(820., 80.)).show(ctx, |ui| {
            panel(ui, &mut app.session, &mut b.state, |s| crate::i18n::t(s).to_string());
        });
        b.state.open = open;
        if !open {
            b.state.cancel();
        }
    }
    if let Some(e) = &b.state.error {
        app.ui.status = e.clone();
        app.ui.status_error = true;
    }
    store(ctx, b);
}
pub fn route(app: &mut PhotocraftApp, ui: &mut Ui, idx: usize, xf: &ViewXform, rect: Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    let enabled =
        app.ui.tool == Tool::PathSelection && app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector && app.ui.dialogs.is_empty();
    let consumed = input(
        ui,
        &mut app.session,
        &mut b.state,
        rect,
        View { origin: xf.to_screen(0., 0.), zoom: f64::from(xf.zoom), flip: xf.flip, rotation: xf.rotation },
        enabled,
    );
    if consumed {
        ctx.request_repaint();
    }
    store(&ctx, b);
    consumed
}
pub fn overlay(app: &PhotocraftApp, ui: &mut Ui, xf: &ViewXform) {
    if app.ui.tool != Tool::PathSelection || app.session.prefs().toolbar.persona != photocraft_engine::toolsets::Persona::Vector || !app.ui.dialogs.is_empty() {
        return;
    }
    paint(ui, &app.session, &bridge(ui.ctx()).state, View { origin: xf.to_screen(0., 0.), zoom: f64::from(xf.zoom), flip: xf.flip, rotation: xf.rotation });
}
/// Wrap canvas::display_doc before planning GPU or CPU output. Use the original source Arc
/// and session revision in addition to preview keys: source edits and undo invalidate caches.
pub fn display(s: &Session, ctx: &egui::Context, source: Arc<Document>, key_value: u64) -> Result<(Arc<Document>, u64)> {
    let mut b = bridge(ctx);
    b.state.synchronize(s, true);
    let active = s.active().is_some_and(|st| st.doc.id == source.id);
    if active && let Some(preview) = &b.state.preview {
        let shown = (preview.clone(), (1 << 45) | b.state.generation);
        store(ctx, b);
        return Ok(shown);
    }
    let revision = s.documents().iter().find(|st| st.doc.id == source.id).map_or(0, |st| st.revision);
    if let Some((id, rev, k, original, projected)) = &b.cache
        && *id == source.id.0
        && *rev == revision
        && *k == key_value
        && Arc::ptr_eq(original, &source)
    {
        let shown = (projected.clone(), key_value);
        store(ctx, b);
        return Ok(shown);
    }
    let projected = if source.walk().iter().any(|(_, _, l)| matches!(l.vector.live, Some(LiveGroup::Repeat(_)))) {
        Arc::new(engine::project_document(&source)?)
    } else {
        source.clone()
    };
    b.cache = Some((source.id.0, revision, key_value, source, projected.clone()));
    store(ctx, b);
    Ok((projected, key_value))
}
