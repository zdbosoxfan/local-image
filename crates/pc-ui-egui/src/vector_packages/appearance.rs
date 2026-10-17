//! Appearance / Stroke panels and Fill / Transparency canvas gestures.
//! Paint edits use the engine's captured document/revision/target transaction. State and preview
//! are transient; the app routes preview into the existing CPU/GPU canvas and cancellation.
use egui::{Color32, Event, Key, PointerButton, Pos2, Rect, Sense, Stroke, Ui};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, Arrowhead, FillItem, StrokeGradientMode, StrokeItem};
use photocraft_doc::{BlendMode, Color, Document, Fill, GradientStyle, LayerContent, LayerId, LineCap, LineJoin, ShapeStroke, StrokeAlign};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::appearance::{self as engine, Gesture},
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    #[default]
    Fill,
    Transparency,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub item: usize,
    pub selected_stop: Option<usize>,
    pub tool: Tool,
}
struct Drag {
    fixed: Point,
    end: Point,
    moving_start: bool,
    origin: Gesture,
    last_pointer: Point,
}
#[derive(Default)]
pub struct State {
    pub options: Options,
    pub preview: Option<Document>,
    pub generation: u64,
    pub gesture: Option<Gesture>,
    drag: Option<Drag>,
}
fn error(s: &str) -> EngineError {
    EngineError::Other(s.into())
}
impl State {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.preview = None;
        self.drag = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        if ((!enabled && self.drag.is_some()) || self.gesture.as_ref().is_some_and(|g| !g.is_fresh(s))) && self.gesture.is_some() {
            self.cancel();
        }
    }
    fn preview(&mut self, s: &Session) -> Result<()> {
        if let Some(g) = &self.gesture {
            match engine::preview_document(s, g.request()) {
                Ok(p) => {
                    self.preview = Some(p);
                    self.generation = self.generation.wrapping_add(1);
                }
                Err(e) => {
                    self.cancel();
                    return Err(e);
                }
            }
        }
        Ok(())
    }
    fn edit(&mut self, s: &Session, ids: &[LayerId], f: impl FnMut(&mut Appearance) -> Result<()>) -> Result<()> {
        self.synchronize(s, true);
        if self.gesture.is_none() {
            self.gesture = Some(Gesture::begin(s, ids)?);
        }
        if let Some(g) = &mut self.gesture
            && let Err(e) = g.update(f)
        {
            self.cancel();
            return Err(e);
        }
        self.preview(s)
    }
    pub fn release(&mut self, s: &mut Session) -> Result<()> {
        let g = self.gesture.take();
        self.cancel();
        if let Some(g) = g {
            g.commit(s)?;
        }
        Ok(())
    }
    fn current(&self, s: &Session, ids: &[LayerId]) -> Result<Appearance> {
        if let Some(g) = &self.gesture {
            return g.request().edits.first().map(|e| e.appearance.clone()).ok_or_else(|| error("no appearance target"));
        }
        Ok(Gesture::begin(s, ids)?.request().edits[0].appearance.clone())
    }
    pub fn begin_handle(&mut self, s: &Session, ids: &[LayerId], at: Point, tolerance: f64) -> Result<()> {
        self.cancel();
        if !at.x.is_finite() || !at.y.is_finite() || !tolerance.is_finite() || tolerance <= 0. {
            return Err(error("invalid paint handle capture"));
        }
        let g = Gesture::begin(s, ids)?;
        let count = g.request().edits[0].appearance.items.len();
        if self.options.item >= count {
            self.options.item = count.saturating_sub(1);
            self.options.selected_stop = None;
        }
        let (start, end) = handles(s, g.request().edits[0].layer, self.options.item, self.options.tool)?;
        let near = |p: Point| (at.x - p.x).hypot(at.y - p.y) <= tolerance;
        let (fixed, end, moving_start) = if near(start) {
            (end, start, true)
        } else if near(end) {
            (start, end, false)
        } else {
            (at, at, false)
        };
        self.gesture = Some(g.clone());
        self.drag = Some(Drag { fixed, end, moving_start, origin: g, last_pointer: at });
        Ok(())
    }
    pub fn motion_handle(&mut self, s: &Session, at: Point, shift: bool) -> Result<()> {
        self.synchronize(s, true);
        let Some(drag) = &mut self.drag else { return Ok(()) };
        drag.last_pointer = at;
        let mut at = at;
        if !at.x.is_finite() || !at.y.is_finite() || at.x.abs() > 1e6 || at.y.abs() > 1e6 {
            self.cancel();
            return Err(error("paint handle coordinates must be finite and bounded"));
        }
        if shift {
            let (dx, dy) = (at.x - drag.fixed.x, at.y - drag.fixed.y);
            let len = dx.hypot(dy);
            let angle = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            at = Point::new(drag.fixed.x + len * angle.cos(), drag.fixed.y + len * angle.sin());
        }
        drag.end = at;
        let (start, end) = if drag.moving_start { (at, drag.fixed) } else { (drag.fixed, at) };
        if (start.x - end.x).hypot(start.y - end.y) < 0.5 {
            return Ok(());
        }
        let mut next = drag.origin.clone();
        let mut index = 0;
        let ids: Vec<_> = next.request().edits.iter().map(|e| e.layer).collect();
        let item = self.options.item;
        let tool = self.options.tool;
        let updated = next.update(|a| {
            let st = s.active().ok_or(EngineError::NoDocument)?;
            let layer = st.doc.layer(ids[index]).ok_or(EngineError::NoLayer(ids[index]))?;
            index += 1;
            let LayerContent::Shape(sh) = &layer.content else {
                return Err(error("paint handles require shapes"));
            };
            let bounds = sh.path.control_bounds().ok_or_else(|| error("paint handles require a bounded path"))?;
            let p = paint_mut(a, item)?;
            if !matches!(p, Fill::Gradient { .. }) {
                let color = match p {
                    Fill::Solid(c) => *c,
                    _ => Color::BLACK,
                };
                *p = Fill::gradient(
                    vec![(0., color), (1., if tool == Tool::Transparency { color } else { Color::WHITE })],
                    0.,
                    1.,
                    GradientStyle::Linear,
                    false,
                );
            }
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let angle = (-dy).atan2(dx);
            let w = (bounds.2 - bounds.0).max(1.);
            let h = (bounds.3 - bounds.1).max(1.);
            let chord = (w / angle.cos().abs().max(1e-6)).min(h / angle.sin().abs().max(1e-6)).max(1.);
            if let Fill::Gradient { angle: a, scale, style, offset, align, opacity_stops, .. } = p {
                *a = angle.to_degrees() as f32;
                if tool == Tool::Transparency {
                    *style = GradientStyle::Linear;
                }
                let len = dx.hypot(dy);
                let center = if *style == GradientStyle::Linear { Point::new((start.x + end.x) / 2., (start.y + end.y) / 2.) } else { start };
                let diagonal = ((angle.cos() * w).powi(2) + (angle.sin() * h).powi(2)).sqrt().max(1.);
                *scale = match *style {
                    GradientStyle::Linear => (len / chord) as f32,
                    GradientStyle::Reflected => (2. * len / chord) as f32,
                    _ => (2. * len / diagonal) as f32,
                };
                *align = true;
                *offset = ((center.x - (bounds.0 + bounds.2) / 2.) as f32 / w as f32, (center.y - (bounds.1 + bounds.3) / 2.) as f32 / h as f32);
                if tool == Tool::Transparency {
                    *opacity_stops = vec![(0., 0.), (1., 1.)];
                }
            }
            Ok(())
        });
        if let Err(e) = updated {
            self.cancel();
            return Err(e);
        }
        self.gesture = Some(next);
        self.preview(s)
    }
}
fn paint_mut(a: &mut Appearance, item: usize) -> Result<&mut Fill> {
    match a.items.get_mut(item) {
        Some(AppearanceItem::Fill(f)) => Ok(&mut f.paint),
        Some(AppearanceItem::Mesh(_)) => Err(error("use the Mesh tool to edit mesh paint")),
        Some(AppearanceItem::Stroke(s)) => Ok(&mut s.base.paint),
        None => Err(error("appearance item is absent")),
    }
}
/// Handles in document coordinates; the current model frame matches the V0 layout convention.
pub fn handles(s: &Session, id: LayerId, item: usize, _tool: Tool) -> Result<(Point, Point)> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Shape(sh) = &layer.content else { return Err(error("paint handles require a shape")) };
    let mut a = layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(sh));
    let p = paint_mut(&mut a, item)?;
    let b = sh.path.control_bounds().ok_or_else(|| error("paint handles need a path frame"))?;
    let (angle, scale, offset, align, style) = match p {
        Fill::Gradient { angle, scale, offset, align, style, .. } => (*angle, *scale, *offset, *align, *style),
        _ => (0., 1., (0., 0.), true, GradientStyle::Linear),
    };
    let b = if align {
        b
    } else {
        let r = st.doc.bounds();
        (f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1))
    };
    let w = (b.2 - b.0).max(1.);
    let h = (b.3 - b.1).max(1.);
    let a = f64::from(angle).to_radians();
    let span = match style {
        GradientStyle::Linear | GradientStyle::Reflected => (w / a.cos().abs().max(1e-6)).min(h / a.sin().abs().max(1e-6)),
        _ => ((a.cos() * w).powi(2) + (a.sin() * h).powi(2)).sqrt().max(1.),
    };
    let len = span * f64::from(scale) / 2.;
    let c = Point::new((b.0 + b.2) / 2. + f64::from(offset.0) * w, (b.1 + b.3) / 2. + f64::from(offset.1) * h);
    Ok((
        if style == GradientStyle::Linear { Point::new(c.x - a.cos() * len, c.y + a.sin() * len) } else { c },
        Point::new(c.x + a.cos() * len, c.y - a.sin() * len),
    ))
}

/// Call on every frame, including while disabled, to cancel on focus/persona/tool changes.
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &egui::Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> Point,
    ids: &[LayerId],
    enabled: bool,
    tolerance: f64,
) -> Result<()> {
    state.synchronize(s, enabled);
    let events = ctx.input(|i| i.events.clone());
    if ctx.input(|i| !i.focused)
        || events.iter().any(|e| matches!(e, Event::PointerGone | Event::WindowFocused(false) | Event::Key { key: Key::Escape, pressed: true, .. }))
    {
        state.cancel();
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }
    for e in events {
        match e {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if canvas.contains(pos) => {
                state.begin_handle(s, ids, to_doc(pos), tolerance)?;
            }
            Event::ModifiersChanged(_) if state.drag.is_some() => {
                let at = state.drag.as_ref().map(|d| d.last_pointer).ok_or_else(|| error("missing paint pointer capture"))?;
                state.motion_handle(s, at, ctx.input(|i| i.modifiers.shift))?;
            }
            Event::PointerMoved(pos) if state.drag.is_some() => {
                state.motion_handle(s, to_doc(pos), ctx.input(|i| i.modifiers.shift))?;
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers } if state.drag.is_some() => {
                state.motion_handle(s, to_doc(pos), modifiers.shift)?;
                if state.preview.is_some() {
                    state.release(s)?;
                } else {
                    state.cancel();
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub fn paint_handles(state: &State, s: &Session, ids: &[LayerId], painter: &egui::Painter, to_screen: impl Fn(Point) -> Pos2) {
    let endpoints = state
        .drag
        .as_ref()
        .map(|d| if d.moving_start { (d.end, d.fixed) } else { (d.fixed, d.end) })
        .or_else(|| ids.first().and_then(|id| handles(s, *id, state.options.item, state.options.tool).ok()));
    if let Some((a, b)) = endpoints {
        let (a, b) = (to_screen(a), to_screen(b));
        painter.line_segment([a, b], Stroke::new(1., Color32::LIGHT_BLUE));
        for p in [a, b] {
            painter.circle_filled(p, 4., Color32::WHITE);
            painter.circle_stroke(p, 4., Stroke::new(1., Color32::BLACK));
        }
    }
}

fn combo<T: Copy + PartialEq>(ui: &mut Ui, key: &str, value: &mut T, choices: &[(T, &str)]) -> bool {
    let old = *value;
    let selected = choices.iter().find(|c| c.0 == old).map_or("", |c| c.1);
    egui::ComboBox::from_label(tl!(key)).selected_text(tl!(selected)).show_ui(ui, |ui| {
        for &(v, label) in choices {
            ui.selectable_value(value, v, tl!(label));
        }
    });
    old != *value
}
fn paint_editor(ui: &mut Ui, p: &mut Fill, doc: &Document, selected_stop: &mut Option<usize>) -> bool {
    let mut kind = match p {
        Fill::Solid(_) => 0,
        Fill::Gradient { .. } => 1,
        Fill::Pattern { .. } => 2,
    };
    let mut changed = combo(ui, "Paint", &mut kind, &[(0, "Solid"), (1, "Gradient"), (2, "Pattern")]);
    if changed {
        *p = match kind {
            1 => Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., GradientStyle::Linear, false),
            2 => {
                let pat = doc.patterns.first();
                Fill::Pattern {
                    name: pat.map_or_else(String::new, |p| p.name.clone()),
                    id: pat.map_or_else(String::new, |p| p.id.clone()),
                    scale: 1.,
                    angle: 0.,
                    link: true,
                    phase: (0., 0.),
                }
            }
            _ => Fill::Solid(Color::BLACK),
        };
    }
    match p {
        Fill::Solid(c) => {
            let rgb = c.to_rgb();
            let mut rgba = [rgb[0], rgb[1], rgb[2], c.alpha];
            if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                *c = Color::rgb(rgba[0], rgba[1], rgba[2]);
                c.alpha = rgba[3];
                changed = true;
            }
        }
        Fill::Gradient { stops, angle, scale, style, reverse, opacity_stops, midpoints, offset, dither, align } => {
            changed |= combo(
                ui,
                "Gradient style",
                style,
                &[
                    (GradientStyle::Linear, "Linear"),
                    (GradientStyle::Radial, "Radial"),
                    (GradientStyle::Angle, "Angle"),
                    (GradientStyle::Reflected, "Reflected"),
                    (GradientStyle::Diamond, "Diamond"),
                ],
            );
            changed |= ui.add(egui::DragValue::new(angle).range(-360000.0..=360000.0).prefix(tl!("Angle"))).changed();
            changed |= ui.add(egui::DragValue::new(scale).range(0.001..=10000.).prefix(tl!("Scale"))).changed();
            changed |= ui.checkbox(reverse, tl!("Reverse")).changed();
            changed |= ui.checkbox(dither, tl!("Dither")).changed();
            changed |= ui.checkbox(align, tl!("Align with layer")).changed();
            changed |= ui.add(egui::DragValue::new(&mut offset.0).range(-10000.0..=10000.0).prefix(tl!("Offset X"))).changed();
            changed |= ui.add(egui::DragValue::new(&mut offset.1).range(-10000.0..=10000.0).prefix(tl!("Offset Y"))).changed();
            if selected_stop.is_some_and(|i| i >= stops.len()) {
                *selected_stop = None;
            }
            for (i, (t, c)) in stops.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(*selected_stop == Some(i), format!("{} {}", tl!("Stop"), i + 1)).clicked() {
                        *selected_stop = Some(i);
                    }
                    changed |= ui.add(egui::DragValue::new(t).range(0.0..=1.0).prefix(format!("{} {} ", tl!("Stop"), i + 1))).changed();
                    let rgb = c.to_rgb();
                    let mut rgba = [rgb[0], rgb[1], rgb[2], c.alpha];
                    if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                        *c = Color::rgb(rgba[0], rgba[1], rgba[2]);
                        c.alpha = rgba[3];
                        changed = true;
                    }
                });
            }
            if crate::widgets::secondary_button(ui, tl!("Add color stop"), 0.0).clicked() {
                stops.push((0.5, Color::WHITE));
                changed = true;
            }
            if crate::widgets::secondary_button(ui, tl!("Remove color stop"), 0.0).clicked() && !stops.is_empty() {
                stops.pop();
                changed = true;
            }
            for v in midpoints.iter_mut() {
                changed |= ui.add(egui::DragValue::new(v).range(0.0..=1.0).prefix(tl!("Midpoint"))).changed();
            }
            if crate::widgets::secondary_button(ui, tl!("Add midpoint"), 0.0).clicked() {
                midpoints.push(0.5);
                changed = true;
            }
            for (t, a) in opacity_stops.iter_mut() {
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.add(egui::DragValue::new(t).range(0.0..=1.0).prefix(tl!("Opacity stop"))).changed();
                    changed |= ui.add(egui::DragValue::new(a).range(0.0..=1.0).prefix(tl!("Opacity"))).changed();
                });
            }
            if crate::widgets::secondary_button(ui, tl!("Add opacity stop"), 0.0).clicked() {
                opacity_stops.push((0.5, 0.5));
                changed = true;
            }
        }
        Fill::Pattern { name, id, scale, angle, link, phase } => {
            egui::ComboBox::from_label(tl!("Pattern tile")).selected_text(name.as_str()).show_ui(ui, |ui| {
                for p in &doc.patterns {
                    if ui.selectable_label(p.id == *id, p.display_name()).clicked() {
                        *name = p.name.clone();
                        *id = p.id.clone();
                        changed = true;
                    }
                }
            });
            changed |= ui.add(egui::DragValue::new(scale).range(0.001..=10000.).prefix(tl!("Scale"))).changed();
            changed |= ui.add(egui::DragValue::new(angle).range(-360000.0..=360000.0).prefix(tl!("Angle"))).changed();
            changed |= ui.checkbox(link, tl!("Link with layer")).changed();
            changed |= ui.add(egui::DragValue::new(&mut phase.0).range(-1e6..=1e6).prefix(tl!("Phase X"))).changed();
            changed |= ui.add(egui::DragValue::new(&mut phase.1).range(-1e6..=1e6).prefix(tl!("Phase Y"))).changed();
        }
    }
    changed
}
fn stroke_editor(ui: &mut Ui, s: &mut StrokeItem) -> bool {
    let mut changed = false;
    changed |= ui.add(egui::Slider::new(&mut s.base.width, 0.0..=5000.).text(tl!("Width"))).changed();
    changed |= combo(
        ui,
        "Stroke alignment",
        &mut s.base.align,
        &[(StrokeAlign::Center, "Center"), (StrokeAlign::Inside, "Inside"), (StrokeAlign::Outside, "Outside")],
    );
    changed |= combo(ui, "Cap", &mut s.base.cap, &[(LineCap::Butt, "Butt"), (LineCap::Round, "Round"), (LineCap::Square, "Square")]);
    changed |= combo(ui, "Join", &mut s.base.join, &[(LineJoin::Miter, "Miter"), (LineJoin::Round, "Round"), (LineJoin::Bevel, "Bevel")]);
    changed |= ui.add(egui::DragValue::new(&mut s.base.miter_limit).range(1.0..=1000.).prefix(tl!("Miter limit"))).changed();
    for (i, d) in s.base.dashes.iter_mut().enumerate() {
        changed |= ui.add(egui::DragValue::new(d).range(0.0..=10000.).prefix(format!("{} {} ", tl!("Dash"), i + 1))).changed();
    }
    if crate::widgets::secondary_button(ui, tl!("Add dash pair"), 0.0).clicked() {
        s.base.dashes.extend([2., 2.]);
        changed = true;
    }
    if crate::widgets::secondary_button(ui, tl!("Solid stroke"), 0.0).clicked() {
        s.base.dashes.clear();
        changed = true;
    }
    changed |= ui.add(egui::DragValue::new(&mut s.base.dash_offset).range(-1e6..=1e6).prefix(tl!("Dash offset"))).changed();
    changed |= ui.checkbox(&mut s.dash_align, tl!("Align dashes to corners")).changed();
    changed |= ui.checkbox(&mut s.scale_with_object, tl!("Scale with object")).changed();
    let arrows = [
        (None, "None"),
        (Some(Arrowhead::Triangle), "Triangle"),
        (Some(Arrowhead::TriangleOpen), "Triangle open"),
        (Some(Arrowhead::Circle), "Circle"),
        (Some(Arrowhead::CircleOpen), "Circle open"),
        (Some(Arrowhead::Square), "Square"),
        (Some(Arrowhead::SquareOpen), "Square open"),
        (Some(Arrowhead::Bar), "Bar"),
        (Some(Arrowhead::Diamond), "Diamond"),
        (Some(Arrowhead::Arrow), "Arrow"),
        (Some(Arrowhead::ArrowOpen), "Arrow open"),
        (Some(Arrowhead::Barbed), "Barbed"),
    ];
    changed |= combo(ui, "Start arrow", &mut s.start_arrow, &arrows);
    changed |= combo(ui, "End arrow", &mut s.end_arrow, &arrows);
    changed |= ui.add(egui::DragValue::new(&mut s.arrow_scale[0]).range(0.001..=1000.).prefix(tl!("Start arrow scale"))).changed();
    changed |= ui.add(egui::DragValue::new(&mut s.arrow_scale[1]).range(0.001..=1000.).prefix(tl!("End arrow scale"))).changed();
    changed |= combo(
        ui,
        "Stroke gradient",
        &mut s.gradient_mode,
        &[(StrokeGradientMode::Object, "Object"), (StrokeGradientMode::Along, "Along"), (StrokeGradientMode::Across, "Across")],
    );
    changed
}

enum PanelChange {
    Add(AppearanceItem),
    Duplicate(usize),
    Delete(usize),
    Move(usize, usize),
    Visible(usize, bool),
    Item(usize, AppearanceItem),
}
impl PanelChange {
    fn apply(&self, a: &mut Appearance) -> Result<()> {
        match self {
            Self::Add(item) => a.items.push(item.clone()),
            Self::Duplicate(i) => {
                let item = a.items.get(*i).ok_or_else(|| error("appearance item is absent on a selected shape"))?.clone();
                a.items.insert(i + 1, item);
            }
            Self::Delete(i) => {
                if *i >= a.items.len() {
                    return Err(error("appearance item is absent on a selected shape"));
                }
                a.items.remove(*i);
            }
            Self::Move(i, j) => {
                if *i >= a.items.len() || *j >= a.items.len() {
                    return Err(error("appearance item is absent on a selected shape"));
                }
                a.items.swap(*i, *j);
            }
            Self::Visible(i, visible) => match a.items.get_mut(*i) {
                Some(AppearanceItem::Fill(f)) => f.visible = *visible,
                Some(AppearanceItem::Mesh(m)) => m.visible = *visible,
                Some(AppearanceItem::Stroke(s)) => s.visible = *visible,
                None => return Err(error("appearance item is absent on a selected shape")),
            },
            Self::Item(i, item) => {
                let target = a.items.get_mut(*i).ok_or_else(|| error("appearance item is absent on a selected shape"))?;
                if std::mem::discriminant(target) != std::mem::discriminant(item) {
                    return Err(error("selected appearance item kinds differ"));
                }
                *target = item.clone();
            }
        }
        Ok(())
    }
}

/// Selected shape IDs come from the existing selection registry. Every click commits one
/// command; continuous slider / color / numeric edits preview until pointer/key release.
pub fn panel(ui: &mut Ui, state: &mut State, s: &mut Session, ids: &[LayerId]) -> Result<()> {
    state.synchronize(s, true);
    if ui.input(|i| !i.focused || i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, Event::PointerGone | Event::WindowFocused(false)))) {
        state.cancel();
        return Ok(());
    }
    if ids.is_empty() && state.gesture.is_none() {
        ui.label(tl!("Select shape layers"));
        return Ok(());
    }
    let mut a = state.current(s, ids)?;
    let original = a.clone();
    ui.heading(tl!("Appearance"));
    let mut immediate = false;
    let mut changes = Vec::new();
    ui.horizontal_wrapped(|ui| {
        if crate::widgets::secondary_button(ui, tl!("Add fill"), 0.0).clicked() {
            let item = AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::BLACK)));
            changes.push(PanelChange::Add(item.clone()));
            a.items.push(item);
            state.options.item = a.items.len() - 1;
            immediate = true;
        }
        if crate::widgets::secondary_button(ui, tl!("Add stroke"), 0.0).clicked() {
            let item = AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width: 6., ..Default::default() }, ..Default::default() });
            changes.push(PanelChange::Add(item.clone()));
            a.items.push(item);
            state.options.item = a.items.len() - 1;
            immediate = true;
        }
    });
    for (i, item) in a.items.iter_mut().enumerate().rev() {
        ui.horizontal_wrapped(|ui| {
            let (name, visible) = match item {
                AppearanceItem::Fill(f) => (tl!("Fill"), &mut f.visible),
                AppearanceItem::Mesh(m) => (tl!("Gradient mesh"), &mut m.visible),
                AppearanceItem::Stroke(s) => (tl!("Stroke"), &mut s.visible),
            };
            if ui.selectable_label(state.options.item == i, format!("{name} {}", i + 1)).clicked() {
                state.options.item = i;
            }
            if crate::widgets::checkbox(ui, visible, &format!("{} {}", tl!("Visible"), i + 1)).changed() {
                changes.push(PanelChange::Visible(i, *visible));
                immediate = true;
            }
        });
    }
    let index = state.options.item.min(a.items.len().saturating_sub(1));
    state.options.item = index;
    let mut changed = false;
    if !a.items.is_empty() {
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::secondary_button(ui, tl!("Move up"), 0.0).clicked() && index + 1 < a.items.len() {
                changes.push(PanelChange::Move(index, index + 1));
                a.items.swap(index, index + 1);
                state.options.item += 1;
                immediate = true;
            }
            if crate::widgets::secondary_button(ui, tl!("Move down"), 0.0).clicked() && index > 0 {
                changes.push(PanelChange::Move(index, index - 1));
                a.items.swap(index, index - 1);
                state.options.item -= 1;
                immediate = true;
            }
            if crate::widgets::secondary_button(ui, tl!("Duplicate item"), 0.0).clicked() {
                changes.push(PanelChange::Duplicate(index));
                a.items.insert(index + 1, a.items[index].clone());
                state.options.item = index + 1;
                immediate = true;
            }
            if crate::widgets::secondary_button(ui, tl!("Delete item"), 0.0).clicked() {
                changes.push(PanelChange::Delete(index));
                a.items.remove(index);
                state.options.item = index.min(a.items.len().saturating_sub(1));
                immediate = true;
            }
        });
        let index = state.options.item;
        if let Some(item) = a.items.get_mut(index) {
            if let AppearanceItem::Mesh(mesh) = item {
                changed |= crate::widgets::slider_row(ui, "Item opacity", &mut mesh.opacity, 0.0..=1.0, "", None).changed();
                let modes: Vec<_> = BlendMode::LAYER_MODES.iter().map(|b| (*b, b.label())).collect();
                changed |= combo(ui, "Blend mode", &mut mesh.blend, &modes);
            } else if let Some((opacity, blend, paint)) = match item {
                AppearanceItem::Fill(f) => Some((&mut f.opacity, &mut f.blend, &mut f.paint)),
                AppearanceItem::Stroke(s) => Some((&mut s.base.opacity, &mut s.blend, &mut s.base.paint)),
                AppearanceItem::Mesh(_) => None,
            } {
                changed |= crate::widgets::slider_row(ui, "Item opacity", opacity, 0.0..=1.0, "", None).changed();
                let modes: Vec<_> = BlendMode::LAYER_MODES.iter().map(|b| (*b, b.label())).collect();
                changed |= combo(ui, "Blend mode", blend, &modes);
                if let Some(st) = s.active() {
                    changed |= paint_editor(ui, paint, &st.doc, &mut state.options.selected_stop);
                }
                if let AppearanceItem::Stroke(stroke) = item {
                    ui.heading(tl!("Stroke"));
                    changed |= stroke_editor(ui, stroke);
                }
            }
        }
    }
    if changed && let Some(item) = a.items.get(state.options.item) {
        changes.push(PanelChange::Item(state.options.item, item.clone()));
    }
    if (changed || immediate) && a != original {
        state.edit(s, ids, |next| {
            for change in &changes {
                change.apply(next)?;
            }
            Ok(())
        })?;
    }
    // Numeric keyboard entry stays detached while the editor has focus. Commit after Enter or
    // after losing focus; pointer drags commit on release, including outside the panel.
    let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
    let finish = ui.input(|i| !i.pointer.any_down() && (!wants_keyboard || i.key_pressed(Key::Enter)));
    if state.drag.is_none() && state.gesture.is_some() && (immediate || finish) {
        state.release(s)?;
    }
    Ok(())
}

/// An accessible drag surface for hosts that use response-based routing.
pub fn canvas_response(ui: &mut Ui, size: egui::Vec2) -> egui::Response {
    ui.allocate_response(size, Sense::click_and_drag())
}
