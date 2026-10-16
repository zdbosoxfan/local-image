//! The legacy Draft/Refinement workspace, with explicit sampling controls and open-image input.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input {
    #[default]
    Composite,
    SelectedLayer,
    Draft,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub draft: GenerateState,
    pub refinement: GenerateState,
    pub input: Input,
}

impl Default for Settings {
    fn default() -> Self {
        let mut refinement = GenerateState { mode: Mode::Refine, ..Default::default() };
        set_model(&mut refinement, ModelId::Klein9B);
        Self { draft: GenerateState::default(), refinement, input: Input::Composite }
    }
}

pub(crate) struct Runtime {
    pub open: bool,
    draft: Preview,
    refinement: Preview,
    source: Preview,
    source_key: Option<(u64, u64, u64, Input)>,
    jobs: HashMap<u64, StageJob>,
    refine_controls: bool,
    draft_entry: Option<Entry>,
    refinement_entry: Option<Entry>,
    before_result: Preview,
    compare: bool,
    split: f32,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            open: false,
            draft: Default::default(),
            refinement: Default::default(),
            source: Default::default(),
            source_key: None,
            jobs: Default::default(),
            refine_controls: true,
            draft_entry: None,
            refinement_entry: None,
            before_result: Default::default(),
            compare: false,
            split: 0.5,
        }
    }
}

struct StageJob {
    refine: bool,
    source: Option<RgbaImage>,
}

#[derive(Default)]
struct Preview {
    image: Option<Arc<RgbaImage>>,
    texture: Option<TextureHandle>,
    camera: Camera,
    rect: Option<egui::Rect>,
}

#[derive(Clone, Copy)]
struct Camera {
    scale: Option<f32>,
    center: egui::Vec2,
}
impl Default for Camera {
    fn default() -> Self {
        Self { scale: None, center: vec2(0.5, 0.5) }
    }
}

impl Preview {
    fn set(&mut self, image: RgbaImage) {
        self.image = Some(Arc::new(image));
        self.texture = None;
        self.camera = Camera::default();
    }
    fn texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        if self.texture.is_none()
            && let Some(img) = &self.image
        {
            // Preview textures are bounded; the full pixels stay available for refinement.
            let thumb = image::imageops::thumbnail(img.as_ref(), 2048, 2048);
            let color = egui::ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw());
            self.texture = Some(ctx.load_texture("draft-refine-preview", color, egui::TextureOptions::LINEAR));
        }
        self.texture.as_ref().map(TextureHandle::id)
    }
}

pub(crate) fn open(app: &mut PhotocraftApp) {
    app.draft_refine.open = true;
    app.draft_refine.refine_controls = app.session.active().is_some();
    app.ui.ai.draft_refine.input = Input::Composite;
    if let Some(d) = app.session.active() {
        let s = &mut app.ui.ai.draft_refine.refinement;
        s.width = d.doc.size.width;
        s.height = d.doc.size.height;
        s.aspect = "custom".into();
    }
}

fn update_source(app: &mut PhotocraftApp) {
    let input = app.ui.ai.draft_refine.input;
    if input == Input::Draft {
        let image = app.draft_refine.draft.image.clone();
        let changed = match (&image, &app.draft_refine.source.image) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            app.draft_refine.source = Preview { image, ..Default::default() };
            if let Some(img) = &app.draft_refine.source.image {
                let s = &mut app.ui.ai.draft_refine.refinement;
                (s.width, s.height) = img.dimensions();
                s.aspect = "custom".into();
            }
        }
        app.draft_refine.source_key = None;
        return;
    }
    let key = app.session.active().map(|d| (d.doc.id.0, d.revision, d.active_layer.map_or(0, |l| l.0), input));
    if app.draft_refine.source_key != key {
        let new_input = app.draft_refine.source_key.map(|k| (k.0, k.3)) != key.map(|k| (k.0, k.3));
        if new_input && let Some(d) = app.session.active() {
            let s = &mut app.ui.ai.draft_refine.refinement;
            (s.width, s.height) = (d.doc.size.width, d.doc.size.height);
            s.aspect = "custom".into();
        }
        let img = document_input(app, if input == Input::SelectedLayer { RefineInput::SelectedLayer } else { RefineInput::Composite });
        app.draft_refine.source = Preview::default();
        match img {
            Ok(img) => app.draft_refine.source.set(img),
            Err(e) => {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
        app.draft_refine.source_key = key;
    }
}

pub(crate) fn window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.draft_refine.open {
        return;
    }
    update_source(app);
    if rt(|r| r.library_loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(10))) {
        let entries = crate::generate_ui::library().list();
        rt(|r| {
            r.library = entries.into();
            r.library_loaded = Some(Instant::now());
        });
    }
    let screen = ctx.content_rect().size();
    let size = vec2((screen.x - 64.0).clamp(700.0, 1600.0), (screen.y - 64.0).clamp(560.0, 1000.0));
    if ctx.embed_viewports() {
        let id = egui::Id::new("draft-refine-window");
        let modal = egui::Modal::new(id)
            .frame(window_frame(ctx))
            .backdrop_color(egui::Color32::TRANSPARENT)
            .area(egui::Area::new(id).order(egui::Order::Foreground).movable(true).default_pos(ctx.content_rect().center() - size / 2.0));
        let response = modal.show(ctx, |ui| {
            egui::Resize::default()
                .id_salt("draft-refine-size")
                .default_size(size - vec2(24.0, 24.0))
                .min_size(vec2(676.0, 536.0))
                .max_size((screen - vec2(64.0, 64.0)).max(vec2(676.0, 536.0)))
                .show(ui, |ui| content(app, ui));
        });
        if response.should_close() {
            app.draft_refine.open = false;
        }
    } else {
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("draft-refine-viewport"),
            egui::ViewportBuilder::default()
                .with_title(tl!("Draft / Refinement"))
                .with_inner_size(size)
                .with_min_inner_size([700.0, 600.0])
                .with_resizable(true),
            |ui, class| {
                if class == egui::ViewportClass::EmbeddedWindow {
                    ui.ctx().memory_mut(|m| m.set_modal_layer(ui.layer_id()));
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) || ui.ctx().input(|i| i.viewport().close_requested()) {
                    app.draft_refine.open = false;
                }
                window_frame(ui.ctx()).show(ui, |ui| content(app, ui));
            },
        );
    }
}

fn window_frame(ctx: &egui::Context) -> egui::Frame {
    let t = Tokens::get(ctx);
    egui::Frame::window(&ctx.global_style())
        .fill(t.dock)
        .stroke(Stroke::new(1.0, t.card_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::same(if t.pro { 8 } else { 12 }))
}

fn content(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    inherit_refine_defaults("draft-refinement", &mut app.ui.ai.draft_refine.refinement);
    let t = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing = vec2(crate::theme::ROW_GAP * 2.0, crate::theme::ROW_GAP);
    let height = ui.available_height();
    egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Draft / Refinement")).font(crate::theme::semibold(15.0)).color(t.text));
            widgets::vline(ui, 20.0);
            if widgets::pill_tab(ui, tl!("Draft"), !app.draft_refine.refine_controls).clicked() {
                app.draft_refine.refine_controls = false;
            }
            if widgets::pill_tab(ui, tl!("Refinement"), app.draft_refine.refine_controls).clicked() {
                app.draft_refine.refine_controls = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::button(ui, "x", 24.0, false, tl!("Close")).clicked() {
                    app.draft_refine.open = false;
                }
            });
        });
    });
    ui.add_space(crate::theme::ROW_GAP);
    // Reserve the lower workspace for settings and the fixed action bar. Both previews keep at
    // least 300 pt at the supported desktop sizes; long settings/LoRA lists scroll independently.
    let preview_height = (height - 380.0).max(300.0);
    ui.columns(2, |cols| {
        for (i, ui) in cols.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let refine = i == 1;
                widgets::section_label(ui, if refine { tl!("Refinement") } else { tl!("Draft") });
                let rt = &mut app.draft_refine;
                let before = if refine { rt.before_result.texture(ui.ctx()) } else { None };
                let p = if refine { &mut rt.refinement } else { &mut rt.draft };
                if refine && p.image.is_none() {
                    rt.source.draw(ui, tl!("Refinement preview"), preview_height, None, 0.5);
                } else {
                    p.draw(ui, if refine { tl!("Refinement preview") } else { tl!("Draft preview") }, preview_height, before.filter(|_| rt.compare), rt.split);
                }
                ui.horizontal(|ui| {
                    if refine {
                        widgets::checkbox(ui, &mut app.draft_refine.compare, tl!("Compare with input"));
                        ui.scope(|ui| {
                            ui.set_width((ui.available_width() - 120.0).max(80.0));
                            let r = widgets::slider(ui, &mut app.draft_refine.split, 0.0..=1.0, None);
                            r.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), app.draft_refine.split as f64, tl!("Comparison")));
                        });
                    } else {
                        ui.label(RichText::new(tl!("Wheel to zoom; drag to pan")).font(crate::theme::medium(11.0)).color(t.text_faint));
                    }
                });
            });
        }
    });
    let refine = app.draft_refine.refine_controls;
    let mut use_entry = None;
    let mut open_entry = None;
    let mut history_pick = None;
    let st = crate::ai_ui::status();
    let stage = if refine { &app.ui.ai.draft_refine.refinement } else { &app.ui.ai.draft_refine.draft };
    let needs_warning = readiness(&st, model_of(stage), &stage.variant).is_err();
    let controls_height = (ui.available_height() - if needs_warning { 112.0 } else { 78.0 }).max(60.0);
    ui.push_id(("stage-controls", refine), |ui| {
        egui::ScrollArea::vertical().id_salt("stage-settings").max_height(controls_height).auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(3, |cols| {
                let s = if refine { &mut app.ui.ai.draft_refine.refinement } else { &mut app.ui.ai.draft_refine.draft };
                s.mode = if refine { Mode::Refine } else { Mode::Create };
                s.refine = false;
                egui::Frame::NONE.fill(t.card).inner_margin(8).corner_radius(t.radius_sm).show(&mut cols[0], |ui| {
                    if crate::props_layout::section(ui, if refine { "refinement-model" } else { "draft-model" }, tl!("Model")) {
                        if let Some(k) = model_picker(ui, "model", (&s.model, &s.variant), s.mode, &st, false) {
                            match k.as_str() {
                                "browse:" => crate::model_browser::open(),
                                "browse-loras:" => browse_loras(&s.model),
                                _ => {
                                    if let Some(m) = ModelId::from_key(&k) {
                                        set_model(s, m);
                                    }
                                }
                            }
                        }
                        let info = model_of(s).info();
                        variant_picker(ui, info.id, &mut s.variant);
                        model_variety_note(ui, info.id);
                        ui.add(
                            egui::TextEdit::multiline(&mut s.prompt)
                                .hint_text(if refine { tl!("Describe the image (optional)…") } else { tl!("Describe the draft…") })
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .char_limit(4000),
                        );
                        if info.lora || refine {
                            if refine {
                                refine_loras_ui(ui, &mut s.loras, info.id, &s.variant, &t);
                            } else {
                                loras_ui(ui, &mut s.loras, &info.resolved, &t);
                            }
                        }
                    }
                });
                egui::Frame::NONE.fill(t.card).inner_margin(8).corner_radius(t.radius_sm).show(&mut cols[1], |ui| {
                    if crate::props_layout::section(ui, if refine { "refinement-size" } else { "draft-size" }, tl!("Size")) {
                        size_controls(ui, s);
                        if refine {
                            let choices = [
                                (Input::Composite, tl!("Open image (composite)")),
                                (Input::SelectedLayer, tl!("Selected layer")),
                                (Input::Draft, tl!("Draft result")),
                            ];
                            ui.horizontal(|ui| {
                                label(ui, tl!("Input"));
                                widgets::dropdown(ui, "input", &mut app.ui.ai.draft_refine.input, &choices, ui.available_width());
                            });
                        } else {
                            ui.label(
                                RichText::new(tl!("Create a draft, then choose Draft result as the refinement input."))
                                    .font(crate::theme::medium(11.0))
                                    .color(t.text_faint),
                            );
                        }
                        let entries = super::rt(|r| r.library.clone());
                        let current = if refine { app.draft_refine.refinement_entry.as_ref() } else { app.draft_refine.draft_entry.as_ref() };
                        let mut selected = current.map_or(String::new(), |e| e.id.clone());
                        let choices: Vec<_> = entries.iter().map(|e| (e.id.clone(), e.name.as_str())).collect();
                        ui.horizontal(|ui| {
                            label(ui, tl!("History"));
                            if widgets::dropdown(ui, "history", &mut selected, &choices, ui.available_width().min(180.0))
                                && let Some(entry) = entries.iter().find(|e| e.id == selected)
                            {
                                history_pick = Some(entry.clone());
                            }
                        });
                    }
                });
                egui::Frame::NONE.fill(t.card).inner_margin(8).corner_radius(t.radius_sm).show(&mut cols[2], |ui| {
                    if crate::props_layout::section(ui, if refine { "refinement-sampling" } else { "draft-sampling" }, tl!("Advanced")) {
                        sampling_ui(ui, s, st.limits, refine);
                    }
                });
            });
        });
    });
    widgets::hairline(ui);
    let s = if refine { &app.ui.ai.draft_refine.refinement } else { &app.ui.ai.draft_refine.draft };
    let ready = if refine { refine_readiness(&st, model_of(s), &s.variant) } else { readiness(&st, model_of(s), &s.variant) };
    let busy = app.draft_refine.jobs.values().any(|job| job.refine == refine);
    let has_source = match app.ui.ai.draft_refine.input {
        Input::Draft => app.draft_refine.draft.image.is_some(),
        _ => app.session.active().is_some(),
    };
    let enabled = ready.is_ok() && !busy && if refine { has_source } else { !s.prompt.trim().is_empty() };
    if let Err(why) = &ready {
        readiness_warning(app, ui, why, &st, false);
    }
    let run = ui
        .horizontal(|ui| {
            let button = if busy {
                tl!("Running…")
            } else if refine {
                tl!("Refine image")
            } else {
                tl!("Generate draft")
            };
            let run = ui.add_enabled_ui(enabled, |ui| widgets::primary_button(ui, button, 160.0)).inner.clicked();
            let entry = if refine { app.draft_refine.refinement_entry.clone() } else { app.draft_refine.draft_entry.clone() };
            if let Some(e) = entry {
                if ui.add_enabled_ui(app.session.active().is_some(), |ui| widgets::secondary_button(ui, tl!("Use"), 0.0)).inner.clicked() {
                    use_entry = Some(e.clone());
                }
                if widgets::secondary_button(ui, tl!("Open as new document"), 0.0).clicked() {
                    open_entry = Some(e);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::secondary_button(ui, tl!("Close"), 0.0).clicked() {
                    app.draft_refine.open = false;
                }
                if !app.draft_refine.jobs.is_empty() && widgets::secondary_button(ui, tl!("Stop generation"), 0.0).clicked() {
                    for job in app.draft_refine.jobs.keys() {
                        app.session.cancel_job(photocraft_engine::jobs::JobId(*job));
                    }
                }
            });
            run
        })
        .inner;
    ui.add(
        egui::Label::new(RichText::new(&app.ui.status).font(crate::theme::medium(11.0)).color(if app.ui.status_error { t.warning } else { t.text_dim }))
            .truncate(),
    );
    let ctx = ui.ctx().clone();
    if let Some(entry) = history_pick {
        set_result(app, refine, entry);
    }
    if run {
        start_stage(app, &ctx, refine);
    }
    if let Some(e) = use_entry {
        entry_action(app, &ctx, &e, "place");
    }
    if let Some(e) = open_entry {
        entry_action(app, &ctx, &e, "open");
    }
}

fn stage_request(app: &PhotocraftApp, refine: bool) -> Result<GenerateRequest, String> {
    let s = if refine { &app.ui.ai.draft_refine.refinement } else { &app.ui.ai.draft_refine.draft };
    let input = if refine {
        let image = match app.ui.ai.draft_refine.input {
            Input::Draft => app.draft_refine.draft.image.as_ref().ok_or_else(|| tl!("Generate a draft first.").to_owned())?.as_ref().clone(),
            Input::Composite => document_input(app, RefineInput::Composite)?,
            Input::SelectedLayer => document_input(app, RefineInput::SelectedLayer)?,
        };
        // The chosen dimensions control this operation, including refinement.
        let image = if image.dimensions() == (s.width, s.height) {
            image
        } else {
            let sizes = &model_of(s).info().resolved.sizes;
            let (w, h) = li_ai::imaging::snap_size(s.width.clamp(sizes.min, sizes.max), s.height.clamp(sizes.min, sizes.max), sizes.multiple.max(8));
            image::imageops::resize(&image, w, h, image::imageops::FilterType::Lanczos3)
        };
        Some((app.session.active().map_or(photocraft_doc::DocId(0), |d| d.doc.id), image, None))
    } else {
        None
    };
    local_request(s, s.prompt.trim().into(), input, Vec::new())
        .map(|(mut req, _, _)| {
            req.seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
            req
        })
        .ok_or_else(|| tl!("Choose a valid generation input.").to_owned())
}

fn start_stage(app: &mut PhotocraftApp, ctx: &egui::Context, refine: bool) {
    let req = match stage_request(app, refine) {
        Ok(req) => req,
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            return;
        }
    };
    if let Err(e) = req.validate() {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
        return;
    }
    let source = req.source.clone();
    #[cfg(test)]
    if TEST_REQUEST.with(|mock| {
        let mut mock = mock.borrow_mut();
        if let Some(requests) = mock.as_mut() {
            requests.push(req.clone());
            true
        } else {
            false
        }
    }) {
        return;
    }
    let info = req.model.info();
    let meta = json!({"model": req.model.key(), "variant": req.variant, "mode": if refine { "refine" } else { "create" }, "prompt": req.prompt, "negative_prompt": req.negative,
        "width": req.width, "height": req.height, "steps": req.steps, "guidance": req.guidance, "denoise": req.denoise, "seed": req.seed, "loras": req.loras, "variety": req.variety});
    let name = format!("{}-{}", info.short, req.seed);
    let label = if refine { tl!("Refinement") } else { tl!("Draft") };
    if launch_resolved(app, label, label, name, meta, None, false, move |ctl| {
        let generated = li_ai::service().generate_detailed(&req, ctl)?;
        Ok((generated.image, generated.parameters))
    }) {
        let (job, entry) = rt(|r| match r.results.front() {
            Some(Tile::Pending { job, .. }) => (Some(*job), None),
            Some(Tile::Done(e)) => (None, Some(e.clone())),
            _ => (None, None),
        });
        if let Some(job) = job {
            app.draft_refine.jobs.insert(job, StageJob { refine, source });
        } else if let Some(entry) = entry {
            set_result(app, refine, entry);
            if refine && let Some(source) = source {
                app.draft_refine.before_result.set(source);
            }
        }
    }
    ctx.request_repaint();
}

fn set_result(app: &mut PhotocraftApp, refine: bool, entry: Entry) {
    match crate::generate_ui::library().load(&entry) {
        Ok(img) => {
            if refine {
                app.draft_refine.refinement.set(img);
                app.draft_refine.before_result = Preview::default();
                app.draft_refine.refinement_entry = Some(entry);
            } else {
                app.draft_refine.draft.set(img);
                app.draft_refine.draft_entry = Some(entry);
            }
        }
        Err(e) => {
            app.ui.status = format!("{e:#}");
            app.ui.status_error = true;
        }
    }
}

pub(super) fn on_generated(app: &mut PhotocraftApp, e: &JobEvent) {
    if let Some(job) = app.draft_refine.jobs.remove(&e.id.0)
        && let JobOutcome::Done(v) = &e.outcome
        && let Ok(entry) = serde_json::from_value(v.clone())
    {
        set_result(app, job.refine, entry);
        if job.refine
            && let Some(source) = job.source
        {
            app.draft_refine.before_result.set(source);
        }
    }
}

impl Preview {
    fn draw(&mut self, ui: &mut egui::Ui, label: &str, height: f32, before: Option<egui::TextureId>, split: f32) {
        let t = Tokens::get(ui.ctx());
        let preview_width = ui.available_width();
        egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(6, 4)).corner_radius(t.radius_sm).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if widgets::secondary_button(ui, tl!("Fit"), 0.0).clicked() {
                    self.camera = Camera::default();
                }
                if widgets::secondary_button(ui, tl!("100%"), 0.0).clicked() {
                    self.camera.scale = Some(1.0);
                }
                widgets::vline(ui, 18.0);
                if crate::icons::button(ui, "minus", 24.0, false, tl!("−")).clicked() {
                    self.zoom(0.8, None);
                }
                if crate::icons::button(ui, "plus", 24.0, false, tl!("+")).clicked() {
                    self.zoom(1.25, None);
                }
                if let Some(img) = &self.image {
                    let scale = self.camera.scale.unwrap_or_else(|| (preview_width / img.width() as f32).min(height / img.height() as f32));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{:.0}%", scale * 100.0)).font(crate::theme::mono(11.0)).color(t.text_dim));
                    });
                }
            });
        });
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        self.rect = Some(rect);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, label));
        let painter = ui.painter().with_clip_rect(rect);
        let t = Tokens::get(ui.ctx());
        widgets::surface(ui, rect, t.canvas, false);
        painter.rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        let Some(img) = self.image.clone() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                tl!("Choose or generate an image."),
                crate::theme::medium(12.5),
                Tokens::get(ui.ctx()).text_dim,
            );
            return;
        };
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                self.zoom((-delta.clamp(-480.0, 480.0) * 0.002).exp(), response.hover_pos());
            }
        }
        let dimensions = vec2(img.width() as f32, img.height() as f32);
        let fitted = (rect.width() / dimensions.x).min(rect.height() / dimensions.y);
        let scale = self.camera.scale.unwrap_or(fitted);
        if response.dragged() {
            self.camera.center = (self.camera.center - response.drag_delta() / (dimensions * scale)).clamp(egui::Vec2::ZERO, vec2(1.0, 1.0));
        }
        if response.has_focus() {
            let (fit, actual, zoom_in, zoom_out, pan) = ui.input_mut(|i| {
                let fit = i.consume_key(egui::Modifiers::NONE, egui::Key::F);
                let actual = i.consume_key(egui::Modifiers::NONE, egui::Key::Num1);
                let zoom_in = i.consume_key(egui::Modifiers::NONE, egui::Key::Plus) || i.consume_key(egui::Modifiers::NONE, egui::Key::Equals);
                let zoom_out = i.consume_key(egui::Modifiers::NONE, egui::Key::Minus);
                let x = if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                    40.0
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                    -40.0
                } else {
                    0.0
                };
                let y = if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    40.0
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    -40.0
                } else {
                    0.0
                };
                (fit, actual, zoom_in, zoom_out, vec2(x, y))
            });
            if fit {
                self.camera = Camera::default();
            }
            if actual {
                self.camera.scale = Some(1.0);
            }
            if zoom_in {
                self.zoom(1.25, None);
            }
            if zoom_out {
                self.zoom(0.8, None);
            }
            self.camera.center = (self.camera.center - pan / (dimensions * scale)).clamp(egui::Vec2::ZERO, vec2(1.0, 1.0));
        }
        let size = dimensions * self.camera.scale.unwrap_or(fitted);
        let image_rect = egui::Rect::from_min_size(rect.center() - size * self.camera.center, size);
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        if let Some(texture) = self.texture(ui.ctx()) {
            painter.image(texture, image_rect, uv, Color32::WHITE);
        }
        if let Some(before) = before {
            let x = rect.left() + rect.width() * split;
            let clip = egui::Rect::from_min_max(rect.min, egui::pos2(x, rect.bottom()));
            painter.with_clip_rect(clip).image(before, image_rect, uv, Color32::WHITE);
            painter.vline(x, rect.y_range(), Stroke::new(2.0, t.accent));
            let handle = egui::Rect::from_center_size(egui::pos2(x, rect.center().y), vec2(10.0, 28.0));
            painter.rect_filled(handle, t.radius_sm, t.accent);
            painter.rect_stroke(handle, t.radius_sm, Stroke::new(1.0, t.accent_border), StrokeKind::Inside);
        }
    }
    fn zoom(&mut self, factor: f32, anchor: Option<egui::Pos2>) {
        let (Some(rect), Some(img)) = (self.rect, &self.image) else { return };
        let dimensions = vec2(img.width() as f32, img.height() as f32);
        let scale = self.camera.scale.unwrap_or((rect.width() / dimensions.x).min(rect.height() / dimensions.y));
        let next = (scale * factor).clamp(0.02, 8.0);
        let offset = anchor.unwrap_or(rect.center()) - rect.center();
        self.camera.center = (self.camera.center + offset / dimensions * (1.0 / scale - 1.0 / next)).clamp(egui::Vec2::ZERO, vec2(1.0, 1.0));
        self.camera.scale = Some(next);
    }
}

#[cfg(test)]
thread_local! { static TEST_REQUEST: std::cell::RefCell<Option<Vec<GenerateRequest>>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
mod tests;
