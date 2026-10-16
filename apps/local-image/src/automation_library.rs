//! Capability-backed control browsing. Staging and subsequent source reads keep the same
//! read capability; the Library never enumerates an ambient request directory.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightcraft_engine::media::{FileLoader, FileLoaderWith, FileProbe, PreviewLoader};
use photocraft_automation::AuthorizedWorkspace;
use photocraft_ui_egui::AutomationBrowse;

#[path = "automation_library_worker.rs"]
mod worker;

fn gated<T>(weight: usize, run: impl FnOnce() -> T) -> T {
    let gate = lightcraft_engine::memory::work_gate();
    let _permit = if lightcraft_engine::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
    let result = run();
    lightcraft_engine::memory::release();
    result
}

fn decode_weight(bytes: usize, edge: usize) -> usize {
    bytes.saturating_mul(if edge <= lightcraft_engine::media::SourceLevel::Thumb.max_edge() { 3 } else { 6 })
}

pub fn stage(workspace: &AuthorizedWorkspace, path: &str) -> Result<AutomationBrowse, String> {
    let files = workspace.list_folder(path).map_err(|error| error.to_string())?;
    let root = workspace.read_root_path().ok_or("automation read authority is absent")?.to_path_buf();
    let folder = if matches!(path, "" | ".") { root.clone() } else { root.join(path) };
    let authority = workspace.clone();
    let aliases = workspace.read_root_aliases().ok_or("automation read authority is absent")?.to_vec();
    let read_roots = aliases.clone();
    let read: photocraft_ui_egui::AutomationSourceRead = Arc::new(move |path| {
        let relative = read_roots.iter().find_map(|root| Path::new(path).strip_prefix(root).ok()).ok_or("outside the automation read root")?;
        let relative = relative.to_str().ok_or("automation paths must be valid UTF-8")?.replace('\\', "/");
        authority.read(&relative).map_err(|error| error.to_string())
    });
    let files = files
        .into_iter()
        .filter(|relative| {
            lightcraft_codecs::is_readable_extension(Path::new(relative).extension().and_then(|extension| extension.to_str()).unwrap_or(""), true)
        })
        .map(|relative| root.join(relative).to_string_lossy().into_owned())
        .collect();
    Ok(AutomationBrowse {
        root: root.to_string_lossy().into_owned(),
        root_aliases: aliases.iter().map(|root| root.to_string_lossy().into_owned()).collect(),
        folder: folder.to_string_lossy().into_owned(),
        files,
        read,
    })
}

/// One retained bundle, reused by repeated browse requests. New Library sessions get fresh
/// wrappers around their native hooks; an already wrapped session is never wrapped again.
pub struct MediaHooks {
    root: PathBuf,
    load: FileLoader,
    worker: Arc<worker::ScanWorker>,
}

fn scoped(path: &str, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| Path::new(path).starts_with(root))
}

fn install_hooks(session: &mut lightcraft_engine::Session, browse: &AutomationBrowse, retained: &mut Option<MediaHooks>) -> Result<(), String> {
    let root = PathBuf::from(&browse.root);
    let aliases: Vec<PathBuf> = browse.root_aliases.iter().map(PathBuf::from).collect();
    if let Some(hooks) = retained {
        if hooks.root != root {
            return Err("The automation read root changed; restart the control session".into());
        }
        if session.media.file_loader.as_ref().is_some_and(|current| Arc::ptr_eq(current, &hooks.load)) {
            return Ok(());
        }
    }
    let worker = match retained.as_ref() {
        Some(hooks) => hooks.worker.clone(),
        None => worker::ScanWorker::new()?,
    };
    let old = session.media.file_loader.clone();
    let (r, root_load) = (browse.read.clone(), aliases.clone());
    let load: FileLoader = Arc::new(move |path, edge| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(decode_weight(bytes.len(), edge), || lightcraft_engine::files::load_vec(bytes, edge))
        } else {
            old.as_ref().ok_or("no native photo reader")?(path, edge)
        }
    });
    let old = session.media.file_loader_with.clone();
    let (r, root_load) = (browse.read.clone(), aliases.clone());
    let load_with: FileLoaderWith = Arc::new(move |path, edge, options| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(decode_weight(bytes.len(), edge), || lightcraft_engine::files::load_vec_with(bytes, edge, options))
        } else {
            old.as_ref().ok_or("no native photo reader")?(path, edge, options)
        }
    });
    let old = session.media.file_probe.clone();
    let (r, root_load) = (browse.read.clone(), aliases.clone());
    let probe: FileProbe = Arc::new(move |path| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(bytes.len(), || lightcraft_engine::files::probe_bytes(path, &bytes))
        } else {
            old.as_ref().ok_or("no native photo probe")?(path)
        }
    });
    let old = session.media.preview_loader.clone();
    let (r, root_load) = (browse.read.clone(), aliases.clone());
    let preview: PreviewLoader = Arc::new(move |path, edge| {
        if scoped(path, &root_load) {
            lightcraft_engine::files::embedded_preview_srgb(&r(path).ok()?, edge)
        } else {
            old.as_ref().and_then(|read| read(path, edge))
        }
    });
    let old = session.media.file_bytes.clone();
    let (r, root_load) = (browse.read.clone(), aliases);
    let bytes: lightcraft_engine::merge::ByteReader = Arc::new(move |path| {
        if scoped(path, &root_load) {
            r(path)
        } else if let Some(read) = &old {
            read(path)
        } else {
            // None is the native fallback contract in merge.rs, not absent authority.
            li_fs::read::file(Path::new(path), li_fs::read::MAX_FILE_BYTES, &lightcraft_sysmem::jobs::Interrupt::NONE).map_err(|error| error.to_string())
        }
    });
    session.media.file_loader = Some(load.clone());
    session.media.file_loader_with = Some(load_with.clone());
    session.media.file_probe = Some(probe.clone());
    session.media.preview_loader = Some(preview.clone());
    session.media.file_bytes = Some(bytes.clone());
    *retained = Some(MediaHooks { root, load, worker });
    Ok(())
}

pub fn show(app: &mut lightcraft_ui_egui::LightcraftApp, browse: AutomationBrowse, retained: &mut Option<MediaHooks>) -> Result<(), String> {
    lightcraft_ui_egui::import::can_browse(app)?;
    install_hooks(&mut app.session, &browse, retained)?;
    let worker = retained.as_ref().ok_or("Library scan worker is unavailable")?.worker.clone();
    lightcraft_ui_egui::import::browse_scoped(app, browse.folder, browse.files, move |job| worker.submit(job))
}

#[cfg(test)]
#[path = "automation_library_tests.rs"]
mod tests;
