//! Blob Brush / vector Eraser pointer capture, adapted from VectorCraft
//! crates/tools/src/draw2/gesture.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE} and integration/40.md.
use crate::canvas::{ToolEvent, ViewXform};
use photocraft_doc::Document;
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::{
        blob_eraser::{self as engine, Capture, Mode, Options, Prepared, Request, sweep},
        pencil::fitting::{Sample, Stabilization, Stabilizer},
    },
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub const BLOB_TOOL_ID: &str = "blobBrush";
pub const ERASER_TOOL_ID: &str = "vectorEraser";

#[derive(Clone, Debug)]
struct Gesture {
    capture: Capture,
    samples: Vec<sweep::Sample>,
    filter: Stabilizer,
    last: Sample,
    rectangle: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub mode: Mode,
    pub options: Options,
    pub rope_length: f64,
    #[serde(skip)]
    gesture: Option<Gesture>,
    #[serde(skip)]
    preview: Option<Prepared>,
    #[serde(skip)]
    generation: u64,
    #[serde(skip)]
    pub excluded: bool,
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
        self.excluded = false;
    }
    pub fn frame(&mut self, s: &Session, ctx: &egui::Context, selected: bool) {
        if !selected
            || !ctx.input(|i| i.focused)
            || self.gesture.as_ref().is_some_and(|g| g.capture.mode != self.mode || g.capture.validate(s).is_err())
            || (self.active() && crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
        {
            self.cancel();
        }
    }
    /// The app's normal CPU/GPU compositor presents this detached overlay document.
    pub fn display_doc(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        let p = self.preview.as_ref()?;
        p.capture().validate(s).ok()?;
        Some((p.document.clone(), self.generation | (1 << 60)))
    }
    pub fn pointer(&mut self, s: &mut Session, event: ToolEvent, mods: egui::Modifiers) -> Result<()> {
        let color = s.tools.foreground;
        self.pointer_with_color(s, event, mods, color)
    }
    /// Pass the persona's current stroke swatch; release uses the colour captured at press.
    pub fn pointer_with_color(&mut self, s: &mut Session, event: ToolEvent, mods: egui::Modifiers, stroke_color: [f32; 4]) -> Result<()> {
        let result = self.pointer_inner(s, event, mods, stroke_color);
        if result.is_err() {
            self.cancel();
        }
        result
    }
    fn pointer_inner(&mut self, s: &mut Session, event: ToolEvent, mods: egui::Modifiers, color: [f32; 4]) -> Result<()> {
        match event {
            ToolEvent::Down { x, y, pressure } => {
                self.cancel();
                let capture = Capture::begin_with_color(s, self.mode, self.options.clone(), color)?;
                let mut filter = Stabilizer::new(Stabilization::Rope { length: self.rope_length }).map_err(EngineError::Other)?;
                let raw = Sample { point: Point::new(x, y), pressure: f64::from(pressure) };
                let sample = filter.push(raw).map_err(EngineError::Other)?;
                self.excluded = self.mode == Mode::Eraser && capture.excluded;
                self.gesture = Some(Gesture {
                    capture,
                    samples: vec![sweep::Sample { point: sample.point, pressure: sample.pressure }],
                    filter,
                    last: raw,
                    rectangle: mods.alt && self.mode == Mode::Eraser,
                });
                self.update_preview(s, mods)?;
            }
            ToolEvent::Move { x, y, pressure } => {
                let Some(g) = &mut self.gesture else {
                    return Ok(());
                };
                g.capture.validate(s)?;
                let raw = Sample { point: Point::new(x, y), pressure: f64::from(pressure) };
                let sample = g.filter.push(raw).map_err(EngineError::Other)?;
                g.last = raw;
                push(&mut g.samples, sweep::Sample { point: sample.point, pressure: sample.pressure })?;
                self.update_preview(s, mods)?;
            }
            ToolEvent::Up { x, y } => {
                let Some(g) = &mut self.gesture else {
                    return Ok(());
                };
                g.capture.validate(s)?;
                let sample = g.filter.finish(Sample { point: Point::new(x, y), pressure: g.last.pressure }).map_err(EngineError::Other)?;
                g.last = sample;
                push(&mut g.samples, sweep::Sample { point: sample.point, pressure: sample.pressure })?;
                self.update_preview(s, mods)?;
                let p = self.preview.take().ok_or_else(|| EngineError::Other("missing area tool preview".into()))?;
                p.commit(s)?;
                self.cancel();
            }
        }
        Ok(())
    }
    fn update_preview(&mut self, s: &Session, mods: egui::Modifiers) -> Result<()> {
        let Some(g) = &mut self.gesture else {
            return Ok(());
        };
        g.rectangle = g.capture.mode == Mode::Eraser && mods.alt;
        let samples = if g.rectangle { vec![g.samples[0], sweep::Sample { point: g.last.point, pressure: g.last.pressure }] } else { g.samples.clone() };
        self.preview = Some(engine::prepare(s, &Request { capture: g.capture.clone(), samples, rectangle: g.rectangle })?);
        self.generation = self.generation.wrapping_add(1).max(1);
        Ok(())
    }
}
fn push(samples: &mut Vec<sweep::Sample>, sample: sweep::Sample) -> Result<()> {
    sample.validate().map_err(EngineError::Other)?;
    if samples.last() == Some(&sample) {
        return Ok(());
    }
    if samples.len() >= sweep::MAX_SAMPLES {
        return Err(EngineError::Other("area tool sample budget exceeded".into()));
    }
    samples.push(sample);
    Ok(())
}
pub fn canvas_input(state: &mut State, s: &mut Session, response: &egui::Response, xf: &ViewXform) -> Result<()> {
    let color = s.tools.foreground;
    canvas_input_with_pressure(state, s, response, xf, 1., color)
}
/// Pressure is the app's recovered tablet value (including synthetic test pressure).
pub fn canvas_input_with_pressure(state: &mut State, s: &mut Session, response: &egui::Response, xf: &ViewXform, pressure: f32, color: [f32; 4]) -> Result<()> {
    let ctx = &response.ctx;
    state.frame(s, ctx, true);
    let mods = ctx.input(|i| i.modifiers);
    let result = (|| {
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers } => {
                    let [x, y] = xf.to_doc(pos);
                    if pressed && response.rect.contains(pos) {
                        state.pointer_with_color(s, ToolEvent::Down { x, y, pressure }, modifiers, color)?;
                    } else if !pressed && state.active() {
                        state.pointer_with_color(s, ToolEvent::Up { x, y }, modifiers, color)?;
                    }
                }
                egui::Event::PointerMoved(pos) if state.active() => {
                    let [x, y] = xf.to_doc(pos);
                    state.pointer_with_color(s, ToolEvent::Move { x, y, pressure }, mods, color)?;
                }
                egui::Event::PointerGone | egui::Event::WindowFocused(false) => state.cancel(),
                _ => {}
            }
        }
        if state.gesture.as_ref().is_some_and(|g| g.rectangle != (g.capture.mode == Mode::Eraser && mods.alt)) {
            state.update_preview(s, mods)?;
        }
        Ok(())
    })();
    if result.is_err() {
        state.cancel();
    }
    result
}
pub fn context_bar(ui: &mut egui::Ui, state: &mut State) {
    use li_i18n::t;
    let before = (state.options.clone(), state.rope_length);
    let label = ui.label(t("Size"));
    ui.add(egui::DragValue::new(&mut state.options.tip.size).range(0.1..=1000.)).labelled_by(label.id);
    let label = ui.label(t("Angle"));
    ui.add(egui::DragValue::new(&mut state.options.tip.angle).range(-360.0..=360.0)).labelled_by(label.id);
    ui.add(egui::Slider::new(&mut state.options.tip.roundness, 0.01..=1.).text(t("Roundness")));
    ui.add(egui::Slider::new(&mut state.options.tip.pressure, 0.0..=1.).text(t("Pressure")));
    ui.add(egui::Slider::new(&mut state.options.fidelity, 0.0..=1.).text(t("Fidelity")));
    let label = ui.label(t("Tolerance"));
    ui.add(egui::DragValue::new(&mut state.options.tolerance).range(0.05..=100.)).labelled_by(label.id);
    let label = ui.label(t("Rope length"));
    ui.add(egui::DragValue::new(&mut state.rope_length).range(0.0..=1000.0)).labelled_by(label.id);
    match state.mode {
        Mode::Blob => {
            ui.checkbox(&mut state.options.keep_selected, t("Keep Selected"));
            ui.checkbox(&mut state.options.merge_only_selection, t("Merge Only with Selection"));
        }
        Mode::Eraser => {
            ui.label(t("Alt-drag to erase a rectangle"));
            ui.label(t("Text and symbols excluded")).on_hover_text(t(engine::EXCLUDED_TOOLTIP));
        }
    }
    if before != (state.options.clone(), state.rope_length) {
        state.cancel();
    }
}
/// Feedback is drawn in screen space; fill/erase pixels come from display_doc, through
/// the actual compositor. This overlay never mutates the session or its render caches.
pub fn paint_overlay(state: &State, painter: &egui::Painter, xf: &ViewXform, position: egui::Pos2) {
    if state.options.validate().is_err() {
        return;
    }
    let tip = state.options.tip;
    let a = -tip.angle.to_radians();
    let r = (tip.size * f64::from(xf.zoom) / 2.) as f32;
    let points = (0..=64)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 64.;
            let (x, y) = (t.cos(), tip.roundness * t.sin());
            position + egui::vec2((a.cos() * x - a.sin() * y) as f32 * r, (a.sin() * x + a.cos() * y) as f32 * r)
        })
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1., egui::Color32::from_rgb(80, 190, 240))));
}
#[cfg(test)]
#[path = "blob_eraser/tests.rs"]
mod tests;
