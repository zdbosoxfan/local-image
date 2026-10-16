//! Live blend controls and captured spine gestures. Original Local Image GPL-3.0-or-later.
//! Interaction follows VectorCraft tests_blendopts/tests_blendspine at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
use egui::{Context, Key, Pos2, Ui};
use photocraft_doc::vector_design::live::{BlendOrientation, BlendSpacing, BlendSpec};
use photocraft_doc::{Document, LayerId, Path};
use photocraft_engine::vector_packages::blend as engine;
use photocraft_engine::{EngineError, Result, Session};
use photocraft_geom::Point;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
struct Drag {
    capture: engine::Capture,
    original: Path,
    anchor: usize,
    start: Point,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct State {
    #[serde(skip)]
    pub open: bool,
    #[serde(skip)]
    pub edit_spine: bool,
    pub spec: BlendSpec,
    #[serde(skip)]
    pub preview: Option<Arc<Document>>,
    #[serde(skip)]
    pub generation: u64,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub host_tool: Option<&'static str>,
    #[serde(skip)]
    capture: Option<engine::Capture>,
    #[serde(skip)]
    drag: Option<Drag>,
    #[serde(skip)]
    synchronized: Option<(u64, u64, Vec<LayerId>)>,
    #[serde(skip)]
    display: Option<(Arc<Document>, Arc<Document>)>,
}
fn selected(s: &Session) -> Vec<LayerId> {
    s.active().map(|s| s.selected_layers()).unwrap_or_default()
}
fn bad(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
impl State {
    pub fn cancel(&mut self) {
        self.preview = None;
        self.capture = None;
        self.drag = None;
        self.synchronized = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn captured(&self) -> bool {
        self.capture.is_some()
    }
    pub fn fresh(&self, s: &Session) -> bool {
        self.capture.as_ref().is_some_and(|c| c.check(s).is_ok() && c.layers == selected(s))
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        if !enabled {
            self.cancel();
            self.edit_spine = false;
            return;
        }
        let Some(st) = s.active() else {
            self.cancel();
            return;
        };
        let ids = selected(s);
        let key = (st.doc.id.0, st.revision, ids.clone());
        if self.capture.is_some() && !self.fresh(s) {
            self.cancel();
        }
        if self.synchronized.as_ref() == Some(&key) || self.capture.is_some() {
            return;
        }
        if let [id] = ids.as_slice()
            && let Ok(spec) = engine::spec(s, *id)
        {
            self.spec = spec;
        } else {
            self.edit_spine = false;
        }
        self.synchronized = Some(key);
    }
    pub fn preview_options(&mut self, s: &Session) -> Result<()> {
        if self.capture.is_none() {
            self.capture = Some(engine::Capture::begin(s, selected(s))?);
        }
        let capture = self.capture.clone().ok_or_else(|| bad("missing blend capture"))?;
        let preview = engine::preview(s, &engine::Request { capture, operation: engine::Operation::Options { spec: self.spec.clone() } })?;
        self.preview = Some(Arc::new(preview.display));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn apply(&mut self, s: &mut Session) -> Result<()> {
        let capture = self.capture.clone().ok_or_else(|| bad("no captured blend options"))?;
        let spec = self.spec.clone();
        s.execute("vector.blend.options", json!({"capture":capture,"spec":spec}))?;
        self.cancel();
        Ok(())
    }
    pub fn down(&mut self, s: &Session, p: Point, tolerance: f64) -> Result<bool> {
        if !self.edit_spine {
            return Ok(false);
        }
        if !tolerance.is_finite() || !(0. ..=1000.).contains(&tolerance) || !p.x.is_finite() || !p.y.is_finite() {
            return Err(bad("invalid blend pointer or hit tolerance"));
        }
        if self.capture.is_some() {
            self.cancel();
        }
        let ids = selected(s);
        let [id] = ids.as_slice() else { return Ok(false) };
        let capture = engine::Capture::begin(s, ids.clone())?;
        let mut path = engine::spine(s, *id)?;
        let sub = &path.subpaths[0];
        let nearest = sub.knots.iter().enumerate().map(|(i, k)| (i, (k.anchor.x - p.x).hypot(k.anchor.y - p.y))).min_by(|a, b| a.1.total_cmp(&b.1));
        let anchor = if let Some((i, d)) = nearest.filter(|(_, d)| *d <= tolerance) {
            let _ = d;
            i
        } else {
            let Some((_, segment, t, _, distance)) = engine::evaluator_nearest(&path, p)? else { return Ok(false) };
            if distance > tolerance {
                return Ok(false);
            }
            if sub.knots.len() >= 128 {
                return Err(bad("blend spine exceeds anchor budget"));
            }
            let mut geometry = engine::evaluator_geometry(&path);
            let i = geometry.subpaths[0].insert_anchor(segment, t.clamp(0.001, 0.999));
            path = engine::evaluator_from_geometry(&geometry);
            i
        };
        self.spec = engine::spec(s, *id)?;
        self.capture = Some(capture.clone());
        self.drag = Some(Drag { capture, original: path, anchor, start: p });
        Ok(true)
    }
    pub fn motion(&mut self, s: &Session, p: Point, modifiers: egui::Modifiers) -> Result<()> {
        let Some(drag) = self.drag.clone() else { return Ok(()) };
        drag.capture.check(s)?;
        let mut path = drag.original;
        let mut delta = Point::new(p.x - drag.start.x, p.y - drag.start.y);
        if modifiers.shift {
            if delta.x.abs() >= delta.y.abs() {
                delta.y = 0.;
            } else {
                delta.x = 0.;
            }
        }
        let k = path.subpaths[0].knots.get_mut(drag.anchor).ok_or_else(|| bad("spine anchor disappeared"))?;
        // Alt moves the outgoing handle; otherwise translate anchor and both handles.
        if modifiers.alt {
            k.out_ctrl = Point::new(k.out_ctrl.x + delta.x, k.out_ctrl.y + delta.y);
        } else {
            for p in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
                p.x += delta.x;
                p.y += delta.y;
            }
        }
        self.spec.spine = Some(path);
        self.preview_options(s)
    }
    pub fn up(&mut self, s: &mut Session, p: Point, modifiers: egui::Modifiers) -> Result<()> {
        if self.drag.is_none() {
            return Ok(());
        }
        self.motion(s, p, modifiers)?;
        self.apply(s)
    }
    /// Lazily project display trees, including the current Move/Node preview selected by the
    /// host. Pointer identity changes when keys are edited; no persistent projection cache.
    pub fn project_display(&mut self, source: Arc<Document>) -> Result<Arc<Document>> {
        if let Some((previous, display)) = &self.display
            && Arc::ptr_eq(previous, &source)
        {
            return Ok(display.clone());
        }
        let display = if source.walk().iter().any(|(_, _, l)| matches!(l.vector.live, Some(photocraft_doc::vector_design::live::LiveGroup::Blend(_)))) {
            Arc::new(engine::evaluator::project_document(&source).map_err(|e| bad(e.to_string()))?)
        } else {
            source.clone()
        };
        self.display = Some((source, display.clone()));
        Ok(display)
    }
}
pub fn panel(ui: &mut Ui, state: &mut State, s: &mut Session, t: impl Fn(&str) -> String) -> Result<()> {
    state.synchronize(s, true);
    let ids = selected(s);
    let live = ids.len() == 1 && engine::spec(s, ids[0]).is_ok();
    let old = state.spec.clone();
    ui.horizontal(|ui| {
        if ui.selectable_label(matches!(state.spec.spacing, BlendSpacing::SmoothColor), t("Smooth colour")).clicked() {
            state.spec.spacing = BlendSpacing::SmoothColor;
        }
        if ui.selectable_label(matches!(state.spec.spacing, BlendSpacing::Steps(_)), t("Specified steps")).clicked() {
            state.spec.spacing = BlendSpacing::Steps(5);
        }
        if ui.selectable_label(matches!(state.spec.spacing, BlendSpacing::Distance(_)), t("Specified distance")).clicked() {
            state.spec.spacing = BlendSpacing::Distance(10.);
        }
    });
    match &mut state.spec.spacing {
        BlendSpacing::Steps(n) => {
            ui.horizontal(|ui| {
                let label = ui.label(t("Steps"));
                ui.add(egui::DragValue::new(n).range(0..=1000)).labelled_by(label.id);
            });
        }
        BlendSpacing::Distance(d) => {
            ui.horizontal(|ui| {
                let label = ui.label(t("Distance"));
                ui.add(egui::DragValue::new(d).range(0.01..=1_000_000.).speed(0.1)).labelled_by(label.id);
            });
        }
        _ => {}
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.spec.orientation, BlendOrientation::AlignToPage, t("Align to page"));
        ui.selectable_value(&mut state.spec.orientation, BlendOrientation::AlignToPath, t("Align to path"));
    });
    if state.spec != old && live {
        state.preview_options(s)?;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(!live && ids.len() >= 2, egui::Button::new(t("Create Blend"))).clicked() {
            let capture = engine::Capture::begin(s, ids.clone())?;
            s.execute("vector.blend.create", json!({"capture":capture,"spec":state.spec}))?;
            state.cancel();
        }
        if ui.add_enabled(live && state.preview.is_some(), egui::Button::new(t("Apply Blend Options"))).clicked() {
            state.apply(s)?;
        }
        if ui.add_enabled(live, egui::Button::new(t("Edit blend spine"))).clicked() {
            state.cancel();
            state.edit_spine = !state.edit_spine;
        }
        if ui.add_enabled(live, egui::Button::new(t("Expand Blend"))).clicked() {
            let capture = engine::Capture::begin(s, ids.clone())?;
            s.execute("vector.blend.expand", json!({"capture":capture}))?;
            state.cancel();
            state.edit_spine = false;
        }
        if ui.add_enabled(live, egui::Button::new(t("Release Blend"))).clicked() {
            let capture = engine::Capture::begin(s, ids.clone())?;
            s.execute("vector.blend.release", json!({"capture":capture}))?;
            state.cancel();
            state.edit_spine = false;
        }
        if ui.button(t("Cancel blend preview")).clicked() {
            state.cancel();
        }
        Ok::<_, EngineError>(())
    })
    .inner?;
    if ui.input(|i| i.key_pressed(Key::Enter)) && state.drag.is_none() && state.preview.is_some() {
        state.apply(s)?;
    }
    Ok(())
}
pub fn input(state: &mut State, s: &mut Session, ctx: &Context, rect: egui::Rect, to_doc: impl Fn(Pos2) -> Point, tolerance: f64, enabled: bool) -> Result<()> {
    state.synchronize(s, enabled);
    let (cancel, press, down, release, pos, mods) = ctx.input(|i| {
        (
            !i.focused || i.key_pressed(Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::PointerGone | egui::Event::WindowFocused(false))),
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
            i.modifiers,
        )
    });
    if cancel {
        state.cancel();
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }
    if let Some(pos) = pos {
        if press && rect.contains(pos) {
            state.down(s, to_doc(pos), tolerance)?;
        }
        if down && state.drag.is_some() {
            state.motion(s, to_doc(pos), mods)?;
        }
        if release && state.drag.is_some() {
            state.up(s, to_doc(pos), mods)?;
        }
    }
    Ok(())
}
pub fn overlay(state: &State, s: &Session, painter: &egui::Painter, to_screen: impl Fn(Point) -> Pos2) {
    if !state.edit_spine {
        return;
    }
    let path = state.spec.spine.clone().or_else(|| selected(s).first().and_then(|id| engine::spine(s, *id).ok()));
    if let Some(path) = path {
        let Ok(polylines) = engine::evaluator_flatten(&path) else { return };
        for sub in polylines {
            let points: Vec<_> = sub.pts.iter().map(|&(x, y)| to_screen(Point::new(x, y))).collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(1., egui::Color32::MAGENTA)));
        }
        for k in &path.subpaths[0].knots {
            painter.circle_filled(to_screen(k.anchor), 4., egui::Color32::MAGENTA);
        }
    }
}

#[cfg(test)]
#[path = "blend/tests.rs"]
mod tests;

#[path = "blend/app_hooks.rs"]
pub mod app_hooks;
