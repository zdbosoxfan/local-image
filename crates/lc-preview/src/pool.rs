//! A prioritized job pool with per-slot de-duplication.
//!
//! Each job targets a *slot* (a grid cell, the loupe…). Submitting for a slot replaces a job for
//! that slot that hasn't started yet, so scrolling or dragging never builds a backlog. At most one
//! job per slot runs at a time: a newer job waits for the running one instead of starting beside
//! it (a slider drag submits a loupe render every frame; renders of one view running side by side
//! only compete for the GPU, the cores and the view's cached stages, and each finishes later than
//! it would alone). Workers take the highest priority first (newest first among equals).
//! Frontends re-prioritize or drop queued jobs when the view changes (e.g. thumbnails scrolled out
//! of view).
//!
//! Native: a fixed set of worker threads, started on first use. wasm32 (no threads): the host
//! calls [`JobPool::run_inline`] once per frame.

use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

pub type Job<R> = Box<dyn FnOnce() -> R + Send>;

struct Entry<S, R> {
    slot: S,
    key: u64,
    priority: u32,
    seq: u64,
    job: Job<R>,
}

/// Queued jobs and the slots whose job is running.
struct State<S, R> {
    queued: Vec<Entry<S, R>>,
    running: Vec<S>,
}

struct Shared<S, R> {
    queue: Mutex<State<S, R>>,
    cv: Condvar,
    shutdown: AtomicBool,
}

/// A finished job.
pub struct Done<S, R> {
    pub slot: S,
    /// The key the job was submitted with (to drop stale results).
    pub key: u64,
    pub result: R,
    /// Run time in milliseconds (0 on wasm).
    pub ms: f64,
}

/// Failed jobs always release their slot and are reported to the owner.
pub struct Failure<S> {
    pub slot: S,
    pub key: u64,
    pub message: String,
}
pub struct JobPool<S, R> {
    shared: Arc<Shared<S, R>>,
    tx: Sender<Done<S, R>>,
    failure_tx: Sender<Failure<S>>,
    failure_rx: Receiver<Failure<S>>,
    rx: Receiver<Done<S, R>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    threads: usize,
    started: bool,
    seq: u64,
    /// The workers, joined on drop (see the `Drop` impl).
    #[cfg(not(target_arch = "wasm32"))]
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl<S: Copy + Eq + Hash + Send + 'static, R: Send + 'static> JobPool<S, R> {
    /// A pool with `threads` workers (none on wasm; `0` = jobs only run via [`JobPool::run_inline`]).
    pub fn new(threads: usize) -> Self {
        let (tx, rx) = channel();
        let (failure_tx, failure_rx) = channel();
        JobPool {
            shared: Arc::new(Shared {
                queue: Mutex::new(State { queued: Vec::new(), running: Vec::new() }),
                cv: Condvar::new(),
                shutdown: AtomicBool::new(false),
            }),
            tx,
            rx,
            failure_tx,
            failure_rx,
            threads,
            started: false,
            seq: 0,
            #[cfg(not(target_arch = "wasm32"))]
            workers: Vec::new(),
        }
    }

    /// Workers for this machine: cores − 1, between 2 and 8.
    pub fn default_threads() -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::available_parallelism().map(|n| n.get().saturating_sub(1)).unwrap_or(4).clamp(2, 8)
        }
        #[cfg(target_arch = "wasm32")]
        {
            1
        }
    }

    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        #[cfg(not(target_arch = "wasm32"))]
        for i in 0..self.threads {
            let shared = self.shared.clone();
            let tx = self.tx.clone();
            let failure_tx = self.failure_tx.clone();
            let spawned = std::thread::Builder::new().name(format!("lc-job-{i}")).spawn(move || {
                while let Some(e) = take(&shared, true) {
                    let _running = Running { shared: &shared, slot: e.slot };
                    let t0 = std::time::Instant::now();
                    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(e.job)) {
                        Ok(result) => result,
                        Err(_) => {
                            let _ = failure_tx.send(Failure { slot: e.slot, key: e.key, message: "Preview decoder failed unexpectedly".into() });
                            continue;
                        }
                    };
                    let done = Done { slot: e.slot, key: e.key, result, ms: t0.elapsed().as_secs_f64() * 1000.0 };
                    if tx.send(done).is_err() {
                        break;
                    }
                }
            });
            if let Ok(h) = spawned {
                self.workers.push(h);
            }
        }
    }

    /// Queue `job` for `slot`, replacing a queued (not yet running) job for the same slot.
    pub fn submit(&mut self, slot: S, key: u64, priority: u32, job: Job<R>) {
        self.start();
        self.seq += 1;
        let (lock, cv) = (&self.shared.queue, &self.shared.cv);
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.queued.retain(|e| e.slot != slot);
        q.queued.push(Entry { slot, key, priority, seq: self.seq, job });
        cv.notify_one();
    }

    /// Change queued jobs' priorities: `f(slot, priority)` returns the new priority, or `None` to
    /// drop the job. Returns the dropped slots.
    pub fn reprioritize(&self, mut f: impl FnMut(&S, u32) -> Option<u32>) -> Vec<S> {
        let mut dropped = Vec::new();
        let mut q = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.queued.retain_mut(|e| match f(&e.slot, e.priority) {
            Some(p) => {
                e.priority = p;
                true
            }
            None => {
                dropped.push(e.slot);
                false
            }
        });
        dropped
    }

    /// Queued (not started) jobs.
    pub fn queued(&self) -> usize {
        self.shared.queue.lock().map(|q| q.queued.len()).unwrap_or(0)
    }

    /// Is a job for `slot` queued?
    pub fn is_queued(&self, slot: S) -> bool {
        self.shared.queue.lock().map(|q| q.queued.iter().any(|e| e.slot == slot)).unwrap_or(false)
    }

    pub fn try_recv_failure(&self) -> Option<Failure<S>> {
        self.failure_rx.try_recv().ok()
    }

    /// A finished job, if any.
    pub fn try_recv(&self) -> Option<Done<S, R>> {
        self.rx.try_recv().ok()
    }

    /// Run up to `n` jobs on the calling thread (wasm, tests). Returns how many ran.
    pub fn run_inline(&mut self, n: usize) -> usize {
        let mut ran = 0;
        while ran < n {
            let Some(e) = take(&self.shared, false) else { break };
            let _running = Running { shared: &self.shared, slot: e.slot };
            let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(e.job)) {
                Ok(result) => result,
                Err(_) => {
                    let _ = self.failure_tx.send(Failure { slot: e.slot, key: e.key, message: "Preview decoder failed unexpectedly".into() });
                    ran += 1;
                    continue;
                }
            };
            let _ = self.tx.send(Done { slot: e.slot, key: e.key, result, ms: 0.0 });
            ran += 1;
        }
        ran
    }
}

/// Pop the best job (highest priority, newest first) whose slot has no job running, and mark its
/// slot running. Blocks when `wait` until one is queued or the pool shuts down.
fn take<S: PartialEq + Copy, R>(shared: &Shared<S, R>, wait: bool) -> Option<Entry<S, R>> {
    let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if shared.shutdown.load(Ordering::Relaxed) {
            return None;
        }
        let State { queued, running } = &mut *q;
        if let Some(i) =
            queued.iter().enumerate().filter(|(_, e)| !running.contains(&e.slot)).max_by_key(|(_, e)| (e.priority, e.seq)).map(|(i, _)| i)
        {
            let e = queued.swap_remove(i);
            running.push(e.slot);
            return Some(e);
        }
        if !wait {
            return None;
        }
        q = shared.cv.wait(q).unwrap_or_else(|e| e.into_inner());
    }
}

/// A job running for `slot`; when it ends (also by a panic) a newer job queued for the slot may
/// start.
struct Running<'a, S: PartialEq, R> {
    shared: &'a Shared<S, R>,
    slot: S,
}

impl<S: PartialEq, R> Drop for Running<'_, S, R> {
    fn drop(&mut self) {
        let mut q = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = q.running.iter().position(|s| *s == self.slot) {
            q.running.swap_remove(i);
        }
        let waiting = q.queued.iter().any(|e| e.slot == self.slot);
        drop(q);
        if waiting {
            // the waiting job may be the only runnable one, and any idle worker can take it
            self.shared.cv.notify_all();
        }
    }
}

impl<S, R> Drop for JobPool<S, R> {
    /// Stops the workers and waits for the jobs running right now (queued ones are dropped).
    ///
    /// Waiting matters: a detached worker still inside a GPU render when its owner (or the whole
    /// process) goes away races the driver's teardown — NVIDIA's driver segfaults in
    /// `libnvidia-glcore` and the GPU raises Xid 13 "Illegal Instruction Encoding".
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Relaxed);
        self.shared.queue.lock().unwrap_or_else(|e| e.into_inner()).queued.clear();
        self.shared.cv.notify_all();
        #[cfg(not(target_arch = "wasm32"))]
        for h in self.workers.drain(..) {
            if h.thread().id() != std::thread::current().id() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod panic_regressions {
    use super::*;
    #[test]
    fn inline_panic_reports_completion_and_releases_slot() {
        let mut pool = JobPool::<u8, u8>::new(0);
        pool.submit(1, 2, 0, Box::new(|| panic!("damaged image")));
        assert_eq!(pool.run_inline(1), 1);
        assert_eq!(pool.try_recv_failure().unwrap().key, 2);
        pool.submit(1, 3, 0, Box::new(|| 7));
        pool.run_inline(1);
        assert_eq!(pool.try_recv().unwrap().result, 7);
    }
    #[test]
    fn native_worker_survives_panic_and_runs_next_job() {
        let mut pool = JobPool::<u8, u8>::new(1);
        pool.submit(1, 2, 0, Box::new(|| panic!("damaged raw")));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while pool.try_recv_failure().is_none() {
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        pool.submit(1, 3, 0, Box::new(|| 7));
        loop {
            if let Some(done) = pool.try_recv() {
                assert_eq!(done.result, 7);
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
    }
}
