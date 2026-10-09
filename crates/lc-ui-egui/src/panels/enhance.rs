//! local-image: AI in Develop — the Remove tool's AI mode (engine, brush or lasso, a develop
//! layer's area, the selected AI removal's Regenerate) and AI Denoise in the Detail section. The
//! work runs as background jobs in the engine (`lightcraft_engine::enhance`); these panels start,
//! watch and cancel them.

use egui::RichText;
use lightcraft_develop::{DevelopSettings, Spot};
use lightcraft_engine::enhance::{DenoiseState, JobKind, PatchState, denoise_state, patch_state};
use serde_json::json;

use crate::LightcraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::widgets::{register, text_button};

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 4, bottom: 4 }).show(ui, add);
}

fn dim(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tr(text)).color(t.text_dim).size(11.5));
}

/// A progress bar with its Cancel button (cancels job `job`).
fn progress(app: &mut LightcraftApp, ui: &mut egui::Ui, id: &str, label: &str, frac: f32, job: Option<u64>) {
    ui.horizontal(|ui| {
        let bar = ui.add(egui::ProgressBar::new(frac).desired_width(150.0).text(format!("{} {:.0}%", tr(label), frac * 100.0)));
        register(ui.ctx(), format!("progress:{id}"), bar.rect);
        if text_button(ui, &format!("{id}Cancel"), "Cancel", false).clicked() {
            let _ = match job {
                Some(j) => app.run("enhance.cancel", json!({"job": j})),
                None => app.run("enhance.downloadModel", json!({"cancel": true})),
            };
        }
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
}

// ------------------------------------------------------------------------------ AI Denoise

/// The AI Denoise part of the photo's Detail section.
pub fn denoise_section(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let Some(id) = app.session.active() else { return };
    super::edit::sub_title(ui, tr("AI Denoise"));
    let state = denoise_state(&app.session, id);
    if let DenoiseState::Done { .. } = state {
        super::edit::control_t(app, ui, d, "enhance.denoise", true, super::edit::Target::Global);
    }
    padded(ui, |ui| match state {
        DenoiseState::Unavailable { why } => dim(ui, &why),
        DenoiseState::NoModel { download } => match download {
            Some(dl) if dl.running => {
                let frac = if dl.total > 0 { dl.done as f32 / dl.total as f32 } else { 0.0 };
                progress(app, ui, "denoiseDownload", "Downloading", frac, None);
            }
            dl => {
                dim(ui, "AI Denoise needs its model (58 MB download, runs on this computer).");
                if let Some(e) = dl.and_then(|d| d.error) {
                    ui.label(RichText::new(e).color(Tokens::get(ui.ctx()).reject).size(11.5));
                }
                if text_button(ui, "denoiseDownload", "Download Model", false).clicked()
                    && let Err(e) = app.run("enhance.downloadModel", json!({}))
                {
                    app.toast_error(ui.ctx(), e);
                }
            }
        },
        DenoiseState::Ready => {
            dim(ui, "Removes noise with an AI model, on this computer (a few minutes for a large raw).");
            if text_button(ui, "denoiseRun", "Denoise", false).clicked()
                && let Err(e) = app.run("enhance.denoise", json!({}))
            {
                app.toast_error(ui.ctx(), e);
            }
        }
        DenoiseState::Running { progress: p, job } => progress(app, ui, "denoise", "Denoising", p, Some(job)),
        DenoiseState::Done { .. } => {}
        DenoiseState::Missing => {
            dim(ui, "The denoised result is missing from the library. Run Denoise again to use it.");
            if text_button(ui, "denoiseRun", "Run Denoise Again", false).clicked()
                && let Err(e) = app.run("enhance.denoise", json!({}))
            {
                app.toast_error(ui.ctx(), e);
            }
        }
    });
}

// ------------------------------------------------------------------------------ AI Remove

/// The Remove panel's AI mode: engine, brush or lasso, a layer's area, running removals.
pub fn remove_controls(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings) {
    let Some(id) = app.session.active() else { return };
    let engines = app.session.enhance.host.as_ref().map(|h| h.remove_engines()).unwrap_or_default();
    padded(ui, |ui| {
        if engines.is_empty() {
            dim(ui, "AI Remove needs the AI engine. Set it up in Compositing › Local AI.");
            return;
        }
        if !engines.iter().any(|e| e.key == app.ui.remove_engine) {
            app.ui.remove_engine = engines.iter().find(|e| e.problem.is_none()).unwrap_or(&engines[0]).key.clone();
        }
        let current = engines.iter().find(|e| e.key == app.ui.remove_engine).cloned();
        ui.horizontal(|ui| {
            ui.label(tr("Engine"));
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
        dim(ui, if app.ui.remove_lasso { "Draw around a distraction to remove it." } else { "Paint over a distraction to remove it." });
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
            app.session.enhance.running_for(id).filter(|j| j.kind != JobKind::Denoise).map(|j| (j.id, j.ctl.progress(), j.label.clone())).collect();
        for (job, frac, label) in running {
            progress(app, ui, &format!("removeJob{job}"), &label, frac, Some(job));
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
            ui.label(RichText::new(tr(n)).color(Tokens::get(ui.ctx()).caution).size(11.5));
        }
        if let Some(p) = &sp.patch {
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
    } else if let Some(done) = p.done.last() {
        app.toast(ctx, format!("{} ✓", tr(done)));
    }
    if app.session.enhance.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
