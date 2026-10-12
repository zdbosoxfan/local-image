//! Fixed Generate actions and completion choices, outside the settings viewport.
use super::*;
use photocraft_engine::jobs::{JobId, JobInfo};

#[derive(Default)]
pub(super) struct Action {
    pub label: String,
    pub can: bool,
    pub why: String,
    pub enter: bool,
    pub count: bool,
    pub warnings: Vec<String>,
    pub cloud: bool,
}

pub(super) struct Layout {
    pub bounds: egui::Rect,
    pub settings: egui::Rect,
    controls: egui::Rect,
}

fn selected_result() -> Option<Entry> {
    rt(|r| {
        r.selected
            .as_ref()
            .and_then(|id| {
                r.results.iter().find_map(|tile| match tile {
                    Tile::Done(entry) if entry.id == *id => Some(entry.clone()),
                    _ => None,
                })
            })
            .or_else(|| r.results.iter().find_map(|tile| if let Tile::Done(entry) = tile { Some(entry.clone()) } else { None }))
    })
}

fn jobs(app: &PhotocraftApp) -> Vec<JobInfo> {
    let running = app.session.jobs();
    rt(|r| {
        r.panel_jobs.retain(|id| running.iter().any(|job| job.id == *id));
        let ids: Vec<JobId> = r.results.iter().filter_map(|tile| if let Tile::Pending { job, .. } = tile { Some(JobId(*job)) } else { None }).collect();
        running.into_iter().filter(|job| r.panel_jobs.contains(&job.id) || ids.contains(&job.id)).collect()
    })
}

pub(super) fn layout(ui: &mut egui::Ui) -> Layout {
    let bounds = ui.available_rect_before_wrap();
    let height = if selected_result().is_some() { 120.0 } else { 58.0 };
    let top = (bounds.bottom() - height).max(bounds.top());
    let settings = egui::Rect::from_min_max(bounds.min, egui::pos2(bounds.right(), (top - 4.0).max(bounds.top())));
    let controls = egui::Rect::from_min_max(egui::pos2(bounds.left(), top), bounds.max);
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("generate-panel-layout"), (bounds, settings)));
    ui.allocate_rect(bounds, Sense::hover());
    Layout { bounds, settings, controls }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct Hit {
    pub name: &'static str,
    pub rect: egui::Rect,
    pub clip: egui::Rect,
    pub enabled: bool,
}
pub(super) fn note(ui: &egui::Ui, name: &'static str, response: &egui::Response) {
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        let id = egui::Id::new("generate-pinned-hits");
        let hits = data.get_temp_mut_or_default::<Vec<Hit>>(id);
        hits.push(Hit { name, rect: response.rect, clip: ui.clip_rect(), enabled: response.enabled() });
    });
    #[cfg(not(test))]
    let _ = (ui, name, response);
}

pub(super) fn show(app: &mut PhotocraftApp, parent: &mut egui::Ui, layout: &Layout, action: Action) {
    #[cfg(test)]
    parent.ctx().data_mut(|data| data.insert_temp(egui::Id::new("generate-pinned-hits"), Vec::<Hit>::new()));
    let mut ui =
        parent.new_child(egui::UiBuilder::new().id_salt("generate-pinned-controls").max_rect(layout.controls).layout(egui::Layout::top_down(egui::Align::Min)));
    ui.set_clip_rect(parent.clip_rect().intersect(layout.controls));
    ui.set_max_width(layout.bounds.width());
    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
    let running = jobs(app);
    let mut count = app.ui.ai.generate.count as f32;
    let browse_only = selected_result().is_none() && rt(|runtime| !runtime.results.is_empty());
    let clicked = ui
        .horizontal(|ui| {
            let stop_width = if running.is_empty() { 0.0 } else { 64.0 };
            let count_width = if action.count { 46.0 } else { 0.0 };
            let width = (ui.available_width() - stop_width - count_width - if browse_only { 66.0 } else { 0.0 }).max(40.0);
            let response = ui.add_enabled_ui(action.can, |ui| widgets::primary_button(ui, &action.label, width)).inner.on_disabled_hover_text(&action.why);
            note(ui, "generate", &response);
            if action.count {
                widgets::value_field(ui, &mut count, 1.0..=4.0, "", 40.0).on_hover_text(tl!("Images per run (1–4)"));
            }
            if !running.is_empty() {
                let stop = widgets::secondary_button(ui, tl!("Stop"), 58.0).on_hover_text(tl!("Stop generation"));
                note(ui, "stop", &stop);
                if stop.clicked() {
                    crate::ai_ui::stop_generation(app, &running.iter().map(|job| job.id).collect::<Vec<_>>());
                }
            }
            if browse_only {
                let results = ui.small_button(tl!("RESULTS"));
                note(ui, "results", &results);
                egui::Popup::from_toggle_button_response(&results).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_width(280.0);
                    egui::ScrollArea::vertical().id_salt("generate-pending-result-history").max_height(240.0).show(ui, |ui| results_ui(app, ui));
                });
            }
            response.clicked()
        })
        .inner;
    app.ui.ai.generate.count = (count.round() as u32).clamp(1, 4);
    if (clicked || action.enter) && action.can {
        start(app, ui.ctx());
    }
    if running.is_empty() {
        let failure = rt(|r| r.results.front().and_then(|tile| if let Tile::Failed(error) = tile { Some(error.clone()) } else { None }));
        let text = failure.as_deref().unwrap_or(if action.can { tl!("Ready") } else { &action.why });
        ui.add(
            egui::Label::new(RichText::new(text).size(11.0).color(if failure.is_some() {
                Tokens::get(ui.ctx()).warning
            } else {
                Tokens::get(ui.ctx()).text_faint
            }))
            .truncate(),
        )
        .on_hover_text(text);
    } else {
        let progress = running.iter().map(|job| job.progress).sum::<f32>() / running.len() as f32;
        let response = ui
            .add(egui::ProgressBar::new(progress).show_percentage().desired_width(ui.available_width()))
            .on_hover_text(running.iter().map(|job| format!("{}: {}", job.label, job.message)).collect::<Vec<_>>().join("\n"));
        note(&ui, "progress", &response);
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    if let Some(entry) = selected_result() {
        completion(app, &mut ui, &entry);
    }
}

fn completion(app: &mut PhotocraftApp, ui: &mut egui::Ui, entry: &Entry) {
    let t = Tokens::get(ui.ctx());
    let library = library();
    let mut selected = None;
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
        note(ui, "thumbnail", &response);
        widgets::checker(ui.painter(), rect, 6.0);
        if let Some(texture) = thumb_texture(ui.ctx(), &entry.id, &library.thumbnail_path(&entry.id)) {
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            ui.painter().image(
                texture.id(),
                egui::Rect::from_center_size(rect.center(), size * scale),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let results = ui.small_button(tl!("RESULTS"));
                note(ui, "results", &results);
                egui::Popup::from_toggle_button_response(&results).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    ui.set_width(280.0);
                    egui::ScrollArea::vertical().id_salt("generate-result-history").max_height(240.0).show(ui, |ui| results_ui(app, ui));
                });
                ui.add(egui::Label::new(RichText::new(&entry.name).size(11.0).color(t.text_dim)).truncate()).on_hover_text(entry.prompt());
            });
            ui.horizontal(|ui| {
                let open = widgets::secondary_button(ui, tl!("Open"), 0.0).on_hover_text(tl!("Open as a new document"));
                note(ui, "open", &open);
                if open.clicked() {
                    selected = Some("open");
                }
                let insert = ui
                    .add_enabled_ui(app.session.active().is_some(), |ui| widgets::secondary_button(ui, tl!("Insert"), 0.0))
                    .inner
                    .on_hover_text(tl!("Insert as a new layer in the active document"));
                note(ui, "insert", &insert);
                if insert.clicked() {
                    selected = Some("place");
                }
            });
        });
    });
    if let Some(action) = selected {
        entry_action(app, ui.ctx(), entry, action);
    }
}

/// Evaluate readiness without constructing offscreen widgets.
pub(super) fn action(app: &PhotocraftApp) -> Action {
    let s = &app.ui.ai.generate;
    let has_doc = app.session.active().is_some();
    if s.mode == Mode::Upscale && !upscale_any() {
        let ready = crate::ai_ui::enhance_ready(s.enhance_method, s.enhance_backend, s.upscale as u32);
        let warnings = ready.as_ref().err().cloned().into_iter().collect();
        return Action {
            label: tl!("Upscale").into(),
            can: has_doc && ready.is_ok(),
            why: if !has_doc { tl!("Open an image first.").into() } else { ready.err().unwrap_or_default() },
            warnings,
            ..Default::default()
        };
    }
    let st = crate::ai_ui::status();
    let custom = custom_of(s);
    let cloud = cloud_of(s);
    let m = model_of(s);
    let ready = match (&custom, cloud) {
        (_, Some(c)) => cloud_ready(c),
        (Some(_), _) if st.connected => Ok(()),
        (Some(_), _) => Err(tl!("The AI engine (ComfyUI) is not running.").to_owned()),
        (None, None) if s.mode == Mode::Refine => refine_readiness(&st, m, &s.variant),
        (None, None) => readiness(&st, m, &s.variant),
    };
    let refine = if s.refine && s.mode == Mode::Create && custom.is_none() && cloud.is_none() {
        match ModelId::from_key(&s.refine_model) {
            Some(rm) => refine_readiness(&st, rm, &s.refine_variant).map_err(|error| crate::i18n::fmt(tl!("Refine model: {why}"), &[("why", &error)])),
            None => Err(tl!("Choose a model to refine with.").into()),
        }
    } else {
        Ok(())
    };
    let wants_prompt = !(s.mode == Mode::Upscale && custom.is_none() && cloud.is_none() && is_upscaler(m.info()))
        && custom.as_ref().is_none_or(|workflow| workflow.has(&li_ai::custom::FieldKind::Prompt));
    let blocker = match s.mode {
        _ if wants_prompt && matches!(s.mode, Mode::Create | Mode::Edit | Mode::Fill) && s.prompt.trim().is_empty() => {
            Some(tl!("Describe what you want first."))
        }
        Mode::Edit | Mode::Refine | Mode::Upscale if !has_doc => Some(tl!("Open an image first.")),
        Mode::Refine if s.refine_input == RefineInput::SelectedLayer && app.session.active().is_none_or(|doc| doc.active_layer.is_none()) => {
            Some(tl!("Select a layer first."))
        }
        Mode::Fill if app.session.active().is_none_or(|doc| doc.doc.selection.is_none()) => Some(tl!("Make a selection to fill.")),
        _ => None,
    };
    let label = match s.mode {
        Mode::Create if s.count > 1 => crate::i18n::fmt(tl!("Generate {n}"), &[("n", &s.count.to_string())]),
        Mode::Create => tl!("Generate").into(),
        Mode::Edit => tl!("Apply Edit").into(),
        Mode::Fill => tl!("Fill Selection").into(),
        Mode::Refine => tl!("Refine").into(),
        Mode::Upscale => crate::i18n::fmt(tl!("Upscale {scale}×"), &[("scale", &widgets::fmt_num(s.upscale as f64))]),
    };
    Action {
        label,
        can: blocker.is_none() && ready.is_ok() && refine.is_ok(),
        why: blocker.map(str::to_owned).or_else(|| ready.as_ref().err().cloned()).or_else(|| refine.as_ref().err().cloned()).unwrap_or_default(),
        count: s.mode == Mode::Create,
        warnings: [ready.err(), refine.err()].into_iter().flatten().collect(),
        cloud: cloud.is_some(),
        ..Default::default()
    }
}

// Use the measured galley for squeezed controls: a response rectangle alone
// does not prove that egui's internally padded button text was not ellipsized.
fn compact_text(ui: &egui::Ui, label: &str) -> std::sync::Arc<egui::Galley> {
    ui.painter().layout_no_wrap(label.to_owned(), crate::theme::medium(12.0), Tokens::get(ui.ctx()).text)
}
fn compact_button(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let galley = compact_text(ui, label);
    let (rect, response) = ui.allocate_exact_size(vec2(width.max(galley.size().x + 14.0), 22.0), Sense::click());
    ui.painter().rect_filled(rect, t.radius_sm, if response.hovered() { t.hover } else { t.field });
    let text_rect = egui::Rect::from_center_size(rect.center(), galley.size());
    ui.painter().galley_with_override_text_color(text_rect.min, galley, if ui.is_enabled() { t.text } else { t.text_faint });
    if response.has_focus() {
        ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label));
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("generate-compact-label", label)), (text_rect, rect, ui.clip_rect())));
    response
}

pub(super) fn show_compact(app: &mut PhotocraftApp, parent: &mut egui::Ui, layout: &Layout) {
    #[cfg(test)]
    parent.ctx().data_mut(|data| data.insert_temp(egui::Id::new("generate-pinned-hits"), Vec::<Hit>::new()));
    restore_upscale_choice(&app.ui.ai.generate);
    let action = action(app);
    let running = jobs(app);
    let mut ui =
        parent.new_child(egui::UiBuilder::new().id_salt("generate-compact-controls").max_rect(layout.bounds).layout(egui::Layout::top_down(egui::Align::Min)));
    ui.set_clip_rect(parent.clip_rect().intersect(layout.bounds));
    ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
    let has_results = rt(|runtime| !runtime.results.is_empty());
    ui.horizontal(|ui| {
        // A squeezed 80px dock can leave only 26px after chrome. All actions and
        // progress must share one row; reserve widths before laying out long labels.
        let stop_width = compact_text(ui, tl!("Stop")).size().x + 14.0;
        let results_width = compact_text(ui, tl!("View")).size().x + 14.0;
        let reserved = 22.0 + if has_results { results_width + 4.0 } else { 0.0 } + if running.is_empty() { 4.0 } else { stop_width + 38.0 + 12.0 };
        let start_width = (ui.available_width() - reserved).max(24.0);
        let start_button = ui
            .add_enabled_ui(action.can, |ui| compact_button(ui, &action.label, start_width))
            .inner
            .on_hover_text(&action.label)
            .on_disabled_hover_text(&action.why);
        note(ui, "generate", &start_button);
        if start_button.clicked() {
            start(app, ui.ctx());
        }
        if !running.is_empty() {
            let stop = compact_button(ui, tl!("Stop"), stop_width).on_hover_text(tl!("Stop generation"));
            note(ui, "stop", &stop);
            if stop.clicked() {
                crate::ai_ui::stop_generation(app, &running.iter().map(|job| job.id).collect::<Vec<_>>());
            }
        }
        let settings = crate::icons::button(ui, "sliders-horizontal", 22.0, false, tl!("Generation settings"));
        note(ui, "settings", &settings);
        egui::Popup::from_toggle_button_response(&settings).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(340.0_f32.min(ui.ctx().content_rect().width() - 32.0));
            let height = (ui.ctx().content_rect().height() - 80.0).max(80.0);
            let action = egui::ScrollArea::vertical()
                .id_salt("generate-compact-settings-scroll")
                .max_height(height)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tl!("Images per run (1–4)"));
                        let mut count = app.ui.ai.generate.count as f32;
                        widgets::value_field(ui, &mut count, 1.0..=4.0, "", 40.0);
                        app.ui.ai.generate.count = (count.round() as u32).clamp(1, 4);
                    });
                    settings_ui_body(app, ui)
                })
                .inner;
            if action.enter && action.can {
                start(app, ui.ctx());
            }
        });
        if has_results {
            let results = compact_button(ui, tl!("View"), results_width).on_hover_text(tl!("View generation results"));
            results.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tl!("View generation results")));
            note(ui, "results", &results);
            egui::Popup::from_toggle_button_response(&results).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_width(280.0);
                egui::ScrollArea::vertical().id_salt("generate-compact-results-scroll").max_height(240.0).show(ui, |ui| results_ui(app, ui));
            });
        }
        if !running.is_empty() {
            let response = ui
                .add(
                    egui::ProgressBar::new(running.iter().map(|job| job.progress).sum::<f32>() / running.len() as f32)
                        .show_percentage()
                        .desired_width(38.0)
                        .desired_height(18.0),
                )
                .on_hover_text(running.iter().map(|job| format!("{}: {}", job.label, job.message)).collect::<Vec<_>>().join("\n"));
            note(ui, "progress", &response);
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    });
}

#[cfg(test)]
#[path = "panel_controls_tests.rs"]
mod tests;
