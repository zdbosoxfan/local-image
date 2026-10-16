//! Effect menu, editable Preview dialog and Appearance section (original Local Image UI).
//! Captured document/revision/targets stay fixed through preview and release. 00 routes
//! detached preview into the canvas; commands publish only on OK or a panel edit.
use egui::{Context, Ui};
pub use engine::{EffectKind, EffectTarget, OffsetJoin, PathEffect, Points, ShapeKind};
use photocraft_doc::{Document, LayerId};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::path_effects::{self as engine, Access, Gesture, Request},
};
use std::sync::Arc;
pub type Dispatch = fn(&mut Session, &Request, Access) -> Result<()>;
pub fn command(s: &mut Session, r: &Request, _: Access) -> Result<()> {
    engine::commit(s, r)
}
#[derive(Clone)]
struct Dialog {
    gesture: Gesture,
    index: usize,
    preview: bool,
}
#[derive(Clone, Default)]
pub struct State {
    dialog: Option<Dialog>,
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub error: Option<String>,
}
fn bad(s: &str) -> EngineError {
    EngineError::Other(s.into())
}
impl State {
    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }
    pub fn cancel(&mut self) {
        self.dialog = None;
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        if !enabled || self.dialog.as_ref().is_some_and(|d| d.gesture.request.capture.check(s).is_err()) {
            self.cancel();
        }
    }
    pub fn display_document(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        self.dialog.as_ref()?.gesture.request.capture.check(s).ok()?;
        self.preview.as_ref().map(|p| (p.clone(), self.generation))
    }
    pub fn open_add(&mut self, s: &Session, ids: &[LayerId], effect: EffectKind, target: EffectTarget, a: Access) -> Result<()> {
        self.cancel();
        self.error = None;
        let mut g = Gesture::begin(s, ids, a)?;
        let index = g.request.edits.first().ok_or_else(|| bad("select a shape"))?.effects.len();
        if g.request.edits.iter().any(|e| e.effects.len() != index) {
            return Err(bad("selected shapes have different effect stacks"));
        }
        g.update(s, |effects| {
            effects.push(PathEffect { effect: effect.clone(), target, enabled: true });
            Ok(())
        })?;
        self.dialog = Some(Dialog { gesture: g, index, preview: true });
        self.refresh(s, a)
    }
    pub fn open_edit(&mut self, s: &Session, ids: &[LayerId], index: usize, a: Access) -> Result<()> {
        self.cancel();
        self.error = None;
        let g = Gesture::begin(s, ids, a)?;
        if g.request.edits.iter().any(|e| index >= e.effects.len()) {
            return Err(bad("effect index is absent"));
        }
        self.dialog = Some(Dialog { gesture: g, index, preview: true });
        self.refresh(s, a)
    }
    fn refresh(&mut self, s: &Session, a: Access) -> Result<()> {
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
        if let Some(d) = &self.dialog
            && d.preview
        {
            self.preview = Some(Arc::new(engine::prepare(s, &d.gesture.request, a)?.preview));
        }
        Ok(())
    }
    fn record(&mut self, r: Result<()>) {
        self.error = r.err().map(|e| e.to_string());
    }
    /// Effect menu fragment. Host supplies translator and current Appearance item target.
    pub fn menu(&mut self, ui: &mut Ui, s: &Session, ids: &[LayerId], target: EffectTarget, a: Access, tr: impl Fn(&str) -> String) {
        for (group, range) in [("Distort & Transform", 0..5), ("Stylize", 5..7), ("Other Path Effects", 7..9)] {
            ui.menu_button(tr(group), |ui| {
                for effect in EffectKind::presets().into_iter().skip(range.start).take(range.len()) {
                    if ui.add_enabled(!ids.is_empty(), egui::Button::new(tr(effect.label()))).clicked() {
                        let r = self.open_add(s, ids, effect, target, a);
                        self.record(r);
                        ui.close();
                    }
                }
            });
        }
    }
    fn panel_edit(&mut self, s: &mut Session, ids: &[LayerId], a: Access, dispatch: Dispatch, f: impl FnMut(&mut Vec<PathEffect>) -> Result<()>) -> Result<()> {
        self.cancel();
        let mut g = Gesture::begin(s, ids, a)?;
        g.update(s, f)?;
        dispatch(s, &g.request, a)
    }
    /// Reorder by drag or keyboard-accessible up/down buttons; toggles and deletion each
    /// capture and validate the complete selection and submit one journaled request.
    pub fn panel(&mut self, ui: &mut Ui, s: &mut Session, ids: &[LayerId], a: Access, dispatch: Dispatch, tr: impl Fn(&str) -> String) {
        self.synchronize(s, true);
        ui.label(tr("Path Effects"));
        ui.menu_button(tr("Add Path Effect"), |ui| self.menu(ui, s, ids, EffectTarget::Layer, a, &tr));
        let Some(doc) = s.active().map(|st| st.doc.clone()) else { return };
        let Some(layer) = ids.first().and_then(|id| doc.layer(*id)) else { return };
        let effects = match (a.read)(layer) {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if ids.iter().skip(1).any(|id| {
            doc.layer(*id).is_none_or(|l| match (a.read)(l) {
                Ok(other) => other != effects,
                Err(_) => true,
            })
        }) {
            ui.label(tr("Selected shapes have different effect stacks"));
            return;
        }
        let mut action = None;
        for (i, effect) in effects.iter().enumerate() {
            let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::new(), |ui| {
                ui.horizontal(|ui| {
                    ui.dnd_drag_source(egui::Id::new(("path-effect-45", i)), i, |ui| {
                        ui.label(tr("Drag effect"));
                    });
                    let mut enabled = effect.enabled;
                    if ui.checkbox(&mut enabled, tr("Effect visible")).changed() {
                        action = Some(Action::Visible(i, enabled));
                    }
                    if ui.button(tr(effect.effect.label())).clicked() {
                        action = Some(Action::Edit(i));
                    }
                    if ui.add_enabled(i > 0, egui::Button::new(tr("Move effect up"))).clicked() {
                        action = Some(Action::Move(i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < effects.len(), egui::Button::new(tr("Move effect down"))).clicked() {
                        action = Some(Action::Move(i, i + 1));
                    }
                    if ui.button(tr("Remove effect")).clicked() {
                        action = Some(Action::Remove(i));
                    }
                });
            });
            if let Some(from) = dropped
                && *from != i
            {
                action = Some(Action::Move(*from, i));
            }
        }
        if let Some(action) = action {
            let r = match action {
                Action::Edit(i) => self.open_edit(s, ids, i, a),
                other => self.panel_edit(s, ids, a, dispatch, |effects| {
                    match other {
                        Action::Visible(i, v) => effects.get_mut(i).ok_or_else(|| bad("effect index is absent"))?.enabled = v,
                        Action::Move(from, to) => {
                            if from >= effects.len() || to >= effects.len() {
                                return Err(bad("effect index is absent"));
                            }
                            let e = effects.remove(from);
                            effects.insert(to, e);
                        }
                        Action::Remove(i) => {
                            if i >= effects.len() {
                                return Err(bad("effect index is absent"));
                            }
                            effects.remove(i);
                        }
                        Action::Edit(_) => {}
                    }
                    Ok(())
                }),
            };
            self.record(r);
        }
    }
    pub fn dialog(&mut self, ctx: &Context, s: &mut Session, a: Access, dispatch: Dispatch, tr: impl Fn(&str) -> String) {
        self.synchronize(s, true);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel();
            return;
        }
        let Some(mut dialog) = self.dialog.clone() else { return };
        let Some(first) = dialog.gesture.request.edits.first().and_then(|e| e.effects.get(dialog.index)) else {
            self.cancel();
            return;
        };
        let mut effect = first.clone();
        let mut changed = false;
        let mut ok = false;
        let mut cancel = false;
        let mut open = true;
        egui::Window::new(tr("Path Effect Options")).id(egui::Id::new("path-effect-options-45")).open(&mut open).collapsible(false).resizable(true).show(
            ctx,
            |ui| {
                ui.label(tr(effect.effect.label()));
                changed |= controls(ui, &mut effect.effect, &tr);
                let mut per_item = matches!(effect.target, EffectTarget::Item { .. });
                if ui.checkbox(&mut per_item, tr("Appearance item only")).changed() {
                    effect.target = if per_item { EffectTarget::Item { index: 0 } } else { EffectTarget::Layer };
                    changed = true;
                }
                if let EffectTarget::Item { index } = &mut effect.target {
                    changed |= numeric(ui, &tr("Appearance item"), index, 0..=127);
                }
                changed |= ui.checkbox(&mut dialog.preview, tr("Preview")).changed();
                if let Some(error) = &self.error {
                    ui.label(tr(error));
                }
                ui.horizontal(|ui| {
                    ok = ui.add_enabled(self.error.is_none(), egui::Button::new(tr("OK"))).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            },
        );
        if cancel || !open {
            self.cancel();
            return;
        }
        if changed {
            let index = dialog.index;
            let r = dialog.gesture.update(s, |effects| {
                *effects.get_mut(index).ok_or_else(|| bad("effect index is absent"))? = effect.clone();
                Ok(())
            });
            if let Err(e) = r {
                self.preview = None;
                self.error = Some(e.to_string());
                return;
            }
            self.dialog = Some(dialog.clone());
            let r = self.refresh(s, a);
            self.record(r);
        } else {
            self.dialog = Some(dialog.clone());
        }
        if ok {
            match dispatch(s, &dialog.gesture.request, a) {
                Ok(()) => {
                    self.cancel();
                    self.error = None;
                }
                Err(e) => {
                    self.preview = None;
                    self.error = Some(e.to_string());
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Edit(usize),
    Visible(usize, bool),
    Move(usize, usize),
    Remove(usize),
}
fn numeric<N: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut N, range: std::ops::RangeInclusive<N>) -> bool {
    ui.horizontal(|ui| {
        let label = ui.label(label);
        ui.add(egui::DragValue::new(value).range(range)).labelled_by(label.id).changed()
    })
    .inner
}
fn points(ui: &mut Ui, v: &mut Points, tr: &impl Fn(&str) -> String) -> bool {
    let old = *v;
    ui.horizontal(|ui| {
        ui.selectable_value(v, Points::Smooth, tr("Smooth"));
        ui.selectable_value(v, Points::Corner, tr("Corner"));
    });
    old != *v
}
fn pair(ui: &mut Ui, v: &mut [f64; 2], labels: [&str; 2], lo: f64, hi: f64, tr: &impl Fn(&str) -> String) -> bool {
    let a = numeric(ui, &tr(labels[0]), &mut v[0], lo..=hi);
    numeric(ui, &tr(labels[1]), &mut v[1], lo..=hi) | a
}
fn controls(ui: &mut Ui, e: &mut EffectKind, tr: &impl Fn(&str) -> String) -> bool {
    let mut changed = false;
    match e {
        EffectKind::Roughen { size, relative, detail, points: p, seed } => {
            changed |= numeric(ui, &tr("Size"), size, 0.0..=10000.);
            changed |= ui.checkbox(relative, tr("Relative size")).changed();
            changed |= numeric(ui, &tr("Detail per inch"), detail, 0.0..=100.);
            changed |= points(ui, p, tr);
            changed |= numeric(ui, &tr("Seed"), seed, 0..=u64::MAX);
        }
        EffectKind::ZigZag { size, relative, ridges, points: p } => {
            changed |= numeric(ui, &tr("Size"), size, -10000.0..=10000.);
            changed |= ui.checkbox(relative, tr("Relative size")).changed();
            changed |= numeric(ui, &tr("Ridges per segment"), ridges, 0..=100);
            changed |= points(ui, p, tr);
        }
        EffectKind::PuckerBloat { amount } => {
            changed |= ui.add(egui::Slider::new(amount, -200.0..=200.).text(tr("Amount"))).changed();
        }
        EffectKind::Twist { angle } => {
            changed |= numeric(ui, &tr("Angle"), angle, -3600.0..=3600.);
        }
        EffectKind::Transform { scale, move_by, angle, reflect, copies } => {
            changed |= pair(ui, scale, ["Horizontal scale", "Vertical scale"], 0.01, 10000., tr);
            changed |= pair(ui, move_by, ["Horizontal move", "Vertical move"], -100000., 100000., tr);
            changed |= numeric(ui, &tr("Angle"), angle, -3600.0..=3600.);
            changed |= ui.checkbox(&mut reflect[0], tr("Reflect X")).changed();
            changed |= ui.checkbox(&mut reflect[1], tr("Reflect Y")).changed();
            changed |= numeric(ui, &tr("Copies"), copies, 0..=1000);
        }
        EffectKind::RoundCorners { radius } => {
            changed |= numeric(ui, &tr("Radius"), radius, 0.0..=100000.);
        }
        EffectKind::OffsetPath { distance, join, miter } => {
            changed |= numeric(ui, &tr("Distance"), distance, -100000.0..=100000.);
            let old = *join;
            ui.horizontal(|ui| {
                ui.selectable_value(join, OffsetJoin::Miter, tr("Miter"));
                ui.selectable_value(join, OffsetJoin::Round, tr("Round"));
                ui.selectable_value(join, OffsetJoin::Bevel, tr("Bevel"));
            });
            changed |= old != *join;
            changed |= numeric(ui, &tr("Miter limit"), miter, 1.0..=1000.);
        }
        EffectKind::FreeDistort { corners } => {
            for (c, label) in corners.iter_mut().zip(["Top left", "Top right", "Bottom right", "Bottom left"]) {
                ui.label(tr(label));
                changed |= pair(ui, c, ["Corner X", "Corner Y"], -16., 16., tr);
            }
        }
        EffectKind::ConvertToShape { shape, extra, radius } => {
            let old = *shape;
            ui.horizontal(|ui| {
                ui.selectable_value(shape, ShapeKind::Rectangle, tr("Rectangle"));
                ui.selectable_value(shape, ShapeKind::RoundedRectangle, tr("Rounded rectangle"));
                ui.selectable_value(shape, ShapeKind::Ellipse, tr("Ellipse"));
            });
            changed |= old != *shape;
            changed |= pair(ui, extra, ["Extra width", "Extra height"], -100000., 100000., tr);
            changed |= numeric(ui, &tr("Radius"), radius, 0.0..=100000.);
        }
    }
    changed
}
#[cfg(test)]
#[path = "path_effects/app_gpu.rs"]
mod app_gpu;
#[path = "path_effects/app_hooks.rs"]
pub mod app_hooks;
#[cfg(test)]
#[path = "path_effects/tests.rs"]
mod tests;
