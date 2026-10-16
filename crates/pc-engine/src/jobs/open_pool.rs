//! Fixed coordinators for file admission. Waiting opens own metadata, never a new OS thread.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

use lightcraft_sysmem::JobCtl;

#[derive(Default)]
pub(super) struct Completion {
    done: Mutex<bool>,
    changed: Condvar,
}
impl Completion {
    pub(super) fn is_finished(&self) -> bool {
        *self.done.lock().unwrap_or_else(PoisonError::into_inner)
    }
    pub(super) fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(PoisonError::into_inner);
        while !*done {
            done = self.changed.wait(done).unwrap_or_else(PoisonError::into_inner);
        }
    }
}
struct Finished(Arc<Completion>);
impl Drop for Finished {
    fn drop(&mut self) {
        *self.0.done.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.0.changed.notify_all();
    }
}
struct Task {
    job: JobCtl,
    work: Box<dyn FnOnce() + Send>,
    _finished: Finished,
}
#[derive(Default)]
struct Queue {
    tasks: Mutex<VecDeque<Task>>,
    ready: Condvar,
    shutdown: AtomicBool,
}
struct Pool(Arc<Queue>);
impl Pool {
    fn new(workers: usize) -> Result<Self, String> {
        let queue = Arc::new(Queue::default());
        let mut started = 0;
        for n in 0..workers.max(1) {
            let queue = queue.clone();
            match std::thread::Builder::new().name(format!("editor-open-{n}")).spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                loop {
                    let task = {
                        let mut tasks = queue.tasks.lock().unwrap_or_else(PoisonError::into_inner);
                        loop {
                            if queue.shutdown.load(Ordering::Acquire) {
                                return;
                            }
                            if let Some(task) = tasks.pop_front() {
                                break task;
                            }
                            tasks = queue.ready.wait(tasks).unwrap_or_else(PoisonError::into_inner);
                        }
                    };
                    if !task.job.is_cancelled() {
                        // The engine work wrapper translates panics into job errors.
                        (task.work)();
                    }
                }
            }) {
                Ok(_) => started += 1,
                Err(e) => log_failure(&e),
            }
        }
        if started == 0 {
            return Err("Could not start a file-opening worker".into());
        }
        Ok(Self(queue))
    }

    fn submit(&self, job: JobCtl, work: Box<dyn FnOnce() + Send>) -> Arc<Completion> {
        let done = Arc::new(Completion::default());
        let mut tasks = self.0.tasks.lock().unwrap_or_else(PoisonError::into_inner);
        tasks.retain(|task| !task.job.is_cancelled());
        tasks.push_back(Task { job, work, _finished: Finished(done.clone()) });
        drop(tasks);
        self.0.ready.notify_one();
        done
    }

    fn discard_cancelled(&self) {
        self.0.tasks.lock().unwrap_or_else(PoisonError::into_inner).retain(|task| !task.job.is_cancelled());
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.0.shutdown.store(true, Ordering::Release);
        self.0.tasks.lock().unwrap_or_else(PoisonError::into_inner).clear();
        self.0.ready.notify_all();
    }
}

fn log_failure(error: &std::io::Error) {
    // No fallback thread per request: a partial pool still makes bounded progress.
    log::warn!("could not start an editor open coordinator: {error}");
}

static POOL: OnceLock<Result<Pool, String>> = OnceLock::new();
pub(super) fn submit(job: JobCtl, work: impl FnOnce() + Send + 'static) -> Result<Arc<Completion>, String> {
    match POOL.get_or_init(|| Pool::new(lightcraft_sysmem::heavy_jobs().max_jobs())) {
        Ok(pool) => Ok(pool.submit(job, Box::new(work))),
        Err(error) => Err(error.clone()),
    }
}
pub(super) fn discard_cancelled() {
    if let Some(Ok(pool)) = POOL.get() {
        pool.discard_cancelled();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn open_queue_bounds_active_workers_and_drops_cancelled_inputs() {
        let pool = Pool::new(1).unwrap();
        let (ready, entered) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let active = pool.submit(
            JobCtl::new(),
            Box::new(move || {
                ready.send(()).unwrap();
                wait.recv().unwrap();
            }),
        );
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let input = Arc::new(vec![0; 1024]);
        let weak = Arc::downgrade(&input);
        let cancelled = JobCtl::new();
        let queued = pool.submit(
            cancelled.clone(),
            Box::new(move || {
                drop(input);
                panic!("cancelled open ran");
            }),
        );
        let (ran, result) = mpsc::channel();
        let next = pool.submit(
            JobCtl::new(),
            Box::new(move || {
                ran.send(()).unwrap();
            }),
        );
        assert!(result.recv_timeout(Duration::from_millis(30)).is_err(), "one coordinator is already occupied");
        cancelled.cancel();
        pool.discard_cancelled();
        assert!(queued.is_finished());
        assert!(weak.upgrade().is_none(), "queued input released before active work finishes");
        release.send(()).unwrap();
        active.wait();
        result.recv_timeout(Duration::from_secs(5)).unwrap();
        next.wait();
    }
}
