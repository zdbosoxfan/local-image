//! local-image: AI in Develop — the Remove tool's AI mode (engine, brush or lasso, a develop
//! layer's area, the selected AI removal's Regenerate). The
//! work runs as background jobs in the engine (`lightcraft_engine::enhance`); these panels start,
//! watch and cancel them.

use egui::RichText;
use lightcraft_develop::{DevelopSettings, Spot};
use lightcraft_engine::enhance::{JobKind, PatchState, patch_state};
use serde_json::json;

use crate::LightcraftApp;
use crate::theme::Tokens;
use crate::widgets::{register, text_button};

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 4 }).show(ui, add);
}

fn dim(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(li_i18n::library::t(text)).color(t.text_dim).size(11.5));
}

/// A progress bar with its Cancel button (cancels job `job`).
fn progress(app: &mut LightcraftApp, ui: &mut egui::Ui, id: &str, label: &str, frac: f32, job: u64) {
    ui.horizontal(|ui| {
        let bar = ui.add(egui::ProgressBar::new(frac).desired_width(150.0).text(format!("{} {:.0}%", li_i18n::library::t(label), frac * 100.0)));
        register(ui.ctx(), format!("progress:{id}"), bar.rect);
        if text_button(ui, &format!("{id}Cancel"), "Cancel", false).clicked() {
            let _ = app.run("enhance.cancel", json!({"job": job}));
        }
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
}

// AI Remove

/// Whether `sp` is a content-aware heal (made on this computer) rather than an AI removal.
pub fn is_local(sp: &Spot) -> bool {
    sp.patch.as_ref().is_some_and(|p| p.engine == lightcraft_engine::enhance::remove::LOCAL)
}

/// Progress (and Cancel) of the active photo's removals and heals being made.
pub fn remove_jobs(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let Some(id) = app.session.active() else { return };
    let running: Vec<(u64, f32, String)> =
        app.session.enhance.running_for(id).map(|j| (j.id, j.ctl.fraction().unwrap_or(0.0), j.label.clone())).collect();
    if running.is_empty() {
        return;
    }
    padded(ui, |ui| {
        for (job, frac, label) in running {
            progress(app, ui, &format!("removeJob{job}"), &label, frac, job);
        }
    });
}

/// The Remove panel's AI mode: what is painted and waiting (Remove / Cancel), engine, brush or
/// lasso, a layer's area, running removals.
pub fn remove_controls(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let Some(id) = app.session.active() else { return };
    let engines = app.session.enhance.host.as_ref().map(|h| h.remove_engines()).unwrap_or_default();
    let draft = app.ui.remove_draft.as_ref().is_some_and(|dr| dr.photo == id.0);
    padded(ui, |ui| {
        if draft {
            dim(ui, "Paint more to add to it (⌥ takes away), then remove it all at once.");
            ui.horizontal(|ui| {
                if text_button(ui, "removePanelApply", "Remove", true).on_hover_text(li_i18n::library::t("Remove what you painted (Enter)")).clicked()
                {
                    super::detail::apply_remove_draft(app, ui.ctx());
                }
                if text_button(ui, "removePanelCancel", "Cancel", false)
                    .on_hover_text(li_i18n::library::t("Discard what you painted (Esc)"))
                    .clicked()
                {
                    app.ui.remove_draft = None;
                }
            });
            ui.add_space(6.0);
        }
        if engines.is_empty() {
            dim(ui, "AI Remove needs the AI engine. Set it up in Compositing › Local AI.");
            return;
        }
        if !engines.iter().any(|e| e.key == app.ui.remove_engine) {
            app.ui.remove_engine = engines.iter().find(|e| e.problem.is_none()).unwrap_or(&engines[0]).key.clone();
        }
        let current = engines.iter().find(|e| e.key == app.ui.remove_engine).cloned();
        ui.horizontal(|ui| {
            ui.label(li_i18n::library::t("Engine"));
            let r = egui::ComboBox::from_id_salt("removeAiEngine")
                .selected_text(current.as_ref().map(|e| e.label.clone()).unwrap_or_default())
                .show_ui(ui, |ui| {
                    for e in &engines {
                        ui.selectable_value(&mut app.ui.remove_engine, e.key.clone(), &e.label);
                    }
                });
            register(ui.ctx(), "button:removeAiEngine", r.response.rect);
        });
        if let Some(p) = current.and_then(|e| e.problem) {
            ui.label(RichText::new(p).color(Tokens::get(ui.ctx()).caution).size(11.5));
        }
        ui.horizontal(|ui| {
            if text_button(ui, "removeBrush", "Brush", !app.ui.remove_lasso).clicked() {
                app.ui.remove_lasso = false;
            }
            if text_button(ui, "removeLasso", "Lasso", app.ui.remove_lasso).clicked() {
                app.ui.remove_lasso = true;
            }
        });
        if !draft {
            dim(
                ui,
                if app.ui.remove_lasso {
                    "Draw around a distraction, then click Remove (Enter)."
                } else {
                    "Paint over a distraction, then click Remove (Enter)."
                },
            );
        }
        // a develop layer's area
        let masks: Vec<(u32, String)> = d.masks.iter().map(|m| (m.id, m.name.clone())).collect();
        if !masks.is_empty() {
            ui.horizontal(|ui| {
                let sel = app.session.active_mask.filter(|m| masks.iter().any(|x| x.0 == *m)).unwrap_or(masks[0].0);
                let mut pick = sel;
                let name = masks.iter().find(|x| x.0 == sel).map(|x| x.1.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("removeAiMask").selected_text(name).show_ui(ui, |ui| {
                    for (mid, n) in &masks {
                        ui.selectable_value(&mut pick, *mid, n);
                    }
                });
                if pick != sel {
                    app.session.active_mask = Some(pick);
                }
                if text_button(ui, "removeMaskArea", "Remove Layer Area", false).clicked() {
                    let r = app.run("spot.add", json!({"mode": "ai", "mask": pick, "engine": app.ui.remove_engine}));
                    if let Err(e) = r {
                        app.toast_error(ui.ctx(), e);
                    }
                }
            });
        }
        // removals being generated
        let running: Vec<(u64, f32, String)> =
            app.session.enhance.running_for(id).map(|j| (j.id, j.ctl.fraction().unwrap_or(0.0), j.label.clone())).collect();
        for (job, frac, label) in running {
            progress(app, ui, &format!("removeJob{job}"), &label, frac, job);
        }
    });
}

/// The selected AI removal: its engine, whether it still lines up, Regenerate.
pub fn spot_info(app: &mut LightcraftApp, ui: &mut egui::Ui, sp: &Spot) {
    let Some(id) = app.session.active() else { return };
    let state = patch_state(&app.session, id, sp);
    padded(ui, |ui| {
        let note = match state {
            PatchState::Ok => None,
            PatchState::Stale => Some("The photo's geometry changed since this was generated: Regenerate to match."),
            PatchState::Missing => Some("The generated pixels are missing from the library: Regenerate them."),
            PatchState::Foreign => Some("This removal was made on another photo and isn't applied here."),
        };
        if let Some(n) = note {
            ui.label(RichText::new(li_i18n::library::t(n)).color(Tokens::get(ui.ctx()).caution).size(11.5));
        }
        if is_local(sp) {
            dim(ui, "Content-aware heal, made on this computer");
        } else if let Some(p) = &sp.patch {
            let engine = app
                .session
                .enhance
                .host
                .as_ref()
                .and_then(|h| h.remove_engines().into_iter().find(|e| e.key == p.engine))
                .map(|e| e.label)
                .unwrap_or(p.engine.clone());
            dim(ui, &engine);
        }
        let busy = app.session.enhance.running_for(id).any(|j| matches!(j.kind, JobKind::Regenerate { .. }));
        ui.horizontal(|ui| {
            if text_button(ui, "spotRegenerate", "Regenerate", false).clicked() && !busy {
                let r = app.run("spot.regenerate", json!({}));
                if let Err(e) = r {
                    app.toast_error(ui.ctx(), e);
                }
            }
            if text_button(ui, "spotDelete", "Delete (⌫)", false).clicked() {
                let _ = app.run("spot.delete", json!({}));
            }
        });
    });
}

/// Each frame: apply finished AI jobs (one undo step each), report failures, keep repainting
/// while jobs run.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let p = app.session.enhance_poll();
    if p.changed {
        ctx.request_repaint();
    }
    if let Some(e) = p.errors.into_iter().last() {
        app.toast_error(ctx, e);
    } else if let Some(done) = p.done.iter().rfind(|l| *l != lightcraft_engine::enhance::remove::HEAL_LABEL) {
        // (a heal shows on the photo when it is done: no toast for each)
        app.toast(ctx, format!("{} ✓", li_i18n::library::t(done)));
    }
    if app.session.enhance.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// Source-stage AI denoise. A quick proxy and a full result are distinct undoable operations.
pub fn denoise_controls(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    use lightcraft_develop::DenoiseModel;
    let Some(id) = app.session.active() else {
        return;
    };
    padded(ui, |ui| {
        ui.label(RichText::new("AI Denoise").strong());
        let mut model = d.enhance.denoise_model;
        ui.horizontal(|ui| {
            ui.label("Model");
            let r = egui::ComboBox::from_id_salt("denoiseModel")
                .selected_text(match model {
                    DenoiseModel::Auto => "Auto",
                    DenoiseModel::Raw => "Raw",
                    DenoiseModel::Rgb => "RGB",
                })
                .show_ui(ui, |ui| {
                    for (m, label) in [(DenoiseModel::Auto, "Auto"), (DenoiseModel::Raw, "Raw"), (DenoiseModel::Rgb, "RGB")] {
                        let r = ui.selectable_value(&mut model, m, label);
                        register(ui.ctx(), format!("button:denoiseModel{label}"), r.rect);
                    }
                });
            register(ui.ctx(), "button:denoiseModel", r.response.rect);
        });
        if model != d.enhance.denoise_model {
            let _ = app.run("develop.merge", json!({"settings":{"enhance":{"denoise_model":model}},"label":"AI Denoise model"}));
        }
        super::edit::control(app, ui, d, "enhance.denoise", true);
        let jobs: Vec<_> = app
            .session
            .enhance
            .running_for(id)
            .filter(|j| matches!(j.kind, JobKind::Denoise | JobKind::DenoiseExport))
            .map(|j| (j.id, j.ctl.fraction().unwrap_or(0.0), j.label.clone()))
            .collect();
        if !jobs.is_empty() {
            for (job, frac, label) in jobs {
                progress(app, ui, &format!("denoiseJob{job}"), &label, frac, job);
            }
            return;
        }
        if d.enhance.ai.is_some_and(|a| !lightcraft_engine::enhance::denoise::matches_decoding(&a, d)) {
            dim(ui, "The Raw Processing or camera profile changed after AI Denoise ran: run it again to apply it.");
        } else if d.enhance.ai.is_some_and(|a| a.preview) {
            dim(ui, "Preview: a 1024 px area at full resolution. Apply AI Denoise to the whole photo before exporting.");
        }
        ui.horizontal(|ui| {
            for (preview, key, label) in [(true, "denoisePreview", "Preview"), (false, "denoiseApply", "Apply AI Denoise")] {
                if text_button(ui, key, label, false).clicked() {
                    let center = if preview { preview_center(app, id, d) } else { None };
                    if let Err(e) = app.run("enhance.denoise", json!({"preview": preview, "center": center})) {
                        app.toast_error(ui.ctx(), e);
                    }
                }
            }
        });
        if text_button(ui, "denoiseNewFile", "Denoise to new DNG/TIFF…", false).clicked() {
            let name = app
                .session
                .catalog
                .photo(id)
                .map(|p| format!("{}-Denoised.dng", p.file_name.rsplit_once('.').map_or(p.file_name.as_str(), |(s, _)| s)))
                .unwrap_or_else(|| "Denoised.dng".into());
            if app.services.file_dialog.is_none() {
                app.toast(ui.ctx(), "Saving a denoised file is unavailable in this host.");
            }
            let ctx = ui.ctx().clone();
            let r = app.ask_file(crate::FileDialogRequest::SaveDenoised { name }, move |app, answer| {
                if let Some(path) = answer.into_path()
                    && let Err(e) = app.run("enhance.denoise", json!({"path":path}))
                {
                    app.toast_error(&ctx, e);
                }
                Ok(serde_json::Value::Null)
            });
            if let Err(e) = r {
                app.toast_error(ui.ctx(), e);
            }
        }
        let family = if model == DenoiseModel::Rgb || (model == DenoiseModel::Auto && !app.session.source_info(id).raw) {
            "denoise-nind"
        } else {
            "rawdenoise-nind"
        };
        denoise_download(app, ui, family);
    });
}

/// Where a Preview denoises: the middle of what the loupe shows, in the photo's decoded source
/// (normalized), through its crop, straightening, lens and perspective corrections and orientation.
fn preview_center(app: &LightcraftApp, id: lightcraft_catalog::PhotoId, d: &DevelopSettings) -> Option<[f64; 2]> {
    let p = app.session.catalog.photo(id)?;
    let lens_db = lightcraft_engine::lens_db::for_photo(p, d);
    let frame = lightcraft_pipeline::geometry::Frame::with_lenses(
        p.width.max(1) as usize,
        p.height.max(1) as usize,
        d,
        true,
        p.embedded_lens.as_ref(),
        lens_db.as_ref(),
    );
    Some(shown_to_source(&frame, view_center(app)))
}

/// The middle of the loupe's view, normalized in the developed photo as shown (its centre when the
/// whole photo is in view).
fn view_center(app: &LightcraftApp) -> [f64; 2] {
    let (Some(img), Some(canvas)) = (app.image_rect, app.canvas_rect) else { return [0.5, 0.5] };
    let shown = img.intersect(canvas);
    if !shown.is_positive() {
        return [0.5, 0.5];
    }
    let c = shown.center();
    [f64::from((c.x - img.left()) / img.width()), f64::from((c.y - img.top()) / img.height())]
}

/// `out` (normalized in `frame`'s output) as a normalized point of the source `frame` samples.
fn shown_to_source(frame: &lightcraft_pipeline::geometry::Frame, out: [f64; 2]) -> [f64; 2] {
    use lightcraft_geom::Point;
    let (w, h) = frame.native_size();
    let (w, h) = (w.round().max(1.0) as usize, h.round().max(1.0) as usize);
    let p = frame.out_to_oriented(w, h).apply(Point::new(out[0] * w as f64, out[1] * h as f64));
    let p = frame.warp.as_ref().map_or(p, |warp| warp.to_source(p, 1));
    let s = lightcraft_pipeline::redeye::oriented_to_source(frame.orient, Point::new(p.x / frame.ow, p.y / frame.oh));
    [s.x, s.y]
}

pub(crate) fn denoise_download(app: &mut LightcraftApp, ui: &mut egui::Ui, family: &str) {
    let status = app.services.model_download_status.as_ref().map(|f| f(family)).unwrap_or_default();
    let installed = app.services.model_installed.as_ref().is_some_and(|f| f(family));
    if status.running {
        let progress = (status.done as f32 / status.total.max(1) as f32).clamp(0., 1.);
        let response = ui.add(egui::ProgressBar::new(progress).show_percentage());
        register(ui.ctx(), "progress:denoiseDownload", response.rect);
        if text_button(ui, "denoiseDownloadCancel", "Cancel download", false).clicked()
            && let Some(download) = &app.services.download_models
            && let Err(error) = download(family, true)
        {
            app.toast_error(ui.ctx(), error);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    } else if installed {
        let response = ui.label(li_i18n::library::t("AI Denoise model ready"));
        register(ui.ctx(), "status:denoiseReady", response.rect);
    } else {
        if let Some(error) = status.error.as_ref() {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).caution, error);
        }
        let label = if status.error.is_some() { "Retry AI Denoise download" } else { "Download AI Denoise Model" };
        if text_button(ui, "denoiseDownload", label, false).clicked() {
            if let Some(download) = &app.services.download_models {
                if let Err(error) = download(family, false) {
                    app.toast_error(ui.ctx(), error);
                }
            } else {
                app.toast(ui.ctx(), "Model downloads are available in Settings › Local AI.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    use crate::headless::Headless;
    use crate::{LightcraftApp, Services};

    /// A cropped, perspective-corrected, turned photo seen at 100 %: Preview denoises where the
    /// view looks, not at the photo's centre.
    #[test]
    fn ai_denoise_preview_follows_the_view_through_crop_and_perspective() {
        let mut session = lightcraft_engine::Session::with_demo();
        session.enhance.denoiser = Some(Arc::new(|pixels, _, _, _| Ok(vec![[0.9, 0.02, 0.9]; pixels.len()])));
        let id = session.active().unwrap();
        let mut d = (*session.develop_of(id).unwrap()).clone();
        d.crop.geometry.rect = lightcraft_geom::Rect::new(0.1, 0.15, 0.85, 0.9);
        d.geometry.vertical = 30.0;
        d.orientation = lightcraft_geom::Orientation::Rotate90;
        session.set_develop(id, d, "Framing").unwrap();
        let mut h = Headless::new(LightcraftApp::new(session, Services { png: None, ..Default::default() }), [1200., 1800.], 1.);
        let t = Duration::from_secs(20);
        assert_eq!(h.request("ui.set", json!({"view":"detail","right":"edit","leftPanel":false}), t)["ok"], true);
        for key in ["light", "color", "effects", "optics", "raw", "geometry"] {
            if h.app.ui.section_open(key) {
                h.app.ui.toggle_section(key);
            }
        }
        if !h.app.ui.section_open("detail") {
            h.app.ui.toggle_section("detail");
        }
        let viewed = (0.2, 0.75);
        h.app.ui.zoom = crate::state::Zoom::Percent(100);
        h.app.ui.pan = viewed;
        h.step();
        h.step();
        // (red + blue − 2 green) at a point of the developed photo: how magenta it is
        let magenta = |h: &mut Headless, x: f32, y: f32| {
            let out = h.app.session.render_now(id, 900, 900).unwrap().image;
            let p = out.get((x * out.width as f32) as usize, (y * out.height as f32) as usize).map(i32::from);
            p[0] + p[2] - 2 * p[1]
        };
        let away = (1.0 - viewed.0, 1.0 - viewed.1);
        let before = [magenta(&mut h, viewed.0, viewed.1), magenta(&mut h, away.0, away.1)];
        assert_eq!(h.request("ui.clickWidget", json!({"id": "button:denoisePreview"}), t)["ok"], true);
        assert!(h.step_until(Duration::from_secs(300), |h| h.app.session.develop_of(id).unwrap().enhance.ai.is_some_and(|a| a.preview)));
        let seen = magenta(&mut h, viewed.0, viewed.1);
        assert!(seen > before[0] + 100, "the viewed point takes the preview: {seen} after {}", before[0]);
        assert_eq!(magenta(&mut h, away.0, away.1), before[1], "only the area around it");
    }
}
