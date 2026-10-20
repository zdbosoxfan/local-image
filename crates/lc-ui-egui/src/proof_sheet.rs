//! Proof sheet dialog and cancellable background PDF export.
use crate::{LightcraftApp, theme::Tokens, widgets};
use lightcraft_engine::proof_sheet::{Caption, PageSize, ProofOptions};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{Receiver, channel},
};

pub struct ProofTask {
    rx: Receiver<Result<usize, String>>,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicUsize>,
    total: usize,
    generation: u64,
}
impl Drop for ProofTask {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn body(app: &mut LightcraftApp, ui: &mut egui::Ui, opts: &mut ProofOptions, folder: &mut String, name: &mut String) {
    ui.label(li_i18n::library::t("Export selected photos as a PDF contact sheet"));
    ui.horizontal_wrapped(|ui| {
        ui.label(li_i18n::library::t("Page size"));
        widgets::choice(ui, "proofPage", &mut opts.page, &[(PageSize::A4, "A4"), (PageSize::Letter, "Letter"), (PageSize::A3, "A3")]);
        widgets::checkbox(ui, "proofLandscape", &mut opts.landscape, "Landscape");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(li_i18n::library::t("Columns"));
        widgets::choice(ui, "proofColumns", &mut opts.columns, &[(1, "1"), (2, "2"), (3, "3"), (4, "4"), (5, "5"), (6, "6")]);
        ui.label(li_i18n::library::t("Rows"));
        widgets::choice(ui, "proofRows", &mut opts.rows, &[(1, "1"), (2, "2"), (3, "3"), (4, "4"), (5, "5"), (6, "6")]);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(li_i18n::library::t("Photo info"));
        widgets::choice(
            ui,
            "proofCaption",
            &mut opts.caption,
            &[
                (Caption::Filename, "Filename"),
                (Caption::Title, "Title"),
                (Caption::Caption, "Caption"),
                (Caption::FilenameTitle, "Filename and title"),
            ],
        );
    });
    ui.label(li_i18n::library::t("Watermark (optional)"));
    widgets::text_field(ui, "proofWatermark", &mut opts.watermark, "Watermark (optional)");
    ui.label(li_i18n::library::t("Output folder"));
    if widgets::field_with_action(ui, "proofFolder", folder, "Output folder", "Choose…", app.services.has_folder_picker()) {
        app.choose_folder("Choose Proof Sheet Folder", "proofFolder");
    }
    if let Some(path) = app.take_chosen("proofFolder") {
        *folder = path;
    }
    ui.label(li_i18n::library::t("PDF filename"));
    widgets::text_field(ui, "proofFilename", name, "PDF filename");
    let n = crate::control::export_targets(app).len();
    let pages = n.div_ceil((opts.rows * opts.columns).max(1));
    ui.label(format!(
        "{n} {} · {pages} {}",
        li_i18n::library::t(if n == 1 { "Photo" } else { "Photos" }),
        li_i18n::library::t(if pages == 1 { "Page" } else { "Pages" })
    ));
}

pub fn start(app: &mut LightcraftApp, opts: &ProofOptions, folder: &str, name: &str) -> Result<Value, String> {
    if app.proof.is_some() {
        return Err("a proof sheet is already exporting".into());
    }
    if name.trim().is_empty() || name.contains(['/', '\\']) || !name.to_ascii_lowercase().ends_with(".pdf") {
        return Err("Enter a PDF filename without folder separators".into());
    }
    let root = std::path::Path::new(folder);
    if !root.is_absolute() || !root.is_dir() {
        return Err("Choose an existing output folder".into());
    }
    let path = root.join(name);
    if path.exists() {
        return Err("A file already exists with that name; choose another filename".into());
    }
    let ids = crate::control::export_targets(app);
    let total = ids.len();
    let job = app.session.prepare_proof_sheet(&ids, opts.clone())?;
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicUsize::new(0));
    let (c, d) = (cancel.clone(), done.clone());
    let worker = move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let result = job.run(&path, &|| c.load(Ordering::Relaxed), &mut |n| {
            d.store(n, Ordering::Relaxed);
        });
        let _ = tx.send(result);
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("proof-sheet".into()).spawn(worker).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    worker();
    app.proof = Some(ProofTask { rx, cancel, done, total, generation: app.session.library_generation });
    Ok(json!({"total":total,"background":true}))
}
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = &app.proof else { return };
    if task.generation != app.session.library_generation {
        task.cancel.store(true, Ordering::Relaxed);
    }
    match task.rx.try_recv() {
        Ok(result) => {
            app.proof = None;
            match result {
                Ok(pages) => app.toast(ctx, format!("{}: {pages} {}", li_i18n::library::t("Proof sheet exported"), li_i18n::library::t("Pages"))),
                Err(e) => app.toast_error(ctx, e),
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.proof = None;
            app.toast_error(ctx, "Proof sheet worker stopped");
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            egui::Window::new(li_i18n::library::t("Export Proof Sheet")).collapsible(false).show(ctx, |ui| {
                let t = Tokens::get(ui.ctx());
                ui.label(egui::RichText::new(format!("{} / {}", task.done.load(Ordering::Relaxed), task.total)).color(t.text));
                if widgets::text_button(ui, "proofCancel", "Cancel", false).clicked() {
                    task.cancel.store(true, Ordering::Relaxed);
                }
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
