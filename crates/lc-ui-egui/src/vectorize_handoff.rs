//! Package 29: a transient Library → existing Compositing file-open request.
//! No selection, filters, files, history or Develop settings are changed here.
use egui::{Context, Id, Ui};
use lightcraft_catalog::{PhotoId, Source};

use crate::LightcraftApp;

pub const LABEL: &str = "Vectorize in Compositing…";
pub const WIDGET: &str = "photo:vectorizeInCompositing";

/// Captured from the context photo, which need not be the active/selected photo.
/// The host checks this before dispatch, then uses its existing file-open job route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub photo: PhotoId,
    pub catalog_revision: u64,
    pub path: String,
}

impl Request {
    pub fn check(&self, app: &LightcraftApp) -> Result<(), String> {
        if self.catalog_revision != app.session.catalog.revision
            || !app.session.catalog.photo(self.photo).is_some_and(|p| matches!(&p.source, Source::File { path } if path == &self.path))
        {
            return Err(crate::i18n::tr("The photo changed; try Vectorize again").into());
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Pending(Option<Request>);

fn key() -> Id {
    Id::new("library-vectorize-handoff-29")
}

/// One request, drained by the shell on the shared egui context. Never persisted.
pub fn take_request(ctx: &Context) -> Option<Request> {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<Pending>(key()).0.take())
}

pub fn pending(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<Pending>(key()).is_some_and(|p| p.0.is_some()))
}

pub fn disabled_reason(app: &LightcraftApp, id: PhotoId) -> Option<&'static str> {
    if app.services.open_with.is_none() || app.host_session.is_some() {
        return Some("Compositing isn't available here");
    }
    match app.session.catalog.photo(id).map(|p| &p.source) {
        Some(Source::File { path }) if !path.is_empty() && path.len() <= 32768 && !path.contains('\0') => None,
        _ => Some("Vectorize requires a photo file"),
    }
}

/// Append this to the existing photo context menu. Call before its selection-changing
/// code (see integration/29.md); this action always preserves the pre-menu selection.
pub fn context_action(app: &LightcraftApp, ui: &mut Ui, id: PhotoId) -> bool {
    let reason = disabled_reason(app, id);
    let response = ui.add_enabled(reason.is_none() && !pending(ui.ctx()), egui::Button::new(crate::i18n::tr(LABEL)));
    crate::widgets::register(ui.ctx(), WIDGET, response.rect);
    let response = if let Some(reason) = reason { response.on_disabled_hover_text(crate::i18n::tr(reason)) } else { response };
    if !response.clicked() {
        return false;
    }
    if let Some(photo) = app.session.catalog.photo(id)
        && let Source::File { path } = &photo.source
    {
        let request = Request { photo: id, catalog_revision: app.session.catalog.revision, path: path.clone() };
        ui.ctx().data_mut(|data| data.get_temp_mut_or_default::<Pending>(key()).0 = Some(request));
        ui.ctx().request_repaint();
        ui.close();
        return true;
    }
    false
}

#[cfg(test)]
#[path = "vectorize_handoff/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "vectorize_handoff/integrated_tests.rs"]
mod integrated_tests;
