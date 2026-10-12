//! Live Paint and Vector Flood Fill input fragments. GPL-3.0-or-later Local Image.
//! Interaction follows Adobe Live Paint Bucket and Affinity Vector Flood Fill help.
//! App/menu/toolbar routing is owned by 00/01/46; see integration/42.md.
use egui::{Color32, Key, Modifiers, Pos2, Stroke};
use photocraft_doc::{Document, Fill, LayerId};
use photocraft_engine::{
    Session,
    vector_packages::live_paint::{self as engine, Action, FloodOptions, Gap, Gesture, Signature},
};
use photocraft_geom::Point;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Bucket,
    Selection,
    Flood,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Keymap {
    #[default]
    Illustrator,
    Affinity,
}
#[derive(Clone, Debug)]
struct Drag {
    gesture: Gesture,
    points: Vec<[f64; 2]>,
    transform: crate::canvas::ViewXform,
}
#[derive(Clone, Debug)]
pub struct State {
    pub tool: Option<Tool>,
    pub targets: Vec<LayerId>,
    pub paint: Option<Fill>,
    pub width: f32,
    pub gap: Gap,
    pub flood: FloodOptions,
    pub hover_face: Option<usize>,
    pub hover_edge: Option<usize>,
    pub selected_faces: BTreeSet<Signature>,
    pub selected_edges: BTreeSet<Signature>,
    pub error: Option<String>,
    map: Option<Gesture>,
    drag: Option<Drag>,
    preview: Option<Arc<Document>>,
    key: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tool: None,
            targets: Vec::new(),
            paint: Some(Fill::Solid(photocraft_doc::Color::BLACK)),
            width: 1.,
            gap: Gap::None,
            flood: Default::default(),
            hover_face: None,
            hover_edge: None,
            selected_faces: BTreeSet::new(),
            selected_edges: BTreeSet::new(),
            error: None,
            map: None,
            drag: None,
            preview: None,
            key: 1 << 54,
        }
    }
}
impl State {
    pub fn set_tool(&mut self, tool: Option<Tool>) {
        if self.tool != tool {
            self.cancel();
            self.tool = tool;
        }
    }
    pub fn set_targets(&mut self, targets: Vec<LayerId>) {
        if self.targets != targets {
            self.cancel();
            self.map = None;
            self.targets = targets;
            self.selected_faces.clear();
            self.selected_edges.clear();
        }
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.preview = None;
        self.hover_face = None;
        self.hover_edge = None;
    }
    pub fn frame(&mut self, session: &Session, ctx: &egui::Context, active: bool) {
        let keyboard = crate::shortcuts::canvas_keyboard_available(ctx);
        let cancel =
            ctx.input(|i| !i.focused || (keyboard && i.key_pressed(Key::Escape)) || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))));
        if !active || cancel || self.drag.as_ref().is_some_and(|d| !d.gesture.is_current(session)) {
            self.cancel();
        }
        if self.map.as_ref().is_some_and(|g| !g.is_current(session)) {
            self.map = None;
            self.selected_faces.clear();
            self.selected_edges.clear();
        }
    }
    fn gesture(&mut self, s: &Session) -> Result<Gesture, String> {
        if self.map.as_ref().is_none_or(|g| !g.is_current(s)) {
            self.map = Some(Gesture::begin(s, &self.targets).map_err(|e| e.to_string())?);
        }
        self.map.clone().ok_or_else(|| "paint gesture unavailable".into())
    }
    pub fn display_document(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        self.drag.as_ref().filter(|d| d.gesture.is_current(s)).and_then(|_| self.preview.clone().map(|p| (p, self.key)))
    }
    pub fn shortcuts(&mut self, s: &mut Session, ctx: &egui::Context, keymap: Keymap, enabled: bool) {
        if !enabled || ctx.egui_wants_keyboard_input() {
            return;
        }
        let mods = ctx.input(|i| i.modifiers);
        let make = ctx.input_mut(|i| i.consume_key(Modifiers::ALT | Modifiers::COMMAND, Key::X));
        if make {
            let result = self.gesture(s).and_then(|g| g.commit(s, Action::Make { gap: self.gap }).map_err(|e| e.to_string()));
            match result {
                Ok(ids) => {
                    self.set_targets(ids);
                    self.set_tool(Some(Tool::Bucket));
                }
                Err(e) => self.error = Some(e),
            }
        }
        if mods.command || mods.ctrl || mods.alt {
            return;
        }
        if keymap == Keymap::Illustrator && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::L)) {
            self.set_tool(Some(Tool::Selection));
        } else if keymap == Keymap::Illustrator && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::K)) {
            self.set_tool(Some(Tool::Bucket));
        } else if keymap == Keymap::Affinity && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::R)) {
            self.set_tool(Some(Tool::Flood));
        }
    }
    fn action(&self, drag: &Drag, mods: Modifiers) -> Option<Action> {
        if mods.alt || self.tool == Some(Tool::Selection) {
            return None;
        }
        if self.tool == Some(Tool::Flood) {
            self.paint.clone().map(|paint| Action::Flood { points: drag.points.clone(), paint, options: self.flood.clone() })
        } else {
            Some(Action::Paint {
                points: drag.points.clone(),
                paint: self.paint.clone(),
                edges: mods.shift,
                width: self.width,
                tolerance: 4. / f64::from(drag.transform.zoom).max(0.01),
            })
        }
    }
    fn update(&mut self, s: &Session, point: Point, mods: Modifiers) -> Result<(), String> {
        if self.drag.is_none() && self.map.as_ref().is_none_or(|g| !g.is_current(s)) {
            self.map = Some(Gesture::begin(s, &self.targets).map_err(|e| e.to_string())?);
        }
        let g = self.drag.as_ref().map(|d| &d.gesture).or(self.map.as_ref()).ok_or("paint gesture unavailable")?;
        self.hover_face = g.graph.face_at(point).map_err(|e| e.to_string())?;
        let zoom = self.drag.as_ref().map_or(1., |d| f64::from(d.transform.zoom));
        self.hover_edge = g.graph.edge_at(point, 4. / zoom.max(0.01)).map_err(|e| e.to_string())?;
        if mods.alt {
            self.preview = None;
            if mods.shift {
                if let Some(i) = self.hover_edge
                    && let Some(stroke) = g.graph.edge_paint(i, &g.sources, &g.spec).map_err(|e| e.to_string())?
                {
                    self.width = stroke.width;
                    self.paint = Some(stroke.paint);
                }
            } else if let Some(i) = self.hover_face {
                self.paint = g.graph.face_paint(i, &g.sources, &g.spec).map_err(|e| e.to_string())?;
            }
        } else if let Some(drag) = &self.drag
            && let Some(action) = self.action(drag, mods)
        {
            let preview = drag.gesture.preview(s, action).map_err(|e| e.to_string())?;
            self.preview = preview.changed.then(|| Arc::new(preview.display));
            self.key = self.key.wrapping_add(1) | (1 << 54);
        }
        Ok(())
    }
    /// Press coordinates own the transform until release, including outside the canvas.
    /// Modifier-only frames update the detached preview too.
    pub fn canvas_input(&mut self, s: &mut Session, response: &egui::Response, xf: &crate::canvas::ViewXform) -> bool {
        if self.tool.is_none() {
            return false;
        }
        self.frame(s, &response.ctx, true);
        let (pressed, released, down, pos, mods) =
            response.ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down(), i.pointer.latest_pos(), i.modifiers));
        if pressed
            && self.drag.is_none()
            && let Some(pos) = pos.filter(|p| response.rect.contains(*p) && response.ctx.layer_id_at(*p) == Some(response.layer_id))
        {
            match self.gesture(s) {
                Ok(gesture) => {
                    let p = xf.to_doc(pos);
                    self.drag = Some(Drag { gesture, points: vec![p], transform: *xf });
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(e);
                    return true;
                }
            }
        }
        let captured = self.drag.is_some();
        if let Some(pos) = pos.filter(|p| captured || response.rect.contains(*p)) {
            let p = self.drag.as_ref().map_or_else(|| xf.to_doc(pos), |d| d.transform.to_doc(pos));
            if let Some(d) = &mut self.drag
                && d.points.last() != Some(&p)
            {
                if d.points.len() >= engine::evaluator::MAX_POINTS {
                    self.error = Some("paint gesture point budget exceeded".into());
                    self.cancel();
                    return true;
                }
                d.points.push(p);
            }
            if let Err(e) = self.update(s, Point::new(p[0], p[1]), mods) {
                self.error = Some(e);
                self.preview = None;
            }
        } else if !down && !captured {
            self.hover_face = None;
            self.hover_edge = None;
        }
        if released && let Some(drag) = self.drag.take() {
            if self.error.is_none() && !mods.alt {
                if self.tool == Some(Tool::Selection) {
                    if !mods.shift {
                        self.selected_faces.clear();
                        self.selected_edges.clear();
                    }
                    // Selection near an edge selects that edge; interior clicks select faces.
                    let last = drag.points.last().copied().unwrap_or([0., 0.]);
                    let p = Point::new(last[0], last[1]);
                    match drag.gesture.graph.edge_at(p, 2. / f64::from(drag.transform.zoom).max(0.01)) {
                        Ok(Some(i)) => {
                            let sig = drag.gesture.graph.edges[i].signature.clone();
                            if !self.selected_edges.remove(&sig) {
                                self.selected_edges.insert(sig);
                            }
                        }
                        Ok(None) => match drag.gesture.graph.touched(&drag.points.iter().map(|p| Point::new(p[0], p[1])).collect::<Vec<_>>()) {
                            Ok(hits) => {
                                for i in hits {
                                    let sig = drag.gesture.graph.faces[i].signature.clone();
                                    if !self.selected_faces.remove(&sig) {
                                        self.selected_faces.insert(sig);
                                    }
                                }
                            }
                            Err(e) => self.error = Some(e.to_string()),
                        },
                        Err(e) => self.error = Some(e.to_string()),
                    }
                } else if let Some(action) = self.action(&drag, mods) {
                    match drag.gesture.commit(s, action) {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                self.targets = ids;
                            }
                            self.map = None;
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
            self.preview = None;
        }
        true
    }
    pub fn context_bar(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        use engine::{FillMode, Insertion};
        let tr = crate::i18n::t;
        ui.label(tr(match self.tool {
            Some(Tool::Flood) => "Vector Flood Fill",
            Some(Tool::Selection) => "Live Paint Selection",
            _ => "Live Paint Bucket",
        }));
        if self.tool == Some(Tool::Flood) {
            egui::ComboBox::from_id_salt("floodInsertion42")
                .selected_text(tr(match self.flood.insertion {
                    Insertion::Inside => "Inside",
                    Insertion::InBetween => "In-between",
                }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.flood.insertion, Insertion::Inside, tr("Inside"));
                    ui.selectable_value(&mut self.flood.insertion, Insertion::InBetween, tr("In-between"));
                });
            egui::ComboBox::from_id_salt("floodMode42")
                .selected_text(tr(match self.flood.mode {
                    FillMode::AddOnTop => "Add on top",
                    FillMode::SmartRefill => "Smart refill",
                    FillMode::Knockout => "Knockout",
                }))
                .show_ui(ui, |ui| {
                    for (mode, label) in [(FillMode::AddOnTop, "Add on top"), (FillMode::SmartRefill, "Smart refill"), (FillMode::Knockout, "Knockout")] {
                        ui.selectable_value(&mut self.flood.mode, mode, tr(label));
                    }
                });
            ui.checkbox(&mut self.flood.visible_boundaries, tr("Fill to Visible Boundaries"));
        } else {
            let label = ui.label(tr("Edge width"));
            ui.add(egui::DragValue::new(&mut self.width).range(0. ..=5000.)).labelled_by(label.id);
            egui::ComboBox::from_id_salt("gap42")
                .selected_text(tr(match self.gap {
                    Gap::None => "No Gaps",
                    Gap::Small => "Small Gaps",
                    Gap::Medium => "Medium Gaps",
                    Gap::Large => "Large Gaps",
                }))
                .show_ui(ui, |ui| {
                    for (gap, label) in [(Gap::None, "No Gaps"), (Gap::Small, "Small Gaps"), (Gap::Medium, "Medium Gaps"), (Gap::Large, "Large Gaps")] {
                        ui.selectable_value(&mut self.gap, gap, tr(label));
                    }
                });
            for (label, action) in [
                ("Make Live Paint", Action::Make { gap: self.gap }),
                ("Apply Gap Options", Action::Options { gap: self.gap }),
                ("Merge Live Paint", Action::Merge),
                ("Expand Live Paint", Action::Expand),
                ("Release Live Paint", Action::Release),
            ] {
                if ui.button(tr(label)).clicked() {
                    self.cancel();
                    let result = self.gesture(s).and_then(|g| g.commit(s, action).map_err(|e| e.to_string()));
                    match result {
                        Ok(ids) => self.set_targets(ids),
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
    }
    pub fn draw_overlay(&self, painter: &egui::Painter, to_screen: &impl Fn(Point) -> Pos2, zoom: f64) {
        let Some(g) = self.drag.as_ref().map(|d| &d.gesture).or(self.map.as_ref()) else {
            return;
        };
        let draw = |path: &photocraft_doc::Path, color| {
            for line in photocraft_vector::flatten_path(path, 0.25 / zoom.max(0.01)) {
                let mut p = line.pts.iter().map(|p| to_screen(Point::new(p.0, p.1))).collect::<Vec<_>>();
                if line.closed
                    && let Some(first) = p.first().copied()
                {
                    p.push(first);
                }
                painter.add(egui::Shape::line(p, Stroke::new(2., color)));
            }
        };
        for (i, f) in g.graph.faces.iter().enumerate() {
            if self.hover_face == Some(i) || self.selected_faces.contains(&f.signature) {
                draw(&f.region.path, Color32::LIGHT_BLUE);
            }
        }
        for (i, e) in g.graph.edges.iter().enumerate() {
            if self.hover_edge == Some(i) || self.selected_edges.contains(&e.signature) {
                draw(&e.path, Color32::YELLOW);
            }
        }
    }
}
/// Runtime context storage for the 00 app hook, never native document/settings storage.
pub fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(egui::Id::new("livePaint42")).unwrap_or_default())
}
pub fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("livePaint42"), state));
}

#[cfg(test)]
#[path = "live_paint/app_gpu.rs"]
mod app_gpu;
#[cfg(test)]
#[path = "live_paint/tests.rs"]
mod tests;
