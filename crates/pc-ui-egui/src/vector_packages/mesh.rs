//! Mesh tool state machine adapted in behaviour from VectorCraft tools/meshblend.rs
//! and tools/meshedit.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors (MIT OR Apache-2.0).
//! Captures native IDs/revision on press; detached actions precede one engine release.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Ui};
use photocraft_color::convert::{linear_to_srgb, srgb_to_linear};
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::mesh::{self as engine, Commit, Gesture, Hooks, Options},
};
use photocraft_vector::mesh_paint::CompiledMesh;
use serde_json::json;

#[derive(Clone, Debug)]
pub enum Action {
    Preview(Commit),
    Commit(Commit),
    Cancel,
}
#[derive(Clone)]
struct Capture {
    gesture: Gesture,
    base: Gesture,
    layer: LayerId,
    index: usize,
    handle: Option<usize>,
    start: [f64; 2],
    anchor: [f64; 2],
    drag: bool,
    ids: Vec<LayerId>,
}
#[derive(Clone)]
pub struct State {
    pub options: Options,
    pub selected: Option<(LayerId, usize)>,
    pub color: [f32; 4],
    pub preview: Option<Document>,
    pub generation: u64,
    capture: Option<Capture>,
    color_gesture: Option<Gesture>,
}
impl Default for State {
    fn default() -> Self {
        Self { options: Options::default(), selected: None, color: [0., 0., 0., 1.], preview: None, generation: 0, capture: None, color_gesture: None }
    }
}
fn error(s: impl Into<String>) -> EngineError {
    EngineError::Other(s.into())
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
impl State {
    pub fn captured(&self) -> bool {
        self.capture.is_some() || self.color_gesture.is_some()
    }
    pub fn pending(&self) -> Option<&Commit> {
        self.capture.as_ref().map(|c| c.gesture.request()).or_else(|| self.color_gesture.as_ref().map(Gesture::request))
    }
    pub fn cancel(&mut self) {
        self.capture = None;
        self.color_gesture = None;
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn lifecycle(&mut self, s: &Session, ids: &[LayerId], enabled: bool, focused: bool, escape: bool) {
        let stale = self.capture.as_ref().is_some_and(|c| !c.gesture.is_fresh(s) || c.ids != ids)
            || self.color_gesture.as_ref().is_some_and(|g| !g.is_fresh(s) || g.request().edits.iter().any(|e| !ids.contains(&e.layer)));
        if !enabled || !focused && self.capture.is_some() || escape || stale {
            self.cancel();
        }
        if self.selected.is_some_and(|(id, _)| !ids.contains(&id)) {
            self.selected = None;
        }
    }
    fn down(&mut self, s: &Session, ids: &[LayerId], at: [f64; 2], mods: Modifiers, tolerance: f64, hooks: Hooks) -> Result<bool> {
        self.cancel();
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let mut gesture = Gesture::begin_with(s, ids, None, hooks)?;
        let mut hit = None;
        for e in gesture.request().edits.iter().rev() {
            for (i, p) in e.mesh.points.iter().enumerate() {
                if distance(p.position, at) <= tolerance {
                    hit = Some((e.layer, i, None, p.position));
                    break;
                }
            }
            if hit.is_none() {
                for (i, p) in e.mesh.points.iter().enumerate() {
                    for (h, offset) in p.handles.iter().enumerate() {
                        let end = [p.position[0] + offset[0], p.position[1] + offset[1]];
                        if distance(end, at) <= tolerance {
                            hit = Some((e.layer, i, Some(h), end));
                            break;
                        }
                    }
                    if hit.is_some() {
                        break;
                    }
                }
            }
            if hit.is_some() {
                break;
            }
        }
        let base = gesture.clone();
        let (layer, index, handle, anchor, drag) = if let Some((layer, index, handle, anchor)) = hit {
            if mods.alt {
                if handle.is_some() {
                    return Ok(false);
                }
                gesture.update(|id, m| if id == layer { m.delete(index) } else { Ok(()) })?;
                (layer, index, None, anchor, false)
            } else {
                (layer, index, handle, anchor, true)
            }
        } else {
            let mut found = None;
            for e in gesture.request().edits.iter().rev() {
                let layer = st.doc.layer(e.layer).ok_or(EngineError::NoLayer(e.layer))?;
                let LayerContent::Shape(shape) = &layer.content else {
                    return Err(error("mesh needs a shape"));
                };
                let rect = photocraft_geom::Rect::new(at[0].floor() as i32, at[1].floor() as i32, at[0].floor() as i32 + 1, at[1].floor() as i32 + 1);
                if photocraft_vector::fill_rasterizer(&shape.path, photocraft_vector::DEFAULT_TOLERANCE).render(rect).first().is_none_or(|a| *a <= 0.) {
                    continue;
                }
                let mesh = CompiledMesh::new(&e.mesh.geometry(), &shape.path, st.doc.bounds()).map_err(error)?;
                if let Some((r, c, u, v)) = mesh.locate(at) {
                    found = Some((e.layer, r, c, u, v));
                    break;
                }
            }
            let Some((layer, r, c, u, v)) = found else {
                return Ok(false);
            };
            let mut index = 0;
            let color = if mods.shift { None } else { Some(self.color) };
            gesture.update(|id, m| {
                if id == layer {
                    index = m.insert(r, c, u, v, color)?;
                }
                Ok(())
            })?;
            (layer, index, None, at, false)
        };
        self.selected = if mods.alt { None } else { Some((layer, index)) };
        self.capture = Some(Capture { gesture, base, layer, index, handle, start: at, anchor, drag, ids: ids.to_vec() });
        Ok(true)
    }
    fn motion(&mut self, at: [f64; 2], mods: Modifiers) -> Result<()> {
        let Some(c) = &mut self.capture else {
            return Ok(());
        };
        if !c.drag {
            return Ok(());
        }
        let mut delta = [at[0] - c.start[0], at[1] - c.start[1]];
        if mods.shift {
            if delta[0].abs() > delta[1].abs() {
                delta[1] = 0.;
            } else {
                delta[0] = 0.;
            }
        }
        let p = [c.anchor[0] + delta[0], c.anchor[1] + delta[1]];
        c.gesture.update(|id, m| if id == c.layer { m.move_point(c.index, c.handle, p) } else { Ok(()) })?;
        Ok(())
    }
    /// One input consumer. Returns requests so the host can use its normal display/error path.
    /// Low-level replay tests exercise this exact state machine without faking native storage.
    #[allow(clippy::too_many_arguments)]
    pub fn events(
        &mut self,
        s: &Session,
        ids: &[LayerId],
        events: &[Event],
        canvas: Rect,
        to_doc: impl Fn(Pos2) -> [f64; 2],
        mods: Modifiers,
        tolerance: f64,
        hooks: Hooks,
    ) -> Result<Vec<Action>> {
        let mut actions = Vec::new();
        let mut preview = false;
        for event in events {
            match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers } if canvas.contains(*pos) => {
                    if self.down(s, ids, to_doc(*pos), *modifiers, tolerance, hooks)?
                        && let Some(request) = self.pending()
                    {
                        let _ = request;
                        preview = true;
                    }
                }
                Event::PointerMoved(pos) if self.capture.is_some() => {
                    self.motion(to_doc(*pos), mods)?;
                    if let Some(request) = self.pending() {
                        let _ = request;
                        preview = true;
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers } if self.capture.is_some() => {
                    self.motion(to_doc(*pos), *modifiers)?;
                    if let Some(c) = self.capture.take()
                        && c.gesture.request() != c.base.request()
                    {
                        actions.push(Action::Commit(c.gesture.request().clone()));
                    }
                    preview = false;
                    self.preview = None;
                    self.generation = self.generation.wrapping_add(1);
                }
                Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) => {
                    self.cancel();
                    preview = false;
                    actions.clear();
                    actions.push(Action::Cancel);
                }
                _ => {}
            }
        }
        if preview && let Some(request) = self.pending() {
            actions.push(Action::Preview(request.clone()));
        }
        Ok(actions)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn input(
        &mut self,
        s: &mut Session,
        ids: &[LayerId],
        ctx: &egui::Context,
        canvas: Rect,
        to_doc: impl Fn(Pos2) -> [f64; 2],
        tolerance: f64,
        enabled: bool,
    ) -> Result<()> {
        let (mut events, mods, focus, escape) = ctx.input(|i| (i.events.clone(), i.modifiers, i.focused, i.key_pressed(Key::Escape)));
        let canvas_focus = crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None;
        let escape = escape && canvas_focus;
        if !canvas_focus { events.retain(|e| !matches!(e, Event::Key { key: Key::Escape, .. })); }
        self.lifecycle(s, ids, enabled, focus, escape);
        if !enabled || !focus || escape {
            return Ok(());
        }
        let result = self.events(s, ids, &events, canvas, to_doc, mods, tolerance, engine::NATIVE);
        let actions = match result {
            Ok(a) => a,
            Err(e) => {
                self.cancel();
                return Err(e);
            }
        };
        for action in actions {
            match action {
                Action::Preview(r) => match engine::preview_document(s, &r) {
                    Ok(doc) => {
                        self.preview = Some(doc);
                        self.generation = self.generation.wrapping_add(1);
                    }
                    Err(e) => {
                        self.cancel();
                        return Err(e);
                    }
                },
                Action::Commit(r) => {
                    self.cancel();
                    s.execute("object.mesh.set", json!(r))?;
                }
                Action::Cancel => self.cancel(),
            }
        }
        Ok(())
    }
    pub fn context_bar(&mut self, ui: &mut Ui, s: &mut Session, ids: &[LayerId]) -> Result<()> {
        ui.label(tl!("Mesh rows"));
        ui.add(egui::DragValue::new(&mut self.options.rows).range(1..=50));
        ui.label(tl!("Mesh columns"));
        ui.add(egui::DragValue::new(&mut self.options.cols).range(1..=50));
        for (a, label) in
            [(engine::Appearance::Flat, tl!("Flat")), (engine::Appearance::ToCenter, tl!("To Center")), (engine::Appearance::ToEdge, tl!("To Edge"))]
        {
            ui.selectable_value(&mut self.options.appearance, a, label);
        }
        ui.label(tl!("Highlight"));
        ui.add(egui::DragValue::new(&mut self.options.highlight).range(0. ..=100.).suffix("%"));
        if ui.button(tl!("Create Gradient Mesh…")).clicked() {
            self.cancel();
            let st = s.active().ok_or(EngineError::NoDocument)?;
            let request = engine::Create { document: st.doc.id.0, revision: st.revision, layers: ids.to_vec(), options: self.options.clone() };
            s.execute("object.mesh.create", json!(request))?;
        }
        Ok(())
    }
    /// Swatches and the existing eyedropper call this same point-colour command path.
    pub fn apply_color(&mut self, s: &mut Session, color: [f32; 4], opacity: f32) -> Result<()> {
        let (layer, index) = self.selected.ok_or_else(|| error("select a mesh point"))?;
        self.cancel();
        let mut g = Gesture::begin(s, &[layer], None)?;
        g.update(|_, m| m.set_color(index, color, opacity))?;
        g.commit(s)?;
        self.color = color;
        Ok(())
    }
    /// The same widget emits detached actions for production dispatch and headless replay.
    pub fn point_controls_with(&mut self, ui: &mut Ui, s: &Session, swatches: &[[f32; 4]], hooks: Hooks) -> Result<Vec<Action>> {
        let mut actions = Vec::new();
        let Some((layer, index)) = self.selected else {
            return Ok(actions);
        };
        let mesh = (hooks.read)(s.active().ok_or(EngineError::NoDocument)?.doc.layer(layer).ok_or(EngineError::NoLayer(layer))?)?
            .ok_or_else(|| error("select a gradient mesh"))?;
        let p = mesh.points.get(index).ok_or_else(|| error("mesh point no longer exists"))?;
        let mut color = p.rgba;
        let mut opacity = p.opacity;
        if let Some(g) = &self.color_gesture {
            let p = &g.request().edits[0].mesh.points[index];
            color = p.rgba;
            opacity = p.opacity;
        }
        ui.label(tl!("Mesh point colour"));
        // Egui's float picker is linear RGBA; editable mesh wire colours are sRGB.
        let mut linear = [srgb_to_linear(color[0]), srgb_to_linear(color[1]), srgb_to_linear(color[2]), color[3]];
        let changed = ui.color_edit_button_rgba_unmultiplied(&mut linear).changed();
        if changed {
            color = [linear_to_srgb(linear[0]), linear_to_srgb(linear[1]), linear_to_srgb(linear[2]), linear[3]];
        }
        ui.label(tl!("Mesh point opacity"));
        let changed = ui.add(egui::Slider::new(&mut opacity, 0. ..=1.)).changed() || changed;
        for &swatch in swatches {
            let c =
                egui::Color32::from_rgba_unmultiplied((swatch[0] * 255.) as u8, (swatch[1] * 255.) as u8, (swatch[2] * 255.) as u8, (swatch[3] * 255.) as u8);
            if ui.add(egui::Button::new(tl!("Apply mesh swatch")).fill(c)).clicked() {
                self.cancel();
                let mut g = Gesture::begin_with(s, &[layer], None, hooks)?;
                g.update(|_, m| m.set_color(index, swatch, opacity))?;
                self.color = swatch;
                actions.push(Action::Commit(g.request().clone()));
                return Ok(actions);
            }
        }
        if changed {
            let mut g = match self.color_gesture.take() {
                Some(g) => g,
                None => Gesture::begin_with(s, &[layer], None, hooks)?,
            };
            g.update(|_, m| m.set_color(index, color, opacity))?;
            self.color = color;
            actions.push(Action::Preview(g.request().clone()));
            self.color_gesture = Some(g);
        }
        if ui.input(|i| i.pointer.any_released() || i.key_pressed(Key::Enter))
            && let Some(g) = self.color_gesture.take()
        {
            actions.push(Action::Commit(g.request().clone()));
        }
        Ok(actions)
    }
    pub fn point_controls(&mut self, ui: &mut Ui, s: &mut Session, swatches: &[[f32; 4]]) -> Result<()> {
        let actions = match self.point_controls_with(ui, s, swatches, engine::NATIVE) {
            Ok(actions) => actions,
            Err(e) => {
                self.cancel();
                return Err(e);
            }
        };
        for action in actions {
            match action {
                Action::Preview(r) => {
                    match engine::preview_document(s, &r) {
                        Ok(doc) => self.preview = Some(doc),
                        Err(e) => {
                            self.cancel();
                            return Err(e);
                        }
                    }
                    self.generation = self.generation.wrapping_add(1);
                }
                Action::Commit(r) => {
                    self.cancel();
                    s.execute("object.mesh.set", json!(r))?;
                }
                Action::Cancel => self.cancel(),
            }
        }
        Ok(())
    }
    pub fn overlay(&self, s: &Session, ids: &[LayerId], painter: &egui::Painter, to_screen: impl Fn([f64; 2]) -> Pos2) -> Result<()> {
        let st = s.active().ok_or(EngineError::NoDocument)?;
        for id in ids {
            let mesh = self
                .pending()
                .and_then(|r| r.edits.iter().find(|e| e.layer == *id).map(|e| e.mesh.clone()))
                .map_or_else(|| (engine::NATIVE.read)(st.doc.layer(*id).ok_or(EngineError::NoLayer(*id))?), |m| Ok(Some(m)))?;
            let Some(mesh) = mesh else {
                continue;
            };
            let mesh = mesh.geometry();
            mesh.validate().map_err(error)?;
            let line = egui::Stroke::new(1., egui::Color32::from_rgb(230, 80, 190));
            for r in 0..mesh.rows {
                for c in 0..mesh.cols {
                    let patch = mesh.patch(r, c).map_err(error)?;
                    for vertical in [false, true] {
                        let points = (0..=16)
                            .map(|i| {
                                let t = i as f64 / 16.;
                                to_screen(if vertical { patch.point(0., t) } else { patch.point(t, 0.) })
                            })
                            .collect();
                        painter.add(egui::Shape::line(points, line));
                    }
                    if r == mesh.rows - 1 {
                        painter.add(egui::Shape::line((0..=16).map(|i| to_screen(patch.point(i as f64 / 16., 1.))).collect(), line));
                    }
                    if c == mesh.cols - 1 {
                        painter.add(egui::Shape::line((0..=16).map(|i| to_screen(patch.point(1., i as f64 / 16.))).collect(), line));
                    }
                }
            }
            for (i, p) in mesh.points.iter().enumerate() {
                let at = to_screen(p.position);
                painter.circle_stroke(at, 3., line);
                if self.selected == Some((*id, i)) {
                    for h in p.handles {
                        let end = to_screen([p.position[0] + h[0], p.position[1] + h[1]]);
                        painter.line_segment([at, end], line);
                        painter.circle_filled(end, 2., line.color);
                    }
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "mesh/tests.rs"]
mod tests;
