//! Serialize engine GPU tests: Vulkan device lifetimes overlap otherwise.
use std::sync::{Mutex, MutexGuard};
static GPU: Mutex<()> = Mutex::new(());
pub fn lock() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
