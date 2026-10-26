//! Capability-based filesystem policy for untrusted automation paths.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use li_fs::{AtomicFs, RenameRetry};
use serde_json::Value;

use crate::AutomationError;

const DENIED: &str = "automation filesystem access is not granted";

#[derive(Clone)]
struct RootCapability {
    dir: Arc<Dir>,
    /// The launch-time root path (canonical when it resolves). Reads and writes never go through
    /// it; it only names files to engines that key records by path (the Library catalog).
    path: PathBuf,
    /// Trusted lexical names of this launch-time root, including platform path aliases.
    aliases: Vec<PathBuf>,
}

/// Shared writer operations resolved exclusively through the granted directory capability.
struct CapFs<'a> {
    dir: &'a Dir,
}

impl AtomicFs for CapFs<'_> {
    fn write_new(&self, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = self.dir.open_with(tmp, &options)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.dir.rename(from, self.dir, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        let result = self.dir.remove_file(path);
        #[cfg(windows)]
        if result.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied) {
            let metadata = self.dir.symlink_metadata(path)?;
            if metadata.is_file() && !metadata.file_type().is_symlink() && metadata.permissions().readonly() {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                self.dir.set_permissions(path, permissions)?;
                return self.dir.remove_file(path);
            }
        }
        result
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.dir.open(dir)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Ok(())
        }
    }

    fn copy_permissions(&self, from: &Path, to: &Path) -> io::Result<()> {
        let permissions = self.dir.metadata(from)?.permissions();
        self.dir.open(to)?.set_permissions(permissions)
    }

    fn prepare_target(&self, path: &Path) -> io::Result<PathBuf> {
        // Never canonicalize or create parents via ambient paths. cap-std resolves each
        // operation beneath the held handle; absent parents retain the existing error.
        Ok(path.to_path_buf())
    }

    fn preserve_permissions(&self, from: &Path, to: &Path, _policy: li_fs::Policy) -> io::Result<()> {
        let metadata = self.dir.metadata(from)?;
        let mut options = OpenOptions::new();
        options.write(true);
        let file = self.dir.open_with(to, &options)?;
        #[cfg(unix)]
        let file = file.into_std();
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            li_fs::preserve_access(&file, metadata.mode(), metadata.gid(), || {
                #[cfg(target_os = "linux")]
                {
                    let source = self.dir.open(from)?.into_std();
                    li_fs::read_access_acl(&source)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Ok(None)
                }
            })?;
        }
        #[cfg(not(unix))]
        file.set_permissions(metadata.permissions())?;
        file.sync_all()
    }
}

/// Separate directory capabilities for automation reads and writes.
///
/// Root paths are trusted launch-time configuration. Request paths are always
/// untrusted, forward-slash relative paths resolved by `cap-std` beneath the
/// held directory handle. Absolute paths, parent traversal, alternate
/// separators, Windows prefixes and empty components are rejected before I/O.
#[derive(Clone, Default)]
pub struct AuthorizedWorkspace {
    read: Option<RootCapability>,
    write: Option<RootCapability>,
}

/// Most files [`AuthorizedWorkspace::list_files`] returns for one request.
pub const MAX_LISTED_FILES: usize = 100_000;

impl AuthorizedWorkspace {
    /// Open the configured roots as capabilities. Either authority may be
    /// omitted; omitted authority fails closed.
    pub fn new(read_root: Option<&Path>, write_root: Option<&Path>) -> Result<Self, AutomationError> {
        Ok(Self { read: open_root(read_root, "read")?, write: open_root(write_root, "write")? })
    }

    /// The read root's path (launch-time configuration), when read authority was granted.
    pub fn read_root_path(&self) -> Option<&Path> {
        self.read.as_ref().map(|r| r.path.as_path())
    }

    /// Trusted lexical spellings of the read root. These only route displayed source names;
    /// actual reads continue through the held directory capability.
    pub fn read_root_aliases(&self) -> Option<&[PathBuf]> {
        self.read.as_ref().map(|root| root.aliases.as_slice())
    }

    /// The write root's path (launch-time configuration), when write authority was granted.
    pub fn write_root_path(&self) -> Option<&Path> {
        self.write.as_ref().map(|r| r.path.as_path())
    }

    /// Whether something already exists at `path` below the write root (export name collisions).
    /// An invalid path or absent authority reports `false`; the write itself then fails.
    pub fn write_target_exists(&self, path: &str) -> bool {
        match (relative_path(path), self.write.as_ref()) {
            (Ok(rel), Some(root)) => root.dir.symlink_metadata(rel).is_ok(),
            _ => false,
        }
    }

    /// The regular files at or below `path` under the read root: `path` itself when it is a
    /// file, else every file in that folder and its subfolders (sorted, hidden entries skipped,
    /// symbolic links not followed), as forward-slash relative paths. `""` or `"."` lists the
    /// whole read root. Fails past [`MAX_LISTED_FILES`] files.
    pub fn list_files(&self, path: &str) -> Result<Vec<String>, AutomationError> {
        let root = self.read.as_ref().ok_or_else(|| AutomationError::BadRequest(format!("{DENIED}: read authority is absent")))?;
        let top = if matches!(path, "" | ".") { None } else { Some(relative_path(path)?) };
        if let Some(rel) = &top {
            let metadata = root.dir.symlink_metadata(rel).map_err(|e| file_error("list", path, e))?;
            if metadata.is_file() {
                return Ok(vec![path.to_string()]);
            }
            if !metadata.is_dir() {
                return Err(AutomationError::BadRequest(format!("automation read path is not a file or folder: `{path}`")));
            }
        }
        let mut out = Vec::new();
        let mut stack = vec![top.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()];
        while let Some(dir_rel) = stack.pop() {
            let dir = if dir_rel.is_empty() { root.dir.open_dir(".") } else { root.dir.open_dir(&dir_rel) };
            let dir = dir.map_err(|e| file_error("list", &dir_rel, e))?;
            let mut entries: Vec<(String, bool)> = Vec::new();
            for entry in dir.entries().map_err(|e| file_error("list", &dir_rel, e))? {
                let entry = entry.map_err(|e| file_error("list", &dir_rel, e))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let Ok(kind) = entry.file_type() else { continue };
                let child = if dir_rel.is_empty() { name } else { format!("{dir_rel}/{name}") };
                if kind.is_dir() {
                    entries.push((child, true));
                } else if kind.is_file() {
                    entries.push((child, false));
                }
            }
            for (child, is_dir) in entries {
                if is_dir {
                    stack.push(child);
                } else if out.len() >= MAX_LISTED_FILES {
                    return Err(AutomationError::BadRequest(format!("`{path}` holds more than {MAX_LISTED_FILES} files; name a smaller folder")));
                } else {
                    out.push(child);
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Enumerate one folder through its held read capability; never follow entry symlinks.
    pub fn list_folder(&self, path: &str) -> Result<Vec<String>, AutomationError> {
        let root = self.read.as_ref().ok_or_else(|| AutomationError::BadRequest(format!("{DENIED}: read authority is absent")))?;
        let relative = if matches!(path, "" | ".") { PathBuf::from(".") } else { relative_path(path)? };
        let folder = root.dir.open_dir(relative).map_err(|error| file_error("browse", path, error))?;
        let mut files = Vec::new();
        for entry in folder.entries().map_err(|error| file_error("browse", path, error))? {
            let entry = entry.map_err(|error| file_error("browse", path, error))?;
            let name = entry.file_name().to_str().map(str::to_owned).ok_or_else(|| path_error(path, "folder names must be valid UTF-8"))?;
            if name.starts_with('.') || !entry.file_type().map_err(|error| file_error("browse", path, error))?.is_file() {
                continue;
            }
            if files.len() >= MAX_LISTED_FILES {
                return Err(path_error(path, "folder contains too many files"));
            }
            files.push(if matches!(path, "" | ".") { name } else { format!("{path}/{name}") });
        }
        files.sort();
        Ok(files)
    }

    /// Read one regular file below the configured read root.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, AutomationError> {
        let relative = relative_path(path)?;
        let root = self.read.as_ref().ok_or_else(|| AutomationError::BadRequest(format!("{DENIED}: read authority is absent")))?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(li_fs::read::REGULAR_READ_OPEN_FLAGS);
        }
        let mut file = root.dir.open_with(&relative, &options).map_err(|e| file_error("read", path, e))?;
        let metadata = file.metadata().map_err(|e| file_error("read", path, e))?;
        if !metadata.is_file() {
            return Err(AutomationError::BadRequest(format!("automation read path is not a regular file: `{path}`")));
        }
        // Bounded reads, and a clear error for a file larger than memory (#375).
        photocraft_format::read::read_all(&mut file, metadata.len()).map_err(|e| file_error("read", path, e))
    }

    /// Create or replace one file below the configured write root, crash-safely: the bytes go to
    /// a temporary file beside the target, which is synced and renamed over it (the same steps
    /// as [`li_fs::atomic_write`], through the directory capability). On failure
    /// the previous file is untouched and the temporary file is removed.
    ///
    /// The parent directory must already exist. `cap-std` performs path
    /// resolution and file creation relative to the held directory handle, so
    /// a non-existent final target is supported without ambient path access.
    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<(), AutomationError> {
        let relative = relative_path(path)?;
        let root = self.write.as_ref().ok_or_else(|| AutomationError::BadRequest(format!("{DENIED}: write authority is absent")))?;
        li_fs::atomic_write_with(&CapFs { dir: &root.dir }, RenameRetry::platform(), &relative, bytes).map_err(|e| file_error("write", path, e))
    }
}

fn open_root(path: Option<&Path>, authority: &str) -> Result<Option<RootCapability>, AutomationError> {
    let Some(path) = path else { return Ok(None) };
    if path.as_os_str().is_empty() {
        return Err(AutomationError::BadRequest(format!("automation {authority} root is empty")));
    }
    let dir = Dir::open_ambient_dir(path, ambient_authority())
        .map_err(|e| AutomationError::Io(format!("cannot open automation {authority} root `{}`: {e}", path.display())))?;
    let absolute = std::path::absolute(path).map_err(|error| AutomationError::Io(format!("cannot name automation {authority} root: {error}")))?;
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| absolute.clone());
    let mut aliases = vec![path.clone()];
    if !aliases.contains(&absolute) {
        aliases.push(absolute);
    }
    #[cfg(target_os = "macos")]
    for alias in ["/var", "/tmp", "/etc"] {
        if let Ok(canonical) = std::fs::canonicalize(alias)
            && let Ok(relative) = path.strip_prefix(&canonical)
        {
            let spelling = Path::new(alias).join(relative);
            if !aliases.contains(&spelling) {
                aliases.push(spelling);
            }
        }
    }
    Ok(Some(RootCapability { dir: Arc::new(dir), path, aliases }))
}

pub(crate) fn relative_path(raw: &str) -> Result<PathBuf, AutomationError> {
    if raw.is_empty() {
        return Err(path_error(raw, "path is empty"));
    }
    if raw.contains('\\') {
        return Err(path_error(raw, "alternate separators are not allowed; use `/`"));
    }
    if raw.starts_with('/') {
        return Err(path_error(raw, "absolute paths are not allowed"));
    }
    if raw.contains(':') {
        return Err(path_error(raw, "drive, device and stream prefixes are not allowed"));
    }
    for component in raw.split('/') {
        if component.is_empty() {
            return Err(path_error(raw, "empty path components are not allowed"));
        }
        if component == "." {
            return Err(path_error(raw, "`.` path components are not allowed"));
        }
        if component == ".." {
            return Err(path_error(raw, "parent traversal is not allowed"));
        }
        if component.ends_with(['.', ' ']) {
            return Err(path_error(raw, "path components ending in a dot or space are not allowed"));
        }
        if is_windows_device_name(component) {
            return Err(path_error(raw, "reserved device names are not allowed"));
        }
    }
    Ok(PathBuf::from(raw))
}

fn is_windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$")
        || stem.strip_prefix("COM").is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        || stem.strip_prefix("LPT").is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

fn path_error(path: &str, reason: &str) -> AutomationError {
    AutomationError::BadRequest(format!("automation path rejected: {reason}: `{path}`"))
}

fn file_error(operation: &str, path: &str, error: std::io::Error) -> AutomationError {
    let kind = error.kind();
    AutomationError::Io(format!("automation {operation} `{path}` failed ({kind:?}): {error}"))
}

/// Filesystem-bearing engine commands have not yet been converted to consume
/// directory capabilities. Automation must use `doc.open`, `doc.save` and
/// `doc.render` for file effects until those commands are migrated.
pub fn authorize_engine_command(id: &str, params: &Value) -> Result<(), AutomationError> {
    let safe_file_command = matches!(
        id,
        "file.new"
            | "file.newFromClipboard"
            | "file.close"
            | "file.closeAll"
            | "file.closeOthers"
            | "file.fileInfo"
            | "file.automate.fitImage"
            | "file.automate.conditionalModeChange"
            | "file.scripts.flattenAllLayerEffects"
            | "file.scripts.flattenAllMasks"
            | "file.scripts.deleteAllEmptyLayers"
            | "file.export.exportPreferences"
    );
    if id.starts_with("file.") && !safe_file_command {
        return Err(command_error(id));
    }
    if (id.starts_with("layer.smartObjects.") && id != "layer.smartObjects.convertToSmartObject")
        || matches!(
            id,
            "pattern.import"
                | "pattern.export"
                | "measurementLog.export"
                | "layer.videoLayers.newVideoLayerFromFile"
                | "layer.videoLayers.replaceFootage"
                | "layer.videoLayers.reloadFrame"
                // local-image: read a file at any path (a generated image; a photo opened as a
                // Develop layer). The app runs them itself; automation can't.
                | "ai.placeLayer"
                | "develop.openPhoto"
        )
        || command_uses_ambient_path(id, params)
        || profile_command_may_read_ambient(id, params)
        || preferences_may_grant_ambient_paths(id, params)
        || params_contain_ambient_path(id, params)
    {
        return Err(command_error(id));
    }
    Ok(())
}

/// Desktop sessions may already contain user-configured ambient colour-profile
/// paths. Commands which implicitly resolve those settings are denied even
/// when their request parameters contain no path. Fresh headless sessions
/// cannot acquire such settings through the automation interface.
///
/// The desktop session also holds the user's cloud AI provider keys, so stored credentials are
/// out of reach and surfaces that can start paid cloud generation stay closed
/// ([`authorize_cloud_command`] without `allow_cloud_ai`).
pub fn authorize_desktop_engine_command(id: &str, params: &Value) -> Result<(), AutomationError> {
    authorize_engine_command(id, params)?;
    if id.starts_with("image.mode.") {
        return Err(command_error(id));
    }
    authorize_cloud_command(id, params, false)
}

/// Commands that open a surface whose Generate button can send the prompt and images to a paid
/// cloud provider with the API key saved in the app (the Generate panel, File › New from
/// Prompt). The engine's own `ai.*` commands run on local models and are not listed.
pub const CLOUD_GENERATION_COMMANDS: &[&str] = &["li.newFromPrompt", "li.panel.generate"];

/// Commands that open the Local AI settings, where provider API keys and credentials are entered.
pub const CREDENTIAL_SETTINGS_COMMANDS: &[&str] = &["li.localAi"];

/// Deny automation access to stored credentials (always) and to the paid cloud generation
/// surfaces (unless `allow_cloud_ai`).
pub fn authorize_cloud_command(id: &str, params: &Value, allow_cloud_ai: bool) -> Result<(), AutomationError> {
    if CREDENTIAL_SETTINGS_COMMANDS.contains(&id) || touches_secret_preference(id, params) {
        return Err(credential_error(id));
    }
    if !allow_cloud_ai && CLOUD_GENERATION_COMMANDS.contains(&id) {
        return Err(cloud_ai_error(id));
    }
    Ok(())
}

/// A preference name that holds a credential (`openai_api_key`, `server.token`, …), with the
/// engine's normalisation: ASCII letters and digits only, lowercase.
pub fn secret_name(name: &str) -> bool {
    let key: String = name.chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect();
    ["apikey", "token", "password", "secret"].iter().any(|suffix| key.ends_with(suffix))
}

/// A dotted preference path with a credential segment.
pub fn secret_path(path: &str) -> bool {
    path.split('.').any(secret_name)
}

/// Whether any object key below `value` names a credential.
pub fn mentions_secret(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| secret_path(key) || mentions_secret(value)),
        Value::Array(values) => values.iter().any(mentions_secret),
        _ => false,
    }
}

/// `prefs.get` / `prefs.set` / `prefs.reset` naming a credential, carrying one in a section
/// value, or resetting a scope that holds them (everything, or the `ai` section).
fn touches_secret_preference(id: &str, params: &Value) -> bool {
    if !id.starts_with("prefs.") {
        return false;
    }
    let path = params.get("path").and_then(Value::as_str);
    if path.is_some_and(secret_path) || params.get("values").is_some_and(mentions_secret) || params.get("value").is_some_and(mentions_secret) {
        return true;
    }
    id == "prefs.reset" && path.and_then(|p| p.split('.').find(|segment| !segment.is_empty())).is_none_or(|section| section == "ai")
}

fn credential_error(id: &str) -> AutomationError {
    AutomationError::BadRequest(format!(
        "automation command `{id}` reaches stored API keys, tokens or passwords and is disabled; enter credentials in the app yourself"
    ))
}

fn cloud_ai_error(id: &str) -> AutomationError {
    AutomationError::BadRequest(format!(
        "automation command `{id}` can start paid cloud AI generation with the API keys saved in the app and is disabled; \
         start `local-image-cli mcp --bridge` with --allow-cloud-ai to permit it (an app started with automation roots \
         keeps it closed to automation)"
    ))
}

/// [`authorize_engine_command`] as the function pointer [`photocraft_engine::Session::authorize`]
/// stores. `actions.play` calls it for every nested step.
pub fn authorize_engine_step(id: &str, params: &Value) -> photocraft_engine::Result<()> {
    authorize_engine_command(id, params).map_err(|e| photocraft_engine::EngineError::Other(e.to_string()))
}

/// [`authorize_desktop_engine_command`] as a [`photocraft_engine::Session::authorize`] hook.
pub fn authorize_desktop_engine_step(id: &str, params: &Value) -> photocraft_engine::Result<()> {
    authorize_desktop_engine_command(id, params).map_err(|e| photocraft_engine::EngineError::Other(e.to_string()))
}

fn params_contain_ambient_path(id: &str, params: &Value) -> bool {
    let keys: &[&str] = match id {
        "image.adjustments.colorLookup" | "layer.newAdjustmentLayer.colorLookup" | "layer.setAdjustment" => &["file"],
        "filter.distort.displace" => &["mapPath"],
        "layer.quickExportAsPng" | "layer.exportAs" | "image.applyDataSet" => &["path"],
        "image.mode.rgb" | "image.mode.grayscale" | "image.mode.cmyk" | "image.mode.lab" => &["profile"],
        "edit.assignProfile" | "edit.convertToProfile" | "edit.profileInfo" | "view.proofSetup" | "view.gamutWarning" => &["profile"],
        "edit.colorSettings" => &["workingRgb", "workingCmyk", "workingGray"],
        _ => &[],
    };
    keys.iter().any(|key| {
        params
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|value| if matches!(*key, "file" | "mapPath" | "path") { !value.is_empty() } else { looks_like_path(value) })
    })
}

fn profile_command_may_read_ambient(id: &str, params: &Value) -> bool {
    if matches!(id, "color.profileMismatch" | "edit.colorSettings" | "view.proofSetup") {
        return true;
    }
    matches!(id, "edit.assignProfile" | "edit.convertToProfile" | "edit.profileInfo")
        && params.get("profile").and_then(Value::as_str).is_some_and(|profile| {
            matches!(profile, "working" | "default" | "working-cmyk" | "workingCmyk" | "working-rgb" | "workingRgb" | "working-gray" | "workingGray")
        })
}

fn preferences_may_grant_ambient_paths(id: &str, params: &Value) -> bool {
    if id != "prefs.set" {
        return false;
    }
    let direct = params.get("path").and_then(Value::as_str).is_some_and(preference_uses_ambient_filesystem);
    let batch = params.get("values").and_then(Value::as_object).is_some_and(|values| values.keys().any(|path| preference_uses_ambient_filesystem(path)));
    direct || batch
}

fn preference_uses_ambient_filesystem(path: &str) -> bool {
    // The engine skips empty segments (`.scriptEvents.enabled` sets `scriptEvents.enabled`), so
    // judge the first non-empty one; no segment at all is the whole-preferences update.
    let Some(section) = path.split('.').find(|segment| !segment.is_empty()) else { return true };
    matches!(section, "colorSettings" | "scriptEvents" | "historyLog" | "plugIns" | "scratchDisks")
}

fn command_uses_ambient_path(id: &str, params: &Value) -> bool {
    match id {
        "brush.presets.importAbr" | "gradient.presets.importGrd" | "plugin.install" => {
            // `data` wins over `path` in these commands; any `path` without it reads the filesystem.
            params.get("data").is_none() && params.get("path").is_some()
        }
        "plugin.reload" => true,
        _ => false,
    }
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.to_ascii_lowercase().ends_with(".icc")
        || value.to_ascii_lowercase().ends_with(".icm")
}

fn command_error(id: &str) -> AutomationError {
    AutomationError::BadRequest(format!("automation command `{id}` uses ambient filesystem paths and is disabled; use capability-scoped document methods"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(name: &str) -> (PathBuf, PathBuf, AuthorizedWorkspace) {
        let base = std::env::temp_dir().join(format!("photocraft-workspace-{}-{name}", std::process::id()));
        let inside = base.join("inside");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let workspace = AuthorizedWorkspace::new(Some(&inside), Some(&inside)).unwrap();
        (inside, outside, workspace)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn capability_replacement_keeps_the_posix_access_acl() {
        use std::os::unix::fs::MetadataExt;
        let (inside, outside, workspace) = roots("access-acl");
        let target = inside.join("access.txt");
        std::fs::write(&target, b"original").unwrap();
        let uid = std::fs::metadata(&target).unwrap().uid();
        assert!(std::process::Command::new("setfacl").args(["-m", &format!("u:{uid}:r,g::---,m::r,o::---"), "--"]).arg(&target).status().unwrap().success());
        let before = std::process::Command::new("getfacl").args(["--omit-header", "--numeric", "--"]).arg(&target).output().unwrap();
        assert!(before.status.success());
        workspace.write("access.txt", b"replacement").unwrap();
        let after = std::process::Command::new("getfacl").args(["--omit-header", "--numeric", "--"]).arg(&target).output().unwrap();
        assert!(after.status.success());
        assert_eq!(before.stdout, after.stdout);
        assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
        drop(workspace);
        std::fs::remove_dir_all(inside.parent().unwrap()).unwrap();
        assert!(!outside.exists());
    }

    #[test]
    fn capability_shared_writer_exclusive_staging_and_publication_failure_cleanup() {
        let (inside, outside, workspace) = roots("atomic");
        let root = workspace.write.as_ref().unwrap();
        let fs = CapFs { dir: &root.dir };
        let reserved = Path::new("reserved.tmp");
        fs.write_new(reserved, b"reserved canary").unwrap();
        assert_eq!(fs.write_new(reserved, b"collision").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(inside.join(reserved)).unwrap(), b"reserved canary");
        fs.remove(reserved).unwrap();
        workspace.write("out.psd", b"original").unwrap();
        workspace.write("out.psd", b"replacement").unwrap();
        assert_eq!(workspace.read("out.psd").unwrap(), b"replacement");
        std::fs::create_dir(inside.join("taken")).unwrap();
        assert!(workspace.write("taken", b"collision").is_err());
        assert!(inside.join("taken").is_dir());
        assert!(temp_files(&inside).is_empty());
        assert!(workspace.write("missing/out.psd", b"missing parent").is_err());
        assert!(!inside.join("missing").exists());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir_all(inside.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn capability_shared_writer_rejects_escape_and_uses_held_root_after_path_changes() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (inside, outside, workspace) = roots("capability");
        let canary = outside.join("secret.txt");
        std::fs::write(&canary, b"outside canary").unwrap();
        symlink(&outside, inside.join("escape")).unwrap();
        assert!(workspace.write("escape/secret.txt", b"blocked").is_err());
        assert_eq!(std::fs::read(&canary).unwrap(), b"outside canary");
        assert!(temp_files(&outside).is_empty());
        workspace.write("mode.psd", b"private original").unwrap();
        std::fs::set_permissions(inside.join("mode.psd"), std::fs::Permissions::from_mode(0o600)).unwrap();
        workspace.write("mode.psd", b"private replacement").unwrap();
        assert_eq!(std::fs::metadata(inside.join("mode.psd")).unwrap().permissions().mode() & 0o777, 0o600);
        // Swapping the configured root's pathname cannot redirect the already-held capability.
        let moved = outside.join("moved-root");
        std::fs::rename(&inside, &moved).unwrap();
        std::fs::create_dir(&inside).unwrap();
        workspace.write("held.psd", b"held capability").unwrap();
        assert_eq!(std::fs::read(moved.join("held.psd")).unwrap(), b"held capability");
        assert!(!inside.join("held.psd").exists());
        assert!(temp_files(&moved).is_empty());
        assert_eq!(std::fs::read(&canary).unwrap(), b"outside canary");
        std::fs::remove_dir_all(inside.parent().unwrap()).unwrap();
    }

    #[test]
    fn valid_read_and_nonexistent_output_write_stay_in_root() {
        let (inside, _, workspace) = roots("valid");
        std::fs::write(inside.join("read.txt"), b"canary").unwrap();
        assert_eq!(workspace.read("read.txt").unwrap(), b"canary");
        workspace.write("new-output.txt", b"created").unwrap();
        assert_eq!(std::fs::read(inside.join("new-output.txt")).unwrap(), b"created");
    }

    fn temp_files(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".tmp")).collect()
    }

    #[test]
    fn write_replaces_atomically_and_leaves_no_temp_file() {
        let (inside, _, workspace) = roots("atomic");
        std::fs::create_dir_all(inside.join("sub")).unwrap();
        workspace.write("sub/out.psd", b"original").unwrap();
        workspace.write("sub/out.psd", b"replacement").unwrap();
        assert_eq!(std::fs::read(inside.join("sub/out.psd")).unwrap(), b"replacement");
        assert!(temp_files(&inside.join("sub")).is_empty());
        // Renaming over a directory fails: nothing is left behind and the directory survives.
        std::fs::create_dir_all(inside.join("adir")).unwrap();
        assert!(workspace.write("adir", b"x").is_err());
        assert!(inside.join("adir").is_dir());
        assert!(temp_files(&inside).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn write_into_read_only_folder_keeps_the_original() {
        use std::os::unix::fs::PermissionsExt;
        let (inside, _, workspace) = roots("readonly");
        let ro = inside.join("ro");
        std::fs::create_dir_all(&ro).unwrap();
        std::fs::write(ro.join("doc.psd"), b"original").unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let root_user = std::fs::File::create(ro.join("probe")).is_ok();
        if !root_user {
            assert!(workspace.write("ro/doc.psd", b"new").is_err());
            assert_eq!(std::fs::read(ro.join("doc.psd")).unwrap(), b"original");
            assert!(temp_files(&ro).is_empty());
        }
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn rejects_absolute_traversal_mixed_prefix_and_malformed_paths_without_panicking() {
        let (_, _, workspace) = roots("reject");
        for path in [
            "",
            "/absolute",
            "../escape",
            "a/../escape",
            "a\\..\\escape",
            "a\\b",
            "C:/escape",
            "a//b",
            "./a",
            "name:stream",
            "NUL",
            "con.txt",
            "folder/COM1.log",
        ] {
            assert!(workspace.read(path).is_err(), "read accepted {path:?}");
            assert!(workspace.write(path, b"x").is_err(), "write accepted {path:?}");
        }
    }

    #[test]
    fn read_and_write_authority_are_separate() {
        let (inside, _, _) = roots("separate");
        std::fs::write(inside.join("read.txt"), b"canary").unwrap();
        let read_only = AuthorizedWorkspace::new(Some(&inside), None).unwrap();
        assert_eq!(read_only.read("read.txt").unwrap(), b"canary");
        assert!(read_only.write("blocked.txt", b"x").is_err());
        let write_only = AuthorizedWorkspace::new(None, Some(&inside)).unwrap();
        assert!(write_only.read("read.txt").is_err());
        write_only.write("written.txt", b"ok").unwrap();
    }

    #[test]
    fn missing_parent_is_a_stable_error_before_any_file_is_created() {
        let (inside, _, workspace) = roots("missing-parent");
        let error = workspace.write("missing/output.txt", b"x").unwrap_err().to_string();
        assert!(error.contains("automation write"), "{error}");
        assert!(!inside.join("missing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let (inside, outside, workspace) = roots("symlink");
        std::fs::write(outside.join("secret.txt"), b"outside").unwrap();
        symlink(&outside, inside.join("link")).unwrap();
        assert!(workspace.read("link/secret.txt").is_err());
        assert!(workspace.write("link/new.txt", b"blocked").is_err());
        assert!(!outside.join("new.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_symlink_escape_is_rejected_when_supported() {
        use std::os::windows::fs::symlink_dir;

        let (inside, outside, workspace) = roots("windows-link");
        std::fs::write(outside.join("secret.txt"), b"outside").unwrap();
        if symlink_dir(&outside, inside.join("link")).is_err() {
            return;
        }
        assert!(workspace.read("link/secret.txt").is_err());
        assert!(workspace.write("link/new.txt", b"blocked").is_err());
        assert!(!outside.join("new.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_escape_is_rejected_when_supported() {
        use std::process::Command;

        let (inside, outside, workspace) = roots("windows-junction");
        std::fs::write(outside.join("secret.txt"), b"outside").unwrap();
        let link = inside.join("junction");
        let status = Command::new("cmd.exe").args(["/D", "/C", "mklink", "/J"]).arg(&link).arg(&outside).status();
        if !status.is_ok_and(|status| status.success()) {
            return;
        }
        assert!(workspace.read("junction/secret.txt").is_err());
        assert!(workspace.write("junction/new.txt", b"blocked").is_err());
        assert!(!outside.join("new.txt").exists());
        let _ = std::fs::remove_dir(link);
    }

    #[test]
    fn filesystem_commands_fail_closed() {
        for id in [
            "file.open",
            "file.openAs",
            "file.save",
            "file.saveAs",
            "file.saveACopy",
            "file.export.saveForWebLegacy",
            "pattern.import",
            "layer.smartObjects.exportContents",
            "measurementLog.export",
            "layer.videoLayers.reloadFrame",
            "edit.colorSettings",
        ] {
            assert!(authorize_engine_command(id, &serde_json::json!({})).is_err());
        }
        assert!(authorize_engine_command("file.new", &serde_json::json!({})).is_ok());
        // The UI-level examples in docs/control-protocol.md.
        for id in ["view.zoomIn", "window.theme.pro", "edit.search"] {
            assert!(authorize_desktop_engine_command(id, &serde_json::json!({})).is_ok(), "{id}");
        }
        assert!(authorize_engine_command("image.mode.cmyk", &serde_json::json!({})).is_ok());
        assert!(authorize_desktop_engine_command("image.mode.cmyk", &serde_json::json!({})).is_err());
        assert!(authorize_engine_command("image.mode.rgb", &serde_json::json!({"profile": "/outside/profile.icc"})).is_err());
        assert!(authorize_engine_command("image.mode.rgb", &serde_json::json!({"profile": "srgb"})).is_ok());
        assert!(authorize_engine_command("filter.distort.displace", &serde_json::json!({"mapPath": "outside.png"})).is_err());
        assert!(authorize_engine_command("layer.setAdjustment", &serde_json::json!({"file": "outside.cube"})).is_err());
        assert!(authorize_engine_command("prefs.set", &serde_json::json!({"path": "colorSettings.workingRgb", "value": "outside.icc"})).is_err());
        assert!(authorize_engine_step("file.open", &serde_json::json!({})).is_err());
        assert!(authorize_engine_step("actions.play", &serde_json::json!({})).is_ok());
        // An allowed command can't reach a denied one by running it on its own behalf.
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", serde_json::json!({"width": 4, "height": 4})).unwrap();
        session.authorize = Some(authorize_desktop_engine_step);
        let params = serde_json::json!({"to": "grayscale"});
        assert!(authorize_desktop_engine_command("file.automate.conditionalModeChange", &params).is_ok());
        assert!(session.execute("file.automate.conditionalModeChange", params).is_err());
        assert_eq!(session.active().unwrap().doc.mode, photocraft_engine::doc::ColorMode::Rgb);
        assert!(authorize_desktop_engine_step("file.saveACopy", &serde_json::json!({})).is_err());
    }

    #[test]
    fn apply_data_set_is_judged_by_the_values_it_applies() {
        let mut headless = crate::Headless::new();
        headless.command_run("file.new", serde_json::json!({"width": 8, "height": 8})).unwrap();
        let layer = headless.command_run("layer.new.layer", serde_json::json!({})).unwrap()["layer"].clone();
        let defs = [
            serde_json::json!({"name": "shown", "layer": layer, "type": "visibility"}),
            serde_json::json!({"name": "photo", "layer": layer, "type": "pixelReplacement"}),
        ];
        headless.command_run("image.variables.define", serde_json::json!({"defs": defs})).unwrap();
        let sets = serde_json::json!({"dataSets": [
            {"name": "hidden", "values": [{"variable": "shown", "kind": "visibility", "value": false}]},
            {"name": "swap", "values": [
                {"variable": "shown", "kind": "visibility", "value": false},
                {"variable": "photo", "kind": "pixels", "value": "/outside/photo.png"},
            ]},
        ]});
        headless.command_run("image.variables.dataSets", sets).unwrap();
        let visible = |headless: &crate::Headless| headless.session.active().unwrap().doc.layers.iter().all(|l| l.visible);
        let refused = headless.command_run("image.applyDataSet", serde_json::json!({"name": "swap"})).unwrap_err();
        assert!(refused.to_string().contains("ambient filesystem paths"), "{refused}");
        assert!(visible(&headless), "a refused data set applies none of its values");
        assert!(headless.command_run("image.applyDataSet", serde_json::json!({"name": "hidden"})).is_ok());
        assert!(!visible(&headless));
    }

    #[test]
    fn automation_rejects_ambient_path_commands_and_preferences() {
        for (id, params) in [
            ("brush.presets.importAbr", serde_json::json!({"path": "/outside/set.abr"})),
            ("gradient.presets.importGrd", serde_json::json!({"path": "/outside/set.grd"})),
            ("plugin.install", serde_json::json!({"path": "/outside/plugin.wasm"})),
            ("plugin.reload", serde_json::json!({"path": "/outside/plugins"})),
            ("plugin.reload", serde_json::json!({})),
            ("plugin.install", serde_json::json!({"path": " "})),
        ] {
            assert!(authorize_engine_command(id, &params).is_err(), "{id}: {params}");
        }
        for (id, params) in [
            ("brush.presets.importAbr", serde_json::json!({"data": "QUJD"})),
            ("gradient.presets.importGrd", serde_json::json!({"data": "QUJD"})),
            ("plugin.install", serde_json::json!({"data": "QUJD"})),
        ] {
            assert!(authorize_engine_command(id, &params).is_ok(), "{id}: {params}");
        }
        for path in [
            "",
            "colorSettings",
            "colorSettings.workingRgb",
            "scriptEvents",
            "scriptEvents.enabled",
            ".scriptEvents",
            ".scriptEvents.enabled",
            "historyLog.filePath",
            ".historyLog.filePath",
            "plugIns.additionalPluginsFolder",
            "scratchDisks.disks",
            ".",
            "..historyLog.filePath",
        ] {
            assert!(authorize_engine_command("prefs.set", &serde_json::json!({"path": path, "value": {}})).is_err(), "{path}");
        }
        assert!(
            authorize_engine_command("prefs.set", &serde_json::json!({"values": {"interface.language": "fr", "historyLog.filePath": "/outside/log"}})).is_err()
        );
        assert!(authorize_engine_command("prefs.set", &serde_json::json!({"path": "interface.language", "value": "fr"})).is_ok());
    }

    #[test]
    fn registry_filesystem_path_params_are_classified() {
        let probe = serde_json::json!({
            "path": "/outside/photocraft-probe",
            "file": "/outside/photocraft-probe.icc",
            "mapPath": "/outside/photocraft-probe.png",
            "profile": "/outside/photocraft-probe.icc",
            "workingRgb": "/outside/photocraft-probe.icc",
            "workingCmyk": "/outside/photocraft-probe.icc",
            "workingGray": "/outside/photocraft-probe.icc",
            "input": "/outside/photocraft-input",
            "output": "/outside/photocraft-output",
            "paths": ["/outside/photocraft-probe.psd"],
        });
        let preference_probe = serde_json::json!({"path": ".scriptEvents", "value": {}});
        let unclassified: Vec<_> = photocraft_engine::command_specs()
            .iter()
            .filter(|spec| documents_filesystem_path_params(spec.id, spec.params))
            .filter(|spec| {
                let params = if spec.id == "prefs.set" { &preference_probe } else { &probe };
                authorize_engine_command(spec.id, params).is_ok()
            })
            .map(|spec| spec.id)
            .collect();
        assert!(unclassified.is_empty(), "filesystem path parameters in the command registry need an automation policy: {unclassified:?}");
    }

    fn documents_filesystem_path_params(id: &str, params: &str) -> bool {
        if matches!(id, "prefs.get" | "prefs.reset") {
            return false;
        }
        // These registry descriptions refer to document vector paths, not host files.
        if matches!(
            id,
            "filter.blurGallery.pathBlur"
                | "filter.render.flame"
                | "shape.create"
                | "shape.edit"
                | "shape.info"
                | "shape.applyPaths"
                | "path.penCommit"
                | "path.pencil.commit"
                | "path.corner.set"
                | "path.corner.preview"
                | "vector.snap.query"
                | "type.path.commit"
                | "layer.vectorize"
                | "shape.presets.list"
                | "shape.presets.new"
                | "path.list"
                | "path.stroke"
                | "path.info"
                | "path.set"
                | "path.transform"
                | "path.moveAnchors"
                | "path.moveHandle"
                | "path.bendSegment"
                | "path.convertPoint"
                | "path.splitAt"
                | "path.clippingPath.set"
                | "path.rename"
                | "select.toWorkPath"
                | "layer.vectorMask.add"
                | "layer.vectorMask.edit"
                | "layer.vectorMask.info"
                | "layer.vectorize.preview"
                | "paint.symmetryFromPath"
                | "edit.defineCustomShape"
                | "layer.combineShapes.unite"
                | "layer.combineShapes.subtractFrontShape"
                | "layer.combineShapes.intersectShapeAreas"
                | "layer.combineShapes.excludeOverlappingShapes"
                | "layer.combineShapes.mergeShapeComponents"
        ) {
            return false;
        }
        params.to_ascii_lowercase().contains("path")
    }

    #[test]
    fn whole_preferences_update_cannot_enable_automation_script_events() {
        let mut headless = crate::headless::Headless::new();
        let result = headless.command_run(
            "prefs.set",
            serde_json::json!({
                "path": "",
                "value": {
                    "scriptEvents": {
                        "enabled": true,
                        "bindings": [{
                            "event": "newDocument",
                            "steps": [["file.saveACopy", {"path": "/outside/canary.psd"}]]
                        }]
                    }
                }
            }),
        );
        assert!(result.is_err());

        for (id, params) in [
            ("brush.presets.importAbr", serde_json::json!({"path": "/outside/set.abr"})),
            ("gradient.presets.importGrd", serde_json::json!({"path": "/outside/set.grd"})),
            ("plugin.install", serde_json::json!({"path": "/outside/plugin.wasm"})),
            ("plugin.reload", serde_json::json!({"path": "/outside/plugins"})),
            ("prefs.set", serde_json::json!({"path": "historyLog.filePath", "value": "/outside/log"})),
            ("prefs.set", serde_json::json!({"values": {"interface.language": "fr", "historyLog.filePath": "/outside/log"}})),
        ] {
            assert!(headless.command_run(id, params).is_err(), "{id}");
        }
        headless.command_run("file.new", serde_json::json!({"width": 5, "height": 5})).unwrap();
        assert!(headless.session.file_menu.event_log.is_empty());
    }

    #[test]
    fn desktop_automation_never_reaches_credentials_or_cloud_generation() {
        use serde_json::json;
        for (id, params) in [
            ("prefs.get", json!({"path": "ai.openai_api_key"})),
            ("prefs.set", json!({"path": "ai.stability_api_key", "value": "sk-test"})),
            ("prefs.set", json!({"path": "ai", "value": {"google_api_key": "k"}})),
            ("prefs.set", json!({"values": {"ai.server.authToken": "t"}})),
            ("prefs.reset", json!({"path": "ai"})),
            ("prefs.reset", json!({})),
            ("li.localAi", json!({})),
            ("li.panel.generate", json!({})),
            ("li.newFromPrompt", json!({})),
        ] {
            let error = authorize_desktop_engine_command(id, &params).unwrap_err().to_string();
            assert!(error.contains(id), "{id}: {error}");
        }
        assert!(authorize_cloud_command("li.panel.generate", &json!({}), false).unwrap_err().to_string().contains("--allow-cloud-ai"));
        assert!(authorize_cloud_command("li.panel.generate", &json!({}), true).is_ok());
        assert!(authorize_cloud_command("prefs.get", &json!({"path": "ai.bfl_api_key"}), true).is_err(), "credentials have no opt-in");
        for (id, params) in [
            ("prefs.get", json!({"path": "interface.language"})),
            ("prefs.get", json!({"path": "ai"})),
            ("prefs.set", json!({"path": "interface.language", "value": "fr"})),
            ("prefs.reset", json!({"path": "interface"})),
            ("ai.generativeFill", json!({"prompt": "a red balloon"})),
            ("layer.new.layer", json!({"name": "token"})),
        ] {
            assert!(authorize_desktop_engine_command(id, &params).is_ok(), "{id} {params}");
        }
    }
}
