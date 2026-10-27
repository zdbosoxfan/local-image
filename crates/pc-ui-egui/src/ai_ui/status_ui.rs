//! AI status and job controls shared by the application status bar.

use super::*;

// Status pill

/// The AI engine status in the status bar (right-to-left layout); click opens Local AI.
pub fn status_pill(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if let Ok(mut c) = shared().ctx.lock()
        && c.is_none()
    {
        *c = Some(ui.ctx().clone());
    }
    let t = Tokens::get(ui.ctx());
    let st = status();
    let (dot, text) = if !st.checked {
        (t.text_faint, tl!("AI").to_owned())
    } else if st.starting {
        (t.warning, tl!("AI starting…").to_owned())
    } else if !st.connected {
        (t.text_faint, tl!("AI off").to_owned())
    } else if !st.any_ready() {
        (t.warning, tl!("AI · no models").to_owned())
    } else {
        let gpu = st.gpu.as_ref().map(|g| {
            let used = g.vram_used.map(|u| format!(" · {:.0}/{:.0} GB", u as f64 / 1e9, g.vram_total as f64 / 1e9)).unwrap_or_default();
            format!(" · {}{used}", short_gpu(&g.name))
        });
        (t.accent, li_i18n::fmt(tl!("AI ready{gpu}"), &[("gpu", &gpu.unwrap_or_default())]))
    };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(11.5), t.text_dim);
    let size = vec2(galley.size().x + 26.0, 18.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 9.0, t.hover);
    }
    ui.painter().circle_filled(
        egui::pos2(r.left() + 10.0, r.center().y),
        3.5,
        if resp.hovered() && dot == t.text_faint { t.text_secondary_on_fill } else { dot },
    );
    ui.painter().galley(egui::pos2(r.left() + 18.0, r.center().y - galley.size().y / 2.0), galley, t.text_dim);
    let mut tip = if st.connected {
        li_i18n::fmt(tl!("ComfyUI at {host} — click for Local AI settings"), &[("host", &st.host)])
    } else {
        tl!("Local AI is off — click to set it up").into()
    };
    if st.connected && !st.setup.new.is_empty() {
        tip.push('\n');
        tip.push_str(&li_i18n::trn(
            li_i18n::current(),
            st.setup.new.len() as u64,
            "{n} new model found: Window › Set Up Models…",
            "{n} new models found: Window › Set Up Models…",
        ));
    }
    let resp = resp.on_hover_text(tip);
    if resp.clicked() {
        open_local_ai(app);
    }
    if !st.checked || st.starting {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

/// The status bar's AI controls, drawn just left of [`status_pill`] (right-to-left layout):
/// a red Stop while AI jobs run, and Eject (unload the models from GPU memory) while the
/// engine is connected.
pub fn status_controls(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    status_controls_with(app, ui, &status());
}

/// AI jobs running in the session (Generate, AI Remove, Enhance, …).
pub fn ai_jobs(app: &PhotocraftApp) -> Vec<photocraft_engine::jobs::JobId> {
    app.session.jobs().into_iter().filter(|j| j.command.starts_with("ai.")).map(|j| j.id).collect()
}

/// A small status-bar icon button with an accessible name; disabled ones are faint and inert.
fn bar_icon(ui: &mut egui::Ui, icon: &str, tint: Color32, enabled: bool, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(20.0, 18.0), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    crate::icons::paint(ui, r, icon, 13.0, if enabled { tint } else { t.text_faint });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

pub(crate) fn status_controls_with(app: &mut PhotocraftApp, ui: &mut egui::Ui, st: &EngineStatus) {
    let t = Tokens::get(ui.ctx());
    let jobs = ai_jobs(app);
    if !jobs.is_empty() && bar_icon(ui, "circle-stop", t.danger, true, tl!("Stop generation")).clicked() {
        stop_generation(app, &jobs);
    }
    let tip = if st.connected { tl!("Unload AI models from GPU memory") } else { tl!("Unload AI models from GPU memory (the AI engine is off)") };
    if bar_icon(ui, "eject", t.text_dim, st.connected, tip).clicked() {
        app.ui.status = tl!("Unloading AI models…").into();
        app.ui.status_error = false;
        let lang = li_i18n::current();
        let ctx = ui.ctx().clone();
        let _ = std::thread::Builder::new().name("ai-unload".into()).spawn(move || {
            li_i18n::set_current(lang);
            let note = match li_ai::service().client.free_memory() {
                Ok(()) => (tl!("Models unloaded.").to_owned(), false),
                Err(e) => (format!("{e:#}"), true),
            };
            if let Ok(mut n) = REMOVE_NOTE.lock() {
                *n = Some(note);
            }
            refresh();
            ctx.request_repaint();
        });
    }
}

/// Cancel only the requested jobs. Each worker's bridged control owns its ComfyUI
/// prompt and handles targeted cancellation/retry; a process-wide prompt snapshot
/// could interrupt another feature's job.
pub fn stop_generation(app: &mut PhotocraftApp, jobs: &[photocraft_engine::jobs::JobId]) {
    for id in jobs {
        crate::jobs_ui::cancel(app, *id);
    }
    app.ui.status = tl!("Stopping generation…").into();
    app.ui.status_error = false;
}

pub(crate) fn short_gpu(name: &str) -> String {
    name.replace("NVIDIA ", "").replace("GeForce ", "").replace("Laptop GPU", "Laptop")
}
