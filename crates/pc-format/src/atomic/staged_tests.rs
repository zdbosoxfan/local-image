use super::*;
use photocraft_raster::Interrupt;
use std::sync::{Arc, atomic::AtomicBool};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("pc-staging-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn target(&self) -> PathBuf {
        let path = self.0.join("export.svg");
        std::fs::write(&path, b"existing export").unwrap();
        path
    }
    fn clean(&self) {
        assert_eq!(std::fs::read_dir(&self.0).unwrap().count(), 1, "owned staging file remains");
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Clone, Copy)]
enum Fault {
    Partial,
    Rename,
    SyncDir,
    Collision,
}
struct FaultFs(Fault);
impl AtomicFs for FaultFs {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if matches!(self.0, Fault::Partial | Fault::Collision) {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
            file.write_all(b"partial or independently owned")?;
            return Err(io::Error::new(if matches!(self.0, Fault::Collision) { io::ErrorKind::AlreadyExists } else { io::ErrorKind::StorageFull }, "injected"));
        }
        RealFs.write_new(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if matches!(self.0, Fault::Rename) { Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected")) } else { RealFs.rename(from, to) }
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        RealFs.remove(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        if matches!(self.0, Fault::SyncDir) { Err(io::Error::other("injected")) } else { RealFs.sync_dir(dir) }
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.copy_permissions(from, to)
    }
}
fn no_retry() -> RenameRetry {
    RenameRetry { attempts: 0, first_delay_ms: 0 }
}

#[test]
fn staged_write_does_not_publish_until_commit_and_drop_cleans() {
    let dir = Dir::new();
    let path = dir.target();
    let pending = stage_write(&path, b"new export", &Interrupt::NONE).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
    drop(pending);
    dir.clean();
    assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
    let pending = stage_write(&path, b"new export", &Interrupt::NONE).unwrap();
    std::thread::spawn(move || pending.commit()).join().unwrap().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new export");
    dir.clean();
}

#[test]
fn staged_write_cancels_before_and_during_bulk_io_without_changing_destination() {
    let dir = Dir::new();
    let path = dir.target();
    let cancel = AtomicBool::new(true);
    let check = || cancel.load(Ordering::Relaxed);
    let progress = |fraction: f32| {
        if fraction >= 0.3 {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    assert!(matches!(stage_write(&path,b"new",&Interrupt::cancel_only(&check)),Err(error) if error.kind()==io::ErrorKind::Interrupted));
    dir.clean();
    cancel.store(false, Ordering::Relaxed);
    let bytes = vec![0x5a; 3 * 1024 * 1024];
    assert!(matches!(stage_write(&path,&bytes,&Interrupt::new(&check,&progress)),Err(error) if error.kind()==io::ErrorKind::Interrupted));
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
    dir.clean();
}

#[test]
fn staged_write_failure_paths_keep_original_and_cleanup_owned_temps() {
    for fault in [Fault::Partial, Fault::Rename] {
        let dir = Dir::new();
        let path = dir.target();
        let prepared = stage_write_with(Arc::new(FaultFs(fault)), no_retry(), &path, b"new export", &Interrupt::NONE);
        match prepared {
            Ok(prepared) => assert!(prepared.commit().is_err()),
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::StorageFull),
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
        dir.clean();
    }
}

#[test]
fn staged_write_collision_never_deletes_an_unowned_temporary_file() {
    let dir = Dir::new();
    let path = dir.target();
    assert!(
        matches!(stage_write_with(Arc::new(FaultFs(Fault::Collision)),no_retry(),&path,b"new",&Interrupt::NONE),Err(error) if error.kind()==io::ErrorKind::AlreadyExists)
    );
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 9);
    for entry in std::fs::read_dir(&dir.0).unwrap().flatten() {
        assert_eq!(
            std::fs::read(entry.path()).unwrap(),
            if entry.path() == path { b"existing export".as_slice() } else { b"partial or independently owned".as_slice() }
        );
    }
}

#[test]
fn staged_write_progress_unwind_cleans_owned_file() {
    let dir = Dir::new();
    let path = dir.target();
    let progress = |_: f32| panic!("injected progress panic");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stage_write(&path, b"new export", &Interrupt::new(&|| false, &progress))));
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing export");
    dir.clean();
}

#[test]
fn staged_write_post_rename_sync_failure_does_not_report_old_file_unchanged() {
    let dir = Dir::new();
    let path = dir.target();
    stage_write_with(Arc::new(FaultFs(Fault::SyncDir)), no_retry(), &path, b"new export", &Interrupt::NONE).unwrap().commit().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new export");
    dir.clean();
}

#[cfg(unix)]
#[test]
fn staged_write_follows_existing_symlink_and_preserves_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = Dir::new();
    let path = dir.target();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.0.join("chosen.svg");
    symlink(&path, &link).unwrap();
    stage_write(&link, b"new export", &Interrupt::NONE).unwrap().commit().unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&path).unwrap(), b"new export");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    std::fs::remove_file(link).unwrap();
    dir.clean();
}
