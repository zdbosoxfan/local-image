//! Result tiles and the Library tab: the tile grid, tile actions and the library listing.
use super::*;

pub(super) fn results_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let n = rt(|r| r.results.len());
    if n == 0 {
        ui.label(RichText::new(tl!("Results appear here and are saved to the Library.")).color(t.text_faint).size(11.5));
        return;
    }
    widgets::section_label(ui, tl!("RESULTS"));
    let items: Vec<Option<Entry>> = rt(|r| r.results.iter().map(|t| if let Tile::Done(e) = t { Some(e.clone()) } else { None }).collect());
    let pendings: Vec<String> = rt(|r| {
        r.results
            .iter()
            .map(|t| match t {
                Tile::Pending { label, job, .. } => {
                    let p = app.session.job(photocraft_engine::jobs::JobId(*job)).map(|j| format!("{:.0}%", j.progress * 100.0)).unwrap_or_default();
                    format!("{label}\n{p}")
                }
                Tile::Failed(e) => e.clone(),
                Tile::Done(_) => String::new(),
            })
            .collect()
    });
    tile_grid(app, ui, &items, &pendings, "results");
    if pendings.iter().zip(&items).any(|(p, i)| i.is_none() && !p.is_empty()) {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

/// A grid of square tiles; `None` items draw `pending[i]` as text.
pub(super) fn tile_grid(app: &mut PhotocraftApp, ui: &mut egui::Ui, items: &[Option<Entry>], pending: &[String], salt: &str) {
    let t = Tokens::get(ui.ctx());
    let lib = library();
    let cols = ((ui.available_width() + 6.0) / 96.0).floor().max(2.0) as usize;
    let size = ((ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32).floor();
    let mut action: Option<(Entry, &'static str)> = None;
    egui::Grid::new(format!("tiles-{salt}")).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, item) in items.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            ui.painter().rect_filled(rect, t.radius, t.field);
            match item {
                Some(e) => {
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, if e.prompt().is_empty() { &e.name } else { e.prompt() }));
                    if let Some(tex) = thumb_texture(ui.ctx(), &e.id, &lib.thumbnail_path(&e.id)) {
                        let sz = tex.size_vec2();
                        let k = (size / sz.x).min(size / sz.y);
                        let r = egui::Rect::from_center_size(rect.center(), sz * k);
                        widgets::checker(ui.painter(), r, 6.0);
                        ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    let sel = rt(|r| r.selected.as_deref() == Some(e.id.as_str()));
                    if sel || resp.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            t.radius,
                            Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { t.accent } else { t.text_dim }),
                            StrokeKind::Inside,
                        );
                    }
                    let tip = format!(
                        "{}\n{} · {}×{}{}\n{}",
                        e.prompt(),
                        e.model(),
                        e.width,
                        e.height,
                        e.seed().map(|s| format!(" · {}", li_i18n::fmt(tl!("seed {seed}"), &[("seed", &s.to_string())]))).unwrap_or_default(),
                        tl!("Double-click to open")
                    );
                    let resp = resp.on_hover_text(tip);
                    if resp.clicked() {
                        rt(|r| r.selected = Some(e.id.clone()));
                    }
                    if resp.double_clicked() {
                        action = Some((e.clone(), "open"));
                    }
                    resp.context_menu(|ui| {
                        for (label, act) in [
                            (tl!("Open"), "open"),
                            (tl!("Place as Layer"), "place"),
                            (tl!("Use as Reference"), "reference"),
                            (tl!("Recreate"), "recreate"),
                            (tl!("Copy Prompt"), "copy"),
                        ] {
                            let enabled = act != "place" || app.session.active().is_some();
                            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                action = Some((e.clone(), act));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(tl!("Delete from Library")).clicked() {
                            action = Some((e.clone(), "delete"));
                            ui.close();
                        }
                    });
                }
                None => {
                    let text = pending.get(i).cloned().unwrap_or_default();
                    let busy = !text.starts_with(tl!("Cancelled")) && text.contains('\n');
                    if busy {
                        let phase = (ui.input(|i| i.time) * 2.0).sin() as f32 * 0.5 + 0.5;
                        ui.painter().rect_filled(rect, t.radius, t.hover.gamma_multiply(0.6 + 0.4 * phase));
                    }
                    let galley = ui.painter().layout(text, egui::FontId::proportional(11.0), if busy { t.text_dim } else { t.warning }, size - 10.0);
                    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, t.text);
                }
            }
            if (i + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
    if let Some(e) = rt(|r| r.selected.clone()).and_then(|id| items.iter().flatten().find(|e| e.id == id).cloned()) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let open = widgets::secondary_button(ui, tl!("Open"), 0.0);
            if salt == "results" {
                panel_controls::note(ui, "history-open", &open);
            }
            if open.clicked() {
                action = Some((e.clone(), "open"));
            }
            if app.session.active().is_some() {
                let insert = widgets::secondary_button(ui, tl!("Place as Layer"), 0.0);
                if salt == "results" {
                    panel_controls::note(ui, "history-insert", &insert);
                }
                if insert.clicked() {
                    action = Some((e.clone(), "place"));
                }
            }
            if widgets::secondary_button(ui, tl!("Use as Reference"), 0.0).clicked() {
                action = Some((e.clone(), "reference"));
            }
        });
    }
    if let Some((e, act)) = action {
        entry_action(app, ui.ctx(), &e, act);
    }
}

pub(super) fn entry_action(app: &mut PhotocraftApp, ctx: &egui::Context, e: &Entry, act: &str) {
    let lib = library();
    match act {
        "open" => match photocraft_engine::batch_io::read_file(&lib.image_path(&e.id)) {
            Ok(bytes) => {
                if let Err(err) = app.open_bytes(&e.name, &bytes) {
                    app.ui.status = err;
                    app.ui.status_error = true;
                }
            }
            Err(err) => {
                app.ui.status = li_i18n::fmt(tl!("Could not open {name}: {error}"), &[("name", &e.name), ("error", &err)]);
                app.ui.status_error = true;
            }
        },
        "place" => {
            let r = app.run(
                "ai.placeLayer",
                json!({ "path": lib.image_path(&e.id).display().to_string(), "name": e.name.trim_end_matches(".png"), "fit": "contain", "generation": e.generation }),
            );
            if let Err(err) = r {
                app.ui.status = err;
                app.ui.status_error = true;
            }
        }
        "reference" => match lib.load(e) {
            Ok(img) => {
                app.ui.ai.generate.mode = Mode::Create;
                add_reference(ctx, e.name.clone(), img);
            }
            Err(err) => {
                app.ui.status = format!("{err:#}");
                app.ui.status_error = true;
            }
        },
        "recreate" => {
            if let Some(g) = &e.generation {
                let s = &mut app.ui.ai.generate;
                match g.get("model").and_then(Value::as_str) {
                    Some(k) if k.starts_with("custom:") => s.model = k.to_owned(),
                    Some(k) => {
                        if let Some(m) = ModelId::from_key(k) {
                            set_model(s, m);
                        }
                    }
                    None => {}
                }
                if let Some(n) = g.get("negative_prompt").and_then(Value::as_str) {
                    s.negative = n.to_owned();
                }
                s.prompt = e.prompt().to_owned();
                if let Some(v) = g.get("variant").and_then(Value::as_str) {
                    s.variant = v.to_owned();
                }
                s.width = e.width;
                s.height = e.height;
                s.aspect = "custom".into();
                s.mode = Mode::Create;
                s.seed = None;
                app.ui.dock_tabs.generate = 0;
            }
        }
        "copy" => ctx.copy_text(e.prompt().to_owned()),
        "delete" => {
            let _ = lib.delete(std::slice::from_ref(&e.id));
            rt(|r| {
                r.results.retain(|t| !matches!(t, Tile::Done(x) if x.id == e.id));
                Arc::make_mut(&mut r.library).retain(|x| x.id != e.id);
                r.gallery_thumbs.remove(&e.id);
            });
        }
        _ => {}
    }
}

/// The Library tab: every generated image, newest first.
pub fn library_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    if rt(|r| r.library_loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(10))) {
        let list = library().list();
        rt(|r| {
            r.library = Arc::new(list);
            r.library_loaded = Some(Instant::now());
        });
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.ui.ai.generate.library_filter).hint_text(tl!("Search prompts")).desired_width(ui.available_width() - 30.0));
        if crate::icons::button(ui, "rotate-cw", 22.0, false, tl!("Refresh")).clicked() {
            rt(|r| r.library_loaded = None);
        }
    });
    let filter = app.ui.ai.generate.library_filter.to_lowercase();
    let items: Vec<Option<Entry>> = rt(|r| {
        r.library
            .iter()
            .filter(|e| filter.is_empty() || e.prompt().to_lowercase().contains(&filter) || e.name.to_lowercase().contains(&filter))
            .take(200)
            .cloned()
            .map(Some)
            .collect()
    });
    if items.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new(if filter.is_empty() { tl!("Generated images are kept here.") } else { tl!("Nothing matches.") }).color(t.text_faint));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        tile_grid(app, ui, &items, &[], "library");
    });
}
