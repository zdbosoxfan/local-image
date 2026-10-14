//! Battery policy. OS probes run only on the power monitor, never during UI painting.
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static BATTERY: AtomicBool = AtomicBool::new(false);
static FULL_SPEED: AtomicBool = AtomicBool::new(false);
/// Live override: keep full compute speed and background work on battery.
pub fn set_full_speed(on: bool) {
    FULL_SPEED.store(on, Ordering::Release);
}
#[cfg(test)]
thread_local! { static TEST_SOURCE: std::cell::Cell<Option<(bool, bool)>> = const { std::cell::Cell::new(None) }; }
pub fn on_battery() -> bool {
    #[cfg(test)]
    if let Some((battery, _)) = TEST_SOURCE.with(|s| s.get()) {
        return battery;
    }
    BATTERY.load(Ordering::Acquire)
}
pub fn conserving() -> bool {
    #[cfg(test)]
    if let Some((battery, full)) = TEST_SOURCE.with(|s| s.get()) {
        return battery && !full;
    }
    on_battery() && !FULL_SPEED.load(Ordering::Acquire)
}
pub fn threads(normal: usize) -> usize {
    if conserving() { normal.div_ceil(2).max(1) } else { normal }
}
/// Run CPU work on the reduced shared battery pool, or the caller's existing pool on AC.
/// Rayon cannot resize its global pool; this one lazily allocated economy pool is the runtime
/// power-source exception to the one-compute-pool policy. Nested pixel loops reuse this pool.
/// Call on worker threads, never the UI. No separate pool per model/job.
pub fn compute<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    if !conserving() {
        return f();
    }
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(crate::profile().threads.div_ceil(2).max(1))
            .thread_name(|i| format!("li-battery-{i}"))
            .start_handler(|_| {
                crate::set_current_thread_class(crate::ThreadClass::Utility);
            })
            .build()
            .ok()
    });
    match pool {
        Some(pool) => pool.install(f),
        None => f(),
    }
}
/// Wait on a coordinator OS thread between jobs/batches. Cancellation remains responsive.
/// Never call from the UI or a rayon worker; current work finishes before pausing.
pub fn wait_background(cancelled: &dyn Fn() -> bool) -> bool {
    while conserving() {
        if cancelled() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !cancelled()
}
pub fn init() {
    static START: Once = Once::new();
    START.call_once(|| {
        BATTERY.store(probe().unwrap_or(false), Ordering::Release);
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::thread::Builder::new().name("li-power".into()).spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                if let Some(battery) = probe() {
                    BATTERY.store(battery, Ordering::Release);
                }
            }
        });
    });
}
#[cfg(target_os = "linux")]
fn probe() -> Option<bool> {
    linux_power(std::path::Path::new("/sys/class/power_supply"))
}
#[cfg(target_os = "linux")]
fn linux_power(root: &std::path::Path) -> Option<bool> {
    let mut battery = false;
    let mut discharging = false;
    let mut external = false;
    let mut online = false;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let read = |name| std::fs::read_to_string(entry.path().join(name)).ok();
        if read("type").is_some_and(|v| v.trim() == "Battery") {
            if read("present").is_some_and(|v| v.trim() == "0") {
                continue;
            }
            battery = true;
            discharging |= read("status").is_some_and(|v| v.trim() == "Discharging");
        } else if let Some(v) = read("online") {
            external = true;
            online |= v.trim() == "1";
        }
    }
    Some(battery && !online && (discharging || external))
}
#[cfg(target_os = "macos")]
fn probe() -> Option<bool> {
    let output = std::process::Command::new("/usr/bin/pmset").args(["-g", "batt"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pmset(&String::from_utf8_lossy(&output.stdout))
}
#[cfg(any(target_os = "macos", test))]
fn parse_pmset(text: &str) -> Option<bool> {
    if text.contains("Now drawing from 'Battery Power'") {
        Some(true)
    } else if text.contains("Now drawing from 'AC Power'") {
        Some(false)
    } else {
        None
    }
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn probe() -> Option<bool> {
    None
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn battery_sysfs_hotplug_multiple_sources_and_desktop() {
        let root = std::env::temp_dir().join(format!("li-power-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(linux_power(&root), Some(false));
        let bat = root.join("BAT0");
        std::fs::create_dir_all(&bat).unwrap();
        std::fs::write(bat.join("type"), "Battery\n").unwrap();
        std::fs::write(bat.join("status"), "Discharging\n").unwrap();
        assert_eq!(linux_power(&root), Some(true));
        let ac = root.join("USB");
        std::fs::create_dir_all(&ac).unwrap();
        std::fs::write(ac.join("online"), "1\n").unwrap();
        assert_eq!(linux_power(&root), Some(false));
        std::fs::write(ac.join("online"), "0\n").unwrap();
        assert_eq!(linux_power(&root), Some(true));
        std::fs::write(bat.join("present"), "0\n").unwrap();
        assert_eq!(linux_power(&root), Some(false));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[test]
    fn battery_override_reduced_threads_pause_resume_and_cancel() {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                TEST_SOURCE.with(|s| s.set(None));
            }
        }
        let _reset = Reset;
        TEST_SOURCE.with(|s| s.set(Some((true, false))));
        assert_eq!(threads(7), 4);
        assert_eq!(threads(1), 1);
        assert!(!wait_background(&|| true), "cancel while paused must finish");
        let iterations = std::cell::Cell::new(0);
        assert!(wait_background(&|| {
            iterations.set(iterations.get() + 1);
            TEST_SOURCE.with(|s| s.set(Some((false, false))));
            false
        }));
        assert_eq!(threads(7), 7);
        TEST_SOURCE.with(|s| s.set(Some((true, true))));
        assert_eq!(threads(7), 7);
        assert!(wait_background(&|| false), "override resumes on battery");
    }
    #[test]
    fn mac_power_parser_handles_ac_battery_and_failed_output() {
        assert_eq!(
            parse_pmset(
                "Now drawing from 'Battery Power'
 -InternalBattery-0 80%; discharging;"
            ),
            Some(true)
        );
        assert_eq!(
            parse_pmset(
                "Now drawing from 'AC Power'
 -InternalBattery-0 80%; charging;"
            ),
            Some(false)
        );
        assert_eq!(parse_pmset("could not read power state"), None);
    }
}
