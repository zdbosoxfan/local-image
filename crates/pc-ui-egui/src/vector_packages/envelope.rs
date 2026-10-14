//! Envelope controls and mesh handle capture. Original Local Image UI.
//! 00 owns menu/persona/canvas routing; see integration/20.md for exact hooks.
use crate::canvas::ViewXform;
use egui::{Color32, Key, Stroke};
use photocraft_doc::{
    DocId, Document, LayerId, Path,
    vector_design::live::{EnvelopeSpec, LiveGroup},
};
use photocraft_engine::{
    Session,
    vector_packages::envelope::{self as engine, Gesture, geometry},
};
use std::sync::Arc;
fn focused(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.focused && !i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))))
}
#[derive(Clone, Debug)]
struct Drag {
    index: usize,
    initial: [f64; 2],
    start: [f64; 2],
    xf: ViewXform,
    moved: bool,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub target: Option<LayerId>,
    pub error: Option<String>,
    pending: Option<Gesture>,
    preview: Option<Arc<Document>>,
    preview_key: u64,
    drag: Option<Drag>,
    committed: Option<(DocId, u64, Arc<Document>)>,
    controls_epoch: u64,
}
impl State {
    pub fn set_target(&mut self, target: Option<LayerId>) {
        if self.target != target {
            self.cancel();
        }
        self.target = target;
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.preview = None;
        self.drag = None;
        self.preview_key = self.preview_key.wrapping_add(1);
        self.controls_epoch = self.controls_epoch.wrapping_add(1);
    }
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn lifecycle(&mut self, s: &Session, active: bool, focused: bool, escape: bool) {
        if !active || escape || self.pending.as_ref().is_some_and(|g| !g.is_current(s)) {
            self.cancel();
        }
        if !focused {
            self.drag = None;
        }
    }
    pub fn display_document(&self, s: &Session) -> Option<(Arc<Document>, u64)> {
        if self.pending.as_ref()?.is_current(s) { Some((self.preview.clone()?, self.preview_key)) } else { None }
    }
    /// Canvas/Navigator display hook. Source revisions invalidate all envelope dependencies,
    /// including ordinary path/type source edits, native load and undo/redo.
    /// Called when font/paint provider generation changes without a document edit.
    pub fn invalidate_projection(&mut self) {
        self.cancel();
        self.committed = None;
    }
    pub fn display_for_document(&mut self, s: &Session, index: usize) -> Result<Option<(Arc<Document>, u64)>, String> {
        let Some(st) = s.documents().get(index) else { return Ok(None) };
        if s.active().is_some_and(|a| a.doc.id == st.doc.id)
            && let Some(d) = self.display_document(s)
        {
            return Ok(Some(d));
        }
        if !st.doc.walk().into_iter().any(|(_, _, l)| matches!(l.vector.live, Some(LiveGroup::Envelope(_)))) {
            return Ok(None);
        }
        if self.committed.as_ref().is_none_or(|(id, r, _)| *id != st.doc.id || *r != st.revision) {
            self.committed = Some((st.doc.id, st.revision, Arc::new(engine::project_document(&st.doc).map_err(|e| e.to_string())?)));
        }
        Ok(self.committed.as_ref().map(|(_, _, d)| (d.clone(), 0)))
    }
    fn spec(&self, s: &Session) -> Option<EnvelopeSpec> {
        if let Some(g) = &self.pending {
            return g.request.edits.first().map(|e| e.spec.clone());
        }
        s.active()
            .and_then(|st| self.target.and_then(|id| st.doc.layer(id)))
            .and_then(|l| if let Some(LiveGroup::Envelope(spec)) = &l.vector.live { Some(spec.clone()) } else { None })
            .or_else(|| geometry::identity_mesh(2, 2).ok())
    }
    fn begin(&mut self, s: &Session) -> Result<(), String> {
        if self.pending.is_none() {
            self.pending = Some(Gesture::begin(s, &[self.target.ok_or("Select an envelope group")?]).map_err(|e| e.to_string())?);
        }
        Ok(())
    }
    fn update(&mut self, s: &Session, spec: EnvelopeSpec) -> Result<(), String> {
        self.begin(s)?;
        let mut candidate = self.pending.clone().ok_or("Envelope capture missing")?;
        candidate.update(spec).map_err(|e| e.to_string())?;
        let doc = engine::preview_document(s, &candidate.request).map_err(|e| e.to_string())?;
        self.pending = Some(candidate);
        self.preview = Some(Arc::new(doc));
        self.preview_key = self.preview_key.wrapping_add(1) | (1 << 56);
        Ok(())
    }
    fn apply(&mut self, s: &mut Session) -> Result<(), String> {
        let pending = self.pending.as_ref().cloned().ok_or("Envelope capture missing")?;
        pending.commit(s).map_err(|e| e.to_string())?;
        self.cancel();
        self.committed = None;
        Ok(())
    }
    /// Top object is a detached outline supplied by the selection/menu router. The source
    /// layer remains editable; this API never deletes it as an implicit side effect.
    pub fn set_top_object(&mut self, s: &Session, path: Path) -> Result<(), String> {
        self.update(s, EnvelopeSpec::TopObject { path })
    }
    pub fn context_bar(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        self.lifecycle(
            s,
            true,
            focused(ui.ctx()),
            crate::shortcut_dispatch::Focus::of(ui.ctx()) == crate::shortcut_dispatch::Focus::None && ui.ctx().input(|i| i.key_pressed(Key::Escape)),
        );
        ui.push_id(self.controls_epoch, |ui| self.controls(ui, s));
    }
    fn controls(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        let ready = self.target.is_some();
        let Some(mut spec) = self.spec(s) else { return };
        let mut kind = match spec {
            EnvelopeSpec::Warp { .. } => 0,
            EnvelopeSpec::Mesh { .. } => 1,
            EnvelopeSpec::TopObject { .. } => 2,
        };
        let before = kind;
        ui.add_enabled_ui(ready, |ui| {
            egui::ComboBox::new("envelope-kind", li_i18n::t("Envelope kind"))
                .selected_text(li_i18n::t(match kind {
                    0 => "Warp",
                    1 => "Mesh",
                    _ => "Top object",
                }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut kind, 0, li_i18n::t("Warp"));
                    ui.selectable_value(&mut kind, 1, li_i18n::t("Mesh"));
                    ui.add_enabled(false, egui::Label::new(li_i18n::t("Top object"))).on_hover_text(li_i18n::t("Choose an outline from the Object menu"));
                });
        });
        if kind != before {
            spec = if kind == 0 {
                EnvelopeSpec::Warp { style: "arc".into(), bend: 0.0, horizontal: 0.0, vertical: 0.0 }
            } else {
                match geometry::identity_mesh(2, 2) {
                    Ok(s) => s,
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                }
            };
        }
        let previous = spec.clone();
        match &mut spec {
            EnvelopeSpec::Warp { style, bend, horizontal, vertical } => {
                egui::ComboBox::new("envelope-style", li_i18n::t("Warp style")).selected_text(li_i18n::t(style_label(style))).show_ui(ui, |ui| {
                    for (id, label) in STYLES {
                        ui.selectable_value(style, id.to_string(), li_i18n::t(label));
                    }
                });
                for (label, value) in [("Bend", bend), ("Horizontal distortion", horizontal), ("Vertical distortion", vertical)] {
                    let l = ui.label(li_i18n::t(label));
                    ui.add(egui::DragValue::new(value).range(-100.0..=100.0).speed(0.5).suffix("%")).labelled_by(l.id);
                }
            }
            EnvelopeSpec::Mesh { rows, cols, .. } => {
                let l = ui.label(li_i18n::t("Rows"));
                ui.add(egui::DragValue::new(rows).range(1..=geometry::MAX_GRID_SIDE)).labelled_by(l.id);
                let l = ui.label(li_i18n::t("Columns"));
                ui.add(egui::DragValue::new(cols).range(1..=geometry::MAX_GRID_SIDE)).labelled_by(l.id);
                if let EnvelopeSpec::Mesh { rows: r, cols: c, .. } = &previous
                    && (*r != *rows || *c != *cols)
                {
                    match geometry::identity_mesh(*rows, *cols) {
                        Ok(s) => spec = s,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
            EnvelopeSpec::TopObject { .. } => {
                ui.label(li_i18n::t("Top object"));
            }
        }
        if ready && (before != kind || previous != spec) {
            if let Err(e) = self.update(s, spec) {
                self.error = Some(e);
            } else {
                self.error = None;
            }
        }
        if ui.add_enabled(self.pending.is_some() && self.drag.is_none() && self.error.is_none(), egui::Button::new(li_i18n::t("Apply envelope"))).clicked()
            && let Err(e) = self.apply(s)
        {
            self.error = Some(e);
        }
        if ui.button(li_i18n::t("Cancel envelope")).clicked() {
            self.cancel();
            self.error = None;
        }
        let live = self.pending.is_none()
            && s.active().and_then(|st| self.target.and_then(|id| st.doc.layer(id))).is_some_and(|l| matches!(l.vector.live, Some(LiveGroup::Envelope(_))));
        if ui.add_enabled(live, egui::Button::new(li_i18n::t("Expand envelope"))).clicked() {
            let result = (|| {
                let g = Gesture::begin(s, &[self.target.ok_or("Select an envelope group")?]).map_err(|e| e.to_string())?;
                s.execute("vector.envelope.expand", serde_json::to_value(g.request).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            })();
            if let Err(e) = result {
                self.error = Some(e);
            } else {
                self.committed = None;
            }
        }
        if let Some(e) = &self.error {
            ui.colored_label(Color32::RED, li_i18n::t(e));
        }
    }
    pub fn handles(&self, s: &Session) -> Vec<[f64; 2]> {
        let Some(id) = self.target else { return Vec::new() };
        let Ok(bounds) = s.active().ok_or(()).and_then(|st| engine::source_bounds(&st.doc, id).map_err(|_| ())) else { return Vec::new() };
        let Some(spec @ EnvelopeSpec::Mesh { .. }) = self.spec(s) else { return Vec::new() };
        if geometry::Map::new(&spec, bounds).is_err() {
            return Vec::new();
        }
        let EnvelopeSpec::Mesh { points, .. } = spec else { return Vec::new() };
        points.into_iter().map(|p| [bounds.x0 + p[0] * bounds.width(), bounds.y0 + p[1] * bounds.height()]).collect()
    }
    /// Read egui's real pointer stream; captured view transform remains stable through zoom,
    /// canvas motion and horizontal mirroring. Shift is sampled every frame.
    pub fn canvas_input(&mut self, s: &mut Session, response: &egui::Response, xf: &ViewXform) {
        self.lifecycle(
            s,
            true,
            focused(&response.ctx),
            crate::shortcut_dispatch::Focus::of(&response.ctx) == crate::shortcut_dispatch::Focus::None && response.ctx.input(|i| i.key_pressed(Key::Escape)),
        );
        if !xf.zoom.is_finite() || xf.zoom <= 0.0 {
            self.cancel();
            return;
        }
        let (pressed, down, released, pos, shift) = response
            .ctx
            .input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.interact_pos(), i.modifiers.shift));
        if pressed
            && let Some(pos) = pos
            && response.rect.contains(pos)
            && self.drag.is_none()
        {
            let handles = self.handles(s);
            if let Some(index) = handles.iter().position(|p| xf.to_screen(p[0] as f32, p[1] as f32).distance(pos) <= 8.0) {
                if let Err(e) = self.begin(s) {
                    self.error = Some(e);
                    return;
                }
                let Some(EnvelopeSpec::Mesh { points, .. }) = self.spec(s) else {
                    self.cancel();
                    return;
                };
                let Some(&initial) = points.get(index) else {
                    self.cancel();
                    return;
                };
                self.error = None;
                self.drag = Some(Drag { index, initial, start: xf.to_doc(pos), xf: *xf, moved: false });
            }
        }
        if (down || released)
            && let Some(pos) = pos
            && let Some(drag) = self.drag.clone()
        {
            let Some(g) = &self.pending else { return };
            let Ok(b) = g.bounds() else { return };
            let p = drag.xf.to_doc(pos);
            let mut delta = [p[0] - drag.start[0], p[1] - drag.start[1]];
            if shift {
                if delta[0].abs() > delta[1].abs() {
                    delta[1] = 0.0;
                } else {
                    delta[0] = 0.0;
                }
            }
            if delta[0].hypot(delta[1]) > 1e-9 || drag.moved {
                let Some(mut spec) = self.spec(s) else { return };
                if let EnvelopeSpec::Mesh { points, .. } = &mut spec {
                    let Some(point) = points.get_mut(drag.index) else {
                        self.cancel();
                        return;
                    };
                    *point = [drag.initial[0] + delta[0] / b.width(), drag.initial[1] + delta[1] / b.height()];
                }
                match self.update(s, spec) {
                    Ok(()) => {
                        self.error = None;
                        if let Some(d) = &mut self.drag {
                            d.moved = true;
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
        if released && let Some(d) = self.drag.take() {
            if d.moved && self.error.is_none() {
                if let Err(e) = self.apply(s) {
                    self.error = Some(e);
                }
            } else {
                self.cancel();
            }
        }
    }
    pub fn draw_overlay(&self, s: &Session, painter: &egui::Painter, xf: &ViewXform) {
        let Some(EnvelopeSpec::Mesh { rows, cols, .. }) = self.spec(s) else { return };
        if !(1..=geometry::MAX_GRID_SIDE).contains(&rows) || !(1..=geometry::MAX_GRID_SIDE).contains(&cols) {
            return;
        }
        let handles = self.handles(s);
        if handles.len() != ((rows + 1) * (cols + 1)) as usize {
            return;
        }
        let at = |r: u32, c: u32| {
            let p = handles[(r * (cols + 1) + c) as usize];
            xf.to_screen(p[0] as f32, p[1] as f32)
        };
        for r in 0..=rows {
            for c in 0..=cols {
                let p = at(r, c);
                if c < cols {
                    painter.line_segment([p, at(r, c + 1)], Stroke::new(1.0, Color32::LIGHT_BLUE));
                }
                if r < rows {
                    painter.line_segment([p, at(r + 1, c)], Stroke::new(1.0, Color32::LIGHT_BLUE));
                }
                painter.circle_filled(p, 4.0, Color32::LIGHT_BLUE);
            }
        }
    }
}
const STYLES: [(&str, &str); 15] = [
    ("arc", "Arc"),
    ("arcLower", "Arc lower"),
    ("arcUpper", "Arc upper"),
    ("arch", "Arch"),
    ("bulge", "Bulge"),
    ("shellLower", "Shell lower"),
    ("shellUpper", "Shell upper"),
    ("flag", "Flag"),
    ("wave", "Wave"),
    ("fish", "Fish"),
    ("rise", "Rise"),
    ("fisheye", "Fisheye"),
    ("inflate", "Inflate"),
    ("squeeze", "Squeeze"),
    ("twist", "Twist"),
];
fn style_label(style: &str) -> &'static str {
    STYLES.iter().find(|(id, _)| *id == style).map_or("Arc", |(_, label)| *label)
}
#[cfg(test)]
#[path = "envelope/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "envelope/tests.rs"]
mod tests;
