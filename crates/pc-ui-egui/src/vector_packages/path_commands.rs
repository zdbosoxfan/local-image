//! Package 36 Object/Path menu fragment and captured dialogs. The host installs menu() in its
//! Object menu, shortcuts() before Pixel shortcuts in Vector persona, and dialog() each frame.
//! UI conventions follow Adobe's Average/Simplify/Split Into Grid/Expand help (SOURCES.md).
use egui::{Key, Modifiers, Ui};
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::{
    Session,
    vector_packages::{
        expand,
        path_commands::{self as engine, Action, Anchors, Axis, Capture, Request, clipboard::Clipboard},
    },
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug)]
enum Dialog {
    Simplify { precision: f64, corner_angle: f64 },
    Average { axis: Axis, anchors: Vec<Anchors> },
    Grid { rows: usize, columns: usize, row_gutter: f64, column_gutter: f64, guides: bool },
    CleanUp { stray: bool, unpainted: bool, text: bool },
    Expand { options: expand::Options },
    Offset { gesture: photocraft_engine::vector_packages::constructive::Gesture, delta: f64 },
}
#[derive(Clone, Default)]
pub struct State {
    pub error: Option<String>,
    pub clipboard: Option<Clipboard>,
    capture: Option<Capture>,
    dialog: Option<Dialog>,
    preview: Option<Arc<Document>>,
    preview_key: u64,
    preview_enabled: bool,
    counts: Option<(usize, usize)>,
}
impl State {
    pub fn cancel(&mut self) {
        self.capture = None;
        self.dialog = None;
        self.preview = None;
        self.counts = None;
    }
    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }
    pub fn display_document(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        self.capture.as_ref()?.check(s).ok()?;
        self.preview.clone().map(|doc| (doc, self.preview_key))
    }
    fn begin(&mut self, s: &Session, ids: &[LayerId], dialog: Dialog) {
        match Capture::begin(s, ids) {
            Ok(c) => {
                self.capture = Some(c);
                self.dialog = Some(dialog);
                self.preview_enabled = true;
                self.error = None;
                self.rebuild(s);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn open_simplify(&mut self, s: &Session, ids: &[LayerId]) {
        self.begin(s, ids, Dialog::Simplify { precision: 90., corner_angle: 30. });
    }
    pub fn open_grid(&mut self, s: &Session, ids: &[LayerId]) {
        self.begin(s, ids, Dialog::Grid { rows: 2, columns: 2, row_gutter: 0., column_gutter: 0., guides: false });
    }
    pub fn open_expand(&mut self, s: &Session, ids: &[LayerId]) {
        self.begin(s, ids, Dialog::Expand { options: Default::default() });
    }
    fn action(dialog: &Dialog) -> Option<Action> {
        Some(match dialog {
            Dialog::Simplify { precision, corner_angle } => Action::Simplify { precision: *precision, corner_angle: *corner_angle },
            Dialog::Average { axis, anchors } => Action::Average { axis: *axis, anchors: anchors.clone() },
            Dialog::Grid { rows, columns, row_gutter, column_gutter, guides } => {
                Action::Grid { rows: *rows, columns: *columns, row_gutter: *row_gutter, column_gutter: *column_gutter, guides: *guides }
            }
            Dialog::CleanUp { stray, unpainted, text } => Action::CleanUp { stray_points: *stray, unpainted: *unpainted, empty_text: *text },
            Dialog::Expand { .. } | Dialog::Offset { .. } => return None,
        })
    }
    fn rebuild(&mut self, s: &Session) {
        self.preview = None;
        self.error = None;
        let (Some(capture), Some(dialog)) = (&self.capture, &self.dialog) else { return };
        let result = if let Some(action) = Self::action(dialog) {
            engine::prepare(s, &Request { capture: capture.clone(), action }).map(|p| {
                self.counts = Some((p.original_points, p.new_points));
                p.document
            })
        } else {
            match dialog {
                Dialog::Expand { options } => {
                    expand::prepare(s, &expand::Request { capture: capture.clone(), options: *options, appearance: false }).map(|p| p.document)
                }
                Dialog::Offset { gesture, delta } => gesture
                    .preview(&photocraft_engine::vector_packages::constructive::Action::Contour {
                        delta: *delta,
                        join: photocraft_engine::vector_packages::constructive::JoinKind::Miter,
                        miter: 4.,
                    })
                    .and_then(|p| p.document()),
                _ => return,
            }
        };
        match result {
            Ok(doc) => {
                if self.preview_enabled {
                    self.preview = Some(Arc::new(doc));
                    self.preview_key = self.preview_key.wrapping_add(1) | (1 << 54);
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn commit_dialog(&mut self, s: &mut Session) {
        let (Some(capture), Some(dialog)) = (&self.capture, &self.dialog) else { return };
        let result = if let Some(action) = Self::action(dialog) {
            s.execute("object.path.commit", json!({"capture":capture,"action":action}))
        } else {
            match dialog {
                Dialog::Expand { options } => s.execute("object.expand", json!({"capture":capture,"options":options,"appearance":false})),
                Dialog::Offset { gesture, delta } => gesture.commit(
                    s,
                    photocraft_engine::vector_packages::constructive::Action::Contour {
                        delta: *delta,
                        join: photocraft_engine::vector_packages::constructive::JoinKind::Miter,
                        miter: 4.,
                    },
                ),
                _ => return,
            }
        };
        match result {
            Ok(_) => self.cancel(),
            Err(e) => {
                self.error = Some(e.to_string());
                self.preview = None;
            }
        }
    }
    /// Call even when the canvas is hidden. Escape, document switches and lost focus cancel.
    pub fn dialog(&mut self, ctx: &egui::Context, s: &mut Session, tr: impl Fn(&str) -> String) {
        if self.capture.as_ref().is_some_and(|c| c.check(s).is_err())
            || ctx.input(|i| i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))))
        {
            self.cancel();
            return;
        }
        let Some(mut dialog) = self.dialog.take() else { return };
        let title = match dialog {
            Dialog::Simplify { .. } => "Simplify",
            Dialog::Average { .. } => "Average",
            Dialog::Grid { .. } => "Split Into Grid",
            Dialog::CleanUp { .. } => "Clean Up",
            Dialog::Expand { .. } => "Expand",
            Dialog::Offset { .. } => "Offset Path",
        };
        let mut open = true;
        let mut changed = false;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(tr(title)).id(egui::Id::new("object_path_dialog36")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            match &mut dialog {
                Dialog::Simplify { precision, corner_angle } => {
                    changed |= ui.add(egui::Slider::new(precision, 0.0..=100.0).text(tr("Curve precision"))).changed();
                    changed |= ui.add(egui::Slider::new(corner_angle, 0.0..=180.0).text(tr("Corner angle threshold"))).changed();
                    if let Some((original, new)) = self.counts {
                        ui.label(format!("{}: {original}    {}: {new}", tr("Original points"), tr("New points")));
                    }
                }
                Dialog::Average { axis, .. } => {
                    for (value, label) in [(Axis::Horizontal, "Horizontal"), (Axis::Vertical, "Vertical"), (Axis::Both, "Both")] {
                        changed |= ui.radio_value(axis, value, tr(label)).changed();
                    }
                }
                Dialog::Grid { rows, columns, row_gutter, column_gutter, guides } => {
                    changed |= numeric(ui, &tr("Rows"), rows, 1..=64);
                    changed |= numeric(ui, &tr("Columns"), columns, 1..=64);
                    changed |= numeric(ui, &tr("Row gutter"), row_gutter, 0.0..=1_000_000.0);
                    changed |= numeric(ui, &tr("Column gutter"), column_gutter, 0.0..=1_000_000.0);
                    changed |= ui.checkbox(guides, tr("Add guides")).changed();
                }
                Dialog::CleanUp { stray, unpainted, text } => {
                    changed |= ui.checkbox(stray, tr("Stray points")).changed();
                    changed |= ui.checkbox(unpainted, tr("Unpainted objects")).changed();
                    changed |= ui.checkbox(text, tr("Empty text")).changed();
                }
                Dialog::Expand { options } => {
                    changed |= ui.checkbox(&mut options.object, tr("Object")).changed();
                    changed |= ui.checkbox(&mut options.fill, tr("Fill")).changed();
                    changed |= ui.checkbox(&mut options.stroke, tr("Stroke")).changed();
                }
                Dialog::Offset { delta, .. } => {
                    changed |= numeric(ui, &tr("Offset"), delta, -10_000.0..=10_000.0);
                }
            }
            changed |= ui.checkbox(&mut self.preview_enabled, tr("Preview")).changed();
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, tr(error));
            }
            ui.horizontal(|ui| {
                apply = ui.add_enabled(self.error.is_none(), egui::Button::new(tr("OK"))).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
        });
        self.dialog = Some(dialog);
        if !open || cancel {
            self.cancel();
            return;
        }
        if changed {
            self.rebuild(s);
        }
        if apply && self.error.is_none() {
            self.commit_dialog(s);
        }
    }
    fn run(&mut self, s: &mut Session, ids: &[LayerId], action: Action) {
        let result = Capture::begin(s, ids).and_then(|capture| s.execute("object.path.commit", json!({"capture":capture,"action":action})));
        if let Err(e) = result {
            self.error = Some(e.to_string());
        }
    }
    pub fn copy(&mut self, s: &Session, ids: &[LayerId]) {
        match Clipboard::copy(s, ids) {
            Ok(clip) => self.clipboard = Some(clip),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn paste(&mut self, s: &mut Session, ids: &[LayerId], front: bool) {
        let result = Capture::begin_paste(s, ids).and_then(|capture| {
            self.clipboard.as_ref().ok_or_else(|| photocraft_engine::EngineError::Other("vector clipboard is empty".into()))?.paste(s, &capture, front)
        });
        if let Err(e) = result {
            self.error = Some(e.to_string());
        }
    }
    fn average(&mut self, s: &Session, ids: &[LayerId], anchors: &[Anchors]) {
        let anchors = if anchors.is_empty() {
            s.active()
                .map(|st| {
                    ids.iter()
                        .filter_map(|id| {
                            st.doc.layer(*id).and_then(|l| match &l.content {
                                LayerContent::Shape(sh) => Some(Anchors {
                                    layer: *id,
                                    points: sh.path.subpaths.iter().enumerate().flat_map(|(sp, p)| (0..p.knots.len()).map(move |k| [sp, k])).collect(),
                                }),
                                _ => None,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            anchors.to_vec()
        };
        self.begin(s, ids, Dialog::Average { axis: Axis::Both, anchors });
    }
    /// Install only for Vector persona, ahead of the Pixel keymap. Text input keeps its keys.
    pub fn shortcuts(&mut self, ctx: &egui::Context, s: &mut Session, ids: &[LayerId], anchors: &[Anchors]) -> bool {
        if ctx.text_edit_focused() || self.is_open() {
            return false;
        }
        let key = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND | Modifiers::ALT, Key::J) {
                Some(0)
            } else if i.consume_key(Modifiers::COMMAND, Key::J) {
                Some(1)
            } else if i.consume_key(Modifiers::COMMAND, Key::F) {
                Some(2)
            } else if i.consume_key(Modifiers::COMMAND, Key::B) {
                Some(3)
            } else if i.consume_key(Modifiers::COMMAND, Key::C) {
                Some(4)
            } else {
                None
            }
        });
        match key {
            Some(0) => self.average(s, ids, anchors),
            Some(1) => self.run(s, ids, Action::Join { tolerance: 1e-6 }),
            Some(2) => self.paste(s, ids, true),
            Some(3) => self.paste(s, ids, false),
            Some(4) => self.copy(s, ids),
            _ => return false,
        }
        true
    }
    /// Exact menu fragment; 00 supplies it from Object and Edit and supplies the i18n closure.
    pub fn menu(&mut self, ui: &mut Ui, s: &mut Session, ids: &[LayerId], anchors: &[Anchors], tr: impl Fn(&str) -> String) {
        ui.menu_button(tr("Path"), |ui| {
            for (label, action) in [
                ("Join", Action::Join { tolerance: 1e-6 }),
                ("Add Anchor Points", Action::AddAnchors),
                ("Remove Anchor Points", Action::RemoveAnchors { anchors: anchors.to_vec() }),
                ("Divide Objects Below", Action::DivideBelow),
            ] {
                if ui
                    .add_enabled(!ids.is_empty() && (!matches!(action, Action::RemoveAnchors { .. }) || !anchors.is_empty()), egui::Button::new(tr(label)))
                    .clicked()
                {
                    self.run(s, ids, action);
                    ui.close();
                }
            }
            if ui.button(tr("Average…")).clicked() {
                self.average(s, ids, anchors);
                ui.close();
            }
            if ui.button(tr("Simplify…")).clicked() {
                self.open_simplify(s, ids);
                ui.close();
            }
            if ui.button(tr("Split Into Grid…")).clicked() {
                self.open_grid(s, ids);
                ui.close();
            }
            if ui.button(tr("Clean Up…")).clicked() {
                let all = s
                    .active()
                    .map(|st| st.doc.walk().into_iter().filter(|(_, _, l)| l.children().is_none()).map(|(_, _, l)| l.id).collect::<Vec<_>>())
                    .unwrap_or_default();
                self.begin(s, &all, Dialog::CleanUp { stray: true, unpainted: true, text: true });
                ui.close();
            }
            if ui.button(tr("Outline Stroke")).clicked() {
                let result = Capture::begin(s, ids).and_then(|_| {
                    if ids.iter().any(|id| {
                        s.active().and_then(|st| st.doc.layer(*id)).is_some_and(|l| {
                            l.vector.appearance.is_some() || l.vector.live_shape.is_some() || !l.vector.corners.is_empty() || l.vector.clip.is_some()
                        })
                    }) {
                        return Err(photocraft_engine::EngineError::Other("Use Expand Appearance to outline live or advanced strokes".into()));
                    }
                    s.execute("path.outlineStroke", json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
                });
                if let Err(e) = result {
                    self.error = Some(e.to_string());
                }
                ui.close();
            }
            if ui.button(tr("Offset Path…")).clicked() {
                match photocraft_engine::vector_packages::constructive::Gesture::begin(s, ids) {
                    Ok(gesture) => self.begin(s, ids, Dialog::Offset { gesture, delta: 0. }),
                    Err(e) => self.error = Some(e.to_string()),
                }
                ui.close();
            }
        });
        let request = Capture::begin(s, ids).map(|capture| expand::Request { capture, options: Default::default(), appearance: true });
        let reason = match &request {
            Ok(r) => expand::disabled_reason(s, r),
            Err(e) => Some(e.to_string()),
        };
        let options_button = ui.add_enabled(reason.is_none(), egui::Button::new(tr("Expand…")));
        let options_clicked = options_button.clicked();
        if let Some(reason) = &reason {
            options_button.on_disabled_hover_text(tr(reason));
        }
        if options_clicked {
            self.open_expand(s, ids);
            ui.close();
        }
        let button = ui.add_enabled(reason.is_none(), egui::Button::new(tr("Expand Appearance")));
        let clicked = button.clicked();
        if let Some(reason) = reason {
            button.on_disabled_hover_text(tr(&reason));
        }
        if clicked && let Ok(r) = request {
            if let Err(e) = s.execute("object.expand", json!(r)) {
                self.error = Some(e.to_string());
            }
            ui.close();
        }
    }
    pub fn paste_menu(&mut self, ui: &mut Ui, s: &mut Session, ids: &[LayerId], tr: impl Fn(&str) -> String) {
        for (label, front) in [("Paste in Front", true), ("Paste in Back", false)] {
            if ui.add_enabled(self.clipboard.is_some() && s.active().is_some(), egui::Button::new(tr(label))).clicked() {
                self.paste(s, ids, front);
                ui.close();
            }
        }
    }
}
fn numeric<N: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut N, range: std::ops::RangeInclusive<N>) -> bool {
    ui.horizontal(|ui| {
        let text = ui.label(label);
        ui.add(egui::DragValue::new(value).range(range)).labelled_by(text.id).changed()
    })
    .inner
}
#[cfg(test)]
#[path = "path_commands/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "path_commands/app_gpu.rs"]
mod app_gpu;
#[path = "path_commands/app_hooks.rs"]
pub mod app_hooks;
