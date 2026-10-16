//! Own exactly one process from reservation through shutdown/reaping.
use std::process::Child;
use std::sync::Mutex;

pub(super) struct OwnedEngine {
    child: Mutex<Option<Child>>,
}
impl OwnedEngine {
    pub(super) const fn new() -> Self {
        Self { child: Mutex::new(None) }
    }

    fn reap(child: &mut Option<Child>) -> std::io::Result<bool> {
        let Some(process) = child.as_mut() else { return Ok(false) };
        if process.try_wait()?.is_none() {
            return Ok(true);
        }
        *child = None;
        Ok(false)
    }

    pub(super) fn is_running(&self) -> std::io::Result<bool> {
        Self::reap(&mut self.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Hold the reservation across spawn: a second start cannot overwrite a live
    /// handle, and stop cannot pass the reservation before ownership is recorded.
    pub(super) fn start(&self, spawn: impl FnOnce() -> Result<(Child, String), String>) -> Result<String, String> {
        let mut child = self.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if Self::reap(&mut child).map_err(|e| format!("Could not check whether the AI engine is running: {e}"))? {
            return Ok("ComfyUI".into());
        }
        let (process, label) = spawn()?;
        *child = Some(process);
        Ok(label)
    }

    pub(super) fn stop(&self) -> bool {
        let mut slot = self.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(Self::reap(&mut slot), Ok(false)) {
            return false;
        }
        let Some(child) = slot.as_mut() else { return false };
        match child.kill() {
            Ok(()) => match child.wait() {
                Ok(_) => {
                    *slot = None;
                    true
                }
                Err(e) => {
                    log::warn!("Could not reap owned AI engine: {e}");
                    false
                }
            },
            Err(e) => {
                // It may have exited between the liveness check and kill. Reap
                // that exit, but retain ownership if stopping a live child failed.
                if matches!(child.try_wait(), Ok(Some(_))) {
                    *slot = None;
                } else {
                    log::warn!("Could not stop owned AI engine: {e}");
                }
                false
            }
        }
    }
}
impl Drop for OwnedEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread;

    fn sleeper() -> Result<(Child, String), String> {
        std::process::Command::new("sleep").arg("60").spawn().map(|child| (child, "test child".into())).map_err(|e| e.to_string())
    }
    #[test]
    fn concurrent_starts_reserve_one_owned_child_and_shutdown_reaps_it() {
        let owned = Arc::new(OwnedEngine::new());
        let starts = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(3));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let (owned, starts, barrier) = (owned.clone(), starts.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    owned
                        .start(|| {
                            starts.fetch_add(1, Ordering::SeqCst);
                            sleeper()
                        })
                        .unwrap()
                })
            })
            .collect();
        barrier.wait();
        for worker in workers {
            assert!(!worker.join().unwrap().is_empty());
        }
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        let pid = owned.child.lock().unwrap().as_ref().unwrap().id();
        assert!(owned.stop());
        assert!(!owned.is_running().unwrap());
        assert!(!owned.stop());
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "owned child was reaped");
    }
    #[test]
    fn failed_and_exited_starts_release_the_slot_without_touching_other_processes() {
        let owned = OwnedEngine::new();
        assert!(owned.start(|| Err("deliberate launch failure".into())).is_err());
        assert!(!owned.is_running().unwrap());
        owned.start(|| std::process::Command::new("sh").args(["-c", "exit 7"]).spawn().map(|c| (c, "exits".into())).map_err(|e| e.to_string())).unwrap();
        owned.child.lock().unwrap().as_mut().unwrap().wait().unwrap();
        // The next start must reap/clear the old handle and actually invoke spawn.
        let spawned = AtomicUsize::new(0);
        owned
            .start(|| {
                spawned.fetch_add(1, Ordering::SeqCst);
                sleeper()
            })
            .unwrap();
        assert_eq!(spawned.load(Ordering::SeqCst), 1);
        let external = OwnedEngine::new();
        external.start(sleeper).unwrap();
        assert!(owned.stop());
        assert!(external.is_running().unwrap(), "unrelated process remains alive");
        assert!(external.stop());
    }
    #[test]
    fn status_reaps_an_exited_child_and_dead_stop_is_idempotent() {
        let owned = OwnedEngine::new();
        owned.start(|| std::process::Command::new("sh").args(["-c", "exit 0"]).spawn().map(|c| (c, "exits".into())).map_err(|e| e.to_string())).unwrap();
        owned.child.lock().unwrap().as_mut().unwrap().wait().unwrap();
        assert!(!owned.is_running().unwrap());
        assert!(owned.child.lock().unwrap().is_none());
        assert!(!owned.stop());
    }
    #[test]
    fn shutdown_during_start_waits_for_ownership_then_reaps_the_child() {
        use std::sync::mpsc;
        let owned = Arc::new(OwnedEngine::new());
        let (entered, starting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let owner = owned.clone();
        let starter = thread::spawn(move || {
            owner
                .start(|| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
                    sleeper()
                })
                .unwrap()
        });
        starting.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        let (attempted, stopping) = mpsc::channel();
        let owner = owned.clone();
        let stopper = thread::spawn(move || {
            attempted.send(()).unwrap();
            owner.stop()
        });
        stopping.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        release.send(()).unwrap();
        starter.join().unwrap();
        assert!(stopper.join().unwrap());
        assert!(!owned.is_running().unwrap());
    }
}
