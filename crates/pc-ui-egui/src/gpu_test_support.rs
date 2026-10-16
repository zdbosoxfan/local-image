//! Serialize all offscreen GPU device lifetimes in the UI unit-test binary.
use std::sync::{Mutex, MutexGuard};
static GPU: Mutex<()> = Mutex::new(());
pub fn lock() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
