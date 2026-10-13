//! Frame text gestures, detached projection and one atomic edit.
//! Package 00 provides Metadata backed by its additive native document field and routes
//! preview through the app compositor. NativeOnly deliberately refuses unpersisted options.
use egui::{Color32, Context, Key, PointerButton, Pos2, Rect, Sense, Stroke};
use photocraft_doc::{
    Document, LayerContent, LayerId,
    text::{TextAlign, TextShape},
};
use photocraft_engine::{EngineError, Result, Session};
use photocraft_text::frame_compose::{self, Options};

fn error(message: impl ToString) -> EngineError {
    EngineError::Other(message.to_string())
}

/// Required integration boundary: options must be part of the undoable native document.
/// Implementations must be deterministic and store only the target's composer metadata.
pub trait Metadata {
    fn load(&self, doc: &Document, id: LayerId) -> Result<Options>;
    fn store(&self, doc: &mut Document, id: LayerId, options: Options) -> Result<()>;
}
pub struct NativeOnly;
impl Metadata for NativeOnly {
    fn load(&self, _: &Document, _: LayerId) -> Result<Options> {
        Ok(Options::default())
    }
    fn store(&self, _: &mut Document, _: LayerId, options: Options) -> Result<()> {
        if options == Options::default() { Ok(()) } else { Err(error("frame composer metadata requires package 00 integration")) }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub layer: LayerId,
    pub frame: [f32; 4],
    pub align: Option<TextAlign>,
    pub options: Options,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub document: u64,
    pub revision: u64,
    pub edits: Vec<Edit>,
}

fn fresh(s: &Session, d: &Draft) -> Result<()> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if st.doc.id.0 != d.document || st.revision != d.revision {
        return Err(error("frame gesture is stale"));
    }
    Ok(())
}
fn preflight(doc: &Document, edits: &[Edit]) -> Result<()> {
    if edits.is_empty() || edits.len() > 64 {
        return Err(error("invalid frame target count"));
    }
    let mut seen = std::collections::HashSet::new();
    for edit in edits {
        if !seen.insert(edit.layer) {
            return Err(error("duplicate frame target"));
        }
        let layer = doc.layer(edit.layer).ok_or(EngineError::NoLayer(edit.layer))?;
        let lock = doc.effective_locks(edit.layer);
        if lock.all || lock.pixels {
            return Err(error("frame target is locked"));
        }
        let LayerContent::Text(t) = &layer.content else {
            return Err(error("frame edit requires text"));
        };
        if !matches!(t.shape, TextShape::Box { .. }) || layer.vector.on_path.is_some() {
            return Err(error("frame edit requires box text"));
        }
        let mut t = t.clone();
        let [x, y, width, height] = edit.frame;
        t.shape = TextShape::Box { x, y, width, height };
        edit.options.validate().map_err(error)?;
        frame_compose::validate(&t, doc.resolution_dpi).map_err(error)?;
    }
    Ok(())
}
fn prepare(s: &Session, d: &Draft) -> Result<Document> {
    fresh(s, d)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    preflight(source, &d.edits)?;
    let mut doc = (**source).clone();
    let clip = doc.bounds();
    let format = doc.pixel_format();
    let dpi = doc.resolution_dpi;
    // Use the same shared font database as ordinary type edits, including user fonts.
    let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
    let mut total_pixels = 0u64;
    (|| {
        for edit in &d.edits {
            let layer = doc.layer_mut(edit.layer).ok_or(EngineError::NoLayer(edit.layer))?;
            let LayerContent::Text(t) = &mut layer.content else {
                return Err(error("frame target disappeared"));
            };
            let [x, y, width, height] = edit.frame;
            t.shape = TextShape::Box { x, y, width, height };
            if let Some(align) = edit.align {
                t.paragraphs = t.paragraph_runs();
                for p in &mut t.paragraphs {
                    p.style.align = align;
                }
            }
            let composed = frame_compose::compose(&mut engine, t, dpi, edit.options).map_err(error)?;
            let rendered = frame_compose::render_bounded(&composed, t, format, clip).map_err(error)?;
            total_pixels += u64::from(rendered.rect.width()) * u64::from(rendered.rect.height());
            if total_pixels > 16_777_216 {
                return Err(error("frame target caches exceed the pixel budget"));
            }
            t.cache = Some(rendered.surface);
            t.psd_raw = Some(std::sync::Arc::new(photocraft_text::psd::build_tysh(t, dpi, composed.layout.bounds())));
            layer.psd_blocks.retain(|(key, _)| key != b"TySh");
        }
        Ok(doc)
    })()
}
/// No document, revision, history or journal mutation.
pub fn preview(s: &Session, d: &Draft) -> Result<Document> {
    prepare(s, d)
}
/// Entire target set and caches are prepared before the sole Session::edit.
pub fn commit(s: &mut Session, d: &Draft, metadata: &impl Metadata) -> Result<()> {
    let mut doc = prepare(s, d)?;
    for e in &d.edits {
        metadata.store(&mut doc, e.layer, e.options)?;
    }
    fresh(s, d)?;
    preflight(&s.active().ok_or(EngineError::NoDocument)?.doc, &d.edits)?;
    s.edit("Frame text", move |target, _| {
        *target = doc;
        Ok(())
    })
}
#[derive(Default)]
pub struct State {
    pub draft: Option<Draft>,
    pub preview: Option<Document>,
    pub generation: u64,
    pub dragging: bool,
}
impl State {
    pub fn cancel(&mut self) {
        self.draft = None;
        self.preview = None;
        self.dragging = false;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn synchronize(&mut self, s: &Session, enabled: bool) {
        if !enabled
            || self.draft.as_ref().is_some_and(|d| fresh(s, d).is_err() || s.active().is_none_or(|st| st.active_layer != d.edits.first().map(|e| e.layer)))
        {
            self.cancel();
        }
    }
    pub fn begin(&mut self, s: &Session, ids: &[LayerId], metadata: &impl Metadata) -> Result<()> {
        if self.draft.as_ref().is_some_and(|d| d.edits.iter().map(|e| e.layer).eq(ids.iter().copied()) && fresh(s, d).is_ok()) {
            return Ok(());
        }
        self.cancel();
        let st = s.active().ok_or(EngineError::NoDocument)?;
        let mut edits = Vec::new();
        for &id in ids {
            let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
            let LayerContent::Text(t) = &layer.content else {
                return Err(error("select frame text"));
            };
            let TextShape::Box { x, y, width, height } = t.shape else {
                return Err(error("select frame text"));
            };
            edits.push(Edit { layer: id, frame: [x, y, width, height], align: None, options: metadata.load(&st.doc, id)? });
        }
        preflight(&st.doc, &edits)?;
        self.draft = Some(Draft { document: st.doc.id.0, revision: st.revision, edits });
        Ok(())
    }
    pub fn update(&mut self, s: &Session) -> Result<()> {
        if let Some(d) = &self.draft {
            let prepared = match preview(s, d) {
                Ok(doc) => doc,
                Err(e) => {
                    self.cancel();
                    return Err(e);
                }
            };
            self.preview = Some(prepared);
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
    pub fn apply(&mut self, s: &mut Session, metadata: &impl Metadata) -> Result<()> {
        self.dragging = false;
        if let Some(d) = &self.draft {
            commit(s, d, metadata)?;
        }
        self.cancel();
        Ok(())
    }
}

/// Context controls using the existing i18n entrypoint. Catalogue rows are supplied to 00.
pub fn panel(ui: &mut egui::Ui, state: &mut State, s: &mut Session, metadata: &impl Metadata) -> Result<()> {
    state.synchronize(s, true);
    let selected = s.active().and_then(|st| {
        let id = st.active_layer?;
        let LayerContent::Text(text) = &st.doc.layer(id)?.content else {
            return None;
        };
        let TextShape::Box { x, y, width, height } = text.shape else {
            return None;
        };
        Some((st, id, text, x, y, width, height))
    });
    let Some((st, id, t, x, y, width, height)) = selected else {
        empty_state(ui);
        return Ok(());
    };
    let current = state.draft.as_ref().and_then(|d| d.edits.first()).cloned().unwrap_or(Edit {
        layer: id,
        frame: [x, y, width, height],
        align: None,
        options: metadata.load(&st.doc, id)?,
    });
    let mut edit = current.clone();
    ui.label(crate::i18n::t("Frame text"));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::t("Width"));
        ui.add(egui::DragValue::new(&mut edit.frame[2]).range(1.0..=1_048_576.0)).labelled_by(ui.label(crate::i18n::t("Frame width")).id);
        ui.label(crate::i18n::t("Height"));
        ui.add(egui::DragValue::new(&mut edit.frame[3]).range(1.0..=1_048_576.0)).labelled_by(ui.label(crate::i18n::t("Frame height")).id);
    });
    ui.checkbox(&mut edit.options.enabled, crate::i18n::t("Paragraph composer"));
    ui.checkbox(&mut edit.options.balanced, crate::i18n::t("Balance lines"));
    ui.checkbox(&mut edit.options.optical_margins, crate::i18n::t("Optical margins"));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::t("Baseline grid"));
        ui.add(egui::DragValue::new(&mut edit.options.grid_spacing).range(0.0..=4096.0)).labelled_by(ui.label(crate::i18n::t("Grid spacing")).id);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::t("Grid origin"));
        ui.add(egui::DragValue::new(&mut edit.options.grid_origin).range(-1_048_576.0..=1_048_576.0))
            .labelled_by(ui.label(crate::i18n::t("Baseline grid origin")).id);
    });
    let mut justify = edit.align.unwrap_or_else(|| t.paragraph_runs().first().map_or(TextAlign::Left, |p| p.style.align)).is_justified();
    if ui.checkbox(&mut justify, crate::i18n::t("Justify paragraph")).changed() {
        edit.align = Some(if justify { TextAlign::JustifyLeft } else { TextAlign::Left });
    }
    if edit != current {
        state.begin(s, &[id], metadata)?;
        if let Some(d) = &mut state.draft {
            d.edits[0] = edit;
        }
        state.update(s)?;
    }
    ui.horizontal(|ui| -> Result<()> {
        if ui.add_enabled(state.draft.is_some(), egui::Button::new(crate::i18n::t("Apply frame text"))).clicked() {
            state.apply(s, metadata)?;
        }
        if ui.button(crate::i18n::t("Cancel frame text")).clicked() {
            state.cancel();
        }
        Ok(())
    })
    .inner
}

/// UX09: say what to select instead of showing an empty window. Point text, shapes and
/// pixel layers have no frame to compose.
fn empty_state(ui: &mut egui::Ui) {
    let tokens = crate::theme::Tokens::get(ui.ctx());
    ui.label(crate::i18n::t("Select a frame text layer"));
    ui.add(
        egui::Label::new(
            egui::RichText::new(crate::i18n::t("Frame Composition works on text set in a frame. Drag with the Frame Text Tool to make one."))
                .color(tokens.text_dim),
        )
        .wrap(),
    );
}

/// Actual frame-width handle in text space. The caller supplies view/transform conversion.
/// Release commits once. Escape, loss of focus, persona/tool changes cancel before release.
pub fn input(
    state: &mut State,
    s: &mut Session,
    ctx: &Context,
    canvas: Rect,
    to_text: impl Fn(Pos2) -> (f32, f32),
    enabled: bool,
    metadata: &impl Metadata,
) -> Result<()> {
    state.synchronize(s, enabled);
    if !enabled {
        return Ok(());
    }
    if ctx.input(|i| !i.focused || i.key_pressed(Key::Escape)) {
        state.cancel();
        return Ok(());
    }
    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        return state.apply(s, metadata);
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let Some(id) = st.active_layer else { return Ok(()) };
    let Some(layer) = st.doc.layer(id) else { return Ok(()) };
    let LayerContent::Text(t) = &layer.content else { return Ok(()) };
    let TextShape::Box { x, y, width, height } = t.shape else { return Ok(()) };
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let Some(pointer) = pointer else {
        if state.dragging {
            state.cancel();
        }
        return Ok(());
    };
    let (px, py) = to_text(pointer);
    if canvas.contains(pointer)
        && ctx.input(|i| i.pointer.button_pressed(PointerButton::Primary))
        && (px - x - width).abs() <= 6.0
        && py >= y
        && py <= y + height
    {
        state.begin(s, &[id], metadata)?;
        state.dragging = true;
    }
    if state.dragging {
        if let Some(d) = &mut state.draft {
            d.edits[0].frame[2] = (px - x).clamp(1.0, 1_048_576.0);
        }
        state.update(s)?;
        if ctx.input(|i| i.pointer.button_released(PointerButton::Primary)) {
            state.apply(s, metadata)?;
        }
    }
    Ok(())
}
pub fn overlay(ui: &mut egui::Ui, state: &State, s: &Session, to_screen: impl Fn(f32, f32) -> Pos2) {
    let Some(doc) = state.preview.as_ref().or_else(|| s.active().map(|s| &*s.doc)) else { return };
    let Some(id) = s.active().and_then(|s| s.active_layer) else { return };
    let Some(layer) = doc.layer(id) else { return };
    let LayerContent::Text(t) = &layer.content else { return };
    let TextShape::Box { x, y, width, height } = t.shape else { return };
    let rect = Rect::from_two_pos(to_screen(x, y), to_screen(x + width, y + height));
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::LIGHT_BLUE), egui::StrokeKind::Outside);
    ui.interact(Rect::from_center_size(to_screen(x + width, y + height * 0.5), egui::vec2(10.0, 10.0)), ui.id().with("frame width"), Sense::hover())
        .on_hover_text(crate::i18n::t("Resize frame width"));
}

/// Native document-backed metadata adapter.
pub struct NativeMetadata;
impl Metadata for NativeMetadata {
    fn load(&self, doc: &Document, id: LayerId) -> Result<Options> {
        let m = doc.layer(id).ok_or(EngineError::NoLayer(id))?.vector.frame_compose.unwrap_or_default();
        let options =
            Options { enabled: m.enabled, balanced: m.balanced, optical_margins: m.optical_margins, grid_spacing: m.grid_spacing, grid_origin: m.grid_origin };
        options.validate().map_err(error)?;
        Ok(options)
    }
    fn store(&self, doc: &mut Document, id: LayerId, options: Options) -> Result<()> {
        options.validate().map_err(error)?;
        doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.vector.frame_compose =
            (options != Options::default()).then_some(photocraft_doc::vector_design::text::FrameCompose {
                enabled: options.enabled,
                balanced: options.balanced,
                optical_margins: options.optical_margins,
                grid_spacing: options.grid_spacing,
                grid_origin: options.grid_origin,
            });
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers};
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn inapplicable_frame_selection_has_a_bounded_nonmutating_empty_state() {
        for kind in ["no-document", "no-selection", "shape", "point-text"] {
            let mut s = Session::new();
            if kind != "no-document" {
                let mut doc = Document::new("inapplicable", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
                let content = if kind == "point-text" {
                    LayerContent::Text(TextLayer { text: "Point".into(), ..Default::default() })
                } else {
                    LayerContent::Shape(Default::default())
                };
                let layer = Layer::new("Selected", content);
                let id = layer.id;
                doc.layers.push(layer);
                s.add_document(doc, None);
                if kind != "no-selection" {
                    s.select_layer(id).unwrap();
                } else {
                    s.active_mut().unwrap().active_layer = None;
                }
            }
            let before: Vec<_> = s.documents().iter().map(|d| d.doc.clone()).collect();
            let mut h = Harness::builder().with_size(egui::vec2(220., 160.)).build_ui_state(
                |ui, host: &mut (Session, State)| {
                    panel(ui, &mut host.1, &mut host.0, &NativeOnly).unwrap();
                },
                (s, State::default()),
            );
            h.run_steps(2);
            let label = h.get_by_label("Select a frame text layer");
            assert!(h.ctx.content_rect().contains_rect(label.rect()));
            let hint = h.get_by_label("Frame Composition works on text set in a frame. Drag with the Frame Text Tool to make one.");
            assert!(h.ctx.content_rect().contains_rect(hint.rect()), "{kind}: hint wraps inside a narrow panel");
            assert!(h.query_by_label("Frame width").is_none());
            assert!(h.state().0.journal.is_empty());
            assert_eq!(h.state().0.documents().iter().map(|d| d.doc.clone()).collect::<Vec<_>>(), before);
        }
    }
    use photocraft_doc::{ColorMode, Layer, SampleType, Size, TextLayer};
    struct Host {
        session: Session,
        state: State,
        rect: Rect,
        error: Option<String>,
    }
    fn host() -> Host {
        let mut doc = Document::new("Frame input", Size::new(300, 200), ColorMode::Rgb, SampleType::U8);
        doc.resolution_dpi = 72.0;
        let mut t = TextLayer {
            text: "one two three four five six seven eight".into(),
            shape: TextShape::Box { x: 10.0, y: 10.0, width: 200.0, height: 120.0 },
            ..Default::default()
        };
        let c = frame_compose::compose(&mut photocraft_text::TextEngine::new(), &t, 72.0, Options::default()).unwrap();
        t.cache = Some(frame_compose::render_bounded(&c, &t, doc.pixel_format(), doc.bounds()).unwrap().surface);
        let l = Layer::new("Frame", LayerContent::Text(t));
        let id = l.id;
        doc.layers.push(l);
        let mut s = Session::new();
        s.add_document(doc, None);
        s.select_layer(id).unwrap();
        Host { session: s, state: State::default(), rect: Rect::NOTHING, error: None }
    }
    fn draw(ui: &mut egui::Ui, h: &mut Host) {
        if let Err(e) = panel(ui, &mut h.state, &mut h.session, &NativeOnly) {
            h.error = Some(e.to_string());
        }
        let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 250.0), Sense::click_and_drag());
        h.rect = rect;
        if let Err(e) = input(&mut h.state, &mut h.session, ui.ctx(), rect, |p| (p.x - rect.min.x, p.y - rect.min.y), true, &NativeOnly) {
            h.error = Some(e.to_string());
        }
        overlay(ui, &h.state, &h.session, |x, y| rect.min + egui::vec2(x, y));
    }
    fn harness() -> Harness<'static, Host> {
        Harness::builder().with_size(egui::vec2(650.0, 650.0)).build_ui_state(draw, host())
    }
    fn pointer(h: &mut Harness<'_, Host>, x: f32, pressed: Option<bool>) {
        let p = h.state().rect.min + egui::vec2(x, 50.0);
        h.event(Event::PointerMoved(p));
        if let Some(pressed) = pressed {
            h.event(Event::PointerButton { pos: p, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
        }
        h.step();
    }
    #[test]
    fn frame_compose_ui_actual_width_drag_preview_release_undo() {
        let mut h = harness();
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.past_len();
        pointer(&mut h, 210.0, Some(true));
        pointer(&mut h, 100.0, None);
        assert!(h.state().error.is_none(), "{:?}", h.state().error);
        assert!(h.state().state.dragging);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        let draft = h.state().state.preview.as_ref().unwrap();
        let mut e = photocraft_text::TextEngine::new();
        let id = before.layers[0].id;
        let LayerContent::Text(a) = &before.layer(id).unwrap().content else { panic!("text") };
        let LayerContent::Text(b) = &draft.layer(id).unwrap().content else { panic!("text") };
        assert!(e.layout(b, 72.0).lines.len() > e.layout(a, 72.0).lines.len());
        pointer(&mut h, 100.0, Some(false));
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        assert!(h.state().state.draft.is_none());
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
    #[test]
    fn frame_compose_ui_actual_paragraph_click_apply_undo() {
        let mut h = harness();
        let before = h.state().session.active().unwrap().doc.clone();
        h.get_by_label("Justify paragraph").click();
        h.step();
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.get_by_label("Apply frame text").click();
        h.step();
        assert!(h.state().error.is_none(), "{:?}", h.state().error);
        let LayerContent::Text(t) = &h.state().session.active().unwrap().doc.layers[0].content else { panic!("text") };
        assert_eq!(t.paragraph_runs()[0].style.align, TextAlign::JustifyLeft);
        h.state_mut().session.undo();
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
    #[test]
    fn frame_compose_ui_actual_option_cancel_escape_and_unpersisted_commit_rejected() {
        let mut h = harness();
        let before = h.state().session.active().unwrap().doc.clone();
        h.get_by_label("Paragraph composer").click();
        h.step();
        assert!(h.state().state.preview.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.get_by_label("Apply frame text").click();
        h.step();
        assert!(h.state().error.as_ref().unwrap().contains("metadata"));
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.key_press(Key::Escape);
        h.step();
        assert!(h.state().state.draft.is_none());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        pointer(&mut h, 210.0, Some(true));
        pointer(&mut h, 80.0, None);
        h.key_press(Key::Escape);
        h.step();
        pointer(&mut h, 80.0, Some(false));
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
    #[test]
    fn frame_compose_ui_actual_type_width_enter_undo() {
        let mut h = harness();
        let before = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.past_len();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Frame width").click();
        h.step();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.step();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Frame width").type_text("80");
        h.step();
        h.key_press(Key::Enter);
        h.step();
        assert!(h.state().error.is_none(), "{:?}", h.state().error);
        let st = h.state().session.active().unwrap();
        let LayerContent::Text(t) = &st.doc.layers[0].content else { panic!("text") };
        assert!(matches!(t.shape,TextShape::Box{width,..} if (width-80.0).abs()<0.01));
        assert_eq!(st.history.past_len(), history + 1);
        h.state_mut().session.undo();
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
    #[test]
    fn review_additional_frame_target_switch_and_failed_release_stop_drag() {
        let mut h = host();
        let first = h.session.active().unwrap().doc.layers[0].id;
        h.state.begin(&h.session, &[first], &NativeOnly).unwrap();
        h.state.dragging = true;
        h.state.draft.as_mut().unwrap().edits[0].options.enabled = true;
        let before = h.session.active().unwrap().doc.clone();
        assert!(h.state.apply(&mut h.session, &NativeOnly).is_err());
        assert!(!h.state.dragging);
        assert!(h.state.draft.is_some());
        assert_eq!(h.session.active().unwrap().doc, before);
        h.session.active_mut().unwrap().active_layer = None;
        h.state.synchronize(&h.session, true);
        assert!(h.state.draft.is_none());
    }
    #[test]
    fn frame_compose_transaction_entire_set_stale_locks_duplicates_nonfinite() {
        let mut h = host();
        let id = h.session.active().unwrap().doc.layers[0].id;
        h.state.begin(&h.session, &[id], &NativeOnly).unwrap();
        let d = h.state.draft.clone().unwrap();
        let before = h.session.active().unwrap().doc.clone();
        let mut bad = d.clone();
        bad.edits.push(bad.edits[0].clone());
        assert!(commit(&mut h.session, &bad, &NativeOnly).is_err());
        assert_eq!(h.session.active().unwrap().doc, before);
        bad = d.clone();
        bad.edits[0].frame[2] = f32::NAN;
        assert!(commit(&mut h.session, &bad, &NativeOnly).is_err());
        assert_eq!(h.session.active().unwrap().doc, before);
        h.session.select_layer(id).unwrap();
        assert!(commit(&mut h.session, &d, &NativeOnly).is_err());
        h.state.synchronize(&h.session, true);
        assert!(h.state.draft.is_none());
        h.session
            .edit("Lock", |doc, _| {
                doc.layer_mut(id).unwrap().locks.all = true;
                Ok(())
            })
            .unwrap();
        assert!(h.state.begin(&h.session, &[id], &NativeOnly).is_err());
    }
}
