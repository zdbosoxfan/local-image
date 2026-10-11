//! Vector distort tool controls and captured egui gestures. Raster UI files are untouched.
//! Interaction adapted from VectorCraft tools/distort/{liquify,puppet}.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use engine::{
    liquify::{self, Kind, Sample},
    puppet::{self, Pin},
};
use photocraft_doc::{Document, LayerId};
use photocraft_engine::{EngineError, Result, Session, vector_packages::distort_tools as engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    #[default]
    Warp,
    Twirl,
    Pucker,
    Bloat,
    Scallop,
    Crystallize,
    Wrinkle,
    PuppetWarp,
}
impl Tool {
    pub const ALL: [Self; 8] = [Self::Warp, Self::Twirl, Self::Pucker, Self::Bloat, Self::Scallop, Self::Crystallize, Self::Wrinkle, Self::PuppetWarp];
    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::Warp => Some(Kind::Warp),
            Self::Twirl => Some(Kind::Twirl),
            Self::Pucker => Some(Kind::Pucker),
            Self::Bloat => Some(Kind::Bloat),
            Self::Scallop => Some(Kind::Scallop),
            Self::Crystallize => Some(Kind::Crystallize),
            Self::Wrinkle => Some(Kind::Wrinkle),
            Self::PuppetWarp => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Warp => "Warp",
            Self::Twirl => "Twirl",
            Self::Pucker => "Pucker",
            Self::Bloat => "Bloat",
            Self::Scallop => "Scallop",
            Self::Crystallize => "Crystallize",
            Self::Wrinkle => "Wrinkle",
            Self::PuppetWarp => "Puppet Warp",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Warp => "vectorWarp",
            Self::Twirl => "vectorTwirl",
            Self::Pucker => "vectorPucker",
            Self::Bloat => "vectorBloat",
            Self::Scallop => "vectorScallop",
            Self::Crystallize => "vectorCrystallize",
            Self::Wrinkle => "vectorWrinkle",
            Self::PuppetWarp => "vectorPuppetWarp",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub tool: Tool,
    pub brush: liquify::Options,
    pub puppet: puppet::Options,
    pub show_mesh: bool,
    pub show_brush: bool,
    pub seed: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self { tool: Tool::Warp, brush: Default::default(), puppet: Default::default(), show_mesh: true, show_brush: true, seed: 0 }
    }
}
#[derive(Clone, Debug)]
struct Sizing {
    at: [f64; 2],
    width: f64,
    height: f64,
}
#[derive(Clone, Debug)]
struct PinDrag {
    at: [f64; 2],
    pins: Vec<Pin>,
    selected: Vec<usize>,
    turn: Option<(usize, f64, f64)>,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub options: Options,
    pub preview: Option<engine::Prepared>,
    pub pins: Vec<Pin>,
    pub selected_pins: Vec<usize>,
    pub hover: Option<[f64; 2]>,
    pub error: Option<String>,
    pub conversion: Option<engine::Capture>,
    brush: Option<engine::LiquifyGesture>,
    rig: Option<engine::PuppetGesture>,
    sizing: Option<Sizing>,
    drag: Option<PinDrag>,
    held: f64,
    last: Option<Sample>,
    generation: u64,
    owner: Option<Tool>,
}
fn err(e: impl ToString) -> EngineError {
    EngineError::Other(e.to_string())
}
pub type Execute = fn(&mut Session, &str, Value) -> Result<Value>;
fn execute(s: &mut Session, id: &str, v: Value) -> Result<Value> {
    s.execute(id, v)
}
impl State {
    pub fn captured(&self) -> bool {
        self.brush.is_some() || self.sizing.is_some() || self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        if let Some(d) = self.drag.take() {
            self.pins = d.pins;
        }
        self.preview = None;
        self.brush = None;
        if let Some(z) = self.sizing.take() {
            self.options.brush.width = z.width;
            self.options.brush.height = z.height;
        }
        self.drag = None;
        self.last = None;
        self.held = 0.;
    }
    pub fn deactivate(&mut self) {
        self.cancel();
        self.rig = None;
        self.pins.clear();
        self.selected_pins.clear();
        self.conversion = None;
        self.hover = None;
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        let stale =
            self.brush.as_ref().is_some_and(|g| g.request.capture.validate(s).is_err()) || self.rig.as_ref().is_some_and(|g| g.capture.validate(s).is_err());
        let selection_changed = self.rig.as_ref().is_some_and(|g| engine::target_layers(s).map_or(true, |ids| ids != g.capture.layers));
        if !enabled || stale || selection_changed || self.owner.is_some_and(|t| t != self.options.tool) {
            self.deactivate();
        }
        self.owner = Some(self.options.tool);
        if self.conversion.as_ref().is_some_and(|c| s.active().is_none_or(|st| st.doc.id.0 != c.document || st.revision != c.revision)) {
            self.conversion = None;
        }
    }
    pub fn display_doc(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        let p = self.preview.as_ref()?;
        p.capture.validate(s).ok()?;
        Some((p.document.clone(), self.generation))
    }
    fn show(&mut self, p: engine::Prepared) {
        self.preview = Some(p);
        self.generation = self.generation.wrapping_add(1);
    }
    fn attempt<T>(&mut self, r: Result<T>) -> Result<T> {
        if let Err(e) = &r {
            self.cancel();
            self.error = Some(e.to_string());
        }
        r
    }
    fn targets(&mut self, s: &Session) -> Result<Vec<LayerId>> {
        match engine::target_layers(s) {
            Ok(ids) => Ok(ids),
            Err(e) => {
                if e.to_string().contains("Convert") {
                    self.request_conversion(s);
                }
                Err(e)
            }
        }
    }
    fn request_conversion(&mut self, s: &Session) {
        let layers = engine::conversion_targets(s, self.hover.unwrap_or([0., 0.]), self.options.brush);
        self.request_conversion_layers(s, layers);
    }
    fn request_conversion_layers(&mut self, s: &Session, layers: Vec<LayerId>) {
        if let Some(st) = s.active() {
            self.conversion = Some(engine::Capture { document: st.doc.id.0, revision: st.revision, layers, automatic: false });
        }
    }
    fn ensure_rig(&mut self, s: &Session) -> Result<()> {
        if self.rig.is_none() {
            if s.active().is_none_or(|st| st.selected_layers().is_empty()) {
                return Err(err("Select vector paths for Puppet Warp"));
            }
            let ids = self.targets(s)?;
            let r = engine::PuppetGesture::begin(s, ids, self.options.puppet);
            if r.as_ref().is_err_and(|e| e.to_string().contains("Convert")) {
                self.request_conversion(s);
            }
            let rig = r?;
            self.pins = rig.rig.automatic_pins();
            self.selected_pins.clear();
            self.rig = Some(rig);
        }
        Ok(())
    }
    pub fn down(&mut self, s: &Session, p: [f64; 2], mods: Modifiers, pressure: f64, tolerance: f64) -> Result<()> {
        self.synchronize(s, true);
        if self.captured() {
            return Ok(());
        }
        liquify::valid_point(p).map_err(err)?;
        self.hover = Some(p);
        self.error = None;
        let r = self.down_inner(s, p, mods, pressure, tolerance);
        self.attempt(r)
    }
    fn down_inner(&mut self, s: &Session, p: [f64; 2], mods: Modifiers, pressure: f64, tolerance: f64) -> Result<()> {
        if let Some(kind) = self.options.tool.kind() {
            self.options.brush.kind = kind;
            self.options.brush.validate().map_err(err)?;
            if mods.alt {
                self.sizing = Some(Sizing { at: p, width: self.options.brush.width, height: self.options.brush.height });
                return Ok(());
            }
            let ids = self.targets(s)?;
            let r = engine::LiquifyGesture::begin(s, ids, self.options.brush, self.options.seed);
            if r.as_ref().is_err_and(|e| e.to_string().contains("Convert")) {
                self.request_conversion(s);
            }
            let mut g = r?;
            let sample = Sample { at: p, pressure };
            let result = g.push(s, sample);
            if result.as_ref().is_err_and(|e| e.to_string().contains("Convert")) {
                self.request_conversion_layers(s, g.conversion_layers.clone());
            }
            self.show(result?);
            self.last = Some(sample);
            self.brush = Some(g);
            self.held = 0.;
        } else {
            self.ensure_rig(s)?;
            if !tolerance.is_finite() || !(0.01..=1000.).contains(&tolerance) {
                return Err(err("invalid pin hit tolerance"));
            }
            let hit = self
                .pins
                .iter()
                .enumerate()
                .filter_map(|(i, q)| {
                    let d = distance(p, q.at);
                    (d <= tolerance).then_some((i, d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            if let Some(i) = hit {
                if mods.shift {
                    if self.selected_pins.contains(&i) {
                        self.selected_pins.retain(|j| *j != i);
                    } else {
                        self.selected_pins.push(i);
                    }
                } else if !self.selected_pins.contains(&i) {
                    self.selected_pins = vec![i];
                }
                self.drag = Some(PinDrag { at: p, pins: self.pins.clone(), selected: self.selected_pins.clone(), turn: None });
            } else if mods.alt && !self.selected_pins.is_empty() {
                let i = self
                    .selected_pins
                    .iter()
                    .copied()
                    .min_by(|a, b| distance(p, self.pins[*a].at).total_cmp(&distance(p, self.pins[*b].at)))
                    .ok_or_else(|| err("select a pin to rotate"))?;
                if distance(p, self.pins[i].at) > tolerance * 5. {
                    return Err(err("Drag near a selected pin to rotate"));
                }
                let a = angle(p, self.pins[i].at);
                self.drag = Some(PinDrag { at: p, pins: self.pins.clone(), selected: vec![i], turn: Some((i, a, self.pins[i].angle.unwrap_or(0.))) });
            } else {
                let g = self.rig.as_ref().ok_or_else(|| err("missing puppet mesh"))?;
                let deformed = g.rig.deform(&self.pins).map_err(err)?;
                let rest = g.rig.mesh.unmap(&deformed, kurbo_point(p)).ok_or_else(|| err("Pins go on the artwork"))?;
                if self.pins.len() >= 64 {
                    return Err(err("Puppet Warp exceeds pin budget"));
                }
                self.pins.push(Pin { rest: [rest.x, rest.y], at: p, angle: None });
                self.selected_pins = vec![self.pins.len() - 1];
            }
        }
        Ok(())
    }
    pub fn motion(&mut self, s: &Session, p: [f64; 2], mods: Modifiers, pressure: f64) -> Result<()> {
        self.synchronize(s, true);
        self.hover = Some(p);
        let r = self.motion_inner(s, p, mods, pressure);
        if r.as_ref().is_err_and(|e| e.to_string().contains("Convert")) {
            if let Some(g) = &self.brush {
                self.request_conversion_layers(s, g.conversion_layers.clone());
            } else {
                self.request_conversion(s);
            }
        }
        self.attempt(r)
    }
    fn motion_inner(&mut self, s: &Session, p: [f64; 2], mods: Modifiers, pressure: f64) -> Result<()> {
        liquify::valid_point(p).map_err(err)?;
        if let Some(z) = &self.sizing {
            let (mut w, mut h) = (z.width + 2. * (p[0] - z.at[0]), z.height + 2. * (p[1] - z.at[1]));
            if mods.shift {
                if (p[0] - z.at[0]).abs() >= (p[1] - z.at[1]).abs() {
                    h = z.height * w / z.width;
                } else {
                    w = z.width * h / z.height;
                }
            }
            self.options.brush.width = w.clamp(0.5, 10_000.);
            self.options.brush.height = h.clamp(0.5, 10_000.);
            self.hover = Some(z.at);
        } else if let Some(g) = &mut self.brush {
            if self.last.is_some_and(|q| q.at == p && q.pressure == pressure) {
                return Ok(());
            }
            let sample = Sample { at: p, pressure };
            let result = g.push(s, sample);
            let preview = result?;
            self.show(preview);
            self.last = Some(sample);
            self.held = 0.;
        } else if let Some(d) = &self.drag {
            let mut pins = d.pins.clone();
            if let Some((i, a, base)) = d.turn {
                pins[i].angle = Some(base + (angle(p, pins[i].at) - a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI);
            } else {
                let delta = [p[0] - d.at[0], p[1] - d.at[1]];
                for &i in &d.selected {
                    pins[i].at = [d.pins[i].at[0] + delta[0], d.pins[i].at[1] + delta[1]];
                }
            }
            let g = self.rig.as_ref().ok_or_else(|| err("missing puppet mesh"))?;
            let preview = g.preview(s, &pins)?;
            self.pins = pins;
            self.show(preview);
        }
        Ok(())
    }
    pub fn up_with(&mut self, s: &mut Session, run: Execute) -> Result<()> {
        self.synchronize(s, true);
        let old_pins = self.drag.as_ref().map(|d| d.pins.clone());
        let r = self.up_inner(s, run);
        if r.is_err()
            && self.rig.as_ref().is_some_and(|g| g.capture.validate(s).is_ok())
            && let Some(pins) = old_pins
        {
            self.pins = pins;
        }
        self.attempt(r)
    }
    fn up_inner(&mut self, s: &mut Session, run: Execute) -> Result<()> {
        if self.sizing.take().is_some() {
            return Ok(());
        }
        if let Some(g) = self.brush.take() {
            if self.preview.as_ref().is_some_and(|p| !p.edits.is_empty()) {
                g.request.capture.validate(s)?;
                run(s, "object.liquify", serde_json::to_value(&g.request).map_err(err)?)?;
            }
            self.cancel();
        } else if self.drag.take().is_some() {
            let changed = self.preview.as_ref().is_some_and(|p| !p.edits.is_empty());
            if changed {
                let g = self.rig.as_ref().ok_or_else(|| err("missing puppet mesh"))?;
                g.capture.validate(s)?;
                run(
                    s,
                    "object.puppetWarp",
                    serde_json::to_value(engine::PuppetRequest { capture: g.capture.clone(), options: g.rig.options, pins: self.pins.clone() }).map_err(err)?,
                )?;
                // Destructive geometry is now the rest pose for the next fresh drag.
                for q in &mut self.pins {
                    q.rest = q.at;
                    q.angle = None;
                }
                let ids = g.capture.layers.clone();
                self.rig = None;
                self.preview = None;
                self.rig = Some(engine::PuppetGesture::begin(s, ids, self.options.puppet)?);
            }
            self.preview = None;
        }
        Ok(())
    }
    pub fn tick(&mut self, s: &Session, dt: f64) -> Result<()> {
        if !dt.is_finite() || !(0.0..=1.0).contains(&dt) {
            return Err(err("invalid distort frame time"));
        }
        if self.brush.is_none() || !self.options.brush.kind.holds() {
            return Ok(());
        }
        self.held += dt;
        if self.held >= 0.1 {
            self.held -= 0.1;
            if let (Some(g), Some(sample)) = (&mut self.brush, self.last) {
                let p = g.push(s, sample)?;
                self.show(p);
            }
        }
        Ok(())
    }
    pub fn delete_pins(&mut self) {
        if self.drag.is_none() {
            let remove = self.selected_pins.clone();
            self.pins = self.pins.iter().enumerate().filter(|(i, _)| !remove.contains(i)).map(|(_, p)| *p).collect();
            self.selected_pins.clear();
        }
    }
}
fn kurbo_point(p: [f64; 2]) -> engine::puppet::arap::Point {
    engine::puppet::arap::Point::new(p[0], p[1])
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn angle(a: [f64; 2], c: [f64; 2]) -> f64 {
    (a[1] - c[1]).atan2(a[0] - c[0])
}
/// A test host can dispatch the supplied engine CommandSpecs before 00 installs the registry
/// slot. The production entrypoint below always calls Session::execute.
pub fn input_with(
    state: &mut State,
    s: &mut Session,
    ctx: &egui::Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> [f64; 2],
    enabled: bool,
    pressure: f64,
    tolerance: f64,
    run: Execute,
) -> Result<()> {
    state.synchronize(s, enabled);
    if !enabled {
        return Ok(());
    }
    let (events, mut mods, focused, dt) = ctx.input(|i| (i.events.clone(), i.modifiers, i.focused, f64::from(i.stable_dt)));
    if !focused || events.iter().any(|e| matches!(e, Event::WindowFocused(false))) {
        state.deactivate();
        return Ok(());
    }
    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()).filter(|p| canvas.contains(*p)) {
        state.hover = Some(to_doc(pos));
    }
    if state.options.tool == Tool::PuppetWarp {
        let r = state.ensure_rig(s);
        if let Err(e) = r {
            state.error = Some(e.to_string());
        }
    }
    let mut moved = false;
    for event in events {
        match event {
            Event::ModifiersChanged(m) => mods = m,
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers } => {
                mods = modifiers;
                if pressed && canvas.contains(pos) {
                    state.down(s, to_doc(pos), mods, pressure, tolerance)?;
                    moved = true;
                } else if !pressed && state.captured() {
                    state.motion(s, to_doc(pos), mods, pressure)?;
                    state.up_with(s, run)?;
                    moved = true;
                }
            }
            Event::PointerMoved(p) if canvas.contains(p) || state.captured() => {
                state.motion(s, to_doc(p), mods, pressure)?;
                moved = true;
            }
            Event::PointerGone => state.cancel(),
            Event::Key { key: Key::Escape, pressed: true, .. } => {
                state.cancel();
                state.conversion = None;
            }
            Event::Key { key: Key::Delete | Key::Backspace, pressed: true, .. } if !ctx.egui_wants_keyboard_input() => state.delete_pins(),
            Event::Key { key: Key::R, pressed: true, modifiers, .. }
                if modifiers.shift && !modifiers.command && !modifiers.ctrl && !modifiers.alt && !ctx.egui_wants_keyboard_input() =>
            {
                state.deactivate();
                state.options.tool = Tool::Warp;
            }
            _ => {}
        }
    }
    if !moved {
        let r = state.tick(s, dt);
        state.attempt(r)?;
    }
    if state.brush.is_some() {
        ctx.request_repaint();
    }
    Ok(())
}
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &egui::Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> [f64; 2],
    enabled: bool,
    pressure: f64,
    tolerance: f64,
) -> Result<()> {
    input_with(state, s, ctx, canvas, to_doc, enabled, pressure, tolerance, execute)
}
pub fn context(ui: &mut egui::Ui, state: &mut State) {
    let old = state.options.tool;
    ui.horizontal_wrapped(|ui| {
        for t in Tool::ALL {
            ui.selectable_value(&mut state.options.tool, t, tl!(t.label()));
        }
    });
    if old != state.options.tool {
        state.deactivate();
    }
    if state.options.tool == Tool::PuppetWarp {
        let old = state.options.puppet;
        ui.checkbox(&mut state.options.show_mesh, tl!("Show mesh"));
        ui.add(egui::Slider::new(&mut state.options.puppet.expand, 0.0..=100.).text(tl!("Expand mesh")));
        if ui.button(tl!("Select all pins")).clicked() {
            state.selected_pins = (0..state.pins.len()).collect();
        }
        if ui.button(tl!("Delete pins")).clicked() {
            state.delete_pins();
        }
        if old != state.options.puppet {
            state.deactivate();
        }
    } else {
        let old = state.options.brush;
        let p = &mut state.options.brush;
        p.kind = state.options.tool.kind().unwrap_or(Kind::Warp);
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(tl!("Width"));
            ui.add(egui::DragValue::new(&mut p.width).range(0.5..=10_000.)).labelled_by(label.id);
            let label = ui.label(tl!("Height"));
            ui.add(egui::DragValue::new(&mut p.height).range(0.5..=10_000.)).labelled_by(label.id);
            let label = ui.label(tl!("Angle"));
            ui.add(egui::DragValue::new(&mut p.angle).range(-360.0..=360.)).labelled_by(label.id);
            ui.add(egui::Slider::new(&mut p.intensity, 0.0..=1.0).text(tl!("Intensity")));
            ui.add(egui::Slider::new(&mut p.detail, 1.0..=10.0).text(tl!("Detail")));
            ui.checkbox(&mut p.use_pressure, tl!("Use pressure pen"));
            if p.kind.simplifies() {
                ui.checkbox(&mut p.simplify_on, tl!("Simplify"));
                ui.add(egui::Slider::new(&mut p.simplify, 0.0..=100.0).text(tl!("Simplify amount")));
            }
            if p.kind == Kind::Twirl {
                ui.add(egui::Slider::new(&mut p.rate, -180.0..=180.0).text(tl!("Twirl rate")));
            }
            if p.kind.has_affects() {
                ui.add(egui::Slider::new(&mut p.complexity, 0.0..=15.0).text(tl!("Complexity")));
                ui.checkbox(&mut p.affect_anchors, tl!("Anchor points"));
                ui.checkbox(&mut p.affect_in, tl!("In handles"));
                ui.checkbox(&mut p.affect_out, tl!("Out handles"));
            }
            if p.kind == Kind::Wrinkle {
                ui.add(egui::Slider::new(&mut p.horizontal, 0.0..=1.0).text(tl!("Horizontal")));
                ui.add(egui::Slider::new(&mut p.vertical, 0.0..=1.0).text(tl!("Vertical")));
            }
            ui.checkbox(&mut state.options.show_brush, tl!("Show brush"));
        });
        if old != state.options.brush && state.captured() {
            state.cancel();
        }
    }
}
/// Render beside the context bar; 00 may replace this inline prompt with the common modal.
pub fn conversion_prompt_with(ui: &mut egui::Ui, state: &mut State, s: &mut Session, run: Execute) -> Result<()> {
    let Some(c) = state.conversion.clone() else {
        return Ok(());
    };
    ui.label(tl!("Convert live shapes and text to curves to use this tool?"));
    let mut convert = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        convert = ui.button(tl!("Convert to curves")).clicked();
        cancel = ui.button(tl!("Cancel")).clicked();
    });
    if cancel {
        state.conversion = None;
        state.cancel();
    }
    if convert {
        let r = run(s, "object.distort.convert", serde_json::to_value(c).map_err(err)?);
        state.attempt(r)?;
        state.deactivate();
    }
    Ok(())
}
pub fn conversion_prompt(ui: &mut egui::Ui, state: &mut State, s: &mut Session) -> Result<()> {
    conversion_prompt_with(ui, state, s, execute)
}
pub fn paint_overlay(state: &State, painter: &egui::Painter, to_screen: impl Fn([f64; 2]) -> Pos2) {
    let blue = egui::Color32::from_rgb(50, 170, 255);
    if state.options.tool == Tool::PuppetWarp {
        if state.options.show_mesh
            && let Some(rig) = &state.rig
            && let Ok(pos) = rig.rig.deform(&state.pins)
        {
            for t in &rig.rig.mesh.tris {
                for i in 0..3 {
                    let (a, b) = (pos[t[i]], pos[t[(i + 1) % 3]]);
                    painter.line_segment([to_screen([a.x, a.y]), to_screen([b.x, b.y])], egui::Stroke::new(0.5, blue.gamma_multiply(0.3)));
                }
            }
        }
        for (i, q) in state.pins.iter().enumerate() {
            painter.circle_filled(to_screen(q.at), 5., if state.selected_pins.contains(&i) { blue } else { egui::Color32::WHITE });
            painter.circle_stroke(to_screen(q.at), 5., egui::Stroke::new(1., blue));
        }
    } else if state.options.show_brush
        && let Some(c) = state.hover
    {
        let b = state.options.brush;
        let (s, co) = b.angle.to_radians().sin_cos();
        let pts = (0..=64)
            .map(|i| {
                let (si, ci) = (i as f64 * std::f64::consts::TAU / 64.).sin_cos();
                let (x, y) = (ci * b.width / 2., si * b.height / 2.);
                to_screen([c[0] + x * co - y * s, c[1] + x * s + y * co])
            })
            .collect();
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1., egui::Color32::GRAY)));
    }
}
#[derive(Clone, Default)]
pub struct AppBridge {
    pub tool: Option<Tool>,
    pub state: State,
    pub routed: bool,
    owner: Option<crate::state::Tool>,
    persona: Option<photocraft_engine::toolsets::Persona>,
    module: Option<crate::Module>,
}
fn bridge_key() -> egui::Id {
    egui::Id::new("vector-distort-tools-41")
}
pub fn bridge(ctx: &egui::Context) -> AppBridge {
    ctx.data_mut(|d| d.get_temp::<AppBridge>(bridge_key()).unwrap_or_default())
}
pub fn save_bridge(ctx: &egui::Context, mut b: AppBridge) {
    b.tool = b.tool.map(|_| b.state.options.tool);
    ctx.data_mut(|d| d.insert_temp(bridge_key(), b));
}
pub fn activate(app: &crate::PhotocraftApp, ctx: &egui::Context, tool: Option<Tool>) {
    let mut b = bridge(ctx);
    b.state.deactivate();
    b.tool = tool;
    if let Some(t) = tool {
        b.state.options.tool = t;
    }
    b.owner = Some(app.ui.tool);
    b.persona = Some(app.session.prefs().toolbar.persona);
    b.module = Some(app.current_module);
    save_bridge(ctx, b);
}
pub fn frame(app: &crate::PhotocraftApp, ctx: &egui::Context) {
    let mut b = bridge(ctx);
    if b.owner.is_some_and(|t| t != app.ui.tool)
        || b.persona.is_some_and(|p| p != app.session.prefs().toolbar.persona)
        || b.module.is_some_and(|m| m != app.current_module)
    {
        b.tool = None;
    }
    b.state.synchronize(&app.session, b.tool.is_some() && app.ui.dialogs.is_empty() && ctx.input(|i| i.focused));
    save_bridge(ctx, b);
}
pub fn route(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, idx: usize, xf: &crate::canvas::ViewXform, canvas: Rect) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let ctx = ui.ctx().clone();
    frame(app, &ctx);
    let mut b = bridge(&ctx);
    if b.tool.is_none() || !app.ui.dialogs.is_empty() {
        return false;
    }
    b.routed = true;
    let pressure = f64::from(app.stylus.pressure());
    let tolerance = 6. / f64::from(xf.zoom.max(0.001));
    if let Err(e) = input(&mut b.state, &mut app.session, &ctx, canvas, |p| xf.to_doc(p), true, pressure, tolerance) {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    if app.ui.view.extras {
        paint_overlay(&b.state, &ui.painter_at(canvas), |p| xf.to_screen(p[0] as f32, p[1] as f32));
    }
    save_bridge(&ctx, b);
    true
}
pub fn display_doc(app: &crate::PhotocraftApp, idx: usize, ctx: &egui::Context) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    let b = bridge(ctx);
    if b.tool.is_none() || b.owner != Some(app.ui.tool) || b.persona != Some(app.session.prefs().toolbar.persona) || b.module != Some(app.current_module) {
        return None;
    }
    b.state.display_doc(&app.session)
}
#[cfg(test)]
#[path = "distort_tools/app_gpu.rs"]
mod app_gpu;
#[cfg(test)]
#[path = "distort_tools/tests.rs"]
mod tests;

pub fn app_context(_: &mut crate::PhotocraftApp, ui: &mut egui::Ui) {
    let mut b = bridge(ui.ctx());
    context(ui, &mut b.state);
    ui.ctx().data_mut(|d| d.insert_temp(bridge_key(), b));
}
