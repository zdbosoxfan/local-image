//! Starting generations: request building, local and cloud runs (with the cloud confirmation),
//! launching jobs, and placing or opening their results.
use super::*;

/// Snapshot the chosen open-image input; references never replace it in Refine mode.
pub(super) fn document_input(app: &PhotocraftApp, input: RefineInput) -> Result<RgbaImage, String> {
    let d = app.session.active().ok_or_else(|| tl!("Open an image first.").to_owned())?;
    let image = match input {
        RefineInput::Composite => photocraft_engine::ai_cmds::flatten_rgba(&d.doc),
        RefineInput::SelectedLayer => {
            let l = d.active_layer.and_then(|id| d.doc.layer(id)).ok_or_else(|| tl!("Select a layer first.").to_owned())?;
            photocraft_engine::ai_cmds::selected_layer_rgba(&d.doc, l, d.doc.bounds())
        }
    };
    image.map_err(|e| e.to_string())
}

pub(super) type OpenImageInput = (photocraft_doc::DocId, RgbaImage, Option<image::GrayImage>);

/// Only operations that consume the open image should render or colour-convert it.
pub(super) fn request_document(app: &PhotocraftApp, s: &GenerateState) -> Result<Option<OpenImageInput>, String> {
    if s.mode == Mode::Create {
        return Ok(None);
    }
    let Some(d) = app.session.active() else { return Ok(None) };
    let input = if s.mode == Mode::Refine { s.refine_input } else { RefineInput::Composite };
    Ok(Some((d.doc.id, document_input(app, input)?, photocraft_engine::ai_cmds::selection_gray(&d.doc))))
}

pub(super) fn local_request(
    s: &GenerateState,
    prompt: String,
    doc: Option<OpenImageInput>,
    refs: Vec<RgbaImage>,
) -> Option<(GenerateRequest, Option<photocraft_doc::DocId>, bool)> {
    let m = model_of(s);
    let info = m.info();
    let mut req = GenerateRequest::new(m, prompt);
    req.variant = s.variant.clone();
    req.width = s.width;
    req.height = s.height;
    req.transparent = s.transparent && info.transparent;
    // Steps/guidance fitted to what the connected ComfyUI accepts (never built-in ranges).
    let mut fitted = s.clone();
    fit_to_backend(&mut fitted, crate::ai_ui::status().limits);
    req.steps = fitted.steps;
    req.guidance = fitted.guidance;
    req.variety = if m == ModelId::ZImageTurbo && s.mode == Mode::Create && s.variety.is_finite() { s.variety.clamp(0.0, 100.0) } else { 0.0 };
    req.negative = s.negative.clone();
    req.denoise = fitted.denoise;
    if s.mode == Mode::Refine {
        req.refine_defaults = s.refine_defaults;
    }
    req.scale = s.upscale;
    req.enhance = s.enhance_prompt.get(&s.model).copied().unwrap_or(false);
    req.loras = s.loras.iter().map(|l| li_ai::workflows::LoraUse { name: l.name.clone(), strength: l.strength }).collect();
    if !s.sampler.is_empty() {
        req.sampler = Some((s.sampler.clone(), if s.scheduler.is_empty() { info.resolved.pipeline.scheduler.clone() } else { s.scheduler.clone() }));
    }
    // Refine/Upscale cannot accept the Create panel's reference slots. They were kept in
    // runtime while hidden in these modes, causing validation to reject open-image jobs.
    req.references = if matches!(s.mode, Mode::Refine | Mode::Upscale) { Vec::new() } else { refs };
    let (mut target_doc, mut open) = (None, false);
    match s.mode {
        Mode::Create => {
            if s.refine
                && let Some(rm) = ModelId::from_key(&s.refine_model)
            {
                let mut refine = GenerateState {
                    mode: Mode::Refine,
                    model: s.refine_model.clone(),
                    steps: s.refine_steps.unwrap_or(rm.info().steps.default as u32),
                    guidance: s.refine_guidance.unwrap_or(rm.info().guidance.default),
                    denoise: s.refine_strength,
                    ..Default::default()
                };
                fit_to_backend(&mut refine, crate::ai_ui::status().limits);
                req.refine = Some(li_ai::RefineStep {
                    model: rm,
                    variant: s.refine_variant.clone(),
                    strength: refine.denoise,
                    default_strength: s.refine_strength_default,
                    steps: s.refine_steps.map(|_| refine.steps),
                    guidance: s.refine_guidance.map(|_| refine.guidance),
                    loras: s.refine_loras.iter().map(|l| li_ai::workflows::LoraUse { name: l.name.clone(), strength: l.strength }).collect(),
                    scale: s.refine_scale,
                });
            }
        }
        mode => {
            let (id, img, sel) = doc?;
            req.mode = match mode {
                Mode::Edit => GenerateMode::Edit,
                Mode::Fill => GenerateMode::Inpaint,
                Mode::Refine => GenerateMode::Refine,
                _ => GenerateMode::UpscaleRefine,
            };
            (req.width, req.height) = (img.width(), img.height());
            req.source = Some(img);
            if mode == Mode::Fill {
                req.mask = sel;
            }
            if mode == Mode::Upscale {
                open = true;
            } else {
                target_doc = Some(id);
            }
        }
    }
    Some((req, target_doc, open))
}

/// Starts the generation jobs for the current settings.
pub(super) fn start(app: &mut PhotocraftApp, ctx: &egui::Context) {
    #[cfg(test)]
    if rt(|runtime| {
        if runtime.capture_panel_start {
            runtime.panel_start_clicks += 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let before: Vec<_> = app.session.jobs().into_iter().map(|job| job.id).collect();
    start_inner(app, ctx);
    let started: Vec<_> = app.session.jobs().into_iter().filter(|job| job.command.starts_with("ai.") && !before.contains(&job.id)).map(|job| job.id).collect();
    rt(|runtime| runtime.panel_jobs.extend(started));
}

pub(super) fn start_inner(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let s = app.ui.ai.generate.clone();
    restore_upscale_choice(&s);
    if s.mode == Mode::Upscale && !upscale_any() {
        if app.session.active().is_none() {
            return;
        }
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        if let Err(e) = app.run("ai.enhance", json!({"scale": s.upscale, "seed": seed, "method": s.enhance_method, "backend": s.enhance_backend})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ctx.request_repaint();
        return;
    }
    if s.mode == Mode::Refine && s.refine_input == RefineInput::SelectedLayer && app.session.active().is_none_or(|d| d.active_layer.is_none()) {
        app.ui.status = tl!("Select a layer first.").into();
        return;
    }
    let preset = presets().into_iter().find(|p| p.name == s.preset);
    let prompt = preset.as_ref().map_or_else(|| s.prompt.trim().to_owned(), |p| p.apply(&s.prompt));
    let doc = match request_document(app, &s) {
        Ok(doc) => doc,
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            return;
        }
    };
    let refs: Vec<RgbaImage> = rt(|r| r.references.iter().map(|x| (*x.image).clone()).collect());
    // Custom workflows.
    if let Some(w) = custom_of(&s) {
        let mut images = Vec::new();
        if let Some((_, img, _)) = &doc
            && s.mode != Mode::Create
        {
            images.push(img.clone());
        }
        images.extend(refs);
        let mask =
            doc.as_ref().and_then(|(_, _, m)| m.clone()).or_else(|| app.session.active().and_then(|d| photocraft_engine::ai_cmds::selection_gray(&d.doc)));
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        let inputs = li_ai::custom::Inputs {
            prompt: prompt.clone(),
            negative: s.negative.clone(),
            seed,
            steps: Some(s.steps),
            cfg: Some(s.guidance),
            denoise: Some(s.denoise),
            width: Some(s.width),
            height: Some(s.height),
        };
        let name = format!("{}-{seed}", w.name.replace(' ', ""));
        let meta = json!({ "model": format!("custom:{}", w.name), "prompt": prompt, "negative_prompt": s.negative, "seed": seed, "mode": "custom" });
        let label = format!("Running {}", w.name);
        let shown = li_i18n::fmt(tl!("Running {name}"), &[("name", &w.name)]);
        launch(app, &label, &shown, name, meta, None, false, move |ctl| li_ai::service().run_custom(&w, &inputs, &images, mask.as_ref(), ctl));
        ctx.request_repaint();
        return;
    }
    if let Some(c) = cloud_of(&s) {
        start_cloud(app, &s, c, prompt, doc, refs);
        ctx.request_repaint();
        return;
    }
    let m = model_of(&s);
    let info = m.info();
    // Official restoration models share ai.enhance.
    if runs_as_enhance(info, s.mode) {
        if doc.is_none() {
            return;
        }
        let seed = s.seed.unwrap_or_else(li_ai::ops::new_seed);
        if let Err(e) = app.run("ai.enhance", json!({ "scale": s.upscale, "seed": seed, "model": m.key(), "method": if info.id == ModelId::SeedVr2 { li_ai::enhance::Method::Generative } else if info.family == "bsrgan" { li_ai::enhance::Method::Rescue } else { li_ai::enhance::Method::Faithful }, "backend": s.enhance_backend })) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ctx.request_repaint();
        return;
    }
    let Some((req, target_doc, open)) = local_request(&s, prompt, doc, refs) else { return };
    if let Err(e) = req.validate() {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
        return;
    }
    let fill = s.mode == Mode::Fill;
    let count = if s.mode == Mode::Create { s.count.clamp(1, 4) } else { 1 };
    for i in 0..count {
        let mut r = req.clone();
        r.seed = s.seed.map(|v| v.wrapping_add(i as u64)).unwrap_or_else(li_ai::ops::new_seed);
        let label = format!("{} with {}", s.mode.label(), info.short);
        let shown = li_i18n::fmt(tl!("{mode} with {model}"), &[("mode", s.mode.shown()), ("model", &info.short)]);
        let name = format!("{}-{}", info.short.replace(' ', ""), r.seed);
        let meta = json!({
            "model": m.key(), "family": info.family, "variant": r.variant, "prompt": r.prompt, "negative_prompt": r.negative, "width": r.width, "height": r.height,
            "seed": r.seed, "transparent": r.transparent, "steps": r.steps, "guidance": r.guidance, "denoise": r.denoise, "variety": r.variety,
            "loras": s.loras.iter().map(|l| json!({"name": l.name, "strength": l.strength})).collect::<Vec<_>>(),
            "refine": r.refine,
            "reference_count": r.references.len(), "mode": format!("{:?}", s.mode).to_lowercase(),
        });
        let mask = r.mask.clone();
        let ok = launch_resolved(app, &label, &shown, name, meta, target_doc.map(|d| d.0), open, move |ctl| {
            let generated = li_ai::service().generate_detailed(&r, ctl)?;
            let out = generated.image;
            // Fill lands as a layer holding only the regenerated area.
            let image = match (fill, mask) {
                (true, Some(m)) => {
                    let grown = li_ai::imaging::dilate(&m, 6);
                    let soft = image::imageops::blur(&grown, 3.0);
                    RgbaImage::from_fn(out.width(), out.height(), |x, y| {
                        let mut p = *out.get_pixel(x, y);
                        p[3] = soft.get_pixel(x, y)[0];
                        p
                    })
                }
                _ => out,
            };
            Ok((image, generated.parameters))
        });
        if !ok {
            break;
        }
    }
    ctx.request_repaint();
}

/// Home screen › Generate: the prompt typed there, in Create mode, with the Generate panel open
/// so the results show.
pub fn generate_from_home(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let _ = crate::menus::invoke(app, ctx, "li.panel.generate", json!({}));
    app.ui.ai.generate.mode = Mode::Create;
    start(app, ctx);
}

/// A validated cloud generation, ready to start (or waiting for the user to confirm its cost).
pub(super) struct CloudRun {
    pub(super) c: &'static li_ai::cloud::CloudModel,
    pub(super) mode: Mode,
    pub(super) seed: Option<u64>,
    pub(super) base: li_ai::cloud::CloudRequest,
    pub(super) target: Option<u64>,
    pub(super) images: u32,
    pub(super) dont_ask: bool,
}

/// The message to show when `images` more would pass the session cap (0 = no cap).
pub(super) fn cloud_cap_message(cap: u32, used: u32, images: u32) -> Option<String> {
    (cap > 0 && used.saturating_add(images) > cap).then(|| {
        li_i18n::fmt(
            tl!("Cloud limit reached: {used} of {cap} images used this session, and this run needs {images}. Raise or clear the limit in Local AI › Cloud."),
            &[("used", &used.to_string()), ("cap", &cap.to_string()), ("images", &images.to_string())],
        )
    })
}

/// A cloud generation: one job per image, each with its own seed. Edits and fills land on the
/// open document as a layer (a fill holds only the regenerated area). Paid runs ask first unless
/// the user turned that off, and never pass the session cap.
pub(super) fn start_cloud(
    app: &mut PhotocraftApp,
    s: &GenerateState,
    c: &'static li_ai::cloud::CloudModel,
    prompt: String,
    doc: Option<OpenImageInput>,
    refs: Vec<RgbaImage>,
) {
    use li_ai::cloud::{CloudMode, CloudRequest};
    let mode = match s.mode {
        Mode::Create => CloudMode::Create,
        Mode::Edit => CloudMode::Edit,
        Mode::Fill => CloudMode::Fill,
        Mode::Refine | Mode::Upscale => {
            app.ui.status = li_i18n::fmt(tl!("{model} creates, edits and fills; choose a local model to refine or upscale."), &[("model", c.label)]);
            app.ui.status_error = true;
            return;
        }
    };
    let (target, source, mask) = match (&doc, mode) {
        (Some((id, img, sel)), CloudMode::Edit | CloudMode::Fill) => (Some(id.0), Some(img.clone()), if mode == CloudMode::Fill { sel.clone() } else { None }),
        _ => (None, None, None),
    };
    let base = CloudRequest {
        model: c,
        mode,
        prompt,
        negative: s.negative.clone(),
        width: s.width,
        height: s.height,
        seed: 0,
        count: 1,
        source,
        mask,
        refs,
        strength: s.denoise,
    };
    if let Err(e) = base.validate() {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
        return;
    }
    let images = if mode == CloudMode::Create { s.count.clamp(1, 4) } else { 1 };
    let run = CloudRun { c, mode: s.mode, seed: s.seed, base, target, images, dont_ask: false };
    let settings = li_ai::settings::AiSettings::load();
    if let Some(message) = cloud_cap_message(settings.cloud_session_cap_images, rt(|r| r.cloud_images), images) {
        app.ui.status = message;
        app.ui.status_error = true;
        return;
    }
    if settings.cloud_confirm_each_run && !rt(|r| r.cloud_skip_confirm) {
        rt(|r| r.cloud_pending = Some(run));
        return;
    }
    run_cloud(app, run);
}

pub(super) fn run_cloud(app: &mut PhotocraftApp, run: CloudRun) {
    let CloudRun { c, mode, seed, base, target, images, .. } = run;
    for i in 0..images {
        let mut r = base.clone();
        r.seed = seed.map(|v| v.wrapping_add(i as u64)).unwrap_or_else(li_ai::ops::new_seed);
        let label = format!("{} with {}", mode.label(), c.label);
        let shown = li_i18n::fmt(tl!("{mode} with {model}"), &[("mode", mode.shown()), ("model", c.label)]);
        let name = format!("{}-{}", c.model.replace(['.', ' '], ""), r.seed);
        let meta = json!({
            "model": format!("cloud:{}", c.key), "provider": c.provider.label(), "prompt": r.prompt, "negative_prompt": r.negative,
            "width": r.width, "height": r.height, "seed": r.seed, "mode": format!("{mode:?}").to_lowercase(), "reference_count": r.refs.len(),
        });
        let fill_mask = r.mask.clone();
        let ok = launch(app, &label, &shown, name, meta, target, false, move |ctl| {
            let settings = li_ai::settings::AiSettings::load();
            let out = li_ai::cloud::generate(&r, &settings, ctl)?.into_iter().next().ok_or_else(|| anyhow::anyhow!("no image came back"))?;
            Ok(match fill_mask {
                Some(m) => {
                    let soft = image::imageops::blur(&li_ai::imaging::dilate(&m, 6), 3.0);
                    RgbaImage::from_fn(out.width(), out.height(), |x, y| {
                        let mut p = *out.get_pixel(x, y);
                        p[3] = soft.get_pixel(x, y)[0];
                        p
                    })
                }
                None => out,
            })
        });
        if !ok {
            break;
        }
        rt(|r| r.cloud_images = r.cloud_images.saturating_add(1));
    }
}

/// The cost confirmation for a paid cloud run, drawn every frame while one is waiting.
pub fn cloud_confirm_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some((provider, model, images, mut dont_ask)) = rt(|r| r.cloud_pending.as_ref().map(|p| (p.c.provider, p.c.label, p.images, p.dont_ask))) else {
        return;
    };
    let cap = li_ai::settings::AiSettings::load().cloud_session_cap_images;
    let used = rt(|r| r.cloud_images);
    let t = Tokens::get(ctx);
    let mut answer = None;
    use widgets::{ButtonRole, DialogButton};
    let modal = egui::Modal::new(egui::Id::new("cloud-run-confirm")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.heading(li_i18n::fmt(tl!("Generate with {provider}?"), &[("provider", provider.label())]));
        ui.add_space(4.0);
        ui.label(li_i18n::fmt(tl!("Provider: {provider}"), &[("provider", provider.label())]));
        ui.label(li_i18n::fmt(tl!("Model: {model}"), &[("model", model)]));
        ui.label(li_i18n::fmt(tl!("Images: {count}"), &[("count", &images.to_string())]));
        ui.add_space(4.0);
        ui.label(li_i18n::fmt(
            tl!("Your prompt and any selected or reference pixels are uploaded to {provider} and billed to your own API key."),
            &[("provider", provider.label())],
        ));
        if cap > 0 {
            ui.label(
                RichText::new(li_i18n::fmt(tl!("{used} of {cap} cloud images used this session."), &[("used", &used.to_string()), ("cap", &cap.to_string())]))
                    .color(t.text_faint)
                    .size(11.0),
            );
        }
        ui.add_space(4.0);
        widgets::checkbox(ui, &mut dont_ask, tl!("Don't ask again this session"));
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            answer = widgets::dialog_buttons(
                ui,
                &[DialogButton::new(ButtonRole::Default, tl!("Generate"), 96.0), DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0)],
            );
        });
    });
    rt(|r| {
        if let Some(p) = r.cloud_pending.as_mut() {
            p.dont_ask = dont_ask;
        }
    });
    let answer = answer.or_else(|| modal.should_close().then_some(ButtonRole::Cancel));
    match answer {
        Some(ButtonRole::Default) => {
            if let Some(run) = rt(|r| {
                r.cloud_skip_confirm |= dont_ask;
                r.cloud_pending.take()
            }) {
                run_cloud(app, run);
            }
            ctx.request_repaint();
        }
        Some(_) => rt(|r| r.cloud_pending = None),
        None => {}
    }
}

/// Starts one job that produces an image, saves it to the library and shows its tile (`shown`:
/// the tile's label in the UI language; the job keeps the English `label`).
#[allow(clippy::too_many_arguments)]
pub(super) fn launch(
    app: &mut PhotocraftApp,
    label: &str,
    shown: &str,
    name: String,
    meta: Value,
    edit: Option<u64>,
    open: bool,
    work: impl FnOnce(&li_ai::JobControl) -> anyhow::Result<RgbaImage> + Send + 'static,
) -> bool {
    launch_resolved(app, label, shown, name, meta, edit, open, move |ctl| Ok((work(ctl)?, Value::Null)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn launch_resolved(
    app: &mut PhotocraftApp,
    label: &str,
    shown: &str,
    name: String,
    mut meta: Value,
    edit: Option<u64>,
    open: bool,
    work: impl FnOnce(&li_ai::JobControl) -> anyhow::Result<(RgbaImage, Value)> + Send + 'static,
) -> bool {
    let params = json!({ "edit": edit, "name": name.clone(), "open": open });
    let library = library();
    let started = app.session.start_job(
        GENERATE_JOB,
        params,
        label,
        false,
        move |ctx| {
            let (img, resolved) = photocraft_engine::ai_cmds::bridged(ctx, work)?;
            if let (Some(meta), Some(resolved)) = (meta.as_object_mut(), resolved.as_object()) {
                meta.extend(resolved.clone());
            }
            ctx.check()?;
            ctx.report(0.98, "Preparing library image");
            let prepared = library
                .prepare(&img, &name, Some(meta), None)
                .map_err(|e| photocraft_engine::EngineError::Other(format!("Could not prepare the library image: {e:#}")))?;
            ctx.check()?;
            ctx.report(0.99, "Ready to save to the library");
            Ok(prepared)
        },
        |_, prepared| {
            // Session checks cancellation before applying on the UI thread. A queued
            // result cancelled before this point is dropped with its staging files.
            let entry = prepared.publish().map_err(|e| photocraft_engine::EngineError::Other(format!("Could not save to the library: {e:#}")))?;
            Ok(serde_json::to_value(entry).unwrap_or(Value::Null))
        },
    );
    match started {
        Ok(photocraft_engine::jobs::Started::Job(id)) => {
            rt(|rt| rt.results.push_front(Tile::Pending { job: id.0, label: shown.to_owned(), edit, open }));
            true
        }
        Ok(photocraft_engine::jobs::Started::Done(v)) => {
            if let Ok(e) = serde_json::from_value::<Entry>(v) {
                rt(|rt| {
                    rt.selected = Some(e.id.clone());
                    rt.results.push_front(Tile::Done(e));
                });
            }
            true
        }
        Err(e) => {
            app.ui.status = e.to_string();
            app.ui.status_error = true;
            false
        }
    }
}

/// A generation job ended.
pub fn on_generated(app: &mut PhotocraftApp, e: &JobEvent) {
    draft_refine::on_generated(app, e);
    let tile = match &e.outcome {
        JobOutcome::Done(v) => match serde_json::from_value::<Entry>(v.clone()) {
            Ok(entry) => {
                app.ui.status = li_i18n::fmt(tl!("Generated {name} · seed {seed}"), &[("name", &entry.name), ("seed", &entry.seed().unwrap_or(0).to_string())]);
                app.ui.status_error = false;
                // An edit of an open document lands on it as a new layer.
                let (edit_doc, open) = rt(|r| {
                    r.results
                        .iter()
                        .find_map(|t| match t {
                            Tile::Pending { job, edit, open, .. } if *job == e.id.0 => Some((*edit, *open)),
                            _ => None,
                        })
                        .unwrap_or((None, false))
                });
                if let Some(doc) = edit_doc {
                    place_into(app, doc, &entry);
                } else if open {
                    let path = library().image_path(&entry.id);
                    if let Ok(bytes) = photocraft_engine::batch_io::read_file(&path)
                        && let Err(err) = app.open_bytes(&format!("{}.png", entry.name), &bytes)
                    {
                        app.ui.status = err;
                        app.ui.status_error = true;
                    }
                }
                Tile::Done(entry)
            }
            Err(err) => Tile::Failed(err.to_string()),
        },
        JobOutcome::Failed(err) => {
            app.ui.status = li_i18n::fmt(tl!("Generation failed: {error}"), &[("error", err)]);
            app.ui.status_error = true;
            Tile::Failed(err.clone())
        }
        JobOutcome::Cancelled => Tile::Failed(tl!("Cancelled").into()),
    };
    rt(|r| {
        // Selecting a completed tile only exposes its choices; it does not open or insert it.
        if let Tile::Done(entry) = &tile {
            r.selected = Some(entry.id.clone());
        }
        if let Some(i) = r.results.iter().position(|t| matches!(t, Tile::Pending { job, .. } if *job == e.id.0)) {
            r.results[i] = tile;
        } else {
            r.results.push_front(tile);
        }
        r.results.truncate(48);
        r.library_loaded = None;
    });
}

pub(super) fn place_into(app: &mut PhotocraftApp, doc_id: u64, entry: &Entry) {
    let Some(index) = app.session.documents().iter().position(|d| d.doc.id.0 == doc_id) else { return };
    let previous = app.session.active().map(|d| d.doc.id);
    app.session.set_active(index);
    let path = library().image_path(&entry.id);
    let name = format!("Edit: {}", entry.prompt().chars().take(40).collect::<String>());
    // Apply through the engine without syncing UI gestures against the temporary target.
    // Sync once after restoring the viewed document, so an in-progress lasso survives.
    if let Err(e) =
        app.session.execute("ai.placeLayer", json!({ "path": path.display().to_string(), "name": name, "fit": "none", "generation": entry.generation }))
    {
        app.ui.status = e.to_string();
        app.ui.status_error = true;
    }
    if let Some(previous) = previous.filter(|id| id.0 != doc_id)
        && let Some(index) = app.session.documents().iter().position(|d| d.doc.id == previous)
    {
        app.session.set_active(index);
    }
    app.sync_views();
}
