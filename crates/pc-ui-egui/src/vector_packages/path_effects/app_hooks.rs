//! Exact UI routing hooks for 00; no shared app/state fields or menu edits here.
use super::*;
use crate::PhotocraftApp;
#[derive(Clone, Default)]
pub struct Bridge {
    pub state: State,
    owner: Option<(crate::state::Tool, photocraft_engine::toolsets::Persona, crate::Module)>,
}
fn key() -> egui::Id {
    egui::Id::new("path-effects-45")
}
pub fn bridge(ctx: &Context) -> Bridge {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}
pub fn store(ctx: &Context, b: Bridge) {
    ctx.data_mut(|d| d.insert_temp(key(), b));
}
pub fn frame(app: &mut PhotocraftApp, ctx: &Context) -> bool {
    let mut b = bridge(ctx);
    let owner = (app.ui.tool, app.session.prefs().toolbar.persona, app.current_module);
    if b.owner.is_some_and(|o| o != owner) {
        b.state.cancel();
    }
    b.owner = Some(owner);
    let enabled = owner.1 == photocraft_engine::toolsets::Persona::Vector
        && owner.2 == crate::Module::Compositing
        && app.ui.dialogs.is_empty()
        && app.camera_raw.is_none()
        && !app.develop_filter.prompting()
        && !(app.draft_refine.open && ctx.embed_viewports())
        && app.discard.is_none()
        && !app.distort.active()
        && app.distort.gallery.is_none()
        && app.ui.text_edit.is_none()
        && app.ui.transform.is_none()
        && app.ui.pen.is_none()
        && app.ui.direct_selection.drag.is_none()
        && app.ui.direct_selection.pending.is_none()
        && app.live_stroke.is_none();
    b.state.synchronize(&app.session, enabled);
    if let Ok(a) = engine::access() {
        b.state.dialog(ctx, &mut app.session, a, command, |s| crate::i18n::t(s).to_owned());
    }
    if let Some(error) = &b.state.error {
        app.ui.status = crate::i18n::t(error).to_owned();
        app.ui.status_error = true;
    }
    let modal = b.state.is_open();
    store(ctx, b);
    modal
}
pub fn menu(app: &mut PhotocraftApp, ui: &mut Ui, target: EffectTarget) {
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    if let Ok(a) = engine::access() {
        let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
        b.state.menu(ui, &app.session, &ids, target, a, |s| crate::i18n::t(s).to_owned());
    }
    store(&ctx, b);
}
pub fn panel(app: &mut PhotocraftApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    if let Ok(a) = engine::access() {
        let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
        b.state.panel(ui, &mut app.session, &ids, a, command, |s| crate::i18n::t(s).to_owned());
    }
    store(&ctx, b);
}
pub fn display(app: &PhotocraftApp, ctx: &Context, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    bridge(ctx).state.display_document(&app.session)
}
