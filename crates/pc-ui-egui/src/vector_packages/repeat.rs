//! Editable repeat panel and on-canvas controls. Every gesture captures document/revision
//! and target IDs; preview is detached, cancellation discards it, release journals once.
use egui::{Color32, Key, Pos2, Rect, Stroke, Ui};
use engine::{
    Capture, RepeatSpec,
    evaluator::{self, Bounds},
};
use photocraft_doc::{Document, LayerId, vector_design::live::LiveGroup};
use photocraft_engine::{EngineError, Result, Session, vector_packages::repeat as engine};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[path = "repeat/app_hooks.rs"]
pub mod app_hooks;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub radial_count: u32,
    pub rows: u32,
    pub cols: u32,
    pub spacing: [f64; 2],
}
impl Default for Preferences {
    fn default() -> Self {
        Self { radial_count: 8, rows: 3, cols: 3, spacing: [8., 8.] }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Count,
    Rows,
    Cols,
    SpacingX,
    SpacingY,
    Center,
    Angle,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub origin: Pos2,
    pub zoom: f64,
    pub flip: bool,
    pub rotation: f32,
}
impl View {
    fn camera(self) -> crate::view_camera::Camera {
        crate::view_camera::Camera::new(Rect::from_center_size(self.origin, egui::Vec2::ZERO), self.zoom as f32, [0.0, 0.0], self.flip, self.rotation)
    }
    pub fn point(self, pos: Pos2) -> Point {
        let d = self.camera().to_document(pos);
        Point::new(f64::from(d.x), f64::from(d.y))
    }
    pub fn screen(self, p: Point) -> Pos2 {
        self.camera().to_screen(egui::pos2(p.x as f32, p.y as f32))
    }
    fn valid(self) -> bool {
        self.rotation.is_finite() && self.origin.x.is_finite() && self.origin.y.is_finite() && self.zoom.is_finite() && (1e-6..=1e6).contains(&self.zoom)
    }
}
#[derive(Clone, Debug)]
struct Drag {
    capture: Capture,
    original: RepeatSpec,
    handle: Handle,
    start: Point,
    view: View,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub open: bool,
    pub preferences: Preferences,
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub error: Option<String>,
    draft: Option<RepeatSpec>,
    panel: Option<Capture>,
    drag: Option<Drag>,
    dirty: bool,
}
impl State {
    pub fn captured(&self) -> bool {
        self.drag.is_some() || self.panel.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.panel = None;
        self.preview = None;
        self.draft = None;
        self.dirty = false;
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        let capture = self.drag.as_ref().map(|d| &d.capture).or(self.panel.as_ref());
        if !enabled || capture.is_some_and(|c| c.check(s).is_err() || s.active().is_none_or(|st| st.selected_layers() != c.targets)) {
            self.cancel();
        }
    }
    fn failed(&mut self, e: EngineError) {
        self.cancel();
        self.error = Some(e.to_string());
    }
    fn preview(&mut self, s: &Session, capture: &Capture, spec: RepeatSpec) -> Result<()> {
        let doc = engine::preview(s, capture, &spec)?;
        self.generation = self.generation.checked_add(1).ok_or_else(|| EngineError::Other("repeat preview generation exhausted".into()))?;
        self.preview = Some(Arc::new(doc));
        self.draft = Some(spec);
        self.dirty = true;
        Ok(())
    }
    fn commit(&mut self, s: &mut Session, capture: Capture) -> Result<()> {
        let spec = self.draft.clone().ok_or_else(|| EngineError::Other("repeat preview missing".into()))?;
        capture.check(s)?;
        let mut params = capture.params();
        params["spec"] = serde_json::to_value(spec).map_err(|e| EngineError::Other(e.to_string()))?;
        s.execute("vector.repeat.set", params)?;
        self.cancel();
        Ok(())
    }
}
fn selected(s: &Session) -> Vec<LayerId> {
    s.active().map(|st| st.selected_layers()).unwrap_or_default()
}
fn current(s: &Session) -> Option<RepeatSpec> {
    let st = s.active()?;
    let ids = st.selected_layers();
    let l = st.doc.layer(*ids.first()?)?;
    match &l.vector.live {
        Some(LiveGroup::Repeat(spec)) if l.children().is_some() => Some(spec.clone()),
        _ => None,
    }
}
fn target_bounds(s: &Session) -> Result<Bounds> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids = st.selected_layers();
    if let [id] = ids.as_slice() {
        let layer = st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?;
        let sources = if matches!(layer.vector.live, Some(LiveGroup::Repeat(_))) { layer.children().unwrap_or(&[]) } else { std::slice::from_ref(layer) };
        return evaluator::source_bounds(sources).map_err(|e| EngineError::Other(e.to_string()));
    }
    let layers = ids.iter().map(|id| st.doc.layer(*id).cloned().ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    let sources = if layers.len() == 1 && matches!(layers[0].vector.live, Some(LiveGroup::Repeat(_))) { layers[0].children().unwrap_or(&[]) } else { &layers };
    evaluator::source_bounds(sources).map_err(|e| EngineError::Other(e.to_string()))
}

pub fn panel(ui: &mut Ui, s: &mut Session, state: &mut State, tr: impl Fn(&str) -> String) {
    state.synchronize(s, true);
    ui.horizontal(|ui| {
        for (kind, label) in [(0, "Radial Repeat"), (1, "Grid Repeat"), (2, "Mirror Repeat")] {
            if ui.button(tr(label)).clicked() {
                let result = (|| {
                    state.cancel();
                    let b = target_bounds(s)?;
                    let p = &state.preferences;
                    let spec = match kind {
                        0 => RepeatSpec::Radial { count: p.radial_count, center: [b.center().x, b.max[1] + b.width().max(b.height()).max(10.)], angle: 0. },
                        1 => RepeatSpec::Grid { rows: p.rows, cols: p.cols, spacing: p.spacing },
                        _ => RepeatSpec::Mirror { origin: [b.max[0] + 8., b.center().y], angle: std::f64::consts::FRAC_PI_2 },
                    };
                    let capture = Capture::new(s, &selected(s))?;
                    let command = if current(s).is_some() { "vector.repeat.set" } else { "vector.repeat.create" };
                    let mut params = capture.params();
                    params["spec"] = serde_json::to_value(spec).map_err(|e| EngineError::Other(e.to_string()))?;
                    s.execute(command, params)?;
                    Ok(())
                })();
                if let Err(e) = result {
                    state.failed(e);
                } else {
                    state.error = None;
                }
            }
        }
    });
    let mut spec = state.draft.clone().or_else(|| current(s));
    if let Some(spec) = &mut spec {
        let mut changed = false;
        let mut started = false;
        let mut finished = false;
        let mut input = |ui: &mut Ui, label: &str, value: egui::DragValue<'_>| {
            ui.horizontal(|ui| {
                let label = ui.label(tr(label));
                let response = ui.add(value).labelled_by(label.id);
                changed |= response.changed();
                started |= response.drag_started() || response.gained_focus();
                finished |= response.drag_stopped() || (response.lost_focus() && state.dirty);
            });
        };
        match spec {
            RepeatSpec::Radial { count, center, angle } => {
                input(ui, "Repeat count", egui::DragValue::new(count).range(1..=evaluator::MAX_INSTANCES));
                input(ui, "Repeat center X", egui::DragValue::new(&mut center[0]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE));
                input(ui, "Repeat center Y", egui::DragValue::new(&mut center[1]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE));
                input(ui, "Repeat angle", egui::DragValue::new(angle).speed(0.01).range(-std::f64::consts::TAU..=std::f64::consts::TAU));
            }
            RepeatSpec::Grid { rows, cols, spacing } => {
                input(ui, "Repeat rows", egui::DragValue::new(rows).range(1..=evaluator::MAX_INSTANCES));
                input(ui, "Repeat columns", egui::DragValue::new(cols).range(1..=evaluator::MAX_INSTANCES));
                input(
                    ui,
                    "Horizontal repeat spacing",
                    egui::DragValue::new(&mut spacing[0]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE),
                );
                input(
                    ui,
                    "Vertical repeat spacing",
                    egui::DragValue::new(&mut spacing[1]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE),
                );
            }
            RepeatSpec::Mirror { origin, angle } => {
                input(ui, "Repeat center X", egui::DragValue::new(&mut origin[0]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE));
                input(ui, "Repeat center Y", egui::DragValue::new(&mut origin[1]).speed(0.5).range(-evaluator::MAX_COORDINATE..=evaluator::MAX_COORDINATE));
                input(ui, "Repeat angle", egui::DragValue::new(angle).speed(0.01).range(-std::f64::consts::TAU..=std::f64::consts::TAU));
            }
        }
        if (changed || started) && state.panel.is_none() && state.drag.is_none() {
            match Capture::new(s, &selected(s)) {
                Ok(c) => state.panel = Some(c),
                Err(e) => state.failed(e),
            }
        }
        if changed
            && let Some(capture) = state.panel.clone()
            && let Err(e) = state.preview(s, &capture, spec.clone())
        {
            state.failed(e);
        }
        let apply = ui.add_enabled(state.dirty, egui::Button::new(tr("Apply Repeat"))).clicked();
        let cancelled = ui.button(tr("Cancel Repeat Preview")).clicked();
        if cancelled {
            state.cancel();
        }
        finished |= ui.input(|i| i.key_pressed(Key::Enter));
        if !cancelled
            && (apply || finished)
            && state.dirty
            && let Some(capture) = state.panel.clone()
            && let Err(e) = state.commit(s, capture)
        {
            state.failed(e);
        }
        if ui.button(tr("Expand Repeat")).clicked() {
            state.cancel();
            let result = Capture::new(s, &selected(s)).and_then(|c| s.execute("vector.repeat.expand", c.params()).map(|_| ()));
            if let Err(e) = result {
                state.failed(e);
            }
        }
    }
    if let Some(error) = &state.error {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
}

/// Handle locations in document space; hit tolerance stays in screen points.
pub fn handles(spec: &RepeatSpec, b: Bounds) -> Vec<(Handle, Point)> {
    match spec {
        RepeatSpec::Grid { rows, cols, spacing } => {
            let x = b.width() + spacing[0];
            let y = b.height() + spacing[1];
            vec![
                (Handle::Cols, Point::new(b.max[0] + x * f64::from(cols.saturating_sub(1)) + 12., b.min[1] - 12.)),
                (Handle::Rows, Point::new(b.min[0] - 12., b.max[1] + y * f64::from(rows.saturating_sub(1)) + 12.)),
                (Handle::SpacingX, Point::new(b.max[0] + spacing[0], b.center().y)),
                (Handle::SpacingY, Point::new(b.center().x, b.max[1] + spacing[1])),
            ]
        }
        RepeatSpec::Radial { center, angle, .. } => {
            let radius = (b.center().x - center[0]).hypot(b.center().y - center[1]).max(24.);
            vec![
                (Handle::Center, Point::new(center[0], center[1])),
                (Handle::Count, Point::new(center[0] + radius + 20., center[1])),
                (Handle::Angle, Point::new(center[0] + radius * angle.cos(), center[1] + radius * angle.sin())),
            ]
        }
        RepeatSpec::Mirror { origin, angle } => {
            vec![(Handle::Center, Point::new(origin[0], origin[1])), (Handle::Angle, Point::new(origin[0] + 40. * angle.cos(), origin[1] + 40. * angle.sin()))]
        }
    }
}
fn moved(drag: &Drag, p: Point, mods: egui::Modifiers) -> RepeatSpec {
    let mut spec = drag.original.clone();
    let dx = p.x - drag.start.x;
    let dy = p.y - drag.start.y;
    let count = |n: u32, delta: f64| (f64::from(n) + (delta * drag.view.zoom / 12.).round()).clamp(1., f64::from(evaluator::MAX_INSTANCES)) as u32;
    let gap = |v: f64| if mods.shift { v.round() } else { v };
    match (&mut spec, drag.handle) {
        (RepeatSpec::Grid { cols, .. }, Handle::Cols) => *cols = count(*cols, dx),
        (RepeatSpec::Grid { rows, .. }, Handle::Rows) => *rows = count(*rows, dy),
        (RepeatSpec::Grid { spacing, .. }, Handle::SpacingX) => spacing[0] = gap(spacing[0] + dx),
        (RepeatSpec::Grid { spacing, .. }, Handle::SpacingY) => spacing[1] = gap(spacing[1] + dy),
        (RepeatSpec::Radial { count: n, .. }, Handle::Count) => *n = count(*n, dx),
        (RepeatSpec::Radial { center, .. }, Handle::Center) | (RepeatSpec::Mirror { origin: center, .. }, Handle::Center) => {
            center[0] += dx;
            center[1] += dy;
        }
        (RepeatSpec::Radial { center, angle, .. }, Handle::Angle) | (RepeatSpec::Mirror { origin: center, angle }, Handle::Angle) => {
            let start = (drag.start.y - center[1]).atan2(drag.start.x - center[0]);
            let end = (p.y - center[1]).atan2(p.x - center[0]);
            *angle += end - start;
            if mods.shift {
                *angle = (*angle / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            }
        }
        _ => {}
    }
    spec
}
/// Called before legacy Move/PathSelection dispatch, even when the pointer leaves the canvas.
pub fn canvas(ui: &mut Ui, s: &mut Session, state: &mut State, rect: Rect, view: View, enabled: bool) -> bool {
    let consumed = input(ui, s, state, rect, view, enabled);
    if enabled {
        paint(ui, s, state, view);
    }
    consumed
}
/// Input-only entrypoint for routers that run captured input before painting the image.
pub fn input(ui: &mut Ui, s: &mut Session, state: &mut State, rect: Rect, view: View, enabled: bool) -> bool {
    state.synchronize(s, enabled && view.valid());
    let cancel = ui.input(|i| {
        !i.focused || i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::PointerGone | egui::Event::WindowFocused(false)))
    });
    if cancel {
        let consumed = state.captured();
        state.cancel();
        return consumed;
    }
    if !enabled || !view.valid() {
        return false;
    }
    let Some(spec) = state.draft.clone().or_else(|| current(s)) else {
        return false;
    };
    if selected(s).len() != 1 {
        return false;
    }
    let Ok(b) = target_bounds(s) else {
        return false;
    };
    let positions = handles(&spec, b);
    let (pressed, released, down, pos, mods) =
        ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down(), i.pointer.interact_pos(), i.modifiers));
    let mut consumed = state.drag.is_some();
    if pressed
        && state.drag.is_none()
        && let Some(pos) = pos.filter(|p| rect.contains(*p))
        && let Some((handle, _)) = positions.iter().find(|(_, p)| view.screen(*p).distance(pos) <= 8.)
    {
        let result = Capture::new(s, &selected(s));
        match result {
            Ok(capture) => {
                state.panel = None;
                state.drag = Some(Drag { capture, original: spec.clone(), handle: *handle, start: view.point(pos), view });
                consumed = true;
            }
            Err(e) => state.failed(e),
        }
    }
    if let Some(drag) = state.drag.clone()
        && let Some(pos) = pos
        && (down || released)
    {
        let spec = moved(&drag, drag.view.point(pos), mods);
        if (spec != drag.original || state.dirty)
            && let Err(e) = state.preview(s, &drag.capture, spec)
        {
            state.failed(e);
        }
        if released && state.drag.is_some() {
            if state.dirty {
                if let Err(e) = state.commit(s, drag.capture) {
                    state.failed(e);
                }
            } else {
                state.cancel();
            }
        }
    }
    consumed
}
/// Draw after the document image so controls remain visible over opaque repeated art.
pub fn paint(ui: &mut Ui, s: &Session, state: &State, view: View) {
    if !view.valid() || selected(s).len() != 1 {
        return;
    }
    let Some(spec) = state.draft.clone().or_else(|| current(s)) else {
        return;
    };
    let Ok(b) = target_bounds(s) else {
        return;
    };
    for (handle, p) in handles(&spec, b) {
        let p = view.screen(p);
        let response =
            ui.interact(Rect::from_center_size(p, egui::vec2(16., 16.)), ui.id().with(("repeat handle", format!("{handle:?}"))), egui::Sense::hover());
        let label = match handle {
            Handle::Count => "Repeat count",
            Handle::Rows => "Repeat rows",
            Handle::Cols => "Repeat columns",
            Handle::SpacingX => "Horizontal repeat spacing",
            Handle::SpacingY => "Vertical repeat spacing",
            Handle::Center => "Repeat center",
            Handle::Angle => "Repeat angle",
        };
        response.on_hover_text(li_i18n::t(label));
        ui.painter().circle(p, 5., Color32::from_rgb(255, 190, 55), Stroke::new(1., Color32::BLACK));
    }
}

#[cfg(test)]
#[path = "repeat/app_gpu.rs"]
mod app_gpu;
#[cfg(test)]
#[path = "repeat/tests.rs"]
mod tests;
