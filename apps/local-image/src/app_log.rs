//! The app's log: `log` records go to stderr and to a size-capped file, so what happened during a
//! launch from the desktop menu (where stderr is lost) can still be read afterwards.
//!
//! Where the file is:
//! - Linux: `$XDG_DATA_HOME/local-image/logs/local-image.log` (`~/.local/share/local-image/logs`)
//! - macOS: `~/Library/Logs/Local Image/local-image.log`
//! - Windows: `%LOCALAPPDATA%\Local Image\logs\local-image.log`
//! - With `LOCAL_IMAGE_CONFIG_DIR` or in portable mode: `<data dir>/logs/local-image.log`
//!
//! Each launch moves the previous run's log to `local-image.1.log` (so the last two runs are kept),
//! except a process the app re-launched itself after a GPU start-up failure
//! ([`APPEND_ENV`]), which appends to the same file so the whole recovery is in one place. A file
//! that grows past [`MAX_BYTES`] during a run is rotated the same way, so the logs never take more
//! than about twice that.
//!
//! Levels: info and above from the app's own crates and the window/GPU set-up (eframe, egui-wgpu),
//! warnings and errors from everything else. `LOCAL_IMAGE_LOG=debug|info|warn|error|trace` raises
//! or lowers the level for everything. Panics are logged by `crash_guard`'s hook.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The current run's log file name.
pub const LOG_FILE: &str = "local-image.log";
/// The previous run's log file name.
pub const PREVIOUS_LOG_FILE: &str = "local-image.1.log";
/// A log file is rotated once it is larger than this.
pub const MAX_BYTES: u64 = 4 << 20;
/// Set on a process the app starts itself (GPU start-up recovery): append instead of rotating.
pub const APPEND_ENV: &str = "LOCAL_IMAGE_LOG_APPEND";

/// Crate (target) prefixes logged at info level by default.
const INFO_TARGETS: &[&str] = &["local_image", "photocraft", "lightcraft", "li_", "egui_wgpu", "eframe"];

/// The log directory for a data directory chosen as `mode` (see the module docs).
pub fn log_dir(env: &impl Fn(&str) -> Option<OsString>, data: &crate::app_dirs::DataDir) -> Option<PathBuf> {
    use crate::app_dirs::Mode;
    let in_data = || data.dir.as_ref().map(|d| d.join("logs"));
    if data.mode != Mode::Platform {
        return in_data();
    }
    let var = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let platform = if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Logs/Local Image"))
    } else if cfg!(windows) {
        var("LOCALAPPDATA").map(|a| a.join("Local Image").join("logs"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share"))).map(|d| d.join("local-image").join("logs"))
    };
    platform.or_else(in_data)
}

/// The level for `target` under `over` (`LOCAL_IMAGE_LOG`, when set).
fn level_for(target: &str, over: Option<LevelFilter>) -> LevelFilter {
    over.unwrap_or_else(|| if INFO_TARGETS.iter().any(|p| target.starts_with(p)) { LevelFilter::Info } else { LevelFilter::Warn })
}

/// Parse `LOCAL_IMAGE_LOG` leniently (`None` for unset or unknown values).
fn parse_level(v: Option<&str>) -> Option<LevelFilter> {
    v.map(str::trim).filter(|v| !v.is_empty()).and_then(|v| v.parse().ok())
}

/// The file sink: the open file, its path and how much was written.
struct Sink {
    path: PathBuf,
    file: Option<File>,
    written: u64,
}

impl Sink {
    /// Open `dir/local-image.log`, moving an existing one to `.1` first unless `append`.
    fn open(dir: &Path, append: bool) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(LOG_FILE);
        if !append && path.exists() {
            rotate(&path)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Sink { path, file: Some(file), written })
    }

    fn write_line(&mut self, line: &str) {
        if self.written + line.len() as u64 > MAX_BYTES {
            self.file = None;
            let _ = rotate(&self.path);
            self.file = OpenOptions::new().create(true).append(true).open(&self.path).ok();
            self.written = 0;
        }
        if let Some(f) = self.file.as_mut()
            && f.write_all(line.as_bytes()).is_ok()
        {
            self.written += line.len() as u64;
        }
    }
}

/// Move `path` to the previous-run name beside it (replacing an older one).
fn rotate(path: &Path) -> std::io::Result<()> {
    let previous = path.with_file_name(PREVIOUS_LOG_FILE);
    std::fs::rename(path, previous)
}

struct AppLogger {
    over: Option<LevelFilter>,
    sink: Option<Mutex<Sink>>,
    stderr: bool,
}

impl Log for AppLogger {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        m.level() <= level_for(m.target(), self.over)
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_line(SystemTime::now(), record.level(), record.target(), &record.args().to_string());
        if self.stderr {
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
        if let Some(sink) = &self.sink {
            sink.lock().unwrap_or_else(std::sync::PoisonError::into_inner).write_line(&line);
        }
    }

    fn flush(&self) {
        if let Some(sink) = &self.sink
            && let Some(f) = sink.lock().unwrap_or_else(std::sync::PoisonError::into_inner).file.as_mut()
        {
            let _ = f.flush();
        }
    }
}

/// One log line: `2026-10-09 20:10:28.123Z WARN  target: message\n`.
fn format_line(now: SystemTime, level: Level, target: &str, msg: &str) -> String {
    let d = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let (y, mo, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!("{y:04}-{mo:02}-{day:02} {:02}:{:02}:{:02}.{:03}Z {level:<5} {target}: {msg}\n", rem / 3600, rem / 60 % 60, rem % 60, d.subsec_millis())
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Install the logger (once, at the start of `main`). Returns the log file's path when the file
/// could be opened; logging to stderr works either way.
pub fn init() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k);
    let over = parse_level(std::env::var("LOCAL_IMAGE_LOG").ok().as_deref());
    let append = env(APPEND_ENV).is_some_and(|v| !v.is_empty());
    let dir = log_dir(&env, crate::app_dirs::current());
    let (sink, error) = match dir.as_deref().map(|d| Sink::open(d, append)) {
        Some(Ok(s)) => (Some(s), None),
        Some(Err(e)) => (None, Some(e.to_string())),
        None => (None, Some("no user folder found".to_string())),
    };
    let path = sink.as_ref().map(|s| s.path.clone());
    let logger = AppLogger { over, sink: sink.map(Mutex::new), stderr: true };
    let max = over.unwrap_or(LevelFilter::Info).max(LevelFilter::Info);
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max);
    }
    if let Some(e) = error {
        eprintln!("local-image: can't write the log file: {e}");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_dirs::{DataDir, Mode};

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("local-image-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn log_dir_follows_the_data_dir_mode() {
        let env = |k: &str| match k {
            "HOME" => Some(OsString::from("/home/u")),
            "LOCALAPPDATA" => Some(OsString::from("C:\\Users\\u\\AppData\\Local")),
            _ => None,
        };
        let portable = DataDir { dir: Some(PathBuf::from("/usb/LocalImageData")), mode: Mode::Portable, warning: None };
        assert_eq!(log_dir(&env, &portable), Some(PathBuf::from("/usb/LocalImageData/logs")));
        let custom = DataDir { dir: Some(PathBuf::from("/tmp/cfg")), mode: Mode::Override, warning: None };
        assert_eq!(log_dir(&env, &custom), Some(PathBuf::from("/tmp/cfg/logs")));
        let platform = DataDir { dir: Some(PathBuf::from("/home/u/.config/local-image")), mode: Mode::Platform, warning: None };
        let got = log_dir(&env, &platform).unwrap();
        if cfg!(target_os = "linux") {
            assert_eq!(got, PathBuf::from("/home/u/.local/share/local-image/logs"));
            let xdg = |k: &str| if k == "XDG_DATA_HOME" { Some(OsString::from("/data")) } else { env(k) };
            assert_eq!(log_dir(&xdg, &platform), Some(PathBuf::from("/data/local-image/logs")));
        }
        // No home at all: beside the settings.
        assert_eq!(log_dir(&|_: &str| None, &platform), Some(PathBuf::from("/home/u/.config/local-image/logs")));
    }

    #[test]
    fn levels_keep_app_info_and_others_warnings() {
        assert_eq!(level_for("local_image::gpu_startup", None), LevelFilter::Info);
        assert_eq!(level_for("egui_wgpu", None), LevelFilter::Info);
        assert_eq!(level_for("wgpu_hal::vulkan", None), LevelFilter::Warn);
        assert_eq!(level_for("wgpu_hal::vulkan", Some(LevelFilter::Debug)), LevelFilter::Debug);
        assert_eq!(parse_level(Some(" debug ")), Some(LevelFilter::Debug));
        assert_eq!(parse_level(Some("loud")), None);
        assert_eq!(parse_level(Some("")), None);
    }

    #[test]
    fn lines_have_a_utc_timestamp() {
        let t = UNIX_EPOCH + std::time::Duration::from_millis(1_791_590_400_123);
        assert_eq!(format_line(t, Level::Warn, "x", "hi"), "2026-10-10 00:00:00.123Z WARN  x: hi\n");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn each_run_keeps_the_previous_one_and_the_size_is_capped() {
        let dir = temp_dir("rotate");
        let mut s = Sink::open(&dir, false).unwrap();
        s.write_line("run 1\n");
        drop(s);
        let mut s = Sink::open(&dir, false).unwrap();
        s.write_line("run 2\n");
        drop(s);
        // A recovery re-launch appends to the same run.
        let mut s = Sink::open(&dir, true).unwrap();
        s.write_line("run 2, retry\n");
        assert_eq!(std::fs::read_to_string(dir.join(PREVIOUS_LOG_FILE)).unwrap(), "run 1\n");
        assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap(), "run 2\nrun 2, retry\n");
        // Past the cap the file rotates instead of growing.
        let big = "x".repeat(1 << 20);
        for _ in 0..5 {
            s.write_line(&big);
        }
        let len = |n: &str| std::fs::metadata(dir.join(n)).map(|m| m.len()).unwrap_or(0);
        assert!(len(LOG_FILE) <= MAX_BYTES && len(PREVIOUS_LOG_FILE) <= MAX_BYTES, "{} {}", len(LOG_FILE), len(PREVIOUS_LOG_FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
