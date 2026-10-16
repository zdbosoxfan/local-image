//! All GPU integration tests in one binary. Concurrent wgpu instances in one process segfault on
//! some drivers, so every test that opens a device holds this one process-wide lock (it used to be
//! one lock per test file, which only worked because each file was its own process).
mod device_loss;
mod parity;
mod vector_mask;

pub(crate) static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
