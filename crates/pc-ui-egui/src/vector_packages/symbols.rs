//! Symbols panel and captured placement/master-edit previews (GPL-3.0-or-later).
//! Affinity's Create/Place/Edit/Detach semantics, on the app's existing ordinary layers.
use egui::Ui;
use photocraft_doc::{Affine, Document, Fill, Layer, LayerContent, LayerId, SymbolId};
use photocraft_engine::{
    Session,
    vector_packages::symbols::{self as engine, Capture},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
struct Placement {
    capture: Capture,
    symbol: SymbolId,
    above: Option<LayerId>,
    targets: Vec<LayerId>,
    pressed: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct Draft {
    capture: Capture,
    symbol: SymbolId,
    layers: Vec<Layer>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub name: String,
    pub selected: Option<SymbolId>,
    #[serde(skip)]
    placement: Option<Placement>,
    #[serde(skip)]
    draft: Option<Draft>,
    #[serde(skip)]
    pub preview: Option<(Arc<Document>, u64)>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    epoch: u64,
    #[serde(skip)]
    input: Option<InputCapture>,
    #[serde(skip)]
    context: Option<(crate::state::Tool, photocraft_engine::toolsets::Persona)>,
    #[serde(skip)]
    source_document: Option<photocraft_doc::DocId>,
}
impl State {
    pub fn cancel(&mut self) {
        self.placement = None;
        self.draft = None;
        self.preview = None;
        self.input = None;
    }
    pub fn armed(&self) -> bool {
        self.placement.is_some()
    }
    fn show(&mut self, result: photocraft_engine::Result<Document>) {
        match result {
            Ok(doc) => {
                self.epoch = self.epoch.wrapping_add(1);
                self.preview = Some((Arc::new(doc), (1 << 53) | self.epoch));
                self.error = None;
            }
            Err(e) => {
                self.preview = None;
                self.error = Some(e.to_string());
            }
        }
    }
    pub fn maintain(&mut self, session: &Session, targets: &[LayerId], cancel: bool) {
        let document = session.active().map(|s| s.doc.id);
        if self.source_document.is_some() && self.source_document != document {
            self.cancel();
            self.selected = None;
            self.name.clear();
        }
        self.source_document = document;
        let stale = self.placement.as_ref().is_some_and(|p| p.capture.check(session).is_err() || p.targets != targets)
            || self.draft.as_ref().is_some_and(|d| d.capture.check(session).is_err());
        if cancel || stale {
            self.cancel();
        }
    }
}
pub fn panel(ui: &mut Ui, session: &mut Session, state: &mut State, targets: &[LayerId], tr: impl Fn(&'static str) -> String) {
    state.maintain(session, targets, ui.input(cancel_input));
    let Some(st) = session.active() else {
        ui.label(tr("No document"));
        return;
    };
    let masters: Vec<_> = st.doc.symbols.iter().map(|d| (d.id, d.name.clone())).collect();
    if state.selected.is_some_and(|id| !masters.iter().any(|(s, _)| *s == id)) {
        state.selected = None;
        state.cancel();
    }
    let label = ui.label(tr("Symbol name"));
    ui.add(egui::TextEdit::singleline(&mut state.name)).labelled_by(label.id);
    if ui.add_enabled(!targets.is_empty() && !state.name.trim().is_empty(), egui::Button::new(tr("Create Symbol"))).clicked() {
        let result = Capture::new(session).and_then(|c| {
            let mut p = c.params();
            p["layers"] = json!(targets.iter().map(|id| id.0).collect::<Vec<_>>());
            p["name"] = json!(state.name);
            session.execute("symbol.create", p)
        });
        match result {
            Ok(v) => {
                state.selected = v["symbol"].as_u64().map(SymbolId);
                state.cancel();
                state.error = None;
            }
            Err(e) => state.error = Some(e.to_string()),
        }
    }
    for (id, name) in &masters {
        if ui.selectable_label(state.selected == Some(*id), name).clicked() {
            state.cancel();
            state.selected = Some(*id);
            state.name = name.clone();
        }
    }
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(state.selected.is_some(), egui::Button::new(tr("Place Symbol"))).clicked() {
            match Capture::new(session) {
                Ok(c) => {
                    state.cancel();
                    if let Some(symbol) = state.selected {
                        state.placement = Some(Placement {
                            capture: c,
                            symbol,
                            above: session.active().and_then(|s| s.active_layer),
                            targets: targets.to_vec(),
                            pressed: false,
                        });
                    }
                }
                Err(e) => state.error = Some(e.to_string()),
            }
        }
        if ui.add_enabled(state.selected.is_some(), egui::Button::new(tr("Edit Master"))).clicked()
            && let (Ok(c), Some(id), Some(st)) = (Capture::new(session), state.selected, session.active())
            && let Some(def) = st.doc.symbols.iter().find(|d| d.id == id)
        {
            state.cancel();
            state.draft = Some(Draft { capture: c, symbol: id, layers: def.layers.clone() });
        }
        let can_detach = session
            .active()
            .is_some_and(|st| !targets.is_empty() && targets.iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Symbol(_)))));
        if ui.add_enabled(can_detach, egui::Button::new(tr("Detach Symbols"))).clicked() {
            command(session, state, "symbol.detach", targets);
        }
        if ui.add_enabled(state.selected.is_some() && !state.name.trim().is_empty(), egui::Button::new(tr("Rename Symbol"))).clicked() {
            command(session, state, "symbol.rename", targets);
        }
        if ui.add_enabled(state.selected.is_some(), egui::Button::new(tr("Delete Symbol"))).clicked() {
            command(session, state, "symbol.delete", targets);
        }
    });
    if let Some(draft) = &mut state.draft {
        ui.separator();
        ui.label(tr("Edit Master"));
        fn controls(ui: &mut Ui, layers: &mut [Layer], tr: &impl Fn(&'static str) -> String) -> bool {
            let mut changed = false;
            for l in layers {
                ui.push_id(l.id.0, |ui| {
                    changed |= ui.checkbox(&mut l.visible, &l.name).changed();
                    match &mut l.content {
                        LayerContent::Shape(sh) => {
                            if let Some(Fill::Solid(c)) = &mut sh.fill {
                                let rgb = c.to_rgb();
                                let mut rgba = [rgb[0], rgb[1], rgb[2], c.alpha];
                                let mut paint_changed = false;
                                for (i, label) in ["Master red", "Master green", "Master blue", "Master alpha"].iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        let label = ui.label(tr(label));
                                        paint_changed |= ui
                                            .add(egui::DragValue::new(&mut rgba[i]).range(0.0..=1.0).clamp_existing_to_range(false).speed(0.01))
                                            .labelled_by(label.id)
                                            .changed();
                                    });
                                }
                                if paint_changed {
                                    *c = photocraft_doc::Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
                                    changed = true;
                                }
                            }
                        }
                        LayerContent::Text(t) => {
                            let mut text = t.text.clone();
                            if ui.add(egui::TextEdit::multiline(&mut text).hint_text(tr("Master text"))).changed() {
                                let len = t.text.len();
                                photocraft_engine::type_cmds::replace_text(t, 0, len, &text);
                                changed = true;
                            }
                        }
                        LayerContent::Group(g) => changed |= controls(ui, &mut g.children, tr),
                        _ => {}
                    }
                });
            }
            changed
        }
        let changed = controls(ui, &mut draft.layers, &tr);
        let preview = changed.then(|| engine::master_preview(session, draft.capture, draft.symbol, draft.layers.clone()));
        if let Some(preview) = preview {
            state.show(preview);
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Apply Master")).clicked()
                && let Some(d) = &state.draft
            {
                let mut p = d.capture.params();
                p["symbol"] = json!(d.symbol.0);
                fn edits(layers: &[Layer], original: &[Layer], out: &mut Vec<Value>) {
                    for (l, before) in layers.iter().zip(original) {
                        let mut edit = json!({"layer":l.id.0});
                        if l.visible != before.visible {
                            edit["visible"] = json!(l.visible);
                        }
                        match (&l.content, &before.content) {
                            (LayerContent::Group(g), LayerContent::Group(old)) => edits(&g.children, &old.children, out),
                            (LayerContent::Shape(sh), LayerContent::Shape(old)) if sh.fill != old.fill => edit["fill"] = json!(sh.fill),
                            (LayerContent::Text(t), LayerContent::Text(old)) if t.text != old.text => edit["text"] = json!(t.text),
                            _ => {}
                        }
                        if edit.as_object().is_some_and(|e| e.len() > 1) {
                            out.push(edit);
                        }
                    }
                }
                let mut out = Vec::new();
                if let Some(original) = session.active().and_then(|s| s.doc.symbols.iter().find(|def| def.id == d.symbol)) {
                    edits(&d.layers, &original.layers, &mut out);
                }
                if out.is_empty() {
                    state.cancel();
                    return;
                }
                p["edits"] = json!(out);
                match session.execute("symbol.editMaster", p) {
                    Ok(_) => {
                        state.cancel();
                        state.error = None;
                    }
                    Err(e) => state.error = Some(e.to_string()),
                }
            }
            if ui.button(tr("Cancel")).clicked() {
                state.cancel();
            }
        });
    }
    if state.armed() {
        ui.label(tr("Drag on the canvas to place the symbol"));
    }
    if let Some(e) = &state.error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
}
fn command(s: &mut Session, state: &mut State, id: &str, targets: &[LayerId]) {
    let result = Capture::new(s).and_then(|c| {
        let mut p = c.params();
        p["layers"] = json!(targets.iter().map(|id| id.0).collect::<Vec<_>>());
        p["name"] = json!(state.name);
        p["symbol"] = json!(state.selected.map(|s| s.0));
        s.execute(id, p)
    });
    match result {
        Ok(_) => {
            state.cancel();
            state.error = None;
        }
        Err(e) => state.error = Some(e.to_string()),
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Down(f64, f64),
    Move(f64, f64),
    Up(f64, f64),
    Cancel,
}
/// Production canvas adapter. Capture includes both source revision and insertion/selection IDs.
pub fn pointer(session: &mut Session, state: &mut State, event: Event, targets: &[LayerId]) -> bool {
    state.maintain(session, targets, matches!(event, Event::Cancel));
    let Some(mut placement) = state.placement.clone() else {
        return false;
    };
    let (x, y, release) = match event {
        Event::Down(x, y) => {
            placement.pressed = true;
            (x, y, false)
        }
        Event::Move(x, y) if placement.pressed => (x, y, false),
        Event::Up(x, y) if placement.pressed => (x, y, true),
        _ => return true,
    };
    let a = Affine::translate(x, y);
    if release {
        let mut p = placement.capture.params();
        p["symbol"] = json!(placement.symbol.0);
        p["transform"] = json!(a);
        p["above"] = json!(placement.above.map(|id| id.0));
        match session.execute("symbol.place", p) {
            Ok(_) => {
                state.cancel();
                state.error = None;
            }
            Err(e) => {
                state.cancel();
                state.error = Some(e.to_string());
            }
        }
    } else {
        state.show(engine::place_preview(session, placement.capture, placement.symbol, a, placement.above).map(|(doc, _)| doc));
        state.placement = Some(placement);
    }
    true
}
/// Called by the real app's existing display_doc path; no permanent document cache changes.
pub fn display_doc(session: &Session, state: &State, idx: usize) -> Option<(Arc<Document>, u64)> {
    let expected = state.placement.as_ref().map(|p| p.capture).or_else(|| state.draft.as_ref().map(|d| d.capture))?;
    let st = session.documents().get(idx)?;
    if st.doc.id != expected.document || st.revision != expected.revision {
        return None;
    }
    state.preview.clone()
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct InputCapture {
    origin: egui::Pos2,
    start: [f64; 2],
    zoom: f32,
    flip: bool,
}
fn cancel_input(i: &egui::InputState) -> bool {
    !i.focused || i.key_pressed(egui::Key::Escape) || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)))
}
/// Uses the app's existing captured-pointer contract; placement works with any active tool.
pub fn canvas_input(app: &mut crate::PhotocraftApp, response: &egui::Response, xf: &crate::canvas::ViewXform) -> bool {
    if !app.ui.symbols.armed() {
        return false;
    }
    if response.hovered() {
        response.ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let (pressed, released, down, pos, cancel) =
        response.ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down(), i.pointer.latest_pos(), cancel_input(i)));
    if cancel {
        app.ui.symbols.cancel();
        return true;
    }
    if app.ui.symbols.input.is_none() {
        if !pressed {
            return false;
        }
        let Some(p) = pos.filter(|p| xf.rect.contains(*p) && response.ctx.layer_id_at(*p) == Some(response.layer_id)) else {
            return false;
        };
        let start = xf.to_doc(p);
        app.ui.symbols.input = Some(InputCapture { origin: p, start, zoom: xf.zoom, flip: xf.flip });
    }
    let Some(c) = app.ui.symbols.input else {
        return false;
    };
    let at = pos
        .map(|p| {
            let d = (p - c.origin) / c.zoom.max(0.01);
            [c.start[0] + f64::from(d.x) * if c.flip { -1. } else { 1. }, c.start[1] + f64::from(d.y)]
        })
        .unwrap_or(c.start);
    let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    let event = if pressed {
        Event::Down(at[0], at[1])
    } else if released {
        Event::Up(at[0], at[1])
    } else if down {
        Event::Move(at[0], at[1])
    } else {
        Event::Cancel
    };
    pointer(&mut app.session, &mut app.ui.symbols, event, &ids);
    true
}
/// Symbols is initially hidden. Opening it materializes the existing dock layout and adds
/// this ordinary movable tab, preserving older default layouts and tab indices.
pub fn open_panel(app: &mut crate::PhotocraftApp) {
    let pro = crate::dock::is_pro(app);
    let group = crate::dock::Group::Layers;
    if !app.ui.dock.group_tabs(group, pro).contains(&crate::dock::Tab::Symbols) {
        let at = app.ui.dock.group_tabs(group, pro).len();
        app.ui.dock.insert(&[crate::dock::Tab::Symbols], group, at, pro);
    }
    app.ui.dock.active.insert(group, crate::dock::Tab::Symbols);
    app.ui.panels.layers = true;
}
/// Frame guard runs even when the Symbols tab is hidden.
pub fn frame(app: &mut crate::PhotocraftApp, ctx: &egui::Context) {
    let context = (app.ui.tool, app.session.prefs().toolbar.persona);
    let cancel = app.ui.symbols.context.is_some_and(|old| old != context) || ctx.input(cancel_input);
    let targets = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
    app.ui.symbols.maintain(&app.session, &targets, cancel);
    app.ui.symbols.context = Some(context);
}
#[cfg(test)]
#[path = "symbols/tests.rs"]
mod tests;
