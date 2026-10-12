//! Captured SVG export dialog. 00 supplies app routing, translated English-source
//! catalogue lookup and the file-save service. All export actions use one engine command.
use egui::Ui;
use photocraft_doc::LayerId;
use photocraft_doc::vector_design::exchange::ExportedVector;
use photocraft_engine::vector_packages::svg_export::{Request, Scope, preflight};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_svg::export::ExportOptions;

#[derive(Default)]
pub struct State {
    pub options: ExportOptions,
    pub request: Option<Request>,
    selection: Vec<LayerId>,
    artboards: Vec<(LayerId, String)>,
    artboard: Option<LayerId>,
    pub error: Option<String>,
    pending: Option<super::vector_export::Pending>,
}
impl State {
    pub(crate) fn on_job_event(&mut self, event: &photocraft_engine::jobs::JobEvent) -> bool {
        self.pending.as_mut().is_some_and(|pending| pending.on_event(event))
    }
    pub fn open(&mut self, s: &Session) -> Result<()> {
        self.cancel();
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
        self.pending = None;
    }
    pub fn synchronize(&mut self, s: &Session) {
        if self.pending.is_some() {
            return;
        }
        if self.request.as_ref().is_some_and(|r| s.active().is_none_or(|st| st.doc.id.0 != r.document || st.revision != r.revision)) {
            self.cancel();
        }
    }
    /// File-save services must repeat this guard after asynchronous picker completion.
    pub fn export(&mut self, s: &mut Session) -> Result<(Request, ExportedVector)> {
        let mut r = self.request.clone().ok_or_else(|| EngineError::Other("SVG export dialog is closed".into()))?;
        r.options = self.options.clone();
        preflight(s, &r)?;
        let output = photocraft_engine::vector_packages::svg_export::export(s, &r, &|| false)?;
        Ok((r, output))
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
    ui.heading(text("Export SVG"));
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
    ui.checkbox(&mut state.options.minify, text("Minify"));
    ui.horizontal(|ui| {
        let label = ui.label(text("Decimal places"));
        ui.add(egui::DragValue::new(&mut state.options.precision).range(0..=9)).labelled_by(label.id);
    });
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
#[path = "../../../pc-svg/tests/export/ui.rs"]
mod tests;

/// Invoked by the app modal action. Prepare bytes before the picker, then recheck the
/// captured document/targets immediately before the existing write service publishes.
/// Cancellation performs no write. Return one aggregated warning list for the app notice.
pub fn save(state: &mut State, s: &mut Session, services: &mut crate::Services) -> Result<Option<Vec<photocraft_doc::vector_design::exchange::VectorWarning>>> {
    let (request, output) = state.export(s)?;
    let picker = services.pick_save.as_mut().ok_or_else(|| EngineError::Other("file save picker service unavailable".into()))?;
    let path = picker("drawing.svg");
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

pub fn open_dialog(app: &mut crate::PhotocraftApp) -> std::result::Result<(), String> {
    if let Some(options) = app.session.prefs().dialogs.get("vector.svg.exportOptions") {
        app.ui.svg_export.options = serde_json::from_value(options.clone()).map_err(|e| e.to_string())?;
    }
    app.ui.svg_export.open(&app.session).map_err(|e| e.to_string())
}

/// Interactive export runs projection, encoding and native staging on a worker.
pub fn frame(app: &mut crate::PhotocraftApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.ui.svg_export);
    state.synchronize(&app.session);
    if state.pending.is_some() {
        let cancel = egui::Modal::new(egui::Id::new("vector-svg-export-progress")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            state.pending.as_ref().is_some_and(|pending| pending.controls(ui, &app.session))
        });
        if cancel.inner || cancel.should_close() {
            if let Some(pending) = &mut state.pending {
                pending.cancel(&mut app.session);
            }
            state.cancel();
            app.ui.status = tl!("Export cancelled").into();
            app.ui.status_error = false;
        } else if let Some(pending) = &mut state.pending {
            match pending.poll(app) {
                Ok(super::vector_export::Poll::Complete(completion)) => {
                    state.cancel();
                    super::vector_export::report(app, completion);
                }
                Ok(super::vector_export::Poll::Pending) => {}
                Ok(super::vector_export::Poll::Cancelled) => {
                    state.cancel();
                    app.ui.status = tl!("Export cancelled").into();
                    app.ui.status_error = false;
                }
                Err(error) => {
                    pending.cancel(&mut app.session);
                    pending.report_failure(app, &error);
                    state.pending = None;
                    state.error = Some(error);
                }
            }
        }
    } else if state.request.is_some() {
        let modal = egui::Modal::new(egui::Id::new("vector-svg-export")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            controls(ui, &mut state, |text| crate::i18n::t(text).to_owned())
        });
        match modal.inner {
            Some(Action::Export) => {
                let Some(mut request) = state.request.clone() else {
                    app.ui.svg_export = state;
                    return;
                };
                request.options = state.options.clone();
                match super::vector_export::begin(app, super::vector_export::Request::Svg(request)) {
                    Ok(Some(pending)) => {
                        state.pending = Some(pending);
                        state.error = None;
                        if let Ok(options) = serde_json::to_value(&state.options) {
                            app.session.edit_prefs(|prefs| {
                                prefs.dialogs.insert("vector.svg.exportOptions".into(), options);
                            });
                        }
                    }
                    Ok(None) => state.cancel(),
                    Err(error) => state.error = Some(error),
                }
            }
            Some(Action::Cancel) => state.cancel(),
            None if modal.should_close() => state.cancel(),
            _ => {}
        }
    }
    app.ui.svg_export = state;
}
