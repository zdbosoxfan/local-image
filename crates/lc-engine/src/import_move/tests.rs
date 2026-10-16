//! Synthetic files only: no camera media, library, or user folder is touched.
use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("li-move-integrity-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("source")).unwrap();
        fs::create_dir_all(dir.join("destination")).unwrap();
        Self(dir)
    }
    fn source(&self) -> PathBuf {
        self.0.join("source/photo.jpg")
    }
    fn place(&self, fault: Fault) -> Placed {
        fs::write(self.source(), b"ORIGINAL").unwrap();
        inject(fault);
        let result = place(&self.source(), &self.0.join("destination"), "photo.jpg");
        inject(Fault::None);
        result.unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn replace(path: &Path, bytes: &[u8]) {
    let replacement = path.with_extension("replacement");
    fs::write(&replacement, bytes).unwrap();
    fs::rename(replacement, path).unwrap();
}

#[test]
fn replacement_source_is_kept_for_link_and_copy_moves() {
    for fault in [Fault::None, Fault::NoLink] {
        let f = Fixture::new();
        let p = f.place(fault);
        replace(&p.src, b"NEW-DATA");
        assert!(finish(&p).is_err());
        assert_eq!(fs::read(&p.src).unwrap(), b"NEW-DATA");
        assert_eq!(fs::read(&p.dst).unwrap(), b"ORIGINAL");
    }
}

#[test]
fn rewritten_source_with_restored_mtime_is_kept() {
    for fault in [Fault::None, Fault::NoLink] {
        let f = Fixture::new();
        let p = f.place(fault);
        let modified = fs::metadata(&p.src).unwrap().modified().unwrap();
        fs::write(&p.src, b"NEW-DATA").unwrap();
        File::options().write(true).open(&p.src).unwrap().set_modified(modified).unwrap();
        assert!(finish(&p).is_err());
        assert_eq!(fs::read(&p.src).unwrap(), b"NEW-DATA");
    }
}

#[test]
fn changed_destination_is_never_removed_by_finish_or_rollback() {
    for fault in [Fault::None, Fault::NoLink] {
        for rewrite in [false, true] {
            let f = Fixture::new();
            let p = f.place(fault);
            if rewrite {
                let modified = fs::metadata(&p.dst).unwrap().modified().unwrap();
                fs::write(&p.dst, b"NEW-DATA").unwrap();
                File::options().write(true).open(&p.dst).unwrap().set_modified(modified).unwrap();
            } else {
                replace(&p.dst, b"NEW-DATA");
            }
            assert!(finish(&p).is_err());
            rollback(&p);
            assert!(p.src.is_file());
            assert_eq!(fs::read(&p.dst).unwrap(), b"NEW-DATA");
        }
    }
}

#[test]
fn source_replaced_between_validation_and_capture_is_restored() {
    for fault in [Fault::None, Fault::NoLink] {
        let f = Fixture::new();
        let p = f.place(fault);
        assert!(remove_verified_with(&p.src, &p.dst, &p.source, || replace(&p.src, b"NEW-DATA")).is_err());
        assert_eq!(fs::read(&p.src).unwrap(), b"NEW-DATA");
        assert_eq!(fs::read(&p.dst).unwrap(), b"ORIGINAL");
        assert_eq!(fs::read_dir(p.src.parent().unwrap()).unwrap().count(), 1);
    }
}

#[test]
fn recovery_preserves_live_captures_and_occupied_names() {
    let f = Fixture::new();
    let p = f.place(Fault::NoLink);
    let capture = recovery::Capture::take(&p.src, Some(&p.dst), Some(p.source.content.to_string())).unwrap();
    let captured = capture.path.clone();
    recover_pending(p.src.parent().unwrap());
    assert!(!p.src.exists(), "a live Move must not be restored under its worker");
    fs::write(&p.src, b"NEW-DATA").unwrap();
    assert!(capture.restore(&p.src).is_err());
    recover_pending(p.src.parent().unwrap());
    assert_eq!(fs::read(&p.src).unwrap(), b"NEW-DATA");
    assert_eq!(fs::read(&captured).unwrap(), b"ORIGINAL");
    fs::remove_file(&p.src).unwrap();
    recover_pending(p.src.parent().unwrap());
    assert_eq!(fs::read(&p.src).unwrap(), b"ORIGINAL");
    assert!(!captured.exists());
}

#[test]
fn recovery_refuses_malformed_records_and_replaced_handles() {
    for bad in ["../escaped.jpg", "/escaped.jpg", "", ".", "name/", "name/."] {
        let f = Fixture::new();
        let p = f.place(Fault::NoLink);
        let capture = recovery::Capture::take(&p.src, None, None).unwrap();
        let captured = capture.path.clone();
        let record = captured.parent().unwrap().join("recovery.json");
        drop(capture);
        let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        json["original_name"] = bad.into();
        fs::write(record, serde_json::to_vec(&json).unwrap()).unwrap();
        recover_pending(p.src.parent().unwrap());
        assert_eq!(fs::read(&captured).unwrap(), b"ORIGINAL");
        assert!(!p.src.exists());
    }
    let f = Fixture::new();
    let p = f.place(Fault::NoLink);
    let capture = recovery::Capture::take(&p.src, None, None).unwrap();
    let captured = capture.path.clone();
    replace(&captured.parent().unwrap().join("lock"), b"");
    assert!(capture.restore(&p.src).is_err());
    assert_eq!(fs::read(captured).unwrap(), b"ORIGINAL");
}

#[cfg(unix)]
#[test]
fn recovery_does_not_follow_payload_or_record_symlinks() {
    for target in ["payload", "recovery.json", "lock"] {
        let f = Fixture::new();
        let p = f.place(Fault::NoLink);
        let capture = recovery::Capture::take(&p.src, None, None).unwrap();
        let dir = capture.path.parent().unwrap().to_path_buf();
        drop(capture);
        let target = dir.join(target);
        let outside = f.0.join("outside");
        fs::rename(&target, &outside).unwrap();
        let original = fs::read(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, &target).unwrap();
        recover_pending(p.src.parent().unwrap());
        assert!(!p.src.exists());
        assert_eq!(fs::read(outside).unwrap(), original);
        assert!(fs::symlink_metadata(target).unwrap().file_type().is_symlink());
    }
}

#[test]
fn captured_move_crash_child() {
    let Some(root) = std::env::var_os("LI_MOVE_CRASH_TEST_ROOT") else { return };
    let source = PathBuf::from(root).join("source/photo.jpg");
    let _capture = recovery::Capture::take(&source, None, None).unwrap();
    panic!("fault injection should terminate the child after durable capture");
}

#[test]
fn crashed_capture_recovers_in_selected_folder_only() {
    let f = Fixture::new();
    fs::write(f.source(), b"ORIGINAL").unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "import_move::tests::captured_move_crash_child", "--nocapture"])
        .env("LI_MOVE_CRASH_TEST_ROOT", &f.0)
        .env("LI_MOVE_CRASH_STAGE", "captured")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("synthetic crash child did not exit promptly");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(89));
    assert!(!f.source().exists());
    recover_pending(&f.0.join("destination"));
    assert!(!f.source().exists(), "recovery never scans another folder");
    recover_pending(&f.0.join("source"));
    assert_eq!(fs::read(f.source()).unwrap(), b"ORIGINAL");
    assert_eq!(fs::read_dir(f.0.join("source")).unwrap().count(), 1);
}

#[test]
fn replaced_sidecar_is_kept_after_photo_move() {
    for fault in [Fault::None, Fault::NoLink] {
        let f = Fixture::new();
        let sidecar = f.source().with_extension("xmp");
        fs::write(&sidecar, b"ORIGINAL").unwrap();
        let p = f.place(fault);
        replace(&sidecar, b"NEW-DATA");
        let kept = finish(&p).unwrap();
        assert_eq!(kept.len(), 1);
        assert!(!p.src.exists());
        assert_eq!(fs::read(&sidecar).unwrap(), b"NEW-DATA");
        assert_eq!(fs::read(p.dst.with_extension("xmp")).unwrap(), b"ORIGINAL");
    }
}

#[cfg(unix)]
#[test]
fn library_hooks_reject_nonregular_and_oversized_files() {
    let f = Fixture::new();
    let fifo = f.source();
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let link = f.0.join("source/link.jpg");
    std::os::unix::fs::symlink(&fifo, &link).unwrap();
    let huge = f.0.join("source/huge.jpg");
    File::create(&huge).unwrap().set_len(li_fs::read::MAX_FILE_BYTES + 1).unwrap();
    let (load, probe) = crate::files::fs_hooks();
    let load_with = crate::files::fs_loader_with();
    let preview = crate::files::fs_preview_loader();
    let options = crate::files::RawOptions::default();
    for path in [&fifo, &link, &huge, Path::new("/dev/zero")] {
        let path = path.to_str().unwrap();
        assert!(probe(path).is_err());
        assert!(load(path, 64).is_err());
        assert!(load_with(path, 64, &options).is_err());
        assert!(preview(path, 64).is_none());
    }
}

#[test]
fn exclusive_copy_publication_tracks_the_actual_destination() {
    for rollback_after_place in [false, true] {
        let f = Fixture::new();
        let p = {
            let _publication = li_fs::force_copy_publication();
            f.place(Fault::NoLink)
        };
        assert_eq!(FileIdentity::read(&p.dst).unwrap().handle, p.source.destination.handle);
        assert_eq!(fs::read(&p.dst).unwrap(), b"ORIGINAL");
        if rollback_after_place {
            rollback(&p);
            assert!(p.src.is_file());
            assert!(!p.dst.exists());
        } else {
            assert!(finish(&p).unwrap().is_empty());
            assert!(!p.src.exists());
            assert_eq!(fs::read(&p.dst).unwrap(), b"ORIGINAL");
        }
    }
}

#[test]
fn exclusive_copy_receipt_does_not_claim_a_replacement_destination() {
    let f = Fixture::new();
    let p = {
        let _publication = li_fs::force_copy_publication();
        f.place(Fault::NoLink)
    };
    replace(&p.dst, b"NEW-DATA");
    assert!(finish(&p).is_err());
    rollback(&p);
    assert_eq!(fs::read(&p.src).unwrap(), b"ORIGINAL");
    assert_eq!(fs::read(&p.dst).unwrap(), b"NEW-DATA");
}

#[test]
fn destination_sync_failures_restore_the_source_for_link_and_copy_moves() {
    for placement in [Fault::None, Fault::NoLink] {
        for failure in [Fault::FailDestinationFileSync, Fault::FailDestinationDirSync] {
            let f = Fixture::new();
            let p = f.place(placement);
            inject(failure);
            let result = finish(&p);
            inject(Fault::None);
            assert!(result.unwrap_err().contains("original was restored"));
            assert_eq!(fs::read(&p.src).unwrap(), b"ORIGINAL");
            assert_eq!(fs::read(&p.dst).unwrap(), b"ORIGINAL");
            assert_eq!(fs::read_dir(p.src.parent().unwrap()).unwrap().count(), 1);
        }
    }
}
