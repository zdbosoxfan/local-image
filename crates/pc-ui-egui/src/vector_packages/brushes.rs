//! Vector Brush input, presets and context controls. Uses the established pencil/tablet
//! ToolEvent contract; package 00 owns full app routing (integration/17.md).
use crate::canvas::{ToolEvent, ViewXform};
use photocraft_doc::Document;
use photocraft_engine::vector_packages::brushes::{self as engine, Capture, Options, Prepared};
use photocraft_engine::vector_packages::pencil::fitting::{Sample, Stabilization, Stabilizer};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub const TOOL_ID: &str = "vector-brush";
#[derive(Clone, Debug)]
struct Gesture {
    capture: Capture,
    samples: Vec<Sample>,
    filter: Stabilizer,
    last_pressure: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub options: Options,
    pub preset: usize,
    pub rope_length: f64,
    #[serde(skip)]
    gesture: Option<Gesture>,
    #[serde(skip)]
    preview: Option<Prepared>,
    #[serde(skip)]
    generation: u64,
}
impl Default for State {
    fn default() -> Self {
        Self { options: Options::default(), preset: 0, rope_length: 0., gesture: None, preview: None, generation: 0 }
    }
}
impl State {
    pub fn active(&self) -> bool {
        self.gesture.is_some()
    }
    pub fn preview(&self) -> Option<&Prepared> {
        self.preview.as_ref()
    }
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.preview = None;
    }
    pub fn frame(&mut self, session: &Session, ctx: &egui::Context, selected: bool) {
        if !selected
            || !ctx.input(|i| i.focused)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            || self.gesture.as_ref().is_some_and(|g| g.capture.validate(session).is_err())
        {
            self.cancel();
        }
    }
    pub fn display_doc(&self, session: &Session) -> Option<(Arc<Document>, u64)> {
        let p = self.preview.as_ref()?;
        p.capture.validate(session).ok()?;
        Some((p.document.clone(), self.generation | (1_u64 << 61)))
    }
    pub fn pointer(&mut self, session: &mut Session, event: ToolEvent) -> Result<()> {
        let result = self.pointer_inner(session, event);
        if result.is_err() {
            self.cancel();
        }
        result
    }
    fn pointer_inner(&mut self, session: &mut Session, event: ToolEvent) -> Result<()> {
        match event {
            ToolEvent::Down { x, y, pressure } => {
                self.cancel();
                let capture = Capture::begin(session, self.options.clone())?;
                let mut filter = Stabilizer::new(Stabilization::Rope { length: self.rope_length }).map_err(EngineError::Other)?;
                let sample = filter.push(Sample { point: Point::new(x, y), pressure: f64::from(pressure) }).map_err(EngineError::Other)?;
                self.gesture = Some(Gesture { capture, samples: vec![sample], filter, last_pressure: sample.pressure });
            }
            ToolEvent::Move { x, y, pressure } => {
                let Some(g) = &mut self.gesture else {
                    return Ok(());
                };
                g.capture.validate(session)?;
                let sample = g.filter.push(Sample { point: Point::new(x, y), pressure: f64::from(pressure) }).map_err(EngineError::Other)?;
                g.last_pressure = sample.pressure;
                push(&mut g.samples, sample)?;
                if g.samples.len() >= 2 {
                    self.preview = Some(engine::prepare(session, &g.capture, &g.samples)?);
                    self.generation = self.generation.wrapping_add(1);
                }
            }
            ToolEvent::Up { x, y } => {
                let Some(mut g) = self.gesture.take() else {
                    return Ok(());
                };
                self.preview = None;
                g.capture.validate(session)?;
                let sample = g.filter.finish(Sample { point: Point::new(x, y), pressure: g.last_pressure }).map_err(EngineError::Other)?;
                push(&mut g.samples, sample)?;
                if g.samples.len() >= 2 {
                    engine::prepare(session, &g.capture, &g.samples)?.commit(session)?;
                }
            }
        }
        Ok(())
    }
}
fn push(samples: &mut Vec<Sample>, sample: Sample) -> Result<()> {
    if let Some(last) = samples.last_mut()
        && last.point == sample.point
    {
        *last = sample;
        return Ok(());
    }
    if samples.len() >= engine::geometry::MAX_INPUT {
        return Err(EngineError::Other("brush sample budget exceeded".into()));
    }
    samples.push(sample);
    Ok(())
}
pub fn canvas_input(state: &mut State, session: &mut Session, response: &egui::Response, xf: &ViewXform) -> Result<()> {
    canvas_input_with_pressure(state, session, response, xf, 1.)
}
pub fn canvas_input_with_pressure(state: &mut State, session: &mut Session, response: &egui::Response, xf: &ViewXform, pressure: f32) -> Result<()> {
    state.frame(session, &response.ctx, true);
    let events = response.ctx.input(|i| i.events.clone());
    for event in events {
        match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, .. } => {
                let [x, y] = xf.to_doc(pos);
                if pressed && response.rect.contains(pos) {
                    state.pointer(session, ToolEvent::Down { x, y, pressure })?;
                } else if !pressed && state.active() {
                    state.pointer(session, ToolEvent::Up { x, y })?;
                }
            }
            egui::Event::PointerMoved(pos) if state.active() => {
                let [x, y] = xf.to_doc(pos);
                state.pointer(session, ToolEvent::Move { x, y, pressure })?;
            }
            egui::Event::WindowFocused(false) => state.cancel(),
            _ => {}
        }
    }
    Ok(())
}
pub fn context_bar(ui: &mut egui::Ui, state: &mut State) {
    use crate::i18n::t as tr;
    let before = (state.options.clone(), state.rope_length);
    for (i, (name, brush)) in engine::geometry::presets().into_iter().enumerate() {
        if ui.selectable_label(state.preset == i, tr(name)).clicked() {
            state.preset = i;
            state.options.brush = brush;
        }
    }
    let label = ui.label(tr("Brush width"));
    ui.add(egui::DragValue::new(&mut state.options.width).range(0.1..=1000.)).labelled_by(label.id);
    ui.checkbox(&mut state.options.pressure, tr("Pressure"));
    let label = ui.label(tr("Brush tolerance"));
    ui.add(egui::DragValue::new(&mut state.options.tolerance).range(0.05..=100.)).labelled_by(label.id);
    let label = ui.label(tr("Rope length"));
    ui.add(egui::DragValue::new(&mut state.rope_length).range(0.0..=1000.)).labelled_by(label.id);
    match &mut state.options.brush {
        engine::geometry::Brush::Calligraphic(c) => {
            let label = ui.label(tr("Nib angle"));
            ui.add(egui::DragValue::new(&mut c.angle).range(-360.0..=360.0)).labelled_by(label.id);
            ui.add(egui::Slider::new(&mut c.roundness, 0.0..=1.0).text(tr("Roundness")));
        }
        engine::geometry::Brush::Scatter(_) => {
            let label = ui.label(tr("Scatter seed"));
            ui.add(egui::DragValue::new(&mut state.options.seed)).labelled_by(label.id);
        }
        _ => {}
    }
    if before != (state.options.clone(), state.rope_length) {
        state.cancel();
    }
}
#[cfg(test)]
#[path = "brushes/tests.rs"]
mod tests;
