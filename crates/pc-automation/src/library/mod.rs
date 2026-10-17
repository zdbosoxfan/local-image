//! The Library (photo catalog and raw developer) for automation.
//!
//! [`LibraryHeadless`] hosts an in-process [`lightcraft_engine::Session`] and answers the
//! control-channel methods the Library tools use (`engine.execute`, `engine.commands`,
//! `ui.render`, `app.export`), so agents and the CLI can import, develop, render and export
//! photos without a window. [`tools`] holds the Library's MCP tools, which
//! [`crate::PhotocraftMcp`] serves as `library_*` next to the editor's tools.
//!
//! One filesystem policy for both halves of the app:
//!
//! * **Denied** (an MCP server started without automation roots): no file is read or written.
//! * **Workspace** (`--automation-read-root` / `--automation-write-root`): request paths are
//!   forward-slash relative paths below those roots. Photo bytes are read and exports, renders and
//!   saved images are written only through the [`AuthorizedWorkspace`] directory capabilities;
//!   the catalog names files by their path below the read root. Library commands are
//!   deny-by-default: only commands that never touch the filesystem run
//!   ([`library_command_allowed`]); import, export and the write-target check are mapped to the
//!   capabilities. Those that cannot be undone ([`library_command_destructive`]) also need
//!   `--allow-destructive`.
//! * **Trusted local** (the CLI's own arguments): host paths, every command.

pub mod tools;

#[cfg(test)]
mod tests;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use li_raster::Rgba8;
use lightcraft_engine::Session;
use lightcraft_engine::catalog::PhotoId;
use serde_json::{Value, json};

use crate::AuthorizedWorkspace;
use crate::workspace::relative_path;

/// File extensions recognised as photos when expanding folders.
pub use lightcraft_codecs::PHOTO_EXTENSIONS;

/// Default long edge of a Library export when no size parameter is given (as the desktop app's
/// Export Now); `longEdge: 0` exports full size.
pub const DEFAULT_EXPORT_LONG_EDGE: u32 = 3000;

/// Where the Library session may read and write files.
#[derive(Clone)]
pub(crate) enum LibraryFs {
    Denied,
    TrustedLocal,
    Workspace(Arc<AuthorizedWorkspace>),
}

/// A headless Library session.
pub struct LibraryHeadless {
    pub session: Session,
    fs: LibraryFs,
    /// Untrusted sessions run [`library_command_destructive`] commands only when this is set
    /// (`local-image-cli mcp --allow-destructive`).
    allow_destructive: bool,
}

impl Drop for LibraryHeadless {
    /// Snapshot a persistent library on exit (a no-op for in-memory sessions).
    fn drop(&mut self) {
        let _ = self.session.close_library();
    }
}

impl Default for LibraryHeadless {
    fn default() -> Self {
        Self::new()
    }
}

const DENIED: &str = "automation filesystem access is not granted";

impl LibraryHeadless {
    /// An in-memory Library with no filesystem authority.
    pub fn new() -> Self {
        Self::with_policy(LibraryFs::Denied)
    }

    /// A Library for an explicit local CLI invocation: host paths, every command.
    pub fn trusted_local() -> Self {
        Self::with_policy(LibraryFs::TrustedLocal)
    }

    /// A trusted Library around an existing session (tests, the demo library). The caller
    /// installs the session's file hooks.
    pub fn from_session(session: Session) -> Self {
        Self { session, fs: LibraryFs::TrustedLocal, allow_destructive: false }
    }

    /// An in-memory Library whose files are read and written only through `workspace`.
    pub fn with_workspace(workspace: AuthorizedWorkspace) -> Self {
        Self::with_policy(LibraryFs::Workspace(Arc::new(workspace)))
    }

    pub(crate) fn with_policy(fs: LibraryFs) -> Self {
        let mut session = Session::new();
        match &fs {
            LibraryFs::Denied => {}
            LibraryFs::TrustedLocal => session = session.with_fs(),
            LibraryFs::Workspace(ws) => install_workspace_hooks(&mut session, ws),
        }
        Self { session, fs, allow_destructive: false }
    }

    pub(crate) fn policy(&self) -> LibraryFs {
        self.fs.clone()
    }

    /// Let an untrusted session run the commands that cannot be undone
    /// ([`library_command_destructive`]). Trusted sessions always run them.
    pub fn set_allow_destructive(&mut self, allow: bool) {
        self.allow_destructive = allow;
    }

    /// Whether this session takes host paths and runs every command (the CLI).
    pub fn is_trusted(&self) -> bool {
        matches!(self.fs, LibraryFs::TrustedLocal)
    }

    /// Open (or create) the persistent library in `dir` (trusted sessions only). Edits are saved
    /// as they are made; the library is closed (snapshotted) when this value is dropped.
    pub fn open_library(&mut self, dir: &Path) -> Result<Value, String> {
        if !self.is_trusted() {
            return Err("opening a library folder needs a local (trusted) session".into());
        }
        let r = self.session.open_library(dir, false).map_err(|e| e.to_string())?;
        Ok(json!({"library": dir.to_string_lossy(), "replayed": r.replayed, "failed": r.failed, "photos": self.session.catalog.photos().count()}))
    }

    fn photo_or_active(&self, p: &Value) -> Result<PhotoId, String> {
        p.get("id").and_then(Value::as_u64).map(PhotoId).or(self.session.active()).ok_or_else(|| "no photo (import one or pass `id`)".to_string())
    }

    /// Render `id` (or the active photo) so its long edge is at most `size` pixels (`longEdge` is
    /// accepted too), clamped to `16..=max_size`.
    pub fn render(&mut self, p: &Value, default_size: u64, max_size: u64) -> Result<Rgba8, String> {
        let id = self.photo_or_active(p)?;
        let size = p.get("size").or(p.get("longEdge")).and_then(Value::as_u64).unwrap_or(default_size).clamp(16, max_size.max(16)) as usize;
        Ok(self.session.render_now(id, size, size)?.image)
    }

    /// Answer one control-channel method. Results name workspace files by their relative path.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let p = if params.is_null() { json!({}) } else { params };
        let r = match method {
            "engine.execute" | "ui.menu.invoke" | "command" => {
                let id = p.get("command").or(p.get("id")).and_then(Value::as_str).ok_or("missing `command`")?.to_string();
                let params = p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
                self.execute(&id, &params)
            }
            "engine.commands" => Ok(self.command_list()),
            "ui.render" => self.render_to(&p),
            "app.export" => self.export(&p),
            m if m.starts_with("ui.") || m == "app.quit" => {
                Err(format!("`{m}` drives the desktop window; the Library tools run headless (use library_render_photo to see a photo)"))
            }
            other => Err(format!("unknown method `{other}`")),
        };
        r.map(|v| self.display(v))
    }

    /// Run one Library command under this session's filesystem policy.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value, String> {
        if id == "app.export" {
            return self.export(params);
        }
        if !self.is_trusted() {
            match id {
                "library.import" => return self.import(params),
                "export.checkTarget" => {
                    let path = params.get("path").and_then(Value::as_str).ok_or("missing `path`")?;
                    let target = self.write_target(path)?;
                    return self.session.check_write_target(&target.host).map(|()| json!({"path": path}));
                }
                _ if !library_command_allowed(id) => {
                    return Err(format!(
                        "automation command `{id}` reads or writes files outside the automation roots and is disabled; use library_import, library_export and library_render_photo for files"
                    ));
                }
                _ if library_command_destructive(id) && !self.allow_destructive => {
                    return Err(format!(
                        "automation command `{id}` cannot be undone and is disabled; start the MCP server with --allow-destructive to permit it (photo.delete moves photos to Recently Deleted and stays available)"
                    ));
                }
                _ => {}
            }
        }
        self.session.execute(id, params).map_err(|e| e.to_string())
    }

    /// The command registry with `app.export`; an untrusted session lists only what it runs.
    pub fn command_list(&self) -> Value {
        let trusted = self.is_trusted();
        let mut v: Vec<Value> = self
            .session
            .commands()
            .into_iter()
            .filter(|c| trusted || library_command_allowed(c.id) || matches!(c.id, "library.import" | "export.checkTarget"))
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        v.push(json!({"id": "app.export", "label": "Export Now", "menu": [], "shortcut": null,
            "params": "{path?: output file (.jpg/.png/.tif/.webp/.avif/.dng) | dir?, ids?, format?: jpeg|png|tiff|webp|avif|dng|original, longEdge?|shortEdge?|width?|height?|megapixels?|percent? (default longEdge 3000; longEdge 0 = full size), dontEnlarge?, ppi?, quality?, limitKb?, colorSpace?, bitDepth?, sharpen?, metadata?, watermark?, naming?}",
            "enabled": self.session.active().is_some()}));
        Value::Array(v)
    }

    /// Import files and folders (`paths`, a string or a list). Folders are scanned recursively
    /// for photos; explicit files are kept whatever their extension (the probe decides). Trusted:
    /// host paths (relative to the working directory), every import mode. Workspace: paths below
    /// the read root, added in place. Returns the engine's result plus `files` (how many were
    /// offered).
    pub fn import(&mut self, args: &Value) -> Result<Value, String> {
        let paths: Vec<String> = match args.get("paths") {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::String(p)) => vec![p.clone()],
            _ => return Err("missing `paths`".into()),
        };
        let files = match &self.fs {
            LibraryFs::TrustedLocal => expand_paths(&paths),
            LibraryFs::Denied => return Err(format!("{DENIED}: read authority is absent")),
            LibraryFs::Workspace(ws) => {
                if let Some(mode) = args.get("mode").and_then(Value::as_str).filter(|m| *m != "add") {
                    return Err(format!(
                        "import mode `{mode}` copies or moves originals on disk, which automation cannot do; import adds files in place (mode `add`)"
                    ));
                }
                let root = ws.read_root_path().ok_or_else(|| format!("{DENIED}: read authority is absent"))?.to_path_buf();
                let mut files = Vec::new();
                for p in &paths {
                    let listed = ws.list_files(p).map_err(|e| e.to_string())?;
                    let single = listed.len() == 1 && listed[0] == *p;
                    for rel in listed {
                        if single || is_photo_file(&rel) {
                            files.push(root.join(&rel).to_string_lossy().into_owned());
                        }
                    }
                }
                files
            }
        };
        if files.is_empty() {
            return Err(format!("no photos found in {paths:?}"));
        }
        let mut p = json!({"paths": files});
        let keys: &[&str] = if self.is_trusted() { &["album", "mode", "destination", "organize", "rename"] } else { &["album"] };
        for k in keys {
            if let Some(v) = args.get(*k).filter(|v| !v.is_null()) {
                p[*k] = v.clone();
            }
        }
        let mut r = self.session.execute("library.import", &p).map_err(|e| e.to_string())?;
        if let Some(o) = r.as_object_mut() {
            o.insert("files".into(), json!(files.len()));
        }
        Ok(r)
    }

    /// `ui.render`: render a photo; with `path`, also write it there (format from the extension).
    fn render_to(&mut self, p: &Value) -> Result<Value, String> {
        let img = self.render(p, 1600, 16384)?;
        match p.get("path").and_then(Value::as_str) {
            Some(path) => {
                let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                let bytes = encode_image(&ext, &img, 92)?;
                self.write_file(path, &bytes)?;
                Ok(json!({"path": path, "width": img.width, "height": img.height}))
            }
            None => Ok(json!({"width": img.width, "height": img.height})),
        }
    }

    /// Write `bytes` to a request path, never over a photo's original (or its XMP sidecar).
    pub fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let target = self.write_target(path)?;
        self.session.check_write_target(&target.host)?;
        match (&self.fs, target.relative) {
            (LibraryFs::Workspace(ws), Some(rel)) => ws.write(&rel, bytes).map_err(|e| e.to_string()),
            _ => lightcraft_engine::export::write_file(&target.host, bytes),
        }
    }

    /// Resolve a request path for writing: the host path the engine sees, and for a workspace the
    /// relative path the capability writes.
    fn write_target(&self, path: &str) -> Result<WriteTarget, String> {
        match &self.fs {
            LibraryFs::TrustedLocal => Ok(WriteTarget { host: path.to_string(), relative: None }),
            LibraryFs::Denied => Err(format!("{DENIED}: write authority is absent")),
            LibraryFs::Workspace(ws) => {
                let root = ws.write_root_path().ok_or_else(|| format!("{DENIED}: write authority is absent"))?;
                let rel = relative_path(path).map_err(|e| e.to_string())?;
                Ok(WriteTarget { host: root.join(&rel).to_string_lossy().into_owned(), relative: Some(path.to_string()) })
            }
        }
    }

    /// The UI command `app.export`, emulated with the same parameters as the desktop app
    /// (see `lightcraft_engine::export::ExportOptions::from_json`, plus `ids`, `dir`, `path`).
    /// With `path` and no `format`, the format follows the path's extension.
    pub fn export(&mut self, p: &Value) -> Result<Value, String> {
        use lightcraft_engine::export::{Destination, ExportFormat, ExportOptions, Resize, export_batch};
        let p = &self.session.export_params(p)?;
        let mut opts = ExportOptions::from_json(p);
        if !ExportOptions::has_size_param(p) {
            opts.resize = Some(Resize::long_edge(DEFAULT_EXPORT_LONG_EDGE));
        }
        let exact = p.get("path").and_then(Value::as_str);
        if let (Some(path), None) = (exact, p.get("format")) {
            let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            opts.format = ExportFormat::parse(&ext).unwrap_or(ExportFormat::Png);
        }
        let ids: Vec<PhotoId> = match p.get("ids").and_then(Value::as_array) {
            Some(a) => a.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect(),
            // like the desktop app: the selection (in grid order), else the given / active photo
            None if p.get("id").is_none() && self.session.selection.ids.len() > 1 => {
                let sel: std::collections::HashSet<_> = self.session.selection.ids.iter().copied().collect();
                self.session.visible_cloned().into_iter().filter(|id| sel.contains(id)).collect()
            }
            None => vec![self.photo_or_active(p)?],
        };
        let dir = p.get("dir").and_then(Value::as_str).unwrap_or("");
        let files = match self.fs.clone() {
            LibraryFs::TrustedLocal => {
                let to = Destination { dir: dir.to_string(), exact: exact.map(str::to_string) };
                export_batch(&mut self.session, &ids, &opts, &to, &mut lightcraft_engine::export::write_file, &|path| Path::new(path).exists())?
            }
            LibraryFs::Denied => return Err(format!("{DENIED}: write authority is absent")),
            LibraryFs::Workspace(ws) => {
                let root = ws.write_root_path().ok_or_else(|| format!("{DENIED}: write authority is absent"))?.to_path_buf();
                let host = |rel: &str| -> Result<String, String> {
                    if rel.is_empty() || rel == "." {
                        return Ok(root.to_string_lossy().into_owned());
                    }
                    Ok(root.join(relative_path(rel).map_err(|e| e.to_string())?).to_string_lossy().into_owned())
                };
                let to = Destination { dir: host(dir)?, exact: exact.map(host).transpose()? };
                let w = Arc::clone(&ws);
                let r = root.clone();
                let mut write = move |path: &str, bytes: &[u8]| -> Result<(), String> {
                    let rel = relative_to(&r, path).ok_or_else(|| format!("export target `{path}` is outside the automation write root"))?;
                    w.write(&rel, bytes).map_err(|e| e.to_string())
                };
                let exists = |path: &str| relative_to(&root, path).is_some_and(|rel| ws.write_target_exists(&rel));
                export_batch(&mut self.session, &ids, &opts, &to, &mut write, &exists)?
            }
        };
        // Single-photo exports also report path/width/height at the top level.
        let mut out = files.first().cloned().unwrap_or_else(|| json!({}));
        out["files"] = json!(files);
        Ok(out)
    }

    /// Workspace sessions name files relative to their roots in results (the host paths of the
    /// roots stay private to the server).
    pub(crate) fn display(&self, v: Value) -> Value {
        let LibraryFs::Workspace(ws) = &self.fs else { return v };
        let roots: Vec<PathBuf> = [ws.read_root_path(), ws.write_root_path()].into_iter().flatten().map(Path::to_path_buf).collect();
        relativize(v, &roots)
    }
}

struct WriteTarget {
    /// The path the engine sees (for the original-file guard and trusted writes).
    host: String,
    /// The capability-relative path a workspace writes.
    relative: Option<String>,
}

/// `path` below `root` as a forward-slash relative path (plain components only).
fn relative_to(root: &Path, path: &str) -> Option<String> {
    let rest = Path::new(path).strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rest.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str()?.to_string()),
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn relativize(v: Value, roots: &[PathBuf]) -> Value {
    match v {
        Value::String(s) => Value::String(roots.iter().find_map(|r| relative_to(r, &s)).unwrap_or(s)),
        Value::Array(a) => Value::Array(a.into_iter().map(|x| relativize(x, roots)).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (k, relativize(x, roots))).collect()),
        other => other,
    }
}

fn is_photo_file(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| lightcraft_codecs::is_readable_extension(&e.to_string_lossy(), true))
}

/// Photo bytes for a catalog path, read through the workspace's read capability.
fn read_through(ws: &AuthorizedWorkspace, path: &str) -> Result<Vec<u8>, String> {
    let root = ws.read_root_path().ok_or_else(|| format!("{DENIED}: read authority is absent"))?;
    let rel = relative_to(root, path).ok_or_else(|| format!("{path}: outside the automation read root"))?;
    ws.read(&rel).map_err(|e| e.to_string())
}

/// The Library's file hooks, reading only through the workspace capability (the desktop's
/// `Session::with_fs` reads host paths). Decodes wait on the engine's shared work gate as the
/// desktop's hooks do.
fn install_workspace_hooks(session: &mut Session, ws: &Arc<AuthorizedWorkspace>) {
    use lightcraft_engine::files::{embedded_preview_srgb, load_vec, load_vec_with, probe_bytes};
    use lightcraft_engine::media::SourceLevel;
    use lightcraft_engine::memory;
    fn weight(len: usize, max_edge: usize) -> usize {
        len * if max_edge <= SourceLevel::Thumb.max_edge() { 3 } else { 6 }
    }
    fn gated<T>(len: usize, max_edge: usize, f: impl FnOnce() -> T) -> T {
        let gate = memory::work_gate();
        let w = weight(len, max_edge);
        let _permit = if memory::is_background() { gate.acquire(w) } else { gate.acquire_urgent(w) };
        let r = f();
        memory::release();
        r
    }
    let w = Arc::clone(ws);
    session.media.file_loader = Some(Arc::new(move |path: &str, max_edge: usize| {
        let bytes = read_through(&w, path)?;
        gated(bytes.len(), max_edge, || load_vec(bytes, max_edge))
    }));
    let w = Arc::clone(ws);
    session.media.file_loader_with = Some(Arc::new(move |path: &str, max_edge: usize, opts: &lightcraft_engine::files::RawOptions| {
        let bytes = read_through(&w, path)?;
        gated(bytes.len(), max_edge, || load_vec_with(bytes, max_edge, opts))
    }));
    let w = Arc::clone(ws);
    session.media.file_probe = Some(Arc::new(move |path: &str| {
        let bytes = read_through(&w, path)?;
        let _permit = memory::work_gate().acquire(bytes.len());
        probe_bytes(path, &bytes)
    }));
    let w = Arc::clone(ws);
    session.media.preview_loader = Some(Arc::new(move |path: &str, max_edge: usize| embedded_preview_srgb(&read_through(&w, path).ok()?, max_edge)));
    let w = Arc::clone(ws);
    session.media.file_bytes = Some(Arc::new(move |path: &str| read_through(&w, path)));
}

/// Library commands an untrusted (automation) session may run: they change only the in-memory
/// catalog, develop settings and selection, or report state. Everything else — reading or
/// writing files by path, copying/moving originals, sidecars, presets files, merges that write
/// new files, model downloads — is denied. Deny-by-default, so a new command stays off until it
/// is reviewed and listed here. Import, export and the write-target check are served through the
/// workspace capabilities instead ([`LibraryHeadless::execute`]).
pub fn library_command_allowed(id: &str) -> bool {
    const FAMILIES: &[&str] =
        &["develop.", "crop.", "geometry.", "mask.", "spot.", "redeye.", "beforeAfter.", "pointColor.", "stack.", "version.", "history.", "album.", "label."];
    const EXACT: &[&str] = &[
        "edit.undo",
        "edit.redo",
        "catalog.query",
        "catalog.stats",
        "library.state",
        "library.info",
        "library.source",
        "library.filter",
        "library.clearFilter",
        "library.sort",
        "library.shuffle",
        "library.groups",
        "library.select",
        "library.selectAll",
        "library.selectBy",
        "library.selectNone",
        "library.next",
        "library.previous",
        "library.findSimilar",
        "library.memory",
        "app.gpu",
        "app.memoryBudget",
        "journal.list",
        "albums.list",
        "photo.inspect",
        "photo.allMetadata",
        "photo.rate",
        "photo.flag",
        "photo.pick",
        "photo.reject",
        "photo.unflag",
        "photo.label",
        "photo.rotateLeft",
        "photo.rotateRight",
        "photo.flipHorizontal",
        "photo.flipVertical",
        "photo.setMeta",
        "photo.setRegion",
        "photo.removeRegion",
        "photo.copyMetadata",
        "photo.pasteMetadata",
        "photo.virtualCopy",
        "photo.setCaptureTime",
        "photo.analyze",
        "photo.delete",
        "photo.restore",
        "photo.deletePermanently",
        "photo.renameTokens",
        "photo.renamePreview",
        "presets.list",
        "preset.apply",
        "preset.create",
        "preset.delete",
        "preset.favorite",
        "preset.rename",
        "preset.move",
        "preset.update",
        "profiles.list",
        "profiles.menu",
        "profile.favorite",
        "curve.reset",
        "curve.presets",
        "curve.applyPreset",
        "curve.savePreset",
        "curve.deletePreset",
        "filter.presets",
        "filter.savePreset",
        "filter.applyPreset",
        "filter.deletePreset",
        "metadata.presets",
        "metadata.savePreset",
        "metadata.deletePreset",
        "metadata.applyPreset",
        "export.presets",
        "export.savePreset",
        "export.deletePreset",
        "keyword.sets",
        "keyword.useSet",
        "keyword.saveSet",
        "keyword.deleteSet",
        "keyword.toggleFromSet",
        "keyword.list",
        "keyword.suggest",
        "keyword.rename",
        "keyword.delete",
        "keyword.merge",
        "skinTone.pick",
        "segment.prepare",
        "segment.model.status",
        "enhance.status",
        "enhance.jobs",
        "enhance.cancel",
    ];
    EXACT.contains(&id) || FAMILIES.iter().any(|f| id.starts_with(f))
}

/// Allowed Library commands whose effect cannot be undone: they bypass Recently Deleted, or
/// remove presets, keywords, keyword and label sets or albums for good. An untrusted session
/// runs them only with [`LibraryHeadless::set_allow_destructive`]; soft, undoable operations
/// (`photo.delete` to Recently Deleted, ratings, develop edits) are not listed.
pub fn library_command_destructive(id: &str) -> bool {
    matches!(
        id,
        "photo.deletePermanently"
            | "album.delete"
            | "keyword.delete"
            | "keyword.merge"
            | "keyword.deleteSet"
            | "label.deleteSet"
            | "preset.delete"
            | "curve.deletePreset"
            | "filter.deletePreset"
            | "metadata.deletePreset"
            | "export.deletePreset"
    )
}

/// Expand folders (recursively, sorted) into photo files and make paths absolute (host paths;
/// trusted sessions only).
pub fn expand_paths(paths: &[String]) -> Vec<String> {
    fn walk(p: &Path, out: &mut Vec<String>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
                v.sort();
                for c in v {
                    if c.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')) {
                        walk(&c, out);
                    }
                }
            }
        } else if p.extension().is_some_and(|e| lightcraft_codecs::is_readable_extension(&e.to_string_lossy(), true)) {
            out.push(std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_string());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let path = Path::new(p);
        if path.is_dir() {
            walk(path, &mut out);
        } else {
            // Explicit files are kept even with unknown extensions (the probe decides).
            out.push(std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().to_string());
        }
    }
    out
}

/// Encode by extension: `.png` (default), `.jpg`/`.jpeg` (quality), `.tif`/`.tiff`, `.webp` (lossless),
/// `.avif` — via the shared export encoder (embeds an sRGB profile).
pub fn encode_image(ext: &str, img: &Rgba8, quality: u8) -> Result<Vec<u8>, String> {
    use lightcraft_engine::export::{ExportFormat, ExportOptions};
    let format = ExportFormat::parse(ext).unwrap_or(ExportFormat::Png);
    lightcraft_engine::export::encode_image(img, &ExportOptions { format, quality: quality.clamp(1, 100), ..Default::default() })
}
