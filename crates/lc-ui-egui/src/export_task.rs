//! Background export: the Export dialog, File → Export with Preset and Export with Previous hand
//! their batch to a worker thread so the window stays responsive. Photos are prepared on the UI
//! thread ([`lightcraft_engine::export::prepare_export`]: cheap, needs the session) and rendered,
//! encoded and written on the worker; a progress panel shows the count and a Cancel button.
//!
//! Needs [`crate::Services::write_shared`] (a thread-safe writer); without it (web) the batch runs
//! synchronously as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use egui::Align2;
use lightcraft_engine::export::{Destination, ExportOptions, PreparedExport, run_batch_with_ctl};
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::theme::Tokens;

pub struct ExportTask {
    library_generation: u64,
    pub total: usize,
    /// Photos started so far and the file in progress.
    pub progress: Arc<Mutex<(usize, String)>>,
    pub cancel: Arc<AtomicBool>,
    enhance_ctl: Arc<lightcraft_engine::enhance::JobCtl>,
    rx: Receiver<Result<Vec<Value>, String>>,
    /// What to do with the files once they are written (add to the library, show the folder…).
    after: crate::control::AfterExport,
}

impl ExportTask {
    /// `{total, done, current}` for `ui.inspect`.
    pub fn status(&self) -> Value {
        let (done, current) = self.progress.lock().map(|g| g.clone()).unwrap_or_default();
        json!({"total": self.total, "done": done, "current": current})
    }
}

/// Start exporting `items` in the background. Errors per photo are collected, not fatal.
pub fn start(
    app: &mut LightcraftApp,
    items: Vec<PreparedExport>,
    opts: ExportOptions,
    to: Destination,
    after: crate::control::AfterExport,
) -> Result<Value, String> {
    if app.export.is_some() {
        return Err("an export is already running".into());
    }
    let write = app.services.write_shared.clone().ok_or("no background writer")?;
    let total = items.len();
    let progress = Arc::new(Mutex::new((0, String::new())));
    let cancel = Arc::new(AtomicBool::new(false));
    let enhance_ctl = lightcraft_engine::enhance::JobCtl::new();
    let worker_ctl = enhance_ctl.clone();
    let (tx, rx) = channel();
    let (p, c) = (progress.clone(), cancel.clone());
    let work = move || {
        let mut w = |path: &str, bytes: &[u8]| write(path, bytes);
        let r = run_batch_with_ctl(
            items,
            &opts,
            &to,
            &mut w,
            &|path| std::path::Path::new(path).exists(),
            false,
            &mut |done, name| {
                if let Ok(mut g) = p.lock() {
                    *g = (done, name.to_string());
                }
                !c.load(Ordering::Relaxed)
            },
            &worker_ctl,
        );
        let _ = tx.send(r);
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("export".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    app.export = Some(ExportTask { library_generation: app.session.library_generation, total, progress, cancel, enhance_ctl, rx, after });
    Ok(json!({"background": true, "total": total}))
}

/// Per frame: draw the progress panel; when the batch finishes, report it.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = &app.export else { return };
    let library_changed = task.library_generation != app.session.library_generation;
    if library_changed {
        task.cancel.store(true, Ordering::Relaxed);
        task.enhance_ctl.cancel();
    }
    match task.rx.try_recv() {
        Ok(r) => {
            let cancelled = task.cancel.load(Ordering::Relaxed);
            let total = task.total;
            let after = task.after.clone();
            app.export = None;
            let msg = match r {
                Ok(files) => {
                    let ok = files.iter().filter(|f| f.get("path").is_some()).count();
                    let failed: Vec<&Value> = files.iter().filter(|f| f.get("error").is_some()).collect();
                    let skipped = files.iter().filter(|f| f.get("skipped").is_some()).count();
                    let mut m =
                        crate::i18n::tr_format!("Exported {ok} of {total} photo{}", if total == 1 { "" } else { "s" }, ok = ok, total = total);
                    if skipped > 0 {
                        m += &crate::i18n::tr_format!(" · {skipped} skipped (file exists)", skipped = skipped);
                    }
                    if let Some(first) = failed.first() {
                        m += &crate::i18n::tr_format!(" · {} failed: {}", failed.len(), first["error"].as_str().unwrap_or("error"));
                    }
                    if cancelled {
                        m += " · cancelled";
                    }
                    if library_changed {
                        m += " · library changed; completion actions were skipped";
                    } else if let Some(note) = after.run(app, &files) {
                        m += &format!(" · {note}");
                    }
                    app.last_export_result = Some(json!({"files": files, "cancelled": cancelled, "libraryChanged": library_changed}));
                    m
                }
                Err(e) => e,
            };
            app.toast(ctx, msg);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            // Keep collecting worker completion above; suspend only this context-root
            // Cancel window while the shared host dialog owns pointer/keyboard input.
            if app.host_modal {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            let (done, current) = task.progress.lock().map(|g| g.clone()).unwrap_or_default();
            let (total, cancel, enhance_ctl) = (task.total, task.cancel.clone(), task.enhance_ctl.clone());
            let t = Tokens::get(ctx);
            egui::Window::new(crate::i18n::tr("Exporting"))
                .title_bar(false)
                .resizable(false)
                .anchor(Align2::LEFT_BOTTOM, [16.0, -56.0])
                .fixed_size([300.0, 64.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::tr_format!("Exporting {} of {total}", (done + 1).min(total), total = total))
                                .color(t.text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let stopping = cancel.load(Ordering::Relaxed);
                            if crate::widgets::text_button(ui, "exportCancel", if stopping { "Stopping…" } else { "Cancel" }, false).clicked() {
                                cancel.store(true, Ordering::Relaxed);
                                enhance_ctl.cancel();
                            }
                        });
                    });
                    ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).desired_width(280.0));
                    ui.label(egui::RichText::new(current).size(11.0).color(t.text_dim));
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.export = None;
            app.toast(ctx, "Export stopped unexpectedly");
        }
    }
}

#[cfg(test)]
mod library_origin_tests {
    use super::*;

    #[test]
    fn library_origin_export_never_runs_completion_actions_in_another_library() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        let (tx, rx) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        app.services.reveal = Some(Box::new(move |_| {
            seen.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        app.export = Some(ExportTask {
            library_generation: app.session.library_generation,
            total: 1,
            progress: Default::default(),
            cancel: cancel.clone(),
            enhance_ctl: lightcraft_engine::enhance::JobCtl::new(),
            rx,
            after: crate::control::AfterExport { add_to_library: true, add_to_stack: true, after: "showInFolder".into(), app: String::new() },
        });
        assert!(crate::panels::settings::open_library(&mut app, &json!({"path":"unused-export-library"})).is_err());
        app.session
            .open_library_in(
                lightcraft_engine::library::LibraryStores {
                    dir: "other-library".into(),
                    catalog: Box::new(lightcraft_catalog::MemStore::new()),
                    files: Box::new(lightcraft_catalog::MemStore::new()),
                    on_disk: false,
                },
                false,
            )
            .unwrap();
        let before = serde_json::to_value(&app.session.catalog).unwrap();
        tx.send(Ok(vec![json!({"path":"original-export.png","photo":1})])).unwrap();
        poll(&mut app, &egui::Context::default());
        assert!(app.export.is_none());
        assert!(cancel.load(Ordering::Relaxed));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(serde_json::to_value(&app.session.catalog).unwrap(), before);
        assert!(app.session.undo.is_empty());
        assert_eq!(app.last_export_result.as_ref().unwrap()["libraryChanged"], true);
    }
    #[test]
    fn shared_shell_export_poll_keeps_completion_and_failure_when_modal() {
        for fail in [false, true] {
            let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
            app.host_modal = true;
            app.ui.dialog = Some(crate::state::Dialog::Settings { tab: "interface".into() });
            let dialog = app.ui.dialog.clone();
            let (tx, rx) = channel();
            let cancel = Arc::new(AtomicBool::new(false));
            app.export = Some(ExportTask {
                library_generation: app.session.library_generation,
                total: 1,
                progress: Default::default(),
                cancel: cancel.clone(),
                enhance_ctl: lightcraft_engine::enhance::JobCtl::new(),
                rx,
                after: Default::default(),
            });
            let ctx = egui::Context::default();
            poll(&mut app, &ctx);
            assert!(app.export.is_some());
            assert!(!cancel.load(Ordering::Relaxed));
            assert!(crate::widgets::take_registry(&ctx).is_empty(), "only interactive progress paint is suspended");
            tx.send(if fail { Err("injected export failure".into()) } else { Ok(vec![json!({"path":"already-written.png","photo":1})]) }).unwrap();
            poll(&mut app, &ctx);
            assert!(app.export.is_none());
            assert_eq!(app.ui.dialog, dialog);
            assert!(!cancel.load(Ordering::Relaxed));
            if fail {
                assert!(app.ui.toast.as_ref().unwrap().0.contains("injected export failure"));
            } else {
                assert_eq!(app.last_export_result.as_ref().unwrap()["files"][0]["path"], "already-written.png");
            }
        }
    }
}
