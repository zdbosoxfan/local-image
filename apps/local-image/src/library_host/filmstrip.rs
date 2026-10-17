use super::*;

/// Folder-only files use the same renderer/MediaCache backend without adding records to the
/// user's Library. Catalog photos share the Library's actual texture (including its display CMS).
#[derive(Default)]
pub(super) struct FilmstripPreviews {
    pub(super) folder: Option<LightcraftApp>,
    pub(super) entries: std::collections::HashMap<PathBuf, FolderPreview>,
    next_id: u64,
    defaults: Option<lightcraft_engine::import::ImportDefaults>,
    presets: Vec<lightcraft_engine::develop::Preset>,
    xmp: Option<lightcraft_engine::sidecar::XmpPrefs>,
    epoch: Option<u64>,
    index_revision: Option<(String, u64, u64)>,
    photos: std::collections::HashMap<PathBuf, lightcraft_catalog::PhotoId>,
    catalog_keys: std::collections::HashMap<(lightcraft_catalog::PhotoId, usize), u64>,
    resolved: std::collections::HashMap<PathBuf, Option<lightcraft_catalog::PhotoId>>,
}
pub(super) type FileStamp = Option<(u64, std::time::SystemTime)>;
pub(super) type SourceFingerprint = (FileStamp, FileStamp);

pub(super) struct FolderPreview {
    pub(super) id: lightcraft_catalog::PhotoId,
    revision: u64,
    fingerprint: SourceFingerprint,
    checked: std::time::Instant,
    key: u64,
    edge: usize,
}

impl FilmstripPreviews {
    fn folder(&mut self, library: &LightcraftApp) -> &mut LightcraftApp {
        self.folder.get_or_insert_with(|| {
            let mut session = lightcraft_engine::Session::new().with_fs();
            if library.session.media.rendered.disk().is_some() {
                session.media.rendered = library.session.media.rendered.clone();
            } else if let Some(dir) = config_dir() {
                session.media.attach_disk_cache(&dir.join("cache").join("filmstrip"), lightcraft_engine::media::DISK_CACHE_BYTES);
            }
            LightcraftApp::new(session, Services::default())
        })
    }

    pub(super) fn service(
        &mut self,
        ctx: &egui::Context,
        library: &mut LightcraftApp,
        requests: &[photocraft_ui_egui::filmstrip_ui::PreviewRequest],
        display: Option<Arc<photocraft_engine::display_color::CanvasDisplay>>,
    ) {
        use lightcraft_ui_egui::render::Slot;
        use photocraft_ui_egui::filmstrip_ui::accept_preview;
        // Library may never have shown a frame; its normal background tick then returns early.
        if library.renderer.poll(ctx, &mut library.session) {
            ctx.request_repaint();
        }
        let identity = (library.session.library_identity().to_string(), library.session.catalog.revision, library.session.active().map_or(0, |id| id.0));
        if self.index_revision.as_ref() != Some(&identity) {
            self.photos.clear();
            self.catalog_keys.clear();
            self.resolved.clear();
            // Prefer the master; when the active virtual copy is in view, use its current look.
            for p in library.session.catalog.photos().filter(|p| !p.deleted && p.copy_of.is_none()) {
                if let lightcraft_catalog::Source::File { path } = &p.source {
                    self.photos.insert(PathBuf::from(path), p.id);
                }
            }
            if let Some(p) = library.session.active().and_then(|id| library.session.catalog.photo(id))
                && !p.deleted
                && let lightcraft_catalog::Source::File { path } = &p.source
            {
                self.photos.insert(PathBuf::from(path), p.id);
            }
            self.index_revision = Some(identity);
        }
        let epoch = requests.first().map(|r| r.epoch);
        let changed_defaults = self.defaults.as_ref() != Some(&library.session.import_defaults)
            || self.presets != library.session.presets
            || self.xmp != Some(library.session.xmp);
        if self.epoch != epoch || changed_defaults || (self.next_id >= 128 && requests.iter().any(|r| !self.entries.contains_key(&r.source))) {
            if let Some(folder) = &mut self.folder {
                folder.renderer.forget_all();
            }
            self.entries.clear();
            self.next_id = 0;
            self.epoch = epoch;
            self.defaults = Some(library.session.import_defaults.clone());
            self.presets = library.session.presets.clone();
            self.xmp = Some(library.session.xmp);
        }
        let sources: std::collections::HashSet<_> = requests.iter().map(|r| r.source.clone()).collect();
        self.entries.retain(|path, _| sources.contains(path));
        self.resolved.retain(|path, _| sources.contains(path));
        let mut folder_keep = std::collections::HashSet::new();
        let mut catalog_keep = std::collections::HashSet::new();
        for request in requests {
            // Paths are normally absolute from Open Folder / import. Canonicalize only on a
            // catalog index miss, so ordinary settled frames don't traverse the file system.
            let photo = *self.resolved.entry(request.source.clone()).or_insert_with(|| {
                self.photos.get(&request.source).copied().or_else(|| {
                    let source = std::fs::canonicalize(&request.source).ok()?;
                    self.photos.iter().find(|(p, _)| std::fs::canonicalize(p).ok().as_ref() == Some(&source)).map(|(_, id)| *id)
                })
            });
            if let Some(id) = photo {
                catalog_keep.insert(id);
                lightcraft_ui_egui::panels::grid::request_thumb(library, id, request.edge, request.priority.min(11));
                let expected = self.catalog_keys.entry((id, request.edge)).or_insert_with(|| {
                    let Some(job) = library.session.thumb_job(id, request.edge) else { return 0 };
                    let mut key = job.key;
                    if let Some(interaction) = library.session.interaction.as_ref().filter(|i| i.photo == id) {
                        key ^= job.settings.hash64() ^ interaction.original.hash64();
                    }
                    key
                });
                let texture = library.renderer.thumb(id).filter(|tex| tex.key == *expected).map(|tex| tex.tex.clone());
                accept_preview(ctx, request, texture);
                continue;
            }
            if !self.entries.contains_key(&request.source) {
                self.next_id = self.next_id.wrapping_add(1);
                self.entries.insert(
                    request.source.clone(),
                    FolderPreview {
                        id: lightcraft_catalog::PhotoId(self.next_id),
                        revision: request.revision,
                        fingerprint: source_fingerprint(&request.source, library.session.xmp.naming),
                        checked: std::time::Instant::now(),
                        key: self.next_id,
                        edge: request.edge,
                    },
                );
            }
            let Some(entry) = self.entries.get_mut(&request.source) else { continue };
            let fingerprint = if entry.checked.elapsed() >= std::time::Duration::from_secs(1) || entry.revision != request.revision {
                entry.checked = std::time::Instant::now();
                source_fingerprint(&request.source, library.session.xmp.naming)
            } else {
                entry.fingerprint
            };
            if entry.fingerprint != fingerprint || entry.revision != request.revision || entry.edge != request.edge {
                entry.key = entry.key.wrapping_add(1);
                entry.fingerprint = fingerprint;
                entry.revision = request.revision;
                entry.edge = request.edge;
            }
            let (id, key) = (entry.id, entry.key);
            folder_keep.insert(id);
            let folder = self.folder(library);
            if library.session.media.rendered.disk().is_some() {
                folder.session.media.rendered = library.session.media.rendered.clone();
            }
            if let Some(display) = &display {
                folder.renderer.set_display(display.clone());
            }
            let slot = Slot::Thumb(id);
            if request.source.extension().is_some_and(|ext| photocraft_codecs::raw_from_extension(&ext.to_string_lossy()))
                && !folder.renderer.textures.get(&slot).is_some_and(|tex| tex.key == key)
            {
                folder.renderer.request_quick(
                    Slot::ThumbQuick(id),
                    lightcraft_engine::media::QuickJob {
                        request_id: 0,
                        photo: id,
                        key,
                        cached: Vec::new(),
                        embedded: Some((request.source.to_string_lossy().into_owned(), lightcraft_engine::files::fs_preview_loader(), request.edge)),
                        small: None,
                    },
                    request.priority.min(11).saturating_add(1).min(11),
                );
            }
            if !folder.renderer.is_current(slot, key) && !folder.renderer.is_pending(slot) {
                let defaults = library.session.import_defaults.clone();
                let presets = library.session.presets.clone();
                let xmp = library.session.xmp;
                let cache = folder.session.media.rendered.clone();
                let generation = cache.generation();
                let loader = Arc::new(move |path: &str, edge| {
                    lightcraft_engine::memory::in_background(|| {
                        let mut session = lightcraft_engine::Session::new().with_fs();
                        session.media.rendered = cache.clone();
                        session.import_defaults = defaults.clone();
                        session.presets = presets.clone();
                        session.xmp = lightcraft_engine::sidecar::XmpPrefs { auto_write: false, ..xmp };
                        // All reads, hashing, metadata and decoding stay on the existing job worker.
                        let mut job = folder_thumb_job(&mut session, path, edge)?;
                        job.cache_generation = generation;
                        job.run().rendered.ok().map(|r| r.image)
                    })
                });
                let job = lightcraft_engine::media::QuickJob {
                    request_id: 0,
                    photo: id,
                    key,
                    cached: Vec::new(),
                    embedded: Some((request.source.to_string_lossy().into_owned(), loader, request.edge)),
                    small: None,
                };
                folder.renderer.request_quick(slot, job, request.priority.saturating_sub(1).min(10));
            }
            // Never hand out a pre-save texture while the new source is being decoded.
            let texture = [Slot::Thumb(id), Slot::ThumbQuick(id)]
                .iter()
                .find_map(|slot| folder.renderer.textures.get(slot).filter(|tex| tex.key == key).map(|tex| tex.tex.clone()));
            accept_preview(ctx, request, texture);
        }
        self.catalog_keys.retain(|(id, _), _| catalog_keep.contains(id));
        // Match the Library grid's existing 600-texture limit; preserve its other cached
        // thumbnails until that limit is exceeded, rather than flushing on each module change.
        library.renderer.evict_thumbs(&catalog_keep, 600);
        if let Some(folder) = &mut self.folder {
            folder.renderer.evict_thumbs(&folder_keep, folder_keep.len());
            if folder.renderer.poll(ctx, &mut folder.session) {
                ctx.request_repaint();
            }
        }
        if library.renderer.in_flight() > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

/// Probe/hash through the existing import reader in an isolated in-memory session. This reads
/// sidecar settings too, but has no persistent catalog and performs no file copies or writes.
pub(super) fn folder_thumb_job(session: &mut lightcraft_engine::Session, path: &str, edge: usize) -> Option<lightcraft_engine::media::RenderJob> {
    let report = lightcraft_engine::import::import_with(session, &[path.to_string()], &Default::default()).ok()?;
    let photo = report.imported.first().copied().map(lightcraft_catalog::PhotoId)?;
    session.thumb_job(photo, edge)
}

pub(super) fn source_fingerprint(path: &std::path::Path, naming: lightcraft_engine::sidecar::SidecarNaming) -> SourceFingerprint {
    let sidecar = lightcraft_engine::sidecar::find_sidecar(&path.to_string_lossy(), naming);
    (file_fingerprint(path), sidecar.as_deref().and_then(file_fingerprint))
}

pub(super) fn file_fingerprint(path: &std::path::Path) -> FileStamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}
