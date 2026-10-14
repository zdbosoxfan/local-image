//! One file-lock sentinel for window and standalone compute device startup.
//!
//! Dropping an unfinished sentinel releases its OS lock and preserves its text so the next
//! process can detect startup failure. Successful startup explicitly calls `finish`.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Bound marker reads even if a previous process left a damaged or unexpectedly large file.
pub const MARKER_MAX_BYTES: u64 = 64 << 10;

/// Owns the marker path and its exclusive OS file lock until startup finishes or fails.
#[derive(Debug)]
pub struct StartupSentinel {
    path: PathBuf,
    file: Option<File>,
}

impl StartupSentinel {
    /// Lock `path`, returning its previous contents and this start's sentinel. A busy lock
    /// returns `Ok(None)` and must never be treated as a crashed previous process.
    pub fn begin(path: &Path) -> io::Result<Option<(String, Self)>> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        let mut previous = Vec::new();
        // Startup recovery is lenient: invalid/truncated text remains a marker, not a fatal
        // application error. The shell can parse partial text as an unknown backend.
        let _ = (&mut file).take(MARKER_MAX_BYTES).read_to_end(&mut previous);
        let previous = String::from_utf8_lossy(&previous).into_owned();
        Ok(Some((previous, Self { path: path.to_path_buf(), file: Some(file) })))
    }

    /// Replace the locked marker contents and flush them before touching the graphics driver.
    pub fn write_text(&mut self, text: &str) -> io::Result<()> {
        let file = self.file.as_mut().ok_or_else(|| io::Error::other("marker closed"))?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
        file.flush()
    }

    /// Clear the marker and release its lock after a successful or deliberately aborted start.
    pub fn finish(mut self) {
        if let Some(file) = self.file.take() {
            // Empty means no crash even if removing fails, e.g. another process has it open.
            let _ = file.set_len(0);
            drop(file);
        }
        let _ = std::fs::remove_file(&self.path);
    }

    /// Consume an existing, unlocked marker once. A live process's marker is left untouched.
    pub fn take_previous(path: &Path) -> io::Result<Option<String>> {
        if !path.exists() {
            return Ok(None);
        }
        let Some((previous, sentinel)) = Self::begin(path)? else { return Ok(None) };
        sentinel.finish();
        Ok(Some(previous))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn marker_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("li-gpu-startup-{name}-{}", std::process::id())).join("gpu-starting.marker")
    }

    #[test]
    fn j12_startup_sentinel_retains_crash_text_and_clears_success() {
        let path = marker_path("cycle");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let (previous, mut sentinel) = StartupSentinel::begin(&path).unwrap().unwrap();
        assert!(previous.is_empty());
        sentinel.write_text("driver startup in progress").unwrap();
        assert!(StartupSentinel::begin(&path).unwrap().is_none());
        assert!(StartupSentinel::take_previous(&path).unwrap().is_none());
        drop(sentinel);
        let (previous, mut sentinel) = StartupSentinel::begin(&path).unwrap().unwrap();
        assert_eq!(previous, "driver startup in progress");
        sentinel.write_text("new startup").unwrap();
        sentinel.finish();
        assert!(!path.exists());
        assert_eq!(StartupSentinel::take_previous(&path).unwrap(), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn j12_startup_sentinel_bounds_previous_marker_reads() {
        let path = marker_path("bounded");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "x".repeat(MARKER_MAX_BYTES as usize + 1024)).unwrap();
        let (previous, sentinel) = StartupSentinel::begin(&path).unwrap().unwrap();
        assert_eq!(previous.len(), MARKER_MAX_BYTES as usize);
        sentinel.finish();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn j12_startup_sentinel_preserves_damaged_marker_as_crash_evidence() {
        let path = marker_path("damaged");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        let (previous, sentinel) = StartupSentinel::begin(&path).unwrap().unwrap();
        assert!(!previous.trim().is_empty(), "invalid UTF-8 still indicates unfinished startup");
        sentinel.finish();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
