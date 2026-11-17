//! Byte storage for the catalog files (`catalog.snap`, `catalog.log`).
//!
//! [`FsStore`] is the native implementation (a library directory); [`MemStore`] keeps files in
//! memory (tests, ephemeral sessions) and lets tests simulate crashes by editing the bytes. A web
//! host can implement [`Store`] over OPFS. [`FsStore`] writes only while its directory still leads
//! to the folder it opened ([`crate::OpenedFolder`]).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::journal::LOG;
use crate::opened_folder::OpenedFolder;

/// A flat namespace of named files with the few operations the journal needs.
pub trait Store: Send {
    /// The whole file, or `None` if it doesn't exist.
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>>;
    /// Replace a file atomically: after a crash either the old or the new content is visible,
    /// never a mix (native: write temp + fsync + rename + fsync directory).
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> io::Result<()>;
    /// [`Store::write_atomic`] with streamed content: `fill` writes the new content into the
    /// given writer (an error aborts and leaves the old file). Same durability: the file is
    /// replaced only once all of it is written (native: and fsynced). Returns its length.
    ///
    /// The default buffers the content in memory; [`FsStore`] streams it into the temp file, so
    /// a large snapshot is never held in memory as a whole.
    fn write_atomic_with(&mut self, name: &str, fill: &mut dyn FnMut(&mut dyn io::Write) -> io::Result<()>) -> io::Result<u64> {
        let mut buf = Vec::new();
        fill(&mut buf)?;
        self.write_atomic(name, &buf)?;
        Ok(buf.len() as u64)
    }
    /// Append and make durable (fsync) before returning. On an error, part of `data` may have
    /// been written (a full disk, a dropped network share): the journal cuts the file back to its
    /// previous length with [`Store::truncate`] before appending again.
    fn append(&mut self, name: &str, data: &[u8]) -> io::Result<()>;
    /// Cut a file to `len` bytes (drop a torn tail before appending again). A missing file is
    /// left missing.
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()>;
    /// Human-readable location (diagnostics).
    fn describe(&self) -> String;
    /// The target of a symbolic link at `name`, or `None` for an ordinary entry or missing name.
    /// Stores without filesystem links need no override. Callers validate content before writes.
    fn link_target(&self, _name: &str) -> io::Result<Option<PathBuf>> {
        Ok(None)
    }
    /// Another handle on the same files that a worker thread can write `catalog.snap` through
    /// while this one keeps appending to the log (background compaction). `None` (the default):
    /// snapshots are written synchronously.
    fn background_writer(&self) -> Option<Box<dyn Store>> {
        None
    }
}

/// Replace the entry itself, never a link's target; exclusive creation refuses a raced-in entry.
pub(crate) fn create_staging(path: &Path) -> io::Result<std::fs::File> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => std::fs::remove_file(path)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

/// Keep relocated catalog files at their targets without replacing the user's links.
fn resolve_regular_file(path: &Path) -> io::Result<PathBuf> {
    let (path, metadata) = match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_symlink() => {
            let target = std::fs::canonicalize(path)?;
            let metadata = std::fs::symlink_metadata(&target)?;
            (target, metadata)
        }
        Ok(md) => (path.to_path_buf(), md),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(path.to_path_buf()),
        Err(e) => return Err(e),
    };
    if metadata.is_file() {
        Ok(path)
    } else {
        Err(io::Error::other(format!("{} is a {:?}, not a regular file", path.display(), metadata.file_type())))
    }
}

/// What this store knows of the log at the path: its length as last read or written (`None`:
/// neither yet), and the handle of an append that failed, whose torn tail only it may cut. In a
/// folder accepted by this session's note rather than its numbers (a copy of the library's folder
/// put at its path), a log of another length is another file: appending to it would leave a gap,
/// so it is refused until a snapshot replaces it.
#[derive(Default)]
struct LogState {
    good: Option<u64>,
    torn: Option<std::fs::File>,
}

fn check_log_len(len: u64, good: u64) -> io::Result<()> {
    if len == good {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "catalog log holds {len} bytes, the log this session read or wrote has {good}: it is another file, so a snapshot must replace it"
    )))
}

/// Files in a directory.
pub struct FsStore {
    dir: PathBuf,
    /// The folder `dir` named when the store was opened. Every write asks it first and is refused
    /// when `dir` leads somewhere else now.
    folder: OpenedFolder,
    log: LogState,
    /// The folder entry of the file appended to is durable (a directory sync succeeded since the
    /// last failed append).
    dir_synced: bool,
}

impl FsStore {
    /// Use `dir` (created if missing). The store writes to the folder `dir` names now, and only
    /// while it still does.
    pub fn open(dir: impl AsRef<Path>) -> io::Result<FsStore> {
        std::fs::create_dir_all(dir.as_ref())?;
        Ok(FsStore::in_folder(OpenedFolder::at(dir.as_ref())?))
    }
    /// A store in a folder that was recorded already, so that the stores and the lock of one
    /// library ask about the same folder.
    pub fn in_folder(folder: OpenedFolder) -> FsStore {
        FsStore { dir: folder.path().to_path_buf(), folder, log: LogState::default(), dir_synced: false }
    }
    /// The folder the store opened.
    pub fn folder(&self) -> &OpenedFolder {
        &self.folder
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    #[cfg(not(windows))]
    fn sync_dir(dir: &Path) -> io::Result<()> {
        std::fs::File::open(dir)?.sync_all()
    }

    #[cfg(windows)]
    fn sync_dir(_: &Path) -> io::Result<()> {
        // File::open cannot open Windows directories; NTFS journals directory-entry changes.
        Ok(())
    }

    fn append_with_sync_dir(&mut self, name: &str, data: &[u8], sync_dir: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
        use std::io::{Seek, Write};
        self.folder.check()?;
        // Opened by its path for every append: a handle kept open would go on writing into a
        // folder moved away while a copy of it (this session's note too) is at the path.
        let path = resolve_regular_file(&self.path(name))?;
        let mut f = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(path)?;
        if !self.dir_synced {
            // An existing entry may be left by an append whose directory sync failed.
            sync_dir(&self.dir)?;
            self.dir_synced = true;
        }
        let len = f.metadata()?.len();
        if name == LOG {
            let good = *self.log.good.get_or_insert(len);
            if self.folder.renumbered() {
                check_log_len(len, good)?;
            }
        }
        f.seek(io::SeekFrom::Start(len))?;
        let written = f.write_all(data).and_then(|()| f.sync_data());
        if name == LOG {
            match written {
                Ok(()) => self.log.good = Some(len.saturating_add(data.len() as u64)),
                Err(_) => self.log.torn = Some(f),
            }
        }
        if written.is_err() {
            // Part of `data` may be in the file; the caller cuts it back ([`Store::truncate`])
            // before appending again.
            self.dir_synced = false;
        }
        written
    }

    /// Cut the torn tail of a failed log append back through the handle that wrote it, never
    /// through a file now at its path.
    fn roll_back_torn_log(&mut self) -> io::Result<()> {
        if let (Some(f), Some(good)) = (self.log.torn.as_ref(), self.log.good) {
            if f.metadata()?.len() < good {
                return Err(io::Error::other("the failed log was shortened: a snapshot must replace it"));
            }
            f.set_len(good)?;
            f.sync_data()?;
        }
        self.log.torn = None;
        Ok(())
    }

    /// Replace `name` atomically while `dir` leads to the opened folder, asked again once the
    /// content is staged (a large snapshot takes a while).
    fn replace(&mut self, name: &str, fill: &mut dyn FnMut(&mut dyn io::Write) -> io::Result<()>) -> io::Result<u64> {
        self.folder.check()?;
        let path = resolve_regular_file(&self.path(name))?;
        let staged = li_fs::stage_stream(&path, li_fs::Policy::Durable, fill)?;
        self.folder.check()?;
        let n = staged.len();
        staged.commit()?;
        if name == LOG {
            self.log = LogState { good: Some(n), torn: None };
        }
        Ok(n)
    }
}

impl Store for FsStore {
    fn link_target(&self, name: &str) -> io::Result<Option<PathBuf>> {
        let path = self.path(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = std::fs::read_link(path)?;
                Ok(Some(if target.is_absolute() { target } else { self.dir.join(target) }))
            }
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let bytes = match std::fs::read(self.path(name)) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if name == LOG && self.log.good.is_none() {
            // what the journal read is the log appends must find
            self.log.good = Some(bytes.as_ref().map_or(0, |b| b.len() as u64));
        }
        Ok(bytes)
    }

    fn write_atomic(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        self.replace(name, &mut |w| w.write_all(data)).map(|_| ())
    }

    fn write_atomic_with(&mut self, name: &str, fill: &mut dyn FnMut(&mut dyn io::Write) -> io::Result<()>) -> io::Result<u64> {
        self.replace(name, fill)
    }

    fn append(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        self.append_with_sync_dir(name, data, Self::sync_dir)
    }

    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        self.folder.check()?;
        if name == LOG {
            self.roll_back_torn_log()?;
        }
        let path = resolve_regular_file(&self.path(name))?;
        let f = match std::fs::OpenOptions::new().write(true).open(path) {
            Ok(f) => f,
            // nothing to cut (an append that failed before creating the file)
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if name == LOG
            && let Some(good) = self.log.good
            && self.folder.renumbered()
        {
            // a refused append wrote nothing: its cleanup must not cut or extend another log
            check_log_len(f.metadata()?.len(), good)?;
        }
        f.set_len(len)?;
        f.sync_all()?;
        if name == LOG {
            self.log.good = Some(len);
        }
        Ok(())
    }

    fn describe(&self) -> String {
        self.dir.display().to_string()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn background_writer(&self) -> Option<Box<dyn Store>> {
        Some(Box::new(FsStore::in_folder(self.folder.clone())))
    }
}

/// In-memory files. Clones share the same files (so a test can "reopen" or corrupt them).
#[derive(Clone, Default)]
pub struct MemStore {
    pub files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    /// Offer a [`Store::background_writer`] (off by default: snapshots stay synchronous).
    pub background: bool,
}

impl MemStore {
    pub fn new() -> MemStore {
        MemStore::default()
    }
    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
    }
    pub fn set(&self, name: &str, data: Vec<u8>) {
        self.files.lock().unwrap_or_else(|e| e.into_inner()).insert(name.to_string(), data);
    }
}

impl Store for MemStore {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.get(name))
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        self.set(name, data.to_vec());
        Ok(())
    }
    fn append(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        self.files.lock().unwrap_or_else(|e| e.into_inner()).entry(name.to_string()).or_default().extend_from_slice(data);
        Ok(())
    }
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        if let Some(f) = self.files.lock().unwrap_or_else(|e| e.into_inner()).get_mut(name) {
            f.truncate(len as usize);
        }
        Ok(())
    }
    fn describe(&self) -> String {
        "memory".into()
    }
    fn background_writer(&self) -> Option<Box<dyn Store>> {
        self.background.then(|| Box::new(self.clone()) as Box<dyn Store>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn append_retries_directory_sync_after_failure() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!("lc-store-dir-sync-{}", std::process::id()));
        std::fs::create_dir(&dir)?;
        let mut store = FsStore::open(&dir)?;
        let calls = Cell::new(0);
        let fail = Cell::new(true);
        let sync_dir = |dir: &Path| {
            calls.set(calls.get() + 1);
            if fail.get() { Err(io::Error::from_raw_os_error(5)) } else { FsStore::sync_dir(dir) }
        };

        for _ in 0..2 {
            let result = store.append_with_sync_dir("catalog.log", b"committed\n", sync_dir);
            assert!(matches!(result, Err(ref error) if error.raw_os_error() == Some(5)), "{result:?}");
            assert_eq!(std::fs::read(dir.join("catalog.log"))?, b"");
            store.truncate("catalog.log", 0)?;
        }
        assert_eq!(calls.get(), 2);

        #[cfg(not(windows))]
        {
            let result = store.append_with_sync_dir("catalog.log", b"committed\n", |dir| FsStore::sync_dir(&dir.join("missing")));
            assert!(matches!(result, Err(ref error) if error.kind() == io::ErrorKind::NotFound), "{result:?}");
            assert_eq!(std::fs::read(dir.join("catalog.log"))?, b"");
        }

        fail.set(false);
        store.append_with_sync_dir("catalog.log", b"committed\n", sync_dir)?;
        store.append_with_sync_dir("catalog.log", b"cached\n", sync_dir)?;
        assert_eq!(calls.get(), 3);
        assert_eq!(std::fs::read(dir.join("catalog.log"))?, b"committed\ncached\n");
        drop(store);

        let mut reopened = FsStore::open(&dir)?;
        reopened.append("catalog.log", b"reopened\n")?;
        assert_eq!(std::fs::read(dir.join("catalog.log"))?, b"committed\ncached\nreopened\n");
        drop(reopened);
        std::fs::remove_dir_all(dir)
    }
}
