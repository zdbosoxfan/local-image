//! Captured PDF export dialog. 00 supplies app routing, translated English-source
//! catalogue lookup and the file-save service. All export actions use one engine command.
use egui::Ui;
use photocraft_doc::LayerId;
use photocraft_doc::vector_design::exchange::ExportedVector;
use photocraft_engine::vector_packages::pdf_export::{Request, Scope, preflight};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_pdf::export::ExportOptions;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub options: ExportOptions,
    pub request: Option<Request>,
    selection: Vec<LayerId>,
    artboards: Vec<(LayerId, String)>,
    artboard: Option<LayerId>,
    pub error: Option<String>,
}
impl State {
    pub fn open(&mut self, s: &Session) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        self.selection = st.selected_layers();
        self.artboards = st.doc.artboards().into_iter().map(|(id, name, _)| (id, name.to_owned())).collect();
        self.artboard = self.artboards.first().map(|(id, _)| *id);
        self.request = Some(Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Document, options: self.options.clone() });
        self.error = None;
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.request = None;
        self.error = None;
    }
    pub fn synchronize(&mut self, s: &Session) {
        if self.request.as_ref().is_some_and(|r| s.active().is_none_or(|st| st.doc.id.0 != r.document || st.revision != r.revision)) {
            self.cancel();
        }
    }
    /// File-save services must repeat this guard after asynchronous picker completion.
    pub fn export(&mut self, s: &mut Session) -> Result<(Request, ExportedVector)> {
        let mut r = self.request.clone().ok_or_else(|| EngineError::Other("PDF export dialog is closed".into()))?;
        r.options = self.options.clone();
        preflight(s, &r)?;
        let params = serde_json::to_value(&r).map_err(|e| EngineError::Other(e.to_string()))?;
        let output = s.execute("file.pdf.export", params)?;
        let bytes = serde_json::from_value(output["bytes"].clone()).map_err(|e| EngineError::Other(e.to_string()))?;
        let warnings = serde_json::from_value(output["warnings"].clone()).map_err(|e| EngineError::Other(e.to_string()))?;
        Ok((r, ExportedVector { bytes, warnings }))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Export,
    Cancel,
}
/// Draw within the app's existing modal host; labels pass through its catalogue lookup.
/// Controls operate on captured selection/artboards, never a fresh live selection.
pub fn controls(ui: &mut Ui, state: &mut State, text: impl Fn(&str) -> String) -> Option<Action> {
    let Some(request) = &mut state.request else { return None };
    ui.heading(text("Export PDF"));
    let mut kind = match request.scope {
        Scope::Document => 0,
        Scope::Selection { .. } => 1,
        Scope::Artboard { .. } => 2,
    };
    ui.radio_value(&mut kind, 0, text("Document"));
    ui.add_enabled_ui(!state.selection.is_empty(), |ui| {
        ui.radio_value(&mut kind, 1, text("Selection"));
    });
    ui.add_enabled_ui(!state.artboards.is_empty(), |ui| {
        ui.radio_value(&mut kind, 2, text("Artboard"));
    });
    if kind == 2 {
        for (id, name) in &state.artboards {
            ui.selectable_value(&mut state.artboard, Some(*id), name);
        }
    }
    request.scope = match kind {
        1 => Scope::Selection { layers: state.selection.clone() },
        2 => Scope::Artboard { layer: state.artboard.unwrap_or(LayerId(0)) },
        _ => Scope::Document,
    };
    ui.checkbox(&mut state.options.text_as_curves, text("Text as curves"));
    ui.checkbox(&mut state.options.compress, text("Compress PDF"));
    if kind == 0 && !state.artboards.is_empty() {
        ui.label(text("Artboard pages (empty exports document)"));
        for (id, name) in &state.artboards {
            let mut selected = state.options.artboards.contains(id);
            if ui.checkbox(&mut selected, name).changed() {
                if selected {
                    state.options.artboards.push(*id);
                } else {
                    state.options.artboards.retain(|v| v != id);
                }
            }
        }
    } else {
        state.options.artboards.clear();
    }
    if let Some(error) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(text("Export")).clicked() {
            action = Some(Action::Export);
        }
        if ui.button(text("Cancel")).clicked() {
            action = Some(Action::Cancel);
        }
    });
    action
}
#[cfg(test)]
#[path = "../../../pc-pdf/tests/export/ui.rs"]
mod tests;

/// Invoked by the app modal action. Prepare bytes before the picker, then recheck the
/// captured document/targets immediately before the existing write service publishes.
/// Cancellation performs no write. Return one aggregated warning list for the app notice.
pub fn save(state: &mut State, s: &mut Session, services: &mut crate::Services) -> Result<Option<Vec<photocraft_doc::vector_design::exchange::VectorWarning>>> {
    let (request, output) = state.export(s)?;
    let path = services.pick_save.as_mut().and_then(|pick| pick("drawing.pdf"));
    let Some(path) = path else {
        state.cancel();
        return Ok(None);
    };
    preflight(s, &request)?;
    let write = services.write.as_mut().ok_or_else(|| EngineError::Other("file write service unavailable".into()))?;
    write(&path, &output.bytes).map_err(EngineError::Other)?;
    state.cancel();
    Ok(Some(output.warnings))
}

/// File menu hook; captured UI state is transient and options use the existing prefs map.
pub fn open_dialog(app: &mut crate::PhotocraftApp) -> std::result::Result<(), String> {
    if let Some(options) = app.session.prefs().dialogs.get("vector.pdf.exportOptions") {
        app.ui.vector_pdf_export.options = serde_json::from_value(options.clone()).map_err(|e| e.to_string())?;
    }
    app.ui.vector_pdf_export.open(&app.session).map_err(|e| e.to_string())
}
/// Registered in the real modal frame by 00. Uses the existing file services and notice UI.
pub fn frame(app: &mut crate::PhotocraftApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.ui.vector_pdf_export);
    state.synchronize(&app.session);
    if state.request.is_some() {
        let modal = egui::Modal::new(egui::Id::new("vector-pdf-export")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            controls(ui, &mut state, |source| crate::i18n::t(source).to_owned())
        });
        match modal.inner {
            Some(Action::Export) => {
                if let Ok(options) = serde_json::to_value(&state.options) {
                    app.session.edit_prefs(|p| {
                        p.dialogs.insert("vector.pdf.exportOptions".into(), options);
                    });
                }
                match save(&mut state, &mut app.session, &mut app.services) {
                    Ok(Some(warnings)) => {
                        let lines = warnings.iter().map(|w| crate::i18n::t(&w.message).to_owned()).collect::<Vec<_>>();
                        crate::notices::io_warnings(app, crate::i18n::t("Export PDF"), &lines);
                    }
                    Ok(None) => {}
                    Err(error) => state.error = Some(error.to_string()),
                }
            }
            Some(Action::Cancel) => state.cancel(),
            None if modal.should_close() => state.cancel(),
            None => {}
        }
    }
    app.ui.vector_pdf_export = state;
}
