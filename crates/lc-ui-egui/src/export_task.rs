//! Background export: the Export dialog, File → Export with Preset and Export with Previous hand
//! their batch to a worker thread so the window stays responsive. Photos are prepared on the UI
//! thread ([`lightcraft_engine::export::prepare_export`]: cheap, needs the session) and rendered,
//! encoded and written on the worker. The task carries one `JobCtl` (`step` = photos started /
//! total, message = the file in progress); its Cancel stops the batch and any AI enlargement in
//! it (the same job reaches the renderer as an `enhance::JobCtl` view), and it shows in the
//! Library's task panel (`tasks::progress`).
//!
//! Needs [`crate::Services::write_shared`] (a thread-safe writer); without it (web) the batch runs
//! synchronously as before.

use std::sync::mpsc::{Receiver, channel};

use lightcraft_engine::export::{Destination, ExportOptions, PreparedExport, run_batch_with_ctl};
use lightcraft_sysmem::JobCtl;
use serde_json::{Value, json};

use crate::LightcraftApp;

pub struct ExportTask {
    library_generation: u64,
    pub total: usize,
    /// Cancel and progress: `step` = photos started so far / total, message = the file in
    /// progress.
    pub job: JobCtl,
    rx: Receiver<Result<Vec<Value>, String>>,
    /// What to do with the files once they are written (add to the library, show the folder…).
    after: crate::control::AfterExport,
}

impl ExportTask {
    /// `{total, done, current}` for `ui.inspect`.
    pub fn status(&self) -> Value {
        let status = self.job.status();
        json!({"total": self.total, "done": status.step.map_or(0, |s| s.0), "current": status.message})
    }

    /// Stop the batch at the next photo (and any AI enlargement in progress).
    pub fn cancel(&self) {
        self.job.cancel();
    }

    #[cfg(test)]
    pub(crate) fn for_test(library_generation: u64, total: usize, job: JobCtl, rx: Receiver<Result<Vec<Value>, String>>) -> Self {
        Self { library_generation, total, job, rx, after: Default::default() }
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
    let job = JobCtl::new();
    job.set_step(0, u32::try_from(total).unwrap_or(u32::MAX));
    // One job: the batch's photo count and the renderer's AI enlargement see the same cancel.
    let worker_ctl = job.clone();
    let (tx, rx) = channel();
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
                worker_ctl.set_step(u32::try_from(done).unwrap_or(u32::MAX), u32::try_from(total).unwrap_or(u32::MAX));
                worker_ctl.set_message(name);
                !worker_ctl.is_cancelled()
            },
            &worker_ctl,
        );
        let _ = tx.send(r);
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            work()
        })
        .map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    app.export = Some(ExportTask { library_generation: app.session.library_generation, total, job, rx, after });
    Ok(json!({"background": true, "total": total}))
}

/// Per frame: when the batch finishes, report it (its progress row is drawn by the Library's task
/// panel, `tasks::progress`). A library change cancels the batch and skips its completion
/// actions.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = &app.export else { return };
    let library_changed = task.library_generation != app.session.library_generation;
    if library_changed {
        task.cancel();
    }
    match task.rx.try_recv() {
        Ok(r) => {
            let cancelled = task.job.is_cancelled();
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
    use std::sync::{Arc, atomic::Ordering};

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
        let job = JobCtl::new();
        app.export = Some(ExportTask {
            library_generation: app.session.library_generation,
            total: 1,
            job: job.clone(),
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
        assert!(job.is_cancelled());
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
            let job = JobCtl::new();
            app.export =
                Some(ExportTask { library_generation: app.session.library_generation, total: 1, job: job.clone(), rx, after: Default::default() });
            let ctx = egui::Context::default();
            poll(&mut app, &ctx);
            assert!(app.export.is_some());
            assert!(!job.is_cancelled());
            assert!(crate::widgets::take_registry(&ctx).is_empty(), "only interactive progress paint is suspended");
            tx.send(if fail { Err("injected export failure".into()) } else { Ok(vec![json!({"path":"already-written.png","photo":1})]) }).unwrap();
            poll(&mut app, &ctx);
            assert!(app.export.is_none());
            assert_eq!(app.ui.dialog, dialog);
            assert!(!job.is_cancelled());
            if fail {
                assert!(app.ui.toast.as_ref().unwrap().0.contains("injected export failure"));
            } else {
                assert_eq!(app.last_export_result.as_ref().unwrap()["files"][0]["path"], "already-written.png");
            }
        }
    }
}
