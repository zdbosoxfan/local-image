//! Detached decoding for import dialogs. Cancellation discards all unpublished work.
use photocraft_engine::{EngineError, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError},
};

static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::Relaxed);
    }
}
pub(crate) struct Task<T> {
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<T>>,
    started: Option<std::time::Instant>,
}
impl<T: Send + 'static> Task<T> {
    pub fn start(work: impl FnOnce(&AtomicBool) -> Result<T> + Send + 'static) -> Result<Self> {
        RUNNING
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| (n < 4).then_some(n + 1))
            .map_err(|_| EngineError::Other("Other imports are still stopping; try again shortly".into()))?;
        let lease = Lease;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, receiver) = std::sync::mpsc::channel();
        let run = move || {
            let _lease = lease;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&flag)))
                .unwrap_or_else(|_| Err(EngineError::Other("The import decoder failed unexpectedly".into())));
            if !flag.load(Ordering::Relaxed) {
                let _ = tx.send(result);
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::Builder::new().name("file-import".into()).spawn(run).map_err(|e| EngineError::Other(e.to_string()))?;
        #[cfg(target_arch = "wasm32")]
        run();
        #[cfg(not(target_arch = "wasm32"))]
        let started = Some(std::time::Instant::now());
        #[cfg(target_arch = "wasm32")]
        let started = None;
        Ok(Self { cancel, receiver, started })
    }
    pub fn poll(&self) -> Option<Result<T>> {
        if self.started.is_some_and(|start| start.elapsed() > std::time::Duration::from_secs(30)) {
            self.cancel.store(true, Ordering::Relaxed);
            return Some(Err(EngineError::Other("Import took too long; simplify the file and try again".into())));
        }
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(EngineError::Other("The import worker stopped".into()))),
        }
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn test_serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    #[test]
    fn worker_panic_becomes_a_result() {
        let _serial = test_serial();
        let task = Task::<u8>::start(|_| panic!("bad file")).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(result) = task.poll() {
                assert!(result.is_err());
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
    }
    #[test]
    fn cancel_reaches_worker_and_never_publishes() {
        let _serial = test_serial();
        let (tx, rx) = std::sync::mpsc::channel();
        let task = Task::start(move |cancel| {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            tx.send(()).unwrap();
            Ok(7u8)
        })
        .unwrap();
        assert!(task.poll().is_none());
        drop(task);
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    }
}
