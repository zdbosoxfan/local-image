//! Constructive tool capture/hover/preview. Adapted from VectorCraft tools/{builder,cut,draw2}.rs
//! and draw2/{gesture,anchor}.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
//! See licenses/vectorcraft-NOTICE and integration/10.md for the shared app/router hooks.

use egui::{Color32, Modifiers, Pos2, Stroke};
use photocraft_doc::{Document, LayerId, Path};
use photocraft_engine::{
    Session,
    vector_packages::constructive::{Action, Gesture, JoinKind, MAX_POINTS},
};
use photocraft_geom::Point;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Builder,
    Knife,
    Scissors,
    LineCut,
    Contour,
    Smooth,
    Eraser,
    Join,
}
impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Builder => "Shape Builder",
            Self::Knife => "Knife",
            Self::Scissors => "Scissors",
            Self::LineCut => "Line Cut",
            Self::Contour => "Contour",
            Self::Smooth => "Smooth",
            Self::Eraser => "Path Eraser",
            Self::Join => "Join",
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Capture {
    screen: Pos2,
    start: Point,
    zoom: f64,
    flip: bool,
    last: Point,
    modifiers: Modifiers,
}
#[derive(Clone, Debug)]
struct Drag {
    gesture: Gesture,
    tool: Tool,
    points: Vec<Point>,
    faces: BTreeSet<usize>,
    style: Option<LayerId>,
    normal: Option<Point>,
    scissors: Option<Action>,
    action: Option<Action>,
}
#[derive(Clone, Debug)]
pub struct State {
    pub tool: Option<Tool>,
    pub targets: Vec<LayerId>,
    pub delta: f64,
    pub amount: f64,
    pub radius: f64,
    pub join: JoinKind,
    pub miter: f64,
    pub error: Option<String>,
    pub hover: Option<usize>,
    pub removing: bool,
    map: Option<Gesture>,
    drag: Option<Drag>,
    capture: Option<Capture>,
    preview: Option<Arc<Document>>,
    preview_key: u64,
    controls: Option<Gesture>,
    control_epoch: u64,
    control_dirty: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tool: None,
            targets: Vec::new(),
            delta: 0.0,
            amount: 0.5,
            radius: 8.0,
            join: JoinKind::Round,
            miter: 4.0,
            error: None,
            hover: None,
            removing: false,
            map: None,
            drag: None,
            capture: None,
            preview: None,
            preview_key: 1 << 55,
            controls: None,
            control_epoch: 0,
            control_dirty: false,
        }
    }
}
impl State {
    pub fn set_tool(&mut self, tool: Option<Tool>) {
        if self.tool != tool {
            self.cancel();
            self.map = None;
            self.tool = tool;
        }
    }
    pub fn set_targets(&mut self, targets: Vec<LayerId>) {
        if self.targets != targets {
            self.cancel();
            self.map = None;
            self.targets = targets;
        }
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.capture = None;
        self.preview = None;
        self.hover = None;
        self.controls = None;
        self.control_dirty = false;
        self.delta = 0.0;
        self.control_epoch = self.control_epoch.wrapping_add(1);
    }
    pub fn frame(&mut self, session: &Session, ctx: &egui::Context, active: bool) {
        let lost = ctx.input(|i| !i.focused || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))));
        let escape = crate::shortcuts::canvas_keyboard_available(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let stale = self.drag.as_ref().is_some_and(|d| !d.gesture.is_current(session)) || self.controls.as_ref().is_some_and(|g| !g.is_current(session));
        if !active || lost || escape || stale {
            self.cancel();
        }
        if self.map.as_ref().is_some_and(|g| !g.is_current(session)) {
            self.map = None;
            self.hover = None;
        }
    }
    fn gesture(&mut self, session: &Session) -> Result<Gesture, String> {
        if self
            .map
            .as_ref()
            .is_none_or(|g| !g.is_current(session) || g.ids().iter().copied().collect::<BTreeSet<_>>() != self.targets.iter().copied().collect())
        {
            self.map = Some(Gesture::begin(session, &self.targets).map_err(|e| e.to_string())?);
        }
        self.map.clone().ok_or_else(|| "constructive snapshot unavailable".into())
    }
    pub fn display_document(&self, session: &Session) -> Option<(Arc<Document>, u64)> {
        let g = self.drag.as_ref().map(|d| &d.gesture).or(self.controls.as_ref())?;
        g.is_current(session).then(|| self.preview.clone().map(|p| (p, self.preview_key))).flatten()
    }
    fn show(&mut self, gesture: &Gesture, action: &Action) -> Result<(), String> {
        let prepared = gesture.preview(action).map_err(|e| e.to_string())?;
        self.preview = prepared.changed().then(|| prepared.document().map(Arc::new)).transpose().map_err(|e| e.to_string())?;
        self.preview_key = self.preview_key.wrapping_add(1) | (1 << 55);
        Ok(())
    }
    /// Input capture retains the press transform and receives release beyond the canvas.
    /// Modifiers are read every frame, even when the pointer does not move.
    pub fn canvas_input(&mut self, session: &mut Session, response: &egui::Response, xf: &crate::canvas::ViewXform) -> bool {
        if self.tool.is_none() {
            return false;
        }
        let ctx = &response.ctx;
        self.frame(session, ctx, true);
        let (pressed, released, down, pos, modifiers) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos().or(i.pointer.interact_pos()),
                i.modifiers,
            )
        });
        if self.capture.is_none() && !down && pos.is_none_or(|p| !response.rect.contains(p)) {
            self.hover = None;
        }
        if self.capture.is_none()
            && !down
            && let Some(p) = pos.filter(|p| response.rect.contains(*p))
        {
            let [x, y] = xf.to_doc(p);
            self.pointer(session, crate::canvas::ToolEvent::Move { x, y, pressure: 1.0 }, modifiers, xf.zoom as f64);
        }
        if self.capture.is_none() && pressed {
            let origin = ctx.input(|i| i.pointer.press_origin()).or(pos);
            if let Some(p) = origin.filter(|p| response.rect.contains(*p) && ctx.layer_id_at(*p) == Some(response.layer_id)) {
                let [x, y] = xf.to_doc(p);
                self.pointer(session, crate::canvas::ToolEvent::Down { x, y, pressure: 1.0 }, modifiers, xf.zoom as f64);
                if self.is_dragging() {
                    self.capture = Some(Capture { screen: p, start: Point::new(x, y), zoom: xf.zoom as f64, flip: xf.flip, last: Point::new(x, y), modifiers });
                }
            }
        }
        let Some(mut capture) = self.capture else {
            return true;
        };
        let p = pos.map_or(capture.last, |p| {
            let d = p - capture.screen;
            Point::new(capture.start.x + d.x as f64 / capture.zoom * if capture.flip { -1.0 } else { 1.0 }, capture.start.y + d.y as f64 / capture.zoom)
        });
        // Retain every OS pointer sample in the frame, including a release frame. A fast
        // freehand knife must not become the chord between only the first/last samples.
        if down || released {
            let samples = ctx.input(|i| {
                let mut after_press = !pressed;
                i.events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: true, .. } => {
                            after_press = true;
                            None
                        }
                        egui::Event::PointerMoved(p) if after_press => Some(*p),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });
            for sample in samples {
                let delta = sample - capture.screen;
                let at = Point::new(
                    capture.start.x + delta.x as f64 / capture.zoom * if capture.flip { -1.0 } else { 1.0 },
                    capture.start.y + delta.y as f64 / capture.zoom,
                );
                if at != capture.last {
                    self.pointer(session, crate::canvas::ToolEvent::Move { x: at.x, y: at.y, pressure: 1.0 }, modifiers, capture.zoom);
                    capture.last = at;
                }
                if !self.is_dragging() {
                    self.capture = None;
                    return true;
                }
            }
        }
        if released {
            self.pointer(session, crate::canvas::ToolEvent::Up { x: p.x, y: p.y }, modifiers, capture.zoom);
            self.capture = None;
        } else if down {
            if p != capture.last || modifiers != capture.modifiers {
                self.pointer(session, crate::canvas::ToolEvent::Move { x: p.x, y: p.y, pressure: 1.0 }, modifiers, capture.zoom);
            }
            capture.last = p;
            capture.modifiers = modifiers;
            self.capture = self.is_dragging().then_some(capture);
        } else {
            self.cancel();
        }
        true
    }
    pub fn pointer(&mut self, session: &mut Session, event: crate::canvas::ToolEvent, modifiers: Modifiers, zoom: f64) -> bool {
        let Some(tool) = self.tool else {
            return false;
        };
        if !zoom.is_finite() || zoom <= 0.0 {
            self.error = Some("invalid constructive view scale".into());
            self.cancel();
            return true;
        }
        if self.drag.as_ref().is_some_and(|d| !d.gesture.is_current(session)) {
            self.cancel();
            return true;
        }
        let p = match event {
            crate::canvas::ToolEvent::Down { x, y, .. } | crate::canvas::ToolEvent::Move { x, y, .. } | crate::canvas::ToolEvent::Up { x, y } => {
                Point::new(x, y)
            }
        };
        self.removing = modifiers.alt;
        let result = match event {
            crate::canvas::ToolEvent::Move { .. } if self.drag.is_none() => {
                if self.targets.is_empty() {
                    self.hover = None;
                    self.map = None;
                    Ok(())
                } else if tool == Tool::Builder {
                    self.gesture(session).and_then(|g| {
                        self.hover = g.face_at(p).map_err(|e| e.to_string())?;
                        Ok(())
                    })
                } else {
                    Ok(())
                }
            }
            crate::canvas::ToolEvent::Down { .. } => self.gesture(session).and_then(|gesture| {
                let normal = if tool == Tool::Contour { gesture.contour_normal(p, 8.0 / zoom).map_err(|e| e.to_string())? } else { None };
                let scissors = if tool == Tool::Scissors {
                    gesture.nearest(p, 6.0 / zoom).map_err(|e| e.to_string())?.map(|(layer, subpath, segment, t, _)| Action::Scissors {
                        layer,
                        subpath,
                        segment,
                        t,
                    })
                } else {
                    None
                };
                if (tool == Tool::Contour && normal.is_none()) || (tool == Tool::Scissors && scissors.is_none()) {
                    return Ok(());
                }
                self.drag = Some(Drag { gesture, tool, points: vec![p], faces: Default::default(), style: None, normal, scissors, action: None });
                self.delta = 0.0;
                self.error = None;
                self.update(p, modifiers, zoom)
            }),
            crate::canvas::ToolEvent::Move { .. } if self.drag.is_some() => self.update(p, modifiers, zoom),
            crate::canvas::ToolEvent::Up { .. } if self.drag.is_some() => {
                let updated = self.update(p, modifiers, zoom);
                if updated.is_ok() {
                    if let Some(d) = self.drag.take()
                        && let Some(action) = d.action
                    {
                        d.gesture.commit(session, action).map(|_| ()).map_err(|e| e.to_string())
                    } else {
                        Ok(())
                    }
                } else {
                    updated
                }
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.error = Some(e);
            self.cancel();
        }
        if matches!(event, crate::canvas::ToolEvent::Up { .. }) {
            self.preview = None;
            self.drag = None;
            self.map = None;
        }
        true
    }
    fn update(&mut self, p: Point, mods: Modifiers, zoom: f64) -> Result<(), String> {
        let Some(d) = &mut self.drag else {
            return Ok(());
        };
        let start = d.points[0];
        let mut p = p;
        if d.tool == Tool::LineCut && mods.shift {
            let (dx, dy) = (p.x - start.x, p.y - start.y);
            let angle = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            let len = dx.hypot(dy);
            p = Point::new(start.x + angle.cos() * len, start.y + angle.sin() * len);
        }
        if d.tool == Tool::Builder {
            let last = d.points.last().copied().unwrap_or(p);
            let count = (((p.x - last.x).hypot(p.y - last.y) * zoom / 2.0).ceil() as usize).clamp(1, 4000);
            for k in 0..=count {
                let t = k as f64 / count as f64;
                let q = Point::new(last.x + (p.x - last.x) * t, last.y + (p.y - last.y) * t);
                if let Some(face) = d.gesture.face_at(q).map_err(|e| e.to_string())? {
                    d.faces.insert(face);
                    if d.style.is_none() {
                        d.style = Some(d.gesture.face_style(face).map_err(|e| e.to_string())?);
                    }
                }
            }
        }
        if d.points.last().is_none_or(|q| *q != p) {
            if d.points.len() >= MAX_POINTS {
                return Err("constructive stroke exceeds the point budget".into());
            }
            d.points.push(p);
        }
        let stroke = || d.points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>();
        let action = match d.tool {
            Tool::Builder => d.style.map(|style| Action::Builder { faces: d.faces.iter().copied().collect(), remove: mods.alt, style }),
            Tool::Knife if d.points.len() > 1 => Some(Action::Knife { points: stroke() }),
            Tool::LineCut if (p.x - start.x).hypot(p.y - start.y) * zoom >= 3.0 => Some(Action::LineCut { from: [start.x, start.y], to: [p.x, p.y] }),
            Tool::Scissors => d.scissors.clone(),
            Tool::Contour => {
                let normal = d.normal.ok_or("contour normal unavailable")?;
                let mut delta = (p.x - start.x) * normal.x + (p.y - start.y) * normal.y;
                if mods.shift {
                    delta = delta.round();
                }
                self.delta = delta.clamp(-100_000.0, 100_000.0);
                Some(Action::Contour { delta: self.delta, join: self.join, miter: self.miter })
            }
            Tool::Smooth => Some(Action::Smooth { points: stroke(), radius: self.radius, amount: self.amount }),
            Tool::Eraser => Some(Action::Erase { points: stroke(), width: self.radius * 2.0 }),
            Tool::Join => Some(Action::Join { points: stroke(), tolerance: self.radius }),
            _ => None,
        };
        d.action = action.clone();
        let gesture = d.gesture.clone();
        if let Some(action) = action {
            self.show(&gesture, &action)?;
        }
        Ok(())
    }
    /// Numeric distances use document units/DPI; geometry is always in document pixels.
    /// The numeric editor captures before typing, previews on change,
    /// and commits on Enter/focus completion; Escape and revision changes discard the edit buffer.
    pub fn context_bar(&mut self, ui: &mut egui::Ui, session: &mut Session) {
        let Some(tool) = self.tool else {
            return;
        };
        ui.label(crate::i18n::t(tool.label()));
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, crate::i18n::t("Unable to edit paths")).on_hover_text(error);
        }
        if tool == Tool::Builder {
            ui.label(crate::i18n::t("Drag to merge; Alt to remove"));
        }
        if tool == Tool::Contour {
            egui::ComboBox::from_id_salt("constructive-join")
                .selected_text(crate::i18n::t(match self.join {
                    JoinKind::Round => "Round",
                    JoinKind::Miter => "Miter",
                    JoinKind::Bevel => "Bevel",
                }))
                .show_ui(ui, |ui| {
                    for (v, label) in [(JoinKind::Round, "Round"), (JoinKind::Miter, "Miter"), (JoinKind::Bevel, "Bevel")] {
                        ui.selectable_value(&mut self.join, v, crate::i18n::t(label));
                    }
                });
            let label = ui.label(crate::i18n::t("Offset"));
            let (unit, dpi, suffix) = units(session);
            let mut displayed = unit.from_px(self.delta, dpi, 1.0, 72.0);
            let max = unit.from_px(100_000.0, dpi, 1.0, 72.0);
            let r = ui
                .push_id(("constructive-offset", self.control_epoch), |ui| ui.add(egui::DragValue::new(&mut displayed).range(-max..=max).suffix(suffix)))
                .inner
                .labelled_by(label.id);
            if r.changed() {
                self.delta = unit.to_px(displayed, dpi, 1.0, 72.0);
            }
            if self.controls.is_none() && (r.gained_focus() || r.drag_started() || r.changed()) {
                match Gesture::begin(session, &self.targets) {
                    Ok(g) => self.controls = Some(g),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            self.control_dirty |= r.changed();
            let action = Action::Contour { delta: self.delta, join: self.join, miter: self.miter };
            if r.changed()
                && let Some(g) = self.controls.clone()
                && let Err(e) = self.show(&g, &action)
            {
                self.error = Some(e);
                self.cancel();
            }
            if self.control_dirty && (r.lost_focus() || r.drag_stopped()) {
                if let Some(g) = self.controls.take() {
                    self.error = g.commit(session, action).err().map(|e| e.to_string());
                }
                self.control_dirty = false;
                self.preview = None;
                self.map = None;
                self.delta = 0.0;
            }
            let label = ui.label(crate::i18n::t("Miter limit"));
            ui.add(egui::DragValue::new(&mut self.miter).range(1.0..=1000.0)).labelled_by(label.id);
        }
        if matches!(tool, Tool::Smooth | Tool::Eraser | Tool::Join) {
            let label = ui.label(crate::i18n::t("Radius"));
            let (unit, dpi, suffix) = units(session);
            let mut radius = unit.from_px(self.radius, dpi, 1.0, 72.0);
            let lo = unit.from_px(0.01, dpi, 1.0, 72.0);
            let hi = unit.from_px(500.0, dpi, 1.0, 72.0);
            if ui.add(egui::DragValue::new(&mut radius).range(lo..=hi).suffix(suffix)).labelled_by(label.id).changed() {
                self.radius = unit.to_px(radius, dpi, 1.0, 72.0);
            }
        }
        if tool == Tool::Smooth {
            let label = ui.label(crate::i18n::t("Amount"));
            ui.add(egui::Slider::new(&mut self.amount, 0.0..=1.0)).labelled_by(label.id);
        }
    }
    pub fn draw_overlay(&self, painter: &egui::Painter, to_screen: &impl Fn(Point) -> Pos2, zoom: f64) {
        let map = self.drag.as_ref().map(|d| &d.gesture).or(self.map.as_ref());
        if self.tool == Some(Tool::Builder)
            && let Some(map) = map
            && let Ok(arr) = map.arrangement()
        {
            let mut faces = self.drag.as_ref().map_or_else(BTreeSet::new, |d| d.faces.clone());
            if let Some(i) = self.hover {
                faces.insert(i);
            }
            for i in faces {
                if let Some(r) = arr.regions.get(i) {
                    outline(painter, &r.path, to_screen, zoom, if self.removing { Color32::LIGHT_RED } else { Color32::GRAY });
                }
            }
        }
        if let Some(d) = &self.drag
            && d.points.len() > 1
            && d.tool != Tool::Contour
        {
            painter.add(egui::Shape::line(d.points.iter().map(|p| to_screen(*p)).collect(), Stroke::new(1.0, Color32::LIGHT_BLUE)));
        }
    }
}
fn units(session: &Session) -> (photocraft_engine::prefs::Unit, f64, &'static str) {
    use photocraft_doc::vector_design::structure::DesignUnits;
    use photocraft_engine::prefs::Unit;
    let Some(st) = session.active() else {
        return (Unit::Pixels, 72.0, " px");
    };
    let (unit, suffix) = match st.doc.design_units {
        DesignUnits::Pixels => (Unit::Pixels, " px"),
        DesignUnits::Points => (Unit::Points, " pt"),
        DesignUnits::Millimeters => (Unit::Millimeters, " mm"),
        DesignUnits::Inches => (Unit::Inches, " in"),
    };
    let dpi = st.doc.resolution_dpi as f64;
    (unit, if dpi.is_finite() && dpi > 0.0 { dpi } else { 72.0 }, suffix)
}
fn hatch(painter: &egui::Painter, path: &Path, to_screen: &impl Fn(Point) -> Pos2, zoom: f64, color: Color32) {
    let rotate = |p: (f64, f64)| ((p.0 + p.1) * std::f64::consts::FRAC_1_SQRT_2, (p.1 - p.0) * std::f64::consts::FRAC_1_SQRT_2);
    let unrotate = |x: f64, y: f64| Point::new((x - y) * std::f64::consts::FRAC_1_SQRT_2, (x + y) * std::f64::consts::FRAC_1_SQRT_2);
    let mut edges = Vec::new();
    for line in photocraft_vector::flatten_path(path, 0.25 / zoom.max(0.01)) {
        for p in line.pts.windows(2) {
            edges.push((rotate(p[0]), rotate(p[1])));
        }
        if line.closed
            && let (Some(a), Some(b)) = (line.pts.first(), line.pts.last())
        {
            edges.push((rotate(*b), rotate(*a)));
        }
    }
    let y0 = edges.iter().map(|(a, b)| a.1.min(b.1)).fold(f64::INFINITY, f64::min);
    let y1 = edges.iter().map(|(a, b)| a.1.max(b.1)).fold(f64::NEG_INFINITY, f64::max);
    if !y0.is_finite() || y1 <= y0 {
        return;
    }
    let gap = (6.0 / zoom.max(0.01)).max((y1 - y0) / 400.0);
    let mut y = (y0 / gap).floor() * gap + gap * 0.5;
    while y < y1 {
        let mut xs = edges.iter().filter(|(a, b)| (a.1 <= y) != (b.1 <= y)).map(|(a, b)| a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0)).collect::<Vec<_>>();
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            painter.line_segment([to_screen(unrotate(pair[0], y)), to_screen(unrotate(pair[1], y))], Stroke::new(1.0, color.gamma_multiply(0.65)));
        }
        y += gap;
    }
}
fn outline(painter: &egui::Painter, path: &Path, to_screen: &impl Fn(Point) -> Pos2, zoom: f64, color: Color32) {
    hatch(painter, path, to_screen, zoom, color);
    for line in photocraft_vector::flatten_path(path, 0.25 / zoom.max(0.01)) {
        let mut p = line.pts.iter().map(|p| to_screen(Point::new(p.0, p.1))).collect::<Vec<_>>();
        if line.closed
            && let Some(first) = p.first().copied()
        {
            p.push(first);
        }
        painter.add(egui::Shape::line(p, Stroke::new(2.0, color)));
    }
}
#[cfg(test)]
#[path = "constructive/tests.rs"]
mod tests;
