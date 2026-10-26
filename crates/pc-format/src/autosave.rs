//! Autosave and crash recovery (architecture §9): the same incremental writer
//! saves a document *snapshot* into a recovery directory on a background
//! thread, so the UI never blocks.
//!
//! Layout: `<recovery_dir>/<key>.pcraft/` (directory bundles, so repeated
//! autosaves write only changed tiles) plus `<key>.json` with
//! [`RecoveryInfo`]. Native targets only (threads and a filesystem).
//!
//! [`RecoveryStore`] is the app-facing lifecycle: an entry is only deleted once
//! the document is saved or closed, never just because it was recovered.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use photocraft_doc::Document;
use serde::{Deserialize, Serialize};

use crate::{FormatError, PcraftWriter, Result, SaveOptions, SaveStats};

/// Sidecar describing an autosave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryInfo {
    pub key: String,
    pub document_name: String,
    /// Where the user last saved the document, if anywhere.
    pub original_path: Option<String>,
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    pub revision: u64,
}

/// One recoverable document found by [`list_recovery`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryEntry {
    pub info: RecoveryInfo,
    pub bundle: PathBuf,
}

struct Job {
    snapshot: Arc<Document>,
    info: RecoveryInfo,
    opts: SaveOptions,
}

/// One completed revision's disk-write outcome (distinct from enqueue success).
type SaveOutcome = (u64, std::result::Result<(), String>);
type CompletedWrites = Arc<Mutex<Vec<SaveOutcome>>>;

/// Background autosaver for one document. Requests are coalesced: if saves
/// arrive faster than they complete, only the newest snapshot is written.
pub struct Autosaver {
    dir: PathBuf,
    key: String,
    tx: Option<Sender<Job>>,
    handle: Option<JoinHandle<()>>,
    last: Arc<Mutex<Option<Result<SaveStats>>>>,
    /// Per-request write outcomes, delivered to the app without blocking its frame.
    completed: CompletedWrites,
    discard: Arc<AtomicBool>,
}

fn sanitize(key: &str) -> String {
    let s: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() { "untitled".into() } else { s }
}

impl Autosaver {
    /// `key` identifies the document across autosaves (e.g. its DocId).
    pub fn new(recovery_dir: impl Into<PathBuf>, key: &str) -> Self {
        let dir = recovery_dir.into();
        let key = sanitize(key);
        let (tx, rx) = mpsc::channel::<Job>();
        let last = Arc::new(Mutex::new(None));
        let bundle = dir.join(format!("{key}.pcraft"));
        let sidecar = dir.join(format!("{key}.json"));
        let last2 = last.clone();
        let discard = Arc::new(AtomicBool::new(false));
        let discarded = discard.clone();
        let completed: CompletedWrites = Arc::new(Mutex::new(Vec::new()));
        let completed2 = completed.clone();
        // A thread that never started must not look like a working autosave: record the failure
        // where `last_result`/`flush` report it, so recovery health checks see it (upstream
        // PhotoCraft 3ef63161).
        let spawned = std::thread::Builder::new().name(format!("autosave-{key}")).spawn(move || worker(rx, bundle, sidecar, last2, completed2, discarded));
        let handle = match spawned {
            Ok(h) => Some(h),
            Err(e) => {
                note(&last, Err(FormatError::Io(std::io::Error::other(format!("the autosave thread could not start: {e}")))));
                None
            }
        };
        Autosaver { dir, key, tx: Some(tx), handle, last, completed, discard }
    }

    pub fn bundle_path(&self) -> PathBuf {
        self.dir.join(format!("{}.pcraft", self.key))
    }

    /// Queue a snapshot for saving (returns immediately). Callers that only need best-effort
    /// autosaving can use this; the app uses [`request_checked`](Self::request_checked).
    pub fn request(&self, snapshot: Arc<Document>, revision: u64, original_path: Option<String>, opts: SaveOptions) {
        let _ = self.request_checked(snapshot, revision, original_path, opts);
    }

    /// Queue a snapshot, reporting a worker that could not start or has stopped. Success here
    /// means queued, not written: [`take_completed`](Self::take_completed) has the write outcomes.
    pub fn request_checked(&self, snapshot: Arc<Document>, revision: u64, original_path: Option<String>, opts: SaveOptions) -> std::result::Result<(), String> {
        let info = RecoveryInfo {
            key: self.key.clone(),
            document_name: snapshot.name.clone(),
            original_path,
            saved_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            revision,
        };
        self.tx
            .as_ref()
            .ok_or_else(|| "the autosave worker is stopped".to_string())?
            .send(Job { snapshot, info, opts })
            .map_err(|_| self.last_result().and_then(std::result::Result::err).unwrap_or_else(|| "the autosave worker is unavailable".to_string()))
    }

    /// Drain completed write outcomes (revision, result) without waiting for the worker.
    pub fn take_completed(&self) -> Vec<(u64, std::result::Result<(), String>)> {
        self.completed.lock().map(|mut results| std::mem::take(&mut *results)).unwrap_or_default()
    }

    /// Result of the most recent completed save.
    pub fn last_result(&self) -> Option<std::result::Result<SaveStats, String>> {
        let g = self.last.lock().ok()?;
        g.as_ref().map(|r| r.as_ref().map(|s| *s).map_err(|e| e.to_string()))
    }

    /// Finish pending saves and stop the thread.
    pub fn flush(mut self) -> Option<std::result::Result<SaveStats, String>> {
        self.shutdown();
        self.last_result()
    }

    /// The document was saved normally or closed: drop its recovery data.
    pub fn discard(mut self) -> Result<Discard> {
        self.discard.store(true, Ordering::SeqCst);
        self.tx.take();
        // The writer owns final deletion, after any atomic write already in flight.
        // Detach it so neither this call nor Drop joins it on the UI thread.
        if let Some(handle) = self.handle.take() { Ok(Discard { handle: Some(handle) }) } else { discard_entry_async(self.dir.clone(), self.key.clone()) }
    }

    fn shutdown(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker(
    rx: Receiver<Job>,
    bundle: PathBuf,
    sidecar: PathBuf,
    last: Arc<Mutex<Option<Result<SaveStats>>>>,
    completed: CompletedWrites,
    discard: Arc<AtomicBool>,
) {
    let mut writer = PcraftWriter::new();
    while let Ok(mut job) = rx.recv() {
        // Coalesce: skip to the newest queued snapshot.
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }
        if discard.load(Ordering::SeqCst) {
            break;
        }
        let r = (|| {
            create_private_dir(&bundle)?;
            let stats = writer.save_dir(&job.snapshot, &bundle, &job.opts)?;
            li_fs::atomic_write_private(&sidecar, &serde_json::to_vec_pretty(&job.info)?)?;
            Ok(stats)
        })();
        if let Ok(mut results) = completed.lock() {
            results.push((job.info.revision, r.as_ref().map(|_| ()).map_err(ToString::to_string)));
        }
        note(&last, r);
    }
    if discard.load(Ordering::SeqCst) {
        let dir = bundle.parent().unwrap_or_else(|| Path::new("."));
        let key = bundle.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if let Err(e) = remove_entry(dir, key) {
            log::warn!("autosave cleanup: {e}");
        }
    }
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn read_sidecar_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    const MAX_BYTES: u64 = 1 << 20;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(not(unix))]
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "recovery metadata must be a regular file"));
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "recovery metadata must be a regular file of at most 1 MiB"));
    }
    let mut limited = file.take(MAX_BYTES + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes)?;
    if limited.limit() == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "recovery metadata exceeds 1 MiB"));
    }
    Ok(bytes)
}

/// Recoverable documents in `recovery_dir`, newest first. A sidecar must be named after its own
/// (sanitized) key, so a stray or hand-edited one can never point outside the directory.
pub fn list_recovery(recovery_dir: &Path) -> Vec<RecoveryEntry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(recovery_dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        // Invalid sidecars are skipped without modifying their recovery bundles.
        if e.file_type().is_ok_and(|ft| ft.is_file())
            && p.extension().is_some_and(|x| x == "json")
            && let Ok(bytes) = read_sidecar_bounded(&p)
            && let Ok(info) = serde_json::from_slice::<RecoveryInfo>(&bytes)
            && sanitize(&info.key) == info.key
            && p.file_stem().is_some_and(|stem| *stem == *info.key)
        {
            let bundle = recovery_dir.join(format!("{}.pcraft", info.key));
            if bundle.join(crate::store::MANIFEST).is_file() {
                out.push(RecoveryEntry { info, bundle });
            }
        } else if e.file_type().is_ok_and(|ft| ft.is_dir())
            && let Some(entry) = sidecarless_entry(recovery_dir, &p)
        {
            out.push(entry);
        }
    }
    out.sort_by(|a, b| b.info.saved_at.cmp(&a.info.saved_at).then(a.info.key.cmp(&b.info.key)));
    out
}

/// A committed `<key>.pcraft` bundle (its manifest is written last) whose sidecar was never
/// published, e.g. a crash during a document's first autosave. The metadata is rebuilt from the
/// manifest (read like a load reads it) and its modification time. Incomplete bundles (no
/// readable manifest) and bundles with anything at the sidecar's path are not entries.
fn sidecarless_entry(recovery_dir: &Path, bundle: &Path) -> Option<RecoveryEntry> {
    let key = bundle.file_name()?.to_str()?.strip_suffix(".pcraft")?;
    if key.is_empty() || sanitize(key) != key || std::fs::symlink_metadata(recovery_dir.join(format!("{key}.json"))).is_ok() {
        return None;
    }
    let meta = std::fs::symlink_metadata(bundle.join(crate::store::MANIFEST)).ok().filter(std::fs::Metadata::is_file)?;
    let source = crate::store::DirSource { root: bundle.to_path_buf() };
    let manifest = crate::store::read_manifest(&source, &crate::LoadOptions::default()).ok()?;
    let saved_at = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let info = RecoveryInfo { key: key.to_string(), document_name: manifest.document.name, original_path: None, saved_at, revision: 0 };
    Some(RecoveryEntry { info, bundle: bundle.to_path_buf() })
}

/// Load a recovered document.
pub fn recover(entry: &RecoveryEntry) -> Result<Document> {
    crate::load_path(&entry.bundle)
}

/// Delete a recovery entry.
pub fn discard_recovery(recovery_dir: &Path, entry: &RecoveryEntry) -> Result<()> {
    remove_entry(recovery_dir, &entry.info.key)
}

/// The crash-recovery autosaves of one app session, keyed by document id (the `DocId` value).
///
/// New documents save under `doc-<session>-<id>`, with a `<session>` unique to this store: ids
/// restart every launch, so a new document must never overwrite an entry an earlier launch left.
/// A recovered document [adopts](Self::adopt) the entry it was loaded from. Its autosaves replace
/// that entry in place (each save is atomic, so the old copy stays loadable until the new one is
/// complete), and saving or closing it removes the entry. Nothing is deleted just because it was
/// recovered, so a second crash before the next autosave loses nothing.
#[cfg(not(target_arch = "wasm32"))]
pub struct RecoveryStore {
    dir: PathBuf,
    session: String,
    savers: HashMap<u64, Autosaver>,
    adopted: HashMap<u64, String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl RecoveryStore {
    pub fn new(recovery_dir: impl Into<PathBuf>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static STORES: AtomicU64 = AtomicU64::new(0);
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let session = format!("{t:x}_{:x}_{:x}", std::process::id(), STORES.fetch_add(1, Ordering::Relaxed));
        let dir = recovery_dir.into();
        sweep_orphans(&dir, ORPHAN_AGE);
        RecoveryStore { dir, session, savers: HashMap::new(), adopted: HashMap::new() }
    }

    /// Load every recoverable document, newest first, with the entry it came from. Nothing is
    /// deleted: [`adopt`](Self::adopt) each document the app opens. Entries that fail to load stay
    /// on disk untouched.
    pub fn recover(&self) -> Vec<(RecoveryEntry, Document)> {
        list_recovery(&self.dir).into_iter().filter_map(|e| recover(&e).ok().map(|doc| (e, doc))).collect()
    }

    /// Document `doc_id` was opened from the recovery entry `key`: its autosaves now replace that
    /// entry, and [`discard`](Self::discard) removes it.
    pub fn adopt(&mut self, doc_id: u64, key: &str) {
        self.adopted.insert(doc_id, sanitize(key));
    }

    /// Queue an autosave of `doc` (returns immediately; see [`Autosaver::request`]).
    pub fn autosave(&mut self, doc: &Arc<Document>, revision: u64, original_path: Option<String>) {
        let _ = self.autosave_checked(doc, revision, original_path);
    }

    /// Queue an autosave of `doc`, reporting enqueue failures (a worker that never started or
    /// stopped). Write outcomes arrive later through [`take_completed`](Self::take_completed).
    pub fn autosave_checked(&mut self, doc: &Arc<Document>, revision: u64, original_path: Option<String>) -> std::result::Result<(), String> {
        let id = doc.id.0;
        let saver = self.savers.entry(id).or_insert_with(|| {
            let key = self.adopted.get(&id).cloned().unwrap_or_else(|| format!("doc-{}-{id}", self.session));
            Autosaver::new(&self.dir, &key)
        });
        saver.request_checked(doc.clone(), revision, original_path, SaveOptions::default())
    }

    /// Completed write outcomes as (document id, revision, result), without blocking.
    pub fn take_completed(&mut self) -> Vec<(u64, u64, std::result::Result<(), String>)> {
        self.savers.iter().flat_map(|(&id, saver)| saver.take_completed().into_iter().map(move |(revision, result)| (id, revision, result))).collect()
    }

    /// Finish every queued autosave and wait for it, then return the outcomes as
    /// [`take_completed`](Self::take_completed) does. For a session that ends without asking,
    /// e.g. because the window's graphics device was lost. The workers stop; a later
    /// autosave of the same document starts a new one on the same entry, so a failed write can be
    /// tried again.
    pub fn flush(&mut self) -> Vec<(u64, u64, std::result::Result<(), String>)> {
        for saver in self.savers.values_mut() {
            saver.shutdown();
        }
        let done = self.take_completed();
        self.savers.clear();
        done
    }

    /// Document `doc_id` was saved or closed: remove its recovery data, both its own autosaves
    /// and the entry it adopted.
    pub fn discard(&mut self, doc_id: u64) -> Result<Discard> {
        let adopted = self.adopted.remove(&doc_id);
        match self.savers.remove(&doc_id) {
            Some(saver) => saver.discard(),
            None => adopted.map_or(Ok(Discard { handle: None }), |key| discard_entry_async(self.dir.clone(), key)),
        }
    }
}

/// Completion receipt for asynchronous recovery cleanup. Dropping it detaches the worker.
/// Tests and non-UI callers can wait explicitly; closing a document never waits.
pub struct Discard {
    handle: Option<JoinHandle<()>>,
}
impl Discard {
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn wait(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }
}
fn discard_entry_async(dir: PathBuf, key: String) -> Result<Discard> {
    let handle = std::thread::Builder::new().name("autosave-cleanup".into()).spawn(move || {
        if let Err(e) = remove_entry(&dir, &key) {
            log::warn!("autosave cleanup: {e}");
        }
    })?;
    Ok(Discard { handle: Some(handle) })
}

/// How long recovery data must have been left untouched before [`sweep_orphans`] may remove
/// it. Far longer than any save takes, so another running instance's in-progress first
/// autosave (bundle written, sidecar not yet) is never mistaken for an orphan.
#[cfg(not(target_arch = "wasm32"))]
const ORPHAN_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Best-effort removal of recovery data nothing can ever load or clean up (upstream PhotoCraft
/// c20b81b2):
/// - `<key>.pcraft` bundle folders with no `<key>.json` sidecar and no readable manifest (a
///   crash in the middle of a document's first autosave). A sidecar-less bundle whose manifest
///   was committed is complete and listed by `list_recovery`, so it is kept;
/// - `*.pcsave.tmp` files left by an interrupted atomic write (see [`li_fs::temp_name`])
///   that another process created.
///
/// Only entries untouched for at least `min_age` are removed. Bundles with a sidecar, symlinks,
/// and anything outside `recovery_dir` are never touched; errors are ignored.
#[cfg(not(target_arch = "wasm32"))]
fn sweep_orphans(recovery_dir: &Path, min_age: std::time::Duration) {
    let Ok(rd) = std::fs::read_dir(recovery_dir) else { return };
    let mut bundles = Vec::new();
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_file() {
            sweep_temp_file(&path, min_age);
            continue;
        }
        if !ft.is_dir() {
            continue;
        }
        let name = e.file_name();
        let Some(key) = name.to_str().and_then(|n| n.strip_suffix(".pcraft")) else { continue };
        if key.is_empty() || sanitize(key) != key {
            continue;
        }
        // Anything at the sidecar's path (even a broken one) keeps the bundle.
        let has_sidecar = std::fs::symlink_metadata(recovery_dir.join(format!("{key}.json"))).is_ok();
        let parts = [path.clone(), path.join("tiles"), path.join("blobs")];
        // A committed bundle is recoverable work: keep exactly what `list_recovery` lists.
        if !has_sidecar && sidecarless_entry(recovery_dir, &path).is_none() && parts.iter().all(|p| untouched_for(p, min_age, true)) {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            bundles.extend(parts);
        }
    }
    for dir in bundles {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            if e.file_type().is_ok_and(|ft| ft.is_file()) {
                sweep_temp_file(&e.path(), min_age);
            }
        }
    }
}

/// Remove `path` if it is an atomic-write temp file of another process, untouched for `min_age`.
#[cfg(not(target_arch = "wasm32"))]
fn sweep_temp_file(path: &Path, min_age: std::time::Duration) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return };
    // `.{stem}.{pid}-{n}.pcsave.tmp`
    let Some(rest) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".pcsave.tmp")) else { return };
    let Some((_, owner)) = rest.rsplit_once('.') else { return };
    let Some(pid) = owner.split_once('-').and_then(|(pid, _)| pid.parse::<u32>().ok()) else { return };
    if pid != std::process::id() && untouched_for(path, min_age, false) {
        let _ = std::fs::remove_file(path);
    }
}

/// Whether `path` was last modified at least `min_age` ago. Unreadable times count as recent;
/// a missing path counts as untouched only when `missing_ok`.
#[cfg(not(target_arch = "wasm32"))]
fn untouched_for(path: &Path, min_age: std::time::Duration, missing_ok: bool) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(m) => m.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= min_age),
        Err(e) => missing_ok && e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Record the outcome of the last save attempt where `last_result`/`flush` report it. A
/// poisoned lock keeps the previous result rather than panicking in a crash-recovery path.
fn note(last: &Arc<Mutex<Option<Result<SaveStats>>>>, r: Result<SaveStats>) {
    if let Ok(mut g) = last.lock() {
        *g = Some(r);
    }
}

fn remove_entry(dir: &Path, key: &str) -> Result<()> {
    if sanitize(key) != key {
        return Err(FormatError::corrupt("recovery entry key contains path components"));
    }
    let bundle = dir.join(format!("{key}.pcraft"));
    if bundle.exists() {
        std::fs::remove_dir_all(bundle)?;
    }
    let sidecar = dir.join(format!("{key}.json"));
    if sidecar.exists() {
        std::fs::remove_file(sidecar)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bugfix_develop_discard_returns_before_busy_writer_finishes() {
        let dir = std::env::temp_dir().join(format!("li-discard-{}", std::process::id()));
        let (release, wait) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            wait.recv().unwrap();
        });
        let saver =
            Autosaver { dir, key: "busy".into(), tx: None, handle: Some(handle), last: Arc::default(), completed: Arc::default(), discard: Arc::default() };
        let (returned, completed) = mpsc::channel();
        let caller = std::thread::spawn(move || {
            returned.send(saver.discard()).unwrap();
        });
        let early = completed.recv_timeout(std::time::Duration::from_millis(250));
        release.send(()).unwrap();
        caller.join().unwrap();
        assert!(early.is_ok(), "discard blocked on the busy writer");
        early.unwrap().unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "li-autosave-{name}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        ))
    }

    fn small_doc(name: &str) -> Arc<Document> {
        Arc::new(Document::new(name, photocraft_doc::Size { width: 4, height: 4 }, photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8))
    }

    /// Upstream 3ef63161 (#1124): what the spawn-failure arm of `Autosaver::new` records.
    /// Recovery health checks read this through `last_result`/`flush` instead of seeing a
    /// working autosave.
    #[test]
    fn autosave_spawn_failure_is_reported_not_silent() {
        let last = Arc::new(Mutex::new(None));
        note(&last, Err(FormatError::Io(std::io::Error::other("the autosave thread could not start: resources"))));
        let g = last.lock().unwrap();
        let err = g.as_ref().and_then(|r| r.as_ref().err()).expect("the failure is recorded");
        assert!(err.to_string().contains("could not start"), "{err}");
        // A saver without a worker reports the enqueue failure instead of dropping the snapshot.
        let saver = Autosaver {
            dir: temp("no-worker"),
            key: "k".into(),
            tx: None,
            handle: None,
            last: Arc::new(Mutex::new(None)),
            completed: Arc::default(),
            discard: Arc::default(),
        };
        assert!(saver.request_checked(small_doc("k"), 1, None, SaveOptions::default()).is_err());
    }

    /// Upstream 83d5c543 (#1577): a queued snapshot can fail later; callers must observe the
    /// failure and be able to retry that same revision.
    #[test]
    fn autosave_background_outcomes_report_failure_then_recovery() {
        let root = temp("retry-failed-write");
        std::fs::create_dir_all(&root).unwrap();
        let blocked = root.join("recovery");
        std::fs::write(&blocked, b"not a directory").unwrap();
        let saver = Autosaver::new(&blocked, "doc");
        let doc = small_doc("Retry");
        saver.request_checked(doc.clone(), 7, None, SaveOptions::default()).unwrap();
        let receive = |saver: &Autosaver| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while std::time::Instant::now() < deadline {
                if let Some(outcome) = saver.take_completed().into_iter().next() {
                    return outcome;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("autosave worker did not report its completed write within 30 seconds");
        };
        let (revision, result) = receive(&saver);
        assert_eq!(revision, 7);
        assert!(result.is_err(), "the recovery location is not a directory");
        std::fs::remove_file(&blocked).unwrap();
        std::fs::create_dir(&blocked).unwrap();
        saver.request_checked(doc.clone(), 7, None, SaveOptions::default()).unwrap();
        let (revision, result) = receive(&saver);
        assert_eq!(revision, 7);
        result.unwrap();
        assert_eq!(list_recovery(&blocked).len(), 1);
        assert_eq!(recover(&list_recovery(&blocked)[0]).unwrap(), *doc);
        drop(saver);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Upstream c20b81b2 (#3489): bundles without a sidecar and other processes' stale temp
    /// files are swept; complete entries, this process's temp files and unrelated files stay.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn autosave_sweep_removes_orphaned_bundles_and_foreign_temp_files_only() {
        let dir = temp("sweep");
        let doc = small_doc("Kept");
        let saver = Autosaver::new(&dir, "kept");
        saver.request(doc.clone(), 1, None, SaveOptions::default());
        assert!(saver.flush().is_some_and(|r| r.is_ok()));
        // A first autosave that crashed before its sidecar was written.
        let orphan = dir.join("doc-crashed-1.pcraft");
        std::fs::create_dir_all(orphan.join("tiles")).unwrap();
        std::fs::write(orphan.join("tiles/abc.zst"), b"partial").unwrap();
        // Stale temp files: another process's (swept) and this process's (kept, may be in flight).
        let foreign = [dir.join(".kept.json.4294967294-3.pcsave.tmp"), dir.join("kept.pcraft/.manifest.json.4294967294-0.pcsave.tmp")];
        let own = dir.join(format!(".kept.json.{}-9.pcsave.tmp", std::process::id()));
        for p in foreign.iter().chain([&own]) {
            std::fs::write(p, b"tmp").unwrap();
        }
        // Not ours to judge: unrelated files.
        let other = dir.join("notes.txt");
        std::fs::write(&other, b"user file").unwrap();
        // Fresh data is never swept.
        sweep_orphans(&dir, std::time::Duration::from_secs(3600));
        assert!(orphan.is_dir() && foreign.iter().all(|p| p.is_file()));
        sweep_orphans(&dir, std::time::Duration::ZERO);
        assert!(!orphan.exists(), "a bundle without a sidecar is swept");
        assert!(foreign.iter().all(|p| !p.exists()), "another process's temp files are swept");
        assert!(own.is_file(), "this process's temp files are kept");
        assert!(other.is_file());
        let entries = list_recovery(&dir);
        assert_eq!(entries.len(), 1, "the complete entry survives");
        assert_eq!(recover(&entries[0]).unwrap(), *doc);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Upstream ed7df56a (#4967): a crash after the bundle was committed but before its sidecar
    /// was published leaves recoverable work that must be listed and survive the sweep.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_committed_bundle_without_a_sidecar_is_listed_and_survives_the_sweep() {
        let dir = temp("nosidecar");
        let doc = small_doc("Lost");
        let saver = Autosaver::new(&dir, "first");
        saver.request(doc.clone(), 1, None, SaveOptions::default());
        assert!(saver.flush().is_some_and(|r| r.is_ok()));
        std::fs::remove_file(dir.join("first.json")).unwrap();

        let entries = list_recovery(&dir);
        assert_eq!(entries.len(), 1, "the committed bundle is discoverable");
        assert_eq!((entries[0].info.key.as_str(), entries[0].info.document_name.as_str()), ("first", "Lost"));
        assert_eq!(recover(&entries[0]).unwrap(), *doc);

        sweep_orphans(&dir, std::time::Duration::ZERO);
        assert!(dir.join("first.pcraft").is_dir(), "aged cleanup keeps a committed bundle");

        // Incomplete bundles (no manifest, or one that does not read) are neither listed nor kept.
        let partial = dir.join("partial.pcraft");
        std::fs::create_dir_all(partial.join("tiles")).unwrap();
        let broken = dir.join("broken.pcraft");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("manifest.json"), b"{").unwrap();
        assert_eq!(list_recovery(&dir).len(), 1);
        sweep_orphans(&dir, std::time::Duration::ZERO);
        assert!(!partial.exists() && !broken.exists());
        assert!(dir.join("first.pcraft").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
