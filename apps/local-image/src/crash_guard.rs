//! Last-resort crash guard (never crash).
//!
//! Code must return errors, never panic. This is the safety net for a panic that escapes anyway:
//! the hook logs it, and [`guard`] turns a panic during file import/export into an error the UI
//! shows, so the open documents stay in place. Commands get the same treatment in
//! `Session::execute`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// Crash reports are `crash-<UTC stamp>.txt` in the log folder; only the newest few are kept.
const REPORT_PREFIX: &str = "crash-";
const REPORT_SUFFIX: &str = ".txt";
const KEEP_REPORTS: usize = 10;
/// Holds the file name of the newest report the user has been told about.
const ACK_FILE: &str = "crash-ack";

/// Log every panic (with its location) and save a crash report before the default hook prints it.
pub fn install_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Local Image internal error: {info}");
        // Nothing in here may panic: that would abort the process inside the panic machinery.
        let _ = catch_unwind(AssertUnwindSafe(|| {
            if let Some(dir) = crate::app_log::current_log_dir() {
                let payload = info.payload();
                let message = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("(non-text panic payload)");
                let location = info.location().map_or_else(|| "(unknown)".to_string(), |l| l.to_string());
                let thread = std::thread::current();
                // Release builds ship without symbols, so frames print as addresses (`{:#}`). The
                // anchor's address gives the load slide, which with the separate debug file
                // (.dSYM / .debug) symbolicates them offline: see docs/RELEASE.md.
                let report = format!(
                    "Local Image crash report\nversion: {}\nthread: {}\nlocation: {location}\nmessage: {message}\nanchor: install_hook @ {:#x}\n\nbacktrace:\n{:#}\n",
                    photocraft_engine::build_info::long_version(),
                    thread.name().unwrap_or("(unnamed)"),
                    install_hook as *const () as usize,
                    std::backtrace::Backtrace::force_capture()
                );
                let _ = write_report(&dir, std::time::SystemTime::now(), &report);
            }
        }));
        default(info);
    }));
}

/// Save `report` as `crash-<stamp>.txt` in `dir` and prune older reports; returns the file written.
fn write_report(dir: &Path, now: std::time::SystemTime, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{REPORT_PREFIX}{}{REPORT_SUFFIX}", crate::app_log::file_stamp(now)));
    std::fs::write(&path, report)?;
    let _ = prune_reports(dir, KEEP_REPORTS);
    Ok(path)
}

/// Every crash report in `dir`, oldest first (the stamp sorts like time).
fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(REPORT_PREFIX) && name.ends_with(REPORT_SUFFIX)
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// Delete all but the newest `keep` reports.
fn prune_reports(dir: &Path, keep: usize) -> std::io::Result<()> {
    let all = reports(dir);
    for old in &all[..all.len().saturating_sub(keep)] {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

/// The newest crash report written since the last [`acknowledge_crash_report`], for the start-up
/// notice "open crash report". `None` when there is nothing new.
pub fn newest_unacknowledged_report(dir: &Path) -> Option<PathBuf> {
    let acknowledged = std::fs::read_to_string(dir.join(ACK_FILE)).unwrap_or_default();
    let newest = reports(dir).pop()?;
    (newest.file_name()?.to_string_lossy().as_ref() > acknowledged.trim()).then_some(newest)
}

/// Mark `report` (and every older one) as seen, so it is not offered again.
pub fn acknowledge_crash_report(dir: &Path, report: &Path) -> std::io::Result<()> {
    let name = report.file_name().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a report path"))?;
    std::fs::write(dir.join(ACK_FILE), name.to_string_lossy().as_bytes())
}

/// Run `f`; a panic inside it becomes an `Err` naming `what`.
pub fn guard<T>(what: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(format!("{what} failed with an internal error (logged); your open documents are unchanged")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("local-image-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_791_590_400 + seconds)
    }

    #[test]
    fn only_the_newest_reports_are_kept() {
        let dir = temp_dir("prune");
        std::fs::write(dir.join("local-image.log"), "log").unwrap();
        for n in 0..13 {
            write_report(&dir, at(n), "report").unwrap();
        }
        let kept = reports(&dir);
        assert_eq!(kept.len(), KEEP_REPORTS);
        assert_eq!(kept.first().unwrap().file_name().unwrap(), "crash-20261010-000003-000.txt");
        assert_eq!(kept.last().unwrap().file_name().unwrap(), "crash-20261010-000012-000.txt");
        assert!(dir.join("local-image.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_newest_report_is_offered_until_acknowledged() {
        let dir = temp_dir("ack");
        assert_eq!(newest_unacknowledged_report(&dir), None);
        let first = write_report(&dir, at(1), "one").unwrap();
        let second = write_report(&dir, at(2), "two").unwrap();
        assert_eq!(newest_unacknowledged_report(&dir), Some(second.clone()));
        acknowledge_crash_report(&dir, &second).unwrap();
        assert_eq!(newest_unacknowledged_report(&dir), None);
        // An older, already-seen report never comes back, and acknowledging is idempotent.
        acknowledge_crash_report(&dir, &second).unwrap();
        assert_eq!(newest_unacknowledged_report(&dir), None);
        let third = write_report(&dir, at(3), "three").unwrap();
        assert_eq!(newest_unacknowledged_report(&dir), Some(third));
        assert!(first.exists());
        assert_eq!(newest_unacknowledged_report(&dir.join("missing")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn guard_turns_a_panic_into_an_error() {
        let r: Result<(), String> = guard("Open", || panic!("boom"));
        assert!(r.unwrap_err().contains("Open failed"));
        assert_eq!(guard("Open", || Ok(3)), Ok(3));
        assert_eq!(guard::<()>("Open", || Err("bad file".into())), Err("bad file".into()));
    }
}
