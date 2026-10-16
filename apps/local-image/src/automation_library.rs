//! Capability-backed control browsing. Staging and subsequent source reads keep the same
//! read capability; the Library never enumerates an ambient request directory.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightcraft_engine::catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::media::{FileLoader, FileLoaderWith, FileProbe, PreviewLoader};
use photocraft_automation::AuthorizedWorkspace;
use photocraft_ui_egui::AutomationBrowse;

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
    let read_root = root.clone();
    let read: photocraft_ui_egui::AutomationSourceRead = Arc::new(move |path| {
        let relative = Path::new(path).strip_prefix(&read_root).map_err(|_| "outside the automation read root")?;
        let relative = relative.to_str().ok_or("automation paths must be valid UTF-8")?.replace('\\', "/");
        authority.read(&relative).map_err(|error| error.to_string())
    });
    let mut photos = Vec::new();
    for relative in files {
        if !lightcraft_codecs::is_readable_extension(Path::new(&relative).extension().and_then(|extension| extension.to_str()).unwrap_or(""), true) {
            continue;
        }
        let source = root.join(&relative).to_string_lossy().into_owned();
        let bytes = read(&source)?;
        let info = gated(bytes.len(), || lightcraft_engine::files::probe_bytes(&source, &bytes))?;
        let name = Path::new(&relative).file_name().and_then(|name| name.to_str()).ok_or("invalid photo name")?;
        let mut photo = Photo::new(PhotoId(0), Source::File { path: source }, name, &info.format, info.width, info.height, "");
        photo.kind = info.kind;
        photo.file_size = info.file_size;
        photo.captured = info.captured;
        photo.meta = info.meta;
        photo.as_shot_wb = info.as_shot_wb;
        photo.measured_wb = info.measured_wb;
        photo.content_hash = info.content_hash;
        photo.embedded_lens = info.embedded_lens;
        photo.preview_only = info.preview_only;
        photos.push(serde_json::to_value(photo).map_err(|error| error.to_string())?);
    }
    Ok(AutomationBrowse { root: root.to_string_lossy().into_owned(), folder: folder.to_string_lossy().into_owned(), photos, read })
}

/// One retained bundle, reused by repeated browse requests. New Library sessions get fresh
/// wrappers around their native hooks; an already wrapped session is never wrapped again.
pub struct MediaHooks {
    root: PathBuf,
    load: FileLoader,
}

fn scoped(path: &str, root: &Path) -> bool {
    Path::new(path).starts_with(root)
}

fn install_hooks(session: &mut lightcraft_engine::Session, browse: &AutomationBrowse, retained: &mut Option<MediaHooks>) -> Result<(), String> {
    let root = PathBuf::from(&browse.root);
    if let Some(hooks) = retained {
        if hooks.root != root {
            return Err("The automation read root changed; restart the control session".into());
        }
        if session.media.file_loader.as_ref().is_some_and(|current| Arc::ptr_eq(current, &hooks.load)) {
            return Ok(());
        }
    }
    let old = session.media.file_loader.clone();
    let (r, root_load) = (browse.read.clone(), root.clone());
    let load: FileLoader = Arc::new(move |path, edge| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(decode_weight(bytes.len(), edge), || lightcraft_engine::files::load_vec(bytes, edge))
        } else {
            old.as_ref().ok_or("no native photo reader")?(path, edge)
        }
    });
    let old = session.media.file_loader_with.clone();
    let (r, root_load) = (browse.read.clone(), root.clone());
    let load_with: FileLoaderWith = Arc::new(move |path, edge, options| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(decode_weight(bytes.len(), edge), || lightcraft_engine::files::load_vec_with(bytes, edge, options))
        } else {
            old.as_ref().ok_or("no native photo reader")?(path, edge, options)
        }
    });
    let old = session.media.file_probe.clone();
    let (r, root_load) = (browse.read.clone(), root.clone());
    let probe: FileProbe = Arc::new(move |path| {
        if scoped(path, &root_load) {
            let bytes = r(path)?;
            gated(bytes.len(), || lightcraft_engine::files::probe_bytes(path, &bytes))
        } else {
            old.as_ref().ok_or("no native photo probe")?(path)
        }
    });
    let old = session.media.preview_loader.clone();
    let (r, root_load) = (browse.read.clone(), root.clone());
    let preview: PreviewLoader = Arc::new(move |path, edge| {
        if scoped(path, &root_load) {
            lightcraft_engine::files::embedded_preview_srgb(&r(path).ok()?, edge)
        } else {
            old.as_ref().and_then(|read| read(path, edge))
        }
    });
    let old = session.media.file_bytes.clone();
    let (r, root_load) = (browse.read.clone(), root.clone());
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
    *retained = Some(MediaHooks { root, load });
    Ok(())
}

pub fn show(app: &mut lightcraft_ui_egui::LightcraftApp, browse: AutomationBrowse, retained: &mut Option<MediaHooks>) -> Result<(), String> {
    let photos = browse.photos.iter().cloned().map(serde_json::from_value::<Photo>).collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
    install_hooks(&mut app.session, &browse, retained)?;
    let mut existing: HashSet<String> = app
        .session
        .catalog
        .photos()
        .filter_map(|photo| match &photo.source {
            Source::File { path } => Some(path.clone()),
            Source::Demo { .. } => None,
        })
        .collect();
    for mut photo in photos {
        let Source::File { path } = &photo.source else { continue };
        if !existing.insert(path.clone()) {
            continue;
        }
        photo.id = app.session.catalog.alloc_photo_id();
        photo.local = true;
        lightcraft_engine::import::apply_import_defaults(&app.session, &mut photo);
        photo.set_local_baseline();
        app.session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).map_err(|error| error.to_string())?;
    }
    app.session.browse = Some(lightcraft_engine::Browse { path: browse.folder, subfolders: false });
    app.session.source = lightcraft_engine::LibrarySource::Folder;
    app.renderer.forget_imports();
    Ok(())
}

#[cfg(test)]
#[path = "automation_library_tests.rs"]
mod tests;
