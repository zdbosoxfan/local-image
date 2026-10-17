//! Draw modes and Layers menu/drag fragments. Original GPL-3.0-or-later UI;
//! Illustrator drawing modes and Affinity Layer clipping are behavior references.
//! These entrypoints are routed only in the Vector persona.
use egui::{Color32, Key, Modifiers, Pos2, Rect, Response, Sense, Stroke, Ui};
use photocraft_doc::{Document, Fill, FillRule, LayerId, Path, ShapeStroke};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::clip_compound::{self as engine, Action, Capture, DrawMode, DrawRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: DrawMode,
}
#[derive(Default)]
pub struct State {
    pub settings: Settings,
    pub preview: Option<Document>,
    pub generation: u64,
    preview_target: Option<LayerId>,
    pub error: Option<String>,
    /// Edit Mask supplies this to existing Direct Selection as PathRef::Layer(id.0).
    pub mask_target: Option<LayerId>,
    drag: Option<Capture>,
    drawing: Option<DrawRequest>,
    hovered: Option<LayerId>,
}
impl State {
    pub fn cancel(&mut self) {
        if self.preview.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
        self.preview_target = None;
        self.drag = None;
        self.drawing = None;
        self.hovered = None;
    }
    pub fn captured(&self) -> bool {
        self.drag.is_some() || self.drawing.is_some()
    }
    /// Call on Escape, focus loss, persona/document/tool switch. Draw mode remains a preference.
    pub fn frame(&mut self, s: &Session, active: bool, focused: bool, escape: bool) {
        let fresh = self.drag.as_ref().or_else(|| self.drawing.as_ref().map(|r| &r.capture)).is_none_or(|c| c.check(s).is_ok());
        if !active || !focused || escape || !fresh {
            self.cancel();
        }
    }
    /// Isolation supplies its insertion target at Begin. None means use the current selection.
    pub fn begin_draw(&mut self, s: &Session, insertion_target: Option<LayerId>, fill: Option<Fill>, stroke: Option<ShapeStroke>) -> Result<()> {
        self.cancel();
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let selected = st.selected_layers();
        let target = if self.settings.mode == DrawMode::Inside {
            if selected.len() != 1 {
                return Err(EngineError::Other("Draw Inside needs one selected object".into()));
            }
            insertion_target.or_else(|| selected.first().copied())
        } else {
            insertion_target.or(st.active_layer)
        };
        let capture = Capture::begin(s, &target.into_iter().collect::<Vec<_>>())?;
        self.drawing = Some(DrawRequest { capture, mode: self.settings.mode, target, layer: LayerId::fresh(), path: Path::default(), fill, stroke });
        Ok(())
    }
    /// The shape/Pen/Pencil provider supplies its ordinary document-space Path each frame.
    pub fn preview_draw(&mut self, s: &Session, path: Path) -> Result<()> {
        let request = self.drawing.as_mut().ok_or_else(|| EngineError::Other("draw gesture has not begun".into()))?;
        request.path = path;
        let preview = match engine::preview_draw(s, request) {
            Ok(preview) => preview,
            Err(e) => {
                self.cancel();
                self.error = Some(e.to_string());
                return Err(e);
            }
        };
        self.error = None;
        self.preview = Some(preview.document);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn commit_draw(&mut self, s: &mut Session) -> Result<()> {
        let request = self.drawing.take().ok_or_else(|| EngineError::Other("draw gesture has not begun".into()))?;
        self.preview = None;
        let result = s.execute("object.draw.insert", serde_json::to_value(&request).map_err(|e| EngineError::Other(e.to_string()))?);
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                self.error = Some(e.to_string());
                Err(e)
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    MakeClip,
    ReleaseClip,
    EditMask,
    EditContents,
    MakeCompound,
    ReleaseCompound,
    NonZero,
    EvenOdd,
    MoveInside,
    MoveOutside,
}
impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Self::MakeClip => "Make Clipping Mask",
            Self::ReleaseClip => "Release Clipping Mask",
            Self::EditMask => "Edit Mask",
            Self::EditContents => "Edit Contents",
            Self::MakeCompound => "Make Compound Path",
            Self::ReleaseCompound => "Release Compound Path",
            Self::NonZero => "Non-Zero Fill",
            Self::EvenOdd => "Even-Odd Fill",
            Self::MoveInside => "Move Inside",
            Self::MoveOutside => "Move Outside",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::MakeClip => "object.clippingMask.make",
            Self::ReleaseClip => "object.clippingMask.release",
            Self::EditMask => "object.clippingMask.editMask",
            Self::EditContents => "object.clippingMask.editContents",
            Self::MakeCompound => "object.compoundPath.make",
            Self::ReleaseCompound => "object.compoundPath.release",
            Self::NonZero | Self::EvenOdd => "object.fillRule",
            Self::MoveInside => "object.moveInside",
            Self::MoveOutside => "object.moveOutside",
        }
    }
}
pub fn execute(s: &mut Session, state: &mut State, command: Command) -> Result<()> {
    state.cancel();
    state.mask_target = None;
    let result = (|| {
        let mut capture = Capture::selected(s)?;
        if matches!(command, Command::ReleaseClip) {
            let st = s.active().ok_or(EngineError::NoDocument)?;
            if let [id] = capture.layers.as_slice() {
                capture = Capture::begin(s, &[engine::clip_parent(&st.doc, *id).ok_or_else(|| EngineError::Other("select a clip group".into()))?])?;
            }
        }
        let params = match command {
            Command::NonZero | Command::EvenOdd => {
                json!({"capture":capture,"rule":if matches!(command,Command::NonZero) {FillRule::NonZero} else {FillRule::EvenOdd}})
            }
            Command::MoveInside => {
                let st = s.active().ok_or(EngineError::NoDocument)?;
                let id = capture.layers.last().ok_or_else(|| EngineError::Other("select an object".into()))?;
                let mut path = st.doc.path_of(*id).ok_or(EngineError::NoLayer(*id))?;
                let index = path.last_mut().ok_or_else(|| EngineError::Other("missing object position".into()))?;
                *index += 1;
                let target = st.doc.layer_at(&path).ok_or_else(|| EngineError::Other("there is no object above the selection".into()))?.id;
                json!({"capture":capture,"target":target})
            }
            _ => capture.params()?,
        };
        s.execute(command.id(), params)
    })();
    match result {
        Ok(value) => {
            if matches!(command, Command::EditMask) {
                state.mask_target = value["layers"].as_array().and_then(|a| a.first()).and_then(|v| v.as_u64()).map(LayerId);
            }
            state.error = None;
            Ok(())
        }
        Err(e) => {
            state.error = Some(e.to_string());
            Err(e)
        }
    }
}
pub(crate) fn inside_enabled(s: &Session) -> bool {
    let Some(st) = s.active() else { return false };
    let ids = st.selected_layers();
    let [id] = ids.as_slice() else { return false };
    st.doc.layer(*id).is_some_and(|l| {
        matches!(l.content, photocraft_doc::LayerContent::Shape(_) | photocraft_doc::LayerContent::Text(_) | photocraft_doc::LayerContent::Group(_))
    }) && Capture::begin(s, &[*id]).is_ok()
}
/// Exact modifier matching prevents Release from also triggering Make. Return true when consumed.
pub fn shortcuts(ctx: &egui::Context, s: &mut Session, state: &mut State, vector_persona: bool) -> bool {
    if !vector_persona {
        return false;
    }
    let focus = crate::shortcut_dispatch::Focus::of(ctx);
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    let take_key = |mods: Modifiers, key: Key| {
        if !focus.allows(&egui::KeyboardShortcut::new(mods, key)) {
            return false;
        }
        ctx.input_mut(|i| {
            let at =
                i.events.iter().position(|event| matches!(event,egui::Event::Key {key:k,pressed:true,modifiers,..} if *k==key&&modifiers.matches_exact(mods)));
            if let Some(at) = at {
                i.events.remove(at);
                true
            } else {
                false
            }
        })
    };
    if take_key(shift, Key::D) {
        state.cancel();
        state.settings.mode = match state.settings.mode {
            DrawMode::Normal => DrawMode::Behind,
            DrawMode::Behind if inside_enabled(s) => DrawMode::Inside,
            _ => DrawMode::Normal,
        };
        return true;
    }
    let cmd = Modifiers::COMMAND;
    let alt_cmd = Modifiers { alt: true, ..cmd };
    let release_compound = Modifiers { shift: true, ..alt_cmd };
    for (mods, key, command) in [
        (release_compound, Key::Num8, Command::ReleaseCompound),
        (alt_cmd, Key::Num7, Command::ReleaseClip),
        (cmd, Key::Num7, Command::MakeClip),
        (cmd, Key::Num8, Command::MakeCompound),
    ] {
        // Match the event, not end-of-frame modifiers: a key can press and release
        // in one frame. matches_exact handles physical Ctrl/Cmd and forbids extra Alt/Shift.
        if take_key(mods, key) {
            let _ = execute(s, state, command);
            return true;
        }
    }
    false
}
pub fn draw_mode_widget(ui: &mut Ui, s: &Session, state: &mut State, tr: impl Fn(&str) -> String) {
    ui.horizontal(|ui| {
        for (mode, label) in [(DrawMode::Normal, "Draw Normal"), (DrawMode::Behind, "Draw Behind"), (DrawMode::Inside, "Draw Inside")] {
            if ui
                .add_enabled(mode != DrawMode::Inside || inside_enabled(s), egui::Button::new(tr(label)).selected(state.settings.mode == mode))
                .on_hover_text(tr(label))
                .clicked()
            {
                state.cancel();
                state.settings.mode = mode;
            }
        }
    });
}
/// Menu fragment; caller places it in Object and Layers context menus.
pub fn context_menu(ui: &mut Ui, s: &mut Session, state: &mut State, tr: impl Fn(&str) -> String) {
    for command in [
        Command::MakeClip,
        Command::ReleaseClip,
        Command::EditMask,
        Command::EditContents,
        Command::MakeCompound,
        Command::ReleaseCompound,
        Command::NonZero,
        Command::EvenOdd,
        Command::MoveInside,
        Command::MoveOutside,
    ] {
        if ui.add_enabled(Capture::selected(s).is_ok_and(|c| !c.layers.is_empty()), egui::Button::new(tr(command.label()))).clicked() {
            let _ = execute(s, state, command);
            ui.close();
        }
    }
}
/// Four dashed corners, in screen points. The caller supplies the selected target's bounds.
pub fn indicator(painter: &egui::Painter, state: &State, rect: Rect) {
    if state.settings.mode != DrawMode::Inside || !rect.is_finite() || rect.is_negative() {
        return;
    }
    let stroke = Stroke::new(1., Color32::from_rgb(80, 150, 255));
    let r = rect.expand(4.);
    for (corner, dx, dy) in [(r.left_top(), 1., 1.), (r.right_top(), -1., 1.), (r.left_bottom(), 1., -1.), (r.right_bottom(), -1., -1.)] {
        for offset in [0., 4., 8.] {
            painter.line_segment([corner + egui::vec2(dx * offset, 0.), corner + egui::vec2(dx * (offset + 2.), 0.)], stroke);
            painter.line_segment([corner + egui::vec2(0., dy * offset), corner + egui::vec2(0., dy * (offset + 2.))], stroke);
        }
    }
}
/// Attach to a Layers row response with Sense::click_and_drag. Call layers_end once after
/// all rows, so release/focus cancellation works even outside a row. Hover is a detached preview.
pub fn layer_row(ui: &Ui, response: &Response, id: LayerId, s: &Session, state: &mut State) {
    if response.drag_started() {
        state.cancel();
        let ids = s.active().map(|st| if st.is_layer_selected(id) { st.selected_layers() } else { vec![id] }).unwrap_or_default();
        match Capture::begin(s, &ids) {
            Ok(c) => state.drag = Some(c),
            Err(e) => state.error = Some(e.to_string()),
        }
    }
    if state.drag.is_some() && ui.input(|i| i.pointer.hover_pos().is_some_and(|p| response.rect.contains(p))) {
        state.hovered = Some(id);
        if let Some(c) = &state.drag
            && (state.preview_target != Some(id) || state.preview.is_none())
        {
            match engine::prepare(s, c, &Action::MoveInside(id)) {
                Ok(p) => {
                    state.preview = Some(p.document);
                    state.preview_target = Some(id);
                    state.generation = state.generation.wrapping_add(1);
                }
                Err(_) => {
                    state.preview = None;
                    state.preview_target = None;
                }
            }
        }
        if state.preview.is_some() {
            ui.painter().rect_stroke(response.rect, 0., Stroke::new(2., Color32::LIGHT_BLUE), egui::StrokeKind::Inside);
        }
    }
}
pub fn layers_begin(state: &mut State) {
    state.hovered = None;
}
pub fn layers_end(ui: &Ui, s: &mut Session, state: &mut State) {
    state.frame(s, true, ui.input(|i| i.focused), ui.input(|i| i.key_pressed(Key::Escape)));
    if state.hovered.is_none() {
        state.preview = None;
        state.preview_target = None;
    }
    if ui.input(|i| i.pointer.any_released())
        && let Some(capture) = state.drag.take()
    {
        state.preview = None;
        if let Some(target) = state.hovered.take()
            && !capture.layers.contains(&target)
        {
            match s.execute("object.moveInside", json!({"capture":capture,"target":target})) {
                Ok(_) => state.error = None,
                Err(e) => state.error = Some(e.to_string()),
            }
        }
    }
}
/// Small Layers host used by tests; production applies layer_row to existing rows.
pub fn layers_fragment(ui: &mut Ui, s: &mut Session, state: &mut State) -> Vec<(LayerId, Rect)> {
    let rows = s.active().map(|st| st.doc.walk().into_iter().rev().map(|(_, depth, l)| (l.id, depth, l.name.clone())).collect::<Vec<_>>()).unwrap_or_default();
    layers_begin(state);
    let mut output = Vec::new();
    for (id, depth, name) in rows {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(260., 24.), Sense::click_and_drag());
        ui.painter().text(
            Pos2::new(rect.min.x + 12. * depth as f32, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(14.),
            ui.visuals().text_color(),
        );
        layer_row(ui, &response, id, s, state);
        output.push((id, rect));
    }
    layers_end(ui, s, state);
    output
}

#[cfg(test)]
#[path = "clip_compound/tests.rs"]
mod tests;
