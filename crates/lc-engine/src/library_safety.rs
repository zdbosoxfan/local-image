//! Catalog backup policy and read-only verification. Backups publish a complete directory only
//! after both catalog files have been synced, compared and replayed successfully.
use crate::{EngineError, Result, Session};
use lightcraft_catalog::{Journal, MemStore};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupFrequency {
    Never,
    EveryQuit,
    Daily,
    #[default]
    Weekly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BackupPreferences {
    pub frequency: BackupFrequency,
    /// Empty uses Backups inside the library. A chosen folder must already exist.
    pub folder: String,
    pub keep: usize,
    /// Persist a quit-time failure so GUI users can see it after reopening the library.
    pub last_error: Option<String>,
}
impl Default for BackupPreferences {
    fn default() -> Self {
        Self { frequency: BackupFrequency::Weekly, folder: String::new(), keep: 5, last_error: None }
    }
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    library: String,
    seconds: u64,
}
fn other(e: impl std::fmt::Display) -> EngineError {
    EngineError::Other(e.to_string())
}
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Replay into memory, never opening the source with a writable journal (which repairs damage).
pub fn verify_pair(dir: &Path) -> Result<usize> {
    let store = MemStore::new();
    for name in ["catalog.snap", "catalog.log"] {
        store.set(name, std::fs::read(dir.join(name)).map_err(other)?);
    }
    let (_, catalog, report) = Journal::open(Box::new(store))?;
    if report.failed > 0 || report.torn_bytes > 0 || report.damaged.is_some() || report.created {
        return Err(other(format!("catalog integrity failed: {report:?}")));
    }
    Ok(catalog.len())
}
fn backups(root: &Path, identity: &str) -> Vec<(u64, PathBuf)> {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !entry.file_type().ok()?.is_dir() || !entry.file_name().to_string_lossy().starts_with("catalog-backup-") {
                return None;
            }
            let manifest: Manifest = serde_json::from_slice(&std::fs::read(path.join("backup.json")).ok()?).ok()?;
            (manifest.version == 1 && manifest.library == identity).then_some((manifest.seconds, path))
        })
        .collect();
    entries.sort();
    entries
}

/// Copies a quiescent snapshot/log pair. A crash before directory rename leaves a hidden staging
/// folder; retention only considers published backups for this library, after successful publish.
fn publish_backup(source: &Path, root: &Path, identity: &str, seconds: u64, keep: usize) -> Result<PathBuf> {
    verify_pair(source)?;
    let suffix = format!("{seconds:020}-{}", uuid::Uuid::new_v4());
    let staging = root.join(format!(".catalog-backup-{suffix}.tmp"));
    let destination = root.join(format!("catalog-backup-{suffix}"));
    std::fs::create_dir(&staging).map_err(other)?;
    let result: Result<PathBuf> = (|| {
        for name in ["catalog.snap", "catalog.log"] {
            let src = source.join(name);
            li_fs::write_new_with(
                &staging.join(name),
                &mut |writer| std::io::copy(&mut std::fs::File::open(&src)?, writer).map(|_| ()),
                &mut |tmp| {
                    if crate::import_move::same_bytes(&src, tmp)? { Ok(()) } else { Err(std::io::Error::other("backup differs from source")) }
                },
            )
            .map_err(other)?;
            backup_cutpoint(name);
        }
        backup_cutpoint("pair");
        verify_pair(&staging)?;
        let manifest = serde_json::to_vec(&Manifest { version: 1, library: identity.into(), seconds }).map_err(other)?;
        li_fs::write_new(&staging.join("backup.json"), &manifest).map_err(other)?;
        backup_cutpoint("manifest");
        li_fs::sync_dir_checked(&staging).map_err(other)?;
        std::fs::rename(&staging, &destination).map_err(other)?;
        li_fs::sync_dir_checked(root).map_err(other)?;
        backup_cutpoint("published");
        Ok(destination.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    let path = result?;
    let entries = backups(root, identity);
    let remove = entries.len().saturating_sub(keep.max(1));
    for (_, old) in entries.into_iter().filter(|(_, p)| *p != path).take(remove) {
        std::fs::remove_dir_all(old).map_err(other)?;
    }
    li_fs::sync_dir_checked(root).map_err(other)?;
    Ok(path)
}
fn backup_cutpoint(_stage: &str) {
    #[cfg(test)]
    if std::env::var("LI_BACKUP_CRASH_STAGE").ok().as_deref() == Some(_stage) {
        std::process::exit(89);
    }
}
impl Session {
    /// Verify persisted data before compacting; corrupted source bytes are left untouched.
    pub fn verify_optimize_library(&mut self) -> Result<usize> {
        self.end_interaction()?;
        self.persist()?;
        let lib = self.library.as_mut().filter(|l| l.on_disk).ok_or_else(|| other("open a library on disk first"))?;
        // Finish any older background snapshot before reading a consistent pair.
        lib.finish_snapshot()?;
        let count = verify_pair(&lib.dir)?;
        self.compact_library()?;
        Ok(count)
    }
    /// Called after the closing snapshot, so backup writes cannot race catalog appends.
    pub fn backup_library_after_snapshot(&mut self, force: bool) -> Result<Option<PathBuf>> {
        let result = self.backup_library_inner(force);
        let warning = match &result {
            Err(error) => Some(error.to_string()),
            Ok(Some(_)) => None,
            Ok(None) => return result,
        };
        if self.backup_preferences.last_error != warning {
            self.backup_preferences.last_error = warning;
            if let Err(e) = self.save_prefs() {
                log::error!("backup status could not be saved: {e}");
            }
        }
        result
    }
    fn backup_library_inner(&mut self, force: bool) -> Result<Option<PathBuf>> {
        let Some(lib) = self.library.as_ref().filter(|l| l.on_disk) else { return Ok(None) };
        let prefs = &self.backup_preferences;
        if !force && prefs.frequency == BackupFrequency::Never {
            return Ok(None);
        }
        let root = if prefs.folder.is_empty() {
            let root = lib.dir.join("Backups");
            std::fs::create_dir_all(&root).map_err(other)?;
            root
        } else {
            let root = PathBuf::from(&prefs.folder);
            if !root.is_absolute() || !root.is_dir() {
                return Err(other("backup folder is unavailable; connect the backup drive"));
            }
            root
        };
        let seconds = now();
        let interval = match prefs.frequency {
            BackupFrequency::Daily => 86400,
            BackupFrequency::Weekly => 7 * 86400,
            _ => 0,
        };
        if !force
            && prefs.last_error.is_none()
            && interval > 0
            && backups(&root, &lib.identity).last().is_some_and(|(last, _)| seconds.saturating_sub(*last) < interval)
        {
            return Ok(None);
        }
        publish_backup(&lib.dir, &root, &lib.identity, seconds, prefs.keep).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(tag: &str) -> (PathBuf, Session) {
        let dir = std::env::temp_dir().join(format!("li-safety-{tag}-{}", uuid::Uuid::new_v4()));
        let mut session = Session::new();
        session.open_library(&dir, false).unwrap();
        let id = session.catalog.alloc_photo_id();
        let mut photo = lightcraft_catalog::Photo::new(id, lightcraft_catalog::Source::Demo { scene: 7 }, "fixture", "PNG", 8, 6, "2026-10-13");
        photo.rating = 5;
        photo.meta.title = "Backup retains metadata".into();
        session.commit("fixture", lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        session.backup_preferences.frequency = BackupFrequency::Never;
        session.close_library().unwrap();
        (dir, session)
    }
    #[test]
    fn backup_fault_preserves_previous_and_source_and_never_publishes_partial_pair() {
        let (dir, session) = fixture("crash");
        let root = dir.join("Backups");
        std::fs::create_dir(&root).unwrap();
        let identity = &session.library.as_ref().unwrap().identity;
        let first = publish_backup(&dir, &root, identity, 1, 1).unwrap();
        let before = std::fs::read(dir.join("catalog.snap")).unwrap();
        for limit in [0, 4] {
            let fault = li_fs::fail_writes_after(limit);
            // Keep a nonempty, valid log beside the snapshot during injected failures.
            std::fs::write(dir.join("catalog.log"), b"\n").unwrap();
            assert!(publish_backup(&dir, &root, identity, 2, 1).is_err());
            drop(fault);
            assert_eq!(std::fs::read(dir.join("catalog.snap")).unwrap(), before);
            assert!(first.is_dir());
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        }
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore = "subprocess fixture; invoked by backup_process_crashes"]
    fn backup_crash_child() {
        let source = PathBuf::from(std::env::var("LI_BACKUP_CRASH_SOURCE").unwrap());
        let root = source.join("Backups");
        publish_backup(&source, &root, "crash-test-library", 2, 1).unwrap();
    }
    #[test]
    fn backup_process_crashes_leave_previous_or_complete_new_pair() {
        for stage in ["catalog.snap", "catalog.log", "pair", "manifest", "published"] {
            let (dir, session) = fixture(stage);
            let root = dir.join("Backups");
            std::fs::create_dir(&root).unwrap();
            let first = publish_backup(&dir, &root, "crash-test-library", 1, 1).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "library_safety::tests::backup_crash_child", "--ignored"])
                .env("LI_BACKUP_CRASH_STAGE", stage)
                .env("LI_BACKUP_CRASH_SOURCE", &dir)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(89), "{stage}");
            assert!(first.is_dir());
            let complete = backups(&root, "crash-test-library");
            assert_eq!(complete.len(), if stage == "published" { 2 } else { 1 });
            for (_, path) in complete {
                assert_eq!(verify_pair(&path).unwrap(), session.catalog.len());
            }
            assert_eq!(verify_pair(&dir).unwrap(), session.catalog.len());
            drop(session);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn backup_retention_restore_and_incomplete_staging() {
        let (dir, session) = fixture("restore");
        let root = dir.join("Backups");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join(".catalog-backup-interrupted.tmp")).unwrap();
        let identity = &session.library.as_ref().unwrap().identity;
        let old = publish_backup(&dir, &root, identity, 1, 2).unwrap();
        publish_backup(&dir, &root, "another-library", 2, 1).unwrap();
        publish_backup(&dir, &root, identity, 3, 2).unwrap();
        let latest = publish_backup(&dir, &root, identity, 4, 2).unwrap();
        assert!(!old.exists());
        assert_eq!(backups(&root, identity).len(), 2);
        assert_eq!(verify_pair(&latest).unwrap(), session.catalog.len());
        assert!(root.join(".catalog-backup-interrupted.tmp").is_dir());
        let store = lightcraft_catalog::FsStore::open(&latest).unwrap();
        let (_, restored, _) = Journal::open(Box::new(store)).unwrap();
        assert_eq!(restored.to_snapshot(), session.catalog.to_snapshot());
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backup_quit_failure_is_visible_after_reopen_and_success_clears_it() {
        let (dir, mut session) = fixture("warning");
        session.backup_preferences.frequency = BackupFrequency::EveryQuit;
        session.backup_preferences.folder = dir.join("absent-drive").display().to_string();
        assert!(session.close_library().is_err());
        assert!(session.backup_preferences.last_error.is_some());
        drop(session);
        let mut reopened = Session::new();
        reopened.open_library(&dir, false).unwrap();
        assert!(reopened.backup_preferences.last_error.as_ref().unwrap().contains("unavailable"));
        reopened.backup_preferences.folder.clear();
        reopened.close_library().unwrap();
        assert!(reopened.backup_preferences.last_error.is_none());
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backup_policy_and_missing_drive_and_corruption() {
        let (dir, mut session) = fixture("policy");
        session.backup_preferences.frequency = BackupFrequency::Weekly;
        assert!(session.backup_library_after_snapshot(false).unwrap().is_some());
        assert!(session.backup_library_after_snapshot(false).unwrap().is_none());
        session.backup_preferences.last_error = Some("previous publication failed".into());
        assert!(session.backup_library_after_snapshot(false).unwrap().is_some(), "a failed publication must retry even inside the interval");
        assert!(session.backup_preferences.last_error.is_none());
        session.backup_preferences.frequency = BackupFrequency::Daily;
        assert!(session.backup_library_after_snapshot(false).unwrap().is_none());
        session.backup_preferences.frequency = BackupFrequency::EveryQuit;
        assert!(session.backup_library_after_snapshot(false).unwrap().is_some());
        session.backup_preferences.folder = dir.join("absent-drive").display().to_string();
        assert!(session.backup_library_after_snapshot(true).is_err());
        assert!(!dir.join("absent-drive").exists());
        std::fs::write(dir.join("catalog.log"), b"broken record\n").unwrap();
        assert!(session.verify_optimize_library().is_err());
        assert_eq!(std::fs::read(dir.join("catalog.log")).unwrap(), b"broken record\n");
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
