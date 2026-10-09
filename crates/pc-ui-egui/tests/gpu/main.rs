//! GPU integration tests for the UI crate, in one binary. Every test here creates a wgpu device
//! (several simulate device loss), and concurrent wgpu instances in one process crash some
//! drivers (#194), so all tests take this one process-wide lock. Kept apart from `it` so the
//! CPU-only tests are not serialised behind it.
mod adjust_preview_gpu;
mod camera_raw_scope;
mod canvas_16f;
mod canvas_flip_gpu;
mod color_managed_canvas;
mod drag_preview_canvas;
mod gpu_canvas_perf;
mod gpu_device_loss;
mod layout_perf;
mod live_stroke_canvas;

static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
