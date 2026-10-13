//! Thread priority classes (threading rule 7): batch and indexing work runs at a lower OS priority,
//! so the UI thread wins contested time slices instead of a core being left idle. On Apple silicon
//! the class is also what moves work to the efficiency cores (there is no affinity API).
//!
//! | Class | Linux | macOS | Windows |
//! |---|---|---|---|
//! | Interactive | nice 0, SCHED_OTHER | QoS default | normal priority |
//! | Utility | nice +5 | QoS utility | below normal |
//! | Background | nice +10, SCHED_BATCH | QoS background | background mode |
//!
//! Set it on a job's own thread right after it starts, never on a shared thread that later does
//! interactive work (Linux can't take a nice value back without privileges). Failing to set it is
//! not an error: it is logged once and the work runs at normal priority.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::profile::BackgroundPriority;

/// How urgent a thread's work is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadClass {
    /// The UI and what the user is waiting on right now (the OS default).
    Interactive,
    /// Work with a progress bar: export, preview builds, import, AI jobs, editor jobs.
    Utility,
    /// Indexing and housekeeping: smart sort, face indexing, tagging, model downloads.
    Background,
}

/// Preferences › Performance › Background priority: `false` = Normal (skip the lowering).
static LOWER_BACKGROUND: AtomicBool = AtomicBool::new(true);
static FAILURE_LOGGED: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_background_priority(p: BackgroundPriority) {
    LOWER_BACKGROUND.store(p == BackgroundPriority::Low, Ordering::Relaxed);
}

/// Put the calling thread in `class`. Returns whether the OS accepted it (always `true` for
/// Utility/Background when the user chose Normal background priority: nothing to do). On failure it
/// logs once per process and the thread keeps its priority.
pub fn set_current_thread_class(class: ThreadClass) -> bool {
    if class != ThreadClass::Interactive && !LOWER_BACKGROUND.load(Ordering::Relaxed) {
        return true;
    }
    // Threads inherit their creator's nice value / QoS (Linux, macOS): make sure rayon's global pool
    // exists before this thread is lowered, so a lazily started pool can't inherit it.
    let _ = rayon::current_num_threads();
    match apply(class) {
        Ok(()) => true,
        Err(e) => {
            if !FAILURE_LOGGED.swap(true, Ordering::Relaxed) {
                log::warn!("couldn't set the {class:?} thread priority ({e}); work runs at normal priority");
            }
            false
        }
    }
}

#[cfg(target_os = "linux")]
fn apply(class: ThreadClass) -> crate::sys::PriorityResult {
    match class {
        ThreadClass::Interactive => crate::sys::linux::set_current_thread(0, false),
        ThreadClass::Utility => crate::sys::linux::set_current_thread(5, false),
        ThreadClass::Background => crate::sys::linux::set_current_thread(10, true),
    }
}

#[cfg(target_os = "macos")]
fn apply(class: ThreadClass) -> crate::sys::PriorityResult {
    use crate::sys::macos::{QOS_CLASS_BACKGROUND, QOS_CLASS_DEFAULT, QOS_CLASS_UTILITY, set_qos};
    set_qos(match class {
        ThreadClass::Interactive => QOS_CLASS_DEFAULT,
        ThreadClass::Utility => QOS_CLASS_UTILITY,
        ThreadClass::Background => QOS_CLASS_BACKGROUND,
    })
}

#[cfg(target_os = "windows")]
fn apply(class: ThreadClass) -> crate::sys::PriorityResult {
    use crate::sys::windows::{THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL, set_current_thread};
    match class {
        ThreadClass::Interactive => set_current_thread(THREAD_PRIORITY_NORMAL, false),
        ThreadClass::Utility => set_current_thread(THREAD_PRIORITY_BELOW_NORMAL, false),
        ThreadClass::Background => set_current_thread(THREAD_PRIORITY_NORMAL, true),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn apply(_class: ThreadClass) -> crate::sys::PriorityResult {
    Ok(())
}

/// The shared low-priority rayon pool for Background-class fan-out (smart sort, face indexing):
/// the compute budget's size (the global pool's, once [`crate::init`] built it), its workers in
/// [`ThreadClass::Background`]. Under load the OS
/// gives the global (interactive) pool most of the CPU; when the machine is idle background work
/// still gets every core. One pool for the whole process (replacing per-task private pools); `None`
/// if it couldn't be started.
pub fn background_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = crate::profile().threads;
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("li-background-{i}"))
            .start_handler(|_| {
                set_current_thread_class(ThreadClass::Background);
            })
            .build()
            .map_err(|e| log::warn!("couldn't start the background pool ({e}); background work uses the shared pool"))
            .ok()
    })
    .as_ref()
}

/// Run `f` in the [`background_pool`] (its `par_iter`s and tract inference stay there), or on the
/// global pool if that pool couldn't start. Blocks the caller until `f` returns: call it from a
/// job's own thread, never from the UI thread or from inside another pool's task.
pub fn in_background_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match background_pool() {
        Some(pool) => pool.install(f),
        None => f(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This thread's (nice, scheduling policy) from procfs (Linux only).
    #[cfg(target_os = "linux")]
    fn nice_and_policy() -> (i64, i64) {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        let fields: Vec<&str> = stat.rsplit_once(')').unwrap().1.split_whitespace().collect();
        // Fields 19 (nice) and 41 (policy) of proc(5); the list starts at field 3.
        (fields[16].parse().unwrap(), fields[38].parse().unwrap())
    }

    #[test]
    fn priority_classes_apply_to_the_calling_thread_only() {
        let worker = std::thread::spawn(|| {
            let utility = set_current_thread_class(ThreadClass::Utility);
            #[cfg(target_os = "linux")]
            let after_utility = nice_and_policy();
            let background = set_current_thread_class(ThreadClass::Background);
            #[cfg(target_os = "linux")]
            {
                assert!(utility && background);
                assert!(after_utility.0 >= 5, "{after_utility:?}");
                let (nice, policy) = nice_and_policy();
                assert!(nice >= 10, "nice {nice}");
                assert_eq!(policy, 3, "SCHED_BATCH");
            }
            // Asking for Interactive afterwards may be refused (Linux needs privileges to lower
            // nice); it must not panic either way.
            let _ = set_current_thread_class(ThreadClass::Interactive);
            (utility, background)
        });
        let (utility, background) = worker.join().unwrap();
        if cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
            assert!(utility && background);
        }
        #[cfg(target_os = "linux")]
        assert_eq!(nice_and_policy().1, 0, "the test thread itself keeps SCHED_OTHER");
    }

    #[test]
    fn background_pool_workers_run_at_background_priority() {
        let pool = background_pool().expect("background pool");
        assert_eq!(pool.current_num_threads(), crate::profile().threads);
        #[cfg(target_os = "linux")]
        {
            let (nice, policy) = in_background_pool(nice_and_policy);
            assert!(nice >= 10 && policy == 3, "nice {nice} policy {policy}");
        }
        let sum: u64 = in_background_pool(|| {
            use rayon::prelude::*;
            (0..1000u64).into_par_iter().sum()
        });
        assert_eq!(sum, 499_500);
    }
}
