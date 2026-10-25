//! One file-publication backend for documents, catalogs, settings, models and caches.
//!
//! Same-directory, exclusively created staging files protect the destination until commit.
//! [`Policy::Durable`] flushes bytes before publication and attempts a directory sync after it;
//! [`Policy::Atomic`] skips both syncs for recreatable outputs. A directory-sync failure after
//! publication cannot roll back the rename, so commit succeeds (use [`sync_dir_checked`] when
//! a transaction must explicitly verify directory durability before pruning backups).
//!
//! Replacement follows destination symlinks and keeps existing permissions. New-file publication
//! prefers atomic no-replace rename, then hard links. Volumes supporting neither use exclusive
//! final-name creation and a synced, verified streaming copy: readers may briefly see partial
//! new content, but existing files are never overwritten. Owned files are cleaned on failures.
//! [`AtomicFs`] retains the editor's injectable filesystem seam and supports capability adapters.
#![expect(clippy::let_underscore_must_use, clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed"))]

use lightcraft_sysmem::jobs::Interrupt;
pub mod read;
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// Whether publication must flush stable storage or only prevent partial replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Durable,
    Atomic,
}

thread_local! {
    static FAIL_AFTER: Cell<Option<u64>> = const { Cell::new(None) };
    static SYNCS: Cell<u64> = const { Cell::new(0) };
    static FAIL_PUBLISHED_CLEANUP: Cell<bool> = const { Cell::new(false) };
    static FORCE_COPY_PUBLICATION: Cell<bool> = const { Cell::new(false) };
}
#[doc(hidden)]
pub fn syncs_on_this_thread() -> u64 {
    SYNCS.with(Cell::get)
}
fn sync_file(file: &File) -> io::Result<()> {
    SYNCS.with(|n| n.set(n.get() + 1));
    file.sync_all()
}
/// Thread-local fault injection shared by every compatibility API.
#[doc(hidden)]
pub fn fail_writes_after(bytes: u64) -> FaultGuard {
    FaultGuard(FAIL_AFTER.with(|limit| limit.replace(Some(bytes))))
}
#[doc(hidden)]
pub struct FaultGuard(Option<u64>);
impl Drop for FaultGuard {
    fn drop(&mut self) {
        FAIL_AFTER.with(|limit| limit.set(self.0));
    }
}

/// Inject an error after exclusive new-file publication, for recovery-record tests.
#[doc(hidden)]
pub fn fail_published_cleanup() -> PublicationFaultGuard {
    PublicationFaultGuard(FAIL_PUBLISHED_CLEANUP.with(|flag| flag.replace(true)))
}
#[doc(hidden)]
pub struct PublicationFaultGuard(bool);
impl Drop for PublicationFaultGuard {
    fn drop(&mut self) {
        FAIL_PUBLISHED_CLEANUP.with(|flag| flag.set(self.0));
    }
}

/// Exercise copy-only volumes at the publication boundary, without changing direct
/// rename/link operations used to capture files for recovery.
#[doc(hidden)]
pub fn force_copy_publication() -> CopyPublicationGuard {
    CopyPublicationGuard(FORCE_COPY_PUBLICATION.with(|flag| flag.replace(true)))
}
#[doc(hidden)]
pub struct CopyPublicationGuard(bool);
impl Drop for CopyPublicationGuard {
    fn drop(&mut self) {
        FORCE_COPY_PUBLICATION.with(|flag| flag.set(self.0));
    }
}

struct Limited<W> {
    inner: W,
    written: u64,
    limit: Option<u64>,
}
impl<W: Write> Write for Limited<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let room = self.limit.map(|n| usize::try_from(n.saturating_sub(self.written)).unwrap_or(usize::MAX)).unwrap_or(bytes.len());
        if room == 0 && !bytes.is_empty() {
            return Err(io::Error::other("write failed (injected fault)"));
        }
        let count = self.inner.write(&bytes[..room.min(bytes.len())])?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Filesystem operations for shared staging and publication. Legacy implementations remain valid.
pub trait AtomicFs {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()>;
    fn write_new_interrupted(&self, tmp: &Path, bytes: &[u8], interrupt: &Interrupt<'_>) -> io::Result<()> {
        check_interrupt(interrupt)?;
        self.write_new(tmp, bytes)?;
        check_interrupt(interrupt)
    }
    /// Publish a second name without replacement. Unsupported by legacy adapters unless overridden.
    fn link_new(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "filesystem does not support exclusive hard-link publication"))
    }
    /// Rename without replacement; a missing OS/filesystem operation returns Unsupported.
    fn rename_new(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "atomic no-replace rename is unavailable"))
    }
    /// Exclusive final-name creation plus synced, verified bounded streaming copy.
    /// Implementations own cleanup of their newly created destination on any failure.
    fn copy_new_verified(&self, _from: &Path, _to: &Path, _interrupt: &Interrupt<'_>) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "exclusive verified copy is unavailable"))
    }
    /// As [`Self::copy_new_verified`], returning the original exclusive-create handle.
    /// Reopening the published path is not sufficient: it may already name another file.
    /// Legacy adapters need not support receipts for ordinary new-file publication.
    fn copy_new_verified_handle(&self, _from: &Path, _to: &Path, _interrupt: &Interrupt<'_>) -> io::Result<File> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "exclusive verified copy receipts are unavailable"))
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Resolve the target and prepare its parent. Capability adapters override this to avoid
    /// ambient filesystem access; their held directory handle performs all path resolution.
    fn prepare_target(&self, path: &Path) -> io::Result<PathBuf> {
        let target = resolve_symlink(path)?;
        validate_target(&target)?;
        fs::create_dir_all(parent_of(&target))?;
        Ok(target)
    }
    /// Preserve mode and flush its metadata before publication. Override when handles must
    /// stay open across permission changes (e.g. a write-only or unreadable destination).
    fn preserve_permissions(&self, from: &Path, to: &Path, policy: Policy) -> io::Result<()> {
        self.copy_permissions(from, to)?;
        if policy == Policy::Durable {
            self.sync_temp(to)?;
        }
        Ok(())
    }
    /// Explicit installation permissions; unsupported by legacy/custom adapters unless overridden.
    fn set_permissions(&self, _tmp: &Path, _permissions: fs::Permissions, _policy: Policy) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "filesystem does not support explicit installation permissions"))
    }
    /// Flush permission changes after the content was staged. Legacy fake filesystems may ignore it.
    fn sync_temp(&self, _tmp: &Path) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;
impl AtomicFs for RealFs {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
        self.write_new_interrupted(tmp, bytes, &Interrupt::NONE)
    }
    fn write_new_interrupted(&self, tmp: &Path, bytes: &[u8], interrupt: &Interrupt<'_>) -> io::Result<()> {
        check_interrupt(interrupt)?;
        write_stream_to(tmp, Policy::Durable, false, &mut |w| {
            let mut written = 0;
            for chunk in bytes.chunks(1 << 20) {
                check_interrupt(interrupt)?;
                w.write_all(chunk)?;
                written += chunk.len();
                interrupt.progress(0.95 * written as f32 / bytes.len().max(1) as f32);
            }
            check_interrupt(interrupt)
        })?;
        // write_stream_to owns cleanup while callbacks can unwind; the caller owns the file
        // after it returns, including cancellation or panic in the final progress callback.
        let mut cleanup = Temp { fs: self, path: Some(tmp.to_path_buf()) };
        check_interrupt(interrupt)?;
        interrupt.progress(1.0);
        check_interrupt(interrupt)?;
        cleanup.path = None;
        Ok(())
    }
    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::hard_link(from, to)
    }
    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            rustix::fs::renameat_with(rustix::fs::CWD, from, rustix::fs::CWD, to, rustix::fs::RenameFlags::NOREPLACE).map_err(Into::into)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // std::fs::rename uses replacement semantics on Windows too. Never substitute it
            // for a no-replace syscall: use hard links or exclusive creation on this platform.
            let _ = (from, to);
            Err(io::Error::new(io::ErrorKind::Unsupported, "atomic no-replace rename is unavailable"))
        }
    }
    fn copy_new_verified(&self, from: &Path, to: &Path, interrupt: &Interrupt<'_>) -> io::Result<()> {
        self.copy_new_verified_handle(from, to, interrupt).map(drop)
    }
    fn copy_new_verified_handle(&self, from: &Path, to: &Path, interrupt: &Interrupt<'_>) -> io::Result<File> {
        copy_new_verified_handle(from, to, interrupt)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        let result = fs::remove_file(path);
        #[cfg(windows)]
        if result.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied) {
            // Only owned staging files reach this cleanup hook. Clear the inherited Windows
            // readonly bit so cancellation/drop can unlink them without changing the destination.
            let metadata = fs::symlink_metadata(path)?;
            if metadata.is_file() && !metadata.file_type().is_symlink() && metadata.permissions().readonly() {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                fs::set_permissions(path, permissions)?;
                return fs::remove_file(path);
            }
        }
        result
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        sync_dir_checked(dir)
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::set_permissions(to, fs::metadata(from)?.permissions())
    }
    fn sync_temp(&self, tmp: &Path) -> io::Result<()> {
        sync_file(&OpenOptions::new().write(true).open(tmp)?)
    }
    fn preserve_permissions(&self, from: &Path, to: &Path, policy: Policy) -> io::Result<()> {
        self.set_permissions(to, fs::metadata(from)?.permissions(), policy)
    }
    fn set_permissions(&self, tmp: &Path, permissions: fs::Permissions, policy: Policy) -> io::Result<()> {
        // Acquire a writable handle before applying potentially unreadable/read-only permissions.
        let file = OpenOptions::new().write(true).open(tmp)?;
        file.set_permissions(permissions)?;
        if policy == Policy::Durable {
            sync_file(&file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenameRetry {
    pub attempts: u32,
    pub first_delay_ms: u64,
}
impl RenameRetry {
    pub fn platform() -> Self {
        if cfg!(windows) { Self { attempts: 7, first_delay_ms: 10 } } else { Self { attempts: 0, first_delay_ms: 0 } }
    }
}
pub fn retry_rename(retry: RenameRetry, mut rename: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut delay = retry.first_delay_ms;
    let mut left = retry.attempts;
    loop {
        match rename() {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied && left > 0 => {
                left -= 1;
                #[cfg(not(target_arch = "wasm32"))]
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay = delay.saturating_mul(2);
            }
            result => return result,
        }
    }
}

struct Temp<'a> {
    fs: &'a dyn AtomicFs,
    path: Option<PathBuf>,
}
impl Drop for Temp<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = self.fs.remove(&path);
        }
    }
}

/// Fully written staging file; dropping it discards the unpublished content.
pub struct StagedWrite {
    fs: Arc<dyn AtomicFs + Send + Sync>,
    retry: RenameRetry,
    target: PathBuf,
    requested: PathBuf,
    tmp: Option<PathBuf>,
    policy: Policy,
    length: u64,
}
impl Drop for StagedWrite {
    fn drop(&mut self) {
        if let Some(tmp) = self.tmp.take() {
            let _ = self.fs.remove(&tmp);
        }
    }
}
impl StagedWrite {
    /// Inspect/verify the completed file before publication. Do not modify its bytes.
    pub fn path(&self) -> &Path {
        self.tmp.as_deref().expect("live staging file")
    }
    pub fn len(&self) -> u64 {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    /// Set an explicit installation mode before publishing (e.g. an executable launcher).
    pub fn set_permissions(&self, permissions: fs::Permissions) -> io::Result<()> {
        self.fs.set_permissions(self.path(), permissions, self.policy)
    }
    pub fn commit(mut self) -> io::Result<()> {
        let tmp = self.path();
        retry_rename(self.retry, || self.fs.rename(tmp, &self.target)).map_err(|e| rename_error(e, &self.requested))?;
        self.tmp = None;
        if self.policy == Policy::Durable {
            let _ = self.fs.sync_dir(parent_of(&self.target));
        }
        Ok(())
    }
}
fn check_interrupt(interrupt: &Interrupt<'_>) -> io::Result<()> {
    interrupt.check().map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "export cancelled; the original file was not changed"))
}

// Shared preparation for both borrowed filesystem writes and owned worker staging.
fn prepare(fs: &dyn AtomicFs, path: &Path, bytes: &[u8], interrupt: &Interrupt<'_>) -> io::Result<(PathBuf, PathBuf)> {
    check_interrupt(interrupt)?;
    let target = fs.prepare_target(path)?;
    validate_target(&target)?;
    let dir = parent_of(&target);
    for _ in 0..8 {
        check_interrupt(interrupt)?;
        let tmp = dir.join(temp_name(&target.file_name().unwrap_or_default().to_string_lossy()));
        match fs.write_new_interrupted(&tmp, bytes, interrupt) {
            Ok(()) => {
                let mut cleanup = Temp { fs, path: Some(tmp.clone()) };
                check_interrupt(interrupt)?;
                // Missing targets are expected; preserve any existing mode and flush that change.
                match fs.preserve_permissions(&target, &tmp, Policy::Durable) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(write_error(e, path, dir)),
                }
                check_interrupt(interrupt)?;
                cleanup.path = None;
                return Ok((target, tmp));
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                let _ = fs.remove(&tmp);
                return Err(write_error(e, path, dir));
            }
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{}: could not create a temporary file", dir.display())))
}
/// Refuse to replace a file marked read-only (`chmod 444`, the Windows read-only attribute). A
/// rename only needs the folder to be writable, so without this Linux and macOS would silently
/// replace a file the user locked while Windows refuses. Document saves check this first; a
/// missing file, or one behind a symlink to a writable file, is fine.
pub fn refuse_read_only(path: &Path) -> io::Result<()> {
    if fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().readonly()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("cannot save {}: the file is read-only and was not changed; save a copy with Save As", path.display()),
        ));
    }
    Ok(())
}
/// Editor-compatible writer, including automatic parent creation.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(&RealFs, RenameRetry::platform(), path, bytes)
}
/// As [`atomic_write`] for secrets (API keys, tokens): on unix the staging file is created owner-only
/// (0600) before any bytes are written and the published file ends up 0600 even when it replaces a
/// looser one. Other platforms behave like [`atomic_write`].
pub fn atomic_write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(&PrivateFs, RenameRetry::platform(), path, bytes)
}
/// [`RealFs`] with owner-only staging files that never inherit the destination's mode.
struct PrivateFs;
impl AtomicFs for PrivateFs {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
        write_stream_to(tmp, Policy::Durable, true, &mut |w| w.write_all(bytes)).map(drop)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        RealFs.remove(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        RealFs.sync_dir(dir)
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.copy_permissions(from, to)
    }
    fn sync_temp(&self, tmp: &Path) -> io::Result<()> {
        RealFs.sync_temp(tmp)
    }
    fn preserve_permissions(&self, from: &Path, to: &Path, policy: Policy) -> io::Result<()> {
        if cfg!(unix) { Ok(()) } else { RealFs.preserve_permissions(from, to, policy) }
    }
}
pub fn atomic_write_with(fs: &dyn AtomicFs, retry: RenameRetry, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (target, tmp) = prepare(fs, path, bytes, &Interrupt::NONE)?;
    let mut cleanup = Temp { fs, path: Some(tmp.clone()) };
    retry_rename(retry, || fs.rename(&tmp, &target)).map_err(|e| rename_error(e, path))?;
    cleanup.path = None;
    let _ = fs.sync_dir(parent_of(&target));
    Ok(())
}
pub fn stage_write(path: &Path, bytes: &[u8], interrupt: &Interrupt<'_>) -> io::Result<StagedWrite> {
    stage_write_with(Arc::new(RealFs), RenameRetry::platform(), path, bytes, interrupt)
}
pub fn stage_write_with(
    fs: Arc<dyn AtomicFs + Send + Sync>,
    retry: RenameRetry,
    path: &Path,
    bytes: &[u8],
    interrupt: &Interrupt<'_>,
) -> io::Result<StagedWrite> {
    let (target, tmp) = prepare(fs.as_ref(), path, bytes, interrupt)?;
    Ok(StagedWrite { fs, retry, target, requested: path.to_path_buf(), tmp: Some(tmp), policy: Policy::Durable, length: bytes.len() as u64 })
}

/// Catalog-compatible durable replacement; the parent must already exist.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, &mut |w| w.write_all(bytes)).map(|_| ())
}
pub fn write_atomic_with(path: &Path, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<u64> {
    let staged = stage_stream(path, Policy::Durable, fill)?;
    let length = staged.len();
    staged.commit()?;
    Ok(length)
}
pub fn write_atomic_nosync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    stage_stream(path, Policy::Atomic, &mut |w| w.write_all(bytes))?.commit()
}
/// Stream bounded-memory content into owned staging, then verify/check cancellation and commit.
/// The parent must exist. Replacement keeps existing symlinks and permissions.
pub fn stage_stream(path: &Path, policy: Policy, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<StagedWrite> {
    let target = resolve_symlink(path)?;
    validate_target(&target)?;
    let (tmp, length) = stream_temp(&target, policy, fill)?;
    let staged = StagedWrite { fs: Arc::new(RealFs), retry: RenameRetry::platform(), target, requested: path.to_path_buf(), tmp: Some(tmp), policy, length };
    match RealFs.preserve_permissions(&staged.target, staged.path(), policy) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(staged)
}
fn write_stream_to(tmp: &Path, policy: Policy, private: bool, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<u64> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let file = options.open(tmp)?;
    write_owned_stream(file, tmp, policy, fill)
}
fn write_owned_stream(file: File, tmp: &Path, policy: Policy, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<u64> {
    write_owned_stream_held(file, tmp, policy, fill).map(|(length, _)| length)
}
fn write_owned_stream_held(file: File, tmp: &Path, policy: Policy, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<(u64, File)> {
    let mut cleanup = Temp { fs: &RealFs, path: Some(tmp.to_path_buf()) };
    let mut writer = Limited { inner: io::BufWriter::with_capacity(1 << 20, file), written: 0, limit: FAIL_AFTER.with(Cell::get) };
    fill(&mut writer)?;
    writer.flush()?;
    let length = writer.written;
    let file = writer.inner.into_inner().map_err(|e| e.into_error())?;
    if policy == Policy::Durable {
        sync_file(&file)?;
    }
    cleanup.path = None;
    Ok((length, file))
}
fn stream_temp(path: &Path, policy: Policy, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<(PathBuf, u64)> {
    stream_temp_held(path, policy, fill, false).map(|(path, length, _)| (path, length))
}
fn stream_temp_held(path: &Path, policy: Policy, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>, readable: bool) -> io::Result<(PathBuf, u64, File)> {
    validate_target(path)?;
    for _ in 0..1000 {
        let tmp = parent_of(path).join(temp_name(&path.file_name().unwrap_or_default().to_string_lossy()));
        // Only exclusive creation collisions are retried. Errors from the fill callback
        // (including AlreadyExists) abort without replaying an encoder or downloader.
        match OpenOptions::new().read(readable).write(true).create_new(true).open(&tmp) {
            Ok(file) => return write_owned_stream_held(file, &tmp, policy, fill).map(|(length, file)| (tmp, length, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free temporary name"))
}
fn parent_of(path: &Path) -> &Path {
    path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
}
fn validate_target(path: &Path) -> io::Result<()> {
    if path.file_name().is_none() { Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a file path", path.display()))) } else { Ok(()) }
}
fn resolve_symlink(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(e) => Err(e),
    }
}
/// Short unique name with a bounded target hint; also recognized by legacy cleanup tools.
pub fn temp_name(file: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // A bounded ASCII target hint lets cache cleanup distinguish its files from unrelated
    // writers, and keeps even non-ASCII/NAME_MAX destinations well below the name limit.
    let hint: String = file.chars().take(64).map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    format!(".lightcraft-{}-{}.{hint}.pcsave.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}
pub fn sync_dir(dir: &Path) {
    let _ = sync_dir_checked(dir);
}
pub fn sync_dir_checked(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        sync_file(&File::open(dir)?)
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Publish a fully verified new file, without replacing any existing file or symlink.
/// Errors before publication preserve existing names. A later cleanup error is explicitly
/// marked by [`was_published`]; the complete new destination then already exists.
pub fn write_new(path: &Path, data: &[u8]) -> io::Result<()> {
    write_new_unique(&mut std::iter::once(path.to_path_buf()), data).map(|_| ())
}
pub fn write_new_with(path: &Path, fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>, verify: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    write_new_with_interrupted(path, fill, verify, &Interrupt::NONE)
}
/// Publish a verified new file and retain the file actually created by this write.
/// The read/write handle is held across rename/link publication, or is the original
/// exclusive-create handle when publication requires copying. It is never obtained by
/// reopening the destination, which may already have been replaced by another writer.
/// The handle's cursor position is unspecified; seek before reading its contents.
pub fn write_new_with_receipt(
    path: &Path,
    fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    verify: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<File> {
    write_new_with_inner(path, fill, verify, &Interrupt::NONE, true)?.ok_or_else(|| io::Error::other("publication did not retain its file handle"))
}
/// Verified new-file write with cancellation during fallback publication as well as before commit.
pub fn write_new_with_interrupted(
    path: &Path,
    fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    verify: &mut dyn FnMut(&Path) -> io::Result<()>,
    interrupt: &Interrupt<'_>,
) -> io::Result<()> {
    write_new_with_inner(path, fill, verify, interrupt, false).map(|_| ())
}
fn write_new_with_inner(
    path: &Path,
    fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    verify: &mut dyn FnMut(&Path) -> io::Result<()>,
    interrupt: &Interrupt<'_>,
    receipt: bool,
) -> io::Result<Option<File>> {
    check_interrupt(interrupt)?;
    let (tmp, _, staged) = stream_temp_held(path, Policy::Durable, fill, receipt)?;
    let mut cleanup = Temp { fs: &RealFs, path: Some(tmp.clone()) };
    // Legacy callers close before verification, preserving their existing sharing behavior.
    let held = if receipt {
        Some(staged)
    } else {
        drop(staged);
        None
    };
    check_interrupt(interrupt)?;
    verify(&tmp)?;
    check_interrupt(interrupt)?;
    let (publication, held) = publish_new_inner(&RealFs, &tmp, path, interrupt, held)?;
    cleanup.path = None;
    finish_new_publication(&tmp, path, publication)?;
    sync_dir(parent_of(path));
    Ok(held)
}
pub fn write_new_unique(candidates: &mut dyn Iterator<Item = PathBuf>, data: &[u8]) -> io::Result<PathBuf> {
    let mut candidates = candidates.peekable();
    let first = candidates.peek().cloned().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name to write"))?;
    let (tmp, _) = stream_temp(&first, Policy::Durable, &mut |w| w.write_all(data))?;
    let mut cleanup = Temp { fs: &RealFs, path: Some(tmp.clone()) };
    if !same_content(&tmp, data)? {
        return Err(io::Error::other("the written file reads back different (failing drive or connection?)"));
    }
    for path in candidates.take(100_000) {
        if parent_of(&path) != parent_of(&first) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "new-file candidates must share a directory"));
        }
        match publish_new_with(&RealFs, &tmp, &path, &Interrupt::NONE) {
            Ok(publication) => {
                cleanup.path = None;
                finish_new_publication(&tmp, &path, publication)?;
                sync_dir(parent_of(&path));
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{}: no free file name", first.display())))
}
/// Whether a new-file write published complete bytes before a later cleanup failure.
/// Callers must retain recovery records for that published file, even though the write returned Err.
pub fn was_published(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|source| source.is::<PublishedError>())
}
#[derive(Debug)]
struct PublishedError {
    path: PathBuf,
    source: io::Error,
}
impl std::fmt::Display for PublishedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} was published, but staging cleanup or permission synchronization failed: {}", self.path.display(), self.source)
    }
}
impl std::error::Error for PublishedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
// A published hard link shares attributes with its staging name. Windows refuses
// unlinking readonly files: clear that bit, pin the staged inode through its owned
// temporary name, unlink scratch, then restore and sync that inode through the handle.
fn unlink_published_temp(tmp: &Path, published: &Path) -> io::Result<()> {
    unlink_published_temp_inner(tmp, published).map_err(|source| io::Error::new(source.kind(), PublishedError { path: published.to_path_buf(), source }))
}
fn unlink_published_temp_inner(tmp: &Path, published: &Path) -> io::Result<()> {
    if FAIL_PUBLISHED_CLEANUP.with(Cell::get) {
        return Err(io::Error::other("published staging cleanup failed (injected fault)"));
    }
    #[cfg(windows)]
    {
        let permissions = fs::metadata(tmp)?.permissions();
        if permissions.readonly() {
            let mut writable = permissions.clone();
            writable.set_readonly(false);
            fs::set_permissions(tmp, writable)?;
            let file = match OpenOptions::new().write(true).open(tmp) {
                Ok(file) => file,
                Err(error) => {
                    // Do not unlink before acquiring the restoration handle. The final
                    // path may already name a competing replacement; never chmod it.
                    fs::set_permissions(tmp, permissions)?;
                    return Err(error);
                }
            };
            let removed = retry_rename(RenameRetry::platform(), || fs::remove_file(tmp));
            let restored = file.set_permissions(permissions).and_then(|()| sync_file(&file));
            sync_dir(parent_of(published));
            return removed.and(restored);
        }
    }
    let _ = fs::remove_file(tmp);
    let _ = published;
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Publication {
    Renamed,
    Linked,
    Copied,
}
fn unsupported_rename(error: &io::Error) -> bool {
    if matches!(error.kind(), io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput) {
        return true;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if error.raw_os_error() == Some(rustix::io::Errno::NOSYS.raw_os_error()) {
        return true;
    }
    false
}
fn publish_new_with(fs: &dyn AtomicFs, tmp: &Path, path: &Path, interrupt: &Interrupt<'_>) -> io::Result<Publication> {
    publish_new_inner(fs, tmp, path, interrupt, None).map(|(publication, _)| publication)
}
fn publish_new_inner(fs: &dyn AtomicFs, tmp: &Path, path: &Path, interrupt: &Interrupt<'_>, receipt: Option<File>) -> io::Result<(Publication, Option<File>)> {
    check_interrupt(interrupt)?;
    if !FORCE_COPY_PUBLICATION.with(Cell::get) {
        match retry_rename(RenameRetry::platform(), || fs.rename_new(tmp, path)) {
            Ok(()) => return Ok((Publication::Renamed, receipt)),
            Err(error) if unsupported_rename(&error) => {}
            Err(error) => return Err(error),
        }
        check_interrupt(interrupt)?;
        match retry_rename(RenameRetry::platform(), || fs.link_new(tmp, path)) {
            Ok(()) => return Ok((Publication::Linked, receipt)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Err(error),
            // Unsupported links (including filesystem-specific EPERM/Windows errors) may
            // safely fall back to create_new. Other genuine failures are then returned by
            // source opening/exclusive creation; there is never a replacing rename.
            Err(_) => {}
        }
    }
    check_interrupt(interrupt)?;
    let copied = if receipt.is_some() {
        Some(fs.copy_new_verified_handle(tmp, path, interrupt)?)
    } else {
        fs.copy_new_verified(tmp, path, interrupt)?;
        None
    };
    Ok((Publication::Copied, copied))
}
fn finish_new_publication(tmp: &Path, path: &Path, publication: Publication) -> io::Result<()> {
    if publication == Publication::Linked {
        return unlink_published_temp(tmp, path);
    }
    let result = if FAIL_PUBLISHED_CLEANUP.with(Cell::get) {
        Err(io::Error::other("published staging cleanup failed (injected fault)"))
    } else if publication == Publication::Copied {
        // This is a separate inode: clearing its readonly bit during cleanup cannot change
        // the successfully copied destination's permissions.
        RealFs.remove(tmp)
    } else {
        Ok(())
    };
    result.map_err(|source| io::Error::new(source.kind(), PublishedError { path: path.to_path_buf(), source }))
}

struct ExclusiveCopy {
    file: Option<File>,
    path: PathBuf,
    complete: bool,
}
impl Drop for ExclusiveCopy {
    fn drop(&mut self) {
        self.file.take(); // Close our handle before cleanup, including Windows.
        if !self.complete {
            let _ = RealFs.remove(&self.path);
        }
    }
}
// FAT/exFAT-compatible last resort. Opening the final name exclusively is the ownership
// boundary: a collision never creates a cleanup guard and never removes anyone else's file.
fn copy_new_verified_handle(from: &Path, to: &Path, interrupt: &Interrupt<'_>) -> io::Result<File> {
    use std::io::{Read, Seek, SeekFrom};
    check_interrupt(interrupt)?;
    let mut input = File::open(from)?;
    let metadata = input.metadata()?;
    let output = OpenOptions::new().read(true).write(true).create_new(true).open(to)?;
    let mut owned = ExclusiveCopy { file: Some(output), path: to.to_path_buf(), complete: false };
    let output = owned.file.as_mut().ok_or_else(|| io::Error::other("exclusive copy is unavailable"))?;
    // Preserve privacy from the first visible byte. The already-open read/write handle
    // remains usable for copying and verification even with restrictive/readonly modes.
    if output.metadata()?.permissions() != metadata.permissions() {
        output.set_permissions(metadata.permissions())?;
    }
    {
        let mut writer = Limited { inner: io::BufWriter::with_capacity(1 << 20, &mut *output), written: 0, limit: FAIL_AFTER.with(Cell::get) };
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            check_interrupt(interrupt)?;
            let count = match input.read(&mut buffer) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break;
            }
            writer.write_all(&buffer[..count])?;
            interrupt.progress(0.9 * writer.written as f32 / metadata.len().max(1) as f32);
        }
        writer.flush()?;
    }
    check_interrupt(interrupt)?;
    sync_file(output)?;
    interrupt.progress(0.95);
    check_interrupt(interrupt)?;
    input.seek(SeekFrom::Start(0))?;
    output.seek(SeekFrom::Start(0))?;
    if input.metadata()?.len() != output.metadata()?.len() {
        return Err(io::Error::other("the published copy reads back different"));
    }
    let mut expected = vec![0u8; 1 << 20];
    let mut actual = vec![0u8; 1 << 20];
    loop {
        check_interrupt(interrupt)?;
        let count = match input.read(&mut expected) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        output.read_exact(&mut actual[..count])?;
        if expected[..count] != actual[..count] {
            return Err(io::Error::other("the published copy reads back different"));
        }
    }
    // Sync again after successful verification while the original writable handle is held.
    sync_file(output)?;
    interrupt.progress(1.0);
    check_interrupt(interrupt)?;
    owned.complete = true;
    owned.file.take().ok_or_else(|| io::Error::other("exclusive copy did not retain its file handle"))
}
/// Does the file at `path` hold exactly `data`?
fn same_content(path: &Path, data: &[u8]) -> io::Result<bool> {
    use std::io::Read;
    let mut f = File::open(path)?;
    if f.metadata()?.len() != data.len() as u64 {
        return Ok(false);
    }
    let mut buf = vec![0u8; 1 << 20];
    let mut at = 0usize;
    loop {
        let k = match f.read(&mut buf) {
            Ok(0) => return Ok(at == data.len()),
            Ok(k) => k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if data.get(at..at + k) != buf.get(..k) {
            return Ok(false);
        }
        at += k;
    }
}

/// Do `a` and `b` name the same existing file (through links, `..`, case-insensitive names)?
pub fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ma.dev() == mb.dev() && ma.ino() == mb.ino()
    }
    #[cfg(not(unix))]
    {
        if ma.len() != mb.len() {
            return false;
        }
        match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x == y || x.to_string_lossy().to_lowercase() == y.to_string_lossy().to_lowercase(),
            _ => false,
        }
    }
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
