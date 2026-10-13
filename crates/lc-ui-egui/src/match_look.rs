//! Analyze once, preview and apply to many targets. Work is dispatched through the existing
//! Tasks/jobs path; only the frame thread installs measurements or commits the batch.
use crate::{LightcraftApp, i18n::tr, state::Dialog, theme::Tokens, widgets};
use lightcraft_engine::cmd::match_look::{self as engine, Control};
use lightcraft_pipeline::match_look::{self as pipeline, Options};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};

#[cfg(not(test))]
#[rustfmt::skip]
#[allow(dead_code)]
#[path = "../../../apps/local-image/src/app_dirs.rs"]
mod app_dirs;

pub struct Preview {
    options: Options,
    photo: lightcraft_catalog::PhotoId,
    identity: engine::Identity,
    generation: u64,
    looks: Vec<pipeline::Look>,
    texture: egui::TextureHandle,
    pixels: egui::ColorImage,
    before_texture: egui::TextureHandle,
    before_pixels: egui::ColorImage,
    changes: Vec<(&'static str, String)>,
    similarity: f64,
    fit: f64,
    reach: f64,
    wb_ratio: f64,
    preview_only: bool,
    weights: Vec<f64>,
}
pub struct Runtime {
    pub control: Option<Arc<Control>>,
    pub label: String,
    generation: u64,
    pub preview: Option<Preview>,
    pub error: String,
    pub last_result: Option<Value>,
    pub wipe: f32,
    pub compare_reference: bool,
    pub reference_index: usize,
    auto_preview_at: Option<web_time::Instant>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            control: None,
            label: String::new(),
            generation: 0,
            preview: None,
            error: String::new(),
            last_result: None,
            wipe: 0.5,
            compare_reference: false,
            reference_index: 0,
            auto_preview_at: None,
        }
    }
}
impl Runtime {
    pub fn status(&self) -> Option<Value> {
        self.control.as_ref().map(|c|json!({"label":self.label,"completed":c.completed.load(Ordering::Relaxed),"total":c.total,"cancelled":c.cancel.load(Ordering::Relaxed)}))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(c) = &self.control {
            c.cancel.store(true, Ordering::Relaxed);
        }
    }
}
pub fn open_global_store(session: &mut lightcraft_engine::Session) {
    #[cfg(not(test))]
    if let Some(dir) = app_dirs::config_dir() {
        let result = lightcraft_catalog::FsStore::open(dir.join("Presets"))
            .map_err(|e| e.to_string())
            .and_then(|store| session.saved_looks.open(Box::new(store)));
        if let Err(e) = result {
            session.saved_looks.error = Some(e);
        }
    }
    engine::reconcile_selection(session);
}
fn start_control(app: &mut LightcraftApp, label: &str, total: usize) -> Result<Arc<Control>, String> {
    if app.match_look.control.is_some() {
        return Err(tr("Match Look is already running").into());
    }
    let c = Arc::new(Control { total, ..Default::default() });
    app.match_look.control = Some(c.clone());
    app.match_look.label = label.into();
    app.match_look.generation = app.session.library_generation;
    app.match_look.error.clear();
    Ok(c)
}
pub fn analyze(app: &mut LightcraftApp, update: Option<String>) -> Result<Value, String> {
    let work = engine::prepare_analysis(&mut app.session).map_err(|e| e.to_string())?;
    let c = start_control(app, "Analyze Match Look", work.len())?;
    let worker = c.clone();
    app.match_look.preview = None;
    let result = crate::tasks::spawn(
        app,
        "Analyze Match Look",
        move || lightcraft_engine::guard::catch("Analyze Match Look", || work.run(&worker)),
        move |app, ctx, result| {
            if app.match_look.control.as_ref().is_none_or(|current| !Arc::ptr_eq(current, &c)) {
                return;
            }
            app.match_look.control = None;
            if c.cancel.load(Ordering::Relaxed) {
                app.toast(ctx, "Match Look analysis cancelled");
                return;
            }
            let result =
                result.and_then(|r| r).and_then(|r| engine::apply_analysis(&mut app.session, r, update.as_deref()).map_err(|e| e.to_string()));
            match result {
                Ok(r) => {
                    let discarded = r["discarded"] == true;
                    let embedded = r["previewOnly"].as_u64().unwrap_or(0);
                    app.match_look.last_result = Some(r);
                    if embedded > 0 {
                        app.toast(ctx, crate::i18n::tr_format!("{count} references on embedded previews", count = embedded));
                    }
                    app.toast(ctx, if discarded { "References changed — analysis was not stored" } else { "Match Look analysis complete" });
                }
                Err(e) => {
                    app.match_look.error = e.clone();
                    app.toast_error(ctx, e);
                }
            }
        },
    );
    if let Err(e) = result {
        app.match_look.control = None;
        return Err(e);
    }
    Ok(json!({"running":true}))
}
pub fn start(app: &mut LightcraftApp, params: Value, preview: bool) -> Result<Value, String> {
    if !preview {
        cancel_preview(app);
    }
    let mut params = params;
    if preview {
        params["ids"] = json!([app.session.active().ok_or("Select a target photo")?.0]);
    }
    let options: Options = serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
    let looks = engine::selected_looks(&app.session).ok_or("Analyze references or choose a saved look first")?;
    let generation = app.session.library_generation;
    let active = app.session.active();
    let identity = active.and_then(|id| engine::identity(&app.session, id));
    let work = engine::prepare(&mut app.session, &params).map_err(|e| e.to_string())?;
    let label = if preview { "Preview Match Look" } else { "Match Look" };
    let c = start_control(app, label, work.len())?;
    let worker = c.clone();
    let result = crate::tasks::spawn(
        app,
        label,
        move || lightcraft_engine::guard::catch("Match Look", || work.run(&worker, preview)),
        move |app, ctx, result| {
            if app.match_look.control.as_ref().is_none_or(|current| !Arc::ptr_eq(current, &c)) {
                return;
            }
            app.match_look.control = None;
            if c.cancel.load(Ordering::Relaxed) {
                app.toast(ctx, "Match Look cancelled — no photos changed");
                return;
            }
            match result {
                Ok(mut r) if preview => {
                    if identity.as_ref() != app.session.active().and_then(|id| engine::identity(&app.session, id)).as_ref()
                        || generation != app.session.library_generation
                        || engine::selected_looks(&app.session) != Some(looks.clone())
                    {
                        app.match_look.error = tr("Target or look changed — Preview again").into();
                        #[cfg(test)]
                        eprintln!(
                            "MATCH_LOOK_PREVIEW_DISCARDED identity_equal={} generation_equal={} looks_equal={}",
                            identity.as_ref() == app.session.active().and_then(|id| engine::identity(&app.session, id)).as_ref(),
                            generation == app.session.library_generation,
                            engine::selected_looks(&app.session) == Some(looks.clone())
                        );
                        return;
                    }
                    engine::retain_decoded(&mut app.session, &mut r);
                    if let (Some(image), Some(photo), Some(identity)) = (r.preview.take(), active, identity) {
                        let pixels = egui::ColorImage::from_rgba_unmultiplied(
                            [image.width, image.height],
                            &image.data.iter().flatten().copied().collect::<Vec<_>>(),
                        );
                        let texture = ctx.load_texture("match-look-preview", pixels.clone(), egui::TextureOptions::LINEAR);
                        let original = r.preview_before.take().unwrap_or(image);
                        let before_pixels = egui::ColorImage::from_rgba_unmultiplied(
                            [original.width, original.height],
                            &original.data.iter().flatten().copied().collect::<Vec<_>>(),
                        );
                        let before_texture = ctx.load_texture("match-look-before", before_pixels.clone(), egui::TextureOptions::LINEAR);
                        let record = r.applications.remove(&photo);
                        let changes = record.as_ref().map(|r| changed_summary(&r.before, &r.last, options)).unwrap_or_default();
                        let similarity = record.as_ref().map_or(1.0, |r| r.similarity);
                        let (fit, reach, wb_ratio, preview_only) =
                            record.as_ref().map_or((1.0, 1.0, 1.0, false), |r| (r.fit, r.reach, r.wb_ratio, r.preview_only));
                        let weights = record.map_or_else(Vec::new, |r| r.weights);
                        app.match_look.preview = Some(Preview {
                            options,
                            photo,
                            identity,
                            generation,
                            looks,
                            texture,
                            pixels,
                            before_texture,
                            before_pixels,
                            changes,
                            similarity,
                            fit,
                            reach,
                            wb_ratio,
                            preview_only,
                            weights,
                        });
                    } else {
                        app.match_look.error = format!("{}: {:?}", tr("Could not preview photo"), r.failed);
                    }
                }
                Ok(r) => match app.session.execute_fn("develop.matchLook", |s| engine::apply(s, r)) {
                    Ok(value) => {
                        let changed = value["changed"].as_u64().unwrap_or(0);
                        let skipped = value["skipped"].as_array().map_or(0, Vec::len);
                        app.toast(
                            ctx,
                            if value["discarded"] == true {
                                tr("Library changed — Match Look was not applied").into()
                            } else {
                                if skipped == 0 {
                                    crate::i18n::tr_format!("Match Look applied: {changed}", changed = changed)
                                } else {
                                    crate::i18n::tr_format!(
                                        "Match Look applied: {changed}; {skipped} photos edited during the job were skipped",
                                        changed = changed,
                                        skipped = skipped
                                    )
                                }
                            },
                        );
                        let weak = value["weak"].as_u64().unwrap_or(0);
                        let embedded = value["previewOnly"].as_u64().unwrap_or(0);
                        if weak > 0 || embedded > 0 {
                            app.toast(ctx, batch_details(&value));
                        }
                        let failed = value["failed"].as_array().map_or(0, Vec::len);
                        if failed > 0 {
                            app.toast(ctx, crate::i18n::tr_format!("{count} skipped (videos/offline)", count = failed));
                        }
                        app.match_look.last_result = Some(value);
                    }
                    Err(e) => {
                        app.match_look.error = e.to_string();
                        app.toast_error(ctx, e.to_string());
                    }
                },
                Err(e) => {
                    app.match_look.error = e.clone();
                    app.toast_error(ctx, e);
                }
            }
        },
    );
    if let Err(e) = result {
        app.match_look.control = None;
        return Err(e);
    }
    Ok(json!({"running":true}))
}

pub fn cancel_preview(app: &mut LightcraftApp) {
    if app.match_look.label == "Preview Match Look"
        && let Some(c) = app.match_look.control.take()
    {
        c.cancel.store(true, Ordering::Relaxed);
    }
    app.match_look.auto_preview_at = None;
}
pub fn preview_ready(app: &LightcraftApp, opts: Options) -> bool {
    app.match_look.preview.as_ref().is_some_and(|p| {
        p.options == opts
            && Some(p.photo) == app.session.active()
            && engine::identity(&app.session, p.photo).as_ref() == Some(&p.identity)
            && p.generation == app.session.library_generation
            && engine::selected_looks(&app.session) == Some(p.looks.clone())
    })
}
/// Shared jobs progress/Cancel, also drawn while the panel/dialog is closed.
pub fn progress(app: &mut LightcraftApp, ctx: &egui::Context) {
    if app.match_look.generation != app.session.library_generation {
        if let Some(c) = app.match_look.control.take() {
            c.cancel.store(true, Ordering::Relaxed);
        }
        app.match_look.preview = None;
    }
    if let Some(c) = app.match_look.control.clone()
        && widgets::job_progress(ctx, &app.match_look.label, "matchLookCancel", c.completed.load(Ordering::Relaxed), c.total)
    {
        c.cancel.store(true, Ordering::Relaxed);
    }
}
pub fn panel(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    engine::reconcile_selection(&mut app.session);
    let open = app.ui.open_sections.iter().any(|s| s == "matchLook");
    if widgets::section_header(ui, "matchLook", "Match Look", open, None).0.clicked() {
        if open {
            app.ui.open_sections.retain(|s| s != "matchLook");
        } else {
            app.ui.open_sections.push("matchLook".into());
        }
    }
    if !open {
        return;
    }
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: ui.spacing().indent as i8,
            right: ui.spacing().indent as i8,
            top: 0,
            bottom: ui.spacing().item_spacing.y as i8,
        })
        .show(ui, |ui| panel_body(app, ui));
}
fn panel_body(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let name = app
        .session
        .match_look
        .selected
        .as_ref()
        .and_then(|id| app.session.saved_looks.file.looks.iter().find(|l| &l.id == id))
        .map(|l| l.name.clone())
        .unwrap_or_else(|| tr("Analyzed references").into());
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width() - ui.spacing().interact_size.x - ui.spacing().item_spacing.x, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(ui.available_width());
                let r = widgets::dropdown(ui, "matchLookPicker", &name, t.font(12.0), t.text_label);
                egui::Popup::menu(&r).show(|ui| {
                    let mut items = vec![(None, tr("Analyzed references").to_string())];
                    items.extend(app.session.saved_looks.file.looks.iter().map(|l| (Some(l.id.clone()), l.name.clone())));
                    egui::ScrollArea::vertical().max_height(t.section_h * 8.0).show_rows(ui, widgets::TEXT_BUTTON_H, items.len(), |ui, range| {
                        for index in range {
                            let (id, name) = &items[index];
                            if widgets::text_button_translated(ui, &format!("matchLookPick-{index}"), name, false).clicked() {
                                let _ = app.run("develop.savedLook", json!({"action":"select","id":id}));
                                app.match_look.preview = None;
                                ui.close();
                            }
                        }
                    });
                });
            },
        );
        let menu = widgets::icon_button(ui, "matchLookMenu", crate::icons::Icon::More, ui.spacing().interact_size, false, true, "Look actions");
        egui::Popup::menu(&menu).show(|ui| look_menu(app, ui, &name));
    });
    let references = app.session.match_look.references.clone();
    let reference_bucket =
        lightcraft_engine::media::thumb_bucket((widgets::reference_tile_edge(ui.ctx()) * ui.ctx().pixels_per_point()).ceil() as usize);
    let items = references
        .iter()
        .map(|id| {
            crate::panels::grid::request_thumb(app, *id, reference_bucket, 40);
            let name = app.session.catalog.photo(*id).map(|p| p.file_name.clone()).unwrap_or_else(|| tr("Missing reference").into());
            let texture = app.renderer.thumb(*id).map(|tex| (tex.tex.id(), egui::vec2(tex.size[0] as f32, tex.size[1] as f32)));
            (id.0, name, texture)
        })
        .collect::<Vec<_>>();
    let pinned = app.session.match_look.reference_weights.iter().filter(|(_, w)| **w == 2.0).map(|(id, _)| id.0).collect::<Vec<_>>();
    if let Some(action) = widgets::reference_tray(ui, &items, &pinned) {
        match action {
            widgets::ReferenceAction::Remove(id) => {
                let _ = app.run("develop.matchLookReferences", json!({"remove":id}));
            }
            widgets::ReferenceAction::Pin(id) => {
                let _ = app.run("develop.matchLookReferences", json!({"weightId":id}));
                app.match_look.preview = None;
            }
            widgets::ReferenceAction::Compare(id) => {
                app.match_look.reference_index = references.iter().position(|p| p.0 == id).unwrap_or(0);
                app.match_look.compare_reference = true;
            }
        }
    }
    let busy = app.match_look.control.is_some();
    ui.horizontal_wrapped(|ui| {
        let why = if app.session.selection.ids.is_empty() {
            Some("Select reference photos first")
        } else if references.len() >= 10 {
            Some("At most 10 references; remove one first")
        } else if busy {
            Some("Wait for the current job")
        } else {
            None
        };
        if widgets::text_action(ui, "matchLookAdd", "Add selected", why).clicked()
            && let Err(e) = app.run("develop.matchLookReferences", json!({}))
        {
            app.match_look.error = e;
        }
        let why = if busy {
            Some("Wait for the current job")
        } else if references.is_empty() {
            Some("Add reference photos first")
        } else {
            None
        };
        if widgets::text_action(ui, "matchLookAnalyze", if engine::current_ready(&app.session) { "Re-analyze" } else { "Analyze" }, why).clicked()
            && let Err(e) = analyze(app, None)
        {
            app.match_look.error = e;
        }
    });
    let ready = engine::current_ready(&app.session);
    let status = if busy && app.match_look.label == "Analyze Match Look" {
        tr("Analyzing…").into()
    } else if let Some(saved) =
        app.session.match_look.selected.as_ref().and_then(|id| app.session.saved_looks.file.looks.iter().find(|l| &l.id == id))
    {
        crate::i18n::tr_format!("Saved look: {count} references", count = saved.reference_count)
    } else if ready {
        crate::i18n::tr_format!("Analyzed: {count} references", count = references.len())
    } else if app.session.match_look.analysis.is_some() {
        tr("References changed — Analyze again").into()
    } else {
        tr("Add 1–10 edited references, then Analyze").into()
    };
    ui.label(egui::RichText::new(status).font(t.font(11.5)).color(t.text_dim));
    if let Some(looks) = engine::selected_looks(&app.session) {
        let w = vec![1.0 / looks.len() as f64; looks.len()];
        let blend = pipeline::blend(&looks, &w);
        if pipeline::is_monochrome(&blend) {
            widgets::summary_chips(ui, &[tr("Black & white look").into()]);
        }
        ui.scope(|ui| widgets::summary_chips(
            ui,
            &[
                crate::i18n::tr_format!("Midtone lightness {value:.0}/100", value = blend.features[2] * 100.0),
                crate::i18n::tr_format!("Warmth {value:+.1} ({direction})", value = blend.features[6] * 100.0, direction = tr(if blend.features[6]>=0.0 {"warm"} else {"cool"})),
                crate::i18n::tr_format!("Color intensity {value:.1}/100", value = blend.features[7] * 100.0),
            ],
        )).response.on_hover_text(tr("Finished-look measurements: midtone lightness (0 black, 100 white); warmth (Oklab b ×100, positive warm/yellow, negative cool/blue); color intensity (Oklab chroma ×100, zero neutral)."));
    }
    ui.horizontal_wrapped(|ui| {
        let why = if busy {
            Some("Wait for the current job")
        } else if !ready {
            Some("Analyze current references before saving")
        } else {
            None
        };
        if widgets::text_action(ui, "matchLookSave", "Save Look…", why).clicked() {
            prompt(app, "Save Look", "", "save", None);
        }
        let why = if busy {
            Some("Wait for the current job")
        } else if engine::ready(&app.session).is_err() {
            Some("Analyze references or choose a saved look first")
        } else {
            None
        };
        if widgets::text_action(ui, "matchLook", "Match Look…", why)
            .on_hover_text(tr("Sync copies slider values. Match Look measures the finished look and solves each target's own editable sliders."))
            .clicked()
        {
            let _ = app.run("dialog.matchLook", json!({}));
        }
    });
    if let Some(error) = &app.session.saved_looks.error {
        ui.label(egui::RichText::new(error).color(t.caution));
    }
    if !app.match_look.error.is_empty() {
        ui.label(egui::RichText::new(&app.match_look.error).color(t.caution));
    }
    if let Some(skipped) = app.match_look.last_result.as_ref().and_then(|r| r["skipped"].as_array()).filter(|ids| !ids.is_empty()) {
        let names = skipped
            .iter()
            .filter_map(|id| id.as_u64())
            .map(|id| app.session.catalog.photo(lightcraft_catalog::PhotoId(id)).map_or_else(|| format!("#{id}"), |p| p.file_name.clone()))
            .collect::<Vec<_>>();
        ui.label(
            egui::RichText::new(format!("{}: {}", tr("Skipped photos edited during the job"), names.join(", "))).font(t.font(11.5)).color(t.caution),
        );
    }
    if let Some(result) = &app.match_look.last_result {
        if result.get("weak").is_some() {
            ui.label(batch_details(result));
        }
        if result["weak"].as_u64().unwrap_or(0) > 0 && widgets::text_action(ui, "matchWeak", "Review weak matches", None).clicked() {
            let ids = result["perPhoto"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["reach"].as_f64().unwrap_or(1.0) < 0.6 || (r["alreadyClose"] != true && r["fit"].as_f64().unwrap_or(1.0) < 0.5))
                .map(|r| r["id"].clone())
                .collect::<Vec<_>>();
            let _ = app.run("library.filter", json!({"only":ids}));
        }
    }
    engine::reconcile_applied(&mut app.session);
    if let Some(id) = app.session.active()
        && let Some(record) = app.session.match_look.applied.get(&id).cloned()
    {
        ui.separator();
        ui.label(egui::RichText::new(tr("Applied to this photo")).font(t.font(12.0)).color(t.text_label));
        if record.before.treatment == lightcraft_develop::Treatment::Bw {
            widgets::summary_chips(ui, &[tr("Black & white target").into(), tr("Color groups skipped").into()]);
        }
        similarity(ui, record.similarity, &record.weights);
        result_quality(ui, record.fit, record.reach, record.wb_ratio, record.preview_only);
        let editable = app.session.develop_of(id).as_deref() == Some(&record.last)
            && engine::identity(&app.session, id).is_some_and(|i| i.key == record.key)
            && !busy;
        let spec = strength_spec("matchLook.photoStrength");
        let out = widgets::slider(ui, &spec, record.options.strength, editable, None);
        crate::panels::edit::apply_slider_out(app, &lightcraft_develop::ControlSpec { label: "Match Look amount", ..spec }, out, |app, strength| {
            app.run("develop.matchLookAmount", json!({"strength":strength}))
        });
        let rows = changed_summary(&record.before, &record.last, record.options);
        let why = (!editable).then_some("Photo edited since Match Look; undo those edits before changing its amount");
        if let Some(group) = widgets::change_summary(ui, "matchApplied", &rows, why) {
            let key = match group.as_str() {
                "Normalize" => "normalize",
                "Light & Contrast" => "light",
                "Color Adjust" => "color",
                _ => "tone",
            };
            if let Err(error) = app.run("develop.matchLookAmount", json!({"resetGroup":key})) {
                app.match_look.error = error;
            }
        }
    }
}
pub(crate) fn batch_details(value: &Value) -> String {
    crate::i18n::tr_format!(
        "{weak} photos matched weakly; {embedded} on embedded previews",
        weak = value["weak"].as_u64().unwrap_or(0),
        embedded = value["previewOnly"].as_u64().unwrap_or(0)
    )
}
fn result_quality(ui: &mut egui::Ui, fit: f64, reach: f64, wb_ratio: f64, preview_only: bool) {
    let top = ui.cursor().top();
    widgets::summary_chips(
        ui,
        &[
            crate::i18n::tr_format!("Fit {value:.0}%", value = fit * 100.0),
            crate::i18n::tr_format!("Tonal range {value:.0}%", value = reach * 100.0),
            crate::i18n::tr_format!("WB shift {value:.2}×", value = wb_ratio),
            tr(if preview_only { "Embedded preview" } else { "Decoded photo" }).into(),
        ],
    );
    widgets::register(
        ui.ctx(),
        "matchLook:quality",
        egui::Rect::from_min_max(egui::pos2(ui.min_rect().left(), top), egui::pos2(ui.min_rect().right(), ui.cursor().top())),
    );
    if reach < 0.6 {
        let t = Tokens::get(ui.ctx());
        ui.label(
            egui::RichText::new(tr("References have a wider tonal range than this photo can reach — Light & Contrast was limited")).color(t.caution),
        );
    }
}
fn prompt(app: &mut LightcraftApp, title: &str, value: &str, action: &str, id: Option<&str>) {
    app.ui.dialog = Some(Dialog::TextPrompt {
        title: tr(title).into(),
        hint: tr("Look name").into(),
        value: value.into(),
        command: "develop.savedLook".into(),
        params: json!({"action":action,"id":id}),
        key: "name".into(),
    });
}
pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, options: &mut Options) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        egui::RichText::new(tr(
            "Sync copies slider values. Match Look analyses finished reference photos and solves each target's own editable Develop sliders.",
        ))
        .font(t.font(11.5))
        .color(t.text_label),
    );
    if app.session.active().and_then(|id| app.session.develop_of(id)).is_some_and(|d| d.treatment == lightcraft_develop::Treatment::Bw) {
        widgets::summary_chips(ui, &[tr("Black & white target").into(), tr("Color groups skipped").into()]);
    }
    let before = *options;
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, "matchNormalize", &mut options.normalize, "Normalize")
            .on_hover_text(tr("Exposure follows the references fully for similar scenes and partially for different ones; Consistent batch always matches fully. Strength changes the style groups only."));
        widgets::checkbox(ui, "matchLight", &mut options.light_contrast, "Light & Contrast");
    });
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, "matchColor", &mut options.color_adjust, "Color Adjust");
        widgets::checkbox(ui, "matchTone", &mut options.color_tone, "Color Tone");
    });
    const STRENGTH: lightcraft_develop::ControlSpec = strength_spec("matchLook.strength");

    let out = widgets::slider(ui, &STRENGTH, options.strength, true, None);
    if let Some(value) = out.value {
        options.strength = value;
    }
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, "matchSkin", &mut options.protect_skin, "Protect skin").on_hover_text(tr(
            "Protect skin color in cached People faces and limit hue shifts. Without faces, warm neutrals are measured without a skin constraint.",
        ));
        widgets::checkbox(ui, "matchKeepWB", &mut options.keep_white_balance, "Keep white balance");
    });
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, "matchKeepExposure", &mut options.keep_exposure, "Keep exposure");
        widgets::checkbox(ui, "matchContrastGuard", &mut options.contrast_guard, "Contrast guard")
            .on_hover_text(tr("Give Contrast a smaller share of the correction; prefer Whites, Blacks and the curve."));
    });
    if widgets::text_action(ui, "matchColourOnly", "Color only", None).clicked() {
        options.normalize = false;
        options.light_contrast = false;
        options.color_adjust = true;
        options.color_tone = true;
    }
    ui.horizontal_wrapped(|ui| {
        widgets::checkbox(ui, "matchNewLayer", &mut options.new_layer, "Apply as new layer");
        widgets::checkbox(ui, "matchConsistent", &mut options.consistent_batch, "Consistent batch")
            .on_hover_text(tr("Use the same reference blend for every target. Otherwise references are weighted by each target's scene similarity."));
    });
    widgets::checkbox(ui, "matchRegions", &mut options.region_aware, "Match supported regions (experimental)");
    if before != *options
        && (app.match_look.preview.is_some()
            || app.match_look.auto_preview_at.is_some()
            || app.match_look.label == "Preview Match Look" && app.match_look.control.is_some())
    {
        cancel_preview(app);
        app.match_look.auto_preview_at = Some(web_time::Instant::now());
    }
    if let Some(at) = app.match_look.auto_preview_at {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        if at.elapsed() >= std::time::Duration::from_millis(300) && app.match_look.control.is_none() {
            app.match_look.auto_preview_at = None;
            if let Err(error) = start(app, json!(*options), true) {
                app.match_look.error = error;
            }
        }
    }
    app.session.match_look.options = *options;
    if widgets::text_action(ui, "matchLookPreview", "Preview", app.match_look.control.as_ref().map(|_| "Wait for the current job")).clicked()
        && let Err(e) = start(app, json!(*options), true)
    {
        app.match_look.error = e;
    }
    let same_analysis = engine::current_ready(&app.session)
        && app.session.match_look.analysis.as_ref().is_some_and(|a| app.session.match_look.selected.is_none() && !a.looks.is_empty());
    let reference = same_analysis
        .then(|| {
            app.session
                .match_look
                .references
                .get(app.match_look.reference_index % app.session.match_look.references.len().max(1))
                .and_then(|id| app.renderer.thumb(*id))
                .map(|tex| (tex.tex.id(), egui::vec2(tex.size[0] as f32, tex.size[1] as f32)))
        })
        .flatten();
    let mut choices = vec![(tr("Before").to_string(), "before".to_string())];
    if same_analysis {
        for i in 0..app.session.match_look.references.len() {
            choices.push((crate::i18n::tr_format!("Ref {number}", number = i + 1), (i + 1).to_string()));
        }
    }
    let items = choices.iter().map(|(label, key)| (label.as_str(), key.as_str())).collect::<Vec<_>>();
    let active = if app.match_look.compare_reference && same_analysis { app.match_look.reference_index + 1 } else { 0 };
    if let Some(index) = widgets::segmented(ui, "matchCompare", &items, Some(active), 3) {
        app.match_look.compare_reference = index > 0;
        app.match_look.reference_index = index.saturating_sub(1);
    }
    if let Some(p) = app.match_look.preview.as_ref().filter(|_| preview_ready(app, *options)) {
        similarity(ui, p.similarity, &p.weights);
        result_quality(ui, p.fit, p.reach, p.wb_ratio, p.preview_only);
        let original = Some((p.before_texture.id(), p.before_texture.size_vec2()));
        let left = if app.match_look.compare_reference { reference.or(original) } else { original };
        widgets::compare_preview(
            ui,
            "matchLook:preview",
            left,
            Some((p.texture.id(), p.texture.size_vec2())),
            if app.match_look.compare_reference && reference.is_some() { "Reference" } else { "Before" },
            &mut app.match_look.wipe,
        );
        ui.label(egui::RichText::new(tr("What changed")).font(t.font(12.0)).color(t.text_label));
        if let Some(group) = widgets::change_summary(ui, "matchPreview", &p.changes, None) {
            match group.as_str() {
                "Normalize" => options.normalize = false,
                "Light & Contrast" => options.light_contrast = false,
                "Color Adjust" => options.color_adjust = false,
                "Color Tone" => options.color_tone = false,
                _ => {}
            }
            cancel_preview(app);
            app.match_look.auto_preview_at = Some(web_time::Instant::now());
        }
    } else {
        widgets::image_preview(ui, "matchLook:preview", None, "Optional preview of the active target");
    }
    if !app.match_look.error.is_empty() {
        ui.label(egui::RichText::new(&app.match_look.error).color(t.caution));
    }
}
pub fn preview_pixels(app: &LightcraftApp) -> Vec<(egui::TextureId, egui::ColorImage)> {
    app.match_look
        .preview
        .as_ref()
        .map_or_else(Vec::new, |p| vec![(p.texture.id(), p.pixels.clone()), (p.before_texture.id(), p.before_pixels.clone())])
}
const fn strength_spec(id: &'static str) -> lightcraft_develop::ControlSpec {
    lightcraft_develop::ControlSpec {
        id,
        label: "Strength (%)",
        section: lightcraft_develop::Section::Color,
        min: 0.0,
        max: 125.0,
        default: 100.0,
        step: 1.0,
        decimals: 0,
        track: lightcraft_develop::Track::Plain,
    }
}
fn similarity(ui: &mut egui::Ui, value: f64, weights: &[f64]) {
    let t = Tokens::get(ui.ctx());
    widgets::summary_chips(ui, &[crate::i18n::tr_format!("Scene similarity {value:.0}%", value = value * 100.0)]);
    if value < 0.45 {
        ui.label(egui::RichText::new(tr("Very different from the references — results may be weak")).font(t.font(11.5)).color(t.caution));
    }
    if weights.len() > 1 {
        ui.label(
            egui::RichText::new(crate::i18n::tr_format!(
                "Reference weights: {weights}",
                weights = weights.iter().enumerate().map(|(i, w)| format!("{}: {:.0}%", i + 1, w * 100.0)).collect::<Vec<_>>().join(" · ")
            ))
            .font(t.font(11.0))
            .color(t.text_dim),
        );
    }
}
fn changed_summary(
    before: &lightcraft_develop::DevelopSettings,
    after: &lightcraft_develop::DevelopSettings,
    opts: Options,
) -> Vec<(&'static str, String)> {
    use lightcraft_develop::controls;
    let d = if opts.new_layer {
        after
            .masks
            .last()
            .map(|m| lightcraft_develop::DevelopSettings {
                light: m.tools.light.unwrap_or_default(),
                curve: m.tools.curve.clone().unwrap_or_default(),
                color: m.tools.color.unwrap_or_default(),
                mixer: m.tools.mixer.unwrap_or_default(),
                grading: m.tools.grading.unwrap_or_default(),
                effects: m.tools.effects.unwrap_or_default(),
                treatment: m.tools.treatment.unwrap_or(before.treatment),
                wb: lightcraft_develop::WhiteBalance { temp: m.adjust.temp, tint: m.adjust.tint, ..Default::default() },
                ..Default::default()
            })
            .unwrap_or_default()
    } else {
        after.clone()
    };
    let b = if opts.new_layer {
        let mut b = lightcraft_develop::DevelopSettings::default();
        b.wb.temp = 0.0;
        b.wb.tint = 0.0;
        b
    } else {
        before.clone()
    };
    let groups = [
        ("Normalize", vec!["light.exposure", "wb.temp", "wb.tint"]),
        (
            "Light & Contrast",
            vec![
                "light.contrast",
                "light.whites",
                "light.blacks",
                "light.shadows",
                "light.highlights",
                "curve.shadows",
                "curve.darks",
                "curve.lights",
                "curve.highlights",
                "effects.clarity",
            ],
        ),
        (
            "Color Adjust",
            vec![
                "color.saturation",
                "color.vibrance",
                "mixer.red.sat",
                "mixer.orange.sat",
                "mixer.yellow.sat",
                "mixer.green.sat",
                "mixer.aqua.sat",
                "mixer.blue.sat",
                "mixer.purple.sat",
                "mixer.magenta.sat",
                "mixer.red.hue",
                "mixer.orange.hue",
                "mixer.yellow.hue",
                "mixer.green.hue",
                "mixer.aqua.hue",
                "mixer.blue.hue",
                "mixer.purple.hue",
                "mixer.magenta.hue",
                "mixer.red.lum",
                "mixer.orange.lum",
                "mixer.yellow.lum",
                "mixer.green.lum",
                "mixer.aqua.lum",
                "mixer.blue.lum",
                "mixer.purple.lum",
                "mixer.magenta.lum",
            ],
        ),
        (
            "Color Tone",
            vec![
                "grading.shadows.sat",
                "grading.midtones.sat",
                "grading.highlights.sat",
                "grading.shadows.hue",
                "grading.midtones.hue",
                "grading.highlights.hue",
                "grading.global.sat",
                "grading.global.hue",
            ],
        ),
    ];
    groups
        .into_iter()
        .filter_map(|(group, ids)| {
            let mut changed = ids
                .iter()
                .filter_map(|id| {
                    let v = controls::get(&d, id)?;
                    let from = controls::get(&b, id)?;
                    if (v - from).abs() < 0.05 {
                        return None;
                    }
                    let name = controls::find(id).map_or(*id, |s| s.label);
                    let prefix = if id.starts_with("curve.") {
                        tr("Curve").to_string()
                    } else if id.starts_with("mixer.") || id.starts_with("grading.") {
                        let band = id.split('.').nth(1).unwrap_or("");
                        if name.to_lowercase().contains(band) { String::new() } else { band.to_string() }
                    } else {
                        String::new()
                    };
                    let unit = if *id == "light.exposure" {
                        " EV"
                    } else if *id == "wb.temp" && !opts.new_layer {
                        " K"
                    } else {
                        ""
                    };
                    let formatted = |value: f64| controls::find(id).map_or_else(|| format!("{value:.1}"), |s| s.format(value));
                    Some(format!(
                        "{}{} {} → {}{unit}",
                        if prefix.is_empty() { String::new() } else { format!("{prefix} ") },
                        tr(name),
                        formatted(from),
                        formatted(v)
                    ))
                })
                .collect::<Vec<_>>();
            if group == "Color Adjust" && d.treatment != b.treatment {
                changed.push(tr("Treatment Color → B&W").into());
            }
            (!changed.is_empty()).then(|| (group, crate::i18n::tr_format!("{group}: {changes}", group = tr(group), changes = changed.join(" · "))))
        })
        .collect()
}

fn look_menu(app: &mut LightcraftApp, ui: &mut egui::Ui, name: &str) {
    let busy = app.match_look.control.is_some();
    let references = app.session.match_look.references.clone();
    if let Some(id) = app.session.match_look.selected.clone().filter(|id| app.session.saved_looks.file.looks.iter().any(|l| &l.id == id)) {
        ui.vertical(|ui| {
            if widgets::menu_action(ui, "matchLookRename", "Rename…", busy.then_some("Wait for the current job")).clicked() {
                prompt(app, "Rename Look", name, "rename", Some(&id));
                ui.close();
            }
            if widgets::menu_action(ui, "matchLookDuplicate", "Duplicate", busy.then_some("Wait for the current job")).clicked() {
                let _ = app.run("develop.savedLook", json!({"action":"duplicate","id":id}));
                ui.close();
            }
            if widgets::menu_action(ui, "matchLookDelete", "Delete", busy.then_some("Wait for the current job")).clicked() {
                let _ = app.run("develop.savedLook", json!({"action":"delete","id":id}));
                ui.close();
            }
            if widgets::menu_action(
                ui,
                "matchLookUpdate",
                "Update from current references…",
                (busy || references.is_empty()).then_some("Add current references and wait for the current job"),
            )
            .clicked()
            {
                app.ui.dialog = Some(Dialog::UpdateMatchLook { id, name: name.into() });
                ui.close();
            }
        });
    }

    for (id, label, command, reason) in [
        ("matchLookImport", "Import Look…", "file.importLook", busy.then_some("Wait for the current job")),
        (
            "matchLookExport",
            "Export Look…",
            "file.exportLook",
            (busy || engine::selected_looks(&app.session).is_none()).then_some("Analyze references or choose a saved look first"),
        ),
    ] {
        if widgets::menu_action(ui, id, label, reason).clicked() {
            if let Err(error) = app.run(command, json!({})) {
                app.match_look.error = error;
            }
            ui.close();
        }
    }
}
