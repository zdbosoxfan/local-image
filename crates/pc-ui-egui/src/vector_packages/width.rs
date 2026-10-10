//! Width-tool interaction adapted from VectorCraft crates/tools/src/distort/width.rs and
//! profile choices from crates/doc/src/appearance.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! State captures document/revision/targets; all previews are detached, one command on release.
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, Ui};
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::{
    Session,
    vector_packages::width::{self as engine, Gesture, WidthPoint, WidthProfile, geometry},
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub item: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub saved: Vec<geometry::SavedProfile>,
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if self.saved.len() > 64 {
            return Err("saved width profiles exceed the budget".into());
        }
        for saved in &self.saved {
            saved.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Selected {
    target: usize,
    subpath: usize,
    index: usize,
}
enum DragKind {
    Move,
    Width { left: bool },
    Graph { rect: Rect, scale: f64, left: bool },
}
struct Drag {
    origin: Gesture,
    selection: Selected,
    point: WidthPoint,
    center: Point,
    normal: (f64, f64),
    kind: DragKind,
    last: Pos2,
    moved: bool,
    added: bool,
}
pub struct State {
    pub options: Options,
    pub preview: Option<Document>,
    pub generation: u64,
    pub error: Option<String>,
    pub graph_rect: Rect,
    pub profile_name: String,
    gesture: Option<Gesture>,
    drag: Option<Drag>,
    selected: Option<Selected>,
    targets: Vec<LayerId>,
    epoch: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            options: Options::default(),
            preview: None,
            generation: 0,
            error: None,
            graph_rect: Rect::NOTHING,
            profile_name: String::new(),
            gesture: None,
            drag: None,
            selected: None,
            targets: Vec::new(),
            epoch: 0,
        }
    }
}
impl State {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.drag = None;
        self.preview = None;
        self.epoch = self.epoch.wrapping_add(1);
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    /// Call every frame, including when another persona/tool is active.
    pub fn lifecycle(&mut self, s: &Session, ids: &[LayerId], enabled: bool, focused: bool, escape: bool) {
        if self.targets != ids {
            self.cancel();
            self.selected = None;
            self.targets = ids.to_vec();
        }
        if !enabled || !focused || escape || self.gesture.as_ref().is_some_and(|g| !g.is_fresh(s)) {
            self.cancel();
        }
    }
    fn begin(&mut self, s: &Session, ids: &[LayerId]) -> Result<(), String> {
        if self.gesture.is_none() {
            self.gesture = Some(Gesture::begin(s, ids, self.options.item).map_err(|e| e.to_string())?);
        }
        Ok(())
    }
    fn show(&mut self, s: &Session) -> Result<(), String> {
        if let Some(g) = &self.gesture {
            self.preview = Some(engine::preview_document(s, g.request()).map_err(|e| e.to_string())?);
            self.generation = self.generation.wrapping_add(1) | (1 << 56);
        }
        Ok(())
    }
    fn release(&mut self, s: &mut Session) -> Result<(), String> {
        let g = self.gesture.take();
        self.cancel();
        if let Some(g) = g {
            g.commit(s).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn fail(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.error = Some(e);
            self.cancel();
        }
    }
    fn current(&self, s: &Session, ids: &[LayerId]) -> Result<Gesture, String> {
        if let Some(g) = &self.gesture { Ok(g.clone()) } else { Gesture::begin(s, ids, self.options.item).map_err(|e| e.to_string()) }
    }
    fn apply_profile(&mut self, s: &mut Session, ids: &[LayerId], p: Option<WidthProfile>) -> Result<(), String> {
        self.cancel();
        self.begin(s, ids)?;
        if let Some(g) = &mut self.gesture {
            for i in 0..ids.len() {
                g.set_profile(i, p.clone()).map_err(|e| e.to_string())?;
            }
        }
        self.release(s)
    }
    fn move_drag(&mut self, s: &Session, pos: Pos2, doc: Point, mods: Modifiers) -> Result<(), String> {
        let Some(d) = &mut self.drag else { return Ok(()) };
        // A click inserts the inherited widths. Widths change only once the pointer moves;
        // otherwise a centre-line click would unexpectedly pinch the stroke to zero.
        if !d.moved && pos == d.last && !matches!(d.kind, DragKind::Graph { .. }) {
            return Ok(());
        }
        let mut point = d.point;
        match d.kind {
            DragKind::Move => {
                let id = d.origin.request().edits[d.selection.target].layer;
                let layer = s.active().and_then(|st| st.doc.layer(id)).ok_or("width target is absent")?;
                let LayerContent::Shape(sh) = &layer.content else { return Err("width target is not a shape".into()) };
                let metric = geometry::Metric::new(&sh.path)?;
                let q = metric.nearest((doc.x, doc.y).into(), Some(d.selection.subpath)).ok_or("width path is empty")?;
                point.t = q.t;
            }
            DragKind::Width { left } => {
                let off = ((doc.x - d.center.x) * d.normal.0 + (doc.y - d.center.y) * d.normal.1).clamp(-geometry::MAX_WIDTH, geometry::MAX_WIDTH);
                if mods.alt {
                    if left {
                        point.left = off.max(0.0);
                    } else {
                        point.right = (-off).max(0.0);
                    }
                } else {
                    point.left = off.abs();
                    point.right = off.abs();
                }
            }
            DragKind::Graph { rect, scale, left } => {
                point.t = f64::from((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let w = (f64::from((rect.bottom() - pos.y) / rect.height()) * scale).clamp(0.0, geometry::MAX_WIDTH);
                if mods.alt {
                    if left {
                        point.left = w;
                    } else {
                        point.right = w;
                    }
                } else {
                    point.left = w;
                    point.right = w;
                }
            }
        }
        if mods.shift {
            point.t = (point.t * 100.0).round() / 100.0;
        }
        d.moved |= point != d.point;
        d.last = pos;
        let mut next = d.origin.clone();
        let i = next.set_point(d.selection.target, if d.added { None } else { Some(d.selection.index) }, point).map_err(|e| e.to_string())?;
        self.selected = Some(Selected { index: i, ..d.selection });
        self.gesture = Some(next);
        self.show(s)
    }
    fn down(&mut self, s: &Session, ids: &[LayerId], doc: Point, pos: Pos2, tolerance: f64) -> Result<(), String> {
        self.cancel();
        let origin = Gesture::begin(s, ids, self.options.item).map_err(|e| e.to_string())?;
        let mut best = None;
        for (target, e) in origin.request().edits.iter().enumerate() {
            let layer = s.active().and_then(|st| st.doc.layer(e.layer)).ok_or("width target is absent")?;
            let LayerContent::Shape(sh) = &layer.content else { continue };
            let metric = geometry::Metric::new(&sh.path)?;
            let p = origin.profile(target).map_err(|e| e.to_string())?;
            let half = engine::stroke(s, e.layer, Some(e.item)).map_err(|e| e.to_string())?.1.base.width as f64 / 2.0;
            let mut hit = None;
            // Only explicit width points have handles; implicit uniform endpoints are added on first edit.
            if e.profile.is_some() {
                for (index, point) in p.points.iter().enumerate() {
                    for subpath in 0..sh.path.subpaths.len() {
                        if let Some(q) = metric.sample(subpath, point.t) {
                            for (part, x, y) in [
                                (0, q.point.x, q.point.y),
                                (1, q.point.x + q.normal.x * point.left, q.point.y + q.normal.y * point.left),
                                (2, q.point.x - q.normal.x * point.right, q.point.y - q.normal.y * point.right),
                            ] {
                                let d = (doc.x - x).hypot(doc.y - y);
                                if d <= tolerance && hit.as_ref().is_none_or(|(distance, _, _, _, _)| d < *distance) {
                                    hit = Some((d, index, *point, q, part));
                                }
                            }
                        }
                    }
                }
            }
            let (distance, selection, point, q, kind, added) = if let Some((distance, index, point, q, part)) = hit {
                (
                    distance,
                    Selected { target, subpath: q.subpath, index },
                    point,
                    q,
                    if part == 0 { DragKind::Move } else { DragKind::Width { left: part == 1 } },
                    false,
                )
            } else if let Some(q) = metric.nearest((doc.x, doc.y).into(), None) {
                let (left, right) = geometry::at(&p, half, q.t);
                if q.distance > left.max(right) + tolerance {
                    continue;
                }
                let left_side = (doc.x - q.point.x) * q.normal.x + (doc.y - q.point.y) * q.normal.y >= 0.0;
                (
                    q.distance,
                    Selected { target, subpath: q.subpath, index: 0 },
                    WidthPoint { t: q.t, left, right },
                    q,
                    DragKind::Width { left: left_side },
                    true,
                )
            } else {
                continue;
            };
            if best.as_ref().is_none_or(|(d, _)| distance < *d) {
                best = Some((
                    distance,
                    Drag {
                        origin: origin.clone(),
                        selection,
                        point,
                        center: Point::new(q.point.x, q.point.y),
                        normal: (q.normal.x, q.normal.y),
                        kind,
                        last: pos,
                        moved: false,
                        added,
                    },
                ));
            }
        }
        if let Some((_, d)) = best {
            let mut next = origin;
            let index = if d.added { next.set_point(d.selection.target, None, d.point).map_err(|e| e.to_string())? } else { d.selection.index };
            self.selected = Some(Selected { index, ..d.selection });
            self.gesture = Some(next);
            let added = d.added;
            self.drag = Some(d);
            self.error = None;
            if added {
                self.show(s)?;
            }
        }
        Ok(())
    }
    pub fn delete_selected(&mut self, s: &mut Session, ids: &[LayerId]) -> Result<(), String> {
        let Some(selected) = self.selected else { return Ok(()) };
        self.cancel();
        self.begin(s, ids)?;
        if let Some(g) = &mut self.gesture {
            g.remove(selected.target, selected.index).map_err(|e| e.to_string())?;
        }
        self.selected = None;
        self.release(s)
    }
    /// Raw events retain capture outside the canvas. `enabled` includes persona/tool state.
    pub fn input(
        &mut self,
        s: &mut Session,
        ctx: &egui::Context,
        canvas: Rect,
        to_doc: impl Fn(Pos2) -> Point,
        ids: &[LayerId],
        enabled: bool,
        tolerance: f64,
    ) {
        let events = ctx.input(|i| i.events.clone());
        let cancel = events.iter().any(|e| matches!(e, Event::PointerGone | Event::WindowFocused(false) | Event::Key { key: Key::Escape, pressed: true, .. }));
        self.lifecycle(s, ids, enabled, ctx.input(|i| i.focused), cancel);
        if !enabled || cancel || !ctx.input(|i| i.focused) {
            return;
        }
        for event in events {
            let result = match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if canvas.contains(pos) => {
                    self.down(s, ids, to_doc(pos), pos, tolerance)
                }
                Event::PointerMoved(pos) if self.drag.as_ref().is_some_and(|d| !matches!(d.kind, DragKind::Graph { .. })) => {
                    self.move_drag(s, pos, to_doc(pos), ctx.input(|i| i.modifiers))
                }
                Event::ModifiersChanged(mods) if self.drag.as_ref().is_some_and(|d| !matches!(d.kind, DragKind::Graph { .. })) => {
                    let pos = self.drag.as_ref().map_or(Pos2::ZERO, |d| d.last);
                    self.move_drag(s, pos, to_doc(pos), mods)
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers }
                    if self.drag.as_ref().is_some_and(|d| !matches!(d.kind, DragKind::Graph { .. })) =>
                {
                    let update = self.move_drag(s, pos, to_doc(pos), modifiers);
                    if update.is_ok() && self.drag.as_ref().is_some_and(|d| d.moved || d.added) {
                        self.release(s)
                    } else {
                        self.cancel();
                        update
                    }
                }
                Event::Key { key: Key::Delete | Key::Backspace, pressed: true, .. } if !ctx.egui_wants_keyboard_input() => self.delete_selected(s, ids),
                _ => Ok(()),
            };
            self.fail(result);
        }
    }
    pub fn paint_handles(&self, s: &Session, ids: &[LayerId], painter: &egui::Painter, to_screen: impl Fn(Point) -> Pos2) {
        let Ok(g) = self.current(s, ids) else { return };
        for (target, e) in g.request().edits.iter().enumerate() {
            let Some(layer) = s.active().and_then(|st| st.doc.layer(e.layer)) else { continue };
            let LayerContent::Shape(sh) = &layer.content else { continue };
            let Ok(metric) = geometry::Metric::new(&sh.path) else { continue };
            let Some(p) = &e.profile else { continue };
            for (index, point) in p.points.iter().enumerate() {
                for sub in 0..sh.path.subpaths.len() {
                    let Some(q) = metric.sample(sub, point.t) else { continue };
                    let center = to_screen(Point::new(q.point.x, q.point.y));
                    let a = to_screen(Point::new(q.point.x + q.normal.x * point.left, q.point.y + q.normal.y * point.left));
                    let b = to_screen(Point::new(q.point.x - q.normal.x * point.right, q.point.y - q.normal.y * point.right));
                    let color = if self.selected.is_some_and(|p| p.target == target && p.index == index) { Color32::LIGHT_BLUE } else { Color32::WHITE };
                    painter.line_segment([a, b], Stroke::new(1.0, color));
                    for p in [a, center, b] {
                        painter.circle(p, 3.5, color, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
        }
    }
    /// Stroke panel pressure graph, presets, numeric point editor, and persisted saved profiles.
    pub fn panel(&mut self, ui: &mut Ui, s: &mut Session, ids: &[LayerId]) {
        if let Err(error) = self.options.validate() {
            self.cancel();
            self.error = Some(error);
            ui.colored_label(Color32::LIGHT_RED, self.error.as_deref().unwrap_or_default());
            return;
        }
        let cancel = ui.input(|i| i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, Event::PointerGone | Event::WindowFocused(false))));
        self.lifecycle(s, ids, true, ui.input(|i| i.focused), cancel);
        if cancel {
            return;
        }
        let Ok(current) = self.current(s, ids) else {
            ui.label(tl!("Select stroked shapes"));
            return;
        };
        let Ok(profile) = current.profile(0) else { return };
        ui.heading(tl!("Pressure"));
        let mut preset_choice = None;
        egui::ComboBox::from_label(tl!("Width profile")).selected_text(tl!("Profile presets")).show_ui(ui, |ui| {
            for (id, label) in geometry::PRESETS {
                if ui.selectable_label(false, tl!(label)).clicked() {
                    preset_choice = Some(id);
                }
            }
        });
        if let Some(id) = preset_choice {
            self.cancel();
            let result = self.begin(s, ids).and_then(|_| {
                if let Some(g) = &mut self.gesture {
                    g.preset(id).map_err(|e| e.to_string())?;
                }
                self.selected = None;
                self.release(s)
            });
            self.fail(result);
        }
        let max = profile.points.iter().flat_map(|p| [p.left, p.right]).fold(10.0, f64::max) * 1.25;
        let scale = self.drag.as_ref().and_then(|d| if let DragKind::Graph { scale, .. } = d.kind { Some(scale) } else { None }).unwrap_or(max);
        let (response, painter) = ui.allocate_painter(egui::vec2(280.0, 120.0), Sense::click_and_drag());
        self.graph_rect = response.rect.shrink(6.0);
        let rect = self.graph_rect;
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
        let screen = |p: &WidthPoint, left: bool| {
            egui::pos2(rect.left() + rect.width() * p.t as f32, rect.bottom() - rect.height() * ((if left { p.left } else { p.right }) / scale) as f32)
        };
        for left in [true, false] {
            let color = if left { Color32::LIGHT_BLUE } else { Color32::LIGHT_RED };
            for w in profile.points.windows(2) {
                painter.line_segment([screen(&w[0], left), screen(&w[1], left)], Stroke::new(1.0, color));
            }
            for p in &profile.points {
                painter.circle_filled(screen(p, left), 4.0, color);
            }
        }
        let events = ui.input(|i| i.events.clone());
        for event in events {
            let result = match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if rect.contains(pos) => {
                    self.cancel();
                    let closest = profile
                        .points
                        .iter()
                        .enumerate()
                        .flat_map(|(i, p)| [true, false].map(move |left| (screen(p, left).distance(pos), i, left)))
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    let hit = closest.filter(|h| h.0 <= 8.0);
                    let (index, point, left, added) = if let Some((_, i, left)) = hit {
                        (i, profile.points[i], left, false)
                    } else {
                        (0, WidthPoint { t: f64::from((pos.x - rect.left()) / rect.width()), left: 0.0, right: 0.0 }, true, true)
                    };
                    self.begin(s, ids).and_then(|_| {
                        if let Some(g) = &self.gesture {
                            let selection = Selected { target: 0, subpath: 0, index };
                            self.drag = Some(Drag {
                                origin: g.clone(),
                                selection,
                                point,
                                center: Point::new(0.0, 0.0),
                                normal: (0.0, 0.0),
                                kind: DragKind::Graph { rect, scale, left },
                                last: pos,
                                moved: false,
                                added,
                            });
                            self.selected = Some(selection);
                        }
                        self.move_drag(s, pos, Point::new(0.0, 0.0), ui.input(|i| i.modifiers))
                    })
                }
                Event::PointerMoved(pos) if self.drag.as_ref().is_some_and(|d| matches!(d.kind, DragKind::Graph { .. })) => {
                    self.move_drag(s, pos, Point::new(0.0, 0.0), ui.input(|i| i.modifiers))
                }
                Event::ModifiersChanged(mods) if self.drag.as_ref().is_some_and(|d| matches!(d.kind, DragKind::Graph { .. })) => {
                    let pos = self.drag.as_ref().map_or(Pos2::ZERO, |d| d.last);
                    self.move_drag(s, pos, Point::new(0.0, 0.0), mods)
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers }
                    if self.drag.as_ref().is_some_and(|d| matches!(d.kind, DragKind::Graph { .. })) =>
                {
                    self.move_drag(s, pos, Point::new(0.0, 0.0), modifiers).and_then(|_| self.release(s))
                }
                _ => Ok(()),
            };
            self.fail(result);
        }
        if let Some(selected) = self.selected {
            let p = self.current(s, ids).and_then(|g| g.profile(selected.target).map_err(|e| e.to_string()));
            if let Ok(p) = p
                && let Some(point) = p.points.get(selected.index)
            {
                let mut point = *point;
                let mut changed = false;
                let mut capture = false;
                ui.push_id(self.epoch, |ui| {
                    ui.horizontal(|ui| {
                        for (label, value, range) in [
                            ("Position", &mut point.t, 0.0..=1.0),
                            ("Left width", &mut point.left, 0.0..=geometry::MAX_WIDTH),
                            ("Right width", &mut point.right, 0.0..=geometry::MAX_WIDTH),
                        ] {
                            let label = ui.label(tl!(label));
                            let r = ui.add(egui::DragValue::new(value).range(range).speed(0.1)).labelled_by(label.id);
                            changed |= r.changed();
                            capture |= r.gained_focus() || r.drag_started();
                        }
                    })
                });
                if capture || changed {
                    let result = self.begin(s, ids).and_then(|_| {
                        if changed && let Some(g) = &mut self.gesture {
                            let i = g.set_point(selected.target, Some(selected.index), point).map_err(|e| e.to_string())?;
                            self.selected = Some(Selected { index: i, ..selected });
                            self.show(s)?;
                        }
                        Ok(())
                    });
                    self.fail(result);
                }
                if ui.button(tl!("Delete width point")).clicked() {
                    let result = self.delete_selected(s, ids);
                    self.fail(result);
                }
            }
        }
        // Context::input holds the context write lock; read focus before entering it.
        let keyboard_input = ui.ctx().egui_wants_keyboard_input();
        if self.drag.is_none() && self.gesture.is_some() && ui.input(|i| !i.pointer.any_down() && (!keyboard_input || i.key_pressed(Key::Enter))) {
            let changed = self.preview.is_some();
            if changed {
                let result = self.release(s);
                self.fail(result);
            } else {
                self.cancel();
            }
        }
        let label = ui.label(tl!("Profile name"));
        ui.text_edit_singleline(&mut self.profile_name).labelled_by(label.id);
        if ui.button(tl!("Save width profile")).clicked() {
            let result = self.current(s, ids).and_then(|g| g.profile(0).map_err(|e| e.to_string())).and_then(|profile| {
                let saved = geometry::SavedProfile { name: self.profile_name.trim().to_owned(), profile };
                saved.validate()?;
                if self.options.saved.len() >= 64 && !self.options.saved.iter().any(|p| p.name == saved.name) {
                    return Err("saved width profiles exceed the budget".into());
                }
                self.options.saved.retain(|p| p.name != saved.name);
                self.options.saved.push(saved);
                Ok(())
            });
            self.fail(result);
        }
        let mut apply = None;
        let mut remove = None;
        for (i, p) in self.options.saved.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(&p.name).clicked() {
                    apply = Some(p.clone());
                }
                if ui.small_button(tl!("Remove saved profile")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(saved) = apply {
            let result = saved.validate().and_then(|_| self.apply_profile(s, ids, Some(saved.profile)));
            self.fail(result);
        }
        if let Some(i) = remove {
            self.options.saved.remove(i);
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
    }
}
#[cfg(test)]
#[path = "width/tests.rs"]
mod tests;
