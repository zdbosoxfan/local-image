//! Additive function rows: uses the same frames, downloads and custom-model picker as masks.
use super::*;
use li_ai::enhance::{
    Method,
    selection::{self, Choice},
};

#[derive(Clone, Default)]
struct RowState {
    open: bool,
    custom: bool,
    busy: bool,
    error: String,
    unet: String,
    vae: String,
}
static ROWS: Mutex<BTreeMap<String, RowState>> = Mutex::new(BTreeMap::new());
fn state(key: &str) -> RowState {
    ROWS.lock().unwrap_or_else(|e| e.into_inner()).entry(key.into()).or_default().clone()
}
fn update(key: &str, f: impl FnOnce(&mut RowState)) {
    f(ROWS.lock().unwrap_or_else(|e| e.into_inner()).entry(key.into()).or_default());
}

pub(super) fn title(method: Method) -> &'static str {
    match method {
        Method::Faithful => "Upscale Faithful",
        Method::Rescue => "Upscale Rescue",
        Method::Generative => "Generative",
    }
}

pub(super) fn selected_spec(dir: &std::path::Path, method: Method) -> Option<&'static li_seg::ModelSpec> {
    match selection::load(dir, method) {
        Choice::Curated { id } => li_seg::spec(&id),
        _ => None,
    }
}

pub(super) fn row(ui: &mut egui::Ui, t: &Tokens, dls: &BTreeMap<String, Download>, dir: &std::path::Path, method: Method) {
    let key = format!("{}:{}", dir.display(), method.key());
    let choice = selection::load(dir, method);
    let default = selection::default_choice(method);
    let mut s = state(&key);
    let previous_error = s.error.clone();
    egui::Frame::NONE.fill(t.field).corner_radius(t.radius).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tl!(title(method))).size(13.0).strong().color(t.text));
        ui.label(RichText::new(tl!(method.about())).color(t.text_dim).size(11.5));
        ui.horizontal(|ui| {
            let name = match &choice {
                Choice::Curated { id } => {
                    li_seg::spec(id).map_or(id.as_str(), |spec| spec.label.split_once(" · ").map_or(spec.label, |(_, name)| name)).to_owned()
                }
                Choice::Custom { name, .. } => crate::i18n::fmt(tl!("Custom: {name}"), &[("name", name)]),
                Choice::Seed { .. } if choice == default => "SeedVR2 7B FP16".into(),
                Choice::Seed { unet, .. } => crate::i18n::fmt(tl!("Custom: {name}"), &[("name", unet)]),
            };
            ui.label(RichText::new(name).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::secondary_button(ui, tl!("Change…"), 0.0).clicked() {
                    s.open = !s.open;
                }
                if ui.add_enabled(choice != default, egui::Button::new(tl!("Use default"))).clicked() {
                    s.error = selection::save(dir, method, None).err().map(|e| format!("{e:#}")).unwrap_or_default();
                    s.custom = false;
                }
            });
        });
        match &choice {
            Choice::Curated { id } => {
                if let Some(spec) = li_seg::spec(id) {
                    ui.label(
                        RichText::new(format!(
                            "{} CPU · {} GPU · {}",
                            li_ai::download::human_bytes(spec.bytes),
                            catalog::preset(ModelId::intern(id), "default")
                                .map_or_else(|| "?".into(), |p| li_ai::download::human_bytes(p.files.iter().map(|f| f.bytes).sum())),
                            spec.licence
                        ))
                        .color(t.text_faint)
                        .size(11.0),
                    );
                    download_controls(ui, dls, dir, spec);
                }
            }
            Choice::Custom { bytes, scale, .. } => {
                ui.label(
                    RichText::new(format!(
                        "{} · {scale}× · {}",
                        li_ai::download::human_bytes(*bytes),
                        tl!("Your own model, run on the CPU instead of the official one.")
                    ))
                    .color(t.text_faint)
                    .size(11.0),
                );
            }
            Choice::Seed { .. } => {
                let Some(preset) = catalog::preset(ModelId::SeedVr2, "fp16") else { return };
                let details = if choice == default {
                    format!("{} · GPU · Apache-2.0", li_ai::download::human_bytes(preset.files.iter().map(|f| f.bytes).sum()))
                } else {
                    tl!("Your own models, run on the GPU.").into()
                };
                ui.label(RichText::new(details).color(t.text_faint).size(11.0));
                let download_key = preset.id();
                if let Some(d) = dls.get(&download_key).filter(|d| !d.finished) {
                    ui.add(egui::ProgressBar::new(d.done as f32 / d.total.max(1) as f32));
                    if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                        d.ctl.cancel();
                    }
                } else if status().ready(ModelId::SeedVr2, "fp16").is_ok() {
                    ui.label(tl!("Ready"));
                } else if widgets::secondary_button(ui, tl!("Download GPU model"), 0.0).clicked() {
                    start_download(&download_key);
                }
            }
        }
        if s.open {
            if method != Method::Generative {
                for spec in li_seg::MODELS.iter().filter(|m| m.task == li_seg::Task::Upscale) {
                    let candidate = Choice::Curated { id: spec.id.into() };
                    let name = spec.label.split_once(" · ").map_or(spec.label, |(_, name)| name);
                    if ui.selectable_label(choice == candidate, name).clicked() {
                        s.error = selection::save(dir, method, Some(candidate)).err().map(|e| format!("{e:#}")).unwrap_or_default();
                    }
                }
            } else if ui.selectable_label(choice == default, "SeedVR2 7B FP16").clicked() {
                s.error = selection::save(dir, method, None).err().map(|e| format!("{e:#}")).unwrap_or_default();
            }
            if widgets::secondary_button(ui, tl!("Custom model…"), 0.0).clicked() {
                s.custom = !s.custom;
            }
            if s.custom && method != Method::Generative {
                ui.label(tl!("Choose a fixed-tile ONNX RGB model with 2× or 4× output. It is tested before use."));
                if s.busy {
                    ui.spinner();
                    ui.label(tl!("Testing the model…"));
                } else if widgets::secondary_button(ui, tl!("Choose an ONNX file…"), 0.0).clicked()
                    && let Some(source) = pick_onnx()
                {
                    s.busy = true;
                    let (dir, key) = (dir.to_owned(), key.clone());
                    update(&key, |s| {
                        s.busy = true;
                        s.error.clear();
                    });
                    let worker_key = key.clone();
                    let worker = std::thread::Builder::new().name("upscale-custom".into()).spawn(move || {
                        let result = selection::import_custom(&dir, method, &source);
                        update(&worker_key, |s| {
                            s.busy = false;
                            s.error = result.err().map(|e| format!("{e:#}")).unwrap_or_default();
                        });
                        refresh();
                        if let Some(ctx) = shared().ctx.lock().ok().and_then(|c| c.clone()) {
                            ctx.request_repaint();
                        }
                    });
                    if let Err(error) = worker {
                        update(&key, |s| {
                            s.busy = false;
                            s.error = error.to_string();
                        });
                    }
                }
            } else if s.custom {
                ui.label(tl!("Enter installed SeedVR2-compatible model and VAE filenames."));
                ui.text_edit_singleline(&mut s.unet).on_hover_text("UNET");
                ui.text_edit_singleline(&mut s.vae).on_hover_text("VAE");
                if widgets::secondary_button(ui, tl!("Use custom models"), 0.0).clicked() {
                    s.error = selection::save(dir, method, Some(Choice::Seed { unet: s.unet.clone(), vae: s.vae.clone() }))
                        .err()
                        .map(|e| format!("{e:#}"))
                        .unwrap_or_default();
                }
            }
        }
        if !s.error.is_empty() {
            ui.label(RichText::new(&s.error).color(t.danger));
        }
    });
    // Preserve a background validation result; only the UI changes the disclosure fields.
    update(&key, |state| {
        state.open = s.open;
        state.custom = s.custom;
        state.unet = s.unet;
        state.vae = s.vae;
        if s.error != previous_error {
            state.error = s.error;
        }
    });
    ui.add_space(4.0);
}

pub(super) fn download_controls(ui: &mut egui::Ui, dls: &BTreeMap<String, Download>, dir: &std::path::Path, spec: &'static li_seg::ModelSpec) {
    let cpu = format!("seg:{}", spec.id);
    let gpu = format!("{}:default", spec.id);
    for (key, label) in [(&cpu, "CPU"), (&gpu, "GPU")] {
        if let Some(d) = dls.get(key).filter(|d| !d.finished) {
            ui.label(label);
            ui.add(egui::ProgressBar::new(d.done as f32 / d.total.max(1) as f32));
            if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked() {
                d.ctl.cancel();
            }
        } else if key == &cpu {
            ui.horizontal(|ui| {
                if li_seg::installed_bytes(dir, spec).is_some() {
                    ui.label(tl!("Installed"));
                    if remove_button(ui) {
                        ask_remove(spec.label.into(), RemoveTarget::Seg(spec));
                    }
                } else if widgets::secondary_button(ui, tl!("Download CPU model"), 0.0).clicked() {
                    start_seg_download(spec);
                }
            });
        } else if status().ready(ModelId::intern(spec.id), "default").is_err() && widgets::secondary_button(ui, tl!("Download GPU model"), 0.0).clicked() {
            start_download(key);
        }
        if let Some(error) = dls.get(key).and_then(|d| d.error.as_ref()) {
            ui.label(RichText::new(error).color(Tokens::get(ui.ctx()).danger));
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn ds2_gpu_download_displays_progress_and_cancel() {
        let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = super::super::tests::temp_dir("ds2-gpu-download");
        let folder = dir.clone();
        let control = JobControl::new();
        let test_control = control.clone();
        let mut downloads = BTreeMap::new();
        downloads.insert("realplksr-2x:default".into(), Download { total: 100, done: 50, ctl: control, ..Default::default() });
        let mut initialized = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(600.0, 400.0)).build_ui(move |ui| {
            if !initialized {
                PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            download_controls(ui, &downloads, &folder, li_seg::spec("realplksr-2x").unwrap());
        });
        h.run_steps(3);
        assert!(h.query_by_label("Download GPU model").is_none());
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(test_control.is_cancelled());
        std::fs::remove_dir_all(dir).unwrap();
    }

    use super::*;
    use egui_kittest::kittest::Queryable;
    #[test]
    fn enhance_settings_clicks_candidates_custom_and_default_per_function() {
        let _lock = AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for method in Method::ALL {
            let dir = super::super::tests::temp_dir(method.key());
            let folder = dir.clone();
            let mut initialized = false;
            let mut h = egui_kittest::Harness::builder().with_size(vec2(760.0, 900.0)).build_ui(move |ui| {
                if !initialized {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::Studio);
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                row(ui, &Tokens::get(ui.ctx()), &BTreeMap::new(), &folder, method);
            });
            h.run_steps(3);
            h.get_by_label(title(method));
            h.get_by_label("Change…").click();
            h.run_steps(2);
            h.get_by_label("Custom model…").click();
            h.run_steps(2);
            if method != Method::Generative {
                let (id, label) = if method == Method::Faithful { ("bsrgan-2x", "BSRGAN 2×") } else { ("realplksr-2x", "RealPLKSR 2×") };
                h.get_by_label(label).click();
                h.run_steps(2);
                assert_eq!(selection::load(&dir, method), Choice::Curated { id: id.into() });
                h.get_by_label("Use default").click();
                h.run_steps(2);
                assert_eq!(selection::load(&dir, method), selection::default_choice(method));
            } else {
                h.get_by_label("Use custom models");
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
