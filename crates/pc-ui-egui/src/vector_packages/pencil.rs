//! Vector Pencil / Sculpt input and context controls. Distinct id: vector-pencil.
//! Gesture and continuation adapted from VectorCraft tools/src/draw2/gesture.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Notices: licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Caller owns persona routing; deactivate on tool/document/persona change or focus loss.

use crate::canvas::{ToolEvent, ViewXform};
use engine::fitting::{self, Sample, Stabilizer};
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::vector_packages::pencil::{self as engine, Capture, CurvePosition, Operation, Options, Prepared, StabilizerKind};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const TOOL_ID: &str = "vector-pencil";
#[derive(Clone, Debug)]
struct Gesture {
    capture: Capture,
    filter: Stabilizer,
    samples: Vec<Sample>,
    last_raw: Sample,
    sculpt_start: Option<(LayerId, usize, CurvePosition)>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub options: Options,
    #[serde(skip)]
    gesture: Option<Gesture>,
    #[serde(skip)]
    preview: Option<Prepared>,
    #[serde(skip)]
    generation: u64,
    /// Stabilized pressure from the last committed stroke, for package 11 to consume.
    #[serde(skip)]
    pub last_pressure: Vec<Sample>,
}
impl State {
    pub fn frame(&mut self, session: &Session, ctx: &egui::Context, selected: bool) {
        let stale = self.gesture.as_ref().is_some_and(|g| g.capture.validate(session).is_err());
        if !selected || stale || !ctx.input(|i| i.focused) || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.cancel();
        }
    }
    pub fn active(&self) -> bool {
        self.gesture.is_some()
    }
    pub fn preview_generation(&self) -> u64 {
        self.generation
    }
    pub fn preview(&self) -> Option<&Prepared> {
        self.preview.as_ref()
    }
    pub fn cancel(&mut self) {
        if let Some(g) = self.gesture.as_mut() {
            g.filter.cancel();
        }
        self.gesture = None;
        self.preview = None;
    }
    /// Detached document through the real canvas compositor; never an egui stroke impostor.
    pub fn display_doc(&self, session: &Session) -> Option<(Arc<Document>, u64)> {
        let p = self.preview.as_ref()?;
        p.capture.validate(session).ok()?;
        Some((p.document.clone(), self.generation | (1_u64 << 62)))
    }
    pub fn pointer(&mut self, session: &mut Session, event: ToolEvent, modifiers: egui::Modifiers, zoom: f32) -> Result<()> {
        let result = self.pointer_inner(session, event, modifiers, zoom);
        if result.is_err() {
            self.cancel();
        }
        result
    }
    fn pointer_inner(&mut self, session: &mut Session, event: ToolEvent, mods: egui::Modifiers, zoom: f32) -> Result<()> {
        if !zoom.is_finite() || !(0.01..=10000.0).contains(&zoom) {
            return Err(EngineError::Other("invalid pencil view scale".into()));
        }
        if let Some(g) = &self.gesture {
            g.capture.validate(session)?;
            if g.capture.options != self.options {
                self.cancel();
                return Ok(());
            }
        }
        let point = match event {
            ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => Point::new(x, y),
        };
        match event {
            ToolEvent::Down { pressure, .. } => {
                self.cancel();
                self.options.validate()?;
                let sample = Sample { point, pressure: f64::from(pressure) };
                sample.validate().map_err(EngineError::Other)?;
                let within = self.options.edit_within / f64::from(zoom);
                let st = session.active().ok_or(EngineError::NoDocument)?;
                let mut sculpt_start = None;
                let operation = if self.options.sculpt {
                    let (id, si, ki) = hit_curve(session, point, within, None).ok_or_else(|| EngineError::Other("start Sculpt on a selected curve".into()))?;
                    sculpt_start = Some((id, si, ki));
                    Operation::SculptRange { layer: id, subpath: si, from: ki, to: ki }
                } else if mods.alt {
                    find_endpoint(session, point, within).unwrap_or(Operation::Create { above: st.active_layer })
                } else {
                    Operation::Create { above: st.active_layer }
                };
                let capture = Capture::begin(session, operation, self.options.clone())?;
                let mut filter = Stabilizer::new(self.options.stabilization()).map_err(EngineError::Other)?;
                let sample = filter.push(sample).map_err(EngineError::Other)?;
                self.gesture = Some(Gesture { capture, filter, samples: vec![sample], last_raw: sample, sculpt_start });
            }
            ToolEvent::Move { pressure, .. } => {
                let Some(g) = self.gesture.as_mut() else {
                    return Ok(());
                };
                let raw = Sample { point, pressure: f64::from(pressure) };
                let sample = g.filter.push(raw).map_err(EngineError::Other)?;
                g.last_raw = raw;
                push_sample(&mut g.samples, sample)?;
                self.update_preview(session, mods, zoom, false)?;
            }
            ToolEvent::Up { .. } => {
                let Some(g) = self.gesture.as_mut() else {
                    return Ok(());
                };
                let sample = g.filter.finish(Sample { point, ..g.last_raw }).map_err(EngineError::Other)?;
                g.last_raw = sample;
                push_sample(&mut g.samples, sample)?;
                if g.samples.len() < 2 {
                    self.cancel();
                    return Ok(());
                }
                self.update_preview(session, mods, zoom, true)?;
                let preview = self.preview.take().ok_or_else(|| EngineError::Other("Sculpt must finish at a different point on the selected curve".into()))?;
                let pressure = preview.pressure.clone();
                preview.commit(session)?;
                self.last_pressure = pressure;
                self.cancel();
            }
        }
        Ok(())
    }
    fn update_preview(&mut self, session: &Session, mods: egui::Modifiers, zoom: f32, release: bool) -> Result<()> {
        let Some(g) = self.gesture.as_mut() else {
            return Ok(());
        };
        if g.samples.len() < 2 {
            return Ok(());
        }
        if let Some((id, si, first)) = g.sculpt_start {
            let within = self.options.edit_within / f64::from(zoom);
            let last = hit_curve(session, g.last_raw.point, within, Some((id, si))).map(|(_, _, i)| i);
            if let Some(last) = last.filter(|&i| i != first) {
                let LayerContent::Shape(sh) = &session.active().ok_or(EngineError::NoDocument)?.doc.layer(id).ok_or(EngineError::NoLayer(id))?.content else {
                    return Err(EngineError::Other("Sculpt target is missing".into()));
                };
                let (a, b) =
                    if !sh.path.subpaths[si].closed && first.segment as f64 + first.t > last.segment as f64 + last.t { (last, first) } else { (first, last) };
                g.capture.operation = Operation::SculptRange { layer: id, subpath: si, from: a, to: b };
                // Dragging backwards over an open interval reverses the samples, not the base path.
                if a != first {
                    g.samples.reverse();
                    let preview = engine::prepare(session, &g.capture, &g.samples, false);
                    g.samples.reverse();
                    self.preview = Some(preview?);
                } else {
                    self.preview = Some(engine::prepare(session, &g.capture, &g.samples, false)?);
                }
            } else {
                self.preview = None;
                if release {
                    return Err(EngineError::Other("Sculpt must finish at a different point on the selected curve".into()));
                }
            }
        } else {
            let close_within = self.options.close_within / f64::from(zoom);
            let near = g.samples.len() > 2 && fitting::distance(g.samples[0].point, g.last_raw.point) <= close_within;
            let closed = matches!(g.capture.operation, Operation::Create { .. }) && (mods.alt || near);
            let mut capture = g.capture.clone();
            capture.options.close_within = close_within;
            self.preview = Some(engine::prepare(session, &capture, &g.samples, closed && g.samples.len() > 2)?);
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        Ok(())
    }
}
fn push_sample(samples: &mut Vec<Sample>, sample: Sample) -> Result<()> {
    if let Some(last) = samples.last_mut()
        && fitting::distance(last.point, sample.point) < 1e-9
    {
        *last = sample;
        return Ok(());
    }
    if samples.len() >= fitting::MAX_SAMPLES {
        return Err(EngineError::Other("pencil sample budget exceeded".into()));
    }
    samples.push(sample);
    Ok(())
}
fn find_endpoint(session: &Session, p: Point, tol: f64) -> Option<Operation> {
    let st = session.active()?;
    let mut best = None;
    let mut distance = tol;
    for id in st.selected_layers() {
        if photocraft_engine::vector_api::snapshot_shapes(&st.doc, &[id]).is_err() {
            continue;
        }
        let LayerContent::Shape(sh) = &st.doc.layer(id)?.content else {
            continue;
        };
        for (si, sp) in sh.path.subpaths.iter().enumerate().filter(|(_, s)| !s.closed && s.knots.len() >= 2) {
            for (at_start, k) in [(true, &sp.knots[0]), (false, &sp.knots[sp.knots.len() - 1])] {
                let d = fitting::distance(p, k.anchor);
                if d <= distance {
                    distance = d;
                    best = Some(Operation::Continue { layer: id, subpath: si, at_start });
                }
            }
        }
    }
    best
}
fn hit_curve(session: &Session, p: Point, tol: f64, target: Option<(LayerId, usize)>) -> Option<(LayerId, usize, CurvePosition)> {
    use photocraft_vector::edit::{self, Hit};
    let st = session.active()?;
    for id in st.selected_layers().into_iter().rev() {
        if target.is_some_and(|(wanted, _)| wanted != id) || photocraft_engine::vector_api::snapshot_shapes(&st.doc, &[id]).is_err() {
            continue;
        }
        let LayerContent::Shape(sh) = &st.doc.layer(id)?.content else {
            continue;
        };
        let hit = if let Some((_, si)) = target {
            let part = photocraft_doc::Path::new(vec![sh.path.subpaths.get(si)?.clone()]);
            edit::hit(&part, p, tol, &[]).map(|h| match h {
                Hit::Anchor([_, ki]) => Hit::Anchor([si, ki]),
                Hit::Segment([_, ki], t) => Hit::Segment([si, ki], t),
                other => other,
            })
        } else {
            edit::hit(&sh.path, p, tol, &[])
        };
        match hit {
            Some(Hit::Anchor([si, ki])) => {
                let sp = &sh.path.subpaths[si];
                if sp.knots.len() < 2 {
                    continue;
                }
                let pos =
                    if !sp.closed && ki + 1 == sp.knots.len() { CurvePosition { segment: ki - 1, t: 1.0 } } else { CurvePosition { segment: ki, t: 0.0 } };
                return Some((id, si, pos));
            }
            Some(Hit::Segment([si, segment], t)) => return Some((id, si, CurvePosition { segment, t })),
            _ => {}
        }
    }
    None
}
/// Small-host input adapter; full app routes its existing ToolEvent stream to State::pointer.
/// Release outside the canvas is captured; Escape/focus loss discard stabilizer and caches.
pub fn canvas_input(state: &mut State, session: &mut Session, response: &egui::Response, xf: &ViewXform) -> Result<()> {
    canvas_input_with_pressure(state, session, response, xf, 1.0)
}
/// App adapter receives the pressure already recovered by its existing stylus path.
pub fn canvas_input_with_pressure(state: &mut State, session: &mut Session, response: &egui::Response, xf: &ViewXform, pressure: f32) -> Result<()> {
    let ctx = &response.ctx;
    if !ctx.input(|i| i.focused) || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        state.cancel();
        return Ok(());
    }
    let events = ctx.input(|i| i.events.clone());
    let mods = ctx.input(|i| i.modifiers);
    for event in events {
        match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers } => {
                let [x, y] = xf.to_doc(pos);
                if pressed && response.rect.contains(pos) {
                    state.pointer(session, ToolEvent::Down { x, y, pressure }, modifiers, xf.zoom)?;
                } else if !pressed && state.active() {
                    state.pointer(session, ToolEvent::Up { x, y }, modifiers, xf.zoom)?;
                }
            }
            egui::Event::PointerMoved(pos) if state.active() => {
                let [x, y] = xf.to_doc(pos);
                state.pointer(session, ToolEvent::Move { x, y, pressure }, mods, xf.zoom)?;
            }
            egui::Event::WindowFocused(false) => state.cancel(),
            _ => {}
        }
    }
    Ok(())
}
/// Context bar; every visible string follows the app's existing localization path.
pub fn context_bar(ui: &mut egui::Ui, state: &mut State) {
    use li_i18n::t as tr;
    let before = state.options.clone();
    ui.checkbox(&mut state.options.sculpt, tr("Sculpt"));
    ui.add(egui::Slider::new(&mut state.options.smoothness, 0.0..=1.0).text(tr("Smoothness")));
    let label = ui.label(tr("Tolerance"));
    ui.add(egui::DragValue::new(&mut state.options.tolerance).range(0.05..=100.0)).labelled_by(label.id);
    for (kind, label) in [(StabilizerKind::None, "No stabilizer"), (StabilizerKind::Rope, "Rope"), (StabilizerKind::Window, "Window")] {
        ui.selectable_value(&mut state.options.stabilizer, kind, tr(label));
    }
    match state.options.stabilizer {
        StabilizerKind::Rope => {
            ui.add(egui::DragValue::new(&mut state.options.rope_length).range(0.0..=1000.0).prefix(tr("Rope length: ")));
        }
        StabilizerKind::Window => {
            ui.add(egui::DragValue::new(&mut state.options.window_samples).range(1..=64).prefix(tr("Window samples: ")));
        }
        _ => {}
    }
    ui.checkbox(&mut state.options.fill, tr("Fill"));
    ui.add(egui::DragValue::new(&mut state.options.stroke_width).range(0.1..=1000.0).prefix(tr("Stroke width: ")));
    if state.options != before {
        state.cancel();
    }
}
// pencil/gpu_tests.rs is the package-05 scratch-registration harness, whose
// temporary app fields are not production APIs. Package 00 migrates that GPU
// acceptance host to the shared runtime in integration/47.md.
#[cfg(test)]
#[path = "pencil/tests.rs"]
mod tests;
