//! Byte storage for the catalog files (`catalog.snap`, `catalog.log`).
//!
//! [`FsStore`] is the native implementation (a library directory); [`MemStore`] keeps files in
//! memory (tests, ephemeral sessions) and lets tests simulate crashes by editing the bytes. A web
//! host can implement [`Store`] over OPFS.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Files in a directory.
pub struct FsStore {
    dir: PathBuf,
    /// Open append handle (kept between appends; dropped on rewrite/truncate).
    appender: Option<(String, std::fs::File)>,
}

impl FsStore {
    /// Use `dir` (created if missing).
    pub fn open(dir: impl AsRef<Path>) -> io::Result<FsStore> {
        std::fs::create_dir_all(dir.as_ref())?;
        Ok(FsStore { dir: dir.as_ref().to_path_buf(), appender: None })
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    fn sync_dir(&self) {
        // Makes the rename durable on POSIX; opening a directory fails on Windows (where
        // `rename` is already journaled by NTFS), so errors are ignored.
        if let Ok(d) = std::fs::File::open(&self.dir) {
            let _ = d.sync_all();
        }
    }
}

impl Store for FsStore {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.path(name)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write_atomic(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        if self.appender.as_ref().is_some_and(|(n, _)| n == name) {
            self.appender = None;
        }
        let path = resolve_regular_file(&self.path(name))?;
        li_fs::write_atomic(&path, data)
    }

    fn write_atomic_with(&mut self, name: &str, fill: &mut dyn FnMut(&mut dyn io::Write) -> io::Result<()>) -> io::Result<u64> {
        if self.appender.as_ref().is_some_and(|(n, _)| n == name) {
            self.appender = None;
        }
        let path = resolve_regular_file(&self.path(name))?;
        li_fs::write_atomic_with(&path, fill)
    }

    fn append(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        if self.appender.as_ref().is_none_or(|(n, _)| n != name) {
            let path = resolve_regular_file(&self.path(name))?;
            let existed = path.exists();
            let f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
            if !existed {
                self.sync_dir();
            }
            self.appender = Some((name.to_string(), f));
        }
        let Some((_, f)) = self.appender.as_mut() else { return Err(io::Error::other("appender not open")) };
        let written = f.write_all(data).and_then(|()| f.sync_data());
        if written.is_err() {
            // The handle may be dead (a network share that reconnected, a removed volume): the next
            // attempt reopens the file. Part of `data` may be in the file; the caller cuts it back
            // ([`Store::truncate`]) before appending again.
            self.appender = None;
        }
        written
    }

    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        if self.appender.as_ref().is_some_and(|(n, _)| n == name) {
            self.appender = None;
        }
        let path = resolve_regular_file(&self.path(name))?;
        let f = match std::fs::OpenOptions::new().write(true).open(path) {
            Ok(f) => f,
            // nothing to cut (an append that failed before creating the file)
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        f.set_len(len)?;
        f.sync_all()
    }

    fn describe(&self) -> String {
        self.dir.display().to_string()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn background_writer(&self) -> Option<Box<dyn Store>> {
        Some(Box::new(FsStore { dir: self.dir.clone(), appender: None }))
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
