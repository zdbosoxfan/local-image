//! Opt-in, debug-build timing scopes for the real app brush path. No thread pool or global lock.
use std::cell::Cell;
use std::time::Instant;
#[derive(Clone, Copy, Debug, Default)]
pub struct StrokeTiming {
    pub primary_dabs: usize,
    pub dual_dabs: usize,
    pub raster_ms: f64,
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
