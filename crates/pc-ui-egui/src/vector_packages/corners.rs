//! Corner controls. Gesture/radius-bisector interaction adapted from VectorCraft
//! crates/tools/src/corners.rs at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0; see licenses/vectorcraft-LICENSE-{MIT,APACHE}.
//! Shared tool/router registration lives outside this module.

use egui::{Color32, Modifiers, Pos2, Stroke};
use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
use photocraft_doc::{Document, LayerId};
use photocraft_engine::{
    Session,
    vector_packages::corners::{
        CornerGesture,
        geometry::{self, CornerHandle, CornerKey},
    },
};
use photocraft_geom::Point;
use std::sync::Arc;

fn radius_unit(session: &Session) -> (photocraft_engine::prefs::Unit, f64, &'static str) {
    use photocraft_doc::vector_design::structure::DesignUnits;
    use photocraft_engine::prefs::Unit;
    let Some(st) = session.active() else { return (Unit::Pixels, 72.0, " px") };
    let (unit, suffix) = match st.doc.design_units {
        DesignUnits::Pixels => (Unit::Pixels, " px"),
        DesignUnits::Points => (Unit::Points, " pt"),
        DesignUnits::Millimeters => (Unit::Millimeters, " mm"),
        DesignUnits::Inches => (Unit::Inches, " in"),
    };
    (unit, st.doc.resolution_dpi.max(1.0) as f64, suffix)
}

#[derive(Clone, Copy, Debug)]
struct Capture {
    origin: Pos2,
    start: Point,
    zoom: f64,
    camera: crate::view_camera::Camera,
    last: Point,
    modifiers: Modifiers,
}
#[derive(Clone, Debug)]
struct Drag {
    gesture: CornerGesture,
    initial: Vec<KnotCorner>,
    selected: Vec<CornerKey>,
    handle: CornerHandle,
    start: Point,
    radius: f64,
    kind: CornerKind,
    moved: bool,
}
#[derive(Clone, Debug)]
pub struct State {
    pub layer: Option<LayerId>,
    /// Set by the persona/tool router; defaults off and is not persisted.
    pub active: bool,
    /// BASE indices from Direct Select. Empty selection plus `all` targets every eligible corner.
    pub selected: Vec<CornerKey>,
    pub kind: CornerKind,
    pub radius: f64,
    pub all: bool,
    pub error: Option<String>,
    drag: Option<Drag>,
    capture: Option<Capture>,
    preview: Option<Arc<Document>>,
    preview_key: u64,
    controls_dirty: bool,
    controls_epoch: u64,
    controls_capture: Option<CornerGesture>,
    controls_keys: Vec<CornerKey>,
    committed: Option<(photocraft_doc::DocId, u64, Arc<Document>)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            layer: None,
            active: false,
            selected: Vec::new(),
            kind: CornerKind::Round,
            radius: 0.0,
            all: true,
            error: None,
            drag: None,
            capture: None,
            preview: None,
            preview_key: 1 << 57,
            controls_dirty: false,
            controls_epoch: 0,
            controls_capture: None,
            controls_keys: Vec::new(),
            committed: None,
        }
    }
}
impl State {
    pub fn set_target(&mut self, layer: Option<LayerId>, selected: Vec<CornerKey>) {
        if self.layer != layer || self.selected != selected {
            self.cancel();
        }
        self.layer = layer;
        self.selected = selected;
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) {
        // A fresh widget ID abandons egui's old numeric edit buffer as well as our snapshot.
        self.controls_epoch = self.controls_epoch.wrapping_add(1);
        self.drag = None;
        self.capture = None;
        self.preview = None;
        self.controls_capture = None;
        self.controls_keys.clear();
        self.controls_dirty = false;
    }
    pub fn cancel_if_stale(&mut self, session: &Session) {
        if self.drag.as_ref().is_some_and(|d| !d.gesture.is_current(session)) || self.controls_capture.as_ref().is_some_and(|g| !g.is_current(session)) {
            self.cancel();
        }
    }
    /// Explicit cancellation hook: Escape, focus loss, persona/tool switch, document switch.
    pub fn lifecycle(&mut self, session: &Session, active: bool, focused: bool, escape: bool) {
        if !active || !focused || escape {
            self.cancel();
        } else {
            self.cancel_if_stale(session);
        }
    }
    pub fn preview_generation(&self) -> u64 {
        self.preview_key
    }
    pub fn preview_document(&self) -> Option<&Document> {
        self.preview.as_deref()
    }
    pub fn display_document(&self, session: &Session) -> Option<(Arc<Document>, u64)> {
        let d = self.drag.as_ref()?;
        if !d.gesture.is_current(session) {
            return None;
        }
        Some((self.preview.clone()?, self.preview_key))
    }
    /// Revision-keyed projection for the actual app's CPU/GPU canvas and Navigator. Called
    /// from canvas::display_doc before its ordinary committed-document fallback.
    pub fn display_for_document(&mut self, session: &Session, index: usize) -> Result<Option<(Arc<Document>, u64)>, String> {
        let Some(st) = session.documents().get(index) else { return Ok(None) };
        if session.active().is_some_and(|active| active.doc.id == st.doc.id)
            && let Some(preview) = self.display_document(session)
        {
            return Ok(Some(preview));
        }
        fn any(layers: &[photocraft_doc::Layer]) -> bool {
            layers.iter().any(|l| !l.vector.corners.is_empty() || l.children().is_some_and(any))
        }
        if !any(&st.doc.layers) {
            return Ok(None);
        }
        if self.committed.as_ref().is_none_or(|(id, revision, _)| *id != st.doc.id || *revision != st.revision) {
            let projected = photocraft_engine::vector_packages::corners::project_document(&st.doc).map_err(|e| e.to_string())?;
            self.committed = Some((st.doc.id, st.revision, Arc::new(projected)));
        }
        Ok(self.committed.as_ref().map(|(_, _, doc)| (doc.clone(), 0)))
    }
    pub fn widgets(&self, session: &Session, zoom: f64) -> Vec<(CornerHandle, Point)> {
        let Some(st) = session.active() else { return Vec::new() };
        let Some(layer) = self.layer.and_then(|id| st.doc.layer(id)) else { return Vec::new() };
        let photocraft_doc::LayerContent::Shape(shape) = &layer.content else { return Vec::new() };
        if st.doc.effective_locks(layer.id).all || st.doc.effective_locks(layer.id).pixels {
            return Vec::new();
        }
        let Ok(handles) = geometry::handles(&shape.path) else { return Vec::new() };
        let metadata = self
            .drag
            .as_ref()
            .filter(|d| d.gesture.is_current(session))
            .and_then(|d| d.gesture.edits.iter().find(|e| e.layer == layer.id))
            .map_or(&layer.vector.corners, |e| &e.vector.corners);
        handles
            .into_iter()
            .map(|h| {
                let radius = metadata.iter().find(|c| c.subpath == h.key.subpath && c.knot == h.key.knot).map_or(0.0, |c| c.corner.radius);
                (h, h.widget(radius, zoom))
            })
            .collect()
    }
    /// Read real egui pointer input before painting. Capture keeps the original document
    /// transform across relayout, zoom/flip changes and a release outside the viewport.
    pub fn canvas_input(&mut self, session: &mut Session, response: &egui::Response, xf: &crate::canvas::ViewXform) -> bool {
        if !self.active {
            return false;
        }
        self.cancel_if_stale(session);
        let ctx = &response.ctx;
        let (pressed, released, down, pos, modifiers, focused) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos().or(i.pointer.interact_pos()),
                i.modifiers,
                i.focused && !i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))),
            )
        });
        if !focused {
            self.cancel();
            return false;
        }
        if self.capture.is_none() && pressed {
            let origin = ctx.input(|i| i.pointer.press_origin()).or(pos);
            if let Some(origin) = origin.filter(|p| xf.rect.contains(*p) && ctx.layer_id_at(*p) == Some(response.layer_id)) {
                let [x, y] = xf.to_doc(origin);
                let start = Point::new(x, y);
                self.pointer(session, crate::canvas::ToolEvent::Down { x, y, pressure: 1.0 }, modifiers, xf.zoom as f64);
                if self.is_dragging() {
                    self.capture = Some(Capture { origin, start, zoom: xf.zoom as f64, camera: xf.camera(), last: start, modifiers });
                }
            }
        }
        let Some(mut capture) = self.capture else {
            return false;
        };
        let at = pos.map_or(capture.last, |p| {
            let delta = capture.camera.document_delta(p - capture.origin);
            Point::new(capture.start.x + f64::from(delta.x), capture.start.y + f64::from(delta.y))
        });
        if released {
            self.pointer(session, crate::canvas::ToolEvent::Up { x: at.x, y: at.y }, modifiers, capture.zoom);
            self.capture = None;
        } else if down && (at != capture.last || modifiers != capture.modifiers) {
            self.pointer(session, crate::canvas::ToolEvent::Move { x: at.x, y: at.y, pressure: 1.0 }, modifiers, capture.zoom);
            capture.last = at;
            capture.modifiers = modifiers;
            if self.is_dragging() {
                self.capture = Some(capture);
            }
        } else if !down {
            self.cancel();
        }
        true
    }
    fn chosen(&self, handles: &[CornerHandle], hit: CornerKey) -> Vec<CornerKey> {
        if self.all {
            handles.iter().map(|h| h.key).collect()
        } else if self.selected.contains(&hit) {
            self.selected.clone()
        } else {
            vec![hit]
        }
    }
    fn update_drag(&mut self, session: &Session, point: Point, modifiers: Modifiers) -> Result<(), String> {
        let Some(drag) = self.drag.as_mut() else { return Ok(()) };
        if !drag.gesture.is_current(session) {
            self.cancel();
            return Err("corner gesture became stale".into());
        }
        let delta = Point::new(point.x - drag.start.x, point.y - drag.start.y);
        drag.moved |= delta.x.hypot(delta.y) > 1e-6;
        let mut radius = (drag.radius + drag.handle.radius_change(delta)).clamp(0.0, drag.handle.max_radius);
        // Read modifiers on every frame. Shift quantises document-pixel radius.
        if modifiers.shift {
            radius = radius.round().min(drag.handle.max_radius);
        }
        let layer = drag.gesture.edits[0].layer;
        let mut corners = drag.initial.clone();
        for key in &drag.selected {
            corners.retain(|c| (c.subpath, c.knot) != (key.subpath, key.knot));
            corners.push(KnotCorner { subpath: key.subpath, knot: key.knot, corner: Corner { radius, kind: drag.kind } });
        }
        drag.gesture.set(layer, corners).map_err(|e| e.to_string())?;
        self.preview = Some(Arc::new(drag.gesture.preview_document().map_err(|e| e.to_string())?));
        self.preview_key = self.preview_key.wrapping_add(1) | (1 << 57);
        self.radius = radius;
        Ok(())
    }
    /// Coordinates and hit tolerance are in document pixels. Release uses its actual position.
    /// The caller owns pointer capture and continues Move/Up delivery outside the canvas.
    pub fn pointer(&mut self, session: &mut Session, event: crate::canvas::ToolEvent, modifiers: Modifiers, zoom: f64) -> bool {
        self.cancel_if_stale(session);
        let result = match event {
            crate::canvas::ToolEvent::Down { x, y, .. } => {
                let widgets = self.widgets(session, zoom);
                let p = Point::new(x, y);
                let hit = widgets
                    .iter()
                    .filter(|(_, q)| (q.x - x).hypot(q.y - y) <= 6.0 / zoom.max(0.01))
                    .min_by(|(_, a), (_, b)| (a.x - x).hypot(a.y - y).total_cmp(&(b.x - x).hypot(b.y - y)))
                    .map(|(h, _)| *h);
                let Some(handle) = hit else { return false };
                let Some(layer) = self.layer else { return false };
                match CornerGesture::begin(session, &[layer]) {
                    Ok(gesture) => {
                        let initial = gesture.edits[0].vector.corners.clone();
                        let stored = initial.iter().find(|c| (c.subpath, c.knot) == (handle.key.subpath, handle.key.knot));
                        let radius = stored.map_or(0.0, |c| c.corner.radius.min(handle.max_radius));
                        let kind = stored.map_or(self.kind, |c| c.corner.kind);
                        let selected = self.chosen(&widgets.iter().map(|(h, _)| *h).collect::<Vec<_>>(), handle.key);
                        if !self.all {
                            self.selected = selected.clone();
                        }
                        self.kind = kind;
                        self.radius = radius;
                        self.drag = Some(Drag { gesture, initial, selected, handle, start: p, radius, kind, moved: false });
                        self.error = None;
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            crate::canvas::ToolEvent::Move { x, y, .. } if self.drag.is_some() => self.update_drag(session, Point::new(x, y), modifiers),
            crate::canvas::ToolEvent::Up { x, y } if self.drag.is_some() => {
                let result = self.update_drag(session, Point::new(x, y), modifiers);
                if result.is_ok() {
                    if let Some(d) = self.drag.take().filter(|d| d.moved) { d.gesture.commit(session).map(|_| ()).map_err(|e| e.to_string()) } else { Ok(()) }
                } else {
                    result
                }
            }
            _ => return false,
        };
        if let Err(e) = result {
            self.error = Some(e);
            self.cancel();
        }
        if matches!(event, crate::canvas::ToolEvent::Up { .. }) {
            self.preview = None;
        }
        true
    }
    fn apply_controls(&mut self, session: &mut Session, set_radius: bool) -> Result<(), String> {
        let capture = self.controls_capture.take();
        let captured_keys = std::mem::take(&mut self.controls_keys);
        self.cancel();
        let layer = self.layer.ok_or("select a shape layer")?;
        let mut g = match capture {
            Some(g) => g,
            None => CornerGesture::begin(session, &[layer]).map_err(|e| e.to_string())?,
        };
        let hs = geometry::handles(&g.edits[0].shape.path).map_err(|e| e.to_string())?;
        let selected: Vec<_> = if !captured_keys.is_empty() {
            captured_keys
        } else if self.all {
            hs.iter().map(|h| h.key).collect()
        } else {
            self.selected.clone()
        };
        if selected.is_empty() {
            return Err("select corner nodes or enable All corners".into());
        }
        let mut corners = g.edits[0].vector.corners.clone();
        for key in selected {
            let radius =
                if set_radius { self.radius } else { corners.iter().find(|c| (c.subpath, c.knot) == (key.subpath, key.knot)).map_or(0.0, |c| c.corner.radius) };
            corners.retain(|c| (c.subpath, c.knot) != (key.subpath, key.knot));
            corners.push(KnotCorner { subpath: key.subpath, knot: key.knot, corner: Corner { radius, kind: self.kind } });
        }
        g.set(layer, corners).map_err(|e| e.to_string())?;
        g.commit(session).map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Context controls commit on a completed edit/choice; canvas dragging previews continuously.
    pub fn context_bar(&mut self, ui: &mut egui::Ui, session: &mut Session) {
        self.cancel_if_stale(session);
        if !self.controls_dirty
            && self.drag.is_none()
            && let Some(layer) = session.active().and_then(|st| self.layer.and_then(|id| st.doc.layer(id)))
        {
            let corner = layer.vector.corners.iter().find(|c| self.all || self.selected.contains(&CornerKey { subpath: c.subpath, knot: c.knot }));
            self.radius = corner.map_or(0.0, |c| c.corner.radius);
            if let Some(corner) = corner {
                self.kind = corner.corner.kind;
            }
        }
        if ui.checkbox(&mut self.all, li_i18n::t("All corners")).changed() {
            self.cancel();
        }
        let old = self.kind;
        let previous_radius = self.radius;
        egui::ComboBox::new("corner-kind", li_i18n::t("Corner type")).selected_text(li_i18n::t(kind_label(self.kind))).show_ui(ui, |ui| {
            for kind in [CornerKind::Round, CornerKind::Inverted, CornerKind::Chamfer, CornerKind::Straight] {
                ui.selectable_value(&mut self.kind, kind, li_i18n::t(kind_label(kind)));
            }
        });
        let mut apply = old != self.kind;
        let label = ui.label(li_i18n::t("Radius"));
        let (unit, dpi, suffix) = radius_unit(session);
        let mut displayed_radius = unit.from_px(self.radius, dpi, 1.0, 72.0);
        let max = unit.from_px(geometry::MAX_RADIUS, dpi, 1.0, 72.0);
        let response = ui
            .push_id(("corner-radius", self.controls_epoch), |ui| {
                ui.add(egui::DragValue::new(&mut displayed_radius).range(0.0..=max).speed(0.25).suffix(suffix)).labelled_by(label.id)
            })
            .inner;
        if response.changed() {
            self.radius = unit.to_px(displayed_radius, dpi, 1.0, 72.0);
        }
        if !self.radius.is_finite() {
            self.radius = previous_radius;
            self.controls_dirty = false;
            self.error = Some(li_i18n::t("Corner radius must be finite").into());
            return;
        }
        if (response.changed() || response.gained_focus() || response.drag_started()) && self.controls_capture.is_none() {
            match self
                .layer
                .ok_or_else(|| "select a shape layer".to_owned())
                .and_then(|layer| CornerGesture::begin(session, &[layer]).map_err(|e| e.to_string()))
            {
                Ok(g) => {
                    let hs = geometry::handles(&g.edits[0].shape.path);
                    if let Ok(hs) = hs {
                        self.controls_keys = if self.all { hs.iter().map(|h| h.key).collect() } else { self.selected.clone() };
                    }
                    self.controls_capture = Some(g);
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
        self.controls_dirty |= response.changed();
        apply |= self.controls_dirty && (response.drag_stopped() || response.lost_focus());
        if apply {
            let set_radius = self.controls_dirty;
            self.error = self.apply_controls(session, set_radius).err();
        }
        if response.lost_focus() && !self.controls_dirty {
            self.controls_capture = None;
            self.controls_keys.clear();
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
    }
    pub fn draw_overlay(&self, session: &Session, painter: &egui::Painter, to_screen: &dyn Fn(Point) -> Pos2, zoom: f64) {
        for (h, p) in self.widgets(session, zoom) {
            let selected = self.all || self.selected.contains(&h.key);
            painter.circle(to_screen(p), 4.0, if selected { Color32::from_rgb(50, 140, 240) } else { Color32::WHITE }, Stroke::new(1.0, Color32::BLACK));
        }
    }
}
pub fn kind_label(kind: CornerKind) -> &'static str {
    match kind {
        CornerKind::Round => "Round",
        CornerKind::Inverted => "Inverted",
        CornerKind::Chamfer => "Chamfer",
        CornerKind::Straight => "Straight",
    }
}

#[cfg(test)]
#[path = "corners/tests.rs"]
mod tests;
