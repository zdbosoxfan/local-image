//! CPU-AI admission and preflight estimates. Initial ranges are conservative work estimates,
//! not hardware benchmarks. Completed runs calibrate the same model on this machine.
//! Model work is estimated in core-seconds; tensor size and the current compute budget scale
//! it. Refusal prevents accidentally submitting minutes of CPU work from an interactive tool.
use anyhow::{Result, ensure};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;
pub const WARN_SECONDS: f64 = 15.0;
pub const REFUSE_SECONDS: f64 = 120.0;
#[derive(Clone, Copy, Debug)]
pub struct Estimate {
    pub seconds: f64,
    pub measured: bool,
}
impl Estimate {
    pub fn allowed(self) -> bool {
        self.seconds <= REFUSE_SECONDS
    }
    pub fn warning(self) -> bool {
        self.seconds >= WARN_SECONDS
    }
    pub fn label(self) -> String {
        let time = if self.seconds < 60.0 {
            format!("about {:.0}–{:.0} s", self.seconds.max(1.0), (self.seconds * 2.0).max(2.0))
        } else {
            format!("about {:.0}–{:.0} min", self.seconds / 60.0, self.seconds / 30.0)
        };
        if !self.allowed() {
            format!("CPU estimate: {time}. Too slow for an interactive CPU job; use a GPU or a smaller input.")
        } else if self.warning() {
            format!("CPU estimate: {time}. This may take a while; cancellation remains available.")
        } else {
            format!("CPU estimate: {time}{}", if self.measured { " (based on recent runs)" } else { " (first run may take longer)" })
        }
    }
    pub fn batch_label(self) -> String {
        let range = if self.seconds < 60.0 {
            format!("{:.0}–{:.0} s", self.seconds.max(1.0), (self.seconds * 2.0).max(2.0))
        } else {
            format!("{:.0}–{:.0} min", self.seconds / 60.0, self.seconds / 30.0)
        };
        format!("Estimated CPU analysis: {range}. Runs in the background; you can cancel at any time.")
    }
    pub fn check(self) -> Result<()> {
        ensure!(self.allowed(), "{}", self.label());
        Ok(())
    }
}
fn observations() -> &'static Mutex<HashMap<String, f64>> {
    static TIMES: OnceLock<Mutex<HashMap<String, f64>>> = OnceLock::new();
    TIMES.get_or_init(Mutex::default)
}
/// Work units are model-input pixels, including crops, tiles or batch items. Never substitute
/// full photo megapixels when a model always uses a fixed-size tensor.
pub fn estimate(id: &str, units: u64) -> Estimate {
    let cores = if lightcraft_sysmem::power::conserving() { 1.0 } else { lightcraft_sysmem::profile().threads.max(1) as f64 };
    let measured = observations().lock().unwrap_or_else(PoisonError::into_inner).get(id).copied();
    let core_seconds_per_mp = measured.unwrap_or(match id {
        "ddcolor-tiny" => 100.0,
        "microsoft-scratches" => 180.0,
        "clip-b32-laion" => 30.0,
        "yunet" | "sface" => 10.0,
        "inspyrenet" => 120.0,
        _ => 60.0,
    });
    Estimate { seconds: (units as f64 / 1_000_000.0 * core_seconds_per_mp / cores.sqrt()).max(0.1), measured: measured.is_some() }
}
/// Record only a successful inference. Failed/cancelled jobs cannot make future estimates
/// look faster. Each observation is normalized to the actual tensor size and compute budget.
pub fn run<R>(id: &str, units: u64, f: impl FnOnce() -> Result<R>) -> Result<R> {
    estimate(id, units).check()?;
    let start = Instant::now();
    let result = crate::cpu_compute(f)?;
    if units > 0 {
        let cores = if lightcraft_sysmem::power::conserving() { 1.0 } else { lightcraft_sysmem::profile().threads.max(1) as f64 };
        let rate = start.elapsed().as_secs_f64() * cores.sqrt() * 1_000_000.0 / units as f64;
        let mut times = observations().lock().unwrap_or_else(PoisonError::into_inner);
        let old = times.entry(id.to_owned()).or_insert(rate);
        *old = *old * 0.5 + rate * 0.5;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_estimate_refuses_before_call_and_uses_successful_observations() {
        assert!(Estimate { seconds: 16.0, measured: false }.warning());
        assert!(!Estimate { seconds: 121.0, measured: false }.allowed());
        assert!(
            run("test-expensive-guard", u64::MAX, || -> Result<()> {
                panic!("must refuse before executing");
            })
            .is_err()
        );
        run("test-calibration", 1, || Ok(())).unwrap();
        assert!(estimate("test-calibration", 1).measured);
        assert!(run("test-failed-calibration", 1, || -> Result<()> { anyhow::bail!("model failed") }).is_err());
        assert!(!estimate("test-failed-calibration", 1).measured);
    }
}
