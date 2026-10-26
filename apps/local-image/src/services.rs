//! Native platform services: file dialogs (rfd), filesystem, codecs.

#[cfg(any(target_os = "linux", test))]
mod clipboard;
#[cfg(target_os = "linux")]
mod pdf_raster;
#[cfg(target_os = "linux")]
mod portal_save;
mod save_dialog;

use crate::file_dialog_probe::explain_empty;
use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};
use photocraft_doc::Document;
use photocraft_format::RecoveryStore;
use photocraft_ui_egui::dialog_filter::{FileDialogExt as _, async_file_dialog};
use photocraft_ui_egui::{FileDialogAnswer, FileDialogReply, FileDialogRequest, FileDialogSpec, Pick, Recovered, Services};
use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

/// File › Open formats: native/layered documents, editable SVG/SVGZ/PDF, flat/raw
/// images, and Photoshop brushes/gradients (which go to the preset libraries).
const OPEN_EXTS: &[&str] = photocraft_codecs::PHOTO_EXTENSIONS;

fn open_extensions() -> Vec<&'static str> {
    ["pcraft", "psdt", "svg", "svgz", "pdf", "abr", "grd"]
        .into_iter()
        .chain(OPEN_EXTS.iter().copied().filter(|ext| photocraft_codecs::is_readable_extension(ext, true)))
        .collect()
}

/// The File › Open dialog with its filters. On Linux each extension also matches upper and mixed
/// case (`IMG_0001.JPG`, `DSC_0001.NEF`): see [`photocraft_ui_egui::dialog_filter`].
fn open_dialog() -> rfd::AsyncFileDialog {
    async_file_dialog().add_filter_nocase("All Formats", &open_extensions()).add_filter_nocase("Local Image", &["pcraft"])
}

/// File › Save As formats: (filter name, extensions). The filter matching the suggested name's
/// extension comes first, so a .pcraft document saves as .pcraft by default and everything else
/// keeps defaulting to Photoshop.
const SAVE_FILTERS: &[(&str, &[&str])] = &[
    ("Photoshop", &["psd", "psb"]),
    ("Local Image", &["pcraft"]),
    ("PNG", &["png"]),
    ("JPEG", &["jpg"]),
    ("WebP", &["webp"]),
    ("TIFF", &["tif"]),
    ("Targa", &["tga"]),
    ("OpenEXR", &["exr"]),
];

/// The save dialog's file types with the one for `suggested`'s extension first, added if
/// unlisted (.jpeg, .gif, .bmp, .csv, .html …): a dialog that appends the first type's extension
/// then keeps the file's own instead of writing "photo.gif.psd" (upstream PhotoCraft 3952fcef).
fn save_filters(suggested: &str) -> Vec<(String, Vec<String>)> {
    let ext = Path::new(suggested).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let owned = |name: &str, exts: &[&str]| (name.to_string(), exts.iter().map(|e| e.to_string()).collect::<Vec<_>>());
    // Dedicated vector-export dialogs must not append a Photoshop extension.
    // Keep these formats out of ordinary Save As: its writer is the raster/native IO path.
    match ext.as_str() {
        "svg" => return vec![owned("Scalable Vector Graphics", &["svg"])],
        "pdf" => return vec![owned("PDF document", &["pdf"])],
        _ => {}
    }
    let mut v: Vec<(String, Vec<String>)> = SAVE_FILTERS.iter().map(|(name, exts)| owned(name, exts)).collect();
    match v.iter().position(|(_, exts)| exts.contains(&ext)) {
        Some(i) => {
            let f = v.remove(i);
            v.insert(0, f);
        }
        None if !ext.is_empty() => v.insert(0, (ext.to_ascii_uppercase(), vec![ext])),
        None => {}
    }
    v
}

/// What a file dialog will answer, once the user is done with it.
type Answer = Pin<Box<dyn Future<Output = Option<FileDialogAnswer>> + Send>>;

/// Shows an Open, Save or folder dialog without blocking the event loop (upstream PhotoCraft
/// b97a72ac): rfd's async dialog opens here, on the main thread, as a sheet on macOS and a dialog
/// owned by the window elsewhere, and a helper thread waits for it and hands the answer to the
/// app. An empty answer with no dialog backend on the system is explained, not taken for Cancel
/// (see `file_dialog_probe`). The editor's and the Library's dialogs all open here.
pub(crate) fn show_file_dialog(request: FileDialogRequest, reply: FileDialogReply) {
    let answer: Answer = match request {
        FileDialogRequest::Open { multiple: true } => {
            let picked = open_dialog().pick_files();
            Box::pin(async move { explain_empty(picked.await).map(|files| FileDialogAnswer::Paths(files.iter().map(path_of).collect())) })
        }
        FileDialogRequest::Open { multiple: false } => {
            let picked = open_dialog().pick_file();
            Box::pin(async move { explain_empty(picked.await).map(|file| FileDialogAnswer::Paths(vec![path_of(&file)])) })
        }
        FileDialogRequest::Read => {
            let picked = open_dialog().pick_file();
            Box::pin(async move {
                let file = explain_empty(picked.await)?;
                // A read failure goes back to the app, which reports it like any other open failure.
                let bytes = photocraft_format::read_file(file.path()).map_err(|e| e.to_string());
                Some(FileDialogAnswer::File(path_of(&file), bytes))
            })
        }
        FileDialogRequest::Folder => {
            let picked = async_file_dialog().pick_folder();
            Box::pin(async move { explain_empty(picked.await).map(|dir| FileDialogAnswer::Paths(vec![path_of(&dir)])) })
        }
        FileDialogRequest::Save { suggested } => save_as_dialog(suggested),
        FileDialogRequest::Described(spec) => described_dialog(spec),
    };
    // Without the thread the reply is dropped unanswered, which the app takes as Cancel.
    if let Err(e) = std::thread::Builder::new().name("file dialog".into()).spawn(move || reply.send(block_on(answer))) {
        log::error!("couldn't wait for the file dialog: {e}");
    }
}

/// rfd's dialog for `spec` (the Library's, the AI and Swatches pickers), which opens when this is
/// called.
fn described_dialog(spec: FileDialogSpec) -> Answer {
    let mut d = async_file_dialog();
    if let Some(title) = spec.title {
        d = d.set_title(title);
    }
    for (name, exts) in spec.filters {
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        d = d.add_filter_nocase(name, &exts);
    }
    if let Some(name) = spec.file_name {
        d = d.set_file_name(name);
    }
    match spec.pick {
        Pick::File => {
            let picked = d.pick_file();
            Box::pin(async move { explain_empty(picked.await).map(|file| FileDialogAnswer::Paths(vec![path_of(&file)])) })
        }
        Pick::Files => {
            let picked = d.pick_files();
            Box::pin(async move { explain_empty(picked.await).map(|files| FileDialogAnswer::Paths(files.iter().map(path_of).collect())) })
        }
        Pick::Folder => {
            let picked = d.pick_folder();
            Box::pin(async move { explain_empty(picked.await).map(|dir| FileDialogAnswer::Paths(vec![path_of(&dir)])) })
        }
        Pick::Save => {
            let picked = d.save_file();
            Box::pin(async move { explain_empty(picked.await).map(|file| FileDialogAnswer::SaveTo(path_of(&file))) })
        }
    }
}

/// Save As for `suggested`. On Linux the portal's own dialog, which reports the file type picked,
/// and rfd's if the portal can't show it (see `save_dialog`); both wait on the helper thread.
fn save_as_dialog(suggested: String) -> Answer {
    #[cfg(target_os = "linux")]
    {
        Box::pin(async move {
            let req = save_dialog::SaveRequest {
                name: save_dialog::dialog_file_name(&suggested),
                suggested: Path::new(&suggested).file_name().map(|n| n.to_string_lossy().into_owned()),
                folder: save_folder(&suggested),
                filters: save_filters(&suggested),
                current: Some(0),
            };
            let portal = zbus::blocking::Connection::session()
                .map_err(|e| e.to_string())
                .and_then(|conn| save_dialog::save_as(|r| portal_save::save_file(&conn, r), req));
            match portal {
                Ok(path) => return path.map(|p| FileDialogAnswer::SaveTo(p.to_string_lossy().into_owned())),
                Err(e) => log::info!("portal save dialog unavailable ({e}), using rfd"),
            }
            rfd_save_dialog(suggested).await
        })
    }
    #[cfg(not(target_os = "linux"))]
    Box::pin(rfd_save_dialog(suggested))
}

/// The folder a save dialog for `suggested` opens in: a suggestion with a folder (a photo from the
/// Library: beside its original) opens there.
fn save_folder(suggested: &str) -> Option<PathBuf> {
    Path::new(suggested).parent().filter(|d| d.is_absolute() && d.is_dir()).map(Path::to_path_buf)
}

/// rfd's Save dialog for `suggested`, which opens when this is called (on macOS it must be called
/// on the main thread).
fn rfd_save_dialog(suggested: String) -> impl Future<Output = Option<FileDialogAnswer>> + Send {
    let mut d = async_file_dialog();
    for (name, exts) in save_filters(&suggested) {
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        d = d.add_filter_nocase(name, &exts);
    }
    if let Some(dir) = save_folder(&suggested) {
        d = d.set_directory(dir);
    }
    if let Some(name) = Path::new(&suggested).file_name() {
        d = d.set_file_name(name.to_string_lossy());
    }
    let picked = d.save_file();
    async move {
        let chosen = path_of(&explain_empty(picked.await)?);
        Some(FileDialogAnswer::SaveTo(save_dialog::with_suggested_extension(chosen, &suggested)))
    }
}

fn path_of(file: &rfd::FileHandle) -> String {
    file.path().to_string_lossy().into_owned()
}

/// Runs `future` to completion on this thread, sleeping until it is woken.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        // Spurious wake-ups just poll again.
        std::thread::park();
    }
}

/// Per-user settings directory: `LOCAL_IMAGE_CONFIG_DIR`, else `<exe dir>/LocalImageData` in
/// portable mode, else the platform convention. Everything the app persists lives under it; see
/// [`lightcraft_sysmem::app_dirs`].
pub fn config_dir() -> Option<PathBuf> {
    lightcraft_sysmem::app_dirs::config_dir()
}

pub fn prefs_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("preferences.json"))
}

/// The brush preset store (one file per preset group plus tip bitmaps; see
/// `photocraft_engine::preset_store`).
pub fn presets_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("Presets"))
}

fn recovery_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("Recovery"))
}

/// Write `bytes` crash-safely (temp file beside the target, fsync, rename, directory fsync; see
/// [`li_fs`]). Every document write (Save, Save As, Export, Save for Web) and
/// the preferences go through here, so a failed or interrupted save never destroys the old file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    li_fs::refuse_read_only(path).map_err(|e| e.to_string())?;
    li_fs::atomic_write(path, bytes).map_err(|e| e.to_string())
}

/// Persisted separately so the recovery location remains discoverable after restart.
#[derive(serde::Serialize, serde::Deserialize)]
struct PrefsRecoveryRecord {
    backup_name: String,
    reason: String,
}

fn prefs_recovery_file(path: &Path) -> PathBuf {
    path.with_extension("recovery.json")
}
fn prefs_backup_dir(path: &Path) -> PathBuf {
    path.parent().unwrap_or_else(|| Path::new(".")).join("preference-backups")
}

fn prefs_recovery(path: &Path) -> Option<PrefsRecoveryRecord> {
    let record: PrefsRecoveryRecord = serde_json::from_slice(&std::fs::read(prefs_recovery_file(path)).ok()?).ok()?;
    let mut parts = Path::new(&record.backup_name).components();
    if !matches!(parts.next(), Some(std::path::Component::Normal(_))) || parts.next().is_some() {
        return None;
    }
    Some(record)
}

fn read_editor_prefs(path: &Path) -> Result<Option<String>, photocraft_ui_egui::PrefsLoadError> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            photocraft_ui_egui::PrefsLoadError::Damaged(format!(
                "{} is not valid UTF-8; its original bytes must be backed up before replacement",
                path.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(photocraft_ui_egui::PrefsLoadError::Unreadable(format!(
            "Cannot read {}: {error}. The file will not be replaced while it is unreadable",
            path.display()
        ))),
    }
}

/// Match the editor's lenient field repair without mutating a Session/global colour state.
fn damaged_editor_prefs(bytes: &[u8]) -> Option<String> {
    let mut value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(error) => return Some(format!("Preferences contain unreadable JSON: {error}")),
    };
    let color = value.as_object_mut().and_then(|object| object.remove("colorSettings"));
    let (_, mut rejected) = match photocraft_engine::prefs::lenient::<photocraft_engine::prefs::Preferences>(value) {
        Ok(parsed) => parsed,
        Err(error) => return Some(format!("Preferences could not be read: {error}")),
    };
    if let Some(color) = color {
        match photocraft_engine::prefs::lenient::<photocraft_engine::color_cmds::ColorSettings>(color) {
            Ok((_, fields)) => rejected.extend(fields.into_iter().map(|field| format!("colorSettings.{field}"))),
            Err(error) => return Some(format!("Colour preferences could not be read: {error}")),
        }
    }
    (!rejected.is_empty()).then(|| format!("Invalid preference fields used defaults: {}", rejected.join(", ")))
}

fn write_new_prefs_backup(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    write_new_prefs_backup_with(path, |file| file.write_all(bytes))
}

fn write_new_prefs_backup_with(path: &Path, write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = write(&mut file) {
        drop(file);
        // Only this call's newly created, incomplete backup is eligible for cleanup.
        if let Err(cleanup) = std::fs::remove_file(path) {
            return Err(std::io::Error::new(
                error.kind(),
                format!("Backup write failed: {error}; could not remove partial backup {}: {cleanup}", path.display()),
            ));
        }
        return Err(error);
    }
    // Complete bytes remain available for exact-byte verification/resync if sync fails.
    file.sync_all()
}

/// Recover a synced orphan too: a recovery-record failure must not create another
/// full backup on each retry. Only regular app-named files are considered, never symlinks.
fn matching_prefs_backup(path: &Path, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    let directory = prefs_backup_dir(path);
    let matches = |candidate: &Path| -> Result<bool, String> {
        let metadata = match std::fs::symlink_metadata(candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        if !metadata.file_type().is_file() || metadata.len() != bytes.len() as u64 {
            return Ok(false);
        }
        Ok(std::fs::read(candidate).map_err(|error| error.to_string())? == bytes)
    };
    if let Some(record) = prefs_recovery(path) {
        let backup = directory.join(record.backup_name);
        if matches(&backup)? {
            return Ok(Some(backup));
        }
    }
    for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("preferences.corrupt-") && name.ends_with(".json") && matches(&entry.path())? {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

/// The original is never replaced until both its exact-byte backup and recovery record
/// have reached stable storage. Ordinary read failures block saving, rather than looking absent.
static PREFERENCES_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(test)]
fn save_editor_prefs(path: &Path, text: &str) -> Result<(), String> {
    let _write = PREFERENCES_WRITE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    save_editor_prefs_inner(path, text)
}
fn save_editor_prefs_inner(path: &Path, text: &str) -> Result<(), String> {
    use li_fs::{AtomicFs, RealFs};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let original = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Cannot read {} before saving: {error}. The original was not changed", path.display())),
    };
    if let Some(bytes) = &original
        && let Some(reason) = damaged_editor_prefs(bytes)
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let directory = prefs_backup_dir(path);
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Cannot create preference backup directory {}: {error}. The original was not changed", directory.display()))?;
        RealFs.sync_dir(parent).map_err(|error| format!("Cannot make the preference backup directory durable: {error}. The original was not changed"))?;
        let previous =
            matching_prefs_backup(path, bytes).map_err(|error| format!("Cannot verify existing preference backups: {error}. The original was not changed"))?;
        let backup = if let Some(backup) = previous {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&backup)
                .and_then(|file| file.sync_all())
                .map_err(|error| format!("Cannot sync preference backup {}: {error}. The original was not changed", backup.display()))?;
            backup
        } else {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            let mut created = None;
            for _ in 0..32 {
                let backup = directory.join(format!("preferences.corrupt-{stamp}-{}-{}.json", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
                match write_new_prefs_backup(&backup, bytes) {
                    Ok(()) => {
                        created = Some(backup);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(format!("Cannot preserve original preferences at {}: {error}. The original was not changed", backup.display())),
                }
            }
            created.ok_or_else(|| "Cannot allocate a unique preference backup. The original was not changed".to_string())?
        };
        RealFs
            .sync_dir(&directory)
            .map_err(|error| format!("Cannot make preference backup {} durable: {error}. The original was not changed", backup.display()))?;
        let record = PrefsRecoveryRecord { backup_name: backup.file_name().ok_or("invalid preference backup path")?.to_string_lossy().into_owned(), reason };
        let record = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
        write_atomic(&prefs_recovery_file(path), &record).map_err(|error| {
            format!(
                "Original preferences are backed up at {}, but the recovery record could not be saved: {error}. The original was not changed",
                backup.display()
            )
        })?;
        RealFs.sync_dir(parent).map_err(|error| format!("Cannot make the preferences recovery record durable: {error}. The original was not changed"))?;
    }
    // A second window may have changed the source while the backup was synced. Retry
    // against its new content instead of replacing something that was not inspected.
    let current = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Cannot recheck preferences before saving: {error}. The original was not changed")),
    };
    if current != original {
        return Err("Preferences changed while preparing the save; retrying will merge the new version. The original was not changed".into());
    }
    li_fs::atomic_write_private(path, text.as_bytes()).map_err(|e| e.to_string())?;
    if let Ok(root) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(ai) = root.get("ai")
    {
        li_ai::settings::publish(&li_ai::AiSettings::from_value(ai.clone()));
    }
    Ok(())
}

/// Marker the host reads from the `library` section: the stored Library settings could not be
/// read, so it shows the Library-problem screen and never opens or creates a different Library.
pub(crate) const LIBRARY_UNREADABLE_KEY: &str = "unreadable";

/// Read `preferences.json` for the editor. A `library` section that is not an object becomes
/// the [`LIBRARY_UNREADABLE_KEY`] marker; the next save first keeps the exact original bytes in
/// `preference-backups/` (the damaged-file recovery of [`save_editor_prefs_inner`]).
fn read_preferences(path: &Path) -> Result<Option<String>, photocraft_ui_egui::PrefsLoadError> {
    let original = read_editor_prefs(path)?;
    let Some(Ok(serde_json::Value::Object(mut root))) = original.as_deref().map(serde_json::from_str::<serde_json::Value>) else {
        return Ok(original); // Missing, or a damaged root that the lenient loader and recovery own.
    };
    if let Some(ai) = root.get("ai") {
        li_ai::settings::publish(&li_ai::AiSettings::from_value(ai.clone()));
    }
    if !root.get("library").is_some_and(|value| !value.is_object()) {
        return Ok(original);
    }
    root.insert(
        "library".into(),
        serde_json::json!({ LIBRARY_UNREADABLE_KEY: "The stored Library settings could not be read. Choose your existing Library." }),
    );
    serde_json::to_string_pretty(&root).map(Some).map_err(|error| photocraft_ui_egui::PrefsLoadError::Damaged(error.to_string()))
}

fn install_ai_preferences_store(path: PathBuf) {
    li_ai::settings::install_store_backend(ai_preferences_backend(path));
}
/// The AI settings live in the `ai` section of `preferences.json`; nothing else is read.
fn ai_preferences_backend(path: PathBuf) -> li_ai::settings::StoreBackend {
    let load = path.clone();
    let save = path;
    let root = |path: &Path| -> Result<Option<serde_json::Value>, String> {
        read_editor_prefs(path)
            .map_err(|error| error.to_string())?
            .map(|text| serde_json::from_str::<serde_json::Value>(&text).map_err(|error| error.to_string()))
            .transpose()
    };
    li_ai::settings::StoreBackend {
        load: Arc::new(move || root(&load).map(|root| root.and_then(|root| root.get("ai").cloned()))),
        save: Arc::new(move |ai| {
            let _write = PREFERENCES_WRITE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut root = root(&save)?.unwrap_or_else(|| serde_json::json!({}));
            root.as_object_mut().ok_or("Preferences must be an object; the original was not changed")?.insert("ai".into(), ai);
            save_editor_prefs_inner(&save, &serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?)
        }),
    }
}

/// Path-injected preference services keep native recovery tests isolated from user config.
fn preferences_services(path: Option<PathBuf>) -> Services {
    let load = path.clone();
    let save = path.clone();
    let snapshot = Arc::new(std::sync::Mutex::new(None::<Option<Vec<u8>>>));
    let before = snapshot.clone();
    Services {
        load_prefs_checked: Some(Box::new(move || {
            let _write = PREFERENCES_WRITE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let result = match &load {
                Some(path) => read_preferences(path),
                None => Ok(None),
            };
            if result.is_ok() {
                *snapshot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(load.as_ref().and_then(|path| std::fs::read(path).ok()));
            }
            result
        })),
        save_prefs: Some(Box::new(move |text| {
            let path = save.as_deref().ok_or("no config directory")?;
            let _write = PREFERENCES_WRITE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(expected) = &*before.lock().unwrap_or_else(std::sync::PoisonError::into_inner) {
                let current = match std::fs::read(path) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.to_string()),
                };
                if *expected != current {
                    return Err("Preferences changed after the merge read; retrying will preserve the new version".into());
                }
            }
            save_editor_prefs_inner(path, text)?;
            *before.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Some(text.as_bytes().to_vec()));
            Ok(())
        })),
        prefs_recovery: Some(Box::new(move || {
            let path = path.as_ref()?;
            let record = prefs_recovery(path)?;
            Some(photocraft_ui_egui::PrefsRecoveryNotice {
                backup_path: prefs_backup_dir(path).join(record.backup_name).to_string_lossy().into_owned(),
                reason: record.reason,
            })
        })),
        ..Default::default()
    }
}

/// Prepare encoding without touching the original or Trash. The injected Trash
/// destination keeps native host tests isolated from the user's installation.
pub(crate) fn overwrite_original(
    doc: &Document,
    path: &Path,
    quality: u8,
    trash: impl FnOnce(&Path) -> std::io::Result<PathBuf> + 'static,
) -> Result<photocraft_ui_egui::PreparedOverwrite, String> {
    let original = Arc::new(photocraft_format::read_file(path).map_err(|e| e.to_string())?);
    let result = photocraft_io::export_over_original(doc, &original, quality).map_err(|e| e.to_string())?;
    let path = path.to_owned();
    let expected = original.clone();
    Ok(photocraft_ui_egui::PreparedOverwrite {
        original,
        commit: Box::new(move || {
            // A file changed by another application between prepare and commit
            // must not be replaced using stale metadata or preserved source bytes.
            let mut file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
            if file.metadata().map_err(|error| error.to_string())?.len() != expected.len() as u64 {
                return Err("The original file changed; try overwriting again".into());
            }
            use std::io::Read;
            let mut buffer = vec![0u8; 64 * 1024];
            for chunk in expected.chunks(buffer.len()) {
                file.read_exact(&mut buffer[..chunk.len()]).map_err(|error| error.to_string())?;
                if &buffer[..chunk.len()] != chunk {
                    return Err("The original file changed; try overwriting again".into());
                }
            }
            if file.read(&mut buffer[..1]).map_err(|error| error.to_string())? != 0 {
                return Err("The original file changed; try overwriting again".into());
            }
            // Keep the old file present until the replacing rename. No editor
            // metadata is published unless both backup and replacement succeed.
            trash(&path).map_err(|e| format!("Couldn't keep the previous version in Trash: {e}"))?;
            write_atomic(&path, &result.bytes)?;
            Ok(result.warnings)
        }),
    })
}

type SharedRecovery = Rc<RefCell<Option<RecoveryStore>>>;

/// Run `f` on the recovery store (`Err` without a config directory). The service closures never
/// call each other, so the store is never borrowed twice.
fn with_store<R>(store: &SharedRecovery, f: impl FnOnce(&mut RecoveryStore) -> R) -> Result<R, String> {
    let mut slot = store.try_borrow_mut().map_err(|_| "crash recovery is busy".to_string())?;
    Ok(f(slot.as_mut().ok_or("no config directory")?))
}

/// Crash recovery: background incremental .pcraft autosaves into `dir` (`None`: no config
/// directory, so autosaves fail and nothing is recovered). Recovered documents keep their entries
/// until a newer autosave replaces them or they're saved or closed (see [`RecoveryStore`]).
fn recovery_services(dir: Option<PathBuf>) -> Services {
    let store: SharedRecovery = Rc::new(RefCell::new(dir.map(RecoveryStore::new)));
    let (s1, s2, s3, s4, s5) = (store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
    let cleanup: Rc<RefCell<Vec<photocraft_format::autosave::Discard>>> = Rc::default();
    let finish = cleanup.clone();
    Services {
        autosave: Some(Box::new(move |doc: &Arc<Document>, revision: u64, path: Option<&str>| {
            with_store(&s1, |s| s.autosave_checked(doc, revision, path.map(str::to_string)))?
        })),
        // Background write outcomes reach the UI, so a failing autosave is reported and retried.
        autosave_results: Some(Box::new(move || with_store(&s4, |s| s.take_completed()).unwrap_or_default())),
        flush_autosaves: Some(Box::new(move || with_store(&s5, RecoveryStore::flush).unwrap_or_default())),
        discard_autosave: Some(Box::new(move |id: u64| match with_store(&s2, |s| s.discard(id)) {
            Ok(Ok(receipt)) => {
                let mut pending = cleanup.borrow_mut();
                pending.retain(|r| !r.is_finished());
                pending.push(receipt);
            }
            Ok(Err(e)) => log::error!("Couldn't remove recovery data: {e}"),
            Err(e) => log::error!("Couldn't access recovery data: {e}"),
        })),
        finish_autosave_cleanup: Some(Box::new(move || {
            for receipt in finish.borrow_mut().drain(..) {
                receipt.wait();
            }
        })),
        recover: Some(Box::new(move || {
            let found = with_store(&s3, |s| s.recover()).unwrap_or_default();
            found.into_iter().map(|(e, doc)| Recovered { key: e.info.key, path: e.info.original_path, doc }).collect()
        })),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| {
            let _ = with_store(&store, |s| s.adopt(id, key));
        })),
        ..Default::default()
    }
}

/// The pixels to paste when the clipboard holds copied files rather than an image (Copy in
/// Files, Finder or Explorer puts paths on the clipboard, #338): the first file that decodes,
/// upright, as RGBA8. Files that aren't images are skipped after reading only their header.
fn guarded_clipboard<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    lightcraft_engine::guard::catch("Read clipboard image", f).ok().flatten()
}

#[cfg(test)]
fn image_from_files(paths: &[PathBuf]) -> Option<(u32, u32, Vec<u8>)> {
    use std::io::Read;
    paths.iter().find_map(|path| {
        guarded_clipboard(|| {
            // text/uri-list lines end in CRLF (RFC 2483, and GTK writes them so), but arboard splits
            // on LF only, so a path copied in GNOME Files arrives with a trailing '\r'.
            let path = path.to_str().map_or_else(|| path.clone(), |s| PathBuf::from(s.trim_end_matches('\r')));
            let mut head = Vec::with_capacity(256);
            photocraft_format::read::validate_path(&path).ok()?;
            std::fs::File::open(&path).ok()?.take(256).read_to_end(&mut head).ok()?;
            // TGA has no magic number and is only guessed from the header, so trust the extension
            // too before reading what may be a large non-image file.
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match photocraft_codecs::detect(&head)? {
                photocraft_codecs::Format::Tga if photocraft_codecs::from_extension(ext) != Some(photocraft_codecs::Format::Tga) => return None,
                _ => {}
            }
            let img = photocraft_codecs::decode(&photocraft_format::read_file(&path).ok()?).ok()?;
            Some((img.width(), img.height(), img.to_rgba8()))
        })
    })
}

pub fn native(automation: Option<photocraft_automation::AuthorizedWorkspace>) -> Services {
    let clip: Rc<RefCell<Option<arboard::Clipboard>>> = Rc::default();
    let preferences = preferences_services(prefs_file());
    if !cfg!(test)
        && let Some(path) = prefs_file()
    {
        install_ai_preferences_store(path);
    }
    let automation_read = automation.clone().map(|workspace| {
        Box::new(move |path: &str| {
            let bytes = workspace.read(path).map_err(|error| error.to_string())?;
            let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path).to_string();
            Ok((name, bytes))
        }) as photocraft_ui_egui::AutomationReadFn
    });
    let automation_write = automation.clone().map(|workspace| {
        Box::new(move |path: &str, bytes: &[u8]| workspace.write(path, bytes).map_err(|error| error.to_string())) as photocraft_ui_egui::AutomationWriteFn
    });
    let automation_browse = automation
        .clone()
        .map(|workspace| Box::new(move |path: &str| crate::automation_library::stage(&workspace, path)) as photocraft_ui_egui::AutomationBrowseFn);
    let step: fn(&str, &serde_json::Value) -> photocraft_engine::Result<()> = photocraft_automation::workspace::authorize_desktop_engine_step;
    let automation_authorize = automation.is_some().then_some(step);
    let automation_command = automation.map(|_| {
        Box::new(|id: &str, params: &serde_json::Value| {
            photocraft_automation::workspace::authorize_desktop_engine_command(id, params).map_err(|error| error.to_string())
        }) as photocraft_ui_egui::AutomationCommandFn
    });
    Services {
        import: Some(Box::new(|name: &str, bytes: &[u8]| {
            crate::crash_guard::guard("Open", || photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))
        })),
        #[cfg(target_os = "linux")]
        pdf_raster: Some(Arc::new(pdf_raster::run)),
        export: Some(std::sync::Arc::new(|doc: &Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
            let mut opts = photocraft_io::ExportOptions::default();
            if let Some(q) = settings.jpeg_quality {
                opts.encode.jpeg_quality = q;
            }
            opts.encode.webp_lossless = settings.webp_lossless;
            if let Some(q) = settings.webp_quality {
                opts.encode.webp_quality = q;
            }
            opts.tiff_layers = settings.tiff_layers;
            opts.read_linked_files = settings.read_linked_files;
            opts.xmp = if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None };
            crate::crash_guard::guard("Export", || {
                photocraft_engine::vector_integration::export_document(doc, path, &opts).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
            })
        })),
        prepare_save: Some(Arc::new(prepare_document_save)),
        overwrite_original: li_ai::trash::available().then(|| {
            Box::new(|doc: &Document, path: &str, quality: u8| overwrite_original(doc, Path::new(path), quality, li_ai::trash::copy_to_trash))
                as photocraft_ui_egui::OverwriteFn
        }),
        file_dialog: Some(Box::new(show_file_dialog)),
        write: Some(Box::new(|path: &str, bytes: &[u8]| write_atomic(Path::new(path), bytes))),
        file_stamp: Some(Box::new(|path: &str| {
            // Follows a symlink, as Save writes through it to the file it points at.
            let meta = std::fs::metadata(path).ok()?;
            Some(photocraft_ui_egui::FileStamp { len: meta.len(), modified: meta.modified().ok() })
        })),
        stage_write: Some(Arc::new(|path, bytes, interrupt| {
            li_fs::refuse_read_only(Path::new(path)).map_err(|error| error.to_string())?;
            let staged = li_fs::stage_write(Path::new(path), bytes, interrupt).map_err(|error| error.to_string())?;
            Ok(photocraft_ui_egui::PreparedWrite { commit: Box::new(move || staged.commit().map_err(|error| error.to_string())) })
        })),
        automation_read,
        automation_browse,
        automation_write,
        automation_command,
        automation_authorize,
        encode_png: Some(Box::new(|w, h, rgba| {
            let img = Image::from_u8(w, h, ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
            photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).map_err(|e| e.to_string())
        })),
        inbox: None,
        open_url: Some(Box::new(|url: &str| open::that(url).map_err(|e| e.to_string()))),
        clipboard_set_image: Some({
            let clip = clip.clone();
            Box::new(move |w: u32, h: u32, px: &[u8]| {
                let mut slot = clip.try_borrow_mut().map_err(|_| "clipboard is busy".to_string())?;
                let cb = match slot.as_mut() {
                    Some(c) => c,
                    None => slot.insert(arboard::Clipboard::new().map_err(|e| e.to_string())?),
                };
                cb.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(px) }).map_err(|e| e.to_string())
            })
        }),
        clipboard_get_files: Some({
            let clip = clip.clone();
            Box::new(move || {
                let Ok(mut slot) = clip.try_borrow_mut() else { return vec![] };
                let cb = match slot.as_mut() {
                    Some(c) => c,
                    None => match arboard::Clipboard::new() {
                        Ok(c) => slot.insert(c),
                        Err(_) => return vec![],
                    },
                };
                cb.get().file_list().unwrap_or_default().iter().map(|p| p.to_string_lossy().trim_end_matches('\r').to_string()).collect()
            })
        }),
        clipboard_get_image: Some({
            let clip = clip.clone();
            Box::new(move || {
                guarded_clipboard(|| {
                    let mut slot = clip.try_borrow_mut().ok()?;
                    let cb = match slot.as_mut() {
                        Some(c) => c,
                        None => slot.insert(arboard::Clipboard::new().ok()?),
                    };
                    if let Ok(image) = cb.get_image() {
                        return Some((image.width as u32, image.height as u32, image.bytes.into_owned()));
                    }
                    #[cfg(target_os = "linux")]
                    return clipboard::wayland_clipboard_image();
                    #[cfg(not(target_os = "linux"))]
                    None
                })
            })
        }),
        load_prefs: None,
        load_prefs_checked: preferences.load_prefs_checked,
        save_prefs: preferences.save_prefs,
        prefs_recovery: preferences.prefs_recovery,
        append_text: Some(Box::new(|path: &str, text: &str| {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
            f.write_all(text.as_bytes()).map_err(|e| e.to_string())
        })),
        // Set by main once the Apple-event handlers are connected (macOS).
        os_events: None,
        // Set by main, which starts loading the store before the window opens.
        preset_store: None,
        is_wayland: false,
        ..recovery_services(recovery_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_engine::Session;
    use photocraft_format::list_recovery;
    use photocraft_ui_egui::{PhotocraftApp, prefs_ui};
    use serde_json::json;

    #[test]
    fn shared_decode_open_dialog_registry_includes_jxl_raw_and_native_documents() {
        let extensions = open_extensions();
        for ext in ["jxl", "heic", "heif", "hif", "arq", "crw", "pcraft", "svg", "pdf", "abr", "grd"] {
            assert!(extensions.contains(&ext), "{ext}");
        }
        assert!(!extensions.contains(&"avif"));
    }

    /// Upstream 9b4562f2 / LightCraft e2d35de: File › Open shows IMG_0001.JPG, DSC_0001.NEF and
    /// .CR3 on Linux, where the portal matches rfd's `*.ext` globs case-sensitively.
    #[test]
    fn open_filters_cover_uppercase_and_mixed_case_extensions() {
        use photocraft_ui_egui::dialog_filter::{dialog_extensions_for, glob_ext};
        let globs = dialog_extensions_for(OPEN_EXTS, true);
        for ext in OPEN_EXTS {
            let alternating = |upper_first: bool| -> String {
                ext.chars().enumerate().map(|(i, c)| if (i % 2 == 0) == upper_first { c.to_ascii_uppercase() } else { c }).collect()
            };
            for name in [ext.to_string(), ext.to_ascii_uppercase(), alternating(true), alternating(false)] {
                assert!(globs.iter().any(|g| glob_ext(g, &name)), "File › Open hides .{name}");
            }
            assert!(!globs.iter().any(|g| glob_ext(g, &format!("{ext}x"))), "{ext}");
        }
        for name in ["JPG", "NEF", "CR3", "Jpeg"] {
            assert!(globs.iter().any(|g| glob_ext(g, name)), "{name}");
        }
        assert!(!globs.iter().any(|g| glob_ext(g, "txt")));
        // Windows and macOS keep plain extensions.
        assert_eq!(dialog_extensions_for(OPEN_EXTS, false), OPEN_EXTS);
    }

    /// Every `.rs` file under `dir`, skipping build output.
    fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "target") {
                    rust_sources(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// No file dialog blocks the event loop: rfd's blocking `FileDialog` froze the window on Linux
    /// and aborted the app on macOS. Every dialog is an `AsyncFileDialog` parented to the window
    /// (only the parent template is made without one), and every filter matches any letter case.
    #[test]
    fn every_file_dialog_is_asynchronous_parented_and_case_insensitive() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for dir in ["crates", "apps"] {
            rust_sources(&root.join(dir), &mut files);
        }
        assert!(files.len() > 100, "found {} sources", files.len());
        let blocking = concat!("rfd::", "FileDialog");
        let unparented = concat!("rfd::AsyncFileDialog::", "new()");
        for file in files {
            let src = std::fs::read_to_string(&file).unwrap();
            let name = file.strip_prefix(&root).unwrap_or(&file).display().to_string();
            assert!(!src.contains(blocking), "{name} uses rfd's blocking FileDialog; ask through the app's file_dialog service");
            let (any, asynchronous) = (concat!("FileDialog::", "new()"), concat!("AsyncFileDialog::", "new()"));
            assert_eq!(src.matches(any).count(), src.matches(asynchronous).count(), "{name} uses a blocking FileDialog");
            if src.contains(unparented) {
                assert!(name.ends_with("dialog_filter.rs") || name.ends_with("main.rs"), "{name} opens an unparented dialog; use async_file_dialog()");
            }
            if !name.ends_with("dialog_filter.rs") {
                assert!(!src.contains(concat!(".add_filter", "(")), "{name} calls rfd's add_filter; use add_filter_nocase");
            }
        }
        assert!(include_str!("services.rs").contains(".add_filter_nocase("));
    }

    #[test]
    fn block_on_waits_for_a_wake_from_another_thread() {
        let (tx, rx) = std::sync::mpsc::channel::<std::task::Waker>();
        let mut woken = false;
        let ready = std::future::poll_fn(move |cx| {
            if woken {
                return std::task::Poll::Ready(7);
            }
            woken = true;
            let _ = tx.send(cx.waker().clone());
            std::task::Poll::Pending
        });
        let waker = std::thread::spawn(move || rx.recv().unwrap().wake());
        assert_eq!(block_on(ready), 7);
        waker.join().unwrap();
    }

    #[test]
    fn hotfix_native_roundtrip_export_supports_placed_vector_crop() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
        let mut doc = (*s.active().unwrap().doc).clone();
        let mut group = photocraft_doc::Layer::group("Placed", doc.layers.iter().map(photocraft_doc::Layer::duplicate).collect());
        group.vector.clip = Some(photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[(0., 0.), (20., 0.), (20., 20.), (0., 20.)])]));
        doc.layers.push(group);
        let services = native(None);
        let export = services.export.unwrap();
        for path in ["photo-Edit.psd", "photo-Edit.tif"] {
            let (bytes, warnings) = export(&doc, path, &photocraft_ui_egui::ExportSettings { tiff_layers: true, ..Default::default() }).unwrap();
            assert!(warnings.iter().any(|w| w.contains("vector crop controls") && w.contains("editability lost")));
            assert!(photocraft_io::import(path, &bytes).unwrap().document.layers.len() >= 2);
        }
    }

    #[test]
    fn recovery_shutdown_finishes_pending_discard_and_preserves_other_documents() {
        let dir = std::env::temp_dir().join(format!("li-quit-discard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut services = recovery_services(Some(dir.clone()));
        let mut session = Session::new();
        session.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
        let discarded = session.active().unwrap().doc.clone();
        session.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
        let retained = session.active().unwrap().doc.clone();
        // Queue writes and immediately request deletion: a late writer must not resurrect it.
        services.autosave.as_mut().unwrap()(&discarded, 1, None).unwrap();
        services.autosave.as_mut().unwrap()(&retained, 1, None).unwrap();
        services.discard_autosave.as_mut().unwrap()(discarded.id.0);
        services.finish_autosave_cleanup.as_mut().unwrap()();
        drop(services); // flush the retained writer, as a crash/restart would
        let entries = list_recovery(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(photocraft_format::recover(&entries[0]).unwrap().id, retained.id);
        let mut relaunched = recovery_services(Some(dir.clone()));
        let recovered = relaunched.recover.as_mut().unwrap()();
        assert_eq!(recovered.len(), 1);
        relaunched.adopt_autosave.as_mut().unwrap()(recovered[0].doc.id.0, &recovered[0].key);
        relaunched.discard_autosave.as_mut().unwrap()(recovered[0].doc.id.0);
        relaunched.finish_autosave_cleanup.as_mut().unwrap()();
        assert!(list_recovery(&dir).is_empty(), "adopted entries must also disappear on confirmed quit");
        drop(relaunched);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// "Export As" formats must lead with their own filter, or the save panel appends the first one's extension (`photo.webp.psd`).
    #[test]
    fn save_filters_lead_with_every_export_format() {
        for ext in ["png", "jpg", "webp", "tif", "tga", "svg", "pdf"] {
            assert!(save_filters(&format!("photo.{ext}"))[0].1.iter().any(|e| e == ext), "{ext}");
        }
        assert!(
            save_filters("drawing.psd").iter().all(|(_, extensions)| !extensions.iter().any(|e| e == "svg" || e == "pdf")),
            "ordinary Save As must not offer unsupported writer formats"
        );
        let owned = |name: &str, ext: &str| vec![(name.to_string(), vec![ext.to_string()])];
        assert_eq!(save_filters("drawing.SVG"), owned("Scalable Vector Graphics", "svg"));
        assert_eq!(save_filters("drawing.pdf"), owned("PDF document", "pdf"));
    }

    /// Upstream 3952fcef (#1178): a name whose extension has no listed type (.jpeg, .gif, .bmp,
    /// Save for Web's .html, the Measurement Log's .csv) leads with its own type, or the dialog
    /// appends the first one's (`photo.gif.psd`). No extension keeps the Photoshop default.
    #[test]
    fn save_filters_lead_with_the_files_own_extension() {
        for ext in ["jpeg", "gif", "bmp", "wbmp", "html", "csv", "pcpresets", "pcraft", "psb"] {
            let first = &save_filters(&format!("cat.{ext}"))[0];
            assert!(first.1.iter().any(|e| e == ext), "{ext}: {first:?}");
        }
        assert_eq!(save_filters("cat.GIF")[0], ("GIF".to_string(), vec!["gif".to_string()]));
        assert_eq!(save_filters("Untitled")[0].0, "Photoshop", "no extension keeps the default");
        assert_eq!(save_filters("Untitled").len(), SAVE_FILTERS.len());
    }

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

    /// One app launch with crash recovery in `dir` (dropping it is the crash: background writes
    /// already queued finish, nothing else runs).
    fn launch(dir: &Path) -> PhotocraftApp {
        PhotocraftApp::new(Session::new(), recovery_services(Some(dir.to_path_buf())))
    }

    /// The first pixel of each open document, and whether it's unsaved.
    fn open_docs(app: &PhotocraftApp) -> Vec<([f32; 4], bool)> {
        app.session.documents().iter().map(|d| (photocraft_compose::flatten(&d.doc).px.first().copied().unwrap_or_default(), d.is_dirty())).collect()
    }

    fn index_of(app: &PhotocraftApp, color: [f32; 4]) -> usize {
        open_docs(app).iter().position(|(c, _)| *c == color).unwrap()
    }

    fn new_doc(app: &mut PhotocraftApp, color: &str) {
        app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
        app.run("edit.fill", json!({"color": color})).unwrap();
    }

    fn autosave(app: &mut PhotocraftApp, ctx: &egui::Context) {
        prefs_ui::autosave_now(app);
        prefs_ui::tick(app, ctx);
    }

    /// Upstream 83d5c543 (#1577): a background autosave write that fails reaches the user (status
    /// bar and one notice), and the same revision is retried without another edit.
    #[test]
    fn failed_background_autosave_is_reported_and_retried_without_another_edit() {
        let dir = temp("autosave-retry");
        let recovery = dir.join("Recovery");
        // A regular file prevents creation of the recovery bundle directory.
        std::fs::write(&recovery, b"blocked").unwrap();
        let ctx = egui::Context::default();
        let mut app = launch(&recovery);
        new_doc(&mut app, "#ff0000");
        let revision = app.session.active().unwrap().revision;
        autosave(&mut app, &ctx);
        let mut failed = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            prefs_ui::tick(&mut app, &ctx);
            if app.ui.status.starts_with("Autosave failed:") {
                failed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(failed, "background write failures must reach the UI");
        assert!(app.ui.status_error);
        assert_eq!(app.ui.notices.iter().filter(|n| n.title == "Autosave failed" && n.error).count(), 1);
        assert!(list_recovery(&recovery).is_empty());
        // No edit occurs: retrying this very same revision must still work.
        std::fs::remove_file(&recovery).unwrap();
        std::fs::create_dir(&recovery).unwrap();
        autosave(&mut app, &ctx);
        let mut recovered = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            prefs_ui::tick(&mut app, &ctx);
            recovered = list_recovery(&recovery);
            // A committed bundle is listed before its sidecar is written; wait for the sidecar.
            if recovered.first().is_some_and(|e| e.info.revision == revision) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(recovered.len(), 1, "unchanged revision should retry after failure");
        assert_eq!(recovered[0].info.revision, revision);
        let restored = photocraft_format::recover(&recovered[0]).unwrap();
        assert_eq!(photocraft_compose::flatten(&restored).px.first().copied(), Some(RED));
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn wait_for_recovery_count(dir: &Path, expected: usize) {
        let started = std::time::Instant::now();
        while list_recovery(dir).len() != expected && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn recovered_documents_survive_a_second_crash_until_saved_or_closed() {
        let dir = std::env::temp_dir().join(format!("photocraft-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = egui::Context::default();
        // Launch 1: two unsaved documents are autosaved, then the app crashes.
        {
            let mut app = launch(&dir);
            new_doc(&mut app, "#ff0000");
            new_doc(&mut app, "#0000ff");
            autosave(&mut app, &ctx);
        }
        assert_eq!(list_recovery(&dir).len(), 2);
        // Launch 2 recovers both and crashes again before its first autosave (#444).
        {
            let mut app = launch(&dir);
            let mut docs = open_docs(&app);
            docs.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
            assert_eq!(docs, [(BLUE, true), (RED, true)], "recovered and still unsaved");
            prefs_ui::tick(&mut app, &ctx);
        }
        assert_eq!(list_recovery(&dir).len(), 2, "recovering keeps the entries on disk");
        // Launch 3 has both again; autosaving unchanged documents adds nothing, a new document
        // (whose id may repeat one from an earlier launch) gets an entry of its own.
        {
            let mut app = launch(&dir);
            assert_eq!(app.session.documents().len(), 2);
            autosave(&mut app, &ctx);
            new_doc(&mut app, "#00ff00");
            autosave(&mut app, &ctx);
        }
        assert_eq!(list_recovery(&dir).len(), 3);
        // Launch 4: closing and saving recovered documents removes their entries, no duplicates.
        let mut app = launch(&dir);
        assert_eq!(app.session.documents().len(), 3);
        let red = index_of(&app, RED);
        app.run("file.close", json!({"document": red})).unwrap();
        // Saving marks the revision saved (the file write itself is the save service's job).
        assert!(app.session.set_active(index_of(&app, BLUE)));
        let st = app.session.active_mut().unwrap();
        st.saved_revision = st.revision;
        prefs_ui::tick(&mut app, &ctx);
        wait_for_recovery_count(&dir, 1);
        let left = list_recovery(&dir);
        assert_eq!(left.len(), 1);
        assert_eq!(photocraft_compose::flatten(&photocraft_format::recover(&left[0]).unwrap()).px.first().copied(), Some(GREEN));
        let green = index_of(&app, GREEN);
        app.run("file.close", json!({"document": green})).unwrap();
        prefs_ui::tick(&mut app, &ctx);
        wait_for_recovery_count(&dir, 0);
        assert!(list_recovery(&dir).is_empty());
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("photocraft-services-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn red_png(w: u32, h: u32) -> Vec<u8> {
        let img = Image::from_u8(w, h, ChannelLayout::Rgba, [255u8, 0, 0, 255].repeat((w * h) as usize)).unwrap();
        photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).unwrap()
    }

    /// A 1×1 uncompressed 32-bit TGA (TGA has no magic number).
    pub(super) fn tga_1x1() -> Vec<u8> {
        let mut b = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 32, 8];
        b.extend_from_slice(&[0, 0, 255, 255]); // BGRA red
        b
    }

    #[test]
    fn copied_image_file_pastes_its_pixels() {
        let dir = temp("png");
        let png = dir.join("red.png");
        std::fs::write(&png, red_png(3, 2)).unwrap();
        let (w, h, px) = image_from_files(&[png]).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(px, [255u8, 0, 0, 255].repeat(6));
    }

    #[test]
    fn crlf_from_a_uri_list_is_ignored() {
        // arboard splits text/uri-list on LF, so GNOME Files' CRLF list leaves a '\r' behind.
        let dir = temp("crlf");
        let png = dir.join("red image.png");
        std::fs::write(&png, red_png(2, 2)).unwrap();
        let with_cr = PathBuf::from(format!("{}\r", png.display()));
        assert_eq!(image_from_files(&[with_cr]).map(|(w, h, _)| (w, h)), Some((2, 2)));
    }

    #[test]
    fn non_images_are_skipped_for_the_first_image() {
        let dir = temp("mixed");
        let (txt, png) = (dir.join("notes.txt"), dir.join("red.png"));
        std::fs::write(&txt, b"not an image").unwrap();
        std::fs::write(&png, red_png(4, 1)).unwrap();
        let missing = dir.join("gone.png");
        assert_eq!(image_from_files(&[missing.clone(), txt.clone(), dir.clone(), png]).map(|(w, h, _)| (w, h)), Some((4, 1)));
        assert!(image_from_files(&[missing, txt, dir]).is_none());
        assert!(image_from_files(&[]).is_none());
    }

    #[test]
    fn a_damaged_image_is_skipped_not_a_panic() {
        let dir = temp("damaged");
        let bad = dir.join("cut.png");
        std::fs::write(&bad, &red_png(8, 8)[..20]).unwrap();
        assert!(image_from_files(&[bad]).is_none());
    }

    #[test]
    fn tga_is_trusted_only_with_its_extension() {
        let dir = temp("tga");
        let (tga, bin) = (dir.join("red.tga"), dir.join("red.bin"));
        std::fs::write(&tga, tga_1x1()).unwrap();
        std::fs::write(&bin, tga_1x1()).unwrap();
        assert_eq!(image_from_files(&[tga]), Some((1, 1, vec![255, 0, 0, 255])));
        assert!(image_from_files(&[bin]).is_none());
    }
}

#[cfg(test)]
mod bugfix_develop_tests {
    #[test]
    fn bugfix_develop_clipboard_reader_survives_decoder_panic() {
        assert!(super::guarded_clipboard::<()>(|| panic!("damaged clipboard file")).is_none());
    }
}

#[cfg(test)]
mod prefs_safety_tests {
    use super::*;
    use photocraft_ui_egui::{PhotocraftApp, prefs_ui};
    use serde_json::json;

    struct TempPrefs(PathBuf);
    impl TempPrefs {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("li-editor-prefs-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("preferences.json")
        }
    }
    impl Drop for TempPrefs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn damaged_editor_preferences_keep_exact_bytes_before_service_save_and_report_on_restart() {
        for bytes in [
            br#"{"interface":{"theme":"studio""#.as_slice(),
            b"{\"interface\":{\"theme\":\"studio\"}}\xff\x80".as_slice(),
            br#"{"interface":{"theme":"retired-theme","showTooltips":false},"performance":{"historyStates":12}}"#.as_slice(),
        ] {
            let temp = TempPrefs::new();
            let path = temp.path();
            std::fs::write(&path, bytes).unwrap();
            let mut services = preferences_services(Some(path.clone()));
            if std::str::from_utf8(bytes).is_err() {
                assert!(services.load_prefs_checked.as_mut().unwrap()().is_err());
            }
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
            app.run("prefs.set", json!({"values":{"interface.theme":"studio"}})).unwrap();
            prefs_ui::tick(&mut app, &egui::Context::default());
            let record = prefs_recovery(&path).unwrap();
            let backup = prefs_backup_dir(&path).join(&record.backup_name);
            assert_eq!(std::fs::read(&backup).unwrap(), bytes);
            let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(saved["interface"]["theme"], "studio");
            if bytes.starts_with(br#"{"interface":{"theme":"retired-theme"#) {
                assert_eq!(saved["interface"]["showTooltips"], false);
                assert_eq!(saved["performance"]["historyStates"], 12);
            }
            assert!(app.ui.notices.iter().any(|notice| notice.lines.iter().any(|line| line.contains(backup.to_string_lossy().as_ref()))));
            app.ui.status = "an unrelated operation completed".into();
            let restarted = PhotocraftApp::new(photocraft_engine::Session::new(), preferences_services(Some(path.clone())));
            assert!(
                restarted
                    .ui
                    .notices
                    .iter()
                    .any(|notice| notice.dismiss_pref.is_some() && notice.lines.iter().any(|line| line.contains(backup.to_string_lossy().as_ref())))
            );
            save_editor_prefs(&path, &serde_json::to_string(&saved).unwrap()).unwrap();
            assert_eq!(std::fs::read_dir(prefs_backup_dir(&path)).unwrap().count(), 1);
            // The same original content encountered again reuses a verified backup.
            std::fs::write(&path, bytes).unwrap();
            save_editor_prefs(&path, &serde_json::to_string(&saved).unwrap()).unwrap();
            assert_eq!(prefs_recovery(&path).unwrap().backup_name, record.backup_name);
            assert_eq!(std::fs::read_dir(prefs_backup_dir(&path)).unwrap().count(), 1);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(backup).unwrap().permissions().mode() & 0o077, 0);
            }
        }
    }

    #[test]
    fn backup_or_recovery_record_failure_never_replaces_original_preferences() {
        for block_record in [false, true] {
            let temp = TempPrefs::new();
            let path = temp.path();
            let original = b"\xff\x80truncated {\"original\":true";
            std::fs::write(&path, original).unwrap();
            if block_record {
                std::fs::create_dir(prefs_recovery_file(&path)).unwrap();
            } else {
                std::fs::write(prefs_backup_dir(&path), b"blocked backup directory").unwrap();
            }
            let mut services = preferences_services(Some(path.clone()));
            for _ in 0..5 {
                let error = services.save_prefs.as_mut().unwrap()("{\"interface\":{\"theme\":\"studio\"}}").unwrap_err();
                assert!(error.contains("original was not changed"), "{error}");
                assert_eq!(std::fs::read(&path).unwrap(), original);
            }
            if block_record {
                let entries = std::fs::read_dir(prefs_backup_dir(&path)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
                assert_eq!(entries.len(), 1);
                assert_eq!(std::fs::read(entries[0].path()).unwrap(), original);
                std::fs::remove_dir(prefs_recovery_file(&path)).unwrap();
                services.save_prefs.as_mut().unwrap()("{}").unwrap();
                assert_eq!(std::fs::read_dir(prefs_backup_dir(&path)).unwrap().count(), 1);
                assert_eq!(prefs_recovery(&path).unwrap().backup_name, entries[0].file_name().to_string_lossy());
            }
        }
    }

    #[test]
    fn partial_preference_backup_write_failures_clean_only_their_own_file() {
        use std::io::Write;
        let temp = TempPrefs::new();
        let source = temp.path();
        let original = b"{broken preferences";
        std::fs::write(&source, original).unwrap();
        let directory = prefs_backup_dir(&source);
        std::fs::create_dir_all(&directory).unwrap();
        for attempt in 0..5 {
            let path = directory.join(format!("preferences.corrupt-test-{attempt}.json"));
            let error = write_new_prefs_backup_with(&path, |file| {
                file.write_all(&original[..4])?;
                Err(std::io::Error::other("injected short-write failure"))
            })
            .unwrap_err();
            assert!(error.to_string().contains("short-write"));
            assert!(!path.exists());
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
        let existing = directory.join("preferences.corrupt-existing.json");
        std::fs::write(&existing, b"older backup").unwrap();
        assert_eq!(write_new_prefs_backup_with(&existing, |_| Ok(())).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&existing).unwrap(), b"older backup");
    }

    #[test]
    fn unreadable_preferences_are_errors_not_absence_and_cannot_be_overwritten() {
        let temp = TempPrefs::new();
        let path = temp.path();
        std::fs::create_dir(&path).unwrap(); // deterministic read failure on every platform, including privileged tests
        let sentinel = path.join("do-not-delete");
        std::fs::write(&sentinel, b"original directory").unwrap();
        let mut services = preferences_services(Some(path.clone()));
        assert!(services.load_prefs_checked.as_mut().unwrap()().is_err());
        assert!(services.save_prefs.as_mut().unwrap()("{}").is_err());
        assert_eq!(std::fs::read(sentinel).unwrap(), b"original directory");
        assert!(path.is_dir());
    }
}

#[cfg(all(test, unix))]
#[test]
fn preference_orphan_reuse_never_follows_a_backup_symlink() {
    use std::os::unix::fs::symlink;
    let directory = std::env::temp_dir().join(format!("li-pref-symlink-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("preferences.json");
    let bytes = b"{broken preferences";
    std::fs::write(&path, bytes).unwrap();
    let other = directory.join("unrelated-file");
    std::fs::write(&other, bytes).unwrap();
    std::fs::create_dir_all(prefs_backup_dir(&path)).unwrap();
    let link = prefs_backup_dir(&path).join("preferences.corrupt-link.json");
    symlink(&other, &link).unwrap();
    save_editor_prefs(&path, "{}").unwrap();
    let record = prefs_recovery(&path).unwrap();
    assert_ne!(record.backup_name, "preferences.corrupt-link.json");
    assert_eq!(std::fs::read(prefs_backup_dir(&path).join(record.backup_name)).unwrap(), bytes);
    assert_eq!(std::fs::read(other).unwrap(), bytes);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(test)]
mod prepared_overwrite_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn overwrite_preparation_leaves_original_and_trash_untouched_and_rejects_changed_source() {
        let dir = std::env::temp_dir().join(format!("li-prepared-overwrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("original.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([40u8, 80, 120])).save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let document = Document::with_background(
            "Replacement",
            photocraft_doc::Size::new(4, 3),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U8,
            photocraft_doc::Color::WHITE,
        );
        let touched = Arc::new(AtomicBool::new(false));
        let flag = touched.clone();
        let prepared = overwrite_original(&document, &path, 90, move |_| {
            flag.store(true, Ordering::Relaxed);
            Ok(PathBuf::new())
        })
        .unwrap();
        assert_eq!(prepared.original.as_ref(), &original);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!touched.load(Ordering::Relaxed));
        image::RgbImage::from_pixel(4, 3, image::Rgb([120u8, 80, 40])).save(&path).unwrap();
        let changed = std::fs::read(&path).unwrap();
        assert!((prepared.commit)().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), changed);
        assert!(!touched.load(Ordering::Relaxed));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SaveTargetStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
fn save_target_stamp(path: &Path) -> Result<Option<SaveTargetStamp>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err("Save destination is not a regular file".into());
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(Some(SaveTargetStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        identity: (metadata.dev(), metadata.ino(), metadata.ctime(), metadata.ctime_nsec()),
    }))
}
fn resolved_save_target(path: &Path) -> Result<PathBuf, String> {
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    match std::fs::symlink_metadata(path) {
        Ok(_) => std::fs::canonicalize(path).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(std::fs::canonicalize(parent).map_err(|error| error.to_string())?.join(path.file_name().ok_or("Save path has no file name")?))
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn prepare_document_save(
    snapshot: Arc<Document>,
    name: &str,
    settings: &photocraft_ui_egui::ExportSettings,
    interrupt: &lightcraft_sysmem::jobs::Interrupt<'_>,
) -> Result<photocraft_ui_egui::PreparedSave, String> {
    let path = Path::new(name);
    interrupt.check().map_err(|e| e.to_string())?;
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let resolved = resolved_save_target(path)?;
    let stamp = save_target_stamp(path)?;
    let original = if stamp.is_some() { Some(Arc::new(photocraft_format::read_file(path).map_err(|e| e.to_string())?)) } else { None };
    if save_target_stamp(path)? != stamp || resolved_save_target(path)? != resolved {
        return Err("Save destination changed while reading its previous contents".into());
    }
    let snapshot = if let Some(bytes) = &original {
        photocraft_engine::develop_layer_cmds::freeze_source_snapshot(&snapshot, name, bytes.clone()).map_err(|e| e.to_string())?
    } else {
        snapshot
    };
    let mut options = photocraft_io::ExportOptions::default();
    if let Some(value) = settings.jpeg_quality {
        options.encode.jpeg_quality = value;
    }
    options.encode.webp_lossless = settings.webp_lossless;
    if let Some(value) = settings.webp_quality {
        options.encode.webp_quality = value;
    }
    options.tiff_layers = settings.tiff_layers;
    options.read_linked_files = settings.read_linked_files;
    options.xmp = if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None };
    let progress = |fraction| interrupt.progress(fraction * 0.8);
    let cancel = || interrupt.cancelled();
    let encode = lightcraft_sysmem::jobs::Interrupt::new(&cancel, &progress);
    let output = crate::crash_guard::guard("Save", || {
        photocraft_engine::vector_integration::export_document_background(&snapshot, name, &options, &encode).map_err(|e| e.to_string())
    })?;
    photocraft_format::read::admit_output_size(output.bytes.len() as u64).map_err(|e| e.to_string())?;
    interrupt.check().map_err(|e| e.to_string())?;
    let progress = |fraction| interrupt.progress(0.8 + fraction * 0.19);
    let writing = lightcraft_sysmem::jobs::Interrupt::new(&cancel, &progress);
    li_fs::refuse_read_only(&resolved).map_err(|e| e.to_string())?;
    let staged = li_fs::stage_write(&resolved, &output.bytes, &writing).map_err(|e| e.to_string())?;
    let path = path.to_owned();
    #[cfg(not(target_os = "linux"))]
    let expected = original.clone();
    Ok(photocraft_ui_egui::PreparedSave {
        original,
        snapshot,
        warnings: output.warnings,
        write: photocraft_ui_egui::PreparedWrite {
            commit: Box::new(move || {
                if save_target_stamp(&path)? != stamp || resolved_save_target(&path)? != resolved {
                    return Err("Save destination changed; save again to review the current file".into());
                }
                // Linux ctime plus file identity detects content changes without rereading a large
                // file on the UI thread. Other targets retain an exact-byte check until their
                // native change-token implementation is available.
                #[cfg(not(target_os = "linux"))]
                if let Some(expected) = expected
                    && photocraft_format::read_file(&path).map_err(|e| e.to_string())? != *expected
                {
                    return Err("Save destination changed; save again to review the current file".into());
                }
                staged.commit().map_err(|e| e.to_string())
            }),
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod save_worker_native_tests {
    use super::*;
    use lightcraft_sysmem::jobs::Interrupt;
    use photocraft_doc::{ColorMode, Layer, LayerContent, SampleType, Size, SmartObject, SmartSource};
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tag = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "li-save-worker-{}-{tag}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn doc() -> Arc<Document> {
        photocraft_engine::vector_integration::initialize();
        let mut doc = Document::new("snapshot", Size::new(32, 24), ColorMode::Rgb, SampleType::U8);
        let mut layer = Layer::raster("pixels", doc.pixel_format());
        layer.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.2, 0.4, 0.7, 1.0]);
        doc.layers.push(layer);
        Arc::new(doc)
    }
    fn path(dir: &Dir, name: &str) -> String {
        dir.0.join(name).to_string_lossy().into_owned()
    }
    #[test]
    fn background_save_native_stages_without_publication_and_reopens_exact_snapshot() {
        let dir = Dir::new();
        let path = path(&dir, "saved.pcraft");
        let doc = doc();
        let prepared = prepare_document_save(doc.clone(), &path, &Default::default(), &Interrupt::NONE).unwrap();
        assert!(!Path::new(&path).exists());
        assert!(std::fs::read_dir(&dir.0).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().ends_with(".pcsave.tmp")));
        (prepared.write.commit)().unwrap();
        let back = photocraft_io::import(&path, &std::fs::read(&path).unwrap()).unwrap().document;
        assert_eq!(photocraft_compose::flatten(&back).to_rgba8().pixels, photocraft_compose::flatten(&doc).to_rgba8().pixels);
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[test]
    fn background_save_native_drop_cancel_and_changed_destination_keep_previous_file() {
        let dir = Dir::new();
        let path = path(&dir, "copy.png");
        std::fs::write(&path, b"old contents").unwrap();
        let prepared = prepare_document_save(doc(), &path, &Default::default(), &Interrupt::NONE).unwrap();
        drop(prepared);
        assert_eq!(std::fs::read(&path).unwrap(), b"old contents");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
        assert!(prepare_document_save(doc(), &path, &Default::default(), &Interrupt::cancel_only(&|| true)).is_err());
        let prepared = prepare_document_save(doc(), &path, &Default::default(), &Interrupt::NONE).unwrap();
        std::fs::write(&path, b"new contents").unwrap();
        assert!((prepared.write.commit)().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"new contents");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[test]
    fn background_save_native_embeds_self_referenced_old_bytes_before_encoding() {
        let dir = Dir::new();
        let path = path(&dir, "source.pcraft");
        let original_doc = doc();
        let bytes = photocraft_io::export(&original_doc, "pcraft", &Default::default()).unwrap().bytes;
        std::fs::write(&path, &bytes).unwrap();
        let mut edited = (*original_doc).clone();
        let cache = edited.layers[0].surface().unwrap().clone();
        edited.layers.push(Layer::new(
            "linked original",
            LayerContent::Smart(SmartObject::new(SmartSource::Linked { path: path.clone() }, Default::default(), Some(cache))),
        ));
        let prepared = prepare_document_save(Arc::new(edited), &path, &Default::default(), &Interrupt::NONE).unwrap();
        assert_eq!(prepared.original.as_deref().unwrap().as_slice(), bytes);
        (prepared.write.commit)().unwrap();
        let back = photocraft_io::import(&path, &std::fs::read(&path).unwrap()).unwrap().document;
        assert!(matches!(&back.layers[1].content, LayerContent::Smart(s) if matches!(&s.source, SmartSource::Embedded { bytes:b, .. } if b.as_slice()==bytes)));
    }
    #[test]
    #[cfg(unix)]
    fn background_save_native_detects_symlink_retarget_between_hardlinks() {
        let dir = Dir::new();
        let a = dir.0.join("A.pcraft");
        let b = dir.0.join("B.pcraft");
        let link = dir.0.join("chosen.pcraft");
        std::fs::write(&a, b"old source").unwrap();
        std::fs::hard_link(&a, &b).unwrap();
        std::os::unix::fs::symlink(&a, &link).unwrap();
        let prepared = prepare_document_save(doc(), &link.to_string_lossy(), &Default::default(), &Interrupt::NONE).unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&b, &link).unwrap();
        assert!((prepared.write.commit)().is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"old source");
        assert_eq!(std::fs::read(&link).unwrap(), b"old source");
        assert!(!std::fs::read_dir(&dir.0).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().ends_with(".pcsave.tmp")));
    }
    #[test]
    fn background_save_native_rejects_a_new_file_created_after_staging() {
        let dir = Dir::new();
        let path = path(&dir, "new.pcraft");
        let prepared = prepare_document_save(doc(), &path, &Default::default(), &Interrupt::NONE).unwrap();
        std::fs::write(&path, b"another application created this").unwrap();
        assert!((prepared.write.commit)().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"another application created this");
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use serde_json::{Value, json};
    fn fixture(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tests").join(format!("{tag}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }
    #[test]
    fn one_store_holds_library_and_ai_and_old_stores_are_not_read() {
        let directory = fixture("one-store");
        let prefs = directory.join("preferences.json");
        // Files earlier versions kept beside the preferences are ignored.
        std::fs::write(directory.join("library-ui.json"), br#"{"settings":{"libraryPath":"/old"}}"#).unwrap();
        std::fs::write(directory.join("mode"), "develop").unwrap();
        std::fs::create_dir_all(directory.join("models")).unwrap();
        std::fs::write(directory.join("config.json"), br#"{"comfy_host":"old.example"}"#).unwrap();
        assert_eq!(read_preferences(&prefs).unwrap(), None);
        let library = json!({"settings":{"libraryPath":directory.join("catalog"),"previewEdge":3840},"autoAdvance":true,"module":"develop"});
        let stored = json!({"library": library, "ai": {"comfy_host":"server.example","future_provider":{"token":"test-only"}}});
        std::fs::write(&prefs, serde_json::to_vec(&stored).unwrap()).unwrap();
        let mut service = preferences_services(Some(prefs.clone()));
        let text = service.load_prefs_checked.as_mut().unwrap()().unwrap().unwrap();
        let mut value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["library"], library);
        value["general"]["autoUpdateOpenDocuments"] = json!(true);
        service.save_prefs.as_mut().unwrap()(&serde_json::to_string(&value).unwrap()).unwrap();
        // Consecutive local saves work without confusing our own last write for an external edit.
        service.save_prefs.as_mut().unwrap()(&serde_json::to_string(&value).unwrap()).unwrap();
        li_ai::settings::with_store_backend(ai_preferences_backend(prefs.clone()), || {
            let mut settings = li_ai::AiSettings::load();
            assert_eq!(settings.comfy_host, "server.example");
            settings.comfy_port = 8190;
            settings.save().unwrap();
            assert_eq!(li_ai::AiSettings::load().comfy_port, 8190);
        });
        let root: Value = serde_json::from_slice(&std::fs::read(&prefs).unwrap()).unwrap();
        assert_eq!(root["library"]["autoAdvance"], true);
        assert_eq!(root["ai"]["comfy_port"], 8190);
        assert_eq!(root["ai"]["future_provider"]["token"], "test-only");
        // A general/UI writer read before that AI save must retry, never replace the new AI data.
        assert!(service.save_prefs.as_mut().unwrap()(&text).is_err());
        assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&prefs).unwrap()).unwrap()["ai"]["comfy_port"], 8190);
    }

    #[test]
    fn unreadable_library_section_blocks_catalog_substitution_and_keeps_exact_bytes() {
        let directory = fixture("invalid-section");
        let prefs = directory.join("preferences.json");
        std::fs::write(&prefs, br#"{"library":["bad section"],"ai":{"comfy_port":8190}}"#).unwrap();
        let original = std::fs::read(&prefs).unwrap();
        let mut service = preferences_services(Some(prefs.clone()));
        let text = service.load_prefs_checked.as_mut().unwrap()().unwrap().unwrap();
        let root: Value = serde_json::from_str(&text).unwrap();
        assert!(root["library"][LIBRARY_UNREADABLE_KEY].as_str().unwrap().contains("could not be read"));
        assert_eq!(root["library"].as_object().unwrap().len(), 1, "only the marker, no copy of the damaged value");
        assert_eq!(root["ai"]["comfy_port"], 8190);
        assert_eq!(std::fs::read(&prefs).unwrap(), original, "reading never rewrites the file");
        service.save_prefs.as_mut().unwrap()(&text).unwrap();
        let backups: Vec<_> = prefs_backup_dir(&prefs).read_dir().unwrap().filter_map(Result::ok).map(|entry| entry.path()).collect();
        assert!(backups.iter().any(|backup| std::fs::read(backup).unwrap() == original), "the damaged original is kept byte for byte");
    }
}
