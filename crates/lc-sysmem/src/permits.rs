//! Heavy-job permits: concurrent heavy jobs are limited by memory first, cores
//! second. A batch pipeline (export, Build Previews, import copies) takes a [`Permit`] for each
//! item before its heavy work starts; the permit reserves the item's estimated working memory and
//! one of the [`HeavyJobs::max_jobs`] job slots.
//!
//! Admission is computed, never a fixed count:
//! - job slots: `ceil(C / 4)` clamped to 1..=4 ([`heavy_job_cap`], `C` = the compute threads);
//! - memory: the reserved bytes plus the new item must fit in the [`headroom`], re-read on every
//!   admission: `min(budget − our RSS, available RAM − 1 GB)`, `budget` = the profile's memory
//!   share of RAM (70 % unless the user set one);
//! - at least one job always runs: with nothing reserved an item is admitted whatever its size.
//!
//! A permit can be scaled down as its item shrinks ([`Permit::finish_heavy`] after a render keeps
//! only the bytes of the rendered output, [`Permit::shrink`] after encoding), so the next item's
//! render can start while the previous one is encoded and written. Dropping it releases the rest.
//!
//! Visible-grid thumbnail renders (`JobPool`) never take a permit: they must not queue behind a
//! batch. The process shares one set of permits ([`heavy_jobs`]), so an export and a preview build
//! running together split the same budget instead of each assuming the whole machine.
//!
//! Testing override: `LOCAL_IMAGE_MEMORY_GB=<n>` makes [`heavy_jobs`] plan as if the machine had
//! `n` GB of RAM, about half of it in use by other programs (the documented detection fallback),
//! so a low-memory laptop can be exercised on a large machine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

/// Memory kept free for the operating system and other programs whatever the budget says.
pub const SYSTEM_RESERVE: u64 = 1 << 30;

/// How often a waiting admission re-reads free memory and its stop condition.
const RECHECK: Duration = Duration::from_millis(50);

/// What the memory looks like right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryNow {
    /// RAM available to new work (bytes).
    pub available: u64,
    /// This process's resident size (bytes), when the platform reports it.
    pub rss: Option<u64>,
}

impl MemoryNow {
    /// The live values: [`crate::ram_available`] and [`process_rss`].
    pub fn live() -> MemoryNow {
        MemoryNow { available: crate::ram_available(), rss: process_rss() }
    }
}

/// This process's resident set size in bytes (Linux `VmRSS`; `None` elsewhere, where the budget
/// term then relies on the available-memory term alone).
pub fn process_rss() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        parse_vm_rss(&status)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// `VmRSS:  1234 kB` from `/proc/self/status`, in bytes.
pub fn parse_vm_rss(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

/// Heavy jobs allowed at once by the cores alone: `ceil(C / 4)`, between 1 and 4 (2 cores → 1,
/// 7 → 2, 15 → 4). Memory usually limits further ([`headroom`]).
pub fn heavy_job_cap(threads: usize) -> usize {
    threads.div_ceil(4).clamp(1, 4)
}

/// Bytes new heavy work may still plan for: the memory budget minus what this process already
/// holds, and never more than the available RAM minus [`SYSTEM_RESERVE`].
pub fn headroom(budget: u64, now: MemoryNow) -> u64 {
    let by_budget = now.rss.map_or(budget, |rss| budget.saturating_sub(rss));
    by_budget.min(now.available.saturating_sub(SYSTEM_RESERVE))
}

type MemorySource = Box<dyn Fn() -> MemoryNow + Send + Sync>;

#[derive(Debug, Default)]
struct InFlight {
    jobs: usize,
    bytes: u64,
}

/// A set of heavy-job permits: at most [`HeavyJobs::max_jobs`] items in their heavy phase, and the
/// reserved bytes within the memory headroom (see the module docs).
pub struct HeavyJobs {
    max_jobs: usize,
    budget: u64,
    memory: MemorySource,
    state: Mutex<InFlight>,
    cv: Condvar,
}

impl std::fmt::Debug for HeavyJobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (jobs, bytes) = self.in_flight();
        f.debug_struct("HeavyJobs")
            .field("max_jobs", &self.max_jobs)
            .field("budget", &self.budget)
            .field("jobs", &jobs)
            .field("bytes", &bytes)
            .finish()
    }
}

impl HeavyJobs {
    /// Permits for `max_jobs` (at least 1) heavy jobs within `budget` bytes, reading the memory
    /// from `memory` on each admission (tests pass a mock).
    pub fn new(max_jobs: usize, budget: u64, memory: impl Fn() -> MemoryNow + Send + Sync + 'static) -> HeavyJobs {
        HeavyJobs { max_jobs: max_jobs.max(1), budget, memory: Box::new(memory), state: Mutex::new(InFlight::default()), cv: Condvar::new() }
    }

    /// Permits derived from the compute profile: [`heavy_job_cap`] of its threads, its memory
    /// budget, the live memory (or the `LOCAL_IMAGE_MEMORY_GB` simulation).
    pub fn from_profile() -> HeavyJobs {
        let p = crate::profile();
        let cap = heavy_job_cap(p.threads);
        match simulated_ram() {
            Some(total) => {
                let budget = total / 100 * u64::from(p.memory_percent());
                HeavyJobs::new(cap, budget, move || {
                    let rss = process_rss();
                    MemoryNow { available: (total / 2).saturating_sub(rss.unwrap_or(0)), rss }
                })
            }
            None => HeavyJobs::new(cap, p.memory_budget(), MemoryNow::live),
        }
    }

    /// Heavy jobs allowed at once (memory permitting).
    pub fn max_jobs(&self) -> usize {
        self.max_jobs
    }

    /// Heavy jobs and reserved bytes right now.
    pub fn in_flight(&self) -> (usize, u64) {
        let s = self.lock();
        (s.jobs, s.bytes)
    }

    /// The memory headroom right now (re-read).
    pub fn headroom(&self) -> u64 {
        headroom(self.budget, (self.memory)())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InFlight> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn fits(&self, s: &InFlight, bytes: u64) -> bool {
        s.jobs < self.max_jobs && (s.bytes == 0 || s.bytes.saturating_add(bytes) <= self.headroom())
    }

    /// A permit for a heavy item of about `bytes`, if one is free right now.
    pub fn try_admit(&self, bytes: u64) -> Option<Permit<'_>> {
        let mut s = self.lock();
        self.fits(&s, bytes).then(|| self.grant(&mut s, bytes))
    }

    /// Wait for a permit for a heavy item of about `bytes`. Re-reads the memory while it waits;
    /// returns `None` once `stop` says so (cancel). Blocks the calling thread: call it from a
    /// pipeline's own OS thread, never from inside a rayon task.
    pub fn admit(&self, bytes: u64, stop: &dyn Fn() -> bool) -> Option<Permit<'_>> {
        let mut s = self.lock();
        loop {
            if stop() {
                return None;
            }
            if self.fits(&s, bytes) {
                return Some(self.grant(&mut s, bytes));
            }
            s = self.cv.wait_timeout(s, RECHECK).unwrap_or_else(PoisonError::into_inner).0;
        }
    }

    fn grant(&self, s: &mut InFlight, bytes: u64) -> Permit<'_> {
        s.jobs += 1;
        s.bytes = s.bytes.saturating_add(bytes);
        Permit { owner: self, bytes, heavy: true }
    }

    fn release(&self, jobs: usize, bytes: u64) {
        if jobs == 0 && bytes == 0 {
            return;
        }
        let mut s = self.lock();
        s.jobs = s.jobs.saturating_sub(jobs);
        s.bytes = s.bytes.saturating_sub(bytes);
        drop(s);
        self.cv.notify_all();
    }
}

/// One admitted item: a job slot (until [`Permit::finish_heavy`]) and its reserved bytes (until
/// dropped). `Send`: it travels with its item through a pipeline's stages.
#[derive(Debug)]
pub struct Permit<'a> {
    owner: &'a HeavyJobs,
    bytes: u64,
    heavy: bool,
}

impl Permit<'_> {
    /// The bytes still reserved.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The heavy phase is over (e.g. rendered): give the job slot back and keep only `bytes` (the
    /// result waiting for the next stage) reserved. Never raises the reservation.
    pub fn finish_heavy(&mut self, bytes: u64) {
        let slot = usize::from(std::mem::take(&mut self.heavy));
        let keep = bytes.min(self.bytes);
        self.owner.release(slot, self.bytes - keep);
        self.bytes = keep;
    }

    /// Lower the reservation to `bytes` (never raises it).
    pub fn shrink(&mut self, bytes: u64) {
        let keep = bytes.min(self.bytes);
        self.owner.release(0, self.bytes - keep);
        self.bytes = keep;
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.owner.release(usize::from(self.heavy), self.bytes);
    }
}

/// The process's heavy-job permits ([`HeavyJobs::from_profile`], made on first use), shared by
/// every batch pipeline.
pub fn heavy_jobs() -> &'static HeavyJobs {
    static JOBS: OnceLock<HeavyJobs> = OnceLock::new();
    JOBS.get_or_init(|| {
        let jobs = HeavyJobs::from_profile();
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
            log::info!(
                "heavy jobs: up to {} at once; memory budget {:.1} GB, headroom now {:.1} GB{}",
                jobs.max_jobs,
                gb(jobs.budget),
                gb(jobs.headroom()),
                if simulated_ram().is_some() { " (LOCAL_IMAGE_MEMORY_GB simulation)" } else { "" }
            );
        }
        jobs
    })
}

/// `LOCAL_IMAGE_MEMORY_GB` (testing only): the RAM to plan for instead of the machine's.
fn simulated_ram() -> Option<u64> {
    let gb: f64 = std::env::var("LOCAL_IMAGE_MEMORY_GB").ok()?.trim().parse().ok()?;
    (gb > 0.0 && gb.is_finite()).then_some((gb * (1u64 << 30) as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    const GB: u64 = 1 << 30;
    const MB: u64 = 1 << 20;

    /// Permits over a mocked `ram_available` (and no RSS term) the test can change.
    fn mocked(max_jobs: usize, budget: u64, available: u64) -> (HeavyJobs, Arc<AtomicU64>) {
        let avail = Arc::new(AtomicU64::new(available));
        let a = avail.clone();
        (HeavyJobs::new(max_jobs, budget, move || MemoryNow { available: a.load(Ordering::SeqCst), rss: None }), avail)
    }

    #[test]
    fn job_cap_follows_the_compute_threads() {
        // 2-core laptop, 4c/8t laptop (C = 3), 9800X3D (C = 7), M4 Max (C = 15), Threadripper (C = 31)
        for (threads, cap) in [(1, 1), (2, 1), (3, 1), (4, 1), (5, 2), (7, 2), (8, 2), (12, 3), (15, 4), (31, 4), (63, 4)] {
            assert_eq!(heavy_job_cap(threads), cap, "C = {threads}");
        }
    }

    #[test]
    fn headroom_is_the_smaller_of_budget_and_available_ram() {
        let now = |available, rss| MemoryNow { available, rss };
        // big machine: the budget minus our RSS limits
        assert_eq!(headroom(42 * GB, now(47 * GB, Some(2 * GB))), 40 * GB);
        // little free RAM: available minus the system reserve limits
        assert_eq!(headroom(5 * GB, now(3 * GB, Some(GB))), 2 * GB);
        // no RSS reading: the budget alone
        assert_eq!(headroom(5 * GB, now(16 * GB, None)), 5 * GB);
        // nothing left: zero, never negative
        assert_eq!(headroom(GB, now(GB / 2, Some(2 * GB))), 0);
        assert_eq!(parse_vm_rss("Name:\tx\nVmHWM:\t  900 kB\nVmRSS:\t  512 kB\n"), Some(512 * 1024));
        assert_eq!(parse_vm_rss("Name:\tx\n"), None);
    }

    #[test]
    fn admission_is_limited_by_memory_and_job_slots() {
        // headroom = 4 GB available − 1 GB reserve = 3 GB
        let (jobs, avail) = mocked(4, 64 * GB, 4 * GB);
        let a = jobs.try_admit(2 * GB).expect("first fits");
        assert!(jobs.try_admit(2 * GB).is_none(), "2 + 2 GB > 3 GB headroom");
        let b = jobs.try_admit(GB).expect("2 + 1 GB fits");
        assert_eq!(jobs.in_flight(), (2, 3 * GB));
        // memory freed elsewhere: re-read on the next admission
        avail.store(10 * GB, Ordering::SeqCst);
        let c = jobs.try_admit(2 * GB).expect("headroom grew to 9 GB");
        let d = jobs.try_admit(MB).expect("fourth slot");
        assert!(jobs.try_admit(MB).is_none(), "all four job slots taken");
        drop((a, b, c, d));
        assert_eq!(jobs.in_flight(), (0, 0));
    }

    #[test]
    fn one_job_always_runs_whatever_the_memory() {
        let (jobs, _) = mocked(2, 0, 0);
        let p = jobs.try_admit(100 * GB).expect("nothing reserved: admitted anyway");
        assert!(jobs.try_admit(1).is_none(), "but no second one");
        drop(p);
        assert!(jobs.try_admit(100 * GB).is_some());
    }

    #[test]
    fn a_finished_render_frees_its_slot_and_most_of_its_memory() {
        // one slot (a 2-core machine): the next render may start while the last one is encoded
        let (jobs, _) = mocked(1, 64 * GB, 4 * GB);
        let mut a = jobs.try_admit(2 * GB).unwrap();
        assert!(jobs.try_admit(2 * GB).is_none(), "slot taken");
        a.finish_heavy(200 * MB);
        assert_eq!(jobs.in_flight(), (0, 200 * MB));
        let b = jobs.try_admit(2 * GB).expect("slot free, 0.2 + 2 GB fits in 3 GB");
        a.shrink(10 * MB);
        a.shrink(50 * MB); // never raised
        assert_eq!(a.bytes(), 10 * MB);
        assert_eq!(jobs.in_flight(), (1, 2 * GB + 10 * MB));
        drop(a);
        drop(b);
        assert_eq!(jobs.in_flight(), (0, 0));
    }

    #[test]
    fn a_waiting_admission_wakes_on_release_and_stops_on_cancel() {
        let (jobs, _) = mocked(1, 64 * GB, 64 * GB);
        let held = jobs.try_admit(GB).unwrap();
        let stop = AtomicBool::new(false);
        std::thread::scope(|sc| {
            let waiter = sc.spawn(|| jobs.admit(GB, &|| stop.load(Ordering::SeqCst)).map(|p| p.bytes()));
            std::thread::sleep(Duration::from_millis(20));
            drop(held);
            assert_eq!(waiter.join().unwrap(), Some(GB));
        });
        let held = jobs.try_admit(GB).unwrap();
        std::thread::scope(|sc| {
            let waiter = sc.spawn(|| jobs.admit(GB, &|| stop.load(Ordering::SeqCst)).is_some());
            stop.store(true, Ordering::SeqCst);
            assert!(!waiter.join().unwrap(), "cancel ends the wait without a permit");
        });
        drop(held);
        assert_eq!(jobs.in_flight(), (0, 0));
    }

    #[test]
    fn the_process_permits_come_from_the_profile() {
        let jobs = heavy_jobs();
        assert_eq!(jobs.max_jobs(), heavy_job_cap(crate::profile().threads));
        assert!(jobs.max_jobs() >= 1);
    }
}
