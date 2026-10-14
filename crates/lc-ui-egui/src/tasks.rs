//! The Library's background-task runner, on the editor's job model (duplicate-systems audit D14):
//! work runs on a worker thread against what it captured, its result is applied on the UI thread
//! between frames, and every task carries one [`JobCtl`] — real cancel (the worker sees it at its
//! next check; a cancelled task's result is discarded, so the library is left as it was) and
//! progress (`set_step` / `set_fraction` / `set_message`). A task from another library is
//! cancelled and dropped when the library changes.
//!
//! Used for commands whose file-system work can take long on a slow or offline drive (Find
//! Missing Photos, listing the auto-import folder…) and Assisted Culling. Export, Smart Sort and
//! face analysis stream their results through their own tasks, but on the same `JobCtl`, and all
//! of them show in one task panel ([`progress`], the shared `li_ui_kit::progress` widget).

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use lightcraft_sysmem::JobCtl;
use serde_json::{Value, json};

use crate::LightcraftApp;
use crate::i18n::tr;

/// Applies a task's result to the app (on the UI thread).
type Finish = Box<dyn FnOnce(&mut LightcraftApp, &egui::Context) + Send>;

/// Tasks shorter than this never show in the task panel (no flashing for quick work), like the
/// editor's progress dialog delay.
pub const SHOW_AFTER_S: f64 = 0.25;

/// The label of the culling task.
pub const CULLING: &str = "Assisted Culling";

struct Task {
    label: String,
    library_generation: u64,
    job: JobCtl,
    /// UI time when the runner first saw the task (for [`SHOW_AFTER_S`]).
    seen_at: Option<f64>,
    rx: Receiver<Finish>,
}

/// The background tasks in flight.
#[derive(Default)]
pub struct Tasks {
    running: Vec<Task>,
    /// Repaints the window when a task finishes (set every frame).
    pub(crate) repaint: Option<egui::Context>,
}

impl Tasks {
    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// What is running (`ui.inspect` → `tasks`).
    pub fn labels(&self) -> Vec<String> {
        self.running.iter().map(|t| t.label.clone()).collect()
    }

    pub fn is_running(&self, label: &str) -> bool {
        self.running.iter().any(|t| t.label == label)
    }

    /// The job of the running task `label`.
    pub fn job(&self, label: &str) -> Option<&JobCtl> {
        self.running.iter().find(|t| t.label == label).map(|t| &t.job)
    }

    /// Ask task `label` (every task with `None`) to stop: its worker sees it at the next check
    /// and its result is discarded. Returns how many were asked.
    pub fn cancel(&self, label: Option<&str>) -> usize {
        let mut n = 0;
        for t in self.running.iter().filter(|t| label.is_none_or(|l| l == t.label)) {
            t.job.cancel();
            n += 1;
        }
        n
    }

    /// `[{label, fraction, step, message, cancelled}]` of the running tasks.
    pub fn status(&self) -> Value {
        self.running
            .iter()
            .map(|t| {
                let s = t.job.status();
                json!({"label": t.label, "fraction": t.job.fraction(), "step": s.step, "message": s.message, "cancelled": t.job.is_cancelled()})
            })
            .collect()
    }
}

/// Run `work` on a worker thread, then `done(app, ctx, result)` on the UI thread (in the browser
/// build, which has no threads, `work` runs at once and `done` on the next frame). The task can
/// be cancelled (its result is then discarded) but `work` can't see it: use [`spawn_job`] for
/// work that stops early and reports progress.
pub fn spawn<T: Send + 'static>(
    app: &mut LightcraftApp,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut LightcraftApp, &egui::Context, T) + Send + 'static,
) -> Result<(), String> {
    spawn_job(app, label, move |_| work(), done).map(drop)
}

/// [`spawn`] for work that takes the task's [`JobCtl`]: check it (`job.check()` /
/// `job.is_cancelled()`) to stop early, report with `set_step`, `set_fraction`, `set_message`.
/// Returns the job (the caller may cancel it or read its progress).
pub fn spawn_job<T: Send + 'static>(
    app: &mut LightcraftApp,
    label: &str,
    work: impl FnOnce(&JobCtl) -> T + Send + 'static,
    done: impl FnOnce(&mut LightcraftApp, &egui::Context, T) + Send + 'static,
) -> Result<JobCtl, String> {
    let (tx, rx) = channel::<Finish>();
    let repaint = app.tasks.repaint.clone();
    let job = JobCtl::new();
    let worker = job.clone();
    let run = move || {
        let t = work(&worker);
        let _ = tx.send(Box::new(move |app: &mut LightcraftApp, ctx: &egui::Context| done(app, ctx, t)));
        if let Some(c) = repaint {
            c.request_repaint();
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new()
        .name(format!("lc-task-{label}"))
        .spawn(move || {
            // A task the user started and can watch (threading rule 7), like the editor's jobs.
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            run()
        })
        .map_err(|e| format!("{label}: could not start: {e}"))?;
    #[cfg(target_arch = "wasm32")]
    run();
    app.tasks.running.push(Task {
        label: label.to_string(),
        library_generation: app.session.library_generation,
        job: job.clone(),
        seen_at: None,
        rx,
    });
    Ok(job)
}

/// Apply finished tasks (called every frame). A cancelled task's result is discarded.
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    app.tasks.repaint = Some(ctx.clone());
    let now = ctx.input(|i| i.time);
    let mut i = 0;
    while i < app.tasks.running.len() {
        app.tasks.running[i].seen_at.get_or_insert(now);
        if app.tasks.running[i].library_generation != app.session.library_generation {
            // Another library: stop the worker; its result never reaches this catalog.
            app.tasks.running.remove(i).job.cancel();
            continue;
        }
        match app.tasks.running[i].rx.try_recv() {
            Ok(finish) => {
                let task = app.tasks.running.remove(i);
                if task.job.is_cancelled() {
                    app.toast(ctx, format!("{} cancelled", tr(&task.label)));
                } else {
                    finish(app, ctx);
                }
            }
            Err(TryRecvError::Empty) => i += 1,
            Err(TryRecvError::Disconnected) => {
                // the worker died (a panic, already logged)
                let t = app.tasks.running.remove(i);
                app.toast(ctx, format!("{} failed", t.label));
            }
        }
    }
    if !app.tasks.is_empty() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// What the task panel's Cancel stops.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    Task(String),
    Export,
    SmartSort,
    People,
}

struct Row {
    target: Target,
    /// Automation id of the Cancel button.
    automation: String,
    title: String,
    detail: String,
    fraction: Option<f32>,
    stopping: bool,
}

/// `done / total` for a detail line.
fn count(step: Option<(u32, u32)>) -> String {
    step.filter(|s| s.1 > 0).map(|(v, m)| format!("{} / {m}", v.min(m))).unwrap_or_default()
}

/// The photo a Smart Sort / face job is on (its message is the photo id), by file name.
fn photo_name(app: &LightcraftApp, message: &str) -> String {
    message
        .parse::<u64>()
        .ok()
        .and_then(|id| app.session.catalog.photo(lightcraft_catalog::PhotoId(id)))
        .map(|p| p.file_name.clone())
        .unwrap_or_default()
}

fn rows(app: &LightcraftApp, now: f64) -> Vec<Row> {
    let mut rows = Vec::new();
    for t in app.tasks.running.iter().filter(|t| t.seen_at.is_some_and(|s| now - s >= SHOW_AFTER_S)) {
        let status = t.job.status();
        rows.push(Row {
            target: Target::Task(t.label.clone()),
            automation: if t.label == CULLING { "button:cullCancel".into() } else { format!("button:taskCancel:{}", t.label) },
            title: tr(&t.label).to_string(),
            detail: if status.message.is_empty() { count(status.step) } else { status.message },
            fraction: t.job.fraction(),
            stopping: t.job.is_cancelled(),
        });
    }
    if let Some(e) = &app.export {
        let status = e.job.status();
        let done = status.step.map_or(0, |s| s.0 as usize);
        rows.push(Row {
            target: Target::Export,
            automation: "button:exportCancel".into(),
            title: crate::i18n::tr_format!("Exporting {} of {total}", (done + 1).min(e.total), total = e.total),
            detail: status.message,
            fraction: Some(done as f32 / e.total.max(1) as f32),
            stopping: e.job.is_cancelled(),
        });
    }
    // The Smart Sort dialog shows its own progress while it is open.
    if !matches!(app.ui.dialog, Some(crate::state::Dialog::SmartSort { .. })) {
        if let Some(s) = &app.smart_sort {
            let status = s.job.status();
            rows.push(Row {
                target: Target::SmartSort,
                automation: "button:smartSortTaskCancel".into(),
                title: tr("Smart Sort").to_string(),
                detail: [count(status.step), photo_name(app, &status.message)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                fraction: s.job.fraction(),
                stopping: s.job.is_cancelled(),
            });
        }
        if let Some(p) = &app.people_task {
            rows.push(Row {
                target: Target::People,
                automation: "button:peopleTaskCancel".into(),
                title: tr("People").to_string(),
                detail: count(p.job.status().step),
                fraction: p.job.fraction(),
                stopping: p.job.is_cancelled(),
            });
        }
    }
    rows
}

/// The Library's task panel: one row per running background task (culling, export, Smart Sort,
/// faces, slow file-system tasks) with its progress and Cancel, drawn with the shared
/// `li_ui_kit::progress` widget. Results are collected by the polls, not here.
pub fn progress(app: &mut LightcraftApp, ctx: &egui::Context) {
    let rows = rows(app, ctx.input(|i| i.time));
    if rows.is_empty() {
        return;
    }
    let mut cancel = None;
    egui::Window::new(tr("Background Tasks"))
        .id(egui::Id::new("lc-task-panel"))
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -56.0])
        .fixed_size([300.0, 0.0])
        .show(ctx, |ui| {
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                }
                let line = li_ui_kit::progress::JobLine { title: &row.title, detail: &row.detail, fraction: row.fraction, stopping: row.stopping };
                let r = li_ui_kit::progress::job_row(
                    ui,
                    &line,
                    tr(if row.stopping { "Stopping…" } else { "Cancel" }),
                    li_ui_kit::widgets::Profile::Library,
                );
                crate::widgets::register(ctx, row.automation.clone(), r.rect);
                if r.clicked() {
                    cancel = Some(row.target.clone());
                }
            }
        });
    match cancel {
        Some(Target::Task(label)) => {
            app.tasks.cancel(Some(&label));
        }
        Some(Target::Export) => app.export.as_ref().into_iter().for_each(crate::export_task::ExportTask::cancel),
        Some(Target::SmartSort) => app.smart_sort.as_ref().into_iter().for_each(crate::smart_sort_task::SmartSortTask::cancel),
        Some(Target::People) => app.people_task.as_ref().into_iter().for_each(crate::people_task::PeopleTask::cancel),
        None => {}
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

/// Wait for every task and apply it (tests, and commands asked to `wait`). `false` on timeout.
pub fn wait(app: &mut LightcraftApp, ctx: &egui::Context, timeout: std::time::Duration) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let t0 = std::time::Instant::now();
        loop {
            poll(app, ctx);
            if app.tasks.is_empty() {
                return true;
            }
            if t0.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // tasks ran inline: their results are already waiting
        let _ = timeout;
        poll(app, ctx);
        app.tasks.is_empty()
    }
}

/// Start Assisted Culling as a task: photos are measured on the worker (`step` = photos done /
/// total), then scored on the UI thread as one `photo.analyze` step. Cancel leaves the catalog
/// unchanged.
pub fn start_culling(app: &mut LightcraftApp, params: Value) -> Result<Value, String> {
    if app.tasks.is_running(CULLING) {
        return Err("Assisted Culling is already running".into());
    }
    let work = lightcraft_engine::cmd::cull::prepare(&mut app.session, &params);
    spawn_job(
        app,
        CULLING,
        move |job| lightcraft_engine::guard::catch(CULLING, || work.run_job(job)),
        move |app, ctx, result| match result {
            Ok(measured) => match app.session.execute_fn("photo.analyze", |s| lightcraft_engine::cmd::cull::apply(s, measured, &params)) {
                Ok(r) if r["discarded"] != true => {
                    app.toast(
                        ctx,
                        format!(
                            "Assisted Culling complete: {} scored, {} failed",
                            r["photos"].as_array().map_or(0, Vec::len),
                            r["failed"].as_array().map_or(0, Vec::len)
                        ),
                    );
                }
                Ok(_) => {}
                Err(e) => app.toast_error(ctx, e.to_string()),
            },
            Err(e) => app.toast_error(ctx, e),
        },
    )?;
    Ok(json!({"running":true}))
}
#[cfg(test)]
mod bugfix_develop_tests {
    #[test]
    fn bugfix_develop_culling_runs_as_cancellable_background_task() {
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        super::start_culling(&mut app, serde_json::json!({})).unwrap();
        assert!(app.tasks.is_running("Assisted Culling"));
        let revision = app.session.catalog.revision;
        assert_eq!(app.tasks.cancel(Some(super::CULLING)), 1);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        assert!(super::wait(&mut app, &ctx, std::time::Duration::from_secs(20)));
        assert_eq!(app.session.catalog.revision, revision);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod library_origin_tests {
    #[test]
    fn library_origin_drops_delayed_catalog_callbacks() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        let (ready, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (ended, finished) = mpsc::channel();
        let applied = Arc::new(AtomicBool::new(false));
        let seen = applied.clone();
        super::spawn(
            &mut app,
            "Auto Import",
            move || {
                ready.send(()).unwrap();
                wait.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
                ended.send(()).unwrap();
            },
            move |_, _, ()| {
                seen.store(true, Ordering::Relaxed);
            },
        )
        .unwrap();
        started.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        app.session
            .open_library_in(
                lightcraft_engine::library::LibraryStores {
                    dir: "new-library".into(),
                    catalog: Box::new(lightcraft_catalog::MemStore::new()),
                    files: Box::new(lightcraft_catalog::MemStore::new()),
                    on_disk: false,
                },
                false,
            )
            .unwrap();
        super::poll(&mut app, &egui::Context::default());
        assert!(app.tasks.is_empty());
        release.send(()).unwrap();
        finished.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        super::poll(&mut app, &egui::Context::default());
        assert!(!applied.load(Ordering::Relaxed));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod jobs_runner_tests {
    use super::*;

    /// A context with the Library's font stack (the panel's text uses its named weights).
    fn library_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_fonts(li_ui_kit::fonts::definitions(li_ui_kit::fonts::Profile::Library));
        ctx
    }

    /// One frame at UI time `time`: poll the tasks and draw the task panel; returns the
    /// registered automation ids.
    fn frame(app: &mut LightcraftApp, ctx: &egui::Context, time: f64) -> Vec<String> {
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            poll(app, ui.ctx());
            progress(app, ui.ctx());
        });
        output.textures_delta.clear();
        crate::widgets::take_registry(ctx).into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn jobs_runner_cancel_stops_worker_discards_result_and_panel_shows_progress() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        let ctx = library_ctx();
        let applied = std::sync::Arc::new(AtomicBool::new(false));
        let seen = applied.clone();
        let job = spawn_job(
            &mut app,
            "Find Missing Photos",
            |job| {
                job.set_step(3, 10);
                job.set_message("folder-a");
                while !job.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                "stopped early"
            },
            move |_, _, _| seen.store(true, Ordering::Relaxed),
        )
        .unwrap();
        while job.status().step != Some((3, 10)) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(frame(&mut app, &ctx, 1.0).is_empty(), "a new task waits {SHOW_AFTER_S} s before it shows");
        assert!(frame(&mut app, &ctx, 1.0 + SHOW_AFTER_S).contains(&"button:taskCancel:Find Missing Photos".to_string()));
        let row = &rows(&app, 2.0)[0];
        assert_eq!((row.title.as_str(), row.detail.as_str(), row.fraction, row.stopping), ("Find Missing Photos", "folder-a", Some(0.3), false));
        assert_eq!(app.tasks.status()[0]["step"], json!([3, 10]));

        assert_eq!(app.tasks.cancel(Some("Find Missing Photos")), 1);
        assert!(rows(&app, 2.0)[0].stopping, "Cancel turns into Stopping… until the worker returns");
        assert!(wait(&mut app, &ctx, std::time::Duration::from_secs(20)), "the worker saw the cancel");
        assert!(!applied.load(Ordering::Relaxed), "a cancelled task's result is discarded");
        assert!(app.ui.toast.as_ref().unwrap().0.contains("Find Missing Photos cancelled"));
        assert_eq!(app.tasks.cancel(None), 0);
    }

    #[test]
    fn jobs_runner_panel_lists_export_and_culling_with_their_cancel_ids() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        let ctx = library_ctx();
        let (_tx, rx) = channel();
        let export = JobCtl::new();
        export.set_step(1, 4);
        export.set_message("IMG_0002.jpg");
        app.export = Some(crate::export_task::ExportTask::for_test(app.session.library_generation, 4, export.clone(), rx));
        start_culling(&mut app, json!({})).unwrap();
        frame(&mut app, &ctx, 0.0);
        let ids = frame(&mut app, &ctx, 1.0);
        assert!(ids.contains(&"button:exportCancel".to_string()), "{ids:?}");
        let rows = rows(&app, 1.0);
        let e = rows.iter().find(|r| r.target == Target::Export).unwrap();
        assert_eq!((e.title.as_str(), e.detail.as_str(), e.fraction), ("Exporting 2 of 4", "IMG_0002.jpg", Some(0.25)));
        assert_eq!(app.export.as_ref().unwrap().status()["done"], 1);
        app.export.as_ref().unwrap().cancel();
        assert!(export.is_cancelled(), "the panel, the export and the renderer share one job");
        app.tasks.cancel(Some(CULLING));
        assert!(wait(&mut app, &ctx, std::time::Duration::from_secs(20)));
    }
}
