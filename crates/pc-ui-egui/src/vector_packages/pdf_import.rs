//! PDF page selection and captured canvas placement. Transient state;
//! File/Open, Place and canvas routing are installed by the host app.
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

#[derive(Clone)]
struct Placement {
    target: Place,
    linked_source: Option<String>,
    embedded_confirmed: bool,
}

pub struct State {
    pub background: bool,
    source: Vec<u8>,
    info_worker: Option<crate::import_worker::Task<(Vec<u8>, std::result::Result<PdfInfo, photocraft_pdf::ImportError>)>>,
    pending_source: Option<(String, Vec<u8>, Option<Placement>)>,
    placement: Option<Placement>,
    defer_publication: bool,
    deferred_action: Option<u8>,
    route_finished: Option<(u64, u64, photocraft_doc::LayerId)>,
    raster_service: Option<crate::pdf_raster::Service>,
    fallback_reason: Option<photocraft_pdf::import::RasterFallbackReason>,
    image_mode: bool,
    image_pages: usize,
    image_info_worker: Option<crate::import_worker::Task<crate::pdf_raster::Reply>>,
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
    worker: Option<crate::import_worker::Task<(Option<Place>, std::result::Result<ImportedScene, photocraft_pdf::ImportError>)>>,
    ready_result: Option<Result<(Option<Place>, std::result::Result<ImportedScene, photocraft_pdf::ImportError>)>>,
    worker_action: u8,
    scene: Option<ImportedScene>,
    drag: Option<(Pos2, [i32; 2])>,
    context: Option<(crate::state::Tool, crate::Module)>,
    cancel: Arc<AtomicBool>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            background: false,
            source: vec![],
            info_worker: None,
            pending_source: None,
            placement: None,
            defer_publication: false,
            deferred_action: None,
            route_finished: None,
            raster_service: None,
            fallback_reason: None,
            image_mode: false,
            image_pages: 0,
            image_info_worker: None,
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
            worker: None,
            ready_result: None,
            worker_action: 0,
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
        self.accept_source(name, bytes, result, None)
    }
    fn accept_source(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        result: std::result::Result<PdfInfo, photocraft_pdf::ImportError>,
        placement: Option<Placement>,
    ) -> Result<()> {
        // Malformed/limited replacement must not discard an existing valid source.
        if let Err(error) = &result
            && error.raster_fallback_reason().is_none()
        {
            self.error = Some(error.to_string());
            return Err(EngineError::Other(error.to_string()));
        }
        self.cancel();
        self.source = bytes;
        self.name = name.to_owned();
        self.placement = placement;
        self.info = None;
        self.page = 1;
        self.fallback_reason = None;
        self.image_mode = false;
        self.image_pages = 0;
        self.warnings.clear();
        self.warning_document = None;
        self.warning_targets.clear();
        match result {
            Ok(info) => {
                self.info = Some(info);
                self.error = None;
                Ok(())
            }
            Err(e) => {
                self.fallback_reason = e.raster_fallback_reason();
                self.error = Some(e.to_string());
                Ok(()) // the dialog owns this explicitly unsupported candidate
            }
        }
    }
    fn set_named_source_async(&mut self, name: &str, bytes: Vec<u8>, placement: Option<Placement>) -> Result<()> {
        self.cancel();
        self.pending_source = Some((name.to_owned(), vec![], placement));
        self.info_worker = Some(crate::import_worker::Task::start(move |cancel| {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            let info = photocraft_pdf::info(&bytes, &ImportOptions::default(), cancel);
            Ok((bytes, info))
        })?);
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn set_path_source_async(&mut self, path: &str, placement: Option<Placement>) -> Result<()> {
        self.cancel();
        self.pending_source = Some((crate::file_open::display_name(path), vec![], placement));
        let path = path.to_owned();
        self.info_worker = Some(crate::import_worker::Task::start(move |cancel| {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            let bytes = read_pdf_path(&path).map_err(EngineError::Other)?.ok_or_else(|| EngineError::Other("invalid PDF: missing PDF header".into()))?;
            let info = photocraft_pdf::info(&bytes, &ImportOptions::default(), cancel);
            Ok((bytes, info))
        })?);
        Ok(())
    }
    fn poll_info(&mut self) -> Result<()> {
        if let Some(result) = self.info_worker.as_ref().and_then(|w| w.poll()) {
            self.info_worker = None;
            let (name, _, mut placement) = self.pending_source.take().ok_or_else(|| EngineError::Other("Missing PDF source".into()))?;
            let (bytes, info) = result?;
            if let Some(p) = &mut placement {
                p.target.pdf = bytes.clone();
            }
            self.accept_source(&name, bytes, info, placement)?;
        }
        if let Some(result) = self.image_info_worker.as_ref().and_then(|w| w.poll()) {
            self.image_info_worker = None;
            let crate::pdf_raster::Reply::Metadata { pages } = result? else {
                return Err(EngineError::Other("PDF renderer returned an invalid metadata reply".into()));
            };
            if pages == 0 || pages > crate::pdf_raster::MAX_PAGES {
                return Err(EngineError::Other("PDF image import exceeds the page limit".into()));
            }
            self.image_pages = pages;
            self.error = None;
        }
        Ok(())
    }
    fn choose_image(&mut self) -> Result<()> {
        if self.fallback_reason.is_none() {
            return Err(EngineError::Other("This PDF has no eligible image fallback".into()));
        }
        let service = self.raster_service.clone().ok_or_else(|| EngineError::Other("PDF image import is unavailable on this platform".into()))?;
        self.cancel();
        self.image_mode = true;
        self.image_pages = 0;
        self.error = None;
        let request = crate::pdf_raster::Request {
            bytes: Arc::from(self.source.clone()),
            limits: ImportOptions::default().limits,
            operation: crate::pdf_raster::Operation::Metadata,
        };
        self.image_info_worker = Some(crate::import_worker::Task::start(move |cancel| {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            service(request, cancel).map_err(|e| EngineError::Other(e.to_string()))
        })?);
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.deferred_action = None;
        self.route_finished = None;
        self.worker = None;
        self.ready_result = None;
        self.info_worker = None;
        self.pending_source = None;
        self.image_info_worker = None;
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
        self.request.is_some() || self.worker.is_some() || self.ready_result.is_some() || self.info_worker.is_some() || self.image_info_worker.is_some()
    }
    /// Holds a canvas-drop queue while its page/representation choice is unresolved.
    pub(crate) fn placement_pending(&self) -> bool {
        self.dialog_open && (self.placement.is_some() || self.pending_source.as_ref().is_some_and(|(_, _, placement)| placement.is_some()))
    }
    pub(crate) fn take_route_finished(&mut self, session: &Session) -> bool {
        self.route_finished.take().is_some_and(|(document, revision, layer)| {
            session.active().is_some_and(|s| s.doc.id.0 == document && s.revision == revision && s.active_layer == Some(layer))
        })
    }
    fn cancel_interaction(&mut self) {
        let placing = self.placement_pending();
        self.cancel();
        if placing {
            self.placement = None;
            self.dialog_open = false;
        }
    }
    fn embedding_allowed(&self) -> bool {
        self.placement.as_ref().is_none_or(|p| p.linked_source.is_none() || p.embedded_confirmed)
    }
    pub fn synchronize(&mut self, session: &Session, enabled: bool) {
        let target = match self.pending_source.as_ref() {
            Some((_, _, placement)) => placement.as_ref().map(|p| &p.target),
            None => self.placement.as_ref().map(|p| &p.target).or(self.request.as_ref()),
        };
        if !enabled || target.is_some_and(|r| pdf_import::check(session, r).is_err()) {
            self.cancel_interaction();
        }
    }
    pub fn synchronize_context(&mut self, session: &Session, tool: crate::state::Tool, module: crate::Module, enabled: bool) {
        self.synchronize(session, enabled);
        if self.pending() {
            let current = (tool, module);
            if self.context.is_some_and(|last| last != current) {
                self.cancel_interaction();
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
    fn begin_worker(&mut self, session: &Session, action: u8) -> Result<()> {
        if !self.embedding_allowed() || action == 3 && self.placement.is_some() {
            return Err(EngineError::Other("Confirm embedded placement before importing this PDF".into()));
        }
        if let Some(placement) = &self.placement {
            pdf_import::check(session, &placement.target)?;
        }
        self.cancel();
        let mut request = if action == 3 {
            None
        } else if let Some(placement) = &self.placement {
            Some(placement.target.clone())
        } else {
            Some(pdf_import::capture(session, self.source.clone())?)
        };
        if let Some(r) = &mut request {
            r.pages = self.options().pages;
            r.page_box = self.page_box;
            r.ignore_rotation = !self.apply_rotation;
        }
        let extra = photocraft_pdf::PdfOptions { page_box: self.page_box, ignore_rotation: !self.apply_rotation };
        let dpi = if action == 3 { self.dpi } else { session.active().ok_or(EngineError::NoDocument)?.doc.resolution_dpi };
        let options = ImportOptions { dpi, pages: self.options().pages, ..Default::default() };
        let bytes = self.source.clone();
        let image = self.image_mode;
        let service = self.raster_service.clone();
        self.worker = Some(crate::import_worker::Task::start(move |cancel| {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            let _permit = lightcraft_sysmem::heavy_jobs()
                .admit(bytes.len() as u64 + crate::pdf_raster::MAX_PIXELS * 24, &|| cancel.load(std::sync::atomic::Ordering::Relaxed))
                .ok_or_else(|| EngineError::Other("PDF image import cancelled".into()))?;
            let scene = if image {
                let service = service.ok_or_else(|| EngineError::Other("PDF image import is unavailable on this platform".into()))?;
                let render = crate::pdf_raster::Request {
                    bytes: Arc::from(bytes),
                    limits: options.limits,
                    operation: crate::pdf_raster::Operation::Render {
                        page: options.pages[0],
                        dpi,
                        crop: extra.page_box == PageBox::Crop,
                        apply_rotation: !extra.ignore_rotation,
                    },
                };
                let reply = service(render, cancel).map_err(|e| EngineError::Other(e.to_string()))?;
                Ok(crate::pdf_raster::scene(reply, options.limits, cancel).map_err(|e| EngineError::Other(e.to_string()))?)
            } else {
                photocraft_pdf::import_with(&bytes, &options, &extra, cancel)
            };
            Ok((request, scene))
        })?);
        self.worker_action = action;
        self.error = None;
        Ok(())
    }
    fn collect_worker(&mut self) {
        if self.ready_result.is_some() {
            return;
        }
        let Some(result) = self.worker.as_ref().and_then(|w| w.poll()) else {
            return;
        };
        self.worker = None;
        self.ready_result = Some(result);
    }
    fn poll_worker(&mut self, session: &mut Session) -> Result<()> {
        self.collect_worker();
        let Some(result) = self.ready_result.take() else {
            return Ok(());
        };
        if !self.embedding_allowed() {
            return Err(EngineError::Other("Confirm embedded placement before importing this PDF".into()));
        }
        let (request, scene) = result?;
        // Reject a stale Place before publishing warnings, fallback choices or pixels.
        if let Some(request) = &request {
            pdf_import::check(session, request)?;
        }
        let scene = scene.map_err(|error| {
            self.fallback_reason = error.raster_fallback_reason();
            EngineError::Other(error.to_string())
        })?;
        self.warnings = scene.warnings.clone();
        self.warning_targets = warning_targets(&scene);
        if self.worker_action == 3 {
            let (doc, warnings) = pdf_import::document(scene, &self.name);
            self.warning_document = Some(doc.id.0);
            session.add_document(doc, None);
            self.warnings = warnings;
            return Ok(());
        }
        let request = request.ok_or_else(|| EngineError::Other("Missing import destination".into()))?;
        pdf_import::check(session, &request)?;
        self.warning_document = Some(request.document);
        if self.worker_action != 0 {
            pdf_import::publish(session, &request, &scene)?;
            if self.placement.take().is_some() {
                self.dialog_open = false;
                self.route_finished = session.active().and_then(|s| s.active_layer.map(|layer| (request.document, s.revision, layer)));
            }
            return Ok(());
        }
        let (preview, _) = pdf_import::preview(session, &request, &scene)?;
        self.request = Some(request);
        self.scene = Some(scene);
        self.preview = Some(Arc::new(preview));
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
    pub fn stage(&mut self, session: &Session) -> Result<()> {
        if !self.embedding_allowed() {
            return Err(EngineError::Other("Confirm embedded placement before importing this PDF".into()));
        }
        self.cancel();
        let mut request = if let Some(placement) = &self.placement { placement.target.clone() } else { pdf_import::capture(session, self.source.clone())? };
        pdf_import::check(session, &request)?;
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
        if !self.embedding_allowed() {
            return Err(EngineError::Other("Confirm embedded placement before importing this PDF".into()));
        }
        if !self.pending() {
            self.stage(session)?;
        }
        let request = self.request.as_ref().ok_or_else(|| EngineError::Other("missing PDF placement".into()))?;
        let scene = self.scene.as_ref().ok_or_else(|| EngineError::Other("missing PDF scene".into()))?;
        // One Session::edit publishes the prepared caches. Release does not decode again.
        pdf_import::publish(session, request, scene)?;
        let completed = session.active().and_then(|s| s.active_layer.map(|layer| (request.document, s.revision, layer)));
        self.cancel();
        if self.placement.take().is_some() {
            self.dialog_open = false;
            self.route_finished = completed;
        }
        Ok(())
    }
    pub fn open(&mut self, session: &mut Session) -> Result<()> {
        if self.placement.is_some() {
            return Err(EngineError::Other("This PDF was selected for placement in its captured document".into()));
        }
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
    fn publish_frame(&mut self, session: &mut Session) {
        if let Some(action) = self.deferred_action.take() {
            let result = if !self.background && !self.image_mode {
                match action {
                    0 => self.open(session),
                    1 => self.stage(session),
                    _ => self.place(session),
                }
            } else if action == 2 && self.request.is_some() {
                self.place(session)
            } else {
                self.begin_worker(
                    session,
                    match action {
                        0 => 3,
                        1 => 0,
                        _ => 1,
                    },
                )
            };
            if let Err(error) = result {
                self.error = Some(error.to_string());
                self.cancel_interaction();
            }
        }
        if let Err(error) = self.poll_worker(session) {
            self.error = Some(error.to_string());
        }
    }
}
fn cancellation_frame(ctx: &Context) -> bool {
    let owns_escape = escape_available(ctx);
    ctx.input(|i| {
        !i.focused
            || i.raw.events.iter().any(|event| matches!(event, Event::WindowFocused(false) | Event::PointerGone))
            || owns_escape && i.events.iter().any(|event| matches!(event, Event::Key { key: Key::Escape, pressed: true, .. }))
    })
}
fn publication_available(ctx: &Context) -> bool {
    let focus = crate::shortcut_dispatch::Focus::of(ctx);
    let escape = ctx.input(|i| i.raw.events.iter().any(|event| matches!(event, Event::Key { key: Key::Escape, pressed: true, .. })));
    !ctx.input(|i| i.pointer.any_down())
        && focus != crate::shortcut_dispatch::Focus::Text
        // A consumed Escape can have closed another owner earlier in this pass. Hold
        // publication for the whole pass, but cancel only on an unconsumed owned Escape.
        && !escape
        && !egui::Popup::is_any_open(ctx)
        && ctx.memory(|m| m.top_modal_layer().is_none())
}
fn escape_available(ctx: &Context) -> bool {
    crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None
        && !egui::Popup::is_any_open(ctx)
        && ctx.memory(|m| m.top_modal_layer().is_none())
}
pub fn controls(ui: &mut Ui, state: &mut State, session: &mut Session) {
    if cancellation_frame(ui.ctx()) {
        state.cancel_interaction();
        return;
    }
    state.collect_worker();
    if state.worker.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }

    if let Err(error) = state.poll_info() {
        state.error = Some(error.to_string());
    }
    if state.info_worker.is_some() || state.image_info_worker.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
    let source_name = state.pending_source.as_ref().map_or(state.name.as_str(), |(name, _, _)| name.as_str());
    ui.label(format!("{}: {source_name}", tl!("PDF source")));
    let mut link_changed = false;
    if state.pending_source.is_none()
        && let Some(placement) = &mut state.placement
        && placement.linked_source.is_some()
    {
        ui.label(tl!("PDF placement cannot keep a live link to the source file."));
        link_changed = crate::widgets::checkbox(ui, &mut placement.embedded_confirmed, tl!("Place embedded artwork instead (no live link)")).changed();
    }
    if link_changed {
        state.cancel();
    }
    if state.pending_source.is_none() && state.fallback_reason.is_some() {
        ui.label(tl!("This PDF cannot be imported as editable artwork."));
        ui.label(tl!("Image import uses an opaque white page. Text and paths will not be editable; keep the original PDF."));
        #[cfg(target_os = "linux")]
        ui.label(tl!("Image fallback requires system Poppler, bubblewrap, util-linux and a systemd user session."));
        if state.raster_service.is_none() {
            ui.label(tl!("PDF image import is unavailable on this platform"));
        }
        if ui
            .add_enabled_ui(state.raster_service.is_some() && !state.pending(), |ui| crate::widgets::primary_button(ui, tl!("Use image fallback"), 0.0))
            .inner
            .clicked()
            && let Err(error) = state.choose_image()
        {
            state.error = Some(error.to_string());
        }
    }
    let count = if state.image_mode {
        state.image_pages
    } else if state.fallback_reason.is_some() {
        0
    } else {
        state.info.as_ref().map_or(0, |info| info.pages.len())
    };
    let mut changed = false;
    egui::Grid::new("pdf-import-settings").num_columns(3).show(ui, |ui| {
        let label = ui.label(tl!("PDF page"));
        changed |= crate::widgets::value_field_with_precision(ui, &mut state.page, 1..=count.max(1) as u32, "", ui.spacing().interact_size.x * 3.0, 0)
            .labelled_by(label.id)
            .changed();
        ui.label(format!("/ {count}"));
        ui.end_row();
        let label = ui.label(tl!("Open DPI"));
        changed |= crate::widgets::value_field_with_precision(ui, &mut state.dpi, 1.0..=2400.0, "", ui.spacing().interact_size.x * 3.0, 1)
            .labelled_by(label.id)
            .changed();
        ui.end_row();
    });
    changed |= crate::widgets::dropdown(
        ui,
        "pdf-page-box",
        &mut state.page_box,
        &[(PageBox::Crop, "CropBox"), (PageBox::Media, "MediaBox")],
        ui.spacing().interact_size.x * 5.0,
    );
    changed |= crate::widgets::checkbox(ui, &mut state.apply_rotation, tl!("Apply page rotation")).changed();
    if changed {
        let info_worker = state.info_worker.take();
        let pending_source = state.pending_source.take();
        let image_info_worker = state.image_info_worker.take();
        state.cancel();
        state.info_worker = info_worker;
        state.pending_source = pending_source;
        state.image_info_worker = image_info_worker;
        state.warnings.clear();
        state.warning_document = None;
        state.warning_targets.clear();
    }
    let ready = count > 0
        && state.info_worker.is_none()
        && state.image_info_worker.is_none()
        && state.worker.is_none()
        && state.ready_result.is_none()
        && state.embedding_allowed();
    let mut action = None;
    if state.info_worker.is_some() || state.image_info_worker.is_some() || state.worker.is_some() || state.ready_result.is_some() {
        ui.label(tl!("Working…"));
        let t = crate::theme::Tokens::get(ui.ctx());
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.spacing().item_spacing.y), egui::Sense::hover());
        li_ui_kit::progress::bar(ui, rect, None, &t);
    }
    use crate::widgets::{ButtonRole, DialogButton};
    let open = if state.image_mode { tl!("Open as image") } else { tl!("Open PDF") };
    let preview = if state.image_mode { tl!("Preview image") } else { tl!("Preview PDF") };
    let place = if state.image_mode { tl!("Place as image") } else { tl!("Place PDF") };
    // Separate rows keep long translated choices accessible in a narrow import window.
    match crate::widgets::dialog_buttons(
        ui,
        &[
            DialogButton::new(ButtonRole::Default, open, 0.0).enabled(ready && state.placement.is_none()),
            DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 0.0),
        ],
    ) {
        Some(ButtonRole::Default) => action = Some(0),
        Some(ButtonRole::Cancel) => {
            state.cancel_interaction();
            state.dialog_open = false;
        }
        _ => {}
    }
    if crate::widgets::dialog_buttons(ui, &[DialogButton::new(ButtonRole::Alternate, preview, 0.0).enabled(ready && session.active().is_some())])
        == Some(ButtonRole::Alternate)
    {
        action = Some(1);
    }
    if crate::widgets::dialog_buttons(ui, &[DialogButton::new(ButtonRole::Apply, place, 0.0).enabled(ready && session.active().is_some())])
        == Some(ButtonRole::Apply)
    {
        action = Some(2);
    }
    if action.is_some() {
        state.deferred_action = action;
        state.error = None;
    }
    if let Some(e) = &state.error {
        let tokens = crate::theme::Tokens::get(ui.ctx());
        // The initial editable refusal explains the choice; runtime failures remain errors.
        let color = if state.fallback_reason.is_some() && !state.image_mode { tokens.warning } else { tokens.danger };
        ui.colored_label(color, e);
    }
    warning_controls(ui, state, session);
    // Ready worker output is considered only after this frame's Cancel and option changes.
    // Window hosts defer this further until the close button has also been processed.
    if !state.defer_publication && publication_available(ui.ctx()) {
        state.publish_frame(session);
    }
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
            "pdf-page-image" => tl!("PDF page imported as image"),
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
        if ui.add_enabled_ui(target.is_some(), |ui| crate::widgets::secondary_button(ui, &label, 0.0)).inner.on_hover_text(&warning.message).clicked()
            && let Some(id) = target
            && let Some(s) = session.active_mut()
        {
            s.active_layer = Some(id);
            s.selected_layers = vec![id];
        }
    }
}
pub fn input(state: &mut State, session: &mut Session, ctx: &Context, canvas: Rect, to_doc: impl Fn(Pos2) -> Pos2, enabled: bool) -> Result<()> {
    if cancellation_frame(ctx) {
        state.cancel_interaction();
        return Ok(());
    }
    state.collect_worker();
    if state.worker.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    state.synchronize(session, enabled);
    if !state.pending() {
        return Ok(());
    }
    let (events, focused) = ctx.input(|i| (i.events.clone(), i.focused));
    if !focused {
        state.cancel_interaction();
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
            Event::Key { key: Key::Escape, pressed: true, .. } if escape_available(ctx) => {
                state.cancel_interaction();
                Ok(())
            }
            Event::WindowFocused(false) | Event::PointerGone => {
                state.cancel_interaction();
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
/// Shared app hooks. They share the app's ordinary canvas/GPU caches.
fn app_modal_open(app: &crate::PhotocraftApp, ctx: &Context) -> bool {
    // Match the app-known custom modal ownership used by the shortcut router. These
    // editors paint after the vector/PDF hook, so egui's previous modal layer is insufficient.
    !app.ui.dialogs.is_empty()
        || crate::restoration_ui::is_open(app)
        || app.camera_raw.is_some()
        || app.develop_filter.prompting()
        || app.draft_refine.open && ctx.embed_viewports()
}
pub fn show(app: &mut crate::PhotocraftApp, ctx: &Context) {
    // PC dialogs are opened before this hook but painted later. Their source state must
    // reserve the first frame too, before egui has installed a top modal layer.
    if app_modal_open(app, ctx) {
        app.pdf_import.collect_worker();
        if app.pdf_import.worker.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        return;
    }
    app.pdf_import.background = app.background_jobs;
    app.pdf_import.raster_service = app.services.pdf_raster.clone();
    app.pdf_import.synchronize_context(&app.session, app.ui.tool, app.current_module, app.current_module == crate::Module::Compositing);
    if !app.pdf_import.dialog_open {
        return;
    }
    let mut open = true;
    app.pdf_import.defer_publication = true;
    egui::Window::new(tl!("Import PDF"))
        .default_pos(ctx.content_rect().min + ctx.global_style().spacing.item_spacing * 4.0)
        .open(&mut open)
        .show(ctx, |ui| controls(ui, &mut app.pdf_import, &mut app.session));
    app.pdf_import.defer_publication = false;
    if !open {
        app.pdf_import.cancel_interaction();
        app.pdf_import.dialog_open = false;
    } else if app.pdf_import.dialog_open && !cancellation_frame(ctx) && publication_available(ctx) {
        app.pdf_import.publish_frame(&mut app.session);
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
    if app_modal_open(app, &response.ctx) {
        app.pdf_import.collect_worker();
        return captured;
    }
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
    offer_with_placement(app, name, bytes, None)
}
/// Shared File/Place and byte-drop route. Capture the destination before metadata work.
pub(crate) fn offer_place(app: &mut crate::PhotocraftApp, name: &str, bytes: &[u8], linked: Option<String>) -> std::result::Result<bool, String> {
    if photocraft_pdf::import::header_offset(bytes).is_none() {
        return Ok(false);
    }
    if app.pdf_import.placement_pending() {
        return Err("Finish or cancel the current PDF placement first".into());
    }
    if bytes.len() > ImportOptions::default().limits.bytes {
        return Err("PDF exceeds the input byte limit".into());
    }
    let target = pdf_import::capture(&app.session, bytes.to_vec()).map_err(|e| e.to_string())?;
    offer_with_placement(app, name, bytes, Some(Placement { target, linked_source: linked, embedded_confirmed: false }))
}
fn offer_with_placement(app: &mut crate::PhotocraftApp, name: &str, bytes: &[u8], placement: Option<Placement>) -> std::result::Result<bool, String> {
    if photocraft_pdf::import::header_offset(bytes).is_none() {
        return Ok(false);
    }
    if bytes.len() > ImportOptions::default().limits.bytes {
        return Err("PDF exceeds the input byte limit".into());
    }
    app.pdf_import.background = app.background_jobs;
    app.pdf_import.raster_service = app.services.pdf_raster.clone();
    if app.background_jobs {
        app.pdf_import.set_named_source_async(name, bytes.to_vec(), placement)
    } else {
        let result = photocraft_pdf::info(bytes, &ImportOptions::default(), &AtomicBool::new(false));
        app.pdf_import.accept_source(name, bytes.to_vec(), result, placement)
    }
    .map_err(|e| e.to_string())?;
    app.pdf_import.dialog_open = true;
    app.current_module = crate::Module::Compositing;
    Ok(true)
}
/// Native paths are read with the PDF admission byte limit before the existing raster worker.
#[cfg(not(target_arch = "wasm32"))]
pub fn offer_path(app: &mut crate::PhotocraftApp, path: &str) -> std::result::Result<bool, String> {
    if app.background_jobs {
        let extension = std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !extension && !pdf_path_candidate(path)? {
            return Ok(false);
        }
        app.pdf_import.background = true;
        app.pdf_import.raster_service = app.services.pdf_raster.clone();
        app.pdf_import.set_path_source_async(path, None).map_err(|e| e.to_string())?;
        app.pdf_import.dialog_open = true;
        app.current_module = crate::Module::Compositing;
        return Ok(true);
    }
    let Some(bytes) = read_pdf_path(path)? else {
        return Ok(false);
    };
    if !offer(app, &crate::file_open::display_name(path), &bytes)? {
        return Err("invalid PDF: missing PDF header".into());
    }
    Ok(true)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn offer_place_path(app: &mut crate::PhotocraftApp, path: &str) -> std::result::Result<bool, String> {
    offer_place_path_with_link(app, path, None)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn offer_place_path_with_link(app: &mut crate::PhotocraftApp, path: &str, linked: Option<String>) -> std::result::Result<bool, String> {
    if app.background_jobs {
        let extension = std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !extension && !pdf_path_candidate(path)? {
            return Ok(false);
        }
        if app.pdf_import.placement_pending() {
            return Err("Finish or cancel the current PDF placement first".into());
        }
        let target = pdf_import::capture(&app.session, vec![]).map_err(|e| e.to_string())?;
        app.pdf_import.background = true;
        app.pdf_import.raster_service = app.services.pdf_raster.clone();
        app.pdf_import.set_path_source_async(path, Some(Placement { target, linked_source: linked, embedded_confirmed: false })).map_err(|e| e.to_string())?;
        app.pdf_import.dialog_open = true;
        app.current_module = crate::Module::Compositing;
        return Ok(true);
    }
    let Some(bytes) = read_pdf_path(path)? else {
        return Ok(false);
    };
    if !offer_place(app, path, &bytes, linked)? {
        return Err("invalid PDF: missing PDF header".into());
    }
    Ok(true)
}
#[cfg(target_arch = "wasm32")]
pub(crate) fn offer_place_path(_app: &mut crate::PhotocraftApp, _path: &str) -> std::result::Result<bool, String> {
    Ok(false)
}
#[cfg(not(target_arch = "wasm32"))]
fn pdf_path_candidate(path: &str) -> std::result::Result<bool, String> {
    use std::io::Read;
    let pdf_extension = std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    photocraft_format::read::validate_path(std::path::Path::new(path)).map_err(|e| e.to_string())?;
    let mut probe = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = Vec::new();
    (&mut probe).take(1032).read_to_end(&mut header).map_err(|e| e.to_string())?;
    if photocraft_pdf::import::header_offset(&header).is_none() {
        if pdf_extension {
            return Err("invalid PDF: missing PDF header".into());
        }
        return Ok(false);
    }
    Ok(true)
}
#[cfg(not(target_arch = "wasm32"))]
fn read_pdf_path(path: &str) -> std::result::Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    if !pdf_path_candidate(path)? {
        return Ok(None);
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
    Ok(Some(bytes))
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
    fn fonts_ready(ctx: &Context) -> bool {
        let id = egui::Id::new("pdf-test-fonts");
        if ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
            return true;
        }
        crate::theme::install_fonts(ctx);
        crate::theme::apply(ctx, crate::theme::ThemeKind::Pro);
        ctx.data_mut(|d| d.insert_temp(id, true));
        ctx.request_repaint();
        false
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
        let mut h = Harness::builder().with_size(egui::vec2(700.0, 500.0)).build_ui_state(
            |ui, h| {
                if !fonts_ready(ui.ctx()) {
                    return;
                }
                controls(ui, &mut h.state, &mut h.session);
                let (r, _) = ui.allocate_exact_size(egui::vec2(400.0, 200.0), egui::Sense::click_and_drag());
                h.canvas = r;
                input(&mut h.state, &mut h.session, ui.ctx(), r, |p| p - r.min.to_vec2(), true).unwrap();
            },
            Host { state, session, canvas: Rect::NOTHING },
        );
        h.run_steps(3);
        h
    }
    #[test]
    fn pdf_worker_cancel_and_stale_destination_do_not_publish() {
        let _serial = crate::import_worker::test_serial();
        let mut session = Session::new();
        session.execute("file.new", serde_json::json!({"width":128,"height":128})).unwrap();
        let before = session.active().unwrap().doc.layer_count();
        let mut state = State { source: fixture::pages(), background: true, ..Default::default() };
        state.begin_worker(&session, 1).unwrap();
        assert_eq!(session.active().unwrap().doc.layer_count(), before);
        state.cancel();
        state.poll_worker(&mut session).unwrap();
        assert_eq!(session.active().unwrap().doc.layer_count(), before);
        state.begin_worker(&session, 1).unwrap();
        session.execute("layer.new.layer", serde_json::json!({"name":"New"})).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if state.poll_worker(&mut session).is_err() {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        assert_eq!(session.active().unwrap().doc.layer_count(), before + 1);
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

    fn unsupported_pdf() -> Vec<u8> {
        fixture::page(
            "/S sh",
            "/Shading << /S 5 0 R >>",
            "",
            &[
                b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 64 0] /Function 6 0 R /Extend [true true] >>".to_vec(),
                fixture::stream("/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]", b"{ pop 1 0 0 }"),
            ],
        )
    }
    fn image_service() -> crate::pdf_raster::Service {
        Arc::new(|request, cancel| {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(crate::pdf_raster::Error::Cancelled);
            }
            Ok(match request.operation {
                crate::pdf_raster::Operation::Metadata => crate::pdf_raster::Reply::Metadata { pages: 1 },
                crate::pdf_raster::Operation::Render { dpi, .. } => {
                    crate::pdf_raster::Reply::Pixels { width: 2, height: 1, dpi, rgba: vec![255, 0, 0, 255, 0, 0, 255, 255] }
                }
            })
        })
    }
    fn finish_info(state: &mut State) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.info_worker.is_some() || state.image_info_worker.is_some() {
            state.poll_info().unwrap();
            assert!(std::time::Instant::now() < deadline, "metadata worker timed out");
            std::thread::yield_now();
        }
    }
    fn finish_render(state: &mut State, session: &mut Session) -> Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.worker.is_some() || state.ready_result.is_some() {
            state.poll_worker(session)?;
            assert!(std::time::Instant::now() < deadline, "render worker timed out");
            std::thread::yield_now();
        }
        Ok(())
    }
    #[test]
    fn unsupported_pdf_has_reachable_image_choice_before_native_metadata() {
        let _serial = crate::import_worker::test_serial();
        let mut h = harness();
        h.state_mut().state.raster_service = Some(image_service());
        h.state_mut().state.set_named_source("calculator.pdf", unsupported_pdf()).unwrap();
        assert!(h.state().state.info.is_none());
        assert!(h.state().state.fallback_reason.is_some());
        h.run_steps(2);
        h.get_by_label("Use image fallback").click();
        h.run_steps(2);
        finish_info(&mut h.state_mut().state);
        h.run_steps(2);
        assert_eq!(h.state().state.image_pages, 1);
        let before = h.state().session.active().unwrap().doc.clone();
        h.get_by_label("Preview image").click();
        h.run_steps(2);
        {
            let host = h.state_mut();
            finish_render(&mut host.state, &mut host.session).unwrap();
        }
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().state.preview.is_some());
        h.run_steps(2);
        h.get_by_label("Place as image").click();
        h.run_steps(2);
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), before.layer_count() + 2);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state_mut().session.redo());
    }
    #[test]
    fn image_render_staleness_cancellation_and_open_without_document() {
        let _serial = crate::import_worker::test_serial();
        let mut session = Session::new();
        let mut state = State { raster_service: Some(image_service()), ..Default::default() };
        state.set_source(unsupported_pdf()).unwrap();
        state.choose_image().unwrap();
        finish_info(&mut state);
        state.begin_worker(&session, 3).unwrap();
        finish_render(&mut state, &mut session).unwrap();
        assert_eq!(session.documents().len(), 1);
        assert!(session.active().unwrap().doc.walk().iter().any(|(_, _, l)| matches!(l.content, photocraft_doc::LayerContent::Raster(_))));
        state.begin_worker(&session, 1).unwrap();
        session.execute("layer.new.layer", serde_json::json!({"name":"changed"})).unwrap();
        let before = session.active().unwrap().doc.clone();
        assert!(finish_render(&mut state, &mut session).is_err());
        assert_eq!(session.active().unwrap().doc, before);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (stopped_tx, stopped_rx) = std::sync::mpsc::channel();
        state.raster_service = Some(Arc::new(move |_, cancel| {
            started_tx.send(()).unwrap();
            while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::yield_now();
            }
            stopped_tx.send(()).unwrap();
            Err(crate::pdf_raster::Error::Cancelled)
        }));
        state.begin_worker(&session, 1).unwrap();
        started_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        state.cancel();
        stopped_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        state.poll_worker(&mut session).unwrap();
        assert_eq!(session.active().unwrap().doc, before);
    }
    #[test]
    fn leading_header_is_offered_from_bytes_and_native_path() {
        let _serial = crate::import_worker::test_serial();
        let mut app = crate::PhotocraftApp::new(Session::new(), crate::Services::default());
        app.background_jobs = true;
        let mut bytes = vec![b' '; 1024];
        bytes.extend_from_slice(&fixture::artwork());
        assert!(offer(&mut app, "leading.pdf", &bytes).unwrap());
        app.pdf_import.cancel();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = std::env::temp_dir().join(format!(
                "li-pdf-route-{}-{}.pdf",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::write(&path, &bytes).unwrap();
            assert!(offer_path(&mut app, path.to_str().unwrap()).unwrap());
            app.pdf_import.cancel();
            std::fs::remove_file(path).unwrap();
        }
    }
    fn route_app(pick: Option<(String, Vec<u8>)>) -> crate::PhotocraftApp {
        let mut session = Session::new();
        session.execute("file.new", serde_json::json!({"width":128,"height":128,"background":"transparent"})).unwrap();
        let mut pick = pick;
        let services = crate::Services {
            pdf_raster: Some(image_service()),
            file_dialog: Some(crate::file_dialog::answering(move |_| pick.take().map(|(name, bytes)| crate::FileDialogAnswer::File(name, Ok(bytes))))),
            ..Default::default()
        };
        let mut app = crate::PhotocraftApp::new(session, services);
        app.background_jobs = false;
        app
    }
    #[test]
    fn actual_place_picker_retains_target_and_requires_explicit_link_conversion() {
        let _serial = crate::import_worker::test_serial();
        for linked in [false, true] {
            let original = unsupported_pdf();
            let path = "/chosen/source-calculator.pdf";
            let mut app = route_app(Some((path.into(), original.clone())));
            let before = app.session.active().unwrap().doc.clone();
            let selected = app.session.active().unwrap().selected_layers.clone();
            let result =
                crate::menus::invoke(&mut app, &Context::default(), if linked { "file.placeLinked" } else { "file.placeEmbedded" }, serde_json::json!({}))
                    .unwrap();
            assert_eq!(result["pending"], true);
            assert!(app.pdf_import.placement_pending());
            assert!(app.pdf_import.fallback_reason.is_some());
            let placement = app.pdf_import.placement.as_ref().unwrap();
            assert_eq!(placement.target.document, before.id.0);
            assert_eq!(placement.linked_source.as_deref(), linked.then_some(path));
            assert!(!placement.embedded_confirmed);
            assert_eq!(app.pdf_import.source, original);
            assert_eq!(app.session.active().unwrap().doc, before);
            assert_eq!(app.session.active().unwrap().selected_layers, selected);
            if linked {
                assert!(app.pdf_import.begin_worker(&app.session, 1).is_err());
            }
            let mut h = Harness::builder().with_size(egui::vec2(900.0, 650.0)).build_ui_state(
                |ui, app: &mut crate::PhotocraftApp| {
                    if !fonts_ready(ui.ctx()) {
                        return;
                    }
                    show(app, ui.ctx());
                },
                app,
            );
            h.run_steps(2);
            if linked {
                h.get_by_label("Cancel").click();
                h.run_steps(2);
                assert_eq!(h.state().session.active().unwrap().doc, before);
                assert_eq!(h.state().session.active().unwrap().selected_layers, selected);
                assert_eq!(h.state().pdf_import.source, original);
                assert!(!h.state().pdf_import.dialog_open);
                h.state_mut().services.file_dialog =
                    Some(crate::file_dialog::answering(move |_| Some(crate::FileDialogAnswer::File(path.into(), Ok(unsupported_pdf())))));
                crate::menus::invoke(h.state_mut(), &Context::default(), "file.placeLinked", serde_json::json!({})).unwrap();
                h.run_steps(2);
                assert!(!h.state().pdf_import.placement.as_ref().unwrap().embedded_confirmed);
                h.get_by_label("Place embedded artwork instead (no live link)").click();
                h.run_steps(2);
                assert!(h.state().pdf_import.placement.as_ref().unwrap().embedded_confirmed);
            }
            h.get_by_label("Use image fallback").click();
            h.run_steps(2);
            finish_info(&mut h.state_mut().pdf_import);
            h.run_steps(2);
            h.get_by_label("Place as image").click();
            h.run_steps(2);
            {
                let app = h.state_mut();
                finish_render(&mut app.pdf_import, &mut app.session).unwrap();
            }
            assert_eq!(h.state().session.documents().len(), 1);
            assert!(h.state().session.active().unwrap().path.is_none());
            assert_eq!(h.state().session.active().unwrap().doc.layer_count(), before.layer_count() + 2);
            assert!(h.state_mut().session.undo());
            assert_eq!(h.state().session.active().unwrap().doc, before);
            assert!(h.state_mut().session.redo());
        }
    }
    #[derive(Debug)]
    struct PdfDrop(std::path::PathBuf, std::result::Result<Vec<u8>, String>);
    impl egui::DroppedFile for PdfDrop {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
            self.1.clone()
        }
    }
    #[test]
    fn actual_canvas_drop_waits_for_pdf_choice_and_cancel_preserves_queue_and_destination() {
        let _serial = crate::import_worker::test_serial();
        let mut app = route_app(None);
        app.background_jobs = true;
        let ctx = Context::default();
        app.drop_canvas_rect = Some(Rect::from_min_max(Pos2::new(0., 0.), Pos2::new(500., 500.)));
        let before = app.session.active().unwrap().doc.clone();
        let selected = app.session.active().unwrap().selected_layers.clone();
        let dir = std::env::temp_dir().join(format!(
            "li-pdf-drops-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("leading.pdf");
        let mut leading = vec![b' '; 1024];
        leading.extend_from_slice(&unsupported_pdf());
        std::fs::write(&path, &leading).unwrap();
        let files: Vec<egui::DroppedFileHandle> = vec![
            Arc::new(PdfDrop(path.clone(), Err("absolute drop must use its bounded native path".into()))),
            Arc::new(PdfDrop("bytes.pdf".into(), Ok(unsupported_pdf()))),
        ];
        app.open_dropped(&ctx, files, Some(Pos2::new(10., 10.)));
        app.place_next_dropped(&ctx);
        assert_eq!(app.drop_places.len(), 1);
        assert!(app.pdf_import.placement_pending());
        assert!(app.ui.transform.is_none());
        app.place_next_dropped(&ctx);
        assert_eq!(app.drop_places.len(), 1, "the following drop must wait for the chooser");
        app.pdf_import.cancel_interaction();
        assert_eq!(app.drop_places.len(), 1);
        assert_eq!(app.session.active().unwrap().doc, before);
        assert_eq!(app.session.active().unwrap().selected_layers, selected);
        app.place_next_dropped(&ctx);
        assert!(app.drop_places.is_empty());
        assert!(app.pdf_import.placement_pending());
        assert!(app.ui.transform.is_none());
        app.pdf_import.cancel_interaction();
        assert_eq!(app.session.active().unwrap().doc, before);
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn queue_ready(state: &mut State) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.ready_result.is_none() {
            state.collect_worker();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    #[test]
    fn ready_reply_loses_to_escape_cancel_close_and_focus_loss_in_real_window_frame() {
        let _serial = crate::import_worker::test_serial();
        for (action, with_document) in [("Open as image", false), ("Place as image", true), ("Preview image", true)] {
            for cancel in ["escape", "button", "close", "button-down", "close-down", "focus", "none"] {
                let gate = Arc::new(AtomicBool::new(false));
                let flag = gate.clone();
                let mut app = route_app(None);
                if !with_document {
                    app.session = Session::new();
                }
                app.services.pdf_raster = Some(Arc::new(move |request, cancel| {
                    if matches!(request.operation, crate::pdf_raster::Operation::Render { .. }) {
                        while !flag.load(std::sync::atomic::Ordering::Relaxed) && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                            std::thread::yield_now();
                        }
                    }
                    image_service()(request, cancel)
                }));
                offer(&mut app, "ready.pdf", &unsupported_pdf()).unwrap();
                app.pdf_import.choose_image().unwrap();
                finish_info(&mut app.pdf_import);
                let documents = app.session.documents().len();
                let before = app.session.active().map(|s| s.doc.clone());
                let selected = app.session.active().map(|s| s.selected_layers.clone());
                let revision = app.session.active().map(|s| s.revision);
                let history = app.session.active().map(|s| s.history.past_len());
                let paths: Vec<_> = app.session.documents().iter().map(|s| s.path.clone()).collect();
                let journal = app.session.journal.len();
                let mut h = Harness::builder().with_size(egui::vec2(900.0, 650.0)).build_ui_state(
                    |ui, app: &mut crate::PhotocraftApp| {
                        if !fonts_ready(ui.ctx()) {
                            return;
                        }
                        show(app, ui.ctx());
                    },
                    app,
                );
                h.run_steps(2);
                h.get_by_label(action).click();
                h.run_steps(2);
                assert!(h.state().pdf_import.worker.is_some(), "{action}/{cancel}");
                gate.store(true, std::sync::atomic::Ordering::Relaxed);
                queue_ready(&mut h.state_mut().pdf_import);
                assert!(h.state().pdf_import.ready_result.is_some());
                if matches!(cancel, "button-down" | "close-down") {
                    let point = h.get_by_label(if cancel == "button-down" { "Cancel" } else { "Close window" }).rect().center();
                    h.input_mut().events.push(Event::PointerMoved(point));
                    h.input_mut().events.push(Event::PointerButton {
                        pos: point,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    });
                    h.run_steps(1);
                    assert!(h.state().pdf_import.ready_result.is_some(), "mouse down must not accept a ready result");
                    assert_eq!(h.state().session.documents().len(), documents);
                    assert_eq!(h.state().session.active().map(|s| s.doc.clone()), before);
                    h.event(Event::PointerButton { pos: point, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
                }
                match cancel {
                    "escape" => h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }),
                    "button" | "close" => {
                        let point = h.get_by_label(if cancel == "button" { "Cancel" } else { "Close window" }).rect().center();
                        // kittest's click() emits hover in a separate earlier frame. Here the
                        // completion and native click must arrive in the same frame.
                        h.input_mut().events.extend([
                            Event::PointerMoved(point),
                            Event::PointerButton { pos: point, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
                            Event::PointerButton { pos: point, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE },
                        ]);
                    }
                    "focus" => h.event(Event::WindowFocused(false)),
                    _ => {}
                }
                h.run_steps(2);
                if cancel == "none" {
                    let accepted = h.state().session.active().map(|s| s.doc.clone());
                    h.run_steps(2);
                    assert_eq!(h.state().session.active().map(|s| s.doc.clone()), accepted, "ready result publishes at most once");
                    if action == "Open as image" {
                        assert_eq!(h.state().session.documents().len(), 1);
                        assert!(h.state().session.active().unwrap().path.is_none());
                    } else if action == "Place as image" {
                        assert_eq!(h.state().session.active().unwrap().revision, revision.unwrap() + 1);
                        assert_eq!(h.state().session.active().unwrap().history.past_len(), history.unwrap() + 1);
                        assert!(h.state_mut().session.undo());
                        assert_eq!(h.state().session.active().map(|s| s.doc.clone()), before);
                        assert!(h.state_mut().session.redo());
                    } else {
                        assert_eq!(accepted, before);
                        assert!(h.state().pdf_import.preview.is_some());
                    }
                    continue;
                }
                assert_eq!(h.state().session.documents().len(), documents, "{action}/{cancel}");
                assert_eq!(h.state().session.active().map(|s| s.doc.clone()), before, "{action}/{cancel}");
                assert_eq!(h.state().session.active().map(|s| s.selected_layers.clone()), selected);
                assert_eq!(h.state().session.active().map(|s| s.revision), revision);
                assert_eq!(h.state().session.active().map(|s| s.history.past_len()), history);
                assert_eq!(h.state().session.documents().iter().map(|s| s.path.clone()).collect::<Vec<_>>(), paths);
                assert_eq!(h.state().session.journal.len(), journal);
                assert!(h.state().pdf_import.warnings.is_empty());
                assert!(h.state().pdf_import.preview.is_none());
                assert!(h.state().pdf_import.ready_result.is_none());
            }
        }
    }
    #[test]
    fn ready_route_rejects_replaced_source_and_changed_destination_before_publish_frame() {
        let _serial = crate::import_worker::test_serial();
        for change in ["source", "closed", "switched", "revised", "selection"] {
            let mut app = route_app(Some(("/chosen/captured.pdf".into(), unsupported_pdf())));
            app.session.execute("layer.new.layer", serde_json::json!({"name":"second"})).unwrap();
            crate::menus::invoke(&mut app, &Context::default(), "file.placeEmbedded", serde_json::json!({})).unwrap();
            app.pdf_import.choose_image().unwrap();
            finish_info(&mut app.pdf_import);
            app.pdf_import.begin_worker(&app.session, 1).unwrap();
            queue_ready(&mut app.pdf_import);
            match change {
                "source" => {
                    offer(&mut app, "replacement.pdf", &fixture::artwork()).unwrap();
                }
                "closed" => {
                    app.session.execute("file.close", serde_json::json!({})).unwrap();
                }
                "switched" => {
                    app.session.add_document(
                        Document::new("other", photocraft_doc::Size::new(32, 32), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8),
                        None,
                    );
                }
                "revised" => {
                    app.session.execute("layer.new.layer", serde_json::json!({"name":"newer edit"})).unwrap();
                }
                _ => {
                    let id = app.session.active().unwrap().doc.layers[0].id;
                    assert_ne!(app.session.active().unwrap().selected_layers, vec![id]);
                    app.session.active_mut().unwrap().selected_layers = vec![id];
                }
            }
            let snapshot: Vec<_> =
                app.session.documents().iter().map(|s| (s.doc.clone(), s.revision, s.selected_layers.clone(), s.path.clone(), s.history.past_len())).collect();
            let journal = app.session.journal.len();
            let mut h = Harness::builder().with_size(egui::vec2(900., 650.)).build_ui_state(
                |ui, app: &mut crate::PhotocraftApp| {
                    if !fonts_ready(ui.ctx()) {
                        return;
                    }
                    show(app, ui.ctx());
                },
                app,
            );
            h.run_steps(2);
            assert_eq!(
                h.state()
                    .session
                    .documents()
                    .iter()
                    .map(|s| (s.doc.clone(), s.revision, s.selected_layers.clone(), s.path.clone(), s.history.past_len()))
                    .collect::<Vec<_>>(),
                snapshot,
                "{change}"
            );
            assert_eq!(h.state().session.journal.len(), journal);
            assert!(h.state().pdf_import.ready_result.is_none());
            assert!(h.state().pdf_import.warnings.is_empty());
        }
    }
    #[test]
    fn another_top_modal_owns_escape_while_pdf_ready_reply_waits() {
        let _serial = crate::import_worker::test_serial();
        let mut app = route_app(None);
        offer(&mut app, "ready.pdf", &unsupported_pdf()).unwrap();
        app.pdf_import.choose_image().unwrap();
        finish_info(&mut app.pdf_import);
        let before = app.session.active().unwrap().doc.clone();
        let mut h = Harness::builder().with_size(egui::vec2(900., 650.)).build_ui_state(
            |ui, state: &mut (crate::PhotocraftApp, bool)| {
                if !fonts_ready(ui.ctx()) {
                    return;
                }
                let modal = egui::Modal::new(egui::Id::new("other-owned-dialog")).show(ui.ctx(), |ui| {
                    ui.label("Other dialog");
                });
                if modal.is_top_modal && ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                    state.1 = true;
                }
                show(&mut state.0, ui.ctx());
            },
            (app, false),
        );
        h.run_steps(2);
        {
            let app = &mut h.state_mut().0;
            app.pdf_import.begin_worker(&app.session, 1).unwrap();
            queue_ready(&mut app.pdf_import);
        }
        h.run_steps(1);
        assert!(h.state().0.pdf_import.ready_result.is_some());
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        assert!(h.state().1);
        assert!(h.state().0.pdf_import.ready_result.is_some());
        assert_eq!(h.state().0.session.active().unwrap().doc, before);
    }
    #[test]
    fn newly_opened_pc_dialog_blocks_ready_publication_before_its_first_paint() {
        let _serial = crate::import_worker::test_serial();
        let mut app = route_app(None);
        offer(&mut app, "ready.pdf", &unsupported_pdf()).unwrap();
        app.pdf_import.choose_image().unwrap();
        finish_info(&mut app.pdf_import);
        let before = app.session.active().unwrap().doc.clone();
        let mut h = Harness::builder().with_size(egui::vec2(900., 650.)).build_ui_state(
            |ui, app: &mut crate::PhotocraftApp| {
                if !fonts_ready(ui.ctx()) {
                    return;
                }
                // Same relevant order as the application: PDF before the generic PC dialogs.
                show(app, ui.ctx());
                crate::dialogs::show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(2);
        {
            let app = h.state_mut();
            app.pdf_import.begin_worker(&app.session, 1).unwrap();
            queue_ready(&mut app.pdf_import);
        }
        assert!(h.ctx.memory(|m| m.top_modal_layer().is_none()));
        h.state_mut().ui.open_dialog(crate::state::DialogKind::Error, serde_json::json!({"message":"Hold publication"}).as_object().unwrap().clone());
        h.run_steps(1);
        assert!(h.state().pdf_import.ready_result.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().pdf_import.warnings.is_empty());
    }
    #[test]
    fn focused_open_dpi_escape_uses_prepass_owner_and_neither_cancels_nor_publishes() {
        let _serial = crate::import_worker::test_serial();
        let gate = Arc::new(AtomicBool::new(false));
        let flag = gate.clone();
        let mut app = route_app(None);
        app.services.pdf_raster = Some(Arc::new(move |request, cancel| {
            if matches!(request.operation, crate::pdf_raster::Operation::Render { .. }) {
                while !flag.load(std::sync::atomic::Ordering::Relaxed) && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::yield_now();
                }
            }
            image_service()(request, cancel)
        }));
        offer(&mut app, "ready.pdf", &unsupported_pdf()).unwrap();
        app.pdf_import.choose_image().unwrap();
        finish_info(&mut app.pdf_import);
        let count = app.session.documents().len();
        let before = app.session.active().unwrap().doc.clone();
        let mut h = Harness::builder().with_size(egui::vec2(900., 650.)).build_ui_state(
            |ui, app: &mut crate::PhotocraftApp| {
                if !fonts_ready(ui.ctx()) {
                    return;
                }
                show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(2);
        h.get_by_label("Open as image").click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Open DPI").click();
        h.run_steps(2);
        assert_eq!(crate::shortcut_dispatch::Focus::of(&h.ctx), crate::shortcut_dispatch::Focus::Text);
        assert!(h.state().pdf_import.worker.is_some());
        gate.store(true, std::sync::atomic::Ordering::Relaxed);
        queue_ready(&mut h.state_mut().pdf_import);
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        assert!(h.state().pdf_import.ready_result.is_some());
        assert_eq!(h.state().session.documents().len(), count);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().pdf_import.warnings.is_empty());
    }
    #[test]
    fn first_frame_restore_owns_ready_pdf_until_its_real_escape_close() {
        let _serial = crate::import_worker::test_serial();
        let mut app = route_app(None);
        offer(&mut app, "ready.pdf", &unsupported_pdf()).unwrap();
        app.pdf_import.choose_image().unwrap();
        finish_info(&mut app.pdf_import);
        let before = app.session.active().unwrap().doc.clone();
        let mut h = Harness::builder().with_size(egui::vec2(1100., 800.)).build_ui_state(
            |ui, app: &mut crate::PhotocraftApp| {
                if !fonts_ready(ui.ctx()) {
                    return;
                }
                crate::restoration_ui::handle_keys(app, ui.ctx());
                show(app, ui.ctx());
                crate::restoration_ui::show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(2);
        assert!(!app_modal_open(h.state(), &h.ctx));
        {
            let app = h.state_mut();
            app.pdf_import.begin_worker(&app.session, 1).unwrap();
            queue_ready(&mut app.pdf_import);
        }
        assert!(h.ctx.memory(|m| m.top_modal_layer().is_none()));
        crate::menus::invoke(h.state_mut(), &Context::default(), "ai.restore", serde_json::json!({})).unwrap();
        assert!(crate::restoration_ui::is_open(h.state()));
        assert!(app_modal_open(h.state(), &h.ctx));
        h.run_steps(1);
        assert!(h.state().pdf_import.ready_result.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        assert!(!crate::restoration_ui::is_open(h.state()));
        assert!(h.state().pdf_import.ready_result.is_some());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        h.run_steps(2);
        assert!(h.state().pdf_import.ready_result.is_none());
        assert_eq!(h.state().session.active().unwrap().doc.layer_count(), before.layer_count() + 2);
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../apps/local-image/src/services/pdf_raster.rs"]
mod native_test_provider;
#[cfg(all(test, target_os = "linux"))]
#[path = "pdf_import_native_tests.rs"]
mod native_tests;
