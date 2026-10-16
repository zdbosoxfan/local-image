//! One serial worker and one replaceable pending scan per desktop Library bridge.
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use lightcraft_engine::import::ScanProgress;
use lightcraft_ui_egui::import::ScopedScanJob;

#[derive(Default)]
struct Queue {
    pending: Option<ScopedScanJob>,
    active: Option<Arc<ScanProgress>>,
    stopped: bool,
}

#[derive(Default)]
struct State {
    queue: Mutex<Queue>,
    wake: Condvar,
}

pub(super) struct ScanWorker {
    state: Arc<State>,
}

impl ScanWorker {
    pub(super) fn new() -> Result<Arc<Self>, String> {
        let state = Arc::new(State::default());
        let thread = state.clone();
        std::thread::Builder::new().name("automation-library-scan".into()).spawn(move || run(thread)).map_err(|error| error.to_string())?;
        Ok(Arc::new(Self { state }))
    }

    pub(super) fn submit(&self, job: ScopedScanJob) -> Result<(), String> {
        let mut queue = self.state.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.stopped {
            return Err("Library scan worker is closed".into());
        }
        if let Some(active) = &queue.active {
            active.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(previous) = queue.pending.replace(job) {
            previous.progress.cancel.store(true, Ordering::Relaxed);
        }
        self.state.wake.notify_one();
        Ok(())
    }
}

impl Drop for ScanWorker {
    fn drop(&mut self) {
        let mut queue = self.state.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.stopped = true;
        if let Some(active) = &queue.active {
            active.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(pending) = queue.pending.take() {
            pending.progress.cancel.store(true, Ordering::Relaxed);
        }
        self.state.wake.notify_one();
    }
}

fn run(state: Arc<State>) {
    loop {
        let job = {
            let mut queue = state.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            while queue.pending.is_none() && !queue.stopped {
                queue = state.wake.wait(queue).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if queue.stopped {
                return;
            }
            let Some(job) = queue.pending.take() else { continue };
            queue.active = Some(job.progress.clone());
            job
        };
        let result = lightcraft_engine::guard::catch("Library scan", || lightcraft_engine::import::scan_scoped_with(job.input, &job.files, &job.progress));
        let mut queue = state.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !queue.stopped && !job.progress.cancel.load(Ordering::Relaxed) {
            let _ = job.reply.send(result);
        }
        queue.active = None;
    }
}
