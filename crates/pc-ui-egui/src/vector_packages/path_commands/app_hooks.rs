//! Exact production hook implementations for 00. No fields added to shared app/UI state.
use super::*;
use crate::{PhotocraftApp, direct_select::PathRef};
#[derive(Clone, Default)]
pub struct Bridge {
    pub state: State,
    owner: Option<(crate::state::Tool, photocraft_engine::toolsets::Persona, crate::Module)>,
}
fn key() -> egui::Id {
    egui::Id::new("object-path-36")
}
pub fn bridge(ctx: &egui::Context) -> Bridge {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}
pub fn store(ctx: &egui::Context, b: Bridge) {
    ctx.data_mut(|d| d.insert_temp(key(), b));
}
fn selection(app: &PhotocraftApp) -> (Vec<LayerId>, Vec<Anchors>) {
    let mut ids = app.session.active().map(|st| st.selected_layers()).unwrap_or_default();
    let d = &app.ui.direct_selection;
    if app.ui.tool != crate::state::Tool::DirectSelection || app.session.active().is_none_or(|st| d.document != Some(st.doc.id.0)) {
        return (ids, Vec::new());
    }
    let mut anchors = Vec::new();
    if let Some(PathRef::Layer(id)) = d.target
        && !d.anchors.is_empty()
    {
        anchors.push(Anchors { layer: LayerId(id), points: d.anchors.clone() });
    }
    for a in &d.additional {
        if let PathRef::Layer(id) = a.target
            && !a.anchors.is_empty()
        {
            anchors.push(Anchors { layer: LayerId(id), points: a.anchors.clone() });
        }
    }
    if !anchors.is_empty() {
        // Node selection is independent of the layer-panel selection; include every curve
        // with selected anchors, including 03's additional selections.
        ids = anchors.iter().map(|a| a.layer).collect();
    }
    (ids, anchors)
}
/// Call before normal shortcut dispatch; true means Vector consumed a shortcut.
pub fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    let mut b = bridge(ctx);
    let owner = (app.ui.tool, app.session.prefs().toolbar.persona, app.current_module);
    if b.owner.is_some_and(|previous| previous != owner) {
        b.state.cancel();
    }
    b.owner = Some(owner);
    let vector = owner.1 == photocraft_engine::toolsets::Persona::Vector && owner.2 == crate::Module::Compositing;
    let foreign_owner = !app.ui.dialogs.is_empty()
        || app.camera_raw.is_some()
        || app.develop_filter.prompting()
        || (app.draft_refine.open && ctx.embed_viewports())
        || app.discard.is_some()
        || app.distort.active()
        || app.distort.gallery.is_some()
        || app.ui.text_edit.is_some()
        || app.ui.transform.is_some()
        || app.ui.pen.is_some()
        || app.ui.direct_selection.drag.is_some()
        || app.ui.direct_selection.pending.is_some()
        || app.live_stroke.is_some();
    if !vector || foreign_owner {
        b.state.cancel();
        store(ctx, b);
        return false;
    }
    let (ids, anchors) = selection(app);
    let modal = b.state.is_open();
    // menu_nav remembers closure for two passes; suppress keys without discarding a
    // dialog just opened by a menu click.
    let consumed = !crate::menu_nav::is_open(ctx) && b.state.shortcuts(ctx, &mut app.session, &ids, &anchors);
    b.state.dialog(ctx, &mut app.session, |label| crate::i18n::t(label).to_owned());
    if let Some(error) = &b.state.error {
        app.ui.status = crate::i18n::t(error).to_owned();
        app.ui.status_error = true;
    }
    let handled = consumed || modal || b.state.is_open();
    store(ctx, b);
    handled
}
/// Object menu fragment. Pass the host's translation closure, e.g. i18n::tr(...).to_string().
pub fn menu(app: &mut PhotocraftApp, ui: &mut Ui, tr: impl Fn(&str) -> String) {
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    let (ids, anchors) = selection(app);
    b.state.menu(ui, &mut app.session, &ids, &anchors, tr);
    store(&ctx, b);
}
/// Route the shared Edit/Copy entry here for Vector persona.
pub fn copy(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    let (ids, _) = selection(app);
    b.state.copy(&app.session, &ids);
    store(ctx, b);
}
pub fn paste_menu(app: &mut PhotocraftApp, ui: &mut Ui, tr: impl Fn(&str) -> String) {
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    let (ids, _) = selection(app);
    b.state.paste_menu(ui, &mut app.session, &ids, tr);
    store(&ctx, b);
}
/// Call before canvas::display_doc's other providers, for the active document only.
pub fn display(app: &PhotocraftApp, ctx: &egui::Context, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    bridge(ctx).state.display_document(&app.session)
}
