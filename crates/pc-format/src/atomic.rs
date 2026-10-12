//! Crash-safe file replacement: the one way PhotoCraft writes a document to disk.
//!
//! [`atomic_write`] never truncates the destination. It writes a temporary file in the **same
//! directory**, flushes it to stable storage (`sync_all`), renames it over the destination and,
//! on Unix, fsyncs the directory so the rename itself survives a power loss. If anything fails,
//! the destination keeps its previous bytes and the temporary file is removed.
//!
//! - **Windows:** antivirus scanners and the search indexer briefly hold files open, which makes
//!   the replacing rename fail with `PermissionDenied`. The rename is retried with exponential
//!   backoff before giving up. (`std::fs::rename` uses `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`,
//!   i.e. replace semantics.)
//! - **Cross-device (`EXDEV`):** cannot happen while the temporary file sits beside the target;
//!   if it does anyway, the save fails with a clear error. It never falls back to writing the
//!   destination in place.
//! - **Unwritable folder:** the save fails with a clear error instead of writing in place.
//!
//! On the web there is no file system: writes go through the platform services (downloads), so
//! this module is never reached there; on `wasm32` every call returns the `std` "unsupported"
//! error.
//!
//! [`atomic_write_with`] takes an [`AtomicFs`] so tests can inject failures at each step.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The file-system steps [`atomic_write_with`] performs. [`RealFs`] is the real thing; tests
/// wrap it to inject failures.
pub trait AtomicFs {
    /// Create `tmp` (it must not exist), write all of `bytes` and `sync_all` it.
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()>;
    /// Create/write/sync a staging file with cooperative cancellation. Hosts which
    /// override this should check between bounded chunks; the default supports test
    /// filesystems while preserving their existing failure-injection implementation.
    fn write_new_interrupted(&self, tmp: &Path, bytes: &[u8], interrupt: &photocraft_raster::Interrupt<'_>) -> io::Result<()> {
        check_interrupt(interrupt)?;
        self.write_new(tmp, bytes)?;
        check_interrupt(interrupt)
    }
    /// Rename `from` over `to`, replacing it.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Remove a (temporary) file.
    fn remove(&self, path: &Path) -> io::Result<()>;
    /// Flush a directory entry change (Unix); a no-op elsewhere.
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;
    /// Copy the destination's permissions onto the temporary file, so replacing a file keeps its
    /// mode. Best effort.
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl AtomicFs for RealFs {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    }
    fn write_new_interrupted(&self, tmp: &Path, bytes: &[u8], interrupt: &photocraft_raster::Interrupt<'_>) -> io::Result<()> {
        check_interrupt(interrupt)?;
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(tmp)?;
        // Native callbacks can unwind too: never leave an owned partial temporary
        // file behind on an error, cancellation or progress-callback panic.
        struct Partial {
            file: Option<std::fs::File>,
            path: Option<PathBuf>,
        }
        impl Drop for Partial {
            fn drop(&mut self) {
                self.file.take(); // Close before unlinking, including on Windows.
                if let Some(path) = self.path.take() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let mut partial = Partial { file: Some(file), path: Some(tmp.to_path_buf()) };
        let file = partial.file.as_mut().ok_or_else(|| io::Error::other("staging file is unavailable"))?;
        let mut written = 0usize;
        for chunk in bytes.chunks(1024 * 1024) {
            check_interrupt(interrupt)?;
            file.write_all(chunk)?;
            written += chunk.len();
            interrupt.progress(0.95 * written as f32 / bytes.len().max(1) as f32);
        }
        check_interrupt(interrupt)?;
        file.sync_all()?;
        check_interrupt(interrupt)?;
        interrupt.progress(1.0);
        check_interrupt(interrupt)?;
        partial.path = None;
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::fs::File::open(dir)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Ok(())
        }
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        let perms = std::fs::metadata(from)?.permissions();
        std::fs::set_permissions(to, perms)
    }
}

/// How hard to retry a rename that fails with `PermissionDenied`.
#[derive(Debug, Clone, Copy)]
pub struct RenameRetry {
    /// Retries after the first attempt (0 = no retry).
    pub attempts: u32,
    /// Delay before the first retry; doubles each time.
    pub first_delay_ms: u64,
}

impl RenameRetry {
    /// The platform default: retry on Windows (about 1.3 s in total), never elsewhere, where
    /// `PermissionDenied` is a real permission problem that waiting won't fix.
    pub fn platform() -> Self {
        if cfg!(windows) { Self { attempts: 7, first_delay_ms: 10 } } else { Self { attempts: 0, first_delay_ms: 0 } }
    }
}

/// Atomically replace (or create) `path` with `bytes`. See the module docs.
///
/// Errors keep their [`io::ErrorKind`] and say what happened and that the original file is
/// unchanged.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(&RealFs, RenameRetry::platform(), path, bytes)
}

/// [`atomic_write`] through an explicit file system and retry policy (the failure-injection seam).
pub fn atomic_write_with(fs: &dyn AtomicFs, retry: RenameRetry, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_symlink(path);
    let name = target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a file path", path.display())))?;
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir).map_err(|e| context(e, format!("cannot create the folder {}", dir.display())))?;

    // Create a fresh temporary file next to the target (same directory, so the rename never
    // crosses a device). The name is unique per process and call; retry a few on collisions.
    let mut tmp = None;
    for _ in 0..8 {
        let candidate = dir.join(temp_name(&name.to_string_lossy()));
        match fs.write_new(&candidate, bytes) {
            Ok(()) => {
                tmp = Some(candidate);
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                // A partial temp file may exist; the destination was never touched.
                let _ = fs.remove(&candidate);
                return Err(write_error(e, path, &dir));
            }
        }
    }
    let tmp = tmp.ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, format!("{}: could not create a temporary file", dir.display())))?;

    // Keep the existing file's mode (e.g. 0600) instead of the process default.
    if target.exists() {
        let _ = fs.copy_permissions(&target, &tmp);
    }

    if let Err(e) = retry_rename(retry, || fs.rename(&tmp, &target)) {
        let _ = fs.remove(&tmp);
        return Err(rename_error(e, path));
    }
    // The new bytes are in place; a failed directory fsync can't be undone and must not report
    // the save as failed, so it is best effort.
    let _ = fs.sync_dir(&dir);
    Ok(())
}

fn check_interrupt(interrupt: &photocraft_raster::Interrupt<'_>) -> io::Result<()> {
    interrupt.check().map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "export cancelled; the original file was not changed"))
}

/// An owned, fully written and synced same-directory temporary file. No destination
/// bytes change until commit; dropping the token discards its staging file.
pub struct StagedWrite {
    fs: std::sync::Arc<dyn AtomicFs + Send + Sync>,
    retry: RenameRetry,
    target: PathBuf,
    requested: PathBuf,
    dir: PathBuf,
    tmp: Option<PathBuf>,
}
impl Drop for StagedWrite {
    fn drop(&mut self) {
        if let Some(tmp) = self.tmp.take() {
            let _ = self.fs.remove(&tmp);
        }
    }
}
impl StagedWrite {
    /// Publish after the caller's final capture/cancellation checks. Bulk writes and
    /// file fsync already happened on the worker. Directory fsync retains atomic_write's
    /// post-rename best-effort semantics: a published file must not be reported unchanged.
    pub fn commit(mut self) -> io::Result<()> {
        let tmp = self.tmp.as_ref().ok_or_else(|| io::Error::other("staged write is unavailable"))?;
        retry_rename(self.retry, || self.fs.rename(tmp, &self.target)).map_err(|error| rename_error(error, &self.requested))?;
        self.tmp = None;
        let _ = self.fs.sync_dir(&self.dir);
        Ok(())
    }
}

/// Prepare a cancellable atomic replacement on a worker, without publishing it.
pub fn stage_write(path: &Path, bytes: &[u8], interrupt: &photocraft_raster::Interrupt<'_>) -> io::Result<StagedWrite> {
    stage_write_with(std::sync::Arc::new(RealFs), RenameRetry::platform(), path, bytes, interrupt)
}

/// Staging's failure-injection seam; every pre-publication error preserves the destination.
pub fn stage_write_with(
    fs: std::sync::Arc<dyn AtomicFs + Send + Sync>,
    retry: RenameRetry,
    path: &Path,
    bytes: &[u8],
    interrupt: &photocraft_raster::Interrupt<'_>,
) -> io::Result<StagedWrite> {
    check_interrupt(interrupt)?;
    let target = resolve_symlink(path);
    let name = target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a file path", path.display())))?;
    let dir = target.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|error| context(error, format!("cannot create the folder {}", dir.display())))?;
    for _ in 0..8 {
        check_interrupt(interrupt)?;
        let tmp = dir.join(temp_name(&name.to_string_lossy()));
        match fs.write_new_interrupted(&tmp, bytes, interrupt) {
            Ok(()) => {
                let staged =
                    StagedWrite { fs: fs.clone(), retry, target: target.clone(), requested: path.to_path_buf(), dir: dir.clone(), tmp: Some(tmp.clone()) };
                check_interrupt(interrupt)?;
                if target.exists() {
                    let _ = fs.copy_permissions(&target, &tmp);
                }
                check_interrupt(interrupt)?;
                return Ok(staged);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                let _ = fs.remove(&tmp);
                return Err(write_error(error, path, &dir));
            }
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{}: could not create a temporary file", dir.display())))
}

/// Run `rename`, retrying with backoff while it fails with `PermissionDenied` (see
/// [`RenameRetry`]). For callers that rename through their own handles (capability directories).
pub fn retry_rename(retry: RenameRetry, mut rename: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut delay = retry.first_delay_ms;
    let mut left = retry.attempts;
    loop {
        match rename() {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && left > 0 => {
                left -= 1;
                sleep_ms(delay);
                delay = delay.saturating_mul(2);
            }
            r => return r,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
#[cfg(target_arch = "wasm32")]
fn sleep_ms(_ms: u64) {}

/// Write through a symlink to the file it points at, instead of replacing the link.
fn resolve_symlink(path: &Path) -> PathBuf {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// A unique hidden temporary file name to write beside `file` before renaming over it.
pub fn temp_name(file: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // Keep the name short: a long file name plus the suffix must stay under NAME_MAX.
    let stem: String = file.chars().take(64).collect();
    format!(".{stem}.{}-{n}.pcsave.tmp", std::process::id())
}

fn context(e: io::Error, msg: String) -> io::Error {
    io::Error::new(e.kind(), format!("{msg}: {e}"))
}

const UNCHANGED: &str = "the original file was not changed";

fn write_error(e: io::Error, path: &Path, dir: &Path) -> io::Error {
    let msg = match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            format!("cannot save {}: the folder {} is not writable ({e}); {UNCHANGED}", path.display(), dir.display())
        }
        io::ErrorKind::StorageFull => format!("cannot save {}: the disk is full ({e}); {UNCHANGED}", path.display()),
        _ => format!("cannot save {}: {e}; {UNCHANGED}", path.display()),
    };
    io::Error::new(e.kind(), msg)
}

fn rename_error(e: io::Error, path: &Path) -> io::Error {
    let msg = if is_cross_device(&e) {
        format!("cannot save {}: the temporary file is on a different volume than the destination ({e}); {UNCHANGED}", path.display())
    } else if e.kind() == io::ErrorKind::PermissionDenied {
        format!("cannot save {}: the file is locked or read-only ({e}); {UNCHANGED}", path.display())
    } else {
        format!("cannot save {}: {e}; {UNCHANGED}", path.display())
    };
    io::Error::new(e.kind(), msg)
}

fn is_cross_device(e: &io::Error) -> bool {
    // EXDEV is 18 on Linux, macOS and the BSDs; ERROR_NOT_SAME_DEVICE is 17 on Windows.
    e.kind() == io::ErrorKind::CrossesDevices || (cfg!(unix) && e.raw_os_error() == Some(18)) || (cfg!(windows) && e.raw_os_error() == Some(17))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod staged_tests;
