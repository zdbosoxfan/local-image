use super::*;
use std::sync::atomic::AtomicBool;

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("li-fs-j11-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn target(&self) -> PathBuf {
        self.0.join("document")
    }
    fn clean(&self) {
        assert!(fs::read_dir(&self.0).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const OLD: &[u8] = b"original document";

#[test]
fn j11_golden_all_write_policies_and_stream_lengths() {
    let dir = Dir::new();
    // Empty, binary, multi-buffer: bytes must survive every adapter's shared operation.
    for bytes in [vec![], vec![0, 255, 128, 0], (0..2_100_001).map(|n| (n % 251) as u8).collect()] {
        let target = dir.target();
        for policy in [Policy::Durable, Policy::Atomic] {
            fs::write(&target, OLD).unwrap();
            let before = syncs_on_this_thread();
            let staged = stage_stream(&target, policy, &mut |w| {
                for chunk in bytes.chunks(7919) {
                    w.write_all(chunk)?;
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(staged.len(), bytes.len() as u64);
            assert_eq!(fs::read(&target).unwrap(), OLD);
            staged.commit().unwrap();
            assert_eq!(fs::read(&target).unwrap(), bytes);
            let syncs = syncs_on_this_thread() - before;
            if policy == Policy::Atomic {
                assert_eq!(syncs, 0);
            } else {
                assert!(syncs >= 1);
            }
            dir.clean();
        }
        atomic_write(&target, &bytes).unwrap();
        assert_eq!(fs::read(&target).unwrap(), bytes);
        let new = dir.0.join("new");
        write_new(&new, &bytes).unwrap();
        assert_eq!(fs::read(&new).unwrap(), bytes);
        fs::remove_file(new).unwrap();
    }
}

#[test]
fn j11_faults_preserve_destination_and_cleanup_across_all_entry_points() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    for boundary in [0, 4, 1 << 20] {
        let _fault = fail_writes_after(boundary);
        let bytes = vec![37; (1 << 20) + 11];
        assert!(write_atomic(&target, &bytes).is_err());
        assert!(write_atomic_nosync(&target, &bytes).is_err());
        assert!(atomic_write(&target, &bytes).is_err());
        assert!(stage_write(&target, &bytes, &Interrupt::NONE).is_err());
        assert!(write_new(&dir.0.join("new"), &bytes).is_err());
        assert_eq!(fs::read(&target).unwrap(), OLD);
        dir.clean();
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
}

#[test]
fn j11_stream_callback_errors_are_not_replayed_and_panic_cleans() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    let mut calls = 0;
    let error = stage_stream(&target, Policy::Durable, &mut |w| {
        calls += 1;
        w.write_all(b"partial")?;
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "encoder error"))
    })
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(calls, 1);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = stage_stream(&target, Policy::Durable, &mut |w| {
            w.write_all(b"partial")?;
            panic!("encoder panic")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(fs::read(&target).unwrap(), OLD);
    dir.clean();
}

#[test]
fn j11_cancel_before_during_and_final_progress_preserves_original() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    let flag = AtomicBool::new(true);
    let cancel = || flag.load(Ordering::Relaxed);
    assert_eq!(stage_write(&target, b"new", &Interrupt::cancel_only(&cancel)).err().unwrap().kind(), io::ErrorKind::Interrupted);
    for threshold in [0.2, 1.0] {
        flag.store(false, Ordering::Relaxed);
        let progress = |f: f32| {
            if f >= threshold {
                flag.store(true, Ordering::Relaxed);
            }
        };
        assert_eq!(stage_write(&target, &vec![7; 3 << 20], &Interrupt::new(&cancel, &progress)).err().unwrap().kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read(&target).unwrap(), OLD);
        dir.clean();
    }
}

#[test]
fn j11_progress_panic_and_staged_drop_cleanup() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    for threshold in [0.2, 1.0] {
        let progress = |f: f32| {
            assert!(f < threshold, "progress panic");
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stage_write(&target, &vec![4; 2 << 20], &Interrupt::new(&|| false, &progress))));
        assert!(result.is_err());
        dir.clean();
    }
    let staged = stage_write(&target, b"new", &Interrupt::NONE).unwrap();
    drop(staged);
    assert_eq!(fs::read(&target).unwrap(), OLD);
    dir.clean();
    let staged = stage_write(&target, b"new", &Interrupt::NONE).unwrap();
    std::thread::spawn(move || staged.commit()).join().unwrap().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

#[derive(Default)]
struct FaultFs {
    locks: Cell<u32>,
    renames: Cell<u32>,
    rename_error: Option<io::ErrorKind>,
    sync_error: bool,
    collision: bool,
}
impl AtomicFs for FaultFs {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
        if self.collision {
            fs::write(tmp, b"independent owner's file").unwrap();
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "collision"));
        }
        RealFs.write_new(tmp, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.renames.set(self.renames.get() + 1);
        if let Some(kind) = self.rename_error {
            return Err(io::Error::new(kind, "rename fault"));
        }
        if self.locks.get() > 0 {
            self.locks.set(self.locks.get() - 1);
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        RealFs.rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        RealFs.remove(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        if self.sync_error { Err(io::Error::other("post-publication sync fault")) } else { RealFs.sync_dir(dir) }
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.copy_permissions(from, to)
    }
}

#[test]
fn j11_retry_and_permanent_rename_failure_parity() {
    let dir = Dir::new();
    let target = dir.target();
    let retry = RenameRetry { attempts: 3, first_delay_ms: 0 };
    let fs = FaultFs { locks: Cell::new(2), ..Default::default() };
    atomic_write_with(&fs, retry, &target, b"new").unwrap();
    assert_eq!(fs.renames.get(), 3);
    for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::CrossesDevices, io::ErrorKind::Other] {
        fs::write(&target, OLD).unwrap();
        let fs = FaultFs { rename_error: Some(kind), ..Default::default() };
        let error = atomic_write_with(&fs, retry, &target, b"new").unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(error.to_string().contains("not changed"));
        if kind == io::ErrorKind::CrossesDevices {
            assert!(error.to_string().contains("different volume"));
        }
        assert_eq!(fs::read(&target).unwrap(), OLD);
        assert_eq!(fs.renames.get(), if kind == io::ErrorKind::PermissionDenied { 4 } else { 1 });
        dir.clean();
    }
}

#[test]
fn j11_collisions_never_delete_an_unowned_temp() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    let fs = FaultFs { collision: true, ..Default::default() };
    assert_eq!(atomic_write_with(&fs, RenameRetry { attempts: 0, first_delay_ms: 0 }, &target, b"new").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 9);
    for entry in fs::read_dir(&dir.0).unwrap().flatten() {
        assert_eq!(fs::read(entry.path()).unwrap(), if entry.path() == target { OLD } else { b"independent owner's file" });
    }
}

#[test]
fn j11_post_publication_sync_failure_never_claims_unchanged() {
    let dir = Dir::new();
    let target = dir.target();
    let fs = FaultFs { sync_error: true, ..Default::default() };
    // FaultFs uses Cell for retry tests; the staging seam requires Send + Sync instead.
    atomic_write_with(&fs, RenameRetry { attempts: 0, first_delay_ms: 0 }, &target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    dir.clean();
}

#[test]
fn j11_new_files_never_replace_and_verified_unique_names() {
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    assert_eq!(write_new(&target, b"new").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    let mut names = [target.clone(), dir.0.join("next")].into_iter();
    let selected = write_new_unique(&mut names, b"new").unwrap();
    assert_eq!(selected, dir.0.join("next"));
    assert_eq!(fs::read(selected).unwrap(), b"new");
    assert_eq!(fs::read(&target).unwrap(), OLD);
    assert!(write_new_with(&dir.0.join("bad"), &mut |w| w.write_all(b"new"), &mut |_| Err(io::Error::other("verification failed"))).is_err());
    assert!(!dir.0.join("bad").exists());
    // Verification creates a competing name just before publish. It must survive unchanged.
    let race = dir.0.join("race");
    let result = write_new_with(&race, &mut |w| w.write_all(b"new"), &mut |_| fs::write(&race, OLD));
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(race).unwrap(), OLD);
    dir.clean();
}

#[test]
fn j11_new_file_publication_never_falls_back_to_replacing_rename() {
    let dir = Dir::new();
    let tmp = dir.0.join("staging");
    fs::write(&tmp, b"complete new file").unwrap();
    let target = dir.target();
    let adapter = FaultFs::default(); // link_new is Unsupported; rename would succeed.
    let error = publish_new_with(&adapter, &tmp, &target, &Interrupt::NONE).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(!was_published(&error));
    assert!(!target.exists());
    assert_eq!(adapter.renames.get(), 0);
    fs::write(&target, OLD).unwrap();
    assert!(publish_new_with(&adapter, &tmp, &target, &Interrupt::NONE).is_err());
    assert_eq!(fs::read(&target).unwrap(), OLD);
    assert_eq!(adapter.renames.get(), 0);
}

#[test]
fn j11_invalid_paths_and_parent_creation_contract() {
    let dir = Dir::new();
    for path in [Path::new(""), Path::new("/"), &dir.0] {
        assert!(atomic_write(path, b"new").is_err());
    }
    let nested = dir.0.join("missing/child");
    assert_eq!(write_atomic(&nested, b"new").unwrap_err().kind(), io::ErrorKind::NotFound);
    atomic_write(&nested, b"new").unwrap();
    assert_eq!(fs::read(&nested).unwrap(), b"new");
    assert!(write_new_unique(&mut std::iter::empty(), b"new").is_err());
}

#[cfg(unix)]
#[test]
fn j11_symlinks_modes_identity_and_dangling_link_safety() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.0.join("link");
    symlink("document", &link).unwrap();
    for durable in [true, false] {
        if durable {
            write_atomic(&link, b"new").unwrap();
        } else {
            write_atomic_nosync(&link, b"new").unwrap();
        }
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
    }
    atomic_write(&link, b"editor").unwrap();
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
    let hard = dir.0.join("hard");
    fs::hard_link(&target, &hard).unwrap();
    assert!(same_file(&target, &hard));
    assert!(same_file(&target, &link));
    assert!(!same_file(&target, &dir.0.join("absent")));
    let dangling = dir.0.join("dangling");
    symlink("missing", &dangling).unwrap();
    assert!(write_atomic(&dangling, b"new").is_err());
    assert_eq!(write_new(&dangling, b"new").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert!(fs::symlink_metadata(dangling).unwrap().file_type().is_symlink());
    dir.clean();
}

#[test]
fn j11_nested_fault_guards_restore_previous_limit() {
    let dir = Dir::new();
    let outer = fail_writes_after(3);
    {
        let _inner = fail_writes_after(0);
        assert!(write_atomic(&dir.target(), b"a").is_err());
    }
    write_atomic(&dir.target(), b"abc").unwrap();
    assert!(write_atomic(&dir.target(), b"abcd").is_err());
    drop(outer);
    write_atomic(&dir.target(), b"abcd").unwrap();
}

#[test]
fn j11_concurrent_replacement_never_mixes_bytes_and_new_file_has_one_winner() {
    let dir = Dir::new();
    let target = dir.target();
    let writers: Vec<_> = (1..=4u8)
        .map(|value| {
            let path = target.clone();
            std::thread::spawn(move || write_atomic_nosync(&path, &vec![value; 32768]).unwrap())
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let bytes = fs::read(&target).unwrap();
    assert_eq!(bytes.len(), 32768);
    assert!(bytes.iter().all(|v| *v == bytes[0]));
    let fresh = dir.0.join("new");
    let writers: Vec<_> = (1..=4u8)
        .map(|value| {
            let path = fresh.clone();
            std::thread::spawn(move || write_new(&path, &[value; 1024]))
        })
        .collect();
    let results: Vec<_> = writers.into_iter().map(|writer| writer.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().filter_map(|r| r.as_ref().err()).all(|e| e.kind() == io::ErrorKind::AlreadyExists));
    let bytes = fs::read(fresh).unwrap();
    assert_eq!(bytes.len(), 1024);
    assert!(bytes.iter().all(|v| *v == bytes[0]));
    dir.clean();
}

#[cfg(unix)]
#[test]
fn j11_write_only_mode_is_preserved_and_synced_through_owned_handle() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Dir::new();
    let target = dir.target();
    fs::write(&target, OLD).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o200)).unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o200);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"new");
    dir.clean();
}

#[cfg(windows)]
#[test]
fn j11_readonly_new_file_publication_cleans_staging_and_keeps_readonly_destination() {
    let dir = Dir::new();
    let target = dir.target();
    write_new_with(&target, &mut |out| out.write_all(b"complete"), &mut |tmp| {
        let mut permissions = fs::metadata(tmp)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(tmp, permissions)
    })
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"complete");
    assert!(fs::metadata(&target).unwrap().permissions().readonly());
    dir.clean();
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(target, permissions).unwrap();
}

#[test]
fn j11_published_cleanup_error_keeps_complete_new_file_and_marks_its_outcome() {
    let dir = Dir::new();
    let target = dir.target();
    let error = {
        let _fault = fail_published_cleanup();
        write_new(&target, b"verified complete file").unwrap_err()
    };
    assert!(was_published(&error));
    assert!(error.to_string().contains("was published"));
    assert_eq!(fs::read(&target).unwrap(), b"verified complete file");
    // Atomic rename consumes scratch before the injected post-publication failure;
    // a link/copy backend can leave one owned scratch. Every remaining file is complete.
    let files: Vec<_> = fs::read_dir(&dir.0).unwrap().flatten().collect();
    assert!((1..=2).contains(&files.len()));
    for entry in files {
        assert_eq!(fs::read(entry.path()).unwrap(), b"verified complete file");
    }
    let error = write_new(&target, b"other writer").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert!(!was_published(&error));
    assert_eq!(fs::read(target).unwrap(), b"verified complete file");
}

// Test volume exposing native failure kinds while using real exclusive-create/copy IO.
#[derive(Clone, Copy)]
struct VolumeFs {
    rename_kind: io::ErrorKind,
    links: bool,
    copy_limit: Option<u64>,
}
impl VolumeFs {
    fn unsupported() -> Self {
        Self { rename_kind: io::ErrorKind::Unsupported, links: false, copy_limit: None }
    }
}
impl AtomicFs for VolumeFs {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealFs.write_new(path, bytes)
    }
    fn rename_new(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::new(self.rename_kind, "simulated filesystem no-replace rename rejection"))
    }
    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.links { RealFs.link_new(from, to) } else { Err(io::Error::new(io::ErrorKind::Unsupported, "simulated no-hard-link volume")) }
    }
    fn copy_new_verified(&self, from: &Path, to: &Path, interrupt: &Interrupt<'_>) -> io::Result<()> {
        let _fault = self.copy_limit.map(fail_writes_after);
        RealFs.copy_new_verified(from, to, interrupt)
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        panic!("replacing rename must never be called")
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        RealFs.remove(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.copy_permissions(from, to)
    }
}

#[test]
fn j11_volumes_unsupported_rename_and_links_copy_verified_without_overwrite() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    let bytes: Vec<u8> = (0..65537).map(|n| (n % 251) as u8).collect();
    fs::write(&source, &bytes).unwrap();
    for kind in [io::ErrorKind::Unsupported, io::ErrorKind::InvalidInput] {
        let target = dir.target();
        let volume = VolumeFs { rename_kind: kind, ..VolumeFs::unsupported() };
        let before = syncs_on_this_thread();
        assert_eq!(publish_new_with(&volume, &source, &target, &Interrupt::NONE).unwrap(), Publication::Copied);
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert!(syncs_on_this_thread() >= before + 2, "copy bytes and mode synced");
        assert_eq!(publish_new_with(&volume, &source, &target, &Interrupt::NONE).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&target).unwrap(), bytes);
        fs::remove_file(target).unwrap();
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn j11_volumes_native_no_replace_rename_prefers_atomic_publication() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    let target = dir.target();
    fs::write(&source, b"complete").unwrap();
    assert_eq!(publish_new_with(&RealFs, &source, &target, &Interrupt::NONE).unwrap(), Publication::Renamed);
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), b"complete");
    fs::write(&source, OLD).unwrap();
    assert_eq!(publish_new_with(&RealFs, &source, &target, &Interrupt::NONE).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&source).unwrap(), OLD);
    assert_eq!(fs::read(target).unwrap(), b"complete");
}

#[test]
fn j11_volumes_concurrent_exclusive_publishers_have_exactly_one_winner() {
    let dir = Dir::new();
    let target = dir.target();
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut threads = Vec::new();
    for value in 1..=8u8 {
        let staged = dir.0.join(format!("source-{value}"));
        fs::write(&staged, vec![value; 65537]).unwrap();
        let target = target.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            publish_new_with(&VolumeFs::unsupported(), &staged, &target, &Interrupt::NONE)
        }));
    }
    let results: Vec<_> = threads.into_iter().map(|thread| thread.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results.iter().filter_map(|result| result.as_ref().err()).all(|error| error.kind() == io::ErrorKind::AlreadyExists));
    let bytes = fs::read(&target).unwrap();
    assert_eq!(bytes.len(), 65537);
    assert!(bytes.iter().all(|value| *value == bytes[0]));
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 9, "eight staged fixtures plus winner; no loser destination or scratch");
}

#[test]
fn j11_volumes_partial_copy_failure_cancellation_and_panic_remove_owned_destination() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    let bytes = vec![0x5a; 3 << 20];
    fs::write(&source, &bytes).unwrap();
    let target = dir.target();
    for limit in [0, 3, (1 << 20) + 7] {
        let volume = VolumeFs { copy_limit: Some(limit), ..VolumeFs::unsupported() };
        assert!(publish_new_with(&volume, &source, &target, &Interrupt::NONE).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read(&source).unwrap(), bytes);
    }
    let cancelled = AtomicBool::new(false);
    let cancel = || cancelled.load(Ordering::Relaxed);
    let progress = |fraction: f32| {
        if fraction > 0.0 {
            assert!(target.exists(), "fallback reserves the final name exclusively");
            assert!(fs::metadata(&target).unwrap().len() < bytes.len() as u64, "readers may see a partial new file");
            cancelled.store(true, Ordering::Relaxed);
        }
    };
    assert_eq!(
        publish_new_with(&VolumeFs::unsupported(), &source, &target, &Interrupt::new(&cancel, &progress)).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!target.exists());
    cancelled.store(false, Ordering::Relaxed);
    let progress = |_: f32| panic!("injected fallback progress panic");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        publish_new_with(&VolumeFs::unsupported(), &source, &target, &Interrupt::new(&cancel, &progress))
    }));
    assert!(result.is_err());
    assert!(!target.exists());
    // Failure injection on a collision must not delete the file another publisher owns.
    fs::write(&target, OLD).unwrap();
    let volume = VolumeFs { copy_limit: Some(0), ..VolumeFs::unsupported() };
    assert_eq!(publish_new_with(&volume, &source, &target, &Interrupt::NONE).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&target).unwrap(), OLD);
}

#[test]
fn j11_volumes_readback_failure_and_final_cancellation_cleanup() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    let bytes = vec![0x5a; 65537];
    fs::write(&source, &bytes).unwrap();
    let target = dir.target();
    let corrupt = |fraction: f32| {
        if fraction >= 0.95 {
            OpenOptions::new().write(true).open(&target).unwrap().write_all(b"tampered").unwrap();
        }
    };
    let error = publish_new_with(&VolumeFs::unsupported(), &source, &target, &Interrupt::new(&|| false, &corrupt)).unwrap_err();
    assert!(error.to_string().contains("reads back different"));
    assert!(!target.exists());
    let cancelled = AtomicBool::new(false);
    let cancel = || cancelled.load(Ordering::Relaxed);
    let progress = |fraction: f32| {
        if fraction == 1.0 {
            cancelled.store(true, Ordering::Relaxed);
        }
    };
    assert_eq!(
        publish_new_with(&VolumeFs::unsupported(), &source, &target, &Interrupt::new(&cancel, &progress)).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!target.exists());
    assert_eq!(fs::read(source).unwrap(), bytes);
}

#[test]
fn j11_volumes_real_permission_error_does_not_trigger_fallback() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    fs::write(&source, b"complete").unwrap();
    let target = dir.target();
    let volume = VolumeFs { rename_kind: io::ErrorKind::PermissionDenied, ..VolumeFs::unsupported() };
    assert_eq!(publish_new_with(&volume, &source, &target, &Interrupt::NONE).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(!target.exists());
}

#[test]
fn j11_volumes_fallback_empty_binary_readonly_and_private_modes_match_staging() {
    let dir = Dir::new();
    let source = dir.0.join("staged");
    let target = dir.target();
    for bytes in [vec![], vec![0, 255, 128, 0], vec![0x37; 65537]] {
        fs::write(&source, &bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o400)).unwrap();
        }
        let progress = |_: f32| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o400, "privacy retained while final name is visible");
            }
        };
        assert_eq!(publish_new_with(&VolumeFs::unsupported(), &source, &target, &Interrupt::new(&|| false, &progress)).unwrap(), Publication::Copied);
        assert_eq!(fs::read(&target).unwrap(), bytes);
        let permissions = fs::metadata(&source).unwrap().permissions();
        assert_eq!(fs::metadata(&target).unwrap().permissions(), permissions);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        }
        RealFs.remove(&target).unwrap();
    }
}
