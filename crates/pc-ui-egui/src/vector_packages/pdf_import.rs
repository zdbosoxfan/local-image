//! Package 22 PDF page selection and captured canvas placement. Transient state;
//! File/Open, Place and canvas routing are installed by package 00 (integration/22.md).
use egui::{Context, Event, Key, PointerButton, Pos2, Rect, Ui};
use photocraft_doc::{
    Document,
    vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning},
};
use photocraft_engine::{
    EngineError, Result, Session,
    vector_packages::pdf_import::{self, Place},
};
use photocraft_pdf::{PageBox, PdfInfo};
use std::sync::{Arc, atomic::AtomicBool};

pub struct State {
    source: Vec<u8>,
    name: String,
    pub dialog_open: bool,
    pub info: Option<PdfInfo>,
    pub page: u32,
    pub dpi: f32,
    pub page_box: PageBox,
    pub apply_rotation: bool,
    pub preview: Option<Arc<Document>>,
    pub generation: u64,
    pub warnings: Vec<VectorWarning>,
    warning_document: Option<u64>,
    warning_targets: Vec<Option<photocraft_doc::LayerId>>,
    pub error: Option<String>,
    request: Option<Place>,
    scene: Option<ImportedScene>,
    drag: Option<(Pos2, [i32; 2])>,
    context: Option<(crate::state::Tool, crate::Module)>,
    cancel: Arc<AtomicBool>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            source: vec![],
            name: "PDF".into(),
            dialog_open: false,
            info: None,
            page: 1,
            dpi: 72.0,
            page_box: PageBox::Crop,
            apply_rotation: true,
            preview: None,
            generation: 0,
            warnings: vec![],
            warning_document: None,
            warning_targets: vec![],
            error: None,
            request: None,
            scene: None,
            drag: None,
            context: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl State {
    /// File picker passes bounded bytes; there is no file/network resolution in the adapter.
    pub fn set_source(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.set_named_source("PDF", bytes)
    }
    pub fn set_named_source(&mut self, name: &str, bytes: Vec<u8>) -> Result<()> {
        let result = photocraft_pdf::info(&bytes, &ImportOptions::default(), &Arc::new(AtomicBool::new(false)));
        match result {
            Ok(info) => {
                self.cancel();
                self.warnings.clear();
                self.warning_document = None;
                self.warning_targets.clear();
                self.source = bytes;
                self.name = name.to_owned();
                self.info = Some(info);
                self.page = 1;
                self.error = None;
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.to_string());
                Err(EngineError::Other(e.to_string()))
            }
        }
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.preview = None;
        self.request = None;
        self.scene = None;
        self.drag = None;
        self.context = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn pending(&self) -> bool {
        self.request.is_some()
    }
    pub fn synchronize(&mut self, session: &Session, enabled: bool) {
        if !enabled || self.request.as_ref().is_some_and(|r| pdf_import::check(session, r).is_err()) {
            self.cancel();
        }
    }
    pub fn synchronize_context(&mut self, session: &Session, tool: crate::state::Tool, module: crate::Module, enabled: bool) {
        self.synchronize(session, enabled);
        if self.pending() {
            let current = (tool, module);
            if self.context.is_some_and(|last| last != current) {
                self.cancel();
            } else {
                self.context = Some(current);
            }
        }
    }
    fn options(&self) -> ImportOptions {
        ImportOptions { dpi: self.dpi, pages: vec![self.page.saturating_sub(1) as usize], ..Default::default() }
    }
    fn decode(&self) -> Result<ImportedScene> {
        photocraft_pdf::import_with(
            &self.source,
            &self.options(),
            &photocraft_pdf::PdfOptions { page_box: self.page_box, ignore_rotation: !self.apply_rotation },
            &self.cancel,
        )
        .map_err(|e| EngineError::Other(e.to_string()))
    }
    pub fn stage(&mut self, session: &Session) -> Result<()> {
        self.cancel();
        let mut request = pdf_import::capture(session, self.source.clone())?;
        request.pages = self.options().pages;
        request.page_box = self.page_box;
        request.ignore_rotation = !self.apply_rotation;
        // Place uses destination DPI, so physical page size survives placement.
        let scene = pdf_import::decode(session, &request)?;
        let (preview, _) = pdf_import::preview(session, &request, &scene)?;
        self.warning_targets = warning_targets(&scene);
        self.warning_document = Some(request.document);
        self.warnings = scene.warnings.clone();
        self.request = Some(request);
        self.scene = Some(scene);
        self.preview = Some(Arc::new(preview));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn place(&mut self, session: &mut Session) -> Result<()> {
        if !self.pending() {
            self.stage(session)?;
        }
        let request = self.request.as_ref().ok_or_else(|| EngineError::Other("missing PDF placement".into()))?;
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing PDF scene".into()))?;
        // One Session::edit publishes the prepared caches. Release does not decode again.
        pdf_import::publish(session, request, scene)?;
        self.cancel();
        Ok(())
    }
    pub fn open(&mut self, session: &mut Session) -> Result<()> {
        let scene = self.decode()?;
        self.warning_targets = warning_targets(&scene);
        let (doc, warnings) = pdf_import::document(scene, &self.name);
        self.warning_document = Some(doc.id.0);
        session.add_document(doc, None);
        self.warnings = warnings;
        self.cancel();
        Ok(())
    }
    fn motion(&mut self, session: &Session, at: Pos2) -> Result<()> {
        let Some((origin, base)) = self.drag else { return Ok(()) };
        let Some(request) = &mut self.request else { return Ok(()) };
        let delta = [at.x - origin.x, at.y - origin.y];
        if delta.iter().any(|n| !n.is_finite() || n.abs() > 1_000_000.0) {
            return Err(EngineError::Other("invalid PDF placement pointer".into()));
        }
        request.offset = [base[0].saturating_add(delta[0].round() as i32), base[1].saturating_add(delta[1].round() as i32)];
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing PDF preview".into()))?;
        self.preview = Some(Arc::new(pdf_import::preview(session, request, scene)?.0));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}
pub fn controls(ui: &mut Ui, state: &mut State, session: &mut Session) {
    let count = state.info.as_ref().map_or(0, |info| info.pages.len());
    let mut changed = false;
    ui.horizontal(|ui| {
        let label = ui.label(tl!("PDF page"));
        changed |= ui.add(egui::DragValue::new(&mut state.page).range(1..=count.max(1) as u32)).labelled_by(label.id).changed();
        ui.label(format!("/ {count}"));
        let label = ui.label(tl!("Open DPI"));
        changed |= ui.add(egui::DragValue::new(&mut state.dpi).range(1.0..=2400.0)).labelled_by(label.id).changed();
    });
    egui::ComboBox::from_id_salt("pdf-page-box")
        .selected_text(match state.page_box {
            PageBox::Crop => tl!("CropBox"),
            PageBox::Media => tl!("MediaBox"),
        })
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(&mut state.page_box, PageBox::Crop, tl!("CropBox")).changed();
            changed |= ui.selectable_value(&mut state.page_box, PageBox::Media, tl!("MediaBox")).changed();
        });
    changed |= ui.checkbox(&mut state.apply_rotation, tl!("Apply page rotation")).changed();
    if changed {
        state.cancel();
        state.warnings.clear();
        state.warning_document = None;
        state.warning_targets.clear();
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.add_enabled(count > 0, egui::Button::new(tl!("Open PDF"))).clicked() {
            action = Some(0);
        }
        if ui.add_enabled(count > 0 && session.active().is_some(), egui::Button::new(tl!("Preview PDF"))).clicked() {
            action = Some(1);
        }
        if ui.add_enabled(count > 0 && session.active().is_some(), egui::Button::new(tl!("Place PDF"))).clicked() {
            action = Some(2);
        }
        if ui.add_enabled(state.pending(), egui::Button::new(tl!("Cancel"))).clicked() {
            state.cancel();
        }
    });
    if let Some(a) = action {
        state.error = match a {
            0 => state.open(session),
            1 => state.stage(session),
            _ => state.place(session),
        }
        .err()
        .map(|e| e.to_string());
        if state.error.is_some() {
            state.cancel();
        }
    }
    if let Some(e) = &state.error {
        ui.colored_label(egui::Color32::RED, e);
    }
    warning_controls(ui, state, session);
}
fn warning_targets(scene: &ImportedScene) -> Vec<Option<photocraft_doc::LayerId>> {
    let mut stack: Vec<_> = scene.layers.iter().rev().collect();
    let mut layers = vec![];
    while let Some(layer) = stack.pop() {
        layers.push(layer);
        if let photocraft_doc::LayerContent::Group(group) = &layer.content {
            stack.extend(group.children.iter().rev());
        }
    }
    scene
        .warnings
        .iter()
        .map(|warning| warning.object.as_ref().and_then(|name| layers.iter().filter(|l| &l.name == name).min_by_key(|l| l.is_group()).map(|l| l.id)))
        .collect()
}
pub fn warning_controls(ui: &mut Ui, state: &State, session: &mut Session) {
    for (index, warning) in state.warnings.iter().enumerate() {
        let target = state
            .warning_targets
            .get(index)
            .copied()
            .flatten()
            .filter(|&id| session.active().is_some_and(|s| Some(s.doc.id.0) == state.warning_document && s.doc.layer(id).is_some()));
        let feature = match warning.feature.as_str() {
            "glyph-outlines" => tl!("Text converted to curves"),
            "type3-glyph" => tl!("Glyph artwork converted"),
            "stroke-outline" => tl!("Stroke converted to outlines"),
            "gradient-projection" => tl!("Gradient editing may change appearance"),
            "raster-fallback" => tl!("Paint converted to pixels"),
            "soft-mask" => tl!("Mask converted to pixels"),
            "image-placement" => tl!("Image placement applied"),
            "font-substitution" => tl!("Font substituted"),
            "unsupported-font" => tl!("Font could not be decoded"),
            "image-decode" => tl!("Image could not be decoded"),
            "annotations" => tl!("Annotations omitted"),
            "private-data" => tl!("Editor data omitted"),
            _ => warning.feature.as_str(),
        };
        let label = warning.object.as_ref().map_or_else(|| feature.to_owned(), |o| format!("{feature}: {o}"));
        if ui.add_enabled(target.is_some(), egui::Button::new(label)).on_hover_text(&warning.message).clicked()
            && let Some(id) = target
            && let Some(s) = session.active_mut()
        {
            s.active_layer = Some(id);
            s.selected_layers = vec![id];
        }
    }
}
pub fn input(state: &mut State, session: &mut Session, ctx: &Context, canvas: Rect, to_doc: impl Fn(Pos2) -> Pos2, enabled: bool) -> Result<()> {
    state.synchronize(session, enabled);
    if !state.pending() {
        return Ok(());
    }
    let (events, focused) = ctx.input(|i| (i.events.clone(), i.focused));
    if !focused {
        state.cancel();
        return Ok(());
    }
    for event in events {
        let result = match event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } if canvas.contains(pos) => {
                if let Some(r) = &state.request {
                    state.drag = Some((to_doc(pos), r.offset));
                }
                Ok(())
            }
            Event::PointerMoved(pos) if state.drag.is_some() => state.motion(session, to_doc(pos)),
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, .. } if state.drag.is_some() => {
                state.motion(session, to_doc(pos)).and_then(|()| state.place(session))
            }
            Event::Key { key: Key::Escape, pressed: true, .. } | Event::WindowFocused(false) | Event::PointerGone => {
                state.cancel();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            state.cancel();
            state.error = Some(e.to_string());
            return Err(e);
        }
    }
    Ok(())
}
/// Shared app hooks registered by 00. They share the app's ordinary canvas/GPU caches.
pub fn show(app: &mut crate::PhotocraftApp, ctx: &Context) {
    app.pdf_import.synchronize_context(&app.session, app.ui.tool, app.current_module, app.current_module == crate::Module::Compositing);
    if !app.pdf_import.dialog_open {
        return;
    }
    let mut open = true;
    egui::Window::new(tl!("Import PDF"))
        .default_pos(egui::pos2(20.0, 80.0))
        .open(&mut open)
        .show(ctx, |ui| controls(ui, &mut app.pdf_import, &mut app.session));
    if !open {
        app.pdf_import.dialog_open = false;
        app.pdf_import.cancel();
    }
}
pub fn display_doc(app: &crate::PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    app.pdf_import.preview.as_ref().map(|d| (d.clone(), (1_u64 << 49) | app.pdf_import.generation))
}
pub fn canvas_input(app: &mut crate::PhotocraftApp, response: &egui::Response, xf: &crate::canvas::ViewXform) -> bool {
    let captured = app.pdf_import.pending();
    if captured {
        let press = response.ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } => Some(*pos),
                _ => None,
            })
        });
        let hit = if press.is_some_and(|p| response.ctx.layer_id_at(p) != Some(response.layer_id)) { Rect::NOTHING } else { xf.rect };
        let result = input(
            &mut app.pdf_import,
            &mut app.session,
            &response.ctx,
            hit,
            |p| {
                let d = xf.to_doc(p);
                egui::pos2(d[0] as f32, d[1] as f32)
            },
            true,
        );
        if let Err(e) = result {
            app.ui.status = e.to_string();
        }
    }
    captured
}

/// Common byte-file entry point. Returns true when the page chooser owns the request.
/// This must run before raster decoding/background-job routing in Open and Place.
pub fn offer(app: &mut crate::PhotocraftApp, name: &str, bytes: &[u8]) -> std::result::Result<bool, String> {
    if !bytes.starts_with(b"%PDF-") {
        return Ok(false);
    }
    if bytes.len() > ImportOptions::default().limits.bytes {
        return Err("PDF exceeds the input byte limit".into());
    }
    app.pdf_import.set_named_source(name, bytes.to_vec()).map_err(|e| e.to_string())?;
    app.pdf_import.dialog_open = true;
    app.current_module = crate::Module::Compositing;
    Ok(true)
}
/// Native paths are read with the PDF admission byte limit before the existing raster worker.
#[cfg(not(target_arch = "wasm32"))]
pub fn offer_path(app: &mut crate::PhotocraftApp, path: &str) -> std::result::Result<bool, String> {
    use std::io::Read;
    let pdf_extension = std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let mut probe = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 5];
    let count = probe.read(&mut header).map_err(|e| e.to_string())?;
    if count != 5 || &header != b"%PDF-" {
        if pdf_extension {
            return Err("invalid PDF: missing PDF header".into());
        }
        return Ok(false);
    }
    let limit = ImportOptions::default().limits.bytes;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit as u64 {
        return Err("PDF exceeds the input byte limit".into());
    }
    let mut bytes = vec![];
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("PDF exceeds the input byte limit".into());
    }
    if !offer(app, &crate::file_open::display_name(path), &bytes)? {
        return Err("invalid PDF: missing PDF header".into());
    }
    Ok(true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../pc-pdf/fixtures/import/source.rs"));
    }
    struct Host {
        state: State,
        session: Session,
        canvas: Rect,
    }
    fn harness() -> Harness<'static, Host> {
        let mut session = Session::new();
        session.execute("file.new", serde_json::json!({"width":128,"height":128,"background":"transparent"})).unwrap();
        let mut state = State::default();
        state.set_source(fixture::pages()).unwrap();
        Harness::builder().with_size(egui::vec2(700.0, 500.0)).build_ui_state(
            |ui, h| {
                controls(ui, &mut h.state, &mut h.session);
                let (r, _) = ui.allocate_exact_size(egui::vec2(400.0, 200.0), egui::Sense::click_and_drag());
                h.canvas = r;
                input(&mut h.state, &mut h.session, ui.ctx(), r, |p| p - r.min.to_vec2(), true).unwrap();
            },
            Host { state, session, canvas: Rect::NOTHING },
        )
    }
    #[test]
    fn pdf_import_ui_page_type_open_place_warning_and_undo() {
        let mut h = harness();
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "PDF page").click();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
        h.run_steps(2);
        h.event(Event::Text("2".into()));
        h.step();
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().state.page, 2);
        h.get_by_label("Open PDF").click();
        h.run_steps(2);
        assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
        assert_eq!(h.state().session.active().unwrap().doc.size, photocraft_doc::Size::new(30, 60));
        h.get_by_label("Place PDF").click();
        h.run_steps(2);
        assert!(h.state().state.error.is_none());
        let placed = h.state().session.active().unwrap().active_layer.unwrap();
        h.get_by_label("Text converted to curves: PDF glyph").click();
        h.run_steps(2);
        let s = h.state().session.active().unwrap();
        assert!(matches!(s.doc.layer(s.active_layer.unwrap()).unwrap().content, photocraft_doc::LayerContent::Shape(_)));
        assert!(
            s.doc.path_of(s.active_layer.unwrap()).unwrap().starts_with(&s.doc.path_of(placed).unwrap()),
            "warning must select the newly placed glyph, even when older glyphs share its name"
        );
        assert!(h.state_mut().session.undo());
    }
    #[test]
    fn pdf_import_ui_drag_detached_release_and_cancel() {
        let mut h = harness();
        h.get_by_label("Preview PDF").click();
        h.run_steps(2);
        assert!(h.state().state.pending(), "{:?}", h.state().state.error);
        let rev = h.state().session.active().unwrap().revision;
        let count = h.state().session.active().unwrap().doc.layer_count();
        let a = h.state().canvas.min + egui::vec2(10.0, 10.0);
        let b = a + egui::vec2(20.0, 15.0);
        h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(Event::PointerMoved(b));
        h.step();
        assert_eq!(h.state().session.active().unwrap().revision, rev);
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), count);
        assert!(h.state().state.preview.is_some());
        h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.step();
        assert_eq!(h.state().session.active().unwrap().revision, rev + 1);
        assert!(!h.state().state.pending());
        h.get_by_label("Preview PDF").click();
        h.run_steps(2);
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.step();
        assert!(!h.state().state.pending());
    }
    #[test]
    fn review_invalid_pdf_replacement_retains_valid_source() {
        let mut state = State::default();
        state.set_source(fixture::pages()).unwrap();
        let before = state.source.clone();
        assert!(state.set_source(b"invalid PDF".to_vec()).is_err());
        assert_eq!(state.source, before);
        assert!(state.info.is_some());
    }
}
