//! Background tasks for commands whose file-system work can take long on a slow or offline drive
//! (Find Missing Photos walking a folder, listing the auto-import folder…): the work runs on a
//! worker thread and its result is applied on the UI thread between frames, so the window keeps
//! answering. Imports, scans, exports and preview builds have their own tasks with progress.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::LightcraftApp;

/// Applies a task's result to the app (on the UI thread).
type Finish = Box<dyn FnOnce(&mut LightcraftApp, &egui::Context) + Send>;

struct Task {
    label: String,
    rx: Receiver<Finish>,
}

/// The background tasks in flight.
#[derive(Default)]
pub struct Tasks {
    running: Vec<Task>,
    culling: Option<std::sync::Arc<lightcraft_engine::cmd::cull::Control>>,
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
}

/// Run `work` on a worker thread, then `done(app, ctx, result)` on the UI thread (in the browser
/// build, which has no threads, `work` runs at once and `done` on the next frame).
pub fn spawn<T: Send + 'static>(
    app: &mut LightcraftApp,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut LightcraftApp, &egui::Context, T) + Send + 'static,
) -> Result<(), String> {
    let (tx, rx) = channel::<Finish>();
    let repaint = app.tasks.repaint.clone();
    let job = move || {
        let t = work();
        let _ = tx.send(Box::new(move |app: &mut LightcraftApp, ctx: &egui::Context| done(app, ctx, t)));
        if let Some(c) = repaint {
            c.request_repaint();
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name(format!("lc-task-{label}")).spawn(job).map_err(|e| format!("{label}: could not start: {e}"))?;
    #[cfg(target_arch = "wasm32")]
    job();
    app.tasks.running.push(Task { label: label.to_string(), rx });
    Ok(())
}

/// Apply finished tasks (called every frame).
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    app.tasks.repaint = Some(ctx.clone());
    let mut i = 0;
    while i < app.tasks.running.len() {
        match app.tasks.running[i].rx.try_recv() {
            Ok(finish) => {
                app.tasks.running.remove(i);
                finish(app, ctx);
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

/// Paint culling progress once per UI frame; polling/explicit waits only collect results.
pub fn progress(app: &LightcraftApp, ctx: &egui::Context) {
    if let Some(control) = app.tasks.culling.clone() {
        use std::sync::atomic::Ordering;
        egui::Window::new(crate::i18n::tr("Assisted Culling")).collapsible(false).resizable(false).show(ctx, |ui| {
            let completed = control.completed.load(Ordering::Relaxed);
            ui.add(egui::ProgressBar::new(completed as f32 / control.total.max(1) as f32).text(format!("{completed} / {}", control.total)));
            let cancel = ui.button(crate::i18n::tr("Cancel"));
            crate::widgets::register(ctx, "button:cullCancel", cancel.rect);
            if cancel.clicked() {
                control.cancel.store(true, Ordering::Relaxed);
            }
        });
    }
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

pub fn start_culling(app: &mut LightcraftApp, params: serde_json::Value) -> Result<serde_json::Value, String> {
    if app.tasks.culling.is_some() {
        return Err("Assisted Culling is already running".into());
    }
    let work = lightcraft_engine::cmd::cull::prepare(&mut app.session, &params);
    let control = std::sync::Arc::new(lightcraft_engine::cmd::cull::Control { total: work.len(), ..Default::default() });
    let worker_control = control.clone();
    let finish_control = control.clone();
    spawn(
        app,
        "Assisted Culling",
        move || lightcraft_engine::guard::catch("Assisted Culling", || work.run(&worker_control)),
        move |app, ctx, result| {
            app.tasks.culling = None;
            if finish_control.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                app.toast(ctx, "Assisted Culling cancelled");
                return;
            }
            match result {
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
            }
        },
    )?;
    app.tasks.culling = Some(control);
    Ok(serde_json::json!({"running":true}))
}
#[cfg(test)]
mod bugfix_develop_tests {
    #[test]
    fn bugfix_develop_culling_runs_as_cancellable_background_task() {
        let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        super::start_culling(&mut app, serde_json::json!({})).unwrap();
        assert!(app.tasks.is_running("Assisted Culling"));
        let revision = app.session.catalog.revision;
        app.tasks.culling.as_ref().unwrap().cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        assert!(super::wait(&mut app, &ctx, std::time::Duration::from_secs(20)));
        assert_eq!(app.session.catalog.revision, revision);
    }
}
