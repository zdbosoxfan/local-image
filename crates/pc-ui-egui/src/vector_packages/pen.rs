//! Pen context controls, pointer capture and detached canvas preview.
//! Adapted from VectorCraft crates/tools/src/pen.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 and existing PhotoCraft capture patterns.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; see licenses/vectorcraft-*.

use egui::{Color32, Context, Event, Key, Painter, PointerButton, Pos2, Rect, Stroke};
use photocraft_doc::{Document, Fill, LayerContent, ShapeStroke};
use photocraft_engine::{
    Result, Session,
    vector_packages::pen::{self, Gesture, Mode, Modifiers, Outcome, Target},
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub mode: Mode,
    #[serde(default = "yes")]
    pub rubber_band: bool,
    pub add_to_selected: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { mode: Mode::Pen, rubber_band: true, add_to_selected: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub options: Options,
    pub gesture: Option<Gesture>,
    /// Consumed by the existing CPU/GPU canvas preview route; never put in Session.
    pub preview: Option<Document>,
    pub generation: u64,
    last_pointer: Option<Point>,
    modifiers: egui::Modifiers,
    preview_layer: Option<photocraft_doc::LayerId>,
}

fn mods(m: egui::Modifiers) -> Modifiers {
    Modifiers { alt: m.alt, shift: m.shift }
}

impl State {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.preview = None;
        self.preview_layer = None;
        self.last_pointer = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        if (!enabled || self.gesture.as_ref().is_some_and(|g| !g.is_fresh(s) || g.mode() != self.options.mode))
            && (self.gesture.is_some() || self.preview.is_some())
        {
            self.cancel();
        }
    }
    fn update_preview(&mut self, s: &Session) -> Result<()> {
        if let Some(g) = &self.gesture {
            pen::update_preview(s, g.request(), &mut self.preview, &mut self.preview_layer)?;
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
    pub fn down(
        &mut self,
        s: &Session,
        target: Target,
        at: Point,
        tolerance: f64,
        modifiers: egui::Modifiers,
        fill: Option<Fill>,
        stroke: Option<ShapeStroke>,
    ) -> Result<()> {
        self.synchronize(s, true);
        self.modifiers = modifiers;
        self.last_pointer = Some(at);
        let r = if let Some(g) = &mut self.gesture {
            g.press(at, mods(modifiers), tolerance)
        } else {
            Gesture::begin(s, target, self.options.mode, at, tolerance, fill, stroke).map(|g| {
                self.gesture = Some(g);
            })
        };
        if let Err(e) = r {
            self.cancel();
            return Err(e);
        }
        if let Err(e) = self.update_preview(s) {
            self.cancel();
            return Err(e);
        }
        Ok(())
    }
    pub fn motion(&mut self, s: &Session, at: Point, modifiers: egui::Modifiers) -> Result<()> {
        self.synchronize(s, true);
        self.modifiers = modifiers;
        self.last_pointer = Some(at);
        if let Some(g) = &mut self.gesture {
            let before = g.generation();
            if let Err(e) = g.motion(at, mods(modifiers)) {
                self.cancel();
                return Err(e);
            }
            if g.generation() != before
                && let Err(e) = self.update_preview(s)
            {
                self.cancel();
                return Err(e);
            }
        }
        Ok(())
    }
    pub fn up(&mut self, s: &mut Session) -> Result<()> {
        self.synchronize(s, true);
        if let Some(g) = &mut self.gesture {
            match g.release()? {
                Outcome::Finished(c) => {
                    s.execute("path.penCommit", serde_json::to_value(c).map_err(|e| photocraft_engine::EngineError::Other(e.to_string()))?)?;
                    self.cancel();
                }
                Outcome::Preview => {}
            }
        }
        Ok(())
    }
    pub fn enter(&mut self, s: &mut Session) -> Result<()> {
        let Some(g) = self.gesture.clone() else { return Ok(()) };
        // A lone node or a continuation with no edit is simply abandoned.
        if let Ok(c) = g.finish() {
            s.execute("path.penCommit", serde_json::to_value(c).map_err(|e| photocraft_engine::EngineError::Other(e.to_string()))?)?;
        }
        self.cancel();
        Ok(())
    }
}

/// Choose a target once at Begin. Continuation takes precedence over new-layer drawing.
pub fn destination(s: &Session, at: Point, tolerance: f64, shape_mode: bool, vector_mask: bool, add_to_selected: bool) -> Result<Target> {
    let st = s.active().ok_or(photocraft_engine::EngineError::NoDocument)?;
    if vector_mask {
        return st.active_layer.map(|layer| Target::VectorMask { layer }).ok_or_else(|| photocraft_engine::EngineError::Other("no vector mask target".into()));
    }
    if !shape_mode {
        return Ok(Target::Work);
    }
    if let Some(layer) = st.selected_layers().first().copied()
        && let Some(l) = st.doc.layer(layer)
        && let LayerContent::Shape(sh) = &l.content
    {
        let endpoint = sh
            .path
            .subpaths
            .iter()
            .filter(|sp| !sp.closed)
            .any(|sp| sp.knots.first().into_iter().chain(sp.knots.last()).any(|k| (at.x - k.anchor.x).hypot(at.y - k.anchor.y) <= tolerance));
        if add_to_selected || endpoint {
            if st.selected_layers.len() != 1 {
                return Err(photocraft_engine::EngineError::Other("select one curve for pen addition".into()));
            }
            return Ok(Target::Shape { layer });
        }
    }
    if add_to_selected {
        return Err(photocraft_engine::EngineError::Other("select one curve for pen addition".into()));
    }
    Ok(Target::NewShape { above: st.active_layer })
}

/// Context-bar controls; changing mode cancels an unfinished gesture.
pub fn context(ui: &mut egui::Ui, state: &mut State) {
    if context_options(ui, &mut state.options) {
        state.cancel();
    }
}

pub fn context_options(ui: &mut egui::Ui, options: &mut Options) -> bool {
    let old = options.clone();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut options.mode, Mode::Pen, tl!("Pen"));
        ui.selectable_value(&mut options.mode, Mode::Smart, tl!("Smart"));
        ui.selectable_value(&mut options.mode, Mode::Polygon, tl!("Polygon"));
        ui.selectable_value(&mut options.mode, Mode::Line, tl!("Line"));
        ui.checkbox(&mut options.rubber_band, tl!("Rubber band"));
        ui.checkbox(&mut options.add_to_selected, tl!("Add to selected curve"));
    });
    old.mode != options.mode || old.add_to_selected != options.add_to_selected
}

/// Isolated-host input entrypoint. The app router may instead call down/motion/up directly
/// after its existing view transforms, snapping and modifier delegation. Captured releases
/// outside the canvas are honored. Tool/persona/modal deactivation must pass enabled=false.
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> Point,
    enabled: bool,
    shape_mode: bool,
    vector_mask: bool,
    tolerance: f64,
    fill: Option<Fill>,
    stroke: Option<ShapeStroke>,
) -> Result<()> {
    state.synchronize(s, enabled);
    if !enabled {
        return Ok(());
    }
    let (events, current_mods, focused) = ctx.input(|i| (i.events.clone(), i.modifiers, i.focused));
    if !focused || events.iter().any(|e| matches!(e, Event::WindowFocused(false))) {
        state.cancel();
        return Ok(());
    }
    let mut m = current_mods;
    for event in events {
        match event {
            Event::ModifiersChanged(new) => {
                m = new;
                if let Some(p) = state.last_pointer {
                    state.motion(s, p, m)?;
                }
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers } => {
                m = modifiers;
                if pressed && canvas.contains(pos) {
                    let at = to_doc(pos);
                    let target = destination(s, at, tolerance, shape_mode, vector_mask, state.options.add_to_selected)?;
                    state.down(s, target, at, tolerance, m, fill.clone(), stroke.clone())?;
                } else if !pressed && state.gesture.as_ref().is_some_and(Gesture::captured) {
                    state.motion(s, to_doc(pos), m)?;
                    state.up(s)?;
                }
            }
            Event::PointerMoved(pos) if canvas.contains(pos) || state.gesture.as_ref().is_some_and(Gesture::captured) => state.motion(s, to_doc(pos), m)?,
            Event::PointerGone => {
                state.cancel();
            }
            Event::Key { key: Key::Escape, pressed: true, .. } => state.cancel(),
            Event::Key { key: Key::Enter, pressed: true, .. } if !ctx.egui_wants_keyboard_input() => state.enter(s)?,
            _ => {}
        }
    }
    if current_mods != state.modifiers
        && let Some(p) = state.last_pointer
    {
        state.motion(s, p, current_mods)?;
    }
    Ok(())
}

/// Geometry/controls overlay. Filled preview pixels come from State::preview through the
/// normal compositor, so curves/concavity/closed segments have committed-cache parity.
pub fn paint_overlay(state: &State, painter: &Painter, to_screen: impl Fn(Point) -> Pos2) {
    let Some(g) = &state.gesture else { return };
    let ink = Color32::from_rgb(48, 180, 255);
    let line = |sp: &photocraft_doc::Subpath, color| {
        for [a, b, c, d] in sp.segments() {
            let points: Vec<_> = (0..=32)
                .map(|i| {
                    let t = i as f64 / 32.0;
                    let u = 1.0 - t;
                    to_screen(Point::new(
                        u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x,
                        u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y,
                    ))
                })
                .collect();
            painter.add(egui::Shape::line(points.clone(), Stroke::new(3.0, Color32::BLACK)));
            painter.add(egui::Shape::line(points, Stroke::new(1.0, color)));
        }
    };
    for sp in &g.path().subpaths {
        line(sp, ink);
        for k in &sp.knots {
            let p = to_screen(k.anchor);
            for h in [k.in_ctrl, k.out_ctrl] {
                if h != k.anchor {
                    painter.line_segment([p, to_screen(h)], Stroke::new(1.0, ink));
                    painter.circle_filled(to_screen(h), 3.0, ink);
                }
            }
            painter.rect_filled(Rect::from_center_size(p, egui::vec2(6.0, 6.0)), 0.0, ink);
        }
    }
    if state.options.rubber_band
        && let Some(sp) = g.rubber_band(mods(state.modifiers))
    {
        line(&sp, Color32::WHITE);
    }
}
