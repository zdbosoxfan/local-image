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
//! Unsaved edits appear in the Library grid and loupe as a worker-rendered composite. Returning
//! to Library shows a non-blocking notice with Save and Keep Editing actions; saves report their
//! destination and Library stack in both modules.
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
    /// One non-blocking round-trip notice, also visible while its preview is rendering.
    round_trip_notice: Option<RoundTripNotice>,
}

#[derive(Clone)]
enum RoundTripNotice {
    Unsaved { doc: photocraft_doc::DocId, photo: lightcraft_catalog::PhotoId, destination: String },
    Saved(String),
    Error(String),
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
            round_trip_notice: None,
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
                    (Some(id), format!("Saved {}; added to the Library, stacked with {name}", save.path))
                }
                Ok(Some(LibraryUpdate::Reloaded(id))) => {
                    let name = lib.session.catalog.photo(id).map(|p| p.file_name.clone()).unwrap_or_default();
                    (
                        Some(id),
                        if save.overwritten {
                            format!("Overwrote {}; updated {name} in the Library; previous version kept in Trash", save.path)
                        } else {
                            format!("Saved {}; updated {name} in the Library", save.path)
                        },
                    )
                }
                Ok(None) => (None, String::new()),
                Err(e) => {
                    self.editor.ui.status = format!("Saved {}, but the Library couldn't add it: {e}", save.path);
                    self.editor.ui.status_error = true;
                    self.round_trip_notice = Some(RoundTripNotice::Error(self.editor.ui.status.clone()));
                    continue;
                }
            };
            if !msg.is_empty() {
                self.editor.ui.status = msg.clone();
                self.editor.ui.status_error = false;
                self.round_trip_notice = Some(RoundTripNotice::Saved(msg.clone()));
                if ctx.is_some() {
                    photocraft_ui_egui::notices::post(&mut self.editor, msg.clone(), vec![], false, None);
                }
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

    /// Save the document named by the notice, even if another editor tab is active now.
    fn save_preview(&mut self, ctx: &egui::Context, doc: photocraft_doc::DocId) {
        let Some(i) = self.editor.session.documents().iter().position(|d| d.doc.id == doc) else {
            self.round_trip_notice = None;
            return;
        };
        self.editor.session.set_active(i);
        self.editor.sync_views();
        match photocraft_ui_egui::menus::invoke(&mut self.editor, ctx, photocraft_ui_egui::develop_layer::SAVE_RETURN_ID, serde_json::json!({})) {
            Ok(_) => {
                self.take_library_saves(Some(ctx));
                self.update_composites(ctx);
            }
            Err(e) => {
                // Keep Save / Keep Editing available after a failed write.
                self.library().toast_error(ctx, format!("Couldn't save the Compositing edit: {e}"));
            }
        }
    }

    /// Library's ⌘S saves its selected unsaved preview through the same editor save path.
    fn preview_save_key(&mut self, ctx: &egui::Context) {
        if self.mode != Module::Library || ctx.egui_wants_keyboard_input() {
            return;
        }
        let doc = self.library.as_ref().and_then(|lib| {
            let id = lib.session.active()?;
            unsaved_composites(&self.editor, &lib.session).into_iter().find(|(p, _, _)| *p == id).map(|(_, _, d)| d.id)
        });
        if let Some(doc) = doc
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
        {
            self.save_preview(ctx, doc);
        }
    }

    fn round_trip_notice_ui(&mut self, ctx: &egui::Context) {
        if self.mode != Module::Library {
            return;
        }
        let Some(notice) = self.round_trip_notice.clone() else { return };
        if let RoundTripNotice::Unsaved { doc, .. } = &notice
            && !self.editor.session.documents().iter().any(|d| d.doc.id == *doc && d.is_dirty())
        {
            self.round_trip_notice = None;
            return;
        }
        let overwrite_reason = match &notice {
            RoundTripNotice::Unsaved { doc, .. } => photocraft_ui_egui::overwrite_original::disabled_reason(&self.editor, *doc),
            _ => None,
        };
        let mut action = None;
        let t = lightcraft_ui_egui::theme::Tokens::get(ctx);
        egui::Area::new(egui::Id::new("library-round-trip-notice"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -48.0))
            .show(ctx, |ui| {
                egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.button_border)).corner_radius(6.0).inner_margin(12).show(ui, |ui| {
                    ui.set_width(440.0);
                    match &notice {
                        RoundTripNotice::Unsaved { destination, .. } => {
                            ui.label(egui::RichText::new("Edited in Compositing · unsaved").strong());
                            ui.label("The Library shows a preview of your layers. Save a copy or overwrite the original to keep this edit.");
                            ui.label(format!("Save as {destination} and stack it with the original in the Library."));
                        }
                        RoundTripNotice::Saved(message) | RoundTripNotice::Error(message) => {
                            ui.label(message);
                        }
                    }
                    ui.horizontal(|ui| {
                        for (id, label, choice) in if matches!(notice, RoundTripNotice::Unsaved { .. }) {
                            vec![
                                ("roundTripSave", "Save", 0),
                                ("roundTripOverwrite", "Overwrite Original", 3),
                                ("roundTripEdit", "Keep Editing", 1),
                                ("roundTripDismiss", "Dismiss", 2),
                            ]
                        } else {
                            vec![("roundTripDismiss", "Dismiss", 2)]
                        } {
                            let r = ui.add_enabled(choice != 3 || overwrite_reason.is_none(), egui::Button::new(label));
                            let r = if choice == 3 { r.on_disabled_hover_text(overwrite_reason.unwrap_or("")) } else { r };
                            lightcraft_ui_egui::widgets::register(ctx, format!("button:{id}"), r.rect);
                            if r.clicked() {
                                action = Some(choice);
                            }
                        }
                    });
                });
            });
        match (action, notice) {
            (Some(0), RoundTripNotice::Unsaved { doc, .. }) => self.save_preview(ctx, doc),
            (Some(3), RoundTripNotice::Unsaved { doc, .. }) => {
                if let Err(e) = photocraft_ui_egui::overwrite_original::request(&mut self.editor, doc, true) {
                    self.library().toast_error(ctx, e);
                }
            }
            (Some(1), RoundTripNotice::Unsaved { doc, photo, .. }) => {
                if let Some(i) = self.editor.session.documents().iter().position(|d| d.doc.id == doc) {
                    self.editor.session.set_active(i);
                    self.editor.sync_views();
                    self.left_compositing_with = Some(photo.0);
                    self.library().session.selection = lightcraft_engine::Selection::single(photo);
                    self.switch(ctx, Module::Compositing);
                }
                self.round_trip_notice = None;
            }
            (Some(2), _) => self.round_trip_notice = None,
            _ => {}
        }
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
            // Library shows the document's saved file when it has one; Develop follows its
            // Develop layer's source. Otherwise a plain Save followed by Library selects the
            // original again and hides the newly saved layers.
            let saved = (to == Module::Library)
                .then(|| self.editor.session.active()?.path.as_deref().and_then(|p| self.library.as_ref().and_then(|l| photo_at_path(&l.session, p))))
                .flatten()
                .map(|p| p.0);
            if let Some(id) = saved.or_else(|| photocraft_ui_egui::develop_layer::active_photo(&self.editor)) {
                let lib = self.library();
                let _ = lib.run("library.select", serde_json::json!({ "ids": [id], "active": id }));
                self.left_compositing_with = Some(id);
            }
            if to == Module::Library
                && let (Some(lib), Some(active)) = (&self.library, self.editor.session.active())
                && let Some((photo, _, _)) = unsaved_composites(&self.editor, &lib.session).into_iter().find(|(_, _, d)| d.id == active.doc.id)
            {
                let destination = active.path.clone().or_else(|| photocraft_ui_egui::develop_layer::library_save_path(&self.editor)).unwrap_or_default();
                self.round_trip_notice = Some(RoundTripNotice::Unsaved { doc: active.doc.id, photo, destination });
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
/// counts once it has unsaved changes (a one-layer document can have a mask, filter or retouching);
/// it stands for the
/// photo whose file it was saved to (a `-Edit.psd` in the Library), else the photo its Develop
/// layer follows.
fn unsaved_composites(editor: &PhotocraftApp, s: &lightcraft_engine::Session) -> Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> {
    let mut out: Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> = Vec::new();
    for d in editor.session.documents() {
        if !d.is_dirty() {
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
        self.preview_save_key(ctx);
        // The editor installs the fonts (a superset of the Library's) on its first frame.
        if self.frames == 1 || self.mode == Module::Compositing {
            self.editor.logic(ctx, frame);
        } else {
            self.editor.background_tick(ctx);
            // Started in the Library or Develop: the editor's frames (which run the started hook)
            // don't happen, so the GPU start-up marker stayed for the whole session and any later
            // exit that wasn't clean (out-of-memory killer, logout, a crash elsewhere) read as a
            // GPU driver crash next launch. The window has presented by now: start-up is done.
            if self.frames >= 3 {
                photocraft_ui_egui::gpu_status::finish_startup(&mut self.editor);
            }
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
        // The switched-to-CPU warning appears in every module, not only once Compositing is open.
        photocraft_ui_egui::gpu_status::show_fallback(&mut self.editor, ui.ctx());
        self.round_trip_notice_ui(ui.ctx());
        let status = self.editor.ui.status.clone();
        photocraft_ui_egui::overwrite_original::show(&mut self.editor, ui.ctx());
        if self.editor.ui.status_error && self.editor.ui.status != status {
            let message = self.editor.ui.status.clone();
            self.library().toast_error(ui.ctx(), message);
        }
        if let Some(lib) = self.library.as_mut() {
            lib.widgets.extend(lightcraft_ui_egui::widgets::take_registry(ui.ctx()));
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
        if cfg!(test) || std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
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
        save_denoise_file: Some(Box::new(|name| {
            rfd::FileDialog::new()
                .set_file_name(name)
                .add_filter("Denoised image", &["tiff", "tif", "dng"])
                .save_file()
                .map(|p| p.to_string_lossy().into_owned())
        })),
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
        library_with_photo_format(tag, "png")
    }

    fn library_with_photo_format(tag: &str, ext: &str) -> (PathBuf, String, LightcraftApp, lightcraft_catalog::PhotoId) {
        let dir = std::env::temp_dir().join(format!("li-roundtrip-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("photo.{ext}"));
        image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([(x * 6) as u8, (y * 8) as u8, 60])).save(&path).unwrap();
        let path = path.to_string_lossy().to_string();
        let mut s = lightcraft_engine::Session::new().with_fs();
        s.open_library(dir.join("library"), false).unwrap();
        s.execute("library.import", &json!({ "paths": [path] })).unwrap();
        let id = s.catalog.photos().next().map(|p| p.id).unwrap();
        let mut app = LightcraftApp::new(s, lightcraft_ui_egui::Services::default());
        app.host_fonts = true;
        app.integrated_titlebar = false;
        set_library_view(&mut app, false);
        app.ui.settings.gpu = false;
        (dir, path, app, id)
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
        assert_eq!(host.editor.library_saves, vec![LibrarySave { path: saved.clone(), photo: Some(orig.0), show: false, overwritten: false }]);

        host.take_library_saves(Some(&ctx));
        assert!(host.editor.library_saves.is_empty());
        let s = catalog(&host);
        assert_eq!(s.catalog.len(), 2, "imported once");
        let edit = photo_at_path(s, &saved).expect("the edit is in the Library");
        let stack = s.catalog.stack_of(edit).expect("stacked");
        assert_eq!(stack.photos, vec![edit, orig], "on top of the original");
        assert!(!stack.collapsed);
        assert_eq!(s.active(), Some(edit));
        assert_eq!(host.editor.ui.status, format!("Saved {saved}; added to the Library, stacked with photo.png"));
        assert_eq!(host.mode, Module::Compositing, "a plain save stays in Compositing");
        host.switch(&ctx, Module::Library);
        assert_eq!(catalog(&host).active(), Some(edit), "a normal module switch must show the saved layers too");
        host.switch(&ctx, Module::Compositing);

        // more work, saved again: in place, and the Library re-reads the same photo
        host.editor.run("layer.new.layer", json!({})).unwrap();
        let r = photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, "file.save", json!({})).unwrap();
        assert_eq!(r["path"], saved.as_str());
        host.take_library_saves(Some(&ctx));
        let s = catalog(&host);
        assert_eq!(s.catalog.len(), 2, "reloaded, not imported again");
        assert_eq!(photo_at_path(s, &saved), Some(edit));
        assert!(host.editor.ui.status.contains("in the Library"), "{}", host.editor.ui.status);

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

    /// Drive the real host logic and UI without a window, mirroring Library's headless harness.
    struct RoundTripUi {
        host: Host,
        view: lightcraft_ui_egui::headless::HeadlessView,
        frame: eframe::Frame,
        frames: u64,
    }

    impl RoundTripUi {
        fn new(host: Host) -> Self {
            let view = lightcraft_ui_egui::headless::HeadlessView::new();
            PhotocraftApp::setup_context(&view.ctx, host.editor.ui.theme);
            Self { host, view, frame: eframe::Frame::_new_kittest(), frames: 0 }
        }

        fn step(&mut self, events: Vec<egui::Event>) {
            self.step_at(egui::vec2(1200.0, 900.0), events);
        }

        fn step_at(&mut self, size: egui::Vec2, events: Vec<egui::Event>) -> Vec<egui::ViewportCommand> {
            use eframe::App;
            let mut raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(size, 1.0, self.frames as f64 / 60.0, events);
            self.host.raw_input_hook(&self.view.ctx, &mut raw);
            let commands = self.view.run(raw, |ui| {
                self.host.logic(ui.ctx(), &mut self.frame);
                self.host.ui(ui, &mut self.frame);
            });
            self.frames += 1;
            commands
        }

        fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            self.step(vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
            self.step(vec![egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers }]);
        }

        fn click(&mut self, id: &str) {
            let pos = self.host.library.as_ref().unwrap().widgets.iter().find(|(w, _)| w == id).unwrap().1.center();
            self.step(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                self.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
            }
            self.step(vec![]);
        }

        fn wait_for(&mut self, ready: impl Fn(&Host) -> bool) {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !ready(&self.host) && std::time::Instant::now() < until {
                self.step(vec![]);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(ready(&self.host), "background render did not finish");
            self.step(vec![]);
        }

        fn pixel_at(&self, pos: egui::Pos2) -> egui::Color32 {
            let img = self.view.paint(&self.host.library.as_ref().unwrap().renderer.cpu_textures());
            img.pixels[pos.y as usize * img.size[0] + pos.x as usize]
        }
    }

    fn click_overwrite(h: &mut RoundTripUi, id: &str) {
        // Modal responses use the same egui rectangles on either hosted module.
        h.step(vec![]);
        let pos = h.view.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id))).expect("modal control").center();
        h.step(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            h.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
        }
        h.step(vec![]);
    }

    fn overwrite_host(tag: &str, ext: &str) -> (PathBuf, String, lightcraft_catalog::PhotoId, RoundTripUi) {
        let (dir, path, lib, photo) = library_with_photo_format(tag, ext);
        let mut host = host_with_round_trip(lib, photo, &path);
        let trash = dir.join("Trash");
        host.editor.services.overwrite_original = Some(Box::new(move |doc, path, quality| {
            crate::services::overwrite_original(doc, std::path::Path::new(path), quality, |p| li_ai::trash::copy_to_trash_in(p, &trash))
        }));
        host.editor.run("edit.fill", json!({"color": [1.0, 0.0, 0.0, 1.0]})).unwrap();
        (dir, path, photo, RoundTripUi::new(host))
    }

    fn overwrite_pixels(ext: &str, rs: Option<eframe::egui_wgpu::RenderState>) {
        use lightcraft_ui_egui::state::ViewMode;
        let (dir, path, photo, mut h) = overwrite_host(&format!("overwrite-{ext}-{}", rs.is_some()), ext);
        let previous = std::fs::read(&path).unwrap();
        let mut old_handle = std::fs::File::open(&path).unwrap();
        let history = h.host.editor.session.active().unwrap().history.past_len();
        let develop = catalog(&h.host).catalog.photo(photo).unwrap().develop.clone();
        if let Some(rs) = rs {
            let probe = photocraft_ui_egui::gpu_canvas::GpuCanvas::new(&rs);
            h.host.editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(photocraft_engine::prefs::RenderingMode::Gpu));
            h.host.editor.set_wgpu(rs.clone());
            h.frame.wgpu_render_state = Some(rs);
            h.wait_for(|h| probe.read_texels(h.editor.session.active().unwrap().doc.id.0).is_some());
            assert!(h.host.editor.gpu_active());
            let (_, _, px) = probe.read_texels(h.host.editor.session.active().unwrap().doc.id.0).unwrap();
            assert!(px.iter().all(|p| p[0] > 0.95 && p[1] < 0.05 && p[2] < 0.05));
        }
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&photo));
        h.click("button:roundTripOverwrite");
        assert!(h.host.editor.overwrite_prompt.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), previous, "asking changes no bytes");
        let undo = catalog(&h.host).undo.len();
        let started = std::time::Instant::now();
        click_overwrite(&mut h, "overwriteConfirm");
        eprintln!("overwrite {ext}: real host confirm → atomic write/catalog reload {:?}", started.elapsed());
        assert!(h.host.editor.overwrite_prompt.is_none());
        let new = std::fs::read(&path).unwrap();
        assert_ne!(new, previous);
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".pcsave.tmp")));
        use std::io::Read;
        let mut held_bytes = Vec::new();
        old_handle.read_to_end(&mut held_bytes).unwrap();
        assert_eq!(held_bytes, previous, "atomic rename leaves open old version intact");
        assert_eq!(std::fs::read(dir.join("Trash/files").join(format!("photo.{ext}"))).unwrap(), previous);
        let info = std::fs::read_to_string(dir.join("Trash/info").join(format!("photo.{ext}.trashinfo"))).unwrap();
        assert!(info.contains(&format!("Path={}\n", li_ai::trash::encode_path(std::path::Path::new(&path)))));
        assert_eq!(catalog(&h.host).undo.len(), undo + 1, "one catalog reload step");
        assert_eq!(catalog(&h.host).catalog.len(), 1);
        assert_eq!(catalog(&h.host).catalog.photo(photo).unwrap().develop, develop);
        assert_eq!(h.host.editor.session.active().unwrap().history.past_len(), history);
        assert!(!h.host.editor.has_unsaved());
        assert!(h.host.library.as_ref().unwrap().host_composites.is_empty());
        assert!(
            matches!(&h.host.round_trip_notice, Some(RoundTripNotice::Saved(msg)) if msg.contains("Overwrote") && msg.contains(&path) && msg.contains("Trash"))
        );
        let decoded = photocraft_codecs::decode(&new).unwrap().convert(photocraft_codecs::ChannelLayout::Rgb, photocraft_codecs::SampleType::U8);
        assert!(decoded.data().as_chunks::<3>().0.iter().all(|p| p[0] > 240 && p[1] < 12 && p[2] < 12));
        let slot = lightcraft_ui_egui::render::Slot::Thumb(photo);
        let wanted = h.host.library.as_ref().unwrap().renderer.wanted(slot).unwrap();
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.thumb(photo).is_some_and(|t| t.key == wanted));
        eprintln!("overwrite {ext}: real host confirm → refreshed grid {:?}", started.elapsed());
        let thumb = h.host.library.as_ref().unwrap().widgets.iter().find(|(w, _)| w == &format!("thumb:{}", photo.0)).unwrap().1;
        let pixel = h.pixel_at(thumb.center());
        assert!(pixel.r() > 230 && pixel.g() < 80 && pixel.b() < 80, "new grid: {pixel:?}");
        h.key(egui::Key::D, egui::Modifiers::NONE);
        assert_eq!(h.host.library.as_ref().unwrap().ui.view, ViewMode::Detail);
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.textures.get(&lightcraft_ui_egui::render::Slot::Main).is_some_and(|t| t.photo == photo));
        let image = h.host.library.as_ref().unwrap().image_rect.unwrap();
        let pixel = h.pixel_at(image.center());
        assert!(pixel.r() > 230 && pixel.g() < 80 && pixel.b() < 80, "new loupe: {pixel:?}");
        h.host.editor.session.undo();
        assert!(h.host.editor.has_unsaved(), "Compositing history survives overwrite");
        assert_eq!(std::fs::read(&path).unwrap(), new, "Compositing undo never writes to disk");
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overwrite_original_jpeg_png_pixels_trash_atomic_reload_and_history() {
        for ext in ["png", "jpg"] {
            overwrite_pixels(ext, None);
        }
    }

    #[test]
    fn overwrite_original_cancel_and_dont_ask_again_persist() {
        let (dir, path, photo, mut h) = overwrite_host("overwrite-confirm", "png");
        let stored: Arc<Mutex<Option<String>>> = Arc::default();
        let (read, write) = (stored.clone(), stored.clone());
        h.host.editor.services.load_prefs = Some(Box::new(move || read.lock().unwrap().clone()));
        h.host.editor.services.save_prefs = Some(Box::new(move |s| {
            *write.lock().unwrap() = Some(s.into());
            Ok(())
        }));
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&photo));
        let old = std::fs::read(&path).unwrap();
        let doc = h.host.editor.session.active().unwrap().doc.clone();
        let undo = catalog(&h.host).undo.len();
        h.click("button:roundTripOverwrite");
        click_overwrite(&mut h, "overwriteDontAsk");
        click_overwrite(&mut h, "overwriteCancel");
        assert_eq!(std::fs::read(&path).unwrap(), old);
        assert!(Arc::ptr_eq(&doc, &h.host.editor.session.active().unwrap().doc));
        assert_eq!(catalog(&h.host).undo.len(), undo);
        assert!(!dir.join("Trash").exists());
        assert!(!h.host.editor.session.prefs().dialogs.contains_key("file.saveOverOriginal"));
        h.click("button:roundTripOverwrite");
        click_overwrite(&mut h, "overwriteDontAsk");
        click_overwrite(&mut h, "overwriteConfirm");
        // The background preferences writer persists in every hosted module.
        for _ in 0..4 {
            h.step(vec![]);
        }
        let read = stored.clone();
        let reloaded = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            photocraft_ui_egui::Services { load_prefs: Some(Box::new(move || read.lock().unwrap().clone())), ..Default::default() },
        );
        assert_eq!(reloaded.session.prefs().dialogs["file.saveOverOriginal"]["dontAskAgain"], true);
        h.host.editor.run("edit.fill", json!({"color": [0.0, 1.0, 0.0, 1.0]})).unwrap();
        photocraft_ui_egui::overwrite_original::request(&mut h.host.editor, doc.id, false).unwrap();
        assert!(h.host.editor.overwrite_prompt.is_none());
        assert!(dir.join("Trash/files/photo.png.2").exists());
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overwrite_original_raw_disabled_and_failed_trash_preserves_edit() {
        let (dir, path, photo, mut h) = overwrite_host("overwrite-raw-failure", "png");
        let raw = dir.join("photo.ARW");
        std::fs::write(&raw, b"raw original").unwrap();
        h.host
            .editor
            .session
            .edit("raw source", |doc, _| {
                let photocraft_doc::LayerContent::Smart(s) = &mut doc.layers[0].content else { panic!("Develop layer") };
                s.source = photocraft_doc::SmartSource::Linked { path: raw.to_string_lossy().into() };
                Ok(())
            })
            .unwrap();
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&photo));
        h.click("button:roundTripOverwrite");
        assert!(h.host.editor.overwrite_prompt.is_none(), "disabled card cannot be activated");
        let item = photocraft_ui_egui::menus::menu_items(&h.host.editor).into_iter().find(|i| i.id == photocraft_ui_egui::overwrite_original::SAVE_ID).unwrap();
        assert!(!item.enabled);
        assert_eq!(item.tooltip.as_deref(), Some("Raw files can't be overwritten; save a copy instead"));
        assert_eq!(std::fs::read(&raw).unwrap(), b"raw original");
        h.host.editor.session.undo();
        h.host.editor.services.overwrite_original = Some(Box::new(|doc, path, q| {
            crate::services::overwrite_original(doc, std::path::Path::new(path), q, |_| Err(std::io::Error::other("test Trash unavailable")))
        }));
        let old = std::fs::read(&path).unwrap();
        let undo = catalog(&h.host).undo.len();
        h.click("button:roundTripOverwrite");
        click_overwrite(&mut h, "overwriteConfirm");
        assert_eq!(std::fs::read(&path).unwrap(), old);
        assert_eq!(catalog(&h.host).undo.len(), undo);
        assert!(h.host.editor.has_unsaved());
        assert!(h.host.editor.ui.status.contains("test Trash unavailable"));
        assert!(h.host.library.as_ref().unwrap().host_composites.contains_key(&photo));
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overwrite_original_notice_saves_its_document_when_another_tab_is_active() {
        let (dir, path, photo, mut h) = overwrite_host("overwrite-other-tab", "png");
        let target = h.host.editor.session.active().unwrap().doc.id;
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&photo));
        h.host.editor.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        let other = h.host.editor.session.active().unwrap().doc.clone();
        h.click("button:roundTripOverwrite");
        assert_eq!(h.host.editor.overwrite_prompt.as_ref().unwrap().doc, target);
        click_overwrite(&mut h, "overwriteConfirm");
        assert!(Arc::ptr_eq(&other, &h.host.editor.session.active().unwrap().doc));
        let state = h.host.editor.session.documents().iter().find(|d| d.doc.id == target).unwrap();
        assert!(!state.is_dirty());
        assert!(h.host.library.as_ref().unwrap().host_composites.is_empty());
        let img = photocraft_codecs::decode(&std::fs::read(&path).unwrap())
            .unwrap()
            .convert(photocraft_codecs::ChannelLayout::Rgb, photocraft_codecs::SampleType::U8);
        assert!(img.data().as_chunks::<3>().0.iter().all(|p| p[0] > 240 && p[1] < 12 && p[2] < 12));
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn overwrite_original_gpu_real_canvas_and_library_reload() {
        use eframe::{egui_wgpu, wgpu};
        let setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
        let egui_wgpu::WgpuSetup::CreateNew(new) = &setup else { panic!("GPU setup") };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: new.instance_descriptor.backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
        let rs = block_on(egui_wgpu::RenderState::create(&config, &instance, None, Default::default())).expect("real GPU required");
        let info = rs.adapter.get_info();
        assert_eq!(info.backend, wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
        eprintln!("overwrite GPU: {} ({:?})", info.name, info.backend);
        overwrite_pixels("png", Some(rs));
    }

    #[test]
    fn background_develop_round_trip_keeps_switches_free_of_render_work() {
        use photocraft_engine::develop_layer_cmds::{gate_next_work, work_count_on_thread};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (dir, path, lib, photo) = library_with_photo("background-develop");
        let mut host = host_with_round_trip(lib, photo, &path);
        host.editor.session = photocraft_engine::Session::new();
        host.editor.background_jobs = true;
        host.mode = Module::Library;
        host.library.as_mut().unwrap().run("library.select", json!({"ids": [photo.0]})).unwrap();
        let mut h = RoundTripUi::new(host);
        h.step(vec![]);
        let gate = Arc::new(AtomicBool::new(false));
        gate_next_work(gate.clone());
        let count = work_count_on_thread();
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        assert_eq!(work_count_on_thread(), count, "the module switch never develops on the UI thread");
        assert!(h.host.editor.session.documents().is_empty());
        assert_eq!(h.host.editor.jobs.opens.len(), 1);
        gate.store(true, Ordering::Relaxed);
        h.wait_for(|host| !host.editor.session.has_jobs());
        assert_eq!(h.host.editor.session.documents().len(), 1);
        assert!(h.host.editor.jobs.opens.is_empty());
        let initial = h.host.editor.session.active().unwrap().doc.clone();
        let steps = h.host.editor.session.active().unwrap().history.past_len();
        h.key(egui::Key::Num2, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.host.library.as_mut().unwrap().run("develop.set", json!({"control": "light.exposure", "value": 1.0})).unwrap();
        let gate = Arc::new(AtomicBool::new(false));
        gate_next_work(gate.clone());
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        assert_eq!(work_count_on_thread(), count);
        assert!(Arc::ptr_eq(&h.host.editor.session.active().unwrap().doc, &initial), "old pixels until the worker finishes");
        assert!(h.host.editor.jobs.opens.is_empty(), "already-open path keeps the document visible");
        gate.store(true, Ordering::Relaxed);
        h.wait_for(|host| !host.editor.session.has_jobs());
        assert_eq!(h.host.editor.session.active().unwrap().history.past_len(), steps + 1);
        assert_ne!(&*h.host.editor.session.active().unwrap().doc, &*initial);
        h.host.editor.run("layer.new.layer", json!({})).unwrap();
        let before = h.host.editor.session.active().unwrap().doc.layers.clone();
        let steps = h.host.editor.session.active().unwrap().history.past_len();
        h.key(egui::Key::Num2, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.host.library.as_mut().unwrap().run("develop.set", json!({"control": "light.exposure", "value": 2.0})).unwrap();
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|host| !host.editor.session.has_jobs());
        let state = h.host.editor.session.active().unwrap();
        assert_eq!(&state.doc.layers[..2], &before);
        assert_eq!(state.doc.layers[2].name, "Develop 2");
        assert_eq!(state.history.past_len(), steps + 1);
        assert_eq!(work_count_on_thread(), count);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The interactive path from the task's benchmark, using a real source without a window.
    #[test]
    #[ignore = "set LOCAL_IMAGE_TEST_RAW to a corpus raw file"]
    fn bench_develop_host_switch() {
        let path = std::env::var("LOCAL_IMAGE_TEST_RAW").expect("LOCAL_IMAGE_TEST_RAW");
        let dir = std::env::temp_dir().join(format!("li-develop-host-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, Default::default());
        for typical in [false, true] {
            let mut session = lightcraft_engine::Session::new().with_fs();
            session.open_library(dir.join(if typical { "typical" } else { "plain" }), false).unwrap();
            session.xmp.auto_write = false; // the corpus source is read-only
            let r = session.execute("library.import", &json!({"paths": [path]})).unwrap();
            let photo = r["imported"][0].as_u64().unwrap();
            session.execute("library.select", &json!({"ids": [photo]})).unwrap();
            session.execute("develop.reset", &json!({})).unwrap();
            let base = if typical {
                json!({"light": {"exposure": 0.3, "highlights": -40.0, "shadows": 30.0},
                       "effects": {"clarity": 15.0, "texture": 10.0, "dehaze": 10.0},
                       "detail": {"nr_luminance": 30.0, "nr_color": 25.0, "sharpen_amount": 40.0}})
            } else {
                json!({})
            };
            session.execute("develop.merge", &json!({"settings": base})).unwrap();
            let library = LightcraftApp::new(session, Default::default());
            let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            editor.host_modes = true;
            editor.background_jobs = true;
            let mut host = Host::new(editor);
            host.library = Some(library);
            host.mode = Module::Library;
            let count = photocraft_engine::develop_layer_cmds::work_count_on_thread();
            let t = std::time::Instant::now();
            host.switch(&ctx, Module::Compositing);
            let dispatch = t.elapsed();
            let id = host.editor.session.jobs()[0].id;
            host.editor.session.wait_job(id).unwrap();
            photocraft_ui_egui::jobs_ui::tick_background(&mut host.editor, &ctx);
            let complete = t.elapsed();
            // The return arm is the real Host::switch path; preparation is excluded from timing.
            host.mode = Module::Develop;
            host.library.as_mut().unwrap().run("develop.set", json!({"control": "light.exposure", "value": 0.5})).unwrap();
            let t = std::time::Instant::now();
            host.switch(&ctx, Module::Compositing);
            let sync_dispatch = t.elapsed();
            let id = host.editor.session.jobs()[0].id;
            host.editor.session.wait_job(id).unwrap();
            photocraft_ui_egui::jobs_ui::tick_background(&mut host.editor, &ctx);
            let sync_complete = t.elapsed();
            assert_eq!(photocraft_engine::develop_layer_cmds::work_count_on_thread(), count);
            assert_eq!(host.editor.session.active().unwrap().history.past_len(), 1);
            eprintln!(
                "Host::switch typical={typical}: open dispatch {dispatch:?} / complete {complete:?}; sync dispatch {sync_dispatch:?} / complete {sync_complete:?}"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn roundtrip_edited_pixels(render_state: Option<eframe::egui_wgpu::RenderState>) {
        use lightcraft_ui_egui::state::{BeforeAfter, ViewMode};
        let (dir, path, lib, orig) = library_with_photo(if render_state.is_some() { "gpu-pixels" } else { "cpu-pixels" });
        let mut host = host_with_round_trip(lib, orig, &path);
        // Open through the module switch, as the owner does, rather than injecting a composite.
        host.editor.session = photocraft_engine::Session::new();
        let mode = if render_state.is_some() { photocraft_engine::prefs::RenderingMode::Gpu } else { photocraft_engine::prefs::RenderingMode::Cpu };
        host.editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(mode));
        // The probe reads the same renderer resources that set_wgpu installs for the app.
        let probe = render_state.as_ref().map(photocraft_ui_egui::gpu_canvas::GpuCanvas::new);
        if let Some(rs) = &render_state {
            host.editor.set_wgpu(rs.clone());
            host.library.as_mut().unwrap().ui.settings.gpu = true;
        }
        host.mode = Module::Library;
        host.library.as_mut().unwrap().run("library.select", json!({"ids": [orig.0]})).unwrap();
        let mut h = RoundTripUi::new(host);
        h.frame.wgpu_render_state = render_state;
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.thumb(orig).is_some());
        let thumb = h.host.library.as_ref().unwrap().widgets.iter().find(|(w, _)| w == &format!("thumb:{}", orig.0)).unwrap().1;
        let original = h.pixel_at(thumb.center());
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        h.host.editor.run("layer.new.layer", json!({})).unwrap();
        h.host.editor.run("edit.fill", json!({"color": [1.0, 0.0, 0.0, 1.0]})).unwrap();
        h.step(vec![]);
        if let Some(gpu) = &probe {
            assert!(h.host.editor.gpu_active());
            let d = &h.host.editor.session.active().unwrap().doc;
            // Check the actual GPU canvas before the host hands the composite to Library.
            let (_, _, pixels) = gpu.read_texels(d.id.0).expect("GPU canvas readback");
            assert!(pixels.iter().all(|p| p[0] > 0.95 && p[1] < 0.05 && p[2] < 0.05));
        } else {
            assert!(!h.host.editor.gpu_active());
            assert_eq!(mode, photocraft_engine::prefs::RenderingMode::Cpu);
        }
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&orig));
        let lib = h.host.library.as_ref().unwrap();
        assert_eq!(lib.host_composites[&orig].label, "Edited in Compositing · unsaved");
        assert!(matches!(h.host.round_trip_notice, Some(RoundTripNotice::Unsaved { .. })));
        assert!(lib.widgets.iter().any(|(w, _)| w == "button:roundTripSave"));
        let edited = h.pixel_at(thumb.center());
        assert!(edited.r() > 240 && edited.g() < 10 && edited.b() < 10, "grid: {edited:?}");
        assert_ne!(edited, original);
        // A remembered Before view must not hide the composite in Library.
        h.host.library.as_mut().unwrap().ui.before_after = BeforeAfter::Original;
        h.click(&format!("thumb:{}", orig.0));
        h.key(egui::Key::D, egui::Modifiers::NONE);
        assert_eq!(h.host.library.as_ref().unwrap().ui.view, ViewMode::Detail);
        let image = h.host.library.as_ref().unwrap().image_rect.unwrap();
        let edited = h.pixel_at(image.center());
        assert!(edited.r() > 240 && edited.g() < 10 && edited.b() < 10, "loupe: {edited:?}");
        assert_eq!(h.host.library.as_ref().unwrap().loupe_shown, Some((orig, "compositing")));
        // Return, save using the real editor shortcut, and switch back normally.
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.key(egui::Key::S, egui::Modifiers::COMMAND);
        let saved = dir.join("photo-Edit.psd");
        assert!(saved.is_file());
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        let edit = photo_at_path(catalog(&h.host), &saved.to_string_lossy()).unwrap();
        assert_eq!(catalog(&h.host).active(), Some(edit), "return to the saved composite");
        assert_eq!(catalog(&h.host).catalog.stack_of(edit).unwrap().photos, vec![edit, orig]);
        assert!(h.host.library.as_ref().unwrap().host_composites.is_empty());
        assert!(
            matches!(&h.host.round_trip_notice, Some(RoundTripNotice::Saved(msg)) if msg.contains(saved.to_str().unwrap()) && msg.contains("stacked with photo.png"))
        );
        assert!(h.host.editor.ui.notices.iter().any(|n| n.title.contains(saved.to_str().unwrap()) && n.title.contains("stacked")));
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.thumb(edit).is_some());
        let thumb =
            h.host.library.as_ref().unwrap().widgets.iter().find(|(w, _)| w == &format!("thumb:{}", edit.0)).expect("the saved edit is visible in the grid").1;
        let pixel = h.pixel_at(thumb.center());
        assert!(pixel.r() > 230 && pixel.g() < 120 && pixel.b() < 140, "saved grid: {pixel:?}");
        assert_ne!(pixel, original);
        h.key(egui::Key::D, egui::Modifiers::NONE);
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.textures.get(&lightcraft_ui_egui::render::Slot::Main).is_some_and(|t| t.photo == edit));
        let image = h.host.library.as_ref().unwrap().image_rect.unwrap();
        let pixel = h.pixel_at(image.center());
        // PSD import colour-converts the stored composite; the filled red layer still dominates.
        assert!(pixel.r() > 230 && pixel.g() < 120 && pixel.b() < 140, "saved loupe: {pixel:?}");
        assert_ne!(pixel, original);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hosted_titlebars_drag_captions_and_top_resize_in_every_theme_and_size() {
        use egui::{Event, PointerButton, ViewportCommand};
        for size in [egui::vec2(1800.0, 1000.0), egui::vec2(1280.0, 800.0), egui::vec2(2560.0, 1440.0)] {
            for mode in [Module::Library, Module::Develop] {
                for theme in photocraft_ui_egui::theme::ThemeKind::ALL {
                    let (dir, path, lib, orig) = library_with_photo("titlebar-tests");
                    let mut host = host_with_round_trip(lib, orig, &path);
                    host.editor.custom_titlebar = true;
                    host.editor.ui.theme = theme;
                    host.editor.run("prefs.set", json!({"values": {"interface.theme": theme.id()}})).unwrap();
                    // Include the recently added CPU-mode pill in the right-hand controls.
                    host.editor.perf.gpu_info.lost = Some("test graphics failure".into());
                    host.mode = mode;
                    set_library_view(host.library.as_mut().unwrap(), mode == Module::Develop);
                    let mut h = RoundTripUi::new(host);
                    for _ in 0..6 {
                        h.step_at(size, vec![]);
                    }
                    let gap = h.view.ctx.viewport(|v| {
                        v.prev_pass
                            .widgets
                            .layers()
                            .flat_map(|(_, ws)| ws.iter())
                            .find(|w| w.rect.top() == 0.0 && w.sense.senses_click() && w.sense.senses_drag())
                            .expect("the title bar's drag response")
                            .rect
                    });
                    assert!(gap.width() > 100.0, "{mode:?}, {theme:?}, {size:?}: {gap:?}");
                    let height = if photocraft_ui_egui::theme::Tokens::get(&h.view.ctx).pro { 32.0 } else { 38.0 };
                    assert_eq!(photocraft_ui_egui::theme::Tokens::get(&h.view.ctx).kind, theme);
                    assert_eq!(gap.bottom(), height - 1.0, "the panel leaves one pixel for its bottom separator");
                    let captions = [0u8, 1, 2].map(|c| h.view.ctx.read_response(egui::Id::new(("titlebar-caption", c))).expect("caption button present").rect);
                    for rect in captions {
                        assert_eq!(rect.top(), 0.0);
                        assert_eq!(rect.height(), height);
                        assert!(rect.width() >= 40.0);
                        assert!(rect.left() >= gap.right());
                    }
                    assert_eq!(captions[2].right(), size.x);
                    let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
                    let gesture = |h: &mut RoundTripUi, from, to| {
                        let mut cmds = h.step_at(size, vec![Event::PointerMoved(from)]);
                        cmds.extend(h.step_at(size, vec![button(from, true)]));
                        cmds.extend(h.step_at(size, vec![Event::PointerMoved(to)]));
                        cmds.extend(h.step_at(size, vec![button(to, false)]));
                        cmds
                    };
                    // Both ends and the middle of the free span, including the painted title.
                    for x in [gap.left() + 2.0, gap.center().x, gap.right() - 2.0] {
                        let p = egui::pos2(x, gap.center().y);
                        let cmds = gesture(&mut h, p, p + egui::vec2(20.0, 8.0));
                        assert_eq!(cmds.iter().filter(|c| **c == ViewportCommand::StartDrag).count(), 1, "{mode:?}, {theme:?}, {size:?}: {cmds:?}");
                    }
                    let p = gap.center();
                    let mut cmds = gesture(&mut h, p, p);
                    cmds.extend(gesture(&mut h, p, p));
                    assert!(cmds.contains(&ViewportCommand::Maximized(true)), "{cmds:?}");
                    assert!(gesture(&mut h, captions[0].center(), captions[0].center()).contains(&ViewportCommand::Minimized(true)));
                    assert!(gesture(&mut h, captions[1].center(), captions[1].center()).contains(&ViewportCommand::Maximized(true)));
                    let p = egui::pos2(gap.center().x, 1.0);
                    let cmds = gesture(&mut h, p, p + egui::vec2(20.0, 8.0));
                    assert_eq!(cmds, [ViewportCommand::BeginResize(egui::ResizeDirection::North)]);
                    assert!(gesture(&mut h, captions[2].center(), captions[2].center()).contains(&ViewportCommand::Close));
                    drop(h);
                    std::fs::remove_dir_all(dir).unwrap();
                }
            }
        }
    }

    /// Top 120 pixels of each saved full-window render, in ThemeKind::ALL order.
    fn titlebar_overviews(out: &std::path::Path) {
        for (w, h) in [(1800u32, 1000u32), (1280, 800), (2560, 1440)] {
            for mode in Module::ALL {
                let mut sheet = image::RgbImage::new(w, 600);
                for (row, theme) in photocraft_ui_egui::theme::ThemeKind::ALL.into_iter().enumerate() {
                    let img = image::open(out.join(format!("probe-{mode:?}-{}-{w}x{h}.png", theme.id()))).unwrap().to_rgb8();
                    let top = image::imageops::crop_imm(&img, 0, 0, w, 120).to_image();
                    image::imageops::overlay(&mut sheet, &top, 0, row as i64 * 120);
                }
                sheet.save(out.join(format!("overview-{mode:?}-{w}x{h}.png"))).unwrap();
            }
        }
    }

    #[test]
    #[ignore = "writes the full host screenshot matrix to target/titlebar; run explicitly for visual QA"]
    fn render_titlebars_probe() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/titlebar");
        std::fs::create_dir_all(&out).unwrap();
        for (w, h) in [(1800.0f32, 1000.0f32), (1280.0, 800.0), (2560.0, 1440.0)] {
            for mode in Module::ALL {
                for theme in photocraft_ui_egui::theme::ThemeKind::ALL {
                    let (dir, path, lib, orig) = library_with_photo("tb");
                    let mut host = host_with_round_trip(lib, orig, &path);
                    host.editor.custom_titlebar = true;
                    host.editor.ui.theme = theme;
                    host.editor.run("prefs.set", json!({"values": {"interface.theme": theme.id()}})).unwrap();
                    host.mode = mode;
                    set_library_view(host.library.as_mut().unwrap(), mode == Module::Develop);
                    let mut ui = RoundTripUi::new(host);
                    for _ in 0..6 {
                        ui.step_at(egui::vec2(w, h), vec![]);
                    }
                    let img = ui.view.paint(&ui.host.library.as_ref().unwrap().renderer.cpu_textures());
                    let buf: Vec<u8> = img.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
                    image::RgbImage::from_raw(w as u32, h as u32, buf).unwrap().save(out.join(format!("probe-{mode:?}-{}-{w}x{h}.png", theme.id()))).unwrap();
                    drop(ui);
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
        }
        titlebar_overviews(&out);
        if out.join("before").is_dir() {
            titlebar_overviews(&out.join("before"));
        }
    }

    #[test]
    fn roundtrip_cpu_grid_loupe_and_save_shortcut_show_the_edited_pixels() {
        roundtrip_edited_pixels(None);
    }

    /// No new executor dependency: wake the polling thread for adapter/device creation.
    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        struct WakeThread(std::thread::Thread);
        impl std::task::Wake for WakeThread {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(WakeThread(std::thread::current())));
        let mut cx = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::park(),
            }
        }
    }

    #[test]
    fn roundtrip_gpu_grid_loupe_and_save_shortcut_show_the_edited_pixels() {
        use eframe::{egui_wgpu, wgpu};
        // Some installed Vulkan drivers crash during enumeration inside a device-less sandbox.
        // Skip before loading those drivers; on the coordinator's GPU host enumerate normally.
        #[cfg(target_os = "linux")]
        if !std::path::Path::new("/dev/dri").is_dir() && !std::path::Path::new("/dev/nvidiactl").exists() {
            eprintln!("skipped: no GPU adapter for Compositing → Library round trip");
            return;
        }
        let setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
        let egui_wgpu::WgpuSetup::CreateNew(new) = &setup else { panic!("expected GPU setup") };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: new.instance_descriptor.backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = block_on(instance.enumerate_adapters(new.instance_descriptor.backends));
        if !adapters.iter().any(|a| matches!(a.get_info().device_type, wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu)) {
            eprintln!("skipped: no GPU adapter for Compositing → Library round trip");
            return;
        }
        let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
        let rs = block_on(egui_wgpu::RenderState::create(&config, &instance, None, Default::default())).unwrap();
        let info = rs.adapter.get_info();
        eprintln!("round-trip GPU test: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
        assert!(matches!(info.device_type, wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu));
        roundtrip_edited_pixels(Some(rs));
    }

    #[test]
    fn roundtrip_notice_actions_keep_editing_retry_save_and_dismiss() {
        let (dir, path, lib, orig) = library_with_photo("notice-actions");
        let mut host = host_with_round_trip(lib, orig, &path);
        host.editor.run("edit.fill", json!({"color": [1.0, 0.0, 0.0, 1.0]})).unwrap();
        let doc = host.editor.session.active().unwrap().doc.id;
        let mut h = RoundTripUi::new(host);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.click("button:roundTripEdit");
        assert_eq!(h.host.mode, Module::Compositing);
        assert_eq!(h.host.editor.session.active().unwrap().doc.id, doc);
        assert!(h.host.editor.has_unsaved());
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        for _ in 0..4 {
            h.step(vec![]);
        }
        let writer = h.host.editor.services.write.take();
        h.host.editor.services.write = Some(Box::new(|_, _| Err("test disk full".into())));
        h.click("button:roundTripSave");
        assert!(h.host.editor.has_unsaved());
        assert_eq!(catalog(&h.host).catalog.len(), 1);
        assert!(matches!(h.host.round_trip_notice, Some(RoundTripNotice::Unsaved { .. })));
        assert!(h.host.library.as_ref().unwrap().ui.toast.as_ref().unwrap().0.contains("test disk full"));
        h.host.editor.services.write = writer;
        h.host.editor.session.open_document(
            photocraft_doc::Document::new("Other tab", photocraft_doc::Size::new(4, 4), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8),
            None,
        );
        h.click("button:roundTripSave");
        assert_eq!(h.host.editor.session.active().unwrap().doc.id, doc, "Save targets the notice's document");
        assert_eq!(h.host.mode, Module::Library);
        assert!(!h.host.editor.has_unsaved());
        let edit = photo_at_path(catalog(&h.host), &dir.join("photo-Edit.psd").to_string_lossy()).unwrap();
        assert_eq!(catalog(&h.host).active(), Some(edit));
        assert!(h.host.library.as_ref().unwrap().host_composites.is_empty());
        for _ in 0..4 {
            h.step(vec![]);
        }
        assert!(matches!(&h.host.round_trip_notice, Some(RoundTripNotice::Saved(msg)) if msg.contains("photo-Edit.psd") && msg.contains("stacked")));
        h.click("button:roundTripDismiss");
        assert!(h.host.round_trip_notice.is_none());
        // Editing the saved PSD, saving the preview from Library with ⌘S, reloads in place.
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.host.editor.run("edit.fill", json!({"color": [0.0, 1.0, 0.0, 1.0]})).unwrap();
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.key(egui::Key::S, egui::Modifiers::COMMAND);
        assert!(!h.host.editor.has_unsaved());
        assert_eq!(catalog(&h.host).catalog.len(), 2);
        assert_eq!(catalog(&h.host).active(), Some(edit));
        assert!(matches!(&h.host.round_trip_notice, Some(RoundTripNotice::Saved(msg)) if msg.contains("updated") && msg.contains("photo-Edit.psd")));
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn roundtrip_single_layer_edits_are_previewed_and_closing_drops_the_preview() {
        let (dir, path, lib, orig) = library_with_photo("single-layer");
        let mut host = host_with_round_trip(lib, orig, &path);
        // A clean Develop layer needs no unsaved stand-in.
        host.editor.session = photocraft_engine::Session::new();
        develop_layer::open_photo(&mut host.editor, orig.0, &path, &json!({}), "photo.png").unwrap();
        assert!(unsaved_composites(&host.editor, catalog(&host)).is_empty());
        host.editor.run("layer.setProps", json!({"opacity": 0.5})).unwrap();
        let mut h = RoundTripUi::new(host);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&orig));
        assert_eq!(h.host.editor.session.active().unwrap().doc.layers.len(), 1);
        assert!(matches!(h.host.round_trip_notice, Some(RoundTripNotice::Unsaved { .. })));
        h.click("button:roundTripEdit");
        h.host.editor.session.close(0);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert!(h.host.library.as_ref().unwrap().host_composites.is_empty());
        assert!(h.host.round_trip_notice.is_none());
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The Library checks every unsaved Compositing document against the catalog each frame
    /// (`unsaved_composites`): on a big library that must stay cheap (it cloned every photo's path
    /// per frame) and still find the photo by its exact path, or by another spelling of it.
    #[test]
    fn unsaved_composites_on_a_big_library_find_the_photo_cheaply() {
        let dir = std::env::temp_dir().join(format!("li-bigcat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = lightcraft_engine::Session::new();
        let n = 50_000u64;
        for i in 1..=n {
            let path = format!("/photos/shoot/IMG_{i:05}.CR3");
            let p = lightcraft_catalog::Photo::new(
                lightcraft_catalog::PhotoId(i),
                lightcraft_catalog::Source::File { path },
                "IMG.CR3",
                "cr3",
                6000,
                4000,
                "2026-10-09",
            );
            s.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(p) }).unwrap();
        }
        let real = dir.join("edit.psd");
        std::fs::write(&real, b"x").unwrap();
        let p = lightcraft_catalog::Photo::new(
            lightcraft_catalog::PhotoId(n + 1),
            lightcraft_catalog::Source::File { path: real.to_string_lossy().into() },
            "edit.psd",
            "psd",
            4,
            4,
            "2026-10-09",
        );
        s.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(p) }).unwrap();
        let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), photocraft_ui_egui::Services::default());
        let doc = || photocraft_doc::Document::new("d", photocraft_doc::Size::new(4, 4), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
        // the photo by its exact path, another spelling of it, and a file the Library doesn't have
        let other_spelling = dir.join(".").join("edit.psd").to_string_lossy().to_string();
        for path in [real.to_string_lossy().to_string(), other_spelling, "/elsewhere/new.psd".to_string()] {
            editor.session.open_document(doc(), Some(path));
            editor.session.active_mut().unwrap().revision += 1;
        }
        assert_eq!(photo_at_path(&s, &real.to_string_lossy()), Some(lightcraft_catalog::PhotoId(n + 1)));
        assert_eq!(photo_at_path(&s, &dir.join(".").join("edit.psd").to_string_lossy()), Some(lightcraft_catalog::PhotoId(n + 1)));
        let t = std::time::Instant::now();
        let reps = 20;
        for _ in 0..reps {
            let found = unsaved_composites(&editor, &s);
            assert_eq!(found.len(), 1, "both spellings are the same photo, shown once; the unknown file is skipped");
        }
        eprintln!("unsaved_composites, 3 dirty documents, {n} photos: {:.2} ms/frame", t.elapsed().as_secs_f64() * 1000.0 / reps as f64);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
