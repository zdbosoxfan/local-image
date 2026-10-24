//! AI resource pages inside the existing Preferences shell.
mod connection;
mod providers;
use super::*;

pub(crate) const CONNECTION: &str = "ai.connection";
pub(crate) const PAGES: [(&str, &str); 4] =
    [(CONNECTION, "Connection"), ("ai.tools", "On-device Tools"), ("ai.models", "Generative Models"), ("ai.providers", "Providers")];
pub(crate) fn owns(section: &str) -> bool {
    PAGES.iter().any(|(id, _)| *id == section)
}

pub(crate) fn reset() {
    dialogs_mut(|d| {
        d.settings = None;
        d.applied_settings = None;
        d.lan = None;
        d.message.clear();
    });
}
fn draft() -> AiSettings {
    dialogs_mut(|d| {
        if d.settings.is_none() {
            let settings = AiSettings::load();
            d.applied_settings = Some(settings.clone());
            d.settings = Some(settings);
        }
        d.settings.clone().unwrap_or_default()
    })
}
pub(crate) fn changed() -> bool {
    dialogs_mut(|d| d.settings.as_ref().zip(d.applied_settings.as_ref()).is_some_and(|(draft, applied)| draft != applied))
}
pub(crate) fn pending_settings() -> Option<AiSettings> {
    changed().then(draft)
}
pub(crate) fn apply() -> Result<(), String> {
    if changed() {
        let settings = draft();
        if let Err(error) = save_all(&settings) {
            // The unified Preferences shell presents this transaction error on every page.
            // Connection actions still use save_all's local message independently.
            dialogs_mut(|dialogs| dialogs.message.clear());
            return Err(error);
        }
    }
    Ok(())
}
fn save_all(settings: &AiSettings) -> Result<(), String> {
    settings.save().map_err(|e| {
        let error = format!("{e:#}");
        dialogs_mut(|d| d.message = error.clone());
        error
    })?;
    dialogs_mut(|d| d.applied_settings = Some(settings.clone()));
    refresh();
    Ok(())
}
// Connect applies connection fields only. Provider credentials remain a Preferences draft.
fn save_connection(settings: &AiSettings) -> Result<(), String> {
    let mut connected = AiSettings::load();
    connected.comfy_host = settings.comfy_host.clone();
    connected.comfy_port = settings.comfy_port;
    connected.comfy_directory = settings.comfy_directory.clone();
    connected.comfy_python = settings.comfy_python.clone();
    connected.setup_mode = settings.setup_mode.clone();
    connected.model_directory = settings.model_directory.clone();
    save_all(&connected)
}

pub(crate) fn page(app: &mut PhotocraftApp, ui: &mut egui::Ui, section: &str) -> bool {
    let Some((_, title)) = PAGES.iter().find(|(id, _)| *id == section) else {
        return false;
    };
    let t = Tokens::get(ui.ctx());
    let st = status();
    let mut settings = draft();
    ui.label(RichText::new(tl!(*title)).font(crate::theme::semibold(14.0)).color(t.text));
    ui.add_space(6.0);
    let summary = match section {
        CONNECTION if st.connected => li_i18n::fmt(tl!("Connected to ComfyUI at {host}"), &[("host", &st.host)]),
        CONNECTION => li_i18n::fmt(tl!("ComfyUI is not running at {host}"), &[("host", &st.host)]),
        "ai.tools" => tl!("Download the tools you need. Models and technical options are under Manage.").into(),
        "ai.models" => tl!("Generative models run in ComfyUI, locally or on your server.").into(),
        "ai.providers" => tl!("Optional online models use your own API keys and provider accounts.").into(),
        _ => String::new(),
    };
    ui.label(RichText::new(summary).color(t.text_dim));
    ui.add_space(6.0);
    egui::ScrollArea::vertical().max_height((ui.ctx().content_rect().height() - 210.0).clamp(160.0, 390.0)).id_salt(section).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match section {
            CONNECTION => {
                if let Some(gpu) = &st.gpu {
                    ui.label(RichText::new(&gpu.name).color(t.text_dim));
                }
                connection::draw(ui, &mut settings, &st, &t);
            }
            "ai.tools" => tools(app, ui, &t, &settings, &st),
            "ai.models" => models(app, ui, &mut settings, &st, &t),
            "ai.providers" => providers::draw(ui, &mut settings, &t),
            _ => {}
        }
        let message = dialogs_mut(|d| d.message.clone());
        if !message.is_empty() {
            ui.label(RichText::new(message).color(t.warning));
        }
    });
    dialogs_mut(|d| d.settings = Some(settings));
    if dls_running() || dialogs_mut(|d| d.lan.as_ref().is_some_and(|scan| scan.snapshot().running)) {
        ui.ctx().request_repaint_after(Duration::from_millis(150));
    }
    true
}

fn function(group: li_seg::Group) -> (&'static str, &'static str) {
    use li_seg::Group::*;
    match group {
        Subject => ("Subject Selection", "Select subjects and remove backgrounds."),
        Sky => ("Sky Selection", "Select the sky for targeted edits."),
        Depth => ("Depth Masks", "Separate foreground and background by distance."),
        Tagging => ("Scene Recognition", "Organise photos by scene in Smart Sort."),
        Faces => ("Face Recognition", "Group people in your library. Enable it separately in Smart Sort."),
        RawDenoise => ("RAW Denoise", "Reduce noise in raw photos."),
        Remove => ("AI Remove", "Repair brushed areas on this computer, without an AI server."),
        RgbDenoise => ("Photo Denoise", "Reduce noise in JPEG, TIFF and edited photos."),
    }
}

#[cfg(test)]
thread_local! { static TEST_TOOLS_ROOT: std::cell::RefCell<Option<std::path::PathBuf>> = const {std::cell::RefCell::new(None)}; }

fn tools_model_dir() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_TOOLS_ROOT.with(|root| root.borrow().clone()) {
        return root;
    }
    photocraft_engine::seg::models_dir()
}

fn tools(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &Tokens, settings: &AiSettings, st: &EngineStatus) {
    let dir = tools_model_dir();
    let dls = downloads();
    ui.label(RichText::new(tl!("On-device tools work without a ComfyUI server.")).color(t.text_dim));
    ui.add_space(8.0);
    for group in li_seg::Group::ALL {
        let (title, about) = function(group);
        let spec = if group == li_seg::Group::Subject && app.ui.ai.cutout_engine == "better" {
            catalog::on_device_spec("inspyrenet").unwrap_or(catalog::on_device_default(group))
        } else {
            catalog::on_device_default(group)
        };
        let custom = li_seg::custom(&dir, group).is_some();
        let backend = match group {
            li_seg::Group::RawDenoise | li_seg::Group::RgbDenoise => tl!("This computer · GPU required"),
            _ => tl!("This computer · CPU"),
        };
        tool_row(ui, t, &dls, &dir, title, about, backend, spec, custom);
        let response = egui::CollapsingHeader::new(tl!("Manage…")).id_salt(("ai-tool-manage", group.key())).show(ui, |ui| {
            if group == li_seg::Group::Subject {
                let mut quality = if app.ui.ai.cutout_engine == "better" { "better" } else { "fast" };
                if widgets::dropdown(ui, "ai-subject-quality", &mut quality, &[("fast", "Fast"), ("better", "Better Quality")], 180.0) {
                    app.ui.ai.cutout_engine = quality.into();
                    li_seg::set_subject_quality(&dir, quality == "better");
                }
            }
            model_function_row(ui, t, &dls, &dir, group, spec, true);
        });
        response
            .header_response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, format!("{} {}", tl!("Manage"), tl!(title))));
        ui.separator();
    }
    ui.add_space(8.0);
    ui.label(RichText::new("ComfyUI").font(crate::theme::semibold(14.0)).color(t.text));
    ui.label(RichText::new(tl!("AI enlargement requires a GPU/NPU.")).color(t.text_dim));
    ui.add_space(8.0);
    for method in [li_ai::enhance::Method::Faithful, li_ai::enhance::Method::Rescue] {
        let (title, about) = if method == li_ai::enhance::Method::Faithful {
            ("Photo Upscaling", "Enlarge photos while preserving their appearance.")
        } else {
            ("Photo Restoration", "Restore noisy or compressed photos while enlarging.")
        };
        enhance_tool_row(ui, t, &dls, &dir, method, title, about, settings, st);
        egui::CollapsingHeader::new(li_i18n::fmt(tl!("Manage {function}"), &[("function", tl!(title))]))
            .id_salt(("ai-enhance-manage", method.key()))
            .show(ui, |ui| enhance_settings::row(ui, t, &dls, &dir, method));
        ui.separator();
    }
    egui::CollapsingHeader::new(tl!("Storage and cleanup")).id_salt("ai-local-storage").show(ui, |ui| {
        ui.label(dir.display().to_string());
        let paths: std::collections::HashSet<_> =
            li_seg::MODELS.iter().flat_map(|spec| spec.files()).map(|file| li_seg::companion_path(&dir, file.file)).collect();
        let bytes = paths.iter().filter_map(|path| std::fs::metadata(path).ok()).map(|m| m.len()).sum();
        ui.label(li_i18n::fmt(tl!("Managed model storage: {size}"), &[("size", &li_ai::download::human_bytes(bytes))]));
    });
}

#[allow(clippy::too_many_arguments)]
fn enhance_tool_row(
    ui: &mut egui::Ui,
    t: &Tokens,
    dls: &BTreeMap<String, Download>,
    dir: &std::path::Path,
    method: li_ai::enhance::Method,
    title: &str,
    about: &str,
    settings: &AiSettings,
    st: &EngineStatus,
) {
    let spec = enhance_settings::selected_spec(dir, method);
    let key = spec.map(|spec| format!("{}:default", spec.id));
    let download = key.as_ref().and_then(|key| dls.get(key));
    ui.horizontal_top(|ui| {
        let width = (ui.available_width() - 150.0).max(140.0);
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.label(RichText::new(tl!(title)).font(crate::theme::semibold(12.5)).color(t.text));
            ui.label(RichText::new(tl!(about)).color(t.text_dim));
            if spec.is_none() {
                ui.label(RichText::new(tl!("Choose a GPU enlargement model in Settings › Local AI.")).color(t.warning));
            }
        });
        ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
            let action = if let Some(download) = download.filter(|download| !download.finished) {
                ui.add(egui::ProgressBar::new(download.done as f32 / download.total.max(1) as f32).desired_width(120.0).show_percentage());
                let response = widgets::secondary_button(ui, tl!("Cancel"), 0.0);
                if response.clicked() {
                    download.ctl.cancel();
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, li_i18n::fmt(tl!("Cancel {function} download"), &[("function", tl!(title))]))
                });
                None
            } else if let Some(spec) = spec {
                if st.gpu.is_some() && st.ready(ModelId::intern(spec.id), "default").is_ok() {
                    ui.label(RichText::new(tl!("Ready")).color(t.text_dim));
                    None
                } else if settings.is_remote() {
                    ui.label(RichText::new(tl!("Install on server")).color(t.text_dim));
                    None
                } else if catalog::preset(ModelId::intern(spec.id), "default").is_some_and(|preset| li_ai::download::missing_files(dir, preset).is_empty()) {
                    ui.label(RichText::new(if st.connected { tl!("Files present — restart ComfyUI") } else { tl!("Files present") }).color(t.text_dim));
                    None
                } else {
                    Some(tl!("Download"))
                }
            } else {
                Some(tl!("Use default"))
            };
            if let Some(action) = action {
                let response = widgets::secondary_button(ui, action, 0.0);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        li_i18n::fmt(tl!("{action} {function}"), &[("action", action), ("function", tl!(title))]),
                    )
                });
                if response.clicked() {
                    if let Some(key) = &key {
                        start_download(key);
                    } else if let Err(error) = li_ai::enhance::selection::save(dir, method, None) {
                        dialogs_mut(|dialogs| dialogs.message = format!("{error:#}"));
                    }
                }
            }
        });
    });
    if let Some(error) = download.and_then(|download| download.error.as_ref()) {
        ui.label(RichText::new(error).color(t.danger));
    }
}

fn tool_row(
    ui: &mut egui::Ui,
    t: &Tokens,
    dls: &BTreeMap<String, Download>,
    dir: &std::path::Path,
    title: &str,
    about: &str,
    backend: &str,
    spec: &'static li_seg::ModelSpec,
    custom: bool,
) {
    let key = format!("seg:{}", spec.id);
    let dl = dls.get(&key);
    let installed = custom || li_seg::installed_bytes(dir, spec).is_some();
    ui.horizontal_top(|ui| {
        let label_width = (ui.available_width() - 150.0).max(140.0);
        ui.vertical(|ui| {
            ui.set_width(label_width);
            ui.label(RichText::new(tl!(title)).font(crate::theme::semibold(12.5)).color(t.text));
            ui.label(RichText::new(tl!(about)).color(t.text_dim));
            ui.label(RichText::new(backend).color(t.text_faint));
            if spec.licence.to_lowercase().contains("non-commercial") {
                ui.label(RichText::new(tl!("Non-commercial licence")).color(t.warning));
            }
        });
        ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
            if let Some(download) = dl.filter(|d| !d.finished) {
                let fraction = if download.total == 0 { 0.0 } else { download.done as f32 / download.total as f32 };
                ui.add(egui::ProgressBar::new(fraction).desired_width(120.0).show_percentage());
                let cancel = widgets::secondary_button(ui, tl!("Cancel"), 0.0);
                cancel.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, li_i18n::fmt(tl!("Cancel {function} download"), &[("function", tl!(title))]))
                });
                if cancel.clicked() {
                    download.ctl.cancel();
                }
            } else if installed {
                ui.label(RichText::new(tl!("Ready")).color(t.text_dim));
            } else {
                ui.label(RichText::new(li_ai::download::human_bytes(spec.download_bytes())).color(t.text_dim));
                let partial = spec.files().any(|file| li_seg::companion_path(dir, file.file).is_file());
                let label = if dl.is_some_and(|d| d.error.is_some()) {
                    tl!("Retry")
                } else if partial {
                    tl!("Repair")
                } else {
                    tl!("Download")
                };
                let action = widgets::secondary_button(ui, label, 0.0);
                action.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        li_i18n::fmt(tl!("{action} {function}"), &[("action", label), ("function", tl!(title))]),
                    )
                });
                if action.clicked() {
                    start_seg_download(spec);
                }
            }
        });
    });
    if let Some(error) = dl.and_then(|d| d.error.as_ref()) {
        ui.label(RichText::new(error).color(t.danger));
    }
}

fn models(app: &mut PhotocraftApp, ui: &mut egui::Ui, settings: &mut AiSettings, st: &EngineStatus, t: &Tokens) {
    if st.connected {
        ui.horizontal_wrapped(|ui| {
            let counts = [("new", st.setup.new.len().to_string()), ("attention", st.setup.attention().to_string())];
            let counts: Vec<(&str, &str)> = counts.iter().map(|(k, v)| (*k, v.as_str())).collect();
            ui.label(RichText::new(li_i18n::fmt(tl!("Models: {new} new, {attention} need attention"), &counts)).color(t.text_dim));
            if widgets::secondary_button(ui, tl!("Set Up Models…"), 0.0).clicked() {
                crate::model_setup::open(None);
            }
        });
        ui.add_space(6.0);
    }
    let mut search = dialogs_mut(|d| d.model_search.clone());
    let mut installed_only = dialogs_mut(|d| *d.installed_only.get_or_insert(settings.is_remote() && st.connected));
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut search).hint_text(tl!("Search model families…")).desired_width(260.0));
        let response = widgets::checkbox(ui, &mut installed_only, tl!("Installed only"));
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, installed_only, tl!("Installed only")));
    });
    dialogs_mut(|d| {
        d.model_search = search.clone();
        d.installed_only = Some(installed_only);
    });
    ui.add_space(8.0);
    let dls = downloads();
    let families = families(&search, installed_only, st);
    if families.is_empty() {
        ui.label(tl!("No models match. Connect to your server or show all models."));
    }
    for (family, variants) in families {
        let ready = variants.iter().filter(|(_, p)| st.connected && st.presets.get(&p.id()).is_some_and(Result::is_ok)).count();
        let title = format!("{family}   {ready}/{} {}", variants.len(), tl!("ready"));
        let expanded = app.ui.ai.settings_families.contains(&family);
        let response = egui::CollapsingHeader::new(RichText::new(title).font(crate::theme::semibold(12.5)))
            .id_salt(("ai-family", &family))
            .default_open(expanded)
            .show(ui, |ui| {
                for (model, preset) in variants {
                    model_row(ui, t, model.info(), preset, st, dls.get(&preset.id()), settings);
                }
            });
        if response.body_response.is_some() {
            app.ui.ai.settings_families.insert(family);
        } else {
            app.ui.ai.settings_families.remove(&family);
        }
        ui.separator();
    }
    egui::CollapsingHeader::new(tl!("Generative enhancement settings"))
        .id_salt("ai-generative-enhance")
        .show(ui, |ui| enhance_settings::row(ui, t, &dls, &photocraft_engine::seg::models_dir(), li_ai::enhance::Method::Generative));
    if widgets::secondary_button(ui, tl!("Browse Models…"), 0.0).clicked() {
        crate::model_browser::open();
    }
    egui::CollapsingHeader::new(tl!("Storage and GPU requirements")).id_salt("ai-server-storage").show(ui, |ui| {
        if !settings.is_remote() {
            ui.label(settings.model_dir().display().to_string());
            #[cfg(not(target_arch = "wasm32"))]
            if widgets::secondary_button(ui, tl!("Change…"), 0.0).clicked() {
                let request = crate::FileDialogSpec::new(crate::Pick::Folder).title(tl!("Choose the model folder"));
                crate::file_dialog::ask_later(ui.ctx(), crate::FileDialogRequest::Described(request), |_, answer| {
                    if let Some(dir) = answer.into_path() {
                        let mut settings = draft();
                        settings.model_directory = dir;
                        let _ = save_connection(&settings);
                        dialogs_mut(|d| d.settings = Some(settings));
                    }
                    Ok(Value::Null)
                });
            }
        }
        for (what, gb) in catalog::VRAM_GUIDE {
            ui.label(format!("{gb} GB · {what}"));
        }
    });
}

fn families(search: &str, installed_only: bool, st: &EngineStatus) -> BTreeMap<String, Vec<(ModelId, &'static catalog::Preset)>> {
    let mut families = BTreeMap::<String, Vec<_>>::new();
    let query = search.trim().to_lowercase();
    for model in ModelId::all() {
        let info = model.info();
        if info.origin != catalog::Origin::Installed && !(info.starter || info.tool || ModelId::ORIGINAL.contains(&model)) {
            continue;
        }
        for preset in catalog::presets_for(model) {
            if installed_only && !preset.installed && !st.presets.get(&preset.id()).is_some_and(Result::is_ok) {
                continue;
            }
            if !query.is_empty() && !format!("{} {} {} {}", info.family_label, info.label, preset.label, info.best_for).to_lowercase().contains(&query) {
                continue;
            }
            families.entry(family_name(&info.family_label)).or_default().push((model, preset));
        }
    }
    families
}

fn family_name(label: &str) -> String {
    let name = label.to_lowercase();
    if name == "auto-detect" {
        return tl!("Other installed models").into();
    }
    for (prefix, family) in [("flux", "FLUX"), ("qwen", "Qwen Image"), ("z-image", "Z-Image"), ("ernie", "ERNIE"), ("seedvr", "SeedVR2")] {
        if name.starts_with(prefix) {
            return family.into();
        }
    }
    label.into()
}

#[cfg(test)]
mod tests;
