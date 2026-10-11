//! Serialize the complete GPU lifetime, including context release before
//! libtest returns to the native driver's process-exit handlers.
use std::sync::{Mutex, MutexGuard};

static GPU: Mutex<()> = Mutex::new(());

pub struct Guard {
    _lock: MutexGuard<'static, ()>,
}

pub fn lock() -> Guard {
    Guard { _lock: GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // A hang watchdog, not a GPU performance requirement. Other jobs can
        // saturate the machine; all test-local stages/buffers dropped first.
        let finished = lightcraft_gpu::shutdown(std::time::Duration::from_secs(120));
        assert!(finished || std::thread::panicking(), "GPU test teardown did not finish");
    }
}
