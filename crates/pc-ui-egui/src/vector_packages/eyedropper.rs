//! Vector Eyedropper and Style Picker, informed by VectorCraft tools/xform/eyedropper.rs
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! Raw egui press/move/release captures preview; the existing canvas owns coordinates/hit scope.
use egui::{Event, Key, PointerButton, Pos2, Rect, Ui};
use photocraft_doc::{Document, LayerId};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::styles::{self as engine, Attributes, Operation, Request, Sample},
};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    #[default]
    Eyedropper,
    StylePicker,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub tool: Tool,
    pub pick: Attributes,
    pub apply: Attributes,
    pub color_stroke: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { tool: Tool::Eyedropper, pick: Attributes::ALL, apply: Attributes::ALL, color_stroke: false }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub layer: LayerId,
    pub byte: Option<usize>,
    pub stroke: bool,
}
#[derive(Clone)]
pub struct Loaded {
    pub sample: Sample,
    pub picked: Attributes,
    pub document: u64,
}
#[derive(Clone)]
struct Capture {
    request: Option<Request>,
    start: Point,
    base: Vec<LayerId>,
    document: u64,
    revision: u64,
    loaded: Option<Loaded>,
}
#[derive(Clone, Default)]
pub struct State {
    pub options: Options,
    pub loaded: Option<Loaded>,
    pub preview: Option<Document>,
    pub generation: u64,
    capture: Option<Capture>,
}
fn error(s: &str) -> EngineError {
    EngineError::Other(s.into())
}
impl State {
    pub fn captured(&self) -> bool {
        self.capture.is_some()
    }
    pub fn cancel(&mut self) {
        self.capture = None;
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn unload(&mut self) {
        self.cancel();
        self.loaded = None;
    }
    pub fn synchronize(&mut self, session: &Session, enabled: bool) {
        let st = session.active();
        if self.loaded.as_ref().is_some_and(|l| st.is_none_or(|s| s.doc.id.0 != l.document)) {
            self.unload();
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|c| !enabled || st.is_none_or(|s| s.doc.id.0 != c.document || s.revision != c.revision || s.selected_layers() != c.base))
        {
            self.cancel();
        }
    }
    fn show(&mut self, s: &Session) -> Result<()> {
        self.preview = if let Some(r) = self.capture.as_ref().and_then(|c| c.request.as_ref()) { Some(engine::preview(s, r)?.document) } else { None };
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    fn begin(&mut self, s: &Session, at: Point, hit: Hit, alt: bool, shift: bool) -> Result<()> {
        self.cancel();
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let base = st.selected_layers();
        let mut loaded = None;
        let mut request = None;
        if self.options.tool == Tool::StylePicker {
            if let Some(l) = &self.loaded {
                request = Some(engine::capture(
                    s,
                    vec![hit.layer],
                    Operation::Copy { sample: Box::new(l.sample.clone()), attributes: l.picked.intersection(self.options.apply), append: false },
                )?);
            } else {
                let l = Loaded { sample: engine::sample(&st.doc, hit.layer, hit.byte)?, picked: self.options.pick, document: st.doc.id.0 };
                if !base.is_empty() {
                    request = Some(engine::capture(
                        s,
                        base.clone(),
                        Operation::Copy { sample: Box::new(l.sample.clone()), attributes: l.picked.intersection(self.options.apply), append: false },
                    )?);
                }
                loaded = Some(l);
            }
        } else if shift && !alt {
            let color = engine::color_at(&st.doc, hit.layer, at.x, at.y, hit.stroke)?;
            if !base.is_empty() {
                request = Some(engine::capture(s, base.clone(), Operation::Color { color, stroke: self.options.color_stroke })?);
            }
            let mut sample = engine::sample(&st.doc, hit.layer, hit.byte)?;
            sample.appearance = Some(photocraft_doc::vector_design::appearance::Appearance {
                items: vec![photocraft_doc::vector_design::appearance::AppearanceItem::Fill(photocraft_doc::vector_design::appearance::FillItem::new(
                    photocraft_doc::Fill::Solid(color),
                ))],
            });
            loaded = Some(Loaded { sample, picked: Attributes { fill: true, ..Attributes::NONE }, document: st.doc.id.0 });
        } else if alt && !shift {
            let l = if let Some(l) = &self.loaded {
                l.clone()
            } else {
                let id = base.first().copied().ok_or_else(|| error("sample an appearance before applying"))?;
                Loaded { sample: engine::sample(&st.doc, id, None)?, picked: self.options.pick, document: st.doc.id.0 }
            };
            request = Some(engine::capture(
                s,
                vec![hit.layer],
                Operation::Copy { sample: Box::new(l.sample), attributes: l.picked.intersection(self.options.apply), append: false },
            )?);
        } else {
            let l = Loaded { sample: engine::sample(&st.doc, hit.layer, hit.byte)?, picked: self.options.pick, document: st.doc.id.0 };
            if !base.is_empty() {
                request = Some(engine::capture(
                    s,
                    base.clone(),
                    Operation::Copy { sample: Box::new(l.sample.clone()), attributes: l.picked.intersection(self.options.apply), append: alt && shift },
                )?);
            }
            loaded = Some(l);
        }
        self.capture = Some(Capture { request, start: at, base, document: st.doc.id.0, revision: st.revision, loaded });
        if let Err(e) = self.show(s) {
            self.cancel();
            return Err(e);
        }
        Ok(())
    }
    pub fn release(&mut self, session: &mut Session) -> Result<()> {
        self.synchronize(session, true);
        let c = self.capture.take();
        self.preview = None;
        self.generation = self.generation.wrapping_add(1);
        if let Some(c) = c {
            if let Some(r) = c.request {
                engine::commit(session, &r)?;
            }
            if let Some(l) = c.loaded {
                self.loaded = Some(l);
            }
        }
        Ok(())
    }
}
/// Hit/marquee callbacks MUST apply the authoritative persona isolation and visibility scope.
/// They include raster image hits and return a text byte from pc-text's layout hit test.
#[allow(clippy::too_many_arguments)]
pub fn input(
    state: &mut State,
    session: &mut Session,
    ctx: &egui::Context,
    canvas: Rect,
    to_doc: impl Fn(Pos2) -> Point,
    hit: impl Fn(&Document, Point) -> Result<Option<Hit>>,
    marquee: impl Fn(&Document, Point, Point) -> Result<Vec<LayerId>>,
    enabled: bool,
) -> Result<()> {
    state.synchronize(session, enabled);
    if !enabled {
        return Ok(());
    }
    let events = ctx.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) | Event::PointerGone => state.cancel(),
            Event::Key { key: Key::I, pressed: true, modifiers, .. } if !modifiers.command && !ctx.egui_wants_keyboard_input() => {
                state.cancel();
                state.options.tool = Tool::Eyedropper;
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers } if canvas.contains(pos) => {
                let doc = &session.active().ok_or(EngineError::NoDocument)?.doc;
                if let Some(h) = hit(doc, to_doc(pos))? {
                    state.begin(session, to_doc(pos), h, modifiers.alt, modifiers.shift)?;
                } else if state.options.tool == Tool::StylePicker && state.loaded.is_some() {
                    let st = session.active().ok_or(EngineError::NoDocument)?;
                    state.capture = Some(Capture {
                        request: None,
                        start: to_doc(pos),
                        base: st.selected_layers(),
                        document: st.doc.id.0,
                        revision: st.revision,
                        loaded: None,
                    });
                }
            }
            Event::PointerMoved(pos) if state.options.tool == Tool::StylePicker && state.capture.is_some() && state.loaded.is_some() => {
                let start = state.capture.as_ref().ok_or_else(|| error("missing picker capture"))?.start;
                let end = to_doc(pos);
                if (end.x - start.x).hypot(end.y - start.y) > 2. {
                    let st = session.active().ok_or(EngineError::NoDocument)?;
                    let ids = marquee(&st.doc, start, end)?;
                    let l = state.loaded.as_ref().ok_or_else(|| error("picker unloaded"))?;
                    let r = if ids.is_empty() {
                        None
                    } else {
                        Some(engine::capture(
                            session,
                            ids,
                            Operation::Copy { sample: Box::new(l.sample.clone()), attributes: l.picked.intersection(state.options.apply), append: false },
                        )?)
                    };
                    if let Some(c) = &mut state.capture {
                        c.request = r;
                    }
                    if let Err(e) = state.show(session) {
                        state.cancel();
                        return Err(e);
                    }
                }
            }
            Event::PointerButton { button: PointerButton::Primary, pressed: false, .. } if state.captured() => state.release(session)?,
            _ => {}
        }
    }
    Ok(())
}
pub fn attributes(ui: &mut Ui, a: &mut Attributes) {
    ui.horizontal_wrapped(|ui| {
        for (name, value) in [
            ("Fill", &mut a.fill),
            ("Stroke", &mut a.stroke),
            ("Opacity", &mut a.opacity),
            ("Effects", &mut a.effects),
            ("Character", &mut a.character),
            ("Paragraph", &mut a.paragraph),
            ("Object", &mut a.object),
        ] {
            ui.checkbox(value, crate::i18n::t(name));
        }
    });
}
pub fn context(ui: &mut Ui, state: &mut State) {
    let old = state.options.clone();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.options.tool, Tool::Eyedropper, crate::i18n::t("Vector Eyedropper"));
        ui.selectable_value(&mut state.options.tool, Tool::StylePicker, crate::i18n::t("Style Picker"));
        if ui.button(crate::i18n::t("Unload")).clicked() {
            state.unload();
        }
    });
    if state.options.tool == Tool::Eyedropper {
        ui.collapsing(crate::i18n::t("Eyedropper Options"), |ui| {
            ui.label(crate::i18n::t("Pick"));
            attributes(ui, &mut state.options.pick);
            ui.label(crate::i18n::t("Apply"));
            attributes(ui, &mut state.options.apply);
            ui.checkbox(&mut state.options.color_stroke, crate::i18n::t("Sample colour to stroke"));
        });
    } else {
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::t("All")).clicked() {
                state.options.apply = Attributes::ALL;
            }
            if ui.button(crate::i18n::t("None")).clicked() {
                state.options.apply = Attributes::NONE;
            }
        });
        attributes(ui, &mut state.options.apply);
        ui.label(crate::i18n::t(if state.loaded.is_some() { "Style loaded" } else { "Click an object to load its style" }));
    }
    if old != state.options {
        state.cancel();
    }
}
#[cfg(test)]
#[path = "eyedropper/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "eyedropper/app_gpu.rs"]
mod app_gpu;

/// App bridge follows the existing anchor-tools transient egui-memory contract. The
/// coordinator wires these four hooks; no persisted UiState field or alternative canvas.
#[derive(Clone, Default)]
pub struct AppBridge {
    pub tool: Option<Tool>,
    pub state: State,
    owner: Option<crate::state::Tool>,
    persona: Option<photocraft_engine::toolsets::Persona>,
    module: Option<crate::Module>,
}
fn bridge_key() -> egui::Id {
    egui::Id::new("vector-styles-38-eyedropper")
}
pub fn bridge(ctx: &egui::Context) -> AppBridge {
    ctx.data_mut(|d| d.get_temp::<AppBridge>(bridge_key()).unwrap_or_default())
}
pub fn save_bridge(ctx: &egui::Context, bridge: AppBridge) {
    ctx.data_mut(|d| d.insert_temp(bridge_key(), bridge));
}
pub fn activate(app: &crate::PhotocraftApp, ctx: &egui::Context, tool: Option<Tool>) {
    let mut b = bridge(ctx);
    b.state.cancel();
    b.tool = tool;
    if let Some(tool) = tool {
        b.state.options.tool = tool;
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
/// The supplied scene queries own isolation/visibility, raster bounds and pc-text hit bytes.
#[allow(clippy::too_many_arguments)]
pub fn route(
    app: &mut crate::PhotocraftApp,
    ui: &mut Ui,
    idx: usize,
    xf: &crate::canvas::ViewXform,
    canvas: Rect,
    hit: impl Fn(&Document, Point) -> Result<Option<Hit>>,
    marquee: impl Fn(&Document, Point, Point) -> Result<Vec<LayerId>>,
) -> bool {
    if app.session.active_index() != Some(idx) {
        return false;
    }
    let ctx = ui.ctx().clone();
    frame(app, &ctx);
    let mut b = bridge(&ctx);
    if b.tool.is_none() || !app.ui.dialogs.is_empty() || !ctx.input(|i| i.focused) {
        return false;
    }
    if let Err(e) = input(
        &mut b.state,
        &mut app.session,
        &ctx,
        canvas,
        |p| {
            let p = xf.to_doc(p);
            Point::new(p[0], p[1])
        },
        hit,
        marquee,
        true,
    ) {
        b.state.cancel();
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    if b.state.captured() {
        ctx.request_repaint();
    }
    save_bridge(&ctx, b);
    true
}
pub fn context_bar(app: &crate::PhotocraftApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut b = bridge(&ctx);
    b.state.synchronize(&app.session, true);
    context(ui, &mut b.state);
    save_bridge(&ctx, b);
}
pub fn display_doc(app: &crate::PhotocraftApp, idx: usize, ctx: &egui::Context) -> Option<(std::sync::Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) || !app.ui.dialogs.is_empty() {
        return None;
    }
    let b = bridge(ctx);
    if b.tool.is_none() || b.owner != Some(app.ui.tool) || b.persona != Some(app.session.prefs().toolbar.persona) || b.module != Some(app.current_module) {
        return None;
    }
    let c = b.state.capture.as_ref()?;
    if app.session.active().is_none_or(|s| s.doc.id.0 != c.document || s.revision != c.revision || s.selected_layers() != c.base) {
        return None;
    }
    Some((std::sync::Arc::new(b.state.preview?), b.state.generation))
}
