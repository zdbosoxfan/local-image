use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum LibraryUpdate {
    /// The Library already had the file: it was re-read (thumbnail and previews follow).
    Reloaded(lightcraft_catalog::PhotoId),
    /// Imported and stacked on top of `original`, the photo the document came from; selected.
    Added { id: lightcraft_catalog::PhotoId, original: lightcraft_catalog::PhotoId },
}

/// The editor's document that shows Library photo `id`: the one saved to its file, else one whose
/// Develop layer follows it.
pub(super) fn source_binding(s: &lightcraft_engine::Session, photo: u64) -> Option<(String, String)> {
    let photo = s.catalog.photo(lightcraft_catalog::PhotoId(photo))?;
    let lightcraft_catalog::Source::File { path } = &photo.source else { return None };
    Some((s.library_identity().to_string(), path.clone()))
}

pub(super) fn matches_origin(s: &lightcraft_engine::Session, origin: &photocraft_ui_egui::develop_layer::LibraryOrigin) -> bool {
    origin.library.as_deref() == Some(s.library_identity())
        && s.catalog
            .photo(lightcraft_catalog::PhotoId(origin.photo))
            .is_some_and(|photo| matches!(&photo.source, lightcraft_catalog::Source::File { path } if *path == origin.path))
}

pub(super) fn document_in_library(doc: &photocraft_doc::Document, s: &lightcraft_engine::Session) -> bool {
    let origins: Vec<_> = doc.walk().iter().filter_map(|(_, _, l)| photocraft_ui_egui::develop_layer::layer_origin(l)).collect();
    origins.is_empty() || origins.iter().any(|o| o.library.as_deref() == Some(s.library_identity()))
}

pub(super) fn document_of(editor: &PhotocraftApp, s: &lightcraft_engine::Session, id: lightcraft_catalog::PhotoId) -> Option<usize> {
    editor.session.documents().iter().position(|d| {
        document_in_library(&d.doc, s)
            && (d.path.as_deref().and_then(|p| photo_at_path(s, p)) == Some(id)
                || d.doc
                    .walk()
                    .iter()
                    .filter_map(|(_, _, l)| photocraft_ui_egui::develop_layer::layer_origin(l))
                    .any(|o| o.photo == id.0 && matches_origin(s, &o)))
    })
}

/// The Library photos Compositing has unsaved work on: (photo, version key, document). A document
/// counts once it has unsaved changes (a one-layer document can have a mask, filter or retouching);
/// it stands for the
/// photo whose file it was saved to (a `-Edit.psd` in the Library), else the photo its Develop
/// layer follows.
pub(super) fn unsaved_composites(
    editor: &PhotocraftApp,
    s: &lightcraft_engine::Session,
) -> Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> {
    let mut out: Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> = Vec::new();
    for d in editor.session.documents() {
        if !d.is_dirty() || !document_in_library(&d.doc, s) {
            continue;
        }
        let photo = d.path.as_deref().and_then(|p| photo_at_path(s, p)).or_else(|| {
            d.doc
                .walk()
                .iter()
                .filter_map(|(_, _, l)| photocraft_ui_egui::develop_layer::layer_origin(l))
                .find(|origin| matches_origin(s, origin))
                .map(|origin| lightcraft_catalog::PhotoId(origin.photo))
        });
        let Some(photo) = photo.filter(|p| !out.iter().any(|o| o.0 == *p)) else { continue };
        let key = (Arc::as_ptr(&d.doc) as usize as u64) ^ d.revision.rotate_left(32);
        out.push((photo, key, d.doc.clone()));
    }
    out
}

/// The Library photo whose file is `path` (a master: virtual copies share its file).
pub(super) fn photo_at_path(s: &lightcraft_engine::Session, path: &str) -> Option<lightcraft_catalog::PhotoId> {
    // Runs every Library frame for each unsaved Compositing document (`unsaved_composites`):
    // borrow the paths, no String per catalog photo (85,000 allocations a frame on big libraries).
    fn file(p: &lightcraft_catalog::Photo) -> Option<&str> {
        match &p.source {
            lightcraft_catalog::Source::File { path } if p.copy_of.is_none() => Some(path.as_str()),
            _ => None,
        }
    }
    if let Some(p) = s.catalog.photos().find(|p| file(p) == Some(path)) {
        return Some(p.id);
    }
    // The same file by another spelling (a symlinked folder, `..`): compare only same-named files.
    let canonical = std::fs::canonicalize(path).ok()?;
    let name = std::path::Path::new(path).file_name()?;
    s.catalog
        .photos()
        .filter_map(|p| file(p).map(|f| (p.id, f)))
        .find(|(_, f)| std::path::Path::new(f).file_name() == Some(name) && std::fs::canonicalize(f).ok().as_ref() == Some(&canonical))
        .map(|(id, _)| id)
}

pub(super) fn capture_save_source(
    s: &lightcraft_engine::Session,
    origin: &photocraft_ui_egui::develop_layer::LibraryOrigin,
) -> Option<photocraft_ui_egui::develop_layer::LibrarySaveSource> {
    if !matches_origin(s, origin) {
        return None;
    }
    let photo = s.catalog.photo(lightcraft_catalog::PhotoId(origin.photo))?;
    let master = photo.copy_of.unwrap_or(photo.id);
    let canonical_path = std::fs::canonicalize(&origin.path).ok()?;
    if s.catalog.photo(master)?.copy_of.is_some() {
        return None;
    }
    if !s
        .catalog
        .photos()
        .filter(|p| p.id == master || p.copy_of == Some(master))
        .all(|p| matches!(&p.source,lightcraft_catalog::Source::File{path} if std::fs::canonicalize(path).ok().as_ref() == Some(&canonical_path)))
    {
        return None;
    }
    let source_paths = s
        .catalog
        .photos()
        .filter(|p| p.id == master || p.copy_of == Some(master))
        .filter_map(|p| if let lightcraft_catalog::Source::File { path } = &p.source { Some(path.clone()) } else { None })
        .collect();
    let settings = s
        .catalog
        .photos()
        .filter(|p| p.id == master || p.copy_of == Some(master))
        .map(|p| serde_json::to_value(&p.develop).ok().map(|settings| (p.id.0, settings)))
        .collect::<Option<Vec<_>>>()?;
    Some(photocraft_ui_egui::develop_layer::LibrarySaveSource { origin: origin.clone(), master: master.0, canonical_path, source_paths, settings })
}

pub(super) fn add_replaced_source(
    s: &mut lightcraft_engine::Session,
    path: &str,
    reset: &photocraft_ui_egui::develop_layer::LibrarySaveSource,
) -> Result<Option<LibraryUpdate>, String> {
    if capture_save_source(s, &reset.origin).as_ref() != Some(reset) || !photocraft_ui_egui::develop_layer::same_file_path(path, &reset.origin.path) {
        return Err("Source identity or Develop edits changed; the saved file exists, but the newer Library edits were preserved".into());
    }
    let result = s.execute("photo.reload", &serde_json::json!({"ids":[reset.master],"resetDevelop":true})).map_err(|e| e.to_string())?;
    if !result["reloaded"].as_array().is_some_and(|ids| ids.iter().any(|id| id.as_u64() == Some(reset.master)))
        || !reset.settings.iter().all(|(id, _)| s.catalog.photo(lightcraft_catalog::PhotoId(*id)).is_some_and(|photo| *photo.develop == Default::default()))
    {
        return Err("The file was saved, but the Library could not reload it and reset its Develop settings".into());
    }
    Ok(Some(LibraryUpdate::Reloaded(lightcraft_catalog::PhotoId(reset.master))))
}

/// The Library's half of saving in Compositing: a file it has is reloaded; a file saved from a
/// document that came from Library photo `original` is imported, stacked on top of the original
/// (expanded, as Edit in External Editor does) and selected. Anything else is left alone (`None`).
pub(super) fn add_saved_file(
    s: &mut lightcraft_engine::Session,
    path: &str,
    original: Option<u64>,
    overwritten: bool,
) -> Result<Option<LibraryUpdate>, String> {
    if let Some(id) = photo_at_path(s, path) {
        s.execute("photo.reload", &serde_json::json!({ "ids": [id.0], "resetDevelop": overwritten })).map_err(|e| e.to_string())?;
        return Ok(Some(LibraryUpdate::Reloaded(id)));
    }
    let Some(original) = original.map(lightcraft_catalog::PhotoId).filter(|id| s.catalog.photo(*id).is_some()) else { return Ok(None) };
    let source = s.catalog.photo(original).ok_or("the source photo is missing")?;
    let options = lightcraft_engine::import::ImportOptions {
        inherited_organization: Some(lightcraft_engine::import::InheritedOrganization {
            rating: source.rating,
            flag: source.flag,
            label: source.label,
            keywords: source.meta.keywords.clone(),
        }),
        ..Default::default()
    };
    // Keep the command boundary's durable save and automatic XMP writes, just
    // as ordinary and background imports do; metadata is already in AddPhoto.
    let report = s
        .execute_fn("library.import", |session| {
            let report = lightcraft_engine::import::import_with(session, &[path.to_string()], &options)?;
            serde_json::to_value(report).map_err(|error| lightcraft_engine::EngineError::Other(error.to_string()))
        })
        .map_err(|error| error.to_string())?;
    let Some(id) = report["imported"].get(0).and_then(serde_json::Value::as_u64).map(lightcraft_catalog::PhotoId) else {
        let why = report["failed"][0][1]
            .as_str()
            .or_else(|| report["duplicates"].as_array().filter(|items| !items.is_empty()).map(|_| "the Library already has the same picture"));
        return Err(why.unwrap_or("not a photo it reads").to_string());
    };
    s.execute("stack.group", &serde_json::json!({ "ids": [id.0, original.0], "top": id.0, "collapsed": false })).map_err(|e| e.to_string())?;
    s.selection = lightcraft_engine::Selection::single(id);
    Ok(Some(LibraryUpdate::Added { id, original }))
}

/// Brings the editor's document saved at `path` forward; `false` when none is open.
pub(super) fn focus_open_document(editor: &mut PhotocraftApp, path: &str) -> bool {
    let same = |p: &str| p == path || std::fs::canonicalize(p).ok().zip(std::fs::canonicalize(path).ok()).is_some_and(|(a, b)| a == b);
    match editor.session.documents().iter().position(|d| d.path.as_deref().is_some_and(same)) {
        Some(i) => {
            editor.session.set_active(i);
            true
        }
        None => false,
    }
}
