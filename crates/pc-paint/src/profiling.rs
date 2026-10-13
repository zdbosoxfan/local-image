//! Opt-in, debug-build timing scopes for the real app brush path. No thread pool or global lock.
use std::cell::Cell;
use std::time::Instant;
#[derive(Clone, Copy, Debug, Default)]
pub struct StrokeTiming {
    pub primary_dabs: usize,
    pub dual_dabs: usize,
    pub raster_ms: f64,
    pub tail_ms: f64,
    pub restore_ms: f64,
    pub coverage_ms: f64,
    pub composite_ms: f64,
    pub gpu_upload_ms: f64,
    pub worker_ms: f64,
    pub worker_batches: usize,
    pub preview_latency_ms: f64,
}
thread_local! {static ACTIVE:Cell<Option<StrokeTiming>>=const {Cell::new(None)};}
/// Measure only rasterization/coverage accumulation done on this calling thread within `f`.
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, StrokeTiming) {
    struct Restore(Option<StrokeTiming>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.set(self.0);
        }
    }
    let restore = Restore(ACTIVE.replace(Some(StrokeTiming::default())));
    let value = f();
    let timing = ACTIVE.get().unwrap_or_default();
    drop(restore);
    (value, timing)
}
pub(crate) fn start() -> Option<Instant> {
    ACTIVE.with(|a| a.get().map(|_| Instant::now()))
}
pub(crate) fn finish(start: Option<Instant>, primary: usize, dual: usize) {
    if let Some(start) = start {
        ACTIVE.with(|a| {
            if let Some(mut timing) = a.get() {
                timing.primary_dabs += primary;
                timing.dual_dabs += dual;
                timing.raster_ms += start.elapsed().as_secs_f64() * 1000.0;
                a.set(Some(timing));
            }
        });
    }
}

/// Inclusive timing stages on the production stroke/canvas path, enabled only within `measure`.
#[derive(Clone, Copy)]
pub enum Stage {
    Tail,
    Restore,
    Coverage,
    Composite,
    GpuUpload,
}
/// A timing scope; dropping it records the elapsed time even on early returns.
pub struct Span {
    stage: Stage,
    start: Option<Instant>,
}
pub fn span(stage: Stage) -> Span {
    Span { stage, start: start() }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            ACTIVE.with(|a| {
                if let Some(mut t) = a.get() {
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    match self.stage {
                        Stage::Tail => t.tail_ms += ms,
                        Stage::Restore => t.restore_ms += ms,
                        Stage::Coverage => t.coverage_ms += ms,
                        Stage::Composite => t.composite_ms += ms,
                        Stage::GpuUpload => t.gpu_upload_ms += ms,
                    }
                    a.set(Some(t));
                }
            });
        }
    }
}

/// Include a completed worker's measurements in the calling thread's active frame scope.
pub fn record(other: StrokeTiming) {
    ACTIVE.with(|a| {
        if let Some(mut t) = a.get() {
            t.primary_dabs += other.primary_dabs;
            t.dual_dabs += other.dual_dabs;
            t.raster_ms += other.raster_ms;
            t.tail_ms += other.tail_ms;
            t.restore_ms += other.restore_ms;
            t.coverage_ms += other.coverage_ms;
            t.composite_ms += other.composite_ms;
            t.gpu_upload_ms += other.gpu_upload_ms;
            t.worker_ms += other.worker_ms;
            t.worker_batches += other.worker_batches;
            t.preview_latency_ms = t.preview_latency_ms.max(other.preview_latency_ms);
            a.set(Some(t));
        }
    });
}
