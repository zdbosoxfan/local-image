//! The modules, Lightroom-style, in one window: **Library** and **Develop** are LightCraft's photo
//! library (import, grid, loupe, compare, survey, ratings, flags, colour labels, keywords,
//! collections, smart collections, stacks, develop, masks, export); **Compositing** is the
//! PhotoCraft editor (layers, retouching, every AI tool).
//!
//! [`Host`] owns both apps and shows one at a time. The module switch sits in each title bar
//! (⌘⌥1 Library, ⌘⌥2 Develop, ⌘⌥3 Compositing). While a module is hidden its background work
//! goes on (`background_tick`): library imports, exports and saves, and the editor's jobs.
//!
//! Moving between Develop and Compositing carries the photo across without copies: Compositing
//! shows the photo as a **Develop layer** (the original file plus its develop settings, rendered
//! through the same pipeline; see `photocraft_ui_egui::develop_layer`), and the layer follows the
//! photo's develop settings whenever you come back from Develop. Double-clicking a Develop layer
//! goes to Develop on its photo.
//!
//! **Camera Raw Filter** (Filter › Camera Raw Filter… in Compositing) is edited in the real
//! Develop module: the editor asks for a session (`develop_filter.request`: the layer's pixels,
//! settings and name); the host opens the pixels as a temporary photo the Library never saves
//! (`Session::open_ephemeral`), shows Develop with a "Camera Raw Filter · ‹layer› — Cancel / OK"
//! banner (Library-only actions hidden), and on OK hands the settings back
//! (`develop_filter_ui::finish`, one history step) and returns to Compositing; Cancel (or leaving
//! the session with the module switch) changes nothing. Going from Compositing to Develop with a
//! layered document that has no Develop layer asks how to develop it (`on_switch_to_develop`).
//!
//! **Back to the Library** (Photoshop's Edit In from Lightroom): a document that came from a
//! Library photo saves beside the original as `<name>-Edit.psd` (⌘S writes it there; Save As
//! suggests it). Every save in Compositing reaches the host (`PhotocraftApp::library_saves`):
//! a file the Library already has is reloaded (its thumbnail follows); a new file from a Library
//! photo is imported and stacked on top of the original, as Edit in External Editor does
//! ([`add_saved_file`]). File › Save and Return to Library then shows the Library on it.
//!
//! The library opens lazily, the first time Library or Develop is shown.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lightcraft_ui_egui::{LightcraftApp, Services, UiState};
use photocraft_ui_egui::{Module, PhotocraftApp};

/// Files the Library asked the editor to open (Edit in External Editor with no other editor set).
type Opens = Arc<Mutex<Vec<String>>>;

pub struct Host {
    pub editor: PhotocraftApp,
    library: Option<LightcraftApp>,
    prefs: PrefsWriter,
    mode: Module,
    /// The module whose visuals are applied to the context (Library and Develop share LightCraft's).
    styled: Option<bool>,
    opens: Opens,
    frames: u64,
    /// The Library's active photo when Compositing was last left: coming back with the same photo
    /// returns to what was open there instead of bringing the photo forward again.
    left_compositing_with: Option<u64>,
    /// The Camera Raw Filter session open in Develop.
    camera_raw: Option<CameraRawSession>,
    /// Composites of unsaved Compositing documents, rendered for the Library.
    composites: Composites,
}

/// What a Library photo looks like in Compositing, rendered on worker threads for the Library's
/// loupe and grid (`lightcraft_ui_egui::panels::host_composite`).
#[derive(Default)]
struct Composites {
    /// Photo → key of the render in flight.
    pending: std::collections::HashMap<u64, u64>,
    /// Finished renders: (photo, key, image).
    done: Arc<Mutex<Vec<(u64, u64, photocraft_raster::Rgba8Image)>>>,
}

/// Longest side of a composite shown in the Library.
const COMPOSITE_EDGE: u32 = 2048;

/// An open Camera Raw Filter session: the temporary photo and the Library view to restore.
struct CameraRawSession {
    photo: lightcraft_catalog::PhotoId,
    name: String,
    view: lightcraft_ui_egui::state::ViewMode,
    right: lightcraft_ui_egui::state::RightPanel,
    left_panel: bool,
}

/// Where the Camera Raw Filter's temporary images go (emptied at startup and when a session ends).
fn camera_raw_dir() -> PathBuf {
    config_dir().map(|d| d.join("cache").join("camera-raw")).unwrap_or_else(|| std::env::temp_dir().join("local-image-camera-raw"))
}

impl Host {
    pub fn new(editor: PhotocraftApp) -> Self {
        let mut editor = editor;
        editor.host_modes = std::env::var_os("LOCAL_IMAGE_NO_LIBRARY").is_none();
        let mode = if editor.host_modes { last_module() } else { Module::Compositing };
        Self {
            editor,
            library: None,
            prefs: PrefsWriter::default(),
            mode,
            styled: None,
            opens: Arc::default(),
            frames: 0,
            left_compositing_with: None,
            camera_raw: None,
            composites: Composites::default(),
        }
    }

    fn library(&mut self) -> &mut LightcraftApp {
        let opens = self.opens.clone();
        self.library.get_or_insert_with(|| open_library_app(opens, &mut self.prefs))
    }

    /// The Library's active photo: its id, file and develop settings (JSON).
    fn active_photo(&mut self) -> Option<(u64, String, serde_json::Value, String)> {
        let lib = self.library.as_ref()?;
        let id = lib.session.active()?;
        let ph = lib.session.catalog.photo(id)?;
        let path = match &ph.source {
            lightcraft_catalog::Source::File { path } => path.clone(),
            _ => return None,
        };
        let settings = serde_json::to_value(&*ph.develop).ok()?;
        Some((id.0, path, settings, ph.file_name.clone()))
    }

    /// Files saved in Compositing: the Library adds the ones that came from its photos (stacked on
    /// the original) and reloads the ones it has; Save and Return to Library then shows it.
    /// Without `ctx` (at exit) nothing is shown.
    fn take_library_saves(&mut self, ctx: Option<&egui::Context>) {
        for save in std::mem::take(&mut self.editor.library_saves) {
            // An unopened library has none of the files; one opens for a photo from it.
            if self.library.is_none() && (save.photo.is_none() || ctx.is_none()) {
                continue;
            }
            let lib = self.library();
            let (id, msg) = match add_saved_file(&mut lib.session, &save.path, save.photo) {
                Ok(Some(LibraryUpdate::Added { id, original })) => {
                    let name = lib.session.catalog.photo(original).map(|p| p.file_name.clone()).unwrap_or_default();
                    (Some(id), format!("Saved to Library, stacked with {name}"))
                }
                Ok(Some(LibraryUpdate::Reloaded(id))) => {
                    let name = lib.session.catalog.photo(id).map(|p| p.file_name.clone()).unwrap_or_default();
                    (Some(id), format!("Saved {name}; updated in the Library"))
                }
                Ok(None) => (None, String::new()),
                Err(e) => {
                    self.editor.ui.status = format!("Saved, but the Library couldn't add it: {e}");
                    self.editor.ui.status_error = true;
                    continue;
                }
            };
            if !msg.is_empty() {
                self.editor.ui.status = msg.clone();
                self.editor.ui.status_error = false;
            }
            let Some(ctx) = ctx.filter(|_| save.show) else { continue };
            self.switch(ctx, Module::Library);
            if let Some(id) = id {
                let lib = self.library();
                let _ = lib.run("library.select", serde_json::json!({ "ids": [id.0], "active": id.0 }));
                if !msg.is_empty() {
                    lib.toast(ctx, msg);
                }
                // back in Compositing, the saved document is still the one shown
                self.left_compositing_with = Some(id.0);
            }
        }
    }

    /// Keeps the Library showing what Compositing is doing to its photos: each unsaved document
    /// with compositing work ([`unsaved_composites`]) is rendered off this thread and handed to the
    /// Library (loupe and grid show it, marked); saved or closed ones are dropped.
    fn update_composites(&mut self, ctx: &egui::Context) {
        use lightcraft_ui_egui::panels::host_composite;
        let Some(lib) = self.library.as_mut() else { return };
        let want = unsaved_composites(&self.editor, &lib.session);
        let done = std::mem::take(&mut *self.composites.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        for (photo, key, img) in done {
            if want.iter().any(|(id, k, _)| id.0 == photo && *k == key) {
                let label = lightcraft_ui_egui::i18n::tr("Edited in Compositing · unsaved").to_string();
                host_composite::set(lib, ctx, lightcraft_catalog::PhotoId(photo), key, img.width as usize, img.height as usize, &img.pixels, label);
            }
            if self.composites.pending.get(&photo) == Some(&key) {
                self.composites.pending.remove(&photo);
            }
        }
        let ids: Vec<lightcraft_catalog::PhotoId> = want.iter().map(|w| w.0).collect();
        host_composite::retain(lib, &ids);
        self.composites.pending.retain(|p, _| ids.iter().any(|id| id.0 == *p));
        for (id, key, doc) in want {
            if host_composite::key(lib, id) == Some(key) || self.composites.pending.get(&id.0) == Some(&key) {
                continue;
            }
            self.composites.pending.insert(id.0, key);
            let (done, ctx) = (self.composites.done.clone(), ctx.clone());
            std::thread::spawn(move || {
                let img = photocraft_compose::thumbnail(&doc, COMPOSITE_EDGE);
                done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((id.0, key, img));
                ctx.request_repaint();
            });
        }
    }

    /// The Library's "Open in Compositing" on a photo shown as its unsaved composite: back to that
    /// document as it is.
    fn open_composite(&mut self, ctx: &egui::Context) {
        let Some(id) = self.library.as_mut().and_then(|l| l.host_composite_open.take()) else { return };
        let doc = self.library.as_ref().and_then(|lib| document_of(&self.editor, &lib.session, id));
        if let Some(i) = doc {
            self.editor.session.set_active(i);
            self.editor.sync_views();
        }
        let lib = self.library();
        let _ = lib.run("library.select", serde_json::json!({ "ids": [id.0], "active": id.0 }));
        // the photo is the one Compositing shows: switching doesn't open it again
        self.left_compositing_with = Some(id.0);
        self.switch(ctx, Module::Compositing);
    }

    /// Quitting from Library or Develop with unsaved documents: Compositing comes forward (as it
    /// is: the Library's photo isn't opened there) so its unsaved-changes prompt asks about them
    /// (Save / Don't Save / Cancel). `true` when the editor must run this frame.
    fn quit_review(&mut self, ctx: &egui::Context) -> bool {
        if self.mode == Module::Compositing || !ctx.input(|i| i.viewport().close_requested()) || !self.editor.has_unsaved() {
            return false;
        }
        if self.camera_raw.is_some() {
            self.end_camera_raw(ctx, false);
        }
        self.mode = Module::Compositing;
        self.styled = None;
        ctx.request_repaint();
        true
    }

    /// Re-renders Develop layers whose photo's develop settings changed in Develop.
    fn sync_develop_layers(&mut self) {
        let ids = photocraft_ui_egui::develop_layer::linked_photos(&self.editor);
        if ids.is_empty() {
            return;
        }
        let Some(lib) = self.library.as_ref() else { return };
        let updates: Vec<(u64, serde_json::Value)> = ids
            .into_iter()
            .filter_map(|id| lib.session.develop_of(lightcraft_catalog::PhotoId(id)).and_then(|d| serde_json::to_value(&*d).ok()).map(|v| (id, v)))
            .collect();
        for (id, settings) in updates {
            photocraft_ui_egui::develop_layer::sync(&mut self.editor, id, &settings);
        }
    }

    /// Opens the Camera Raw Filter session the editor asked for.
    fn start_camera_raw(&mut self, ctx: &egui::Context) {
        let Some(req) = self.editor.develop_filter.request.take() else { return };
        if self.camera_raw.is_some() {
            let _ = photocraft_ui_egui::develop_filter_ui::finish(&mut self.editor, None);
            return;
        }
        let dir = camera_raw_dir();
        let lib = self.library();
        match lib.session.open_ephemeral(&dir, &req.name, req.width, req.height, &req.rgb, &req.settings) {
            Ok(photo) => {
                use lightcraft_ui_egui::state::{RightPanel, ViewMode};
                let session = CameraRawSession { photo, name: req.name.clone(), view: lib.ui.view, right: lib.ui.right, left_panel: lib.ui.left_panel };
                lib.host_session = Some(lightcraft_ui_egui::panels::host_session::HostSession::new(req.name, req.hidden));
                lib.ui.view = ViewMode::Detail;
                lib.ui.right = RightPanel::Edit;
                self.camera_raw = Some(session);
                self.mode = Module::Develop;
                self.styled = None;
                ctx.request_repaint();
            }
            Err(e) => {
                let _ = photocraft_ui_egui::develop_filter_ui::finish(&mut self.editor, None);
                self.editor.ui.status = format!("Camera Raw Filter: {e}");
                self.editor.ui.status_error = true;
            }
        }
    }

    /// Ends the session: OK applies the settings in Compositing, Cancel changes nothing.
    fn end_camera_raw(&mut self, ctx: &egui::Context, ok: bool) {
        let Some(cr) = self.camera_raw.take() else { return };
        let settings = match self.library.as_mut() {
            Some(lib) => {
                let settings = lib.session.close_ephemeral();
                lib.host_session = None;
                lib.ui.view = cr.view;
                lib.ui.right = cr.right;
                lib.ui.left_panel = cr.left_panel;
                settings
            }
            None => None,
        };
        if let Err(e) = photocraft_ui_egui::develop_filter_ui::finish(&mut self.editor, if ok { settings } else { None }) {
            self.editor.ui.status = format!("Camera Raw Filter: {e}");
            self.editor.ui.status_error = true;
        }
        self.mode = Module::Compositing;
        self.styled = None;
        ctx.request_repaint();
    }

    /// Keeps the session in Develop on its photo, and takes the banner's answer.
    fn camera_raw_tick(&mut self, ctx: &egui::Context) {
        let Some(photo) = self.camera_raw.as_ref().map(|c| c.photo) else { return };
        let Some(lib) = self.library.as_mut() else { return };
        let answer = lib.host_session.as_mut().and_then(|h| h.result.take());
        if lib.session.ephemeral_photo() != Some(photo) {
            // the Library dropped it (another library opened…): nothing to apply
            return self.end_camera_raw(ctx, false);
        }
        if let Some(ok) = answer {
            return self.end_camera_raw(ctx, ok);
        }
        use lightcraft_ui_egui::state::{RightPanel, ViewMode};
        lib.ui.view = ViewMode::Detail;
        if !lib.ui.right.is_edit_tool() || lib.ui.right == RightPanel::Crop {
            lib.ui.right = RightPanel::Edit;
        }
        if lib.session.active() != Some(photo) {
            lib.session.selection = lightcraft_engine::Selection::single(photo);
        }
    }

    fn switch(&mut self, ctx: &egui::Context, to: Module) {
        if self.mode == to || !self.editor.host_modes {
            return;
        }
        // Leaving a Camera Raw Filter session with the module switch cancels it.
        if self.camera_raw.is_some() {
            self.end_camera_raw(ctx, false);
            return;
        }
        let from = self.mode;
        // Compositing → Develop on a layered document with no Develop layer: ask how (or apply
        // the remembered answer); the session or the switch follows.
        if from == Module::Compositing
            && to == Module::Develop
            && photocraft_ui_egui::develop_layer::active_photo(&self.editor).is_none()
            && !photocraft_ui_egui::develop_filter_ui::on_switch_to_develop(&mut self.editor, ctx)
        {
            ctx.request_repaint();
            return;
        }
        if from == Module::Compositing {
            self.left_compositing_with = self.library.as_ref().and_then(|l| l.session.active()).map(|id| id.0);
            // A Develop layer's photo becomes the Library's active photo, so Develop opens on it.
            if let Some(id) = photocraft_ui_egui::develop_layer::active_photo(&self.editor) {
                let lib = self.library();
                let _ = lib.run("library.select", serde_json::json!({ "ids": [id], "active": id }));
                self.left_compositing_with = Some(id);
            }
        }
        self.mode = to;
        remember_module(to);
        match to {
            Module::Library | Module::Develop => {
                let develop = to == Module::Develop;
                let lib = self.library();
                // Back from the editor: pick up edits saved there to files the Library shows.
                let ids = std::mem::take(&mut lib.ui.external_edits);
                if !ids.is_empty()
                    && let Ok(r) = lib.session.execute("photo.reload", &serde_json::json!({"ids": ids}))
                    && r["reloaded"].as_array().is_some_and(|a| !a.is_empty())
                {
                    lib.toast(ctx, "Updated the edits saved in Compositing");
                }
                set_library_view(lib, develop);
            }
            Module::Compositing => {
                self.sync_develop_layers();
                let photo = self.active_photo();
                let changed = photo.as_ref().map(|p| p.0) != self.left_compositing_with || self.editor.session.documents().is_empty();
                if let Some((id, path, settings, name)) = photo.filter(|_| changed || from == Module::Develop)
                    // a file open in Compositing (a saved `-Edit.psd`) comes forward as it is
                    && (from == Module::Develop || !focus_open_document(&mut self.editor, &path))
                    && let Err(e) = photocraft_ui_egui::develop_layer::open_photo(&mut self.editor, id, &path, &settings, &name)
                {
                    self.editor.ui.status = e;
                    self.editor.ui.status_error = true;
                }
            }
        }
        self.styled = None;
        ctx.request_repaint();
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        let library = self.mode != Module::Compositing;
        if self.styled == Some(library) {
            return;
        }
        if library {
            if let Some(l) = &self.library {
                l.restyle(ctx);
            }
        } else {
            self.editor.restyle(ctx);
        }
        self.styled = Some(library);
    }

    /// ⌘⌥1 / ⌘⌥2 / ⌘⌥3 (Ctrl+Alt elsewhere) pick a module from anywhere.
    fn module_keys(&mut self, ctx: &egui::Context) {
        if !self.editor.host_modes {
            return;
        }
        let picked = Module::ALL.into_iter().find(|m| ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT, m.key())));
        if let Some(m) = picked {
            self.switch(ctx, m);
        }
    }
}

/// What the Library did with a file saved in Compositing ([`add_saved_file`]).
#[derive(Debug, PartialEq, Eq)]
enum LibraryUpdate {
    /// The Library already had the file: it was re-read (thumbnail and previews follow).
    Reloaded(lightcraft_catalog::PhotoId),
    /// Imported and stacked on top of `original`, the photo the document came from; selected.
    Added { id: lightcraft_catalog::PhotoId, original: lightcraft_catalog::PhotoId },
}

/// The editor's document that shows Library photo `id`: the one saved to its file, else one whose
/// Develop layer follows it.
fn document_of(editor: &PhotocraftApp, s: &lightcraft_engine::Session, id: lightcraft_catalog::PhotoId) -> Option<usize> {
    let docs = editor.session.documents();
    docs.iter()
        .position(|d| d.path.as_deref().and_then(|p| photo_at_path(s, p)) == Some(id))
        .or_else(|| photocraft_engine::develop_layer_cmds::document_of_photo(&editor.session, id.0))
}

/// The Library photos Compositing has unsaved work on: (photo, version key, document). A document
/// counts once it is more than its one Develop layer and has unsaved changes; it stands for the
/// photo whose file it was saved to (a `-Edit.psd` in the Library), else the photo its Develop
/// layer follows.
fn unsaved_composites(editor: &PhotocraftApp, s: &lightcraft_engine::Session) -> Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> {
    let mut out: Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> = Vec::new();
    for d in editor.session.documents() {
        if !d.is_dirty() || photocraft_engine::develop_layer_cmds::is_untouched(&d.doc) {
            continue;
        }
        let photo = d.path.as_deref().and_then(|p| photo_at_path(s, p)).or_else(|| {
            photocraft_engine::develop_layer_cmds::develop_layers(&d.doc)
                .into_iter()
                .filter_map(|(_, p)| p.map(lightcraft_catalog::PhotoId))
                .find(|id| s.catalog.photo(*id).is_some())
        });
        let Some(photo) = photo.filter(|p| !out.iter().any(|o| o.0 == *p)) else { continue };
        let key = (Arc::as_ptr(&d.doc) as usize as u64) ^ d.revision.rotate_left(32);
        out.push((photo, key, d.doc.clone()));
    }
    out
}

/// The Library photo whose file is `path` (a master: virtual copies share its file).
fn photo_at_path(s: &lightcraft_engine::Session, path: &str) -> Option<lightcraft_catalog::PhotoId> {
    let file = |p: &lightcraft_catalog::Photo| match &p.source {
        lightcraft_catalog::Source::File { path } if p.copy_of.is_none() => Some(path.clone()),
        _ => None,
    };
    if let Some(p) = s.catalog.photos().find(|p| file(p).as_deref() == Some(path)) {
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

/// The Library's half of saving in Compositing: a file it has is reloaded; a file saved from a
/// document that came from Library photo `original` is imported, stacked on top of the original
/// (expanded, as Edit in External Editor does) and selected. Anything else is left alone (`None`).
fn add_saved_file(s: &mut lightcraft_engine::Session, path: &str, original: Option<u64>) -> Result<Option<LibraryUpdate>, String> {
    if let Some(id) = photo_at_path(s, path) {
        s.execute("photo.reload", &serde_json::json!({ "ids": [id.0] })).map_err(|e| e.to_string())?;
        return Ok(Some(LibraryUpdate::Reloaded(id)));
    }
    let Some(original) = original.map(lightcraft_catalog::PhotoId).filter(|id| s.catalog.photo(*id).is_some()) else { return Ok(None) };
    let r = s.execute("library.import", &serde_json::json!({ "paths": [path] })).map_err(|e| e.to_string())?;
    let Some(id) = r["imported"].get(0).and_then(serde_json::Value::as_u64).map(lightcraft_catalog::PhotoId) else {
        let why = r["failed"][0][1].as_str().or(r["duplicates"][0]["reason"].as_str().map(|_| "the Library already has the same picture"));
        return Err(why.unwrap_or("not a photo it reads").to_string());
    };
    s.execute("stack.group", &serde_json::json!({ "ids": [id.0, original.0], "top": id.0, "collapsed": false })).map_err(|e| e.to_string())?;
    s.selection = lightcraft_engine::Selection::single(id);
    Ok(Some(LibraryUpdate::Added { id, original }))
}

/// Brings the editor's document saved at `path` forward; `false` when none is open.
fn focus_open_document(editor: &mut PhotocraftApp, path: &str) -> bool {
    let same = |p: &str| p == path || std::fs::canonicalize(p).ok().zip(std::fs::canonicalize(path).ok()).is_some_and(|(a, b)| a == b);
    match editor.session.documents().iter().position(|d| d.path.as_deref().is_some_and(same)) {
        Some(i) => {
            editor.session.set_active(i);
            true
        }
        None => false,
    }
}

/// Library shows the grid (or wherever browsing was); Develop shows the active photo with the
/// develop tools (choosing the first photo when none is active).
fn set_library_view(lib: &mut LightcraftApp, develop: bool) {
    use lightcraft_ui_egui::state::{RightPanel, ViewMode};
    if develop {
        if lib.session.active().is_none() {
            let _ = lib.run("library.next", serde_json::json!({}));
        }
        lib.ui.view = ViewMode::Detail;
        if !lib.ui.right.is_edit_tool() {
            lib.ui.right = RightPanel::Edit;
        }
    } else {
        if lib.ui.right.is_edit_tool() {
            lib.ui.right = RightPanel::None;
        }
        if lib.ui.view == ViewMode::Detail {
            lib.ui.view = ViewMode::PhotoGrid;
        }
    }
}

/// Which module the Library app's own navigation is in (it can enter its develop tools itself).
fn library_module(lib: &LightcraftApp) -> Module {
    if lib.ui.view == lightcraft_ui_egui::state::ViewMode::Detail && lib.ui.right.is_edit_tool() { Module::Develop } else { Module::Library }
}

impl eframe::App for Host {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frames += 1;
        self.editor.current_module = self.mode;
        self.module_keys(ctx);
        // The editor installs the fonts (a superset of the Library's) on its first frame.
        if self.frames == 1 || self.mode == Module::Compositing {
            self.editor.logic(ctx, frame);
        } else {
            self.editor.background_tick(ctx);
        }
        // Quitting from the Library with unsaved documents: the editor asks about them.
        if self.quit_review(ctx) {
            self.editor.logic(ctx, frame);
        }
        let opened: Vec<String> = std::mem::take(&mut *self.opens.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        if !opened.is_empty() {
            // a document already open (Open in Compositing twice) comes forward instead
            let fresh: Vec<String> = opened.into_iter().filter(|p| !focus_open_document(&mut self.editor, p)).collect();
            self.editor.open_paths(&fresh);
            self.switch(ctx, Module::Compositing);
        }
        if let Some(m) = self.editor.switch_module.take() {
            self.switch(ctx, m);
            // The filmstrip's Review in Library: the folder in the Library's grid, read in place.
            if let Some(dir) = self.editor.browse_in_library.take()
                && let Err(e) = self.library().run("library.browse", serde_json::json!({ "path": dir }))
            {
                log::warn!("Review in Library: {e}");
            }
        }
        // Double-clicking a Develop layer: Develop on its photo.
        if let Some(id) = self.editor.develop_request.take() {
            self.left_compositing_with = Some(id);
            let lib = self.library();
            let _ = lib.run("library.select", serde_json::json!({ "ids": [id], "active": id }));
            self.mode = Module::Compositing;
            self.switch(ctx, Module::Develop);
        }
        // Saved in Compositing: into the Library (Save and Return to Library: and show it there).
        if !self.editor.library_saves.is_empty() {
            self.take_library_saves(Some(ctx));
        }
        // The Library shows what Compositing is doing to its photos, and goes back to it.
        if self.mode != Module::Compositing && self.library.is_some() {
            self.update_composites(ctx);
            self.open_composite(ctx);
        }
        // Filter › Camera Raw Filter…: a session in Develop.
        if self.editor.develop_filter.request.is_some() {
            self.start_camera_raw(ctx);
        }
        match self.mode {
            Module::Library | Module::Develop if self.frames > 1 && self.camera_raw.is_some() => {
                self.library().logic(ctx);
                self.camera_raw_tick(ctx);
            }
            Module::Library | Module::Develop if self.frames > 1 => {
                self.library().logic(ctx);
                // The Library's own navigation (D, G, Esc, the Edit button) moves between Library
                // and Develop; the title bar follows.
                if let Some(lib) = self.library.as_ref() {
                    let now = library_module(lib);
                    if now != self.mode {
                        self.mode = now;
                        remember_module(now);
                    }
                }
                if let Some(lib) = self.library.as_mut() {
                    self.prefs.tick(lib, ctx);
                }
            }
            _ => {
                if let Some(lib) = self.library.as_mut() {
                    lib.background_tick(ctx);
                }
            }
        }
        self.restyle(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        match (self.mode, self.library.as_mut()) {
            (Module::Library | Module::Develop, Some(lib)) => lib.raw_input_hook(raw),
            _ => self.editor.raw_input_hook(ctx, raw),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.mode == Module::Compositing || self.library.is_none() {
            self.editor.ui(ui, frame);
            return;
        }
        let title = match &self.camera_raw {
            Some(cr) => format!("Camera Raw Filter · {}", cr.name),
            None => self.library.as_ref().map(|l| library_title(l, self.mode)).unwrap_or_default(),
        };
        if let Some(m) = photocraft_ui_egui::panels::library_title_bar(&mut self.editor, ui, &title, self.mode) {
            let ctx = ui.ctx().clone();
            self.switch(&ctx, m);
            ctx.request_repaint();
        }
        if let Some(lib) = self.library.as_mut() {
            lib.ui(ui);
        }
        photocraft_ui_egui::panels::host_resize_zones(&self.editor, ui);
    }

    fn on_exit(&mut self) {
        self.editor.on_exit();
        // Saved while quitting (the unsaved-changes prompt): the Library still takes the files.
        self.take_library_saves(None);
        // An open Camera Raw Filter session is dropped (nothing applied), and the Library's view
        // is restored before its settings are saved.
        if let (Some(cr), Some(lib)) = (self.camera_raw.take(), self.library.as_mut()) {
            lib.session.close_ephemeral();
            lib.host_session = None;
            lib.ui.view = cr.view;
            lib.ui.right = cr.right;
            lib.ui.left_panel = cr.left_panel;
        }
        if let Some(lib) = self.library.as_mut() {
            if let Err(e) = self.prefs.save(lib) {
                eprintln!("local-image: {e}");
            }
            if let Err(e) = lib.session.close_library() {
                eprintln!("local-image: saving the library failed: {e}");
            }
        }
        if !lightcraft_gpu::quiesce(std::time::Duration::from_secs(3)) {
            eprintln!("local-image: GPU work still running at exit");
        }
    }
}

fn library_title(lib: &LightcraftApp, module: Module) -> String {
    if module == Module::Develop
        && let Some(ph) = lib.session.active().and_then(|id| lib.session.catalog.photo(id))
    {
        return format!("Develop · {}", ph.file_name);
    }
    let n = lib.session.catalog.len();
    format!("Library · {n} photo{}", if n == 1 { "" } else { "s" })
}

// ------------------------------------------------------------------------------ settings

fn config_dir() -> Option<PathBuf> {
    crate::app_dirs::config_dir()
}

/// Which module was shown last (`<config>/mode`), so the app reopens where the user was. Develop
/// reopens as Library (there may be no photo to develop yet).
fn last_module() -> Module {
    match config_dir().and_then(|d| std::fs::read_to_string(d.join("mode")).ok()).as_deref().map(str::trim) {
        Some("library" | "develop") => Module::Library,
        _ => Module::Compositing,
    }
}

fn remember_module(m: Module) {
    // tests switch modules too: never over the user's own setting
    if cfg!(test) || std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
        return;
    }
    if let Some(d) = config_dir() {
        let _ = std::fs::create_dir_all(&d);
        let name = match m {
            Module::Library => "library",
            Module::Develop => "develop",
            Module::Compositing => "editor",
        };
        let _ = std::fs::write(d.join("mode"), name);
    }
}

/// The Library's UI state and settings (`<config>/library-ui.json`), if saved.
fn load_prefs() -> Option<UiState> {
    if std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
        return None;
    }
    let bytes = std::fs::read(config_dir()?.join("library-ui.json")).ok()?;
    serde_json::from_slice::<UiState>(&bytes).ok().map(UiState::sanitized)
}

/// Saves `library-ui.json` when it changed (checked every few seconds) and at exit.
#[derive(Default)]
struct PrefsWriter {
    written: Vec<u8>,
    checked: f64,
}

impl PrefsWriter {
    fn save(&mut self, app: &LightcraftApp) -> Result<(), String> {
        if std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
            return Ok(());
        }
        let Some(d) = config_dir() else { return Ok(()) };
        let bytes = serde_json::to_vec_pretty(&app.ui).map_err(|e| e.to_string())?;
        if bytes == self.written {
            return Ok(());
        }
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let tmp = d.join("library-ui.json.tmp");
        std::fs::write(&tmp, &bytes)
            .and_then(|()| std::fs::rename(&tmp, d.join("library-ui.json")))
            .map_err(|e| format!("saving the Library settings failed: {e}"))?;
        self.written = bytes;
        Ok(())
    }

    fn tick(&mut self, app: &mut LightcraftApp, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.checked < 3.0 {
            return;
        }
        self.checked = now;
        if let Err(e) = self.save(app) {
            log::warn!("{e}");
        }
    }
}

/// The library folder: `LOCAL_IMAGE_LIBRARY`, else the one last opened (Library settings), else
/// `~/Pictures/Local Image Library`.
fn library_dir(prefs: Option<&UiState>) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LOCAL_IMAGE_LIBRARY").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = prefs.map(|u| u.settings.library_path.clone()).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }?;
    Some(PathBuf::from(home).join("Pictures").join("Local Image Library"))
}

fn open_library_app(opens: Opens, prefs_writer: &mut PrefsWriter) -> LightcraftApp {
    use lightcraft_engine::Session;
    use lightcraft_ui_egui::panels::library_problem::LibraryProblem;
    let prefs = load_prefs();
    let unopened = || Session::new().with_fs().with_system_clock();
    let (mut session, problem) = match library_dir(prefs.as_ref()) {
        None => (unopened(), Some(LibraryProblem::new("", "There is no home folder to keep the library in. Choose a folder for it."))),
        Some(dir) => {
            let mut s = unopened();
            match s.open_library(&dir, false) {
                Ok(_) => (s, None),
                Err(e) => (unopened(), Some(LibraryProblem::new(dir.to_string_lossy(), e.to_string()))),
            }
        }
    };
    // AI masks (SAM 3): optional, offered for download when first needed.
    session.segmenter.dir = std::env::var_os("LOCAL_IMAGE_SAM3_DIR").map(PathBuf::from).or_else(|| config_dir().map(|d| d.join("models").join("sam3")));
    session.segmenter.mirrors_file = config_dir().map(|d| d.join("models").join("sam3-mirrors.txt"));
    // Camera Raw Filter sessions left behind by a crash.
    lightcraft_engine::ephemeral::clear_dir(&camera_raw_dir());
    // Quick Subject / Background / Sky masks share the models Compositing's selections use.
    session.quick_seg_dir = Some(photocraft_engine::seg::models_dir());
    // AI Remove: Compositing's AI engines.
    session.enhance.host = Some(crate::develop_ai::DevelopAi::shared());
    let mut app = LightcraftApp::new(session, services(opens));
    if let Some(ui) = prefs {
        app.ui = ui;
    }
    lightcraft_ui_egui::i18n::set_language(app.ui.language);
    app.host_fonts = true;
    app.integrated_titlebar = false;
    app.library_problem = problem;
    prefs_writer.written.clear();
    app
}

fn services(opens: Opens) -> Services {
    Services {
        download_models: Some(Box::new(|id, cancel| {
            let spec = li_seg::spec(id).ok_or("unknown model")?;
            if cancel {
                if let Some(dl) = photocraft_ui_egui::ai_ui::downloads().get(&format!("seg:{id}")) {
                    dl.ctl.cancel();
                }
            } else {
                photocraft_ui_egui::ai_ui::start_seg_download(spec);
            }
            Ok(())
        })),
        model_download_status: Some(Box::new(|id| {
            photocraft_ui_egui::ai_ui::downloads()
                .get(&format!("seg:{id}"))
                .map(|dl| lightcraft_ui_egui::ModelDownloadStatus { running: !dl.finished, done: dl.done, total: dl.total, error: dl.error.clone() })
                .unwrap_or_default()
        })),
        pick_folder: Some(Box::new(|| rfd::FileDialog::new().set_title("Choose a Library Folder").pick_folder().map(|p| p.to_string_lossy().to_string()))),
        open_with: Some(Box::new(move |path: &str, app: &str| {
            let app = app.trim();
            // No other editor chosen: Local Image's own editor, in this window.
            if app.is_empty() || app.eq_ignore_ascii_case("local image") {
                opens.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(path.to_owned());
                return Ok(());
            }
            let mut c = if cfg!(target_os = "macos") {
                let mut c = std::process::Command::new("open");
                c.args(["-a", app]);
                c
            } else {
                std::process::Command::new(app)
            };
            c.arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
        })),
        open_url: Some(Box::new(|url: &str| {
            if !url.starts_with("https://") {
                return Err("only https links are opened".into());
            }
            open::that(url).map_err(|e| e.to_string())
        })),
        reveal: Some(Box::new(|path: &str| {
            let p = std::path::Path::new(path);
            let target = if cfg!(target_os = "linux") { p.parent().unwrap_or(p) } else { p };
            open::that(target).map_err(|e| e.to_string())
        })),
        pick_files: Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter(
                    "Photos",
                    &[
                        "jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "rwl", "raw", "pef", "psd",
                        "jxl", "gif", "bmp", "heic", "heif", "avif",
                    ],
                )
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        pick_preset_files: Some(Box::new(|| {
            rfd::FileDialog::new()
                .set_title("Import Presets")
                .add_filter("Presets & Profiles", &["lcpreset", "xmp", "lrtemplate", "zip", "dng", "lmp", "mplumpack", "cube"])
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        pick_tracklog: Some(Box::new(|| {
            rfd::FileDialog::new()
                .set_title("Auto-Tag from Tracklog")
                .add_filter("GPS Track Log", &["gpx"])
                .pick_file()
                .map(|p| vec![p.to_string_lossy().to_string()])
                .unwrap_or_default()
        })),
        save_preset_file: Some(Box::new(|name: &str| {
            rfd::FileDialog::new()
                .set_title("Export Presets")
                .add_filter("Preset", &["lcpreset"])
                .set_file_name(name)
                .save_file()
                .map(|p| p.to_string_lossy().to_string())
        })),
        pick_curve_preset_files: Some(Box::new(|| {
            rfd::FileDialog::new()
                .set_title("Import Point Curve Presets")
                .add_filter("Point Curve Presets", &["lccurve", "json"])
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        save_curve_preset_file: Some(Box::new(|name: &str| {
            rfd::FileDialog::new()
                .set_title("Export Point Curve Presets")
                .add_filter("Point Curve Presets", &["lccurve"])
                .set_file_name(name)
                .save_file()
                .map(|p| p.to_string_lossy().to_string())
        })),
        write_shared: Some(Arc::new(lightcraft_engine::export::write_file)),
        write: Some(Box::new(lightcraft_engine::export::write_file)),
        png: Some(Box::new(|img: &lightcraft_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        backup_library: None,
        restore_library: None,
        doc_layers: Some(Arc::new(crate::doc_layers::load)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_document_twice_brings_it_forward() {
        let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
        let doc =
            |n: &str| photocraft_doc::Document::new(n, photocraft_doc::Size::new(4, 4), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
        editor.session.open_document(doc("a"), Some("/photos/a.psd".into()));
        editor.session.open_document(doc("b"), Some("/photos/b.psd".into()));
        assert_eq!(editor.session.active_index(), Some(1));
        assert!(focus_open_document(&mut editor, "/photos/a.psd"));
        assert_eq!(editor.session.active_index(), Some(0));
        assert!(!focus_open_document(&mut editor, "/photos/c.psd"));
        assert_eq!(editor.session.active_index(), Some(0));
    }

    // ------------------------------------------------------------- Library ↔ Compositing round trip

    use photocraft_ui_egui::develop_layer::{self, LibrarySave};
    use serde_json::json;

    /// A folder with `photo.png` in it, and a Library that has the photo.
    fn library_with_photo(tag: &str) -> (PathBuf, String, LightcraftApp, lightcraft_catalog::PhotoId) {
        let dir = std::env::temp_dir().join(format!("li-roundtrip-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([(x * 6) as u8, (y * 8) as u8, 60])).save(&path).unwrap();
        let path = path.to_string_lossy().to_string();
        let mut s = lightcraft_engine::Session::new().with_fs();
        s.execute("library.import", &json!({ "paths": [path] })).unwrap();
        let id = s.catalog.photos().next().map(|p| p.id).unwrap();
        (dir, path, LightcraftApp::new(s, lightcraft_ui_egui::Services::default()), id)
    }

    /// A host whose editor writes real PSDs, with `library` open and the photo in Compositing as a
    /// Develop layer with a layer added on top (unsaved work).
    fn host_with_round_trip(library: LightcraftApp, photo: lightcraft_catalog::PhotoId, path: &str) -> Host {
        let services = photocraft_ui_egui::Services {
            export: Some(Box::new(|doc: &photocraft_doc::Document, path: &str, _: &photocraft_ui_egui::ExportSettings| {
                photocraft_io::export(doc, path, &photocraft_io::ExportOptions::default()).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
            })),
            write: Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string()))),
            ..Default::default()
        };
        let mut host = Host::new(PhotocraftApp::new(photocraft_engine::Session::new(), services));
        host.editor.host_modes = true;
        // the photo's own settings, as the switch to Compositing passes them
        let settings = serde_json::to_value(&*library.session.catalog.photo(photo).unwrap().develop).unwrap();
        host.library = Some(library);
        host.mode = Module::Compositing;
        develop_layer::open_photo(&mut host.editor, photo.0, path, &settings, "photo.png").unwrap();
        host.editor.run("layer.new.layer", json!({})).unwrap();
        assert!(host.editor.has_unsaved());
        host
    }

    fn catalog(host: &Host) -> &lightcraft_engine::Session {
        &host.library.as_ref().unwrap().session
    }

    /// ⌘S on a photo from the Library writes `<name>-Edit.psd` beside the original; the Library
    /// adds it stacked on top of the original exactly once; saving again reloads it.
    #[test]
    fn saving_a_photo_from_the_library_adds_it_stacked_on_the_original() {
        let (dir, path, lib, orig) = library_with_photo("save");
        let mut host = host_with_round_trip(lib, orig, &path);
        let ctx = egui::Context::default();
        let r = photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, "file.save", json!({})).unwrap();
        let saved = dir.join("photo-Edit.psd").to_string_lossy().to_string();
        assert_eq!(r["path"], saved.as_str(), "beside the original, without a dialog");
        assert!(std::path::Path::new(&saved).is_file());
        assert_eq!(host.editor.library_saves, vec![LibrarySave { path: saved.clone(), photo: Some(orig.0), show: false }]);

        host.take_library_saves(Some(&ctx));
        assert!(host.editor.library_saves.is_empty());
        let s = catalog(&host);
        assert_eq!(s.catalog.len(), 2, "imported once");
        let edit = photo_at_path(s, &saved).expect("the edit is in the Library");
        let stack = s.catalog.stack_of(edit).expect("stacked");
        assert_eq!(stack.photos, vec![edit, orig], "on top of the original");
        assert!(!stack.collapsed);
        assert_eq!(s.active(), Some(edit));
        assert_eq!(host.editor.ui.status, "Saved to Library, stacked with photo.png");
        assert_eq!(host.mode, Module::Compositing, "a plain save stays in Compositing");

        // more work, saved again: in place, and the Library re-reads the same photo
        host.editor.run("layer.new.layer", json!({})).unwrap();
        let r = photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, "file.save", json!({})).unwrap();
        assert_eq!(r["path"], saved.as_str());
        host.take_library_saves(Some(&ctx));
        let s = catalog(&host);
        assert_eq!(s.catalog.len(), 2, "reloaded, not imported again");
        assert_eq!(photo_at_path(s, &saved), Some(edit));
        assert!(host.editor.ui.status.contains("updated in the Library"), "{}", host.editor.ui.status);

        // a second round trip of the same photo doesn't overwrite the first edit
        assert_eq!(develop_layer::edit_path(&path), dir.join("photo-Edit-2.psd").to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// File › Save and Return to Library: saved and stacked as above, then the Library shows the
    /// edit; coming back to Compositing shows the same document.
    #[test]
    fn save_and_return_shows_the_edit_in_the_library() {
        let (dir, path, lib, orig) = library_with_photo("return");
        let mut host = host_with_round_trip(lib, orig, &path);
        let ctx = egui::Context::default();
        assert!(photocraft_ui_egui::menus::is_enabled(&host.editor, develop_layer::SAVE_RETURN_ID));
        photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, develop_layer::SAVE_RETURN_ID, json!({})).unwrap();
        assert!(host.editor.library_saves[0].show);
        host.take_library_saves(Some(&ctx));
        assert_eq!(host.mode, Module::Library);
        let saved = dir.join("photo-Edit.psd").to_string_lossy().to_string();
        let edit = photo_at_path(catalog(&host), &saved).unwrap();
        assert_eq!(catalog(&host).active(), Some(edit));
        host.switch(&ctx, Module::Compositing);
        assert_eq!(host.editor.session.documents().len(), 1, "nothing opened again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Unsaved work in Compositing shows in the Library (its loupe and grid), and goes away once
    /// saved.
    #[test]
    fn the_library_shows_unsaved_compositing_work() {
        let (dir, path, lib, orig) = library_with_photo("composite");
        let mut host = host_with_round_trip(lib, orig, &path);
        let ctx = egui::Context::default();
        host.switch(&ctx, Module::Library);
        let shown = |h: &Host| h.library.as_ref().unwrap().host_composites.get(&orig).map(|c| (c.size, c.label.clone()));
        for _ in 0..500 {
            host.update_composites(&ctx);
            if shown(&host).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(shown(&host), Some(([40, 30], "Edited in Compositing · unsaved".to_string())));
        // its button goes back to the document as it is
        host.library.as_mut().unwrap().host_composite_open = Some(orig);
        host.open_composite(&ctx);
        assert_eq!(host.mode, Module::Compositing);
        assert_eq!(host.editor.session.documents().len(), 1);
        // saved: the Library shows its file again
        photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, "file.save", json!({})).unwrap();
        host.switch(&ctx, Module::Library);
        host.update_composites(&ctx);
        assert_eq!(shown(&host), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Quitting from the Library with unsaved Compositing work: Compositing comes forward with
    /// its Save / Don't Save / Cancel prompt, and the window stays open.
    #[test]
    fn quitting_from_the_library_asks_about_unsaved_work() {
        let (dir, path, lib, orig) = library_with_photo("quit");
        let mut host = host_with_round_trip(lib, orig, &path);
        let ctx = egui::Context::default();
        host.switch(&ctx, Module::Library);
        assert_eq!(host.mode, Module::Library);
        let mut raw = egui::RawInput::default();
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        let mut asked = false;
        let mut out = ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            asked = host.quit_review(&ctx);
            photocraft_ui_egui::discard_ui::guard_window_close(&mut host.editor, &ctx);
        });
        assert!(asked);
        assert_eq!(host.mode, Module::Compositing, "the prompt is in Compositing");
        assert_eq!(host.editor.session.documents().len(), 1, "no photo opened on the way");
        let cmds = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds.contains(&egui::ViewportCommand::CancelClose), "the window stays open: {cmds:?}");
        out.textures_delta.clear();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Library → Develop (new edits) → Compositing: the edits arrive as a new Develop layer on
    /// top of a document with compositing work; its existing layers stay as they were.
    #[test]
    fn new_develop_edits_come_back_as_a_new_layer() {
        let (dir, path, lib, orig) = library_with_photo("develop2");
        let mut host = host_with_round_trip(lib, orig, &path);
        let ctx = egui::Context::default();
        let before = host.editor.session.active().unwrap().doc.layers.clone();
        assert_eq!(before.len(), 2);
        host.switch(&ctx, Module::Library);
        host.switch(&ctx, Module::Develop);
        host.library().run("develop.set", json!({ "control": "light.exposure", "value": 1.0 })).unwrap();
        host.switch(&ctx, Module::Compositing);
        assert_eq!(host.editor.session.documents().len(), 1, "same document");
        let layers = &host.editor.session.active().unwrap().doc.layers;
        assert_eq!(layers.len(), 3, "{:?}", layers.iter().map(|l| &l.name).collect::<Vec<_>>());
        assert_eq!(&layers[..2], &before[..], "existing layers untouched");
        assert_eq!(layers[2].name, "Develop 2");
        // no further edits: coming back again adds nothing
        host.switch(&ctx, Module::Library);
        host.switch(&ctx, Module::Compositing);
        assert_eq!(host.editor.session.active().unwrap().doc.layers.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
