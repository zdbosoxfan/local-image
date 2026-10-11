//! Path type interaction host and canvas hooks. Original Local Image code.
//! Detached editing begins with curve/marker capture; Enter or marker release
//! publishes one engine command. Escape, tool/focus/document changes cancel.
use egui::{Color32, Context, Event, Key, Painter, PointerButton, Pos2, Rect, Stroke};
use photocraft_doc::{
    Document, LayerContent, LayerId,
    text::{CharStyle, ParagraphStyle},
    vector_design::text::PathTextAlign,
};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::path_text::{
        self, Commit, Edit, Sampler, Target,
        on_path::{Curve, PathLayout},
    },
};
use photocraft_geom::{Affine, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Start,
    End,
    Baseline,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub draft: Option<Commit>,
    pub preview: Option<Document>,
    pub overlay: Option<PathLayout>,
    pub generation: u64,
    pub dragging: Option<Marker>,
    pub transform: Affine,
    pub orientation: photocraft_doc::text::Orientation,
    pub paragraph: ParagraphStyle,
    pub caret: usize,
    pub anchor: usize,
    synced: Option<(u64, u64, Option<LayerId>)>,
}
impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.draft == other.draft
            && self.preview == other.preview
            && self.generation == other.generation
            && self.dragging == other.dragging
            && self.transform == other.transform
            && self.orientation == other.orientation
            && self.paragraph == other.paragraph
            && self.caret == other.caret
            && self.anchor == other.anchor
            && self.synced == other.synced
    }
}
fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}
impl State {
    pub fn cancel(&mut self) {
        self.draft = None;
        self.preview = None;
        self.overlay = None;
        self.dragging = None;
        self.synced = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) -> Result<()> {
        if !enabled || self.draft.as_ref().is_some_and(|c| path_text::check_fresh(s, c.document, c.revision).is_err()) {
            self.cancel();
            return Ok(());
        }
        if self.draft.is_some() {
            return Ok(());
        }
        let Some(st) = s.active() else {
            self.cancel();
            return Ok(());
        };
        let key = (st.doc.id.0, st.revision, st.active_layer);
        if self.synced == Some(key) {
            return Ok(());
        }
        self.overlay = None;
        if let Some(layer) = st.active_layer.and_then(|id| st.doc.layer(id))
            && let (LayerContent::Text(t), Some(path)) = (&layer.content, &layer.vector.on_path)
        {
            let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
            self.overlay = Some(path_text::project(&mut engine, &st.doc, t, path)?);
            self.transform = t.transform;
        }
        self.synced = Some(key);
        Ok(())
    }
    fn update(&mut self, s: &Session) -> Result<()> {
        let Some(draft) = &self.draft else {
            return Ok(());
        };
        let (preview, ids) = path_text::preview(s, draft)?;
        let edit = draft.edits.first().ok_or_else(|| error("path text draft missing"))?;
        let id = ids.first().copied().ok_or_else(|| error("path text preview missing"))?;
        let layer = preview.layer(id).ok_or(EngineError::NoLayer(id))?;
        let LayerContent::Text(t) = &layer.content else {
            return Err(error("preview is not text"));
        };
        let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
        self.overlay = Some(path_text::project(&mut engine, &preview, t, &edit.path)?);
        self.transform = t.transform;
        self.preview = Some(preview);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    fn begin_selected(&mut self, s: &Session) -> Result<()> {
        if self.draft.is_some() {
            return Ok(());
        }
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let id = st.active_layer.ok_or_else(|| error("select path text"))?;
        let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        if let LayerContent::Text(t) = &layer.content {
            self.caret = t.text.len();
            self.anchor = self.caret;
        }
        let path = layer.vector.on_path.clone().ok_or_else(|| error("select path text"))?;
        self.draft = Some(Commit {
            document: st.doc.id.0,
            revision: st.revision,
            edits: vec![Edit { target: Target::Text { layer: id }, path, text: None, style: None, orientation: None, paragraph: None }],
        });
        Ok(())
    }
    pub fn down(&mut self, s: &Session, at: Point, tolerance: f64, style: CharStyle) -> Result<bool> {
        self.synchronize(s, true)?;
        if let Some(layout) = &self.overlay {
            let nearest = [
                (Marker::Start, self.transform.apply(layout.start)),
                (Marker::End, self.end_marker().unwrap_or(self.transform.apply(layout.end))),
                (Marker::Baseline, self.baseline_marker().unwrap_or(self.transform.apply(layout.start))),
            ]
            .into_iter()
            .find(|(_, p)| (p.x - at.x).hypot(p.y - at.y) <= tolerance);
            if let Some((marker, _)) = nearest {
                self.begin_selected(s)?;
                self.dragging = Some(marker);
                return Ok(true);
            }
        }
        if let Some(layout) = &self.overlay {
            let point = self.transform.inverse().ok_or_else(|| error("invalid text transform"))?.apply(at);
            let (byte, distance) = layout.hit_test_distance(point);
            let inverse = self.transform.inverse().ok_or_else(|| error("invalid text transform"))?;
            let near = inverse.apply(Point::new(at.x + tolerance, at.y + tolerance));
            if distance <= (near.x - point.x).hypot(near.y - point.y) {
                self.begin_selected(s)?;
                self.caret = byte;
                self.anchor = byte;
                self.update(s)?;
                return Ok(true);
            }
        }
        if self.draft.is_some() {
            return Ok(true);
        }
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let Some((curve, path)) = path_text::curve_at(&st.doc, at, tolerance)? else {
            return Ok(false);
        };
        self.caret = 0;
        self.anchor = 0;
        self.draft = Some(Commit {
            document: st.doc.id.0,
            revision: st.revision,
            edits: vec![Edit {
                target: Target::New { curve },
                path,
                text: Some(String::new()),
                style: Some(style),
                orientation: Some(self.orientation),
                paragraph: Some(self.paragraph.clone()),
            }],
        });
        self.update(s)?;
        Ok(true)
    }
    pub fn type_text(&mut self, s: &Session, text: &str) -> Result<()> {
        self.synchronize(s, true)?;
        let Some(draft) = &mut self.draft else {
            return Ok(());
        };
        let edit = &mut draft.edits[0];
        if edit.text.is_none() {
            let st = s.active().ok_or(EngineError::NoDocument)?;
            let Target::Text { layer } = edit.target else {
                return Err(error("text target missing"));
            };
            let Some(LayerContent::Text(t)) = st.doc.layer(layer).map(|l| &l.content) else {
                return Err(error("type layer missing"));
            };
            edit.text = Some(t.text.clone());
        }
        if let Some(current) = &mut edit.text {
            let (a, b) = (self.caret.min(self.anchor).min(current.len()), self.caret.max(self.anchor).min(current.len()));
            if !current.is_char_boundary(a) || !current.is_char_boundary(b) {
                return Err(error("invalid path text caret"));
            }
            current.replace_range(a..b, text);
            self.caret = a + text.len();
            self.anchor = self.caret;
        }
        self.update(s)
    }
    fn draft_text(&self, s: &Session) -> Option<String> {
        let edit = self.draft.as_ref()?.edits.first()?;
        if let Some(text) = &edit.text {
            return Some(text.clone());
        }
        let Target::Text { layer } = edit.target else {
            return None;
        };
        match &s.active()?.doc.layer(layer)?.content {
            LayerContent::Text(t) => Some(t.text.clone()),
            _ => None,
        }
    }
    pub fn backspace(&mut self, s: &Session) -> Result<()> {
        if self.caret == self.anchor
            && let Some(text) = self.draft_text(s)
        {
            self.anchor = text.get(..self.caret).and_then(|t| t.char_indices().next_back()).map_or(self.caret, |(i, _)| i);
        }
        self.type_text(s, "")
    }
    pub fn delete(&mut self, s: &Session) -> Result<()> {
        if self.caret == self.anchor
            && let Some(text) = self.draft_text(s)
        {
            self.anchor = self.caret + text.get(self.caret..).and_then(|t| t.chars().next()).map_or(0, char::len_utf8);
        }
        self.type_text(s, "")
    }
    pub fn navigate(&mut self, s: &Session, right: bool, extend: bool) {
        if let Some(text) = self.draft_text(s) {
            self.caret = if right {
                self.caret + text.get(self.caret..).and_then(|t| t.chars().next()).map_or(0, char::len_utf8)
            } else {
                text.get(..self.caret).and_then(|t| t.char_indices().next_back()).map_or(0, |(i, _)| i)
            };
            if !extend {
                self.anchor = self.caret;
            }
        }
    }
    pub fn select_all(&mut self, s: &Session) {
        if let Some(text) = self.draft_text(s) {
            self.anchor = 0;
            self.caret = text.len();
        }
    }
    pub fn motion(&mut self, s: &Session, at: Point) -> Result<()> {
        self.synchronize(s, true)?;
        let Some(marker) = self.dragging else {
            return Ok(());
        };
        let at = self.transform.inverse().ok_or_else(|| error("invalid text transform"))?.apply(at);
        let Some(edit) = self.draft.as_mut().and_then(|c| c.edits.first_mut()) else {
            return Ok(());
        };
        let sampler = Sampler::new(&edit.path.path)?;
        let fraction = sampler.fraction(at)?;
        match marker {
            Marker::Start => edit.path.start = if sampler.closed() { fraction } else { fraction.min(edit.path.end) },
            Marker::End => edit.path.end = if sampler.closed() { fraction } else { fraction.max(edit.path.start) },
            Marker::Baseline => {
                let (p, t) = sampler.sample(fraction * sampler.length());
                let sign = if edit.path.flip { -1. } else { 1. };
                let normal = (-t.y * sign, t.x * sign);
                let rise = (at.x - p.x) * normal.0 + (at.y - p.y) * normal.1 - 12.;
                let line = self.overlay.as_ref().and_then(|l| l.shaped.lines.first()).ok_or_else(|| error("baseline metrics missing"))?;
                edit.path.align = [
                    (PathTextAlign::Baseline, 0.),
                    (PathTextAlign::Ascender, f64::from(line.ascent)),
                    (PathTextAlign::Center, f64::from(line.ascent - line.descent) * 0.5),
                    (PathTextAlign::Descender, -f64::from(line.descent)),
                ]
                .into_iter()
                .min_by(|a, b| (a.1 - rise).abs().total_cmp(&(b.1 - rise).abs()))
                .map_or(PathTextAlign::Baseline, |p| p.0);
            }
        }
        self.update(s)
    }
    pub fn commit(&mut self, s: &mut Session) -> Result<()> {
        let Some(draft) = &self.draft else {
            return Ok(());
        };
        let params = serde_json::to_value(draft).map_err(|e| error(&e.to_string()))?;
        self.dragging = None;
        s.execute("type.path.commit", params)?;
        self.cancel();
        Ok(())
    }
    pub fn up(&mut self, s: &mut Session) -> Result<()> {
        if self.dragging.is_some() {
            self.commit(s)?;
        }
        Ok(())
    }
    pub fn flip(&mut self, s: &mut Session) -> Result<()> {
        let live = self.draft.is_some();
        self.begin_selected(s)?;
        if let Some(edit) = self.draft.as_mut().and_then(|c| c.edits.first_mut()) {
            edit.path.flip = !edit.path.flip;
        }
        self.update(s)?;
        if !live {
            self.commit(s)?;
        }
        Ok(())
    }
    pub fn align(&mut self, s: &mut Session, align: PathTextAlign) -> Result<()> {
        let live = self.draft.is_some();
        self.begin_selected(s)?;
        if let Some(edit) = self.draft.as_mut().and_then(|c| c.edits.first_mut()) {
            edit.path.align = align;
        }
        self.update(s)?;
        if !live {
            self.commit(s)?;
        }
        Ok(())
    }
    pub fn caret_segment(&self) -> Option<[Point; 2]> {
        let layout = self.overlay.as_ref()?;
        if let Some(caret) = layout.caret_segment(self.caret) {
            return Some(caret);
        }
        if !layout.clusters.is_empty() {
            return None;
        }
        let edit = self.draft.as_ref()?.edits.first()?;
        let sampler = Sampler::new(&edit.path.path).ok()?;
        let at = if edit.path.flip { edit.path.end } else { edit.path.start };
        let (p, t) = sampler.sample(at * sampler.length());
        let sign = if edit.path.flip { -1. } else { 1. };
        let normal = Point::new(-t.y * sign, t.x * sign);
        let (asc, desc) = layout.shaped.lines.first().map_or((16., 4.), |l| (f64::from(l.ascent), f64::from(l.descent)));
        Some([Point::new(p.x - normal.x * asc, p.y - normal.y * asc), Point::new(p.x + normal.x * desc, p.y + normal.y * desc)])
    }
    pub fn baseline_marker(&self) -> Option<Point> {
        let layout = self.overlay.as_ref()?;
        let cluster = layout.clusters.iter().find(|c| c.visible)?;
        let source = layout.shaped.clusters.get(cluster.source_index)?;
        let line = layout.shaped.lines.get(source.line)?;
        let (x, y) = layout.shaped.to_text(source.x + source.advance * 0.5, line.baseline);
        let baseline = cluster.transform.apply(Point::new(f64::from(x), f64::from(y)));
        // Separate the handle from the baseline by 12px; motion removes this
        // offset so pressing/releasing the handle does not change alignment.
        Some(self.transform.apply(Point::new(baseline.x - cluster.angle.sin() * 12., baseline.y + cluster.angle.cos() * 12.)))
    }

    pub fn end_marker(&self) -> Option<Point> {
        let layout = self.overlay.as_ref()?;
        let p = self.transform.apply(layout.end);
        let start = self.transform.apply(layout.start);
        if (start.x - p.x).hypot(start.y - p.y) < 8. {
            // Separate the coincident brackets at a closed seam in document space.
            let angle = layout.clusters.first().map_or(0., |c| c.angle);
            return Some(Point::new(p.x + angle.sin() * 10., p.y - angle.cos() * 10.));
        }
        Some(p)
    }
}

pub fn context(ui: &mut egui::Ui, state: &mut State, s: &mut Session) -> Result<()> {
    let enabled = state.overlay.is_some() || state.draft.is_some();
    if ui.add_enabled(enabled, egui::Button::new(tl!("Flip path text"))).clicked() {
        state.flip(s)?;
    }
    for (align, label) in [
        (PathTextAlign::Baseline, tl!("Baseline")),
        (PathTextAlign::Ascender, tl!("Ascender")),
        (PathTextAlign::Center, tl!("Center")),
        (PathTextAlign::Descender, tl!("Descender")),
    ] {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            state.align(s, align)?;
        }
    }
    if state.overlay.as_ref().is_some_and(|l| l.overflow) {
        ui.colored_label(Color32::RED, tl!("Path text overflow"));
    }
    Ok(())
}

/// Isolated-host raw input adapter. The full app calls the same methods from its
/// established pointer capture/type routing; do not install two event consumers.
#[allow(clippy::too_many_arguments)] // Mirrors the existing canvas capture/input boundary.
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &Context,
    rect: Rect,
    to_doc: impl Fn(Pos2) -> Point,
    enabled: bool,
    tolerance: f64,
    style: &CharStyle,
) -> Result<()> {
    state.synchronize(s, enabled)?;
    if !enabled {
        return Ok(());
    }
    for event in ctx.input(|i| i.events.clone()) {
        let result = match event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if rect.contains(pos) => {
                state.down(s, to_doc(pos), tolerance, style.clone()).map(|_| ())
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } => state.motion(s, to_doc(pos)).and_then(|_| state.up(s)),
            Event::PointerMoved(pos) => state.motion(s, to_doc(pos)),
            Event::Text(text) | Event::Paste(text) | Event::Ime(egui::ImeEvent::Commit(text)) => state.type_text(s, &text),
            Event::Key { key: Key::Backspace, pressed: true, .. } => state.backspace(s),
            Event::Key { key: Key::Delete, pressed: true, .. } => state.delete(s),
            Event::Key { key: Key::ArrowLeft, pressed: true, modifiers, .. } => {
                state.navigate(s, false, modifiers.shift);
                Ok(())
            }
            Event::Key { key: Key::ArrowRight, pressed: true, modifiers, .. } => {
                state.navigate(s, true, modifiers.shift);
                Ok(())
            }
            Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.command || modifiers.ctrl => {
                state.select_all(s);
                Ok(())
            }
            Event::Key { key: Key::Enter, pressed: true, .. } => state.commit(s),
            Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) | Event::PointerGone => {
                state.cancel();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            state.cancel();
            return Err(e);
        }
    }
    Ok(())
}
pub fn paint_overlay(state: &State, painter: &Painter, to_screen: impl Fn(Point) -> Pos2) {
    let Some(layout) = &state.overlay else { return };
    for (p, color) in [
        (state.transform.apply(layout.start), Color32::GREEN),
        (state.end_marker().unwrap_or(state.transform.apply(layout.end)), if layout.overflow { Color32::RED } else { Color32::LIGHT_BLUE }),
    ] {
        painter.circle_filled(to_screen(p), 4., color);
        painter.circle_stroke(to_screen(p), 4., Stroke::new(1., Color32::BLACK));
    }
    if state.draft.is_some()
        && let Some(caret) = state.caret_segment()
    {
        let caret = caret.map(|p| to_screen(state.transform.apply(p)));
        painter.line_segment(caret, Stroke::new(3., Color32::BLACK));
        painter.line_segment(caret, Stroke::new(1., Color32::WHITE));
    }
    if let Some(p) = state.baseline_marker() {
        painter.rect_filled(Rect::from_center_size(to_screen(p), egui::vec2(6., 6.)), 0., Color32::YELLOW);
    }
}
