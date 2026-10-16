//! Durable, discoverable holding records for destructive Move finalization.
use li_fs::AtomicFs;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

const PREFIX: &str = "Local Image Move Recovery - ";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    id: uuid::Uuid,
    original_name: String,
    // These are recovery context only. Recovery never follows the destination or
    // deletes bytes based on a claimed content hash in a record.
    destination: Option<String>,
    expected_hash: Option<String>,
}

pub(super) struct Capture {
    dir: PathBuf,
    pub path: PathBuf,
    lock: File,
    parent_handle: same_file::Handle,
    dir_handle: same_file::Handle,
    record_handle: same_file::Handle,
    record_hash: lightcraft_preview::ContentId,
}

fn regular_handle(path: &Path, max: u64) -> io::Result<same_file::Handle> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("invalid Move recovery file"));
    }
    same_file::Handle::from_file(li_fs::read::open_regular(path, max)?)
}

fn directory_handle(path: &Path) -> io::Result<same_file::Handle> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(io::Error::other("invalid Move recovery directory"));
    }
    same_file::Handle::from_path(path)
}

impl Capture {
    pub fn take(source: &Path, destination: Option<&Path>, hash: Option<String>) -> io::Result<Self> {
        let parent = source.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let parent = fs::canonicalize(parent)?;
        let parent_handle = directory_handle(&parent)?;
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| io::Error::other("Move recovery requires a Unicode file name; original kept"))?;
        // Operate through the resolved parent, so changing a source-folder alias
        // cannot redirect capture or restoration into another folder.
        let source = parent.join(name);
        let _source = regular_handle(&source, li_fs::read::MAX_FILE_BYTES)?;
        let id = uuid::Uuid::new_v4();
        let dir = parent.join(format!("{PREFIX}{id}"));
        fs::create_dir(&dir)?;
        let prepared = (|| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
            }
            let dir_handle = directory_handle(&dir)?;
            let lock = File::options().read(true).write(true).create_new(true).open(dir.join("lock"))?;
            lock.try_lock().map_err(|e| io::Error::other(e.to_string()))?;
            let record = Record {
                version: 1,
                id,
                original_name: name.into(),
                destination: destination.map(|p| p.to_string_lossy().into()),
                expected_hash: hash,
            };
            let bytes = serde_json::to_vec(&record)?;
            li_fs::write_new(&dir.join("recovery.json"), &bytes)?;
            let record_handle = regular_handle(&dir.join("recovery.json"), 64 << 10)?;
            li_fs::sync_dir_checked(&dir)?;
            li_fs::sync_dir_checked(&parent)?;
            Ok::<_, io::Error>((lock, dir_handle, record_handle, lightcraft_preview::ContentId::of(&bytes)))
        })();
        let (lock, dir_handle, record_handle, record_hash) = match prepared {
            Ok(value) => value,
            // Before capture, no source bytes are inside this directory. Only
            // remove the exact named metadata files; never recurse into it.
            Err(error) => {
                let _ = fs::remove_file(dir.join("recovery.json"));
                let _ = fs::remove_file(dir.join("lock"));
                let _ = fs::remove_dir(&dir);
                return Err(error);
            }
        };
        let capture = Self { path: dir.join("payload"), dir, lock, parent_handle, dir_handle, record_handle, record_hash };
        capture.validate()?;
        if let Err(error) = li_fs::RealFs.rename_new(&source, &capture.path).map_err(|error| {
            if error.kind() != io::ErrorKind::Unsupported {
                return error;
            }
            // The private directory was exclusively created and is still empty.
            // Unsupported platforms keep the original rather than use replace.
            io::Error::other("atomic Move capture is unavailable; original kept")
        }) {
            capture.clean();
            return Err(error);
        }
        // Both names are durable before deleting captured bytes. Failure retains
        // the recorded capture for the next explicitly requested Move recovery.
        li_fs::sync_dir_checked(&capture.dir)
            .and_then(|()| li_fs::sync_dir_checked(&parent))
            .map_err(|e| io::Error::other(format!("{e}; original retained for recovery at {}", capture.path.display())))?;
        #[cfg(test)]
        if std::env::var("LI_MOVE_CRASH_STAGE").ok().as_deref() == Some("captured") {
            std::process::exit(89);
        }
        Ok(capture)
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        let Some(parent) = self.dir.parent() else { return Err(io::Error::other("Move recovery has no parent")) };
        let lock_handle = same_file::Handle::from_file(self.lock.try_clone()?)?;
        if directory_handle(parent)? != self.parent_handle
            || directory_handle(&self.dir)? != self.dir_handle
            || regular_handle(&self.dir.join("lock"), 1024)? != lock_handle
            || regular_handle(&self.dir.join("recovery.json"), 64 << 10)? != self.record_handle
            || lightcraft_preview::ContentId::of(&li_fs::read::file_with_chunk(&self.dir.join("recovery.json"), 64 << 10, 4096)?) != self.record_hash
        {
            return Err(io::Error::other("Move recovery record changed; retained without modification"));
        }
        Ok(())
    }

    pub fn restore(self, original: &Path) -> Result<(), String> {
        let restore = (|| {
            self.validate()?;
            let name = original.file_name().ok_or_else(|| io::Error::other("invalid restoration name"))?;
            let parent = self.dir.parent().ok_or_else(|| io::Error::other("missing restoration folder"))?;
            let original = parent.join(name);
            let _payload = regular_handle(&self.path, li_fs::read::MAX_FILE_BYTES)?;
            restore_new(&self.path, &original)?;
            li_fs::sync_dir_checked(parent)?;
            li_fs::sync_dir_checked(&self.dir)
        })();
        if let Err(e) = restore {
            return Err(format!("{e}; recovery record retained at {}", self.dir.display()));
        }
        self.clean();
        Ok(())
    }

    pub fn clean(self) {
        // A changed/malformed record or unexpected payload must remain visible.
        if self.validate().is_err() || !fs::symlink_metadata(&self.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
            return;
        }
        let _ = fs::remove_file(self.dir.join("recovery.json"));
        let _ = fs::remove_file(self.dir.join("lock"));
        let _ = fs::remove_dir(&self.dir);
        if let Some(parent) = self.dir.parent() {
            li_fs::sync_dir(parent);
        }
    }
}

fn restore_new(captured: &Path, original: &Path) -> io::Result<()> {
    li_fs::RealFs.rename_new(captured, original).or_else(|error| {
        if error.kind() != io::ErrorKind::Unsupported {
            return Err(error);
        }
        // A no-replace hard link preserves both names if durable publication
        // fails. Never unlink the capture on a failed copy/publication.
        fs::hard_link(captured, original)?;
        li_fs::sync_dir_checked(original.parent().unwrap_or(Path::new(".")))?;
        fs::remove_file(captured)
    })
}

/// Restore only recorded captures immediately inside the selected source folder.
/// Live captures hold a process lock and are skipped. An occupied original name
/// leaves recovery bytes intact and emits their visible location.
pub(super) fn recover_pending(parent: &Path) {
    let Ok(parent) = fs::canonicalize(parent) else { return };
    let Ok(entries) = fs::read_dir(&parent) else { return };
    for entry in entries.take(crate::walk::MAX_ENTRIES).flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|n| n.strip_prefix(PREFIX)).and_then(|s| uuid::Uuid::parse_str(s).ok()) else { continue };
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let dir = entry.path();
        let loaded = (|| {
            let parent_handle = directory_handle(&parent)?;
            let dir_handle = directory_handle(&dir)?;
            let lock = regular_handle(&dir.join("lock"), 1024)?.as_file().try_clone()?;
            lock.try_lock().map_err(|e| io::Error::other(e.to_string()))?;
            let record_handle = regular_handle(&dir.join("recovery.json"), 64 << 10)?;
            let bytes = li_fs::read::file_with_chunk(&dir.join("recovery.json"), 64 << 10, 4096)?;
            let record: Record = serde_json::from_slice(&bytes)?;
            let mut parts = Path::new(&record.original_name).components();
            let valid_hash = record.expected_hash.as_ref().is_none_or(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()));
            if record.version != 1
                || record.id != id
                || !matches!(parts.next(), Some(Component::Normal(_)))
                || parts.next().is_some()
                || Path::new(&record.original_name).file_name().and_then(|name| name.to_str()) != Some(record.original_name.as_str())
                || !valid_hash
                || record.destination.is_some() != record.expected_hash.is_some()
            {
                return Err(io::Error::other("invalid Move recovery record"));
            }
            let capture = Capture {
                path: dir.join("payload"),
                dir: dir.clone(),
                lock,
                parent_handle,
                dir_handle,
                record_handle,
                record_hash: lightcraft_preview::ContentId::of(&bytes),
            };
            capture.validate()?;
            Ok::<_, io::Error>((capture, record))
        })();
        let Ok((capture, record)) = loaded else { continue };
        match fs::symlink_metadata(&capture.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => capture.clean(),
            Ok(m) if m.is_file() && !m.file_type().is_symlink() => {
                let original = parent.join(record.original_name);
                match capture.restore(&original) {
                    Ok(()) => log::warn!("restored an interrupted Move source at {}", original.display()),
                    Err(error) => log::warn!("interrupted Move recovery: {error}"),
                }
            }
            _ => {}
        }
    }
}
