//! File › Open Folder and File › Automate › Remove Backgrounds (AI).
use super::*;

// Open Folder

/// File › Open Folder…: opens a folder's images (natural order). The filmstrip shows them.
pub fn open_folder(app: &mut PhotocraftApp) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(dir) = file_dialog().set_title(tl!("Open Folder")).pick_folder() else { return Ok(Value::Null) };
        open_folder_path(app, &dir)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        Err("Open Folder needs the desktop app".into())
    }
}

pub use photocraft_engine::batch_io::{folder_images, natural_cmp};

pub fn open_folder_path(app: &mut PhotocraftApp, dir: &std::path::Path) -> Result<Value, String> {
    let files = folder_images(dir);
    if files.is_empty() {
        return Err(format!("{} has no images Local Image can open.", dir.display()));
    }
    // Open the first image now; the rest wait in the filmstrip (opened on click).
    crate::filmstrip_ui::set_folder(dir, files.clone());
    let first = files[0].clone();
    let bytes = photocraft_engine::batch_io::read_file(&first)?;
    let _ = app.open_file(&first.display().to_string(), &bytes)?;
    app.ui.status = li_i18n::fmt(tl!("{n} images in {folder}"), &[("n", &files.len().to_string()), ("folder", &dir.display().to_string())]);
    app.ui.status_error = false;
    Ok(json!({ "count": files.len() }))
}

// Batch

/// File › Automate › Remove Backgrounds (AI).
#[derive(Default)]
pub struct BatchState {
    pub(super) files: Vec<PathBuf>,
    pub(super) output: Option<PathBuf>,
    pub(super) template: String,
    pub(super) format: String,
    pub(super) conflict: String,
    pub(super) run: Option<std::sync::Arc<std::sync::Mutex<BatchRun>>>,
}

#[derive(Default)]
pub(super) struct BatchRun {
    pub(super) done: usize,
    pub(super) status: Vec<(String, String)>,
    pub(super) finished: bool,
    pub(super) ctl: lightcraft_sysmem::JobCtl,
}

pub(super) fn batch<R>(f: impl FnOnce(&mut Option<BatchState>) -> R) -> R {
    thread_local! { static B: std::cell::RefCell<Option<BatchState>> = const { std::cell::RefCell::new(None) }; }
    B.with(|b| f(&mut b.borrow_mut()))
}

pub fn open_batch() {
    batch(|b| {
        if b.is_none() {
            *b = Some(BatchState { template: "{name}-cutout".into(), format: "png".into(), conflict: "rename".into(), ..Default::default() });
        }
    });
}

pub fn batch_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if batch(|b| b.is_none()) {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    egui::Window::new(tl!("Remove Backgrounds"))
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(560.0, 480.0))
        .frame(egui::Frame::window(&ctx.global_style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).inner_margin(egui::Margin::same(16)))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            batch(|b| {
                let Some(b) = b.as_mut() else { return };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if widgets::secondary_button(ui, tl!("Add Images…"), 0.0).clicked()
                            && let Some(paths) = file_dialog().add_filter_nocase(tl!("Images"), &photocraft_codecs::readable_extensions()).pick_files()
                        {
                            b.files.extend(paths);
                        }
                        if widgets::secondary_button(ui, tl!("Add Folder…"), 0.0).clicked()
                            && let Some(dir) = file_dialog().pick_folder()
                        {
                            b.files.extend(
                                folder_images(&dir)
                                    .into_iter()
                                    .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| photocraft_codecs::is_readable_extension(e, false))),
                            );
                        }
                    }
                    ui.label(RichText::new(li_i18n::fmt(tl!("{n} images"), &[("n", &b.files.len().to_string())])).color(t.text_dim));
                    if !b.files.is_empty() && widgets::secondary_button(ui, tl!("Clear"), 0.0).clicked() {
                        b.files.clear();
                    }
                });
                crate::background_ui::settings(app, ui, false);
                crate::background_ui::readiness(app, ui);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("File Naming")).color(t.text_dim));
                    ui.text_edit_singleline(&mut b.template).on_hover_text(format!("{}: {{name}}, {{counter}}", tl!("File Naming")));
                });
                let example = b.files.first().cloned().unwrap_or_else(|| PathBuf::from("photo.jpg"));
                match batch_name(&b.template, &example, 1, &b.format) {
                    Ok(name) => {
                        ui.label(RichText::new(format!("Example: {name}")).color(t.text_faint));
                    }
                    Err(error) => {
                        ui.label(RichText::new(error).color(t.warning));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Format")).color(t.text_dim));
                    widgets::dropdown(ui, "batch-format", &mut b.format, &[("png".into(), "PNG"), ("tiff".into(), "TIFF"), ("psd".into(), "PSD")], 120.0);
                    let conflict_help = "If an output file exists, overwrite it, skip it, or save under a new name.";
                    ui.label(RichText::new(tl!("Action")).color(t.text_dim)).on_hover_text(tl!(conflict_help));
                    widgets::dropdown(
                        ui,
                        "batch-conflict",
                        &mut b.conflict,
                        &[("overwrite".into(), "Overwrite"), ("skip".into(), "Skip"), ("rename".into(), "Rename")],
                        120.0,
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Save to:")).color(t.text_dim));
                    let out = b.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| tl!("a “cutouts” folder beside each image").into());
                    ui.label(out);
                    #[cfg(not(target_arch = "wasm32"))]
                    if widgets::secondary_button(ui, tl!("Choose…"), 0.0).clicked()
                        && let Some(folder) = file_dialog().pick_folder()
                    {
                        b.output = Some(folder);
                    }
                });
                ui.add_space(6.0);
                let run = b.run.clone();
                let running = run.as_ref().is_some_and(|r| r.lock().map(|r| !r.finished).unwrap_or(false));
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    let rows: Vec<(String, String)> = match &run {
                        Some(r) => r.lock().map(|r| r.status.clone()).unwrap_or_default(),
                        None => b
                            .files
                            .iter()
                            .map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), tl!("Waiting").into()))
                            .collect(),
                    };
                    for (name, s) in rows {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(s).color(t.text_dim)));
                        });
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if running {
                        if let Some(r) = &run {
                            let (d, n) = r.lock().map(|r| (r.done, r.status.len())).unwrap_or((0, 1));
                            ui.add(
                                egui::ProgressBar::new(d as f32 / n.max(1) as f32)
                                    .desired_width(240.0)
                                    .text(li_i18n::fmt(tl!("{done} of {total}"), &[("done", &d.to_string()), ("total", &n.to_string())])),
                            );
                            if widgets::secondary_button(ui, tl!("Cancel"), 0.0).clicked()
                                && let Ok(r) = r.lock()
                            {
                                r.ctl.cancel();
                            }
                        }
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    } else {
                        let ok =
                            !b.files.is_empty() && crate::background_ui::model_ready(&app.ui.ai) && batch_name(&b.template, &example, 1, &b.format).is_ok();
                        if ui.add_enabled_ui(ok, |ui| widgets::primary_button(ui, tl!("Remove Backgrounds"), 0.0)).inner.clicked() {
                            b.run = Some(start_batch(b, &app.ui.ai));
                        }
                    }
                });
            });
        });
    if !open {
        batch(|b| {
            if let Some(r) = b.as_ref().and_then(|b| b.run.clone())
                && let Ok(r) = r.lock()
            {
                r.ctl.cancel();
            }
            *b = None;
        });
    }
}

/// Validate a basename before joining it to the output folder. Never interpret paths.
pub(super) fn batch_name(template: &str, input: &std::path::Path, counter: usize, format: &str) -> Result<String, String> {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let name = template.replace("{name}", stem).replace("{counter}", &counter.to_string());
    if name.trim().is_empty()
        || matches!(name.as_str(), "." | "..")
        || name.contains(['/', '\\', '{', '}', ':', '*', '?', '"', '<', '>', '|'])
        || invalid_windows_basename(&name)
        || name.chars().any(char::is_control)
    {
        return Err("Use a file name with {name} and/or {counter}, without folders, unknown tokens, reserved names or invalid filename characters.".into());
    }
    if !["png", "tiff", "psd"].contains(&format) {
        return Err("Choose PNG, TIFF or PSD".into());
    }
    Ok(format!("{name}.{format}"))
}

pub(super) fn batch_export(
    input: &std::path::Path,
    dest: &std::path::Path,
    format: &str,
    options: &crate::ai_ui::AiOptions,
    ctl: &lightcraft_sysmem::JobCtl,
) -> anyhow::Result<()> {
    ctl.check()?;
    let bytes = photocraft_engine::batch_io::read_input(input)?;
    let mut doc = photocraft_io::import(&input.to_string_lossy(), &bytes)?.document;
    // Export a cutout of the visible photograph, retaining its document colour profile.
    let img = photocraft_engine::ai_cmds::flatten_rgba(&doc)?;
    let layer = photocraft_engine::ai_cmds::layer_from_rgba(&doc, "Cutout", &img, 0, 0)?;
    doc.layers.clear();
    let id = doc.insert_above(None, layer);
    crate::background_ui::apply_cutout(&mut doc, id, options, ctl)?;
    ctl.check()?;
    let encoded = photocraft_io::export(&doc, format, &Default::default())?.bytes;
    ctl.check()?;
    // Shared staging preserves cancellation, modes, symlinks and atomic publication.
    let cancelled = || ctl.is_cancelled();
    let staged = li_fs::stage_write(dest, &encoded, &lightcraft_sysmem::jobs::Interrupt::cancel_only(&cancelled))?;
    ctl.check()?;
    staged.commit()?;
    Ok(())
}

pub(super) fn start_batch(b: &BatchState, options: &crate::ai_ui::AiOptions) -> Arc<std::sync::Mutex<BatchRun>> {
    start_batch_with(b, options, &batch_export)
}

// Inject the work operation in deterministic cancellation tests; the worker loop is real.
pub(super) fn start_batch_with(
    b: &BatchState,
    options: &crate::ai_ui::AiOptions,
    process: &'static (dyn Fn(&std::path::Path, &std::path::Path, &str, &crate::ai_ui::AiOptions, &lightcraft_sysmem::JobCtl) -> anyhow::Result<()> + Sync),
) -> Arc<std::sync::Mutex<BatchRun>> {
    let run = Arc::new(std::sync::Mutex::new(BatchRun {
        status: b.files.iter().map(|p| (p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), tl!("Waiting").into())).collect(),
        ..Default::default()
    }));
    let (files, output, template, format, conflict, options) =
        (b.files.clone(), b.output.clone(), b.template.clone(), b.format.clone(), b.conflict.clone(), options.clone());
    let r2 = run.clone();
    let worker = std::thread::Builder::new().name("batch-cutout".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let ctl = r2.lock().map(|r| r.ctl.clone()).unwrap_or_default();
        for (i, path) in files.iter().enumerate() {
            if ctl.is_cancelled() {
                break;
            }
            let set = |s: String| {
                if let Ok(mut r) = r2.lock() {
                    r.status[i].1 = s;
                }
            };
            set(tl!("Working…").into());
            let res = (|| -> anyhow::Result<Option<PathBuf>> {
                let dir = output.clone().unwrap_or_else(|| path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("cutouts"));
                let name = batch_name(&template, path, i + 1, &format).map_err(anyhow::Error::msg)?;
                let Some(dest) = photocraft_engine::batch_io::prepare_output(&dir, &name, &conflict, &files)? else {
                    return Ok(None);
                };
                process(path, &dest, &format, &options, &ctl)?;
                Ok(Some(dest))
            })();
            let cancelled = ctl.is_cancelled();
            match res {
                Ok(Some(dest)) => set(format!("Saved {}", dest.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
                Ok(None) => {
                    let skipped = "Skip";
                    set(tl!(skipped).into());
                }
                Err(_) if cancelled => set(tl!("Cancelled").into()),
                Err(e) => set(format!("{e:#}")),
            }
            if let Ok(mut r) = r2.lock() {
                r.done = i + 1;
            }
            if cancelled {
                break;
            }
        }
        if let Ok(mut r) = r2.lock() {
            if ctl.is_cancelled() {
                for (_, s) in &mut r.status {
                    if s == tl!("Waiting") {
                        *s = tl!("Cancelled").into();
                    }
                }
            }
            r.finished = true;
        }
    });
    if let Err(error) = worker
        && let Ok(mut r) = run.lock()
    {
        r.finished = true;
        for (_, s) in &mut r.status {
            *s = error.to_string();
        }
    }
    run
}

// Portable outputs also work when a batch is later copied to Windows.
pub(super) fn invalid_windows_basename(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    name.ends_with([' ', '.'])
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|s| matches!(s, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
}
