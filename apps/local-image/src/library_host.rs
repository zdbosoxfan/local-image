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
use photocraft_ui_egui::dialog_filter::{FileDialogExt as _, file_dialog};
use photocraft_ui_egui::{Module, PhotocraftApp};

/// Files the Library asked the editor to open (Edit in External Editor with no other editor set).
type Opens = Arc<Mutex<Vec<String>>>;
type SaveSourcesKey = (String, u64, u64, Vec<photocraft_ui_egui::develop_layer::LibraryOrigin>);

/// One theme authority: reuse editor semantic colours while retaining Library metrics.
fn library_palette(kind: photocraft_ui_egui::theme::ThemeKind) -> lightcraft_ui_egui::theme::Tokens {
    use lightcraft_ui_egui::theme::{Tokens, readable_text, text_on};
    let palette = photocraft_ui_egui::theme::Tokens::for_kind(kind);
    let text = readable_text(palette.text, palette.pressed);
    Tokens {
        dark: palette.dark(),
        selection: palette.pressed,
        selection_text: text_on(palette.pressed),
        chrome: palette.chrome,
        canvas: palette.canvas,
        grid_bg: palette.canvas,
        cell: palette.card,
        cell_selected: palette.hover,
        divider: palette.separator,
        inset: palette.dock,
        field: palette.field,
        field_border: palette.field_border,
        button: palette.card,
        button_border: palette.card_border,
        hover: palette.hover,
        pressed: palette.pressed,
        tool_active: palette.pressed,
        text,
        text_label: text,
        text_dim: readable_text(palette.text_dim, palette.chrome),
        text_disabled: palette.text_faint,
        icon: palette.icon,
        track: palette.text_dim,
        thumb: palette.icon,
        thumb_hover: text,
        accent: palette.accent,
        star: text,
        pick: text,
        reject: readable_text(palette.danger, palette.chrome),
        caution: readable_text(palette.warning, palette.chrome),
        ..Tokens::default()
    }
}

pub struct Host {
    pub editor: PhotocraftApp,
    library: Option<LightcraftApp>,
    prefs: PrefsWriter,
    mode: Module,
    /// The module whose visuals are applied to the context (Library and Develop share LightCraft's).
    styled: Option<(bool, photocraft_ui_egui::theme::ThemeKind)>,
    opens: Opens,
    preserve_library_vectorize_return: bool,
    frames: u64,
    /// The Library's active photo when Compositing was last left: coming back with the same photo
    /// returns to what was open there instead of bringing the photo forward again.
    left_compositing_with: Option<u64>,
    library_seen: Option<(String, u64)>,
    save_sources_seen: Option<SaveSourcesKey>,
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

type CompositeRender = (u64, u64, Result<photocraft_raster::Rgba8Image, String>);

/// What a Library photo looks like in Compositing, rendered on worker threads for the Library's
/// loupe and grid (`lightcraft_ui_egui::panels::host_composite`).
#[derive(Default)]
struct Composites {
    /// Photo → key of the render in flight.
    pending: std::collections::HashMap<u64, u64>,
    /// Failed revisions are reported once; a new edit can retry.
    failed: std::collections::HashMap<u64, u64>,
    /// Finished renders: (photo, key, image).
    done: Arc<Mutex<Vec<CompositeRender>>>,
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
            preserve_library_vectorize_return: false,
            frames: 0,
            left_compositing_with: None,
            library_seen: None,
            save_sources_seen: None,
            camera_raw: None,
            composites: Composites::default(),
            round_trip_notice: None,
        }
    }

    /// One persisted shortcut map and one editor across modules; Library only mirrors it.
    fn shared_shell(&mut self, ctx: &egui::Context) {
        photocraft_ui_egui::prefs_ui::set_host_shortcut_catalog(&mut self.editor, ctx, lightcraft_ui_egui::shortcuts::catalog::host_catalog());
        let request = self.library.as_mut().is_some_and(|lib| std::mem::take(&mut lib.open_shared_shortcuts));
        if request {
            let id = photocraft_ui_egui::prefs_ui::open_shortcuts(&mut self.editor, 0);
            if let Some(dialog) = self.editor.ui.dialogs.iter_mut().find(|d| d.id == id) {
                dialog.fields.insert("shortcutScope".into(), serde_json::json!("library"));
            }
        }
        if let Some(lib) = self.library.as_mut() {
            lib.hosted_menus = true;
            lib.shared_shortcuts = true;
            lib.host_modal = !self.editor.ui.dialogs.is_empty();
            lib.shortcut_overrides = self
                .editor
                .session
                .prefs()
                .shortcut_overrides(true)
                .iter()
                .filter(|(id, _)| id.starts_with("library::"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            lib.shortcut_overrides.insert("library::__profile".into(), self.editor.session.prefs().shortcut_profiles.library.parent().into());
        }
    }

    fn library(&mut self) -> &mut LightcraftApp {
        let opens = self.opens.clone();
        let mode = self.mode;
        let theme = self.editor.ui.theme;
        let library = self.library.get_or_insert_with(|| {
            let mut lib = open_library_app(opens, &mut self.prefs);
            if mode == Module::Develop {
                set_library_view(&mut lib, true);
            }
            lib
        });
        library.set_host_theme(library_palette(theme));
        library.hosted_menus = true;
        library.shared_shortcuts = true;
        library.host_modal = !self.editor.ui.dialogs.is_empty();
        library.shortcut_overrides = self
            .editor
            .session
            .prefs()
            .shortcut_overrides(true)
            .iter()
            .filter(|(id, _)| id.starts_with("library::"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        library.shortcut_overrides.insert("library::__profile".into(), self.editor.session.prefs().shortcut_profiles.library.parent().into());
        library
    }

    fn refresh_save_sources(&mut self) {
        use photocraft_ui_egui::develop_layer;
        let mut sources = Vec::new();
        let origins: Vec<_> = self
            .editor
            .session
            .documents()
            .iter()
            .flat_map(|state| state.doc.walk().into_iter().filter_map(|(_, _, layer)| develop_layer::layer_origin(layer)))
            .collect();
        let key = self
            .library
            .as_ref()
            .map(|lib| (lib.session.library_identity().to_string(), lib.session.library_generation, lib.session.catalog.revision, origins.clone()));
        if key == self.save_sources_seen {
            return;
        }
        if let Some(lib) = &self.library {
            for origin in origins {
                if let Some(source) = capture_save_source(&lib.session, &origin) {
                    sources.push(source);
                }
            }
        }
        *self.editor.library_save_sources.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = sources;
        self.save_sources_seen = key;
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

    fn observe_library(&mut self) {
        self.refresh_save_sources();
        let current = self.library.as_ref().map(|l| (l.session.library_identity().to_string(), l.session.library_generation));
        if self.library_seen == current {
            return;
        }
        if self.library_seen.is_some() {
            self.left_compositing_with = None;
            self.round_trip_notice = None;
            self.composites.pending.clear();
            self.composites.failed.clear();
            self.composites.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
            if let Some(lib) = self.library.as_mut() {
                lib.host_composites.clear();
            }
            // These jobs captured the old Library's source/settings. Other editing/AI jobs retain ownership.
            let ids: Vec<_> =
                self.editor.session.jobs().iter().filter(|j| matches!(j.command.as_str(), "develop.openPhoto" | "develop.syncPhoto")).map(|j| j.id).collect();
            for id in ids {
                self.editor.session.cancel_job(id);
            }
        }
        self.library_seen = current;
    }

    fn open_editor_photo(&mut self, photo: u64, path: &str, settings: &serde_json::Value, name: &str) -> Result<(), String> {
        self.observe_library();
        let identity = self.library().session.library_identity().to_string();
        let result = photocraft_ui_egui::develop_layer::open_photo_in(&mut self.editor, &identity, photo, path, settings, name);
        self.refresh_save_sources();
        result
    }

    fn focus_library_document(&mut self, photo: u64) -> bool {
        let index = self.library.as_ref().and_then(|lib| document_of(&self.editor, &lib.session, lightcraft_catalog::PhotoId(photo)));
        if let Some(index) = index {
            let Some(document) = self.editor.session.documents().get(index).map(|state| state.doc.id) else {
                return false;
            };
            if !self.editor.session.set_active(index) {
                return false;
            }
            self.editor.sync_views();
            // Supplied by the separately coordinated editor admission helper.
            photocraft_ui_egui::toolsets_ui::persona::photo_admitted(&mut self.editor, document);
            true
        } else {
            false
        }
    }

    /// Files saved in Compositing: the Library adds the ones that came from its photos (stacked on
    /// the original) and reloads the ones it has; Save and Return to Library then shows it.
    /// Without `ctx` (at exit) nothing is shown.
    fn take_library_saves(&mut self, ctx: Option<&egui::Context>) {
        self.refresh_save_sources();
        for save in std::mem::take(&mut self.editor.library_saves) {
            // An unopened library has none of the files; one opens for a photo from it.
            if self.library.is_none() && (save.photo.is_none() || ctx.is_none()) {
                continue;
            }
            let lib = self.library();
            if save.photo.is_some() && !save.origin.as_ref().is_some_and(|origin| Some(origin.photo) == save.photo && matches_origin(&lib.session, origin)) {
                self.editor.ui.status = format!("Saved {}, but its source belongs to another or unlinked Library; switch to that Library to add it", save.path);
                self.editor.ui.status_error = true;
                let detail = self.editor.ui.status.clone();
                self.round_trip_notice = Some(RoundTripNotice::Error(detail.clone()));
                photocraft_ui_egui::notices::complete_file_result(
                    &mut self.editor,
                    save.notice,
                    format!("Saved {}; Library update failed", photocraft_ui_egui::file_open::display_name(&save.path)),
                    detail,
                    true,
                );
                continue;
            }
            let update = if let Some(reset) = &save.source_reset {
                add_replaced_source(&mut lib.session, &save.path, reset)
            } else {
                add_saved_file(&mut lib.session, &save.path, save.photo, save.overwritten)
            };
            let (id, msg) = match update {
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
                    let detail = self.editor.ui.status.clone();
                    self.round_trip_notice = Some(RoundTripNotice::Error(detail.clone()));
                    photocraft_ui_egui::notices::complete_file_result(
                        &mut self.editor,
                        save.notice,
                        format!("Saved {}; Library update failed", photocraft_ui_egui::file_open::display_name(&save.path)),
                        detail,
                        true,
                    );
                    continue;
                }
            };
            if (save.overwritten || save.source_reset.is_some())
                && let Some(id) = id
            {
                let settings: Vec<_> = self
                    .library()
                    .session
                    .catalog
                    .photos()
                    .filter(|p| p.id == id || p.copy_of == Some(id))
                    .map(|p| (p.id.0, serde_json::to_value(&p.develop).unwrap_or_default()))
                    .collect();
                for (photo, settings) in settings {
                    if let Some((identity, path)) = self.library.as_ref().and_then(|lib| source_binding(&lib.session, photo)) {
                        self.editor.session.acknowledge_library_overwrite(&identity, photo, &path, &settings);
                    }
                }
                photocraft_ui_egui::prefs_ui::refresh_recovery(&mut self.editor);
            }
            if !msg.is_empty() {
                self.editor.ui.status = msg.clone();
                self.editor.ui.status_error = false;
                self.round_trip_notice = Some(RoundTripNotice::Saved(msg.clone()));
                let title = format!("{} {}", if save.overwritten { "Overwrote" } else { "Saved" }, photocraft_ui_egui::file_open::display_name(&save.path));
                let notice = photocraft_ui_egui::notices::complete_file_result(&mut self.editor, save.notice, title, msg.clone(), false);
                self.editor.ui.status_error =
                    self.editor.ui.notices.iter().find(|entry| entry.id == notice).is_some_and(|entry| entry.warning_count > 0 || entry.error);
            }
            let Some(ctx) = ctx.filter(|_| save.show) else { continue };
            self.switch(ctx, Module::Library);
            if let Some(id) = id {
                let lib = self.library();
                let _ = lib.run("library.select", serde_json::json!({ "ids": [id.0], "active": id.0 }));
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
                match img {
                    Ok(img) => {
                        self.composites.failed.remove(&photo);
                        let label = lightcraft_ui_egui::i18n::tr("Edited in Compositing · unsaved").to_string();
                        host_composite::set(lib, ctx, lightcraft_catalog::PhotoId(photo), key, img.width as usize, img.height as usize, &img.pixels, label);
                    }
                    Err(e) => {
                        self.composites.failed.insert(photo, key);
                        lib.host_composites.remove(&lightcraft_catalog::PhotoId(photo));
                        self.editor.ui.status = format!("Couldn't update the Library preview: {e}");
                        self.editor.ui.status_error = true;
                        let detail = self.editor.ui.status.clone();
                        self.round_trip_notice = Some(RoundTripNotice::Error(detail.clone()));
                        photocraft_ui_egui::notices::complete_file_result(&mut self.editor, None, "Library preview failed".into(), detail, true);
                    }
                }
            }
            if self.composites.pending.get(&photo) == Some(&key) {
                self.composites.pending.remove(&photo);
            }
        }
        let ids: Vec<lightcraft_catalog::PhotoId> = want.iter().map(|w| w.0).collect();
        host_composite::retain(lib, &ids);
        self.composites.pending.retain(|p, _| ids.iter().any(|id| id.0 == *p));
        self.composites.failed.retain(|p, k| want.iter().any(|(id, key, _)| id.0 == *p && key == k));
        for (id, key, doc) in want {
            if host_composite::key(lib, id) == Some(key) || self.composites.pending.get(&id.0) == Some(&key) || self.composites.failed.get(&id.0) == Some(&key)
            {
                continue;
            }
            self.composites.pending.insert(id.0, key);
            let (done, ctx) = (self.composites.done.clone(), ctx.clone());
            std::thread::spawn(move || {
                let img = photocraft_engine::color_cmds::composite_to_srgb(&doc, photocraft_compose::thumbnail_buffer(&doc, COMPOSITE_EDGE))
                    .map(|buf| buf.to_rgba8())
                    .map_err(|e| e.to_string());
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
                photocraft_ui_egui::notices::complete_file_result(&mut self.editor, None, "Couldn't save the layered copy".into(), e, true);
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
            && consume_host_chord(ctx, egui::Modifiers::COMMAND, egui::Key::S)
        {
            self.save_preview(ctx, doc);
        }
    }

    fn round_trip_notice_ui(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let ctx = &context;
        if self.mode != Module::Library {
            return;
        }
        let Some(notice) = self.round_trip_notice.clone() else { return };
        // Completed operations share the editor's compact result row and full history.
        if !matches!(notice, RoundTripNotice::Unsaved { .. }) {
            return;
        }
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
        egui::Panel::bottom("library-round-trip-notice").exact_size(96.0).show(ui, |ui| {
            egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.button_border)).corner_radius(6.0).inner_margin(12).show(ui, |ui| {
                match &notice {
                    RoundTripNotice::Unsaved { destination, .. } => {
                        ui.label(egui::RichText::new("Edited in Compositing · unsaved").strong());
                        ui.add(
                            egui::Label::new(format!(
                                "Save {} as a layered copy, or overwrite the original.",
                                photocraft_ui_egui::file_open::display_name(destination)
                            ))
                            .truncate(),
                        )
                        .on_hover_text(destination);
                    }
                    RoundTripNotice::Saved(message) | RoundTripNotice::Error(message) => {
                        ui.label(message);
                    }
                }
                ui.horizontal_wrapped(|ui| {
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
                    photocraft_ui_egui::notices::complete_file_result(&mut self.editor, None, "Couldn't overwrite the original".into(), e, true);
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
            if let Some((identity, path)) = self.library.as_ref().and_then(|lib| source_binding(&lib.session, id)) {
                photocraft_ui_egui::develop_layer::sync_in(&mut self.editor, &identity, id, &path, &settings);
            }
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
        self.observe_library();
        if self.mode == to || !self.editor.host_modes {
            return;
        }
        // Leaving a Camera Raw Filter session with the module switch cancels it.
        if self.camera_raw.is_some() {
            self.end_camera_raw(ctx, false);
            return;
        }
        let from = self.mode;
        if from == Module::Compositing
            && to == Module::Develop
            && photocraft_ui_egui::develop_layer::active_photo(&self.editor).is_some()
            && !photocraft_ui_egui::develop_layer::active_origin(&self.editor).is_some_and(|origin| matches_origin(&self.library().session, &origin))
        {
            self.editor.ui.status = "This Develop layer belongs to another or legacy Library; open the correct Library before returning to Develop".into();
            self.editor.ui.status_error = true;
            return;
        }
        let preserve_library = from == Module::Compositing
            && to == Module::Library
            && (self.preserve_library_vectorize_return || self.editor.library_vectorize.preserves_library_state(&self.editor.session));
        if from == Module::Compositing {
            let mut handoff = std::mem::take(&mut self.editor.library_vectorize);
            handoff.cancel(&mut self.editor);
            self.editor.library_vectorize = handoff;
            self.editor.trace_dialog.cancel(&mut self.editor.session);
            photocraft_ui_egui::vector_integration::cancel(&mut self.editor);
        }
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
                .then(|| {
                    let doc = self.editor.session.active()?;
                    let lib = self.library.as_ref()?;
                    if !document_in_library(&doc.doc, &lib.session) {
                        return None;
                    }
                    doc.path.as_deref().and_then(|p| photo_at_path(&lib.session, p))
                })
                .flatten()
                .map(|p| p.0);
            if !preserve_library
                && let Some(id) = saved.or_else(|| {
                    photocraft_ui_egui::develop_layer::active_origin(&self.editor)
                        .filter(|origin| self.library.as_ref().is_some_and(|lib| matches_origin(&lib.session, origin)))
                        .map(|origin| origin.photo)
                })
            {
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
                if !preserve_library {
                    set_library_view(lib, develop);
                }
            }
            Module::Compositing => {
                self.sync_develop_layers();
                let photo = self.active_photo();
                let changed = photo.as_ref().map(|p| p.0) != self.left_compositing_with || self.editor.session.documents().is_empty();
                if let Some((id, path, settings, name)) = photo.filter(|_| changed || from == Module::Develop)
                    // a file open in Compositing (a saved `-Edit.psd`) comes forward as it is
                    && (from == Module::Develop || !self.focus_library_document(id))
                    && let Err(e) = self.open_editor_photo(id, &path, &settings, &name)
                {
                    self.editor.ui.status = e;
                    self.editor.ui.status_error = true;
                }
            }
        }
        if to == Module::Library {
            self.preserve_library_vectorize_return = false;
        }
        self.styled = None;
        ctx.request_repaint();
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        let library = self.mode != Module::Compositing;
        let theme = self.editor.ui.theme;
        let key = (library, theme);
        // Do not cache a missing lazy Library: its first logic initializes styles.
        if library && self.library.is_none() {
            self.styled = None;
            return;
        }
        if self.styled == Some(key) {
            return;
        }
        // Keep editor tokens current for the shared title/caption controls.
        self.editor.restyle(ctx);
        if let Some(app) = self.library.as_mut() {
            // Hidden Library screenshots should also use the current shared palette.
            app.set_host_theme(library_palette(theme));
            if library {
                app.restyle(ctx);
            }
        }
        self.styled = Some(key);
    }

    /// ⌘⌥1 / ⌘⌥2 / ⌘⌥3 (Ctrl+Alt elsewhere) pick a module from anywhere.
    fn module_keys(&mut self, ctx: &egui::Context) {
        if !self.editor.host_modes {
            return;
        }
        let picked = Module::ALL.into_iter().find(|m| consume_host_chord(ctx, egui::Modifiers::COMMAND | egui::Modifiers::ALT, m.key()));
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
fn source_binding(s: &lightcraft_engine::Session, photo: u64) -> Option<(String, String)> {
    let photo = s.catalog.photo(lightcraft_catalog::PhotoId(photo))?;
    let lightcraft_catalog::Source::File { path } = &photo.source else { return None };
    Some((s.library_identity().to_string(), path.clone()))
}

fn matches_origin(s: &lightcraft_engine::Session, origin: &photocraft_ui_egui::develop_layer::LibraryOrigin) -> bool {
    origin.library.as_deref() == Some(s.library_identity())
        && s.catalog
            .photo(lightcraft_catalog::PhotoId(origin.photo))
            .is_some_and(|photo| matches!(&photo.source, lightcraft_catalog::Source::File { path } if *path == origin.path))
}

fn document_in_library(doc: &photocraft_doc::Document, s: &lightcraft_engine::Session) -> bool {
    let origins: Vec<_> = doc.walk().iter().filter_map(|(_, _, l)| photocraft_ui_egui::develop_layer::layer_origin(l)).collect();
    origins.is_empty() || origins.iter().any(|o| o.library.as_deref() == Some(s.library_identity()))
}

fn document_of(editor: &PhotocraftApp, s: &lightcraft_engine::Session, id: lightcraft_catalog::PhotoId) -> Option<usize> {
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
fn unsaved_composites(editor: &PhotocraftApp, s: &lightcraft_engine::Session) -> Vec<(lightcraft_catalog::PhotoId, u64, Arc<photocraft_doc::Document>)> {
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

fn capture_save_source(
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

fn add_replaced_source(
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
fn add_saved_file(s: &mut lightcraft_engine::Session, path: &str, original: Option<u64>, overwritten: bool) -> Result<Option<LibraryUpdate>, String> {
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

/// Which module the Library app's own navigation is in (it can enter its develop tools itself)./// These are exact host actions, not egui's more permissive subset-modifier match.
fn consume_host_chord(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events
            .retain(|event| !matches!(event,egui::Event::Key{key:pressed,pressed:true,modifiers:mods,..} if *pressed==key && mods.matches_exact(modifiers)));
        before != i.events.len()
    })
}

fn library_module(lib: &LightcraftApp) -> Module {
    if lib.ui.view == lightcraft_ui_egui::state::ViewMode::Detail && lib.ui.right.is_edit_tool() { Module::Develop } else { Module::Library }
}

impl eframe::App for Host {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.observe_library();
        self.frames += 1;
        self.editor.current_module = self.mode;
        self.shared_shell(ctx);
        // A file dialog that couldn't open on this system (no desktop portal, no zenity) is
        // explained where the user asked for it, not taken for Cancel.
        if let Some(why) = photocraft_ui_egui::dialog_filter::take_unavailable() {
            log::warn!("{why}");
            match self.library.as_mut() {
                Some(lib) if self.mode != Module::Compositing => lib.toast_error(ctx, why),
                _ => photocraft_ui_egui::notices::error(&mut self.editor, why),
            }
        }
        if self.editor.ui.dialogs.is_empty() {
            let library_keys_available = self
                .library
                .as_ref()
                .is_none_or(|lib| lib.ui.dialog.is_none() && lib.ui.people.queue.is_none() && lib.ui.people.finder.is_none() && lib.ui.people.merge.is_none());
            if self.mode != Module::Compositing
                && ctx.input(|input| input.focused)
                && !egui::Popup::is_any_open(ctx)
                && library_keys_available
                && consume_host_chord(ctx, egui::Modifiers::COMMAND, egui::Key::K)
            {
                photocraft_ui_egui::prefs_ui::open_preferences(&mut self.editor, "general");
            }
            self.module_keys(ctx);
            self.preview_save_key(ctx);
        }
        // The editor installs the fonts (a superset of the Library's) on its first frame.
        if self.frames == 1 || self.mode == Module::Compositing {
            self.editor.logic(ctx, frame);
        } else {
            self.editor.background_tick(ctx);
            // Native menus belong to the host, including while the Library owns the canvas.
            // The editor's normal logic/UI are dormant here, so drain clicks and sync explicitly.
            photocraft_ui_egui::native_menu::run(&mut self.editor, ctx);
            photocraft_ui_egui::native_menu::sync(&mut self.editor, ctx);
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
        if let Some(request) = lightcraft_ui_egui::vectorize_handoff::take_request(ctx) {
            let checked = request.check(self.library());
            let mut handoff = std::mem::take(&mut self.editor.library_vectorize);
            let started = checked.and_then(|()| handoff.open_path(&mut self.editor, request.photo.0, &request.path));
            self.editor.library_vectorize = handoff;
            match started {
                Ok(()) => {
                    photocraft_ui_egui::vector_integration::cancel(&mut self.editor);
                    self.mode = Module::Compositing;
                    self.editor.current_module = Module::Compositing;
                    self.preserve_library_vectorize_return = true;
                    remember_module(Module::Compositing);
                    self.styled = None;
                    ctx.request_repaint();
                }
                Err(error) => {
                    self.library().toast_error(ctx, error.clone());
                    self.editor.open_failed(&photocraft_ui_egui::file_open::display_name(&request.path), &error);
                }
            }
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
        if let Some(origin) = self.editor.develop_request.take() {
            if matches_origin(&self.library().session, &origin) {
                let id = origin.photo;
                self.left_compositing_with = Some(id);
                let lib = self.library();
                let _ = lib.run("library.select", serde_json::json!({ "ids": [id], "active": id }));
                self.mode = Module::Compositing;
                self.switch(ctx, Module::Develop);
            } else {
                self.editor.ui.status = "This Develop layer belongs to another or legacy Library; reopen its photo from the correct Library".into();
                self.editor.ui.status_error = true;
            }
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
                    let auxiliary = lib.ui.view == lightcraft_ui_egui::state::ViewMode::Detail
                        && matches!(lib.ui.right, lightcraft_ui_egui::state::RightPanel::Versions | lightcraft_ui_egui::state::RightPanel::Activity);
                    let now = if auxiliary { self.mode } else { library_module(lib) };
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
            (Module::Library | Module::Develop, Some(lib)) => {
                if let Some(menu) = self.editor.services.native_menu.as_mut() {
                    menu.raw_input(raw);
                }
                lib.raw_input_hook(raw);
            }
            _ => self.editor.raw_input_hook(ctx, raw),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.shared_shell(ui.ctx());
        if self.mode == Module::Compositing || self.library.is_none() {
            self.editor.ui(ui, frame);
            return;
        }
        let title = match &self.camera_raw {
            Some(cr) => format!("Camera Raw Filter · {}", cr.name),
            None => self.library.as_ref().map(|l| library_title(l, self.mode)).unwrap_or_default(),
        };
        let modal = !self.editor.ui.dialogs.is_empty();
        let Some(library) = self.library.as_mut() else {
            self.editor.ui(ui, frame);
            return;
        };
        let picked = photocraft_ui_egui::panels::library_title_bar_with_menus(&mut self.editor, ui, &title, self.mode, |ui, room| {
            if library.native_menu {
                return ui.cursor().left();
            }
            let start = ui.cursor().left();
            start + ui.add_enabled_ui(!modal, |ui| lightcraft_ui_egui::menubar::show_in_window(library, ui, room)).inner
        });
        self.shared_shell(ui.ctx());
        if let Some(m) = picked.filter(|_| !modal) {
            let ctx = ui.ctx().clone();
            self.switch(&ctx, m);
            ctx.request_repaint();
        }
        self.round_trip_notice_ui(ui);
        let dismissed = photocraft_ui_egui::notices::panel(&mut self.editor, ui);
        if matches!(self.round_trip_notice, Some(RoundTripNotice::Saved(_) | RoundTripNotice::Error(_))) {
            if let Some(rect) = photocraft_ui_egui::notices::control_rect(ui.ctx(), "dismiss") {
                lightcraft_ui_egui::widgets::register(ui.ctx(), "button:roundTripDismiss", rect);
            }
            if dismissed {
                self.round_trip_notice = None;
            }
        }
        if let Some(lib) = self.library.as_mut() {
            ui.add_enabled_ui(!lib.host_modal, |ui| lib.ui(ui));
        }
        self.shared_shell(ui.ctx());
        photocraft_ui_egui::dialogs::show(&mut self.editor, ui.ctx());
        photocraft_ui_egui::notices::show(&mut self.editor, ui.ctx());
        // The switched-to-CPU warning appears in every module, not only once Compositing is open.
        photocraft_ui_egui::gpu_status::show_fallback(&mut self.editor, ui.ctx());
        photocraft_ui_egui::overwrite_original::show(&mut self.editor, ui.ctx());
        if let Some(lib) = self.library.as_mut() {
            lib.widgets.extend(lightcraft_ui_egui::widgets::take_registry(ui.ctx()));
        }
        photocraft_ui_egui::panels::host_resize_zones(&self.editor, ui);
    }

    fn on_exit(&mut self) {
        let mut handoff = std::mem::take(&mut self.editor.library_vectorize);
        handoff.cancel(&mut self.editor);
        self.editor.library_vectorize = handoff;
        self.editor.trace_dialog.cancel(&mut self.editor.session);
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

/// Reopen the last module; a new profile starts in Library.
fn last_module() -> Module {
    module_preference(config_dir().and_then(|d| std::fs::read_to_string(d.join("mode")).ok()).as_deref())
}

fn module_preference(value: Option<&str>) -> Module {
    match value.map(str::trim) {
        Some("develop") => Module::Develop,
        Some("editor") => Module::Compositing,
        _ => Module::Library,
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
fn read_library_prefs(path: &std::path::Path) -> Result<(UiState, Vec<String>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let parsed =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string()).and_then(|v| photocraft_engine::prefs::lenient::<UiState>(v).map_err(|e| e.to_string()));
    if parsed.as_ref().map_or(true, |(_, rejected)| !rejected.is_empty()) {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let backup = path.with_file_name(format!("library-ui.json.corrupt-{stamp}"));
        lightcraft_catalog::safe_file::write_atomic(&backup, &bytes)
            .map_err(|e| lightcraft_ui_egui::i18n::tr("couldn't preserve damaged Library settings: {error}").replace("{error}", &e.to_string()))?;
    }
    parsed.and_then(|(ui, rejected)| {
        if rejected.iter().any(|key| key.is_empty() || key == "settings" || key == "settings.libraryPath") {
            return Err(lightcraft_ui_egui::i18n::tr("the stored Library path could not be recovered").into());
        }
        Ok((ui.sanitized(), rejected))
    })
}

fn load_prefs(writer: &mut PrefsWriter) -> Option<UiState> {
    if std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
        return None;
    }
    let path = config_dir()?.join("library-ui.json");
    if !path.exists() {
        return None;
    }
    match read_library_prefs(&path) {
        Ok((ui, rejected)) => {
            if !rejected.is_empty() {
                writer.notice = Some(
                    lightcraft_ui_egui::i18n::tr("Some Library settings used defaults: {fields}. A copy of the damaged settings was kept.")
                        .replace("{fields}", &rejected.join(", ")),
                );
            }
            Some(ui)
        }
        Err(e) => {
            writer.blocked = true;
            writer.notice = Some(
                lightcraft_ui_egui::i18n::tr(
                    "Library settings couldn't be read: {error}. Choose your existing Library folder. Your stored settings will not be replaced.",
                )
                .replace("{error}", &e),
            );
            None
        }
    }
}

/// Saves `library-ui.json` when it changed (checked every few seconds) and at exit.
#[derive(Default)]
struct PrefsWriter {
    written: Vec<u8>,
    checked: f64,
    blocked: bool,
    notice: Option<String>,
}

impl PrefsWriter {
    fn save(&mut self, app: &LightcraftApp) -> Result<(), String> {
        if self.blocked && (app.library_problem.is_some() || app.ui.settings.library_path.is_empty()) {
            return Ok(());
        }
        if cfg!(test) || std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
            return Ok(());
        }
        let Some(d) = config_dir() else { return Ok(()) };
        let bytes = serde_json::to_vec_pretty(&app.ui).map_err(|e| e.to_string())?;
        if bytes == self.written {
            return Ok(());
        }
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        lightcraft_catalog::safe_file::write_atomic(&d.join("library-ui.json"), &bytes).map_err(|e| format!("saving the Library settings failed: {e}"))?;
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
    let prefs = load_prefs(prefs_writer);
    let unopened = || Session::new().with_fs().with_system_clock();
    let (mut session, problem) = match if prefs_writer.blocked { None } else { library_dir(prefs.as_ref()) } {
        None if prefs_writer.blocked => (unopened(), Some(LibraryProblem::new("", prefs_writer.notice.as_deref().unwrap_or("Choose your Library folder.")))),
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
    if let Some(notice) = prefs_writer.notice.take() {
        app.toast_error(&egui::Context::default(), notice);
    }
    prefs_writer.written.clear();
    app
}

/// Library › Add Photos dialog extensions: everything the folder scan imports
/// ([`lightcraft_engine::import::EXTENSIONS`]) plus `heif`. On Linux they also match upper and
/// mixed case (`DSC_0001.NEF`, `IMG_0001.JPG`): see [`photocraft_ui_egui::dialog_filter`].
const ADD_PHOTOS_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "rwl", "raw", "pef", "psd", "jxl", "gif",
    "bmp", "heic", "heif", "avif",
];

fn services(opens: Opens) -> Services {
    Services {
        trash_file: li_ai::trash::available()
            .then(|| Box::new(|path: &str| li_ai::trash::move_to_trash(std::path::Path::new(path)).map(|_| ()).map_err(|e| e.to_string())) as _),
        save_denoise_file: Some(Box::new(|name| {
            crate::file_dialog_probe::explain_empty(file_dialog().set_file_name(name).add_filter_nocase("Denoised image", &["tiff", "tif", "dng"]).save_file())
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
        model_installed: Some(Box::new(|id| {
            // Denoise presence is metadata-only; inference workers still verify weights.
            li_seg::spec(id).is_some_and(|spec| {
                matches!(spec.task, li_seg::Task::Denoise { .. }) && li_seg::installed_bytes(&photocraft_engine::seg::models_dir(), spec).is_some()
            })
        })),
        model_download_status: Some(Box::new(|id| {
            photocraft_ui_egui::ai_ui::downloads()
                .get(&format!("seg:{id}"))
                .map(|dl| lightcraft_ui_egui::ModelDownloadStatus { running: !dl.finished, done: dl.done, total: dl.total, error: dl.error.clone() })
                .unwrap_or_default()
        })),
        pick_folder: Some(Box::new(|| {
            crate::file_dialog_probe::explain_empty(file_dialog().set_title("Choose Folder").pick_folder()).map(|p| p.to_string_lossy().to_string())
        })),
        pick_folder_titled: Some(Box::new(|title| {
            crate::file_dialog_probe::explain_empty(file_dialog().set_title(lightcraft_ui_egui::i18n::tr(title)).pick_folder())
                .map(|p| p.to_string_lossy().to_string())
        })),
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
            crate::file_dialog_probe::explain_empty(file_dialog().add_filter_nocase("Photos", ADD_PHOTOS_EXTS).pick_files())
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })),
        pick_preset_files: Some(Box::new(|| {
            crate::file_dialog_probe::explain_empty(
                file_dialog()
                    .set_title("Import Presets")
                    .add_filter_nocase("Presets & Profiles", &["lcpreset", "xmp", "lrtemplate", "zip", "dng", "lmp", "mplumpack", "cube"])
                    .pick_files(),
            )
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
        })),
        pick_tracklog: Some(Box::new(|| {
            crate::file_dialog_probe::explain_empty(file_dialog().set_title("Auto-Tag from Tracklog").add_filter_nocase("GPS Track Log", &["gpx"]).pick_file())
                .map(|p| vec![p.to_string_lossy().to_string()])
                .unwrap_or_default()
        })),
        save_preset_file: Some(Box::new(|name: &str| {
            crate::file_dialog_probe::explain_empty(
                file_dialog().set_title("Export Presets").add_filter_nocase("Preset", &["lcpreset"]).set_file_name(name).save_file(),
            )
            .map(|p| p.to_string_lossy().to_string())
        })),
        pick_curve_preset_files: Some(Box::new(|| {
            crate::file_dialog_probe::explain_empty(
                file_dialog().set_title("Import Point Curve Presets").add_filter_nocase("Point Curve Presets", &["lccurve", "json"]).pick_files(),
            )
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
        })),
        save_curve_preset_file: Some(Box::new(|name: &str| {
            crate::file_dialog_probe::explain_empty(
                file_dialog().set_title("Export Point Curve Presets").add_filter_nocase("Point Curve Presets", &["lccurve"]).set_file_name(name).save_file(),
            )
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
    #[test]
    fn bugfix_develop_startup_defaults_to_library_and_restores_develop() {
        assert_eq!(super::module_preference(None), super::Module::Library);
        assert_eq!(super::module_preference(Some("develop")), super::Module::Develop);
        assert_eq!(super::module_preference(Some("editor")), super::Module::Compositing);
        assert_eq!(super::module_preference(Some("unknown")), super::Module::Library);
    }
    use super::*;

    /// LightCraft e2d35de: Library › Add Photos offers everything the folder scan imports and
    /// shows DSC_0001.NEF / IMG_0001.JPG / .CR3 on Linux.
    #[test]
    fn add_photos_filter_covers_import_extensions_in_any_case() {
        use photocraft_ui_egui::dialog_filter::{dialog_extensions_for, glob_ext};
        let pats = dialog_extensions_for(ADD_PHOTOS_EXTS, true);
        for e in lightcraft_engine::import::EXTENSIONS {
            assert!(ADD_PHOTOS_EXTS.contains(e), "Add Photos is missing .{e}");
        }
        for e in ADD_PHOTOS_EXTS {
            assert!(pats.iter().any(|p| p == e), "{e}");
            assert!(pats.iter().any(|p| glob_ext(p, &e.to_uppercase())), "{e}");
        }
        for name in ["NEF", "JPG", "CR3", "Nef", "jPg"] {
            assert!(pats.iter().any(|p| glob_ext(p, name)), "{name}");
        }
    }

    #[test]
    fn bugfix_save_o009_overwrite_resets_master_virtual_copy_and_its_develop_link() {
        use photocraft_ui_egui::develop_layer::LibrarySave;
        use serde_json::json;
        let (dir, path, mut library, master) = library_with_photo("bugfix-overwrite-copy");
        library.run("develop.set", json!({"control": "light.exposure", "value": 1.0})).unwrap();
        let copy = library.run("photo.virtualCopy", json!({})).unwrap()["ids"][0].as_u64().unwrap();
        library.run("library.select", json!({"ids": [copy]})).unwrap();
        library.run("develop.set", json!({"control": "light.exposure", "value": 0.5})).unwrap();
        let mut host = host_with_round_trip(library, lightcraft_catalog::PhotoId(copy), &path);
        image::RgbImage::from_pixel(40, 30, image::Rgb([128, 64, 32])).save(&path).unwrap();
        host.editor.library_saves.push(LibrarySave {
            notice: None,
            origin: photocraft_ui_egui::develop_layer::active_origin(&host.editor),
            path,
            photo: Some(copy),
            show: false,
            overwritten: true,
            source_reset: None,
        });
        host.take_library_saves(None);
        let defaults = lightcraft_engine::develop::DevelopSettings::default();
        for id in [master, lightcraft_catalog::PhotoId(copy)] {
            assert_eq!(*catalog(&host).catalog.photo(id).unwrap().develop, defaults);
        }
        let latest = host
            .editor
            .session
            .active()
            .unwrap()
            .doc
            .walk()
            .into_iter()
            .filter_map(|(_, _, layer)| {
                if let photocraft_doc::LayerContent::Smart(smart) = &layer.content {
                    smart.develop.as_ref().filter(|link| link.photo == Some(copy))
                } else {
                    None
                }
            })
            .next_back()
            .unwrap();
        assert_eq!(latest.library_baseline.as_deref(), Some(&serde_json::to_value(defaults).unwrap()));
        assert_eq!(latest.settings["light"]["exposure"], 0.5, "render settings still belong to the preserved source");
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn settle_background_save(host: &mut Host) {
        let ctx = egui::Context::default();
        let started = std::time::Instant::now();
        while host.editor.session.has_jobs() {
            host.observe_library();
            photocraft_ui_egui::jobs_ui::tick_background(&mut host.editor, &ctx);
            assert!(started.elapsed() < std::time::Duration::from_secs(15), "background save did not settle");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        host.take_library_saves(None);
    }

    #[test]
    fn background_save_library_source_resets_copies_and_reopens_layered_baseline() {
        for ext in ["png", "tif"] {
            let (dir, path, mut library, master) = library_with_photo_format(&format!("background-source-{ext}"), ext);
            library.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
            let copy = library.run("photo.virtualCopy", json!({})).unwrap()["ids"][0].as_u64().unwrap();
            library.run("library.select", json!({"ids":[copy]})).unwrap();
            library.run("develop.set", json!({"control":"light.exposure","value":0.75})).unwrap();
            let original = std::fs::read(&path).unwrap();
            let mut host = host_with_round_trip(library, lightcraft_catalog::PhotoId(copy), &path);
            host.editor.services.prepare_save = Some(Arc::new(crate::services::prepare_document_save));
            host.editor.background_jobs = true;
            host.editor.session.edit_prefs(|p| p.file_handling.ask_before_saving_layered_tiff = false);
            host.editor.save_as(Some(path.clone())).unwrap();
            settle_background_save(&mut host);
            assert!(host.editor.ui.notices.iter().all(|notice| !notice.error), "{}", host.editor.ui.status);
            assert!(!host.editor.ui.status.contains("Trash"));
            assert!(!dir.join("Trash").exists());
            let defaults = lightcraft_engine::develop::DevelopSettings::default();
            for id in [master, lightcraft_catalog::PhotoId(copy)] {
                assert_eq!(*catalog(&host).catalog.photo(id).unwrap().develop, defaults);
            }
            let state = host.editor.session.active().unwrap();
            let check = |doc: &photocraft_doc::Document, stage: &str| {
                let photocraft_doc::LayerContent::Smart(smart) = &doc.layers[0].content else { panic!("Develop source lost") };
                if stage == "open document" {
                    assert!(matches!(&smart.source, photocraft_doc::SmartSource::Embedded { .. }), "current source was not frozen");
                }
                let bytes = match &smart.source {
                    photocraft_doc::SmartSource::Embedded { bytes, .. } => Some(bytes.as_ref().clone()),
                    photocraft_doc::SmartSource::Linked { path } => photocraft_io::linked::find_linked_file(&doc.metadata, path).map(|file| file.bytes),
                };
                assert!(bytes.as_deref() == Some(original.as_slice()), "{ext} {stage}: source table did not preserve exact original bytes");
                let link = smart.develop.as_ref().unwrap();
                assert_eq!(link.settings["light"]["exposure"], 0.75);
                assert_eq!(link.library_baseline.as_deref(), Some(&serde_json::to_value(&defaults).unwrap()));
            };
            check(&state.doc, "open document");
            if ext == "tif" {
                assert!(!state.is_dirty(), "catalog bookkeeping dirtied the saved TIFF");
                let reopened = photocraft_io::import(&path, &std::fs::read(&path).unwrap()).unwrap().document;
                check(&reopened, "cold reopen");
                assert_eq!(reopened.layers.len(), state.doc.layers.len());
            } else {
                assert!(state.is_dirty(), "a flat PNG doesn't save the editable document");
            }
            drop(host);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn background_save_library_newer_edits_prevent_source_publication() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (dir, path, mut library, photo) = library_with_photo("background-source-conflict");
        library.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut host = host_with_round_trip(library, photo, &path);
        let release = Arc::new(AtomicBool::new(false));
        let worker_release = release.clone();
        host.editor.services.prepare_save = Some(Arc::new(move |doc, path, settings, interrupt| {
            let started = std::time::Instant::now();
            while !worker_release.load(Ordering::Acquire) {
                if started.elapsed() > std::time::Duration::from_secs(10) {
                    return Err("test barrier timeout".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            crate::services::prepare_document_save(doc, path, settings, interrupt)
        }));
        host.editor.background_jobs = true;
        host.editor.save_as(Some(path.clone())).unwrap();
        host.library().run("develop.set", json!({"control":"light.exposure","value":1.0})).unwrap();
        host.observe_library();
        release.store(true, Ordering::Release);
        settle_background_save(&mut host);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(catalog(&host).catalog.photo(photo).unwrap().develop.light.exposure, 1.0);
        assert!(host.editor.ui.status_error);
        assert!(host.editor.ui.status.contains("changed during saving"), "{}", host.editor.ui.status);
        assert!(host.editor.library_saves.is_empty());
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn background_save_library_alias_uses_qualified_source_without_importing_duplicate() {
        let (dir, path, mut library, photo) = library_with_photo("background-source-alias");
        library.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
        let mut host = host_with_round_trip(library, photo, &path);
        let alias = dir.join("different-name.png");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        host.editor.services.prepare_save = Some(Arc::new(crate::services::prepare_document_save));
        host.editor.background_jobs = true;
        host.editor.save_as(Some(alias.to_string_lossy().into_owned())).unwrap();
        settle_background_save(&mut host);
        assert!(host.editor.ui.notices.iter().all(|notice| !notice.error), "{}", host.editor.ui.status);
        assert_eq!(catalog(&host).catalog.len(), 1);
        assert_eq!(catalog(&host).catalog.photo(photo).unwrap().develop.light.exposure, 0.0);
        assert!(alias.is_symlink());
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_save_library_rebound_master_invalidates_virtual_copy_capture() {
        let (dir, path, mut library, master) = library_with_photo("background-source-rebind");
        let copy = library.run("photo.virtualCopy", json!({})).unwrap()["ids"][0].as_u64().unwrap();
        let mut host = host_with_round_trip(library, lightcraft_catalog::PhotoId(copy), &path);
        let captured = host.editor.library_save_sources.lock().unwrap()[0].clone();
        let other = dir.join("other.png");
        std::fs::copy(&path, &other).unwrap();
        host.library()
            .session
            .catalog
            .apply(lightcraft_catalog::Op::SetFile {
                id: master,
                file_name: "other.png".into(),
                source: lightcraft_catalog::Source::File { path: other.to_string_lossy().into_owned() },
            })
            .unwrap();
        host.observe_library();
        assert!(!host.editor.library_save_sources.lock().unwrap().contains(&captured));
        assert!(add_replaced_source(&mut host.library().session, &path, &captured).is_err());
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_save_library_failed_reload_reports_saved_file_but_no_catalog_reset() {
        let (dir, path, mut library, photo) = library_with_photo("background-source-reload-failure");
        library.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
        let mut host = host_with_round_trip(library, photo, &path);
        let captured = host.editor.library_save_sources.lock().unwrap()[0].clone();
        host.library().session.media.file_probe = Some(Arc::new(|_| Err("injected unavailable file".into())));
        host.editor.library_saves.push(LibrarySave {
            notice: None,
            origin: Some(captured.origin.clone()),
            path,
            photo: Some(photo.0),
            show: false,
            overwritten: false,
            source_reset: Some(captured),
        });
        host.take_library_saves(None);
        assert!(host.editor.ui.status_error);
        assert!(host.editor.ui.status.contains("could not reload"));
        assert_eq!(catalog(&host).catalog.photo(photo).unwrap().develop.light.exposure, 0.5);
        assert!(matches!(host.round_trip_notice, Some(RoundTripNotice::Error(_))));
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_save_host_keyboard_cancel_retry_and_library_return() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (dir, path, library, photo) = library_with_photo("background-save-host-keys");
        let mut host = host_with_round_trip(library, photo, &path);
        let release = Arc::new(AtomicBool::new(false));
        let exited = Arc::new(AtomicBool::new(false));
        let worker_release = release.clone();
        let worker_exited = exited.clone();
        host.editor.services.prepare_save = Some(Arc::new(move |doc, path, settings, interrupt| {
            let start = std::time::Instant::now();
            while !worker_release.load(Ordering::Acquire) {
                if start.elapsed() > std::time::Duration::from_secs(10) {
                    return Err("test barrier timeout".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let result = crate::services::prepare_document_save(doc, path, settings, interrupt);
            worker_exited.store(true, Ordering::Release);
            result
        }));
        host.editor.background_jobs = true;
        let saved = dir.join("photo-Edit.psd");
        let mut h = RoundTripUi::new(host);
        h.step(vec![]);
        h.step(vec![]);
        h.key(egui::Key::S, egui::Modifiers::COMMAND);
        assert!(h.host.editor.session.has_jobs());
        assert!(!saved.exists());
        // Real Host frames and keyboard navigation stay live while encoding is held.
        let frames = h.frames;
        for _ in 0..3 {
            h.step(vec![]);
        }
        assert!(h.frames >= frames + 3);
        h.key(egui::Key::Escape, egui::Modifiers::NONE);
        release.store(true, Ordering::Release);
        h.wait_for(|host| !host.editor.session.has_jobs() && exited.load(Ordering::Acquire));
        h.step(vec![]);
        assert!(!saved.exists(), "Escape published cancelled bytes");
        assert!(h.host.editor.has_unsaved());
        let result = photocraft_ui_egui::menus::invoke(&mut h.host.editor, &h.view.ctx, develop_layer::SAVE_RETURN_ID, json!({})).unwrap();
        assert_eq!(result["pending"], true);
        assert_eq!(h.host.mode, Module::Compositing);
        h.wait_for(|host| !host.editor.session.has_jobs() && host.mode == Module::Library);
        assert!(saved.exists());
        assert!(!h.host.editor.has_unsaved());
        assert!(photo_at_path(catalog(&h.host), &saved.to_string_lossy()).is_some());
        let reopened = photocraft_io::import("photo-Edit.psd", &std::fs::read(&saved).unwrap()).unwrap().document;
        assert_eq!(reopened.layers.len(), h.host.editor.session.active().unwrap().doc.layers.len());
        assert!(h.host.editor.ui.notices.iter().all(|notice| !notice.error));
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
        host.open_editor_photo(photo.0, path, &settings, "photo.png").unwrap();
        host.editor.run("layer.new.layer", json!({})).unwrap();
        assert!(host.editor.has_unsaved());
        host
    }

    #[test]
    fn library_identity_prevents_cross_catalog_preview_save_and_document_reuse() {
        let (dir_a, path_a, library_a, a) = library_with_photo("identity-host-a");
        let (dir_b, path_b, library_b, b) = library_with_photo("identity-host-b");
        assert_eq!(a, b, "fixture needs colliding numeric photo IDs");
        let mut host = host_with_round_trip(library_a, a, &path_a);
        let doc_a = host.editor.session.active().unwrap().doc.clone();
        let old_origin = develop_layer::active_origin(&host.editor).unwrap();
        assert!(matches_origin(catalog(&host), &old_origin));
        host.library = Some(library_b);
        host.observe_library();
        assert!(!matches_origin(catalog(&host), &old_origin));
        assert!(document_of(&host.editor, catalog(&host), b).is_none());
        assert!(unsaved_composites(&host.editor, catalog(&host)).is_empty());
        let selection = catalog(&host).selection.clone();
        host.switch(&egui::Context::default(), Module::Develop);
        assert_eq!(host.mode, Module::Compositing, "direct module switch entered another photo");
        assert_eq!(catalog(&host).selection, selection);
        assert!(host.editor.ui.status_error);
        let before = serde_json::to_value(&catalog(&host).catalog).unwrap();
        let saved = dir_a.join("belongs-to-a.psd").to_string_lossy().to_string();
        host.editor.save_as(Some(saved.clone())).unwrap();
        host.take_library_saves(None);
        assert!(std::path::Path::new(&saved).is_file());
        assert_eq!(serde_json::to_value(&catalog(&host).catalog).unwrap(), before);
        assert!(host.editor.ui.status_error);
        host.open_editor_photo(b.0, &path_b, &json!({}), "b.png").unwrap();
        assert_eq!(host.editor.session.documents().len(), 2);
        assert!(Arc::ptr_eq(&host.editor.session.documents()[0].doc, &doc_a));
        let b_doc = host.editor.session.active().unwrap().doc.clone();
        let copy = host.library().run("photo.virtualCopy", json!({})).unwrap()["ids"][0].as_u64().unwrap();
        host.open_editor_photo(copy, &path_b, &json!({}), "copy.png").unwrap();
        assert_eq!(host.editor.session.documents().len(), 3);
        assert!(Arc::ptr_eq(&host.editor.session.documents()[1].doc, &b_doc));
        let origin = develop_layer::active_origin(&host.editor).unwrap();
        assert_eq!(origin.photo, copy);
        assert!(matches_origin(catalog(&host), &origin));
        drop(host);
        std::fs::remove_dir_all(dir_a).unwrap();
        std::fs::remove_dir_all(dir_b).unwrap();
    }

    #[test]
    fn library_identity_legacy_embedded_layer_cannot_enter_an_unrelated_develop_photo() {
        let (dir, path, library, photo) = library_with_photo("identity-legacy-embedded");
        let mut host = host_with_round_trip(library, photo, &path);
        let state = host.editor.session.active_mut().unwrap();
        let doc = Arc::make_mut(&mut state.doc);
        let layer = &mut doc.layers[0];
        let photocraft_doc::LayerContent::Smart(sm) = &mut layer.content else { panic!("Develop layer") };
        sm.source = photocraft_doc::SmartSource::Embedded { file_name: "legacy-photo.png".into(), bytes: Arc::new(std::fs::read(&path).unwrap()) };
        let link = sm.develop.as_mut().unwrap();
        link.library = None;
        link.source = None;
        state.active_layer = Some(layer.id);
        assert_eq!(develop_layer::active_photo(&host.editor), Some(photo.0));
        assert!(develop_layer::active_origin(&host.editor).is_none());
        let before = serde_json::to_value(&catalog(&host).catalog).unwrap();
        let selected = catalog(&host).selection.clone();
        host.switch(&egui::Context::default(), Module::Develop);
        assert_eq!(host.mode, Module::Compositing);
        assert!(host.editor.ui.status_error);
        assert_eq!(catalog(&host).selection, selected);
        assert_eq!(serde_json::to_value(&catalog(&host).catalog).unwrap(), before);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
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
        assert_eq!(
            host.editor.library_saves,
            vec![LibrarySave {
                notice: host.editor.library_saves[0].notice,
                origin: develop_layer::active_origin(&host.editor),
                path: saved.clone(),
                photo: Some(orig.0),
                show: false,
                overwritten: false,
                source_reset: None,
            }]
        );

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

    #[test]
    fn ai_colour_library_worker_converts_linear_unsaved_preview_before_publication() {
        let (dir, path, lib, orig) = library_with_photo("colour-preview");
        let mut host = host_with_round_trip(lib, orig, &path);
        host.editor
            .session
            .edit("Linear preview", |doc, _| {
                doc.depth = photocraft_color::SampleType::F32;
                doc.icc_profile = Some(photocraft_engine::color_cmds::resolve_profile("linear-srgb", None, None)?.to_bytes());
                for layer in &mut doc.layers {
                    layer.visible = false;
                }
                let mut layer = photocraft_doc::Layer::raster("Linear", photocraft_color::PixelFormat::RGBA32F);
                if let photocraft_doc::LayerContent::Raster(surface) = &mut layer.content {
                    let mut data = vec![0.5; doc.size.width as usize * doc.size.height as usize * 4];
                    for p in data.as_chunks_mut::<4>().0 {
                        p[3] = 1.0;
                    }
                    surface.write_region(doc.bounds(), &data);
                }
                doc.layers.push(layer);
                Ok(())
            })
            .unwrap();
        let ctx = egui::Context::default();
        host.update_composites(&ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while host.composites.done.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        {
            let done = host.composites.done.lock().unwrap();
            let img = done.first().expect("preview worker completed").2.as_ref().unwrap();
            assert_eq!(&img.pixels[..4], &[188, 188, 188, 255]);
        }
        host.update_composites(&ctx);
        assert!(host.library.as_ref().unwrap().host_composites.contains_key(&orig));
        assert!(host.composites.failed.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ai_colour_library_preview_failure_is_reported_once_per_revision() {
        let (dir, path, lib, orig) = library_with_photo("colour-preview-failure");
        let mut host = host_with_round_trip(lib, orig, &path);
        let key = unsaved_composites(&host.editor, catalog(&host))[0].1;
        host.composites.done.lock().unwrap().push((orig.0, key, Err("unsupported colour transform".into())));
        let ctx = egui::Context::default();
        host.update_composites(&ctx);
        assert!(host.editor.ui.status_error);
        assert!(host.editor.ui.status.contains("unsupported colour transform"));
        assert_eq!(host.composites.failed.get(&orig.0), Some(&key));
        assert!(host.composites.pending.is_empty());
        host.update_composites(&ctx);
        assert!(host.composites.pending.is_empty(), "do not retry the failed revision every frame");
        assert!(!host.library.as_ref().unwrap().host_composites.contains_key(&orig));
        assert_eq!(host.editor.ui.notices.len(), 1);
        let first = host.editor.ui.notices[0].id;
        host.editor.ui.notices.clear(); // Same removal as dismiss; history remains.
        host.editor.run("edit.fill", json!({"color":"#204060"})).unwrap();
        let key2 = unsaved_composites(&host.editor, catalog(&host))[0].1;
        assert_ne!(key2, key);
        host.composites.done.lock().unwrap().push((orig.0, key2, Err("unsupported colour transform".into())));
        host.update_composites(&ctx);
        assert_eq!(host.editor.ui.notices.len(), 1, "a new failed attempt is visible even with identical text");
        assert_ne!(host.editor.ui.notices[0].id, first);
        assert_eq!(host.editor.ui.notice_history.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
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

    #[test]
    fn native_menu_keys_and_quit_click_reach_the_library_after_startup() {
        use photocraft_ui_egui::native_menu::{Backend, Chord, Event, MenuBar, NativeMenu};
        struct Queue(Arc<Mutex<Vec<Event>>>);
        impl Backend for Queue {
            fn sync(&mut self, _: &MenuBar) {}
            fn drain(&mut self) -> Vec<Event> {
                std::mem::take(&mut *self.0.lock().unwrap())
            }
        }
        let (dir, mut host) = workflow_host("native-library-menu");
        host.mode = Module::Library;
        let events = Arc::new(Mutex::new(Vec::new()));
        host.editor.services.native_menu = Some(NativeMenu::new(Box::new(Queue(events.clone()))));
        let mut h = RoundTripUi::new(host);
        h.step(vec![]);
        h.step(vec![]); // The editor's startup logic no longer runs in this workspace.
        events.lock().unwrap().push(Event::Key(Chord::parse("Cmd+Q").unwrap()));
        let mut raw = egui::RawInput::default();
        eframe::App::raw_input_hook(&mut h.host, &h.view.ctx, &mut raw);
        assert!(raw.events.iter().any(|event| matches!(event, egui::Event::Key { key: egui::Key::Q, pressed: true, modifiers, .. } if modifiers.command)));
        events.lock().unwrap().push(Event::Click("file.exit".into()));
        let commands = h.step_at(egui::vec2(1200., 900.), vec![]);
        assert!(commands.contains(&egui::ViewportCommand::Close), "native Quit was lost in Library: {commands:?}");
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
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
            let trash = trash.clone();
            crate::services::overwrite_original(doc, std::path::Path::new(path), quality, move |p| li_ai::trash::copy_to_trash_in(p, &trash))
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
        let saved_revision = h.host.editor.session.active().unwrap().saved_revision;
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
        assert!(h.host.editor.has_unsaved(), "flattening the original does not save the layered document");
        assert_eq!(h.host.editor.session.active().unwrap().saved_revision, saved_revision);
        assert!(h.host.library.as_ref().unwrap().host_composites.contains_key(&photo), "unsaved layered edits keep their Library preview");
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
        h.key(egui::Key::E, egui::Modifiers::NONE);
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
    fn overwrite_preservation_native_develop_derivative_mask_and_return() {
        use photocraft_doc::{LayerContent, SmartSource};
        let (dir, path, mut library, photo) = library_with_photo("overwrite-develop-mask");
        library.run("develop.set", json!({"control":"light.exposure","value":0.5})).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut host = host_with_round_trip(library, photo, &path);
        let trash = dir.join("Trash");
        host.editor.services.overwrite_original = Some(Box::new(move |doc, path, quality| {
            let trash = trash.clone();
            crate::services::overwrite_original(doc, std::path::Path::new(path), quality, move |p| li_ai::trash::copy_to_trash_in(p, &trash))
        }));
        host.editor.run("edit.fill", json!({"color":"#804020"})).unwrap();
        let ctx = egui::Context::default();
        photocraft_ui_egui::menus::invoke(&mut host.editor, &ctx, "file.save", json!({})).unwrap();
        host.take_library_saves(None);
        let derivative = host.editor.session.active().unwrap().path.clone();
        assert!(derivative.as_deref().unwrap().ends_with("-Edit.psd"));
        host.editor.run("layer.layerMask.revealAll", json!({})).unwrap();
        let before = host.editor.session.active().unwrap().clone();
        let mut h = RoundTripUi::new(host);
        h.step(vec![]); // Install fonts before expecting actual modal rectangles.
        h.step(vec![]);
        photocraft_ui_egui::overwrite_original::request(&mut h.host.editor, before.doc.id, true).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        } // Let the modal's sizing pass settle before clicking.
        click_overwrite(&mut h, "overwriteConfirm");
        assert!(h.host.editor.overwrite_prompt.is_none(), "overwrite confirmation remained open");
        assert!(h.host.editor.ui.notices.last().is_some_and(|notice| !notice.error), "{}", h.host.editor.ui.status);
        assert_eq!(h.host.mode, Module::Library, "{}", h.host.editor.ui.status);
        let state = h.host.editor.session.active().unwrap();
        assert_eq!((state.revision, state.saved_revision, &state.path), (before.revision, before.saved_revision, &derivative));
        assert_eq!(state.history.entries(), before.history.entries());
        assert_eq!(state.doc.layers.len(), before.doc.layers.len());
        assert_eq!(state.doc.layers[1], before.doc.layers[1], "mask and edited pixels survive");
        let LayerContent::Smart(smart) = &state.doc.layers[0].content else { panic!("Develop source") };
        assert!(matches!(&smart.source,SmartSource::Embedded{bytes,..} if bytes.as_ref()==&original));
        let link = smart.develop.as_ref().unwrap();
        assert_eq!(link.settings["light"]["exposure"], 0.5);
        assert_eq!(link.library_baseline.as_deref(), Some(&serde_json::to_value(lightcraft_engine::develop::DevelopSettings::default()).unwrap()));
        assert_eq!(std::fs::read(dir.join("Trash/files/photo.png")).unwrap(), original);
        assert!(h.host.editor.session.undo());
        assert!(h.host.editor.session.active().unwrap().doc.layers[1].mask.is_none());
        assert!(h.host.editor.session.redo());
        assert_eq!(h.host.editor.session.active().unwrap().doc.layers[1], before.doc.layers[1]);
        let count = h.host.editor.session.active().unwrap().doc.layers.len();
        h.host.switch(&h.view.ctx, Module::Develop);
        h.host.switch(&h.view.ctx, Module::Compositing);
        h.wait_for(|host| !host.editor.session.has_jobs());
        assert_eq!(h.host.editor.session.active().unwrap().doc.layers.len(), count, "return after overwrite must not append a Develop layer");
        assert_eq!(h.host.editor.session.active().unwrap().path, derivative);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
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
        let restore_source = h
            .host
            .library
            .as_mut()
            .unwrap()
            .session
            .catalog
            .apply(lightcraft_catalog::Op::SetFile {
                id: photo,
                file_name: "photo.ARW".into(),
                source: lightcraft_catalog::Source::File { path: raw.to_string_lossy().into() },
            })
            .unwrap();
        h.host
            .editor
            .session
            .edit("raw source", |doc, _| {
                let photocraft_doc::LayerContent::Smart(s) = &mut doc.layers[0].content else { panic!("Develop layer") };
                s.source = photocraft_doc::SmartSource::Linked { path: raw.to_string_lossy().into() };
                s.develop.as_mut().unwrap().source = Some(raw.to_string_lossy().into());
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
        h.host.library.as_mut().unwrap().session.catalog.apply(restore_source).unwrap();
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
        let saved_revision = h.host.editor.session.active().unwrap().saved_revision;
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.wait_for(|h| h.library.as_ref().unwrap().host_composites.contains_key(&photo));
        h.host.editor.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        let other = h.host.editor.session.active().unwrap().doc.clone();
        h.click("button:roundTripOverwrite");
        assert_eq!(h.host.editor.overwrite_prompt.as_ref().unwrap().doc, target);
        click_overwrite(&mut h, "overwriteConfirm");
        assert!(Arc::ptr_eq(&other, &h.host.editor.session.active().unwrap().doc));
        let state = h.host.editor.session.documents().iter().find(|d| d.doc.id == target).unwrap();
        assert!(state.is_dirty(), "the overwritten document still has unsaved layers");
        assert_eq!(state.saved_revision, saved_revision);
        assert!(h.host.library.as_ref().unwrap().host_composites.contains_key(&photo));
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
        assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware GPU required");
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
        h.key(egui::Key::E, egui::Modifiers::NONE);
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
        assert!(h.host.editor.ui.notices.iter().any(|notice| notice.title == "Saved photo-Edit.psd"
            && notice.lines.iter().any(|line| line.contains(saved.to_str().unwrap()) && line.contains("stacked"))));
        h.wait_for(|h| h.library.as_ref().unwrap().renderer.thumb(edit).is_some());
        let thumb =
            h.host.library.as_ref().unwrap().widgets.iter().find(|(w, _)| w == &format!("thumb:{}", edit.0)).expect("the saved edit is visible in the grid").1;
        let pixel = h.pixel_at(thumb.center());
        assert!(pixel.r() > 230 && pixel.g() < 120 && pixel.b() < 140, "saved grid: {pixel:?}");
        assert_ne!(pixel, original);
        h.key(egui::Key::E, egui::Modifiers::NONE);
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
                    let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
                    drop(h);
                    // Full-view previews defer JPEG writes outside the renderer pool.
                    // Retire this fixture cache after pool teardown: wait for in-flight
                    // writes and reject late writers before removing its directory.
                    previews.retire();
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
    fn save_notice_retains_encoder_warnings_in_one_reserved_result() {
        fn capture(h: &RoundTripUi, name: &str) {
            let pixels = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/notices-ux");
            std::fs::create_dir_all(&dir).unwrap();
            image::RgbaImage::from_raw(pixels.size[0] as u32, pixels.size[1] as u32, pixels.pixels.iter().flat_map(|pixel| pixel.to_array()).collect())
                .unwrap()
                .save(dir.join(name))
                .unwrap();
        }
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(760.0, 480.0)] {
            let (dir, path, lib, photo) = library_with_photo(&format!("notice-warnings-{}", size.x));
            let mut host = host_with_round_trip(lib, photo, &path);
            let export = host.editor.services.export.take().unwrap();
            host.editor.services.export = Some(Box::new(move |doc, path, settings| {
                let (bytes, mut warnings) = export(doc, path, settings)?;
                warnings.push("Encoder preserved pixels but flattened an unsupported adjustment".into());
                Ok((bytes, warnings))
            }));
            let mut h = RoundTripUi::new(host);
            h.host.switch(&h.view.ctx, Module::Library);
            for _ in 0..5 {
                h.step_at(size, vec![]);
            }
            capture(&h, &format!("library-actions-{}.png", size.x as u32));
            let save = h.host.library.as_ref().unwrap().widgets.iter().find(|(id, _)| id == "button:roundTripSave").unwrap().1;
            assert!(h.view.ctx.content_rect().contains_rect(save));
            let at = save.center();
            h.step_at(size, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                h.step_at(size, vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
            }
            for _ in 0..4 {
                h.step_at(size, vec![]);
            }
            assert_eq!(h.host.editor.ui.notices.len(), 1, "save and Library result must not duplicate");
            assert!(h.host.library.as_ref().unwrap().ui.toast.is_none(), "no duplicate floating success toast");
            let notice = &h.host.editor.ui.notices[0];
            assert!(notice.warning_count >= 1);
            assert_eq!(notice.title, "Saved photo-Edit.psd");
            assert!(notice.lines.iter().any(|line| line.contains("flattened an unsupported adjustment")));
            assert!(notice.lines.iter().any(|line| line.contains("stacked")));
            assert_eq!(h.host.editor.ui.notice_history.len(), 1);
            capture(&h, &format!("library-result-{}.png", size.x as u32));
            for name in ["details", "dismiss"] {
                let rect = photocraft_ui_egui::notices::control_rect(&h.view.ctx, name).unwrap();
                assert!(h.view.ctx.content_rect().contains_rect(rect) && rect.height() >= 18.0);
            }
            assert!(
                !h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "button:roundTripSave"),
                "completed result has no duplicate action card"
            );
            drop(h);
            std::fs::remove_dir_all(dir).unwrap();
        }
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
        assert!(h.host.editor.ui.notices.iter().any(|notice| notice.error && notice.lines.iter().any(|line| line.contains("test disk full"))));
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
        host.open_editor_photo(orig.0, &path, &json!({}), "photo.png").unwrap();
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

    #[test]
    fn generation_metadata_library_save_return_and_update_keep_settings() {
        let (dir, path, lib, orig) = library_with_photo("generation-save-return");
        let mut host = host_with_round_trip(lib, orig, &path);
        let settings =
            json!({"command":"ai.generativeFill","prompt":"a copper badge","seed":u64::MAX,"engine":"int8","bounds":[1,1,2,2],"extension":{"kept":true}});
        host.editor
            .session
            .edit("Generated fixture", |doc, active| {
                let mut layer = photocraft_doc::Layer::raster("Generated badge", photocraft_color::PixelFormat::RGBA8);
                layer.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::from_xywh(1, 1, 2, 2), &[0.7, 0.3, 0.1, 1.]);
                layer.generation = Some(settings.clone());
                let id = doc.insert_above(*active, layer);
                *active = Some(id);
                Ok(())
            })
            .unwrap();
        let mut h = RoundTripUi::new(host);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.click("button:roundTripSave");
        let saved = dir.join("photo-Edit.psd");
        let read = || photocraft_io::import("photo-Edit.psd", &std::fs::read(&saved).unwrap()).unwrap().document;
        let back = read();
        assert_eq!(back.layers.iter().find_map(|layer| layer.generation.as_ref()), Some(&settings));
        assert!(photo_at_path(catalog(&h.host), &saved.to_string_lossy()).is_some());
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        let mut updated = settings.clone();
        updated["prompt"] = json!("a silver badge");
        h.host
            .editor
            .session
            .edit("Update generation settings", |doc, _| {
                doc.layers.iter_mut().find(|layer| layer.generation.is_some()).unwrap().generation = Some(updated.clone());
                Ok(())
            })
            .unwrap();
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.key(egui::Key::S, egui::Modifiers::COMMAND);
        assert!(!h.host.editor.has_unsaved());
        assert_eq!(read().layers.iter().find_map(|layer| layer.generation.as_ref()), Some(&updated));
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn derived_edit_inherits_organization_without_reusing_source_develop_or_dimensions() {
        let (dir, path, mut lib, original) = library_with_photo("derived-organization");
        lib.session.execute("photo.rate", &json!({"ids":[original.0],"rating":4})).unwrap();
        lib.session.execute("photo.pick", &json!({"ids":[original.0]})).unwrap();
        lib.session.execute("photo.label", &json!({"ids":[original.0],"label":"blue"})).unwrap();
        lib.session.execute("photo.setMeta", &json!({"ids":[original.0],"keywords":["Places|Coast","Portfolio"]})).unwrap();
        lib.session.execute("develop.set", &json!({"control":"light.exposure","value":1.5})).unwrap();
        // Finish the source edit's normal automatic version before comparing
        // the derivative handoff; leaving an edited photo records that version.
        lib.session.auto_version(original);
        let original_bytes = std::fs::read(&path).unwrap();
        let before = lib.session.catalog.photo(original).unwrap().clone();
        lib.session.xmp.auto_write = true;
        let saved = dir.join("derivative.png");
        image::RgbaImage::from_pixel(7, 5, image::Rgba([100u8, 60, 30, 255])).save(&saved).unwrap();
        let Some(LibraryUpdate::Added { id, .. }) = add_saved_file(&mut lib.session, &saved.to_string_lossy(), Some(original.0), false).unwrap() else {
            panic!("new derivative")
        };
        let sidecar = lightcraft_engine::sidecar::sidecar_path(&saved.to_string_lossy(), lib.session.sidecar_naming(id));
        let metadata = lightcraft_engine::sidecar::parse_sidecar(&std::fs::read_to_string(sidecar).unwrap(), false).unwrap();
        assert_eq!(metadata.rating, Some(4));
        assert_eq!(metadata.flag, Some(lightcraft_catalog::Flag::Pick));
        let derived = lib.session.catalog.photo(id).unwrap();
        assert_eq!((derived.rating, derived.flag, derived.label), (before.rating, before.flag, before.label));
        assert_eq!(derived.meta.keywords, before.meta.keywords);
        assert_eq!((derived.width, derived.height), (7, 5));
        assert_ne!(derived.source, before.source);
        assert_ne!(derived.develop, before.develop);
        assert!(lib.session.catalog.photo(original).unwrap() == &before, "source catalog record changed");
        lib.session.execute("library.filter", &json!({"rating":4,"flag":"pick","label":"blue"})).unwrap();
        assert!(lib.session.visible_cloned().contains(&id), "the saved edit remains visible in the source's rating/flag filters");
        // A deliberate later rating on the edit belongs to the edit, not its source.
        lib.session.execute("photo.rate", &json!({"ids":[id.0],"rating":2})).unwrap();
        lib.session.execute("photo.unflag", &json!({"ids":[id.0]})).unwrap();
        assert_eq!(add_saved_file(&mut lib.session, &saved.to_string_lossy(), Some(original.0), false).unwrap(), Some(LibraryUpdate::Reloaded(id)));
        assert_eq!(lib.session.catalog.photo(id).unwrap().rating, 2);
        assert_eq!(lib.session.catalog.photo(id).unwrap().flag, lightcraft_catalog::Flag::None);
        assert!(lib.session.catalog.photo(original).unwrap() == &before, "source catalog record changed");
        drop(lib);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn workflow_theme(host: &mut Host, kind: photocraft_ui_egui::theme::ThemeKind) {
        host.editor.ui.theme = kind;
        host.editor.run("prefs.set", json!({"values":{"interface.theme":kind.id()}})).unwrap();
    }
    fn workflow_host(tag: &str) -> (PathBuf, Host) {
        let (dir, path, library, photo) = library_with_photo(tag);
        let settings = serde_json::to_value(&*library.session.catalog.photo(photo).unwrap().develop).unwrap();
        let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(photocraft_engine::prefs::RenderingMode::Cpu));
        let mut host = Host::new(editor);
        host.editor.host_modes = true;
        host.library = Some(library);
        host.mode = Module::Compositing;
        host.open_editor_photo(photo.0, &path, &settings, "photo.png").unwrap();
        assert!(!host.editor.has_unsaved(), "theme fixture has no user edits to bypass");
        (dir, host)
    }
    fn workflow_click(h: &mut RoundTripUi, size: egui::Vec2, id: &str) {
        let rect = h.host.library.as_ref().unwrap().widgets.iter().find(|(key, _)| key == id).unwrap_or_else(|| panic!("missing {id}")).1;
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect), "{id}: {rect:?}");
        let pos = rect.center();
        h.step_at(size, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            h.step_at(size, vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
        }
        h.step_at(size, vec![]);
    }
    fn workflow_save_image(image: &egui::ColorImage, name: &str) {
        let out = std::path::Path::new("target/library-workflow-ux");
        std::fs::create_dir_all(out).unwrap();
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|color| color.to_array()).collect();
        image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, bytes).unwrap().save(out.join(format!("{name}.png"))).unwrap();
    }
    fn workflow_contrast(a: egui::Color32, b: egui::Color32) -> f64 {
        let luminance = |color: egui::Color32| {
            let linear = |v: u8| {
                let v = f64::from(v) / 255.;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    fn workflow_ink_contrast(image: &egui::ColorImage, rect: egui::Rect, fill: egui::Color32) -> f64 {
        let mut maximum = 1.0f64;
        for y in rect.top().ceil().max(0.0) as usize..(rect.bottom().floor() as usize).min(image.size[1]) {
            for x in rect.left().ceil().max(0.0) as usize..(rect.right().floor() as usize).min(image.size[0]) {
                maximum = maximum.max(workflow_contrast(image[(x, y)], fill));
            }
        }
        maximum
    }
    #[test]
    fn library_workflow_host_theme_matrix_and_live_switch_render_actual_bodies() {
        use lightcraft_ui_egui::theme::Tokens as LibraryTokens;
        use photocraft_ui_egui::theme::{ThemeKind, Tokens as EditorTokens};
        for kind in ThemeKind::ALL {
            let (dir, mut host) = workflow_host("workflow-theme-matrix");
            workflow_theme(&mut host, kind);
            let mut h = RoundTripUi::new(host);
            for size in [egui::vec2(1440., 900.), egui::vec2(760., 480.)] {
                for mode in [Module::Library, Module::Develop, Module::Compositing, Module::Library] {
                    let ctx = h.view.ctx.clone();
                    h.host.switch(&ctx, mode);
                    for _ in 0..5 {
                        h.step_at(size, vec![]);
                    }
                    assert_eq!(h.host.mode, mode, "theme workflow uses accepted real module switches");
                    assert_eq!(EditorTokens::get(&h.view.ctx).kind, kind);
                    if mode == Module::Compositing {
                        let image = h.view.paint(&std::collections::HashMap::new());
                        assert_eq!(image[(1, 125)], EditorTokens::for_kind(kind).chrome, "return to Compositing restores its actual toolbar chrome");
                        workflow_save_image(&image, &format!("{kind:?}-{mode:?}-{}x{}", size.x, size.y));
                        continue;
                    }
                    let editor = EditorTokens::for_kind(kind);
                    let library = LibraryTokens::get(&h.view.ctx);
                    assert_eq!(library.chrome, editor.chrome);
                    assert_eq!(library.canvas, editor.canvas);
                    assert_eq!(h.view.ctx.global_style().visuals.dark_mode, editor.dark());
                    assert!(workflow_contrast(library.text_label, library.chrome) >= 4.5);
                    assert!(workflow_contrast(library.selection_text, library.selection) >= 4.5);
                    assert!(workflow_contrast(library.caution, library.chrome) >= 4.5);
                    let lib = h.host.library.as_ref().unwrap();
                    let image = h.view.paint(&lib.renderer.cpu_textures());
                    let search = lib.widgets.iter().find(|(key, _)| key == "field:search").unwrap().1;
                    assert_eq!(image[((search.left() + 5.) as usize, (search.top() + 5.) as usize)], library.field, "{kind:?}/{mode:?}: rendered field");
                    if mode == Module::Develop {
                        let panel = lib.widgets.iter().find(|(key, _)| key == "panel:right_panel").unwrap().1;
                        let histogram = lib.widgets.iter().find(|(key, _)| key == "histogram").unwrap().1;
                        // The histogram has its own card fill. Probe its top margin and the
                        // unobstructed Edit-header margin below it as separate semantic roles.
                        let card = egui::pos2(histogram.left() + 4., histogram.top() + 2.);
                        let body = egui::pos2(histogram.left() + 4., histogram.bottom() + 8.);
                        assert!(panel.contains(card) && panel.contains(body));
                        assert_eq!(image[(card.x as usize, card.y as usize)], editor.card, "{kind:?}: Develop histogram follows host");
                        assert_eq!(image[(body.x as usize, body.y as usize)], editor.chrome, "{kind:?}: Develop body follows host");
                    } else {
                        let canvas = lib.canvas_rect.unwrap();
                        assert_eq!(
                            image[((canvas.left() + 3.) as usize, (canvas.bottom() - 4.) as usize)],
                            editor.canvas,
                            "{kind:?}: Library body follows host"
                        );
                    }
                    workflow_save_image(&image, &format!("{kind:?}-{mode:?}-{}x{}", size.x, size.y));
                    let before = h.host.library.as_ref().unwrap().ui.left_panel;
                    workflow_click(&mut h, size, "icon:sidebar");
                    assert_ne!(h.host.library.as_ref().unwrap().ui.left_panel, before);
                    workflow_click(&mut h, size, "icon:sidebar");
                    assert_eq!(h.host.library.as_ref().unwrap().ui.left_panel, before);
                }
            }
            // Same visible module, new theme: cache must not key only on Library/editor.
            let next = kind.next();
            workflow_theme(&mut h.host, next);
            for _ in 0..4 {
                h.step_at(egui::vec2(1440., 900.), vec![]);
            }
            assert_eq!(LibraryTokens::get(&h.view.ctx).chrome, EditorTokens::for_kind(next).chrome);
            let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
            drop(h);
            previews.retire();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn library_workflow_lazy_palette_and_accent_warning_controls() {
        use photocraft_ui_egui::theme::ThemeKind;
        for kind in ThemeKind::ALL {
            let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(photocraft_engine::prefs::RenderingMode::Cpu));
            let mut host = Host::new(editor);
            host.editor.host_modes = true;
            host.mode = Module::Develop;
            workflow_theme(&mut host, kind);
            let ctx = egui::Context::default();
            host.restyle(&ctx);
            assert!(host.styled.is_none(), "missing lazy Library cannot be cached as styled");
            let (dir, _, mut library, _) = library_with_photo("workflow-lazy-palette");
            set_library_view(&mut library, true);
            library.host_session = Some(lightcraft_ui_egui::panels::host_session::HostSession::new("Contrast fixture", vec!["crop".into()]));
            // A real temporary session has no open persistent library. Otherwise the
            // production recovery logic correctly clears this warning on the first frame.
            assert!(library.session.library.take().is_some());
            library.library_problem = Some(lightcraft_ui_egui::panels::library_problem::LibraryProblem { dismissed: true, ..Default::default() });
            host.library = Some(library);
            let mut h = RoundTripUi::new(host);
            let size = egui::vec2(1440., 900.);
            for _ in 0..5 {
                h.step_at(size, vec![]);
            }
            let palette = lightcraft_ui_egui::theme::Tokens::get(&h.view.ctx);
            assert_eq!(palette, library_palette(kind), "first Library logic must use host palette");
            let lib = h.host.library.as_ref().unwrap();
            let image = h.view.paint(&lib.renderer.cpu_textures());
            let ok = lib.widgets.iter().find(|(id, _)| id == "button:hostSessionOk").unwrap().1;
            let warning = lib.widgets.iter().find(|(id, _)| id == "indicator:temporarySession").unwrap().1;
            assert!(workflow_ink_contrast(&image, ok.shrink(5.), palette.accent) >= 3.0, "{kind:?}: actual antialiased OK text must be visible");
            assert!(workflow_ink_contrast(&image, warning, palette.caution) >= 3.0, "{kind:?}: actual warning text must be visible");
            workflow_save_image(&image, &format!("{kind:?}-filter-warning"));
            workflow_click(&mut h, size, "button:hostSessionOk");
            assert_eq!(h.host.library.as_ref().unwrap().host_session.as_ref().unwrap().result, Some(true));
            h.host.library.as_mut().unwrap().host_session.as_mut().unwrap().result = None;
            workflow_click(&mut h, size, "button:hostSessionCancel");
            assert_eq!(h.host.library.as_ref().unwrap().host_session.as_ref().unwrap().result, Some(false));
            let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
            drop(h);
            previews.retire();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn library_workflow_existing_photo_admission_runs_only_after_successful_focus() {
        use photocraft_engine::toolsets::Persona;
        use photocraft_ui_egui::{state::Tool, toolsets_ui::persona};
        let (dir, mut host) = workflow_host("workflow-photo-admission");
        let photo = host.library.as_ref().unwrap().session.active().unwrap();
        let ctx = egui::Context::default();
        // Finish setup admission before establishing the invalid-focus baseline.
        persona::frame(&mut host.editor, &ctx);
        assert!(host.editor.ui.photo_admission.is_none());
        persona::select(&mut host.editor, &ctx, Persona::Vector);
        persona::select_tool(&mut host.editor, &ctx, Tool::Pen);
        let document = host.editor.session.active().unwrap().doc.id;
        host.editor.ui.crop_rect = Some([1., 2., 20., 20.]);
        let draft = host.editor.ui.crop_rect;
        assert!(!host.focus_library_document(u64::MAX));
        assert!(host.editor.ui.photo_admission.is_none());
        persona::frame(&mut host.editor, &ctx);
        assert_eq!(host.editor.session.active().unwrap().doc.id, document);
        assert_eq!(host.editor.session.prefs().toolbar.persona, Persona::Vector);
        assert_eq!(host.editor.ui.tool, Tool::Pen);
        assert_eq!(host.editor.ui.crop_rect, draft);
        assert!(host.focus_library_document(photo.0));
        assert_eq!(host.editor.ui.photo_admission, Some(document));
        assert_eq!(host.editor.session.prefs().toolbar.persona, Persona::Vector, "admission is intentionally deferred");
        persona::frame(&mut host.editor, &ctx);
        assert!(host.editor.ui.photo_admission.is_none());
        assert_eq!(host.editor.session.prefs().toolbar.persona, Persona::Pixel);
        assert_eq!(host.editor.ui.tool, Tool::Move);
        // A different document becoming active before the frame owns its own draft.
        persona::select(&mut host.editor, &ctx, Persona::Vector);
        persona::select_tool(&mut host.editor, &ctx, Tool::Pen);
        assert!(host.focus_library_document(photo.0));
        assert_eq!(host.editor.ui.photo_admission, Some(document));
        let other = photocraft_doc::Document::new("Other", photocraft_doc::Size::new(32, 32), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        let other_id = other.id;
        host.editor.session.add_document(other, None);
        host.editor.sync_views();
        host.editor.ui.crop_rect = Some([3., 4., 12., 12.]);
        persona::frame(&mut host.editor, &ctx);
        assert!(host.editor.ui.photo_admission.is_none());
        assert_eq!(host.editor.session.active().unwrap().doc.id, other_id);
        assert_eq!(host.editor.session.prefs().toolbar.persona, Persona::Vector);
        assert_eq!(host.editor.ui.tool, Tool::Pen);
        assert_eq!(host.editor.ui.crop_rect, Some([3., 4., 12., 12.]));
        let previews = host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(host);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn workflow_overlay_wait(h: &mut RoundTripUi, size: egui::Vec2, ready: impl Fn(&Host) -> bool) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready(&h.host) {
            assert!(std::time::Instant::now() < until, "overlay fixture render deadline");
            h.step_at(size, vec![]);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        h.step_at(size, vec![]);
    }
    /// Sample only this glyph's known rectangle and a neighboring backing pixel.
    /// Count correctly coloured foreground coverage; a high-contrast frame/photo
    /// elsewhere in a broad rectangle cannot satisfy this oracle.
    fn workflow_overlay_glyph(image: &egui::ColorImage, glyph: egui::Rect, backing: egui::Pos2, foreground: egui::Color32, label: &str) {
        let background = image[(backing.x.floor() as usize, backing.y.floor() as usize)];
        assert!(workflow_contrast(foreground, background) >= 4.5, "{label}: foreground/backing {foreground:?}/{background:?}");
        let delta =
            [foreground.r() as f64 - background.r() as f64, foreground.g() as f64 - background.g() as f64, foreground.b() as f64 - background.b() as f64];
        let energy = delta.iter().map(|v| v * v).sum::<f64>();
        let mut ink = 0;
        for y in glyph.top().ceil().max(0.) as usize..(glyph.bottom().floor() as usize).min(image.size[1]) {
            for x in glyph.left().ceil().max(0.) as usize..(glyph.right().floor() as usize).min(image.size[0]) {
                let actual = image[(x, y)];
                let d = [actual.r() as f64 - background.r() as f64, actual.g() as f64 - background.g() as f64, actual.b() as f64 - background.b() as f64];
                let coverage = d.iter().zip(delta).map(|(a, b)| a * b).sum::<f64>() / energy;
                let residual = d.iter().zip(delta).map(|(a, b)| (a - coverage * b).powi(2)).sum::<f64>();
                if coverage >= 0.45 && residual <= 3. * 24f64.powi(2) && workflow_contrast(actual, background) >= 3.0 {
                    ink += 1;
                }
            }
        }
        assert!(ink >= 2, "{label}: no correctly coloured rendered glyph pixels in {glyph:?} (found {ink})");
    }
    #[test]
    fn library_workflow_photo_overlay_glyphs_on_black_and_white_all_themes() {
        use lightcraft_catalog::Flag;
        use lightcraft_ui_egui::state::{GridBadges, RightPanel};
        use photocraft_ui_egui::theme::ThemeKind;
        let size = egui::vec2(1440., 900.);
        for kind in ThemeKind::ALL {
            let dir = std::env::temp_dir().join(format!("li-workflow-overlay-{}-{}", kind.id(), std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let paths = [dir.join("black.png"), dir.join("white.png")];
            for (path, value) in paths.iter().zip([0, 255]) {
                image::RgbImage::from_pixel(80, 60, image::Rgb([value; 3])).save(path).unwrap();
            }
            let mut session = lightcraft_engine::Session::new().with_fs();
            session.execute("library.import", &json!({"paths":paths.iter().map(|p|p.to_string_lossy()).collect::<Vec<_>>()})).unwrap();
            let photos: Vec<_> = session.catalog.photos().map(|p| (p.id, p.file_name.starts_with("white"))).collect();
            let ids: Vec<_> = photos.iter().map(|(id, _)| id.0).collect();
            let black = photos.iter().find(|(_, white)| !white).unwrap().0;
            session.execute("photo.rate", &json!({"ids":ids,"rating":1})).unwrap();
            session.execute("photo.flag", &json!({"ids":ids,"flag":"pick"})).unwrap();
            for (id, _) in &photos {
                session.execute("library.select", &json!({"ids":[id.0]})).unwrap();
                session.execute("develop.set", &json!({"control":"light.exposure","value":0.25})).unwrap();
            }
            session.execute("library.select", &json!({"ids":[black.0]})).unwrap();
            let mut library = LightcraftApp::new(session, Default::default());
            library.host_fonts = true;
            library.ui.settings.gpu = false;
            library.ui.settings.grid_badges = GridBadges::Always;
            set_library_view(&mut library, false);
            let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(photocraft_engine::prefs::RenderingMode::Cpu));
            let mut host = Host::new(editor);
            host.editor.host_modes = true;
            host.library = Some(library);
            host.mode = Module::Library;
            workflow_theme(&mut host, kind);
            let mut h = RoundTripUi::new(host);
            for _ in 0..4 {
                h.step_at(size, vec![]);
            }
            workflow_overlay_wait(&mut h, size, |host| photos.iter().all(|(id, _)| host.library.as_ref().unwrap().renderer.thumb(*id).is_some()));
            let palette = lightcraft_ui_egui::theme::Tokens::get(&h.view.ctx);
            let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
            for (id, white) in &photos {
                let rect = h.host.library.as_ref().unwrap().widgets.iter().find(|(key, _)| key == &format!("thumb:{}", id.0)).unwrap().1;
                let center = image[(rect.center().x as usize, rect.center().y as usize)];
                assert!(
                    if *white { center.r() > 240 && center.g() > 240 && center.b() > 240 } else { center.r() < 8 && center.g() < 8 && center.b() < 8 },
                    "actual fixture photo: {center:?}"
                );
                let y = rect.bottom() - 12.;
                let background = egui::pos2(rect.left() + 2., y);
                for (name, glyph) in [
                    ("rating", egui::Rect::from_min_size(egui::pos2(rect.left() + 6., y - 6.), egui::vec2(12., 12.))),
                    ("pick", egui::Rect::from_min_size(egui::pos2(rect.left() + 23., y - 7.), egui::vec2(14., 14.))),
                    ("edited", egui::Rect::from_min_size(egui::pos2(rect.right() - 20., y - 7.), egui::vec2(14., 14.))),
                ] {
                    workflow_overlay_glyph(&image, glyph, background, palette.overlay_fg, &format!("{kind:?}/{white}/grid/{name}"));
                }
            }
            workflow_save_image(&image, &format!("{kind:?}-overlay-grid-pick"));
            h.host.library.as_mut().unwrap().run("photo.flag", json!({"ids":ids,"flag":"reject"})).unwrap();
            h.step_at(size, vec![]);
            let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
            for (id, white) in &photos {
                let rect = h.host.library.as_ref().unwrap().widgets.iter().find(|(key, _)| key == &format!("thumb:{}", id.0)).unwrap().1;
                let center = image[(rect.center().x as usize, rect.center().y as usize)];
                if *white {
                    assert!(
                        center.r() > 100 && center.r() < 200 && center.g() == center.r() && center.b() == center.r(),
                        "rejected photo remains visibly dimmed: {center:?}"
                    );
                } else {
                    assert!(center.r() < 8 && center.g() < 8 && center.b() < 8);
                }
                let y = rect.bottom() - 12.;
                workflow_overlay_glyph(
                    &image,
                    egui::Rect::from_min_size(egui::pos2(rect.left() + 23., y - 7.), egui::vec2(14., 14.)),
                    egui::pos2(rect.left() + 2., y),
                    palette.overlay_reject,
                    &format!("{kind:?}/{white}/grid/reject"),
                );
            }
            h.host.library.as_mut().unwrap().run("view.squareGrid", json!({})).unwrap();
            for _ in 0..3 {
                h.step_at(size, vec![]);
            }
            let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
            let format_size = h.view.ctx.fonts_mut(|fonts| fonts.layout_no_wrap("PNG".into(), palette.semibold(9.), palette.overlay_fg).size());
            for (id, white) in &photos {
                let rect = h.host.library.as_ref().unwrap().widgets.iter().find(|(key, _)| key == &format!("thumb:{}", id.0)).unwrap().1;
                let pill = egui::Rect::from_min_size(egui::pos2(rect.right() - format_size.x - 14., rect.top() + 5.), format_size + egui::vec2(8., 3.));
                workflow_overlay_glyph(
                    &image,
                    egui::Rect::from_min_size(pill.min + egui::vec2(4., 1.5), format_size),
                    pill.min + egui::vec2(2., 2.),
                    palette.overlay_fg,
                    &format!("{kind:?}/{white}/format"),
                );
            }
            workflow_save_image(&image, &format!("{kind:?}-overlay-format"));
            h.host.library.as_mut().unwrap().run("view.detail", json!({})).unwrap();
            h.host.library.as_mut().unwrap().ui.filmstrip = true;
            for flag in [Flag::Pick, Flag::Reject] {
                h.host.library.as_mut().unwrap().run("photo.flag", json!({"ids":ids,"flag":if flag==Flag::Pick{"pick"}else{"reject"}})).unwrap();
                for _ in 0..3 {
                    h.step_at(size, vec![]);
                }
                let lib = h.host.library.as_ref().unwrap();
                let image = h.view.paint(&lib.renderer.cpu_textures());
                for (id, white) in &photos {
                    let cell = lib.widgets.iter().find(|(key, _)| key == &format!("film:{}", id.0)).unwrap().1;
                    let area = egui::Rect::from_min_max(cell.min + egui::vec2(10., 22.), cell.max - egui::vec2(10., 8.));
                    let [w, height] = lib.renderer.thumb(*id).unwrap().size;
                    let scale = (area.width() / w as f32).min(area.height() / height as f32);
                    let photo = egui::Rect::from_center_size(area.center(), egui::vec2(w as f32 * scale, height as f32 * scale));
                    let y = photo.bottom() - 7.5;
                    let background = egui::pos2(photo.right() - 2., y);
                    for (name, glyph, color) in [
                        ("rating", egui::Rect::from_min_size(egui::pos2(photo.left() + 3., y - 4.), egui::vec2(8., 8.)), palette.overlay_fg),
                        (
                            "flag",
                            egui::Rect::from_min_size(egui::pos2(photo.left() + 14., y - 5.), egui::vec2(10., 10.)),
                            if flag == Flag::Pick { palette.overlay_fg } else { palette.overlay_reject },
                        ),
                        ("edited", egui::Rect::from_min_size(egui::pos2(photo.right() - 13., y - 5.), egui::vec2(10., 10.)), palette.overlay_fg),
                    ] {
                        workflow_overlay_glyph(&image, glyph, background, color, &format!("{kind:?}/{white}/film/{flag:?}/{name}"));
                    }
                }
                workflow_save_image(&image, &format!("{kind:?}-overlay-film-{flag:?}"));
            }
            // Collapse unrelated profile groups to bound this favorite-preview fixture.
            for group in lightcraft_engine::presets::profile_groups() {
                if group != "Favorites" {
                    h.view.ctx.data_mut(|data| data.insert_temp(egui::Id::new(("profile-group", group)), false));
                }
            }
            h.view.ctx.data_mut(|data| data.insert_temp(egui::Id::new(("profile-group", "Favorites")), true));
            for (id, white) in &photos {
                let lib = h.host.library.as_mut().unwrap();
                lib.run("library.select", json!({"ids":[id.0]})).unwrap();
                lib.ui.right = RightPanel::Profiles;
                lib.session.profile_favorites = vec!["lc.color".into()];
                let settings = lib.session.develop_of(*id).unwrap();
                let key = lib.session.variant_job(*id, &settings, 256).unwrap().key;
                for _ in 0..2 {
                    h.step_at(size, vec![]);
                }
                workflow_overlay_wait(&mut h, size, |host| {
                    host.library.as_ref().unwrap().renderer.textures.contains_key(&lightcraft_ui_egui::render::Slot::Variant(key))
                });
                let lib = h.host.library.as_ref().unwrap();
                let star = lib.widgets.iter().find(|(key, _)| key == "profileStar:fav:lc.color").unwrap().1;
                let image = h.view.paint(&lib.renderer.cpu_textures());
                workflow_overlay_glyph(
                    &image,
                    star.shrink(4.),
                    egui::pos2(star.center().x, star.top() + 2.),
                    palette.overlay_fg,
                    &format!("{kind:?}/{white}/favorite"),
                );
                workflow_save_image(&image, &format!("{kind:?}-overlay-favorite-{white}"));
            }
            let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
            drop(h);
            previews.retire();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn library_navigation_history_versions_keep_module_and_footer_bounds() {
        let (dir, host) = workflow_host("navigation-modules");
        let mut h = RoundTripUi::new(host);
        workflow_theme(&mut h.host, photocraft_ui_egui::theme::ThemeKind::StudioLight);
        let size = egui::vec2(760., 480.);
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, Module::Develop);
        for _ in 0..5 {
            h.step_at(size, vec![]);
        }
        for command in ["panel.activity", "panel.versions"] {
            h.host.library.as_mut().unwrap().run(command, json!({})).unwrap();
            for _ in 0..3 {
                h.step_at(size, vec![]);
            }
            assert_eq!(h.host.mode, Module::Develop);
        }
        h.host.library.as_mut().unwrap().run("panel.versions", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        assert_eq!(h.host.mode, Module::Develop, "closing auxiliary panel restores Edit");
        assert_eq!(h.host.library.as_ref().unwrap().ui.right, lightcraft_ui_egui::state::RightPanel::Edit);
        let lib = h.host.library.as_ref().unwrap();
        let left = lib.widgets.iter().find(|(id, _)| id == "footer:left").unwrap().1;
        let right = lib.widgets.iter().find(|(id, _)| id == "footer:right").unwrap().1;
        assert!(left.right() <= right.left(), "actual narrow Develop footer overlaps: {left:?}/{right:?}");
        for id in ["button:compactViews", "dropdown:zoom", "icon:filmstrip", "icon:beforeAfter"] {
            let hit = lib.widgets.iter().find(|(key, _)| key == id).unwrap().1;
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(hit), "{id}: {hit:?}");
            assert!(left.contains_rect(hit) || right.contains_rect(hit));
        }
        let image = h.view.paint(&lib.renderer.cpu_textures());
        workflow_save_image(&image, "StudioLight-Develop-760x480-navigation-footer");
        h.host.library.as_mut().unwrap().run("view.detail", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        assert_eq!(h.host.mode, Module::Library, "explicit Loupe still leaves Develop");
        h.host.library.as_mut().unwrap().run("panel.activity", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        assert_eq!(h.host.mode, Module::Library, "Library history does not force Develop");
        h.host.library.as_mut().unwrap().run("panel.activity", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        assert_eq!(h.host.mode, Module::Library);
        assert_eq!(
            h.host.library.as_ref().unwrap().ui.right,
            lightcraft_ui_egui::state::RightPanel::None,
            "closing Library history restores Loupe, not stale Edit"
        );
        h.host.library.as_mut().unwrap().run("view.develop", json!({})).unwrap();
        h.host.library.as_mut().unwrap().run("panel.versions", json!({})).unwrap();
        h.host.library.as_mut().unwrap().run("view.photoGrid", json!({})).unwrap();
        h.host.library.as_mut().unwrap().run("panel.versions", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        assert_eq!(h.host.mode, Module::Library);
        assert_eq!(h.host.library.as_ref().unwrap().ui.right, lightcraft_ui_egui::state::RightPanel::None, "explicit grid change invalidates auxiliary return");
        let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(h);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn library_navigation_toast_overlay_contrast_light_and_classic() {
        use photocraft_ui_egui::theme::ThemeKind;
        for kind in [ThemeKind::StudioLight, ThemeKind::Classic] {
            for background in [egui::Color32::BLACK, egui::Color32::WHITE] {
                let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
                let mut view = lightcraft_ui_egui::headless::HeadlessView::new();
                app.set_host_theme(library_palette(kind));
                app.restyle(&view.ctx);
                let canvas = egui::Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(630., 300.));
                app.canvas_rect = Some(canvas);
                app.ui.toast = Some(("Versions Off".into(), 2.));
                let mut rectangles = Vec::new();
                let raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(egui::vec2(640., 320.), 1., 0.1, vec![]);
                view.run(raw, |ui| {
                    ui.painter().rect_filled(canvas, 0., background);
                    lightcraft_ui_egui::panels::toast(&mut app, ui.ctx());
                    rectangles = lightcraft_ui_egui::widgets::take_registry(ui.ctx());
                });
                let body = rectangles.iter().find(|(id, _)| id == "toast:body").unwrap().1;
                let text = rectangles.iter().find(|(id, _)| id == "toast:text").unwrap().1;
                assert!(canvas.contains_rect(body) && body.contains_rect(text));
                let image = view.paint(&std::collections::HashMap::new());
                workflow_overlay_glyph(
                    &image,
                    text,
                    body.left_center() + egui::vec2(5., 0.),
                    library_palette(kind).overlay_fg,
                    &format!("{kind:?}/{background:?}/toast"),
                );
                assert_eq!(app.ui.toast.as_ref().unwrap().0, "Versions Off", "contrast fix must not hide the notice");
                workflow_save_image(&image, &format!("{kind:?}-toast-{}", background.r()));
                let raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(egui::vec2(640., 320.), 1., 2.1, vec![]);
                view.run(raw, |ui| lightcraft_ui_egui::panels::toast(&mut app, ui.ctx()));
                assert!(app.ui.toast.is_none(), "existing lifetime remains unchanged");
            }
        }
    }
    fn shared_shell_frame(h: &mut RoundTripUi, size: egui::Vec2, ppp: f32, events: Vec<egui::Event>) -> Vec<egui::ViewportCommand> {
        use eframe::App;
        let mut raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(size, ppp, h.frames as f64 / 60., events);
        h.host.raw_input_hook(&h.view.ctx, &mut raw);
        let commands = h.view.run(raw, |ui| {
            h.host.logic(ui.ctx(), &mut h.frame);
            h.host.ui(ui, &mut h.frame);
        });
        h.frames += 1;
        commands
    }
    #[test]
    fn shared_shell_host_title_menus_visible_once_without_drag_overlap() {
        for kind in photocraft_ui_egui::theme::ThemeKind::ALL {
            let (dir, host) = workflow_host("shared-shell-title");
            let mut h = RoundTripUi::new(host);
            h.host.editor.custom_titlebar = true;
            workflow_theme(&mut h.host, kind);
            for size in [egui::vec2(1440., 900.), egui::vec2(760., 480.)] {
                for ppp in [1., 2.] {
                    for mode in [Module::Library, Module::Develop] {
                        let ctx = h.view.ctx.clone();
                        h.host.switch(&ctx, mode);
                        for _ in 0..4 {
                            shared_shell_frame(&mut h, size, ppp, vec![]);
                        }
                        assert_eq!(h.host.mode, mode);
                        let lib = h.host.library.as_ref().unwrap();
                        let menus: Vec<_> = lib.widgets.iter().filter(|(id, _)| id.starts_with("menu:")).map(|(id, r)| (id.clone(), *r)).collect();
                        assert!(!menus.is_empty());
                        let title_height = if photocraft_ui_egui::theme::Tokens::for_kind(kind).pro { 32. } else { 38. };
                        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                        let mode_rects: Vec<_> = Module::ALL
                            .map(|module| module.digit())
                            .into_iter()
                            .map(|digit| h.view.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", digit)))).unwrap())
                            .collect();
                        for (id, r) in &menus {
                            assert_eq!(menus.iter().filter(|(other, _)| other == id).count(), 1, "no duplicate second-row menu");
                            assert!(viewport.contains_rect(*r) && r.bottom() <= title_height, "{kind:?}/{mode:?}/{size:?}/{ppp}: {id}/{r:?}");
                            for m in &mode_rects {
                                assert!(!r.intersects(*m));
                            }
                        }
                        let search = lib.widgets.iter().find(|(id, _)| id == "field:search").unwrap().1;
                        assert!(search.top() >= title_height && viewport.contains_rect(search));
                        let menu = menus[0].1;
                        let pos = menu.center();
                        let mut commands = shared_shell_frame(
                            &mut h,
                            size,
                            ppp,
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
                            ],
                        );
                        commands.extend(shared_shell_frame(&mut h, size, ppp, vec![egui::Event::PointerMoved(pos + egui::vec2(12., 0.))]));
                        commands.extend(shared_shell_frame(
                            &mut h,
                            size,
                            ppp,
                            vec![egui::Event::PointerButton {
                                pos: pos + egui::vec2(12., 0.),
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            }],
                        ));
                        assert!(!commands.iter().any(|v| matches!(v, egui::ViewportCommand::StartDrag)), "menu gesture cannot drag native title");
                        egui::Popup::close_all(&h.view.ctx);
                        shared_shell_frame(&mut h, size, ppp, vec![]);
                        let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
                        workflow_save_image(&image, &format!("shared-{kind:?}-{mode:?}-{}x{}-{ppp}x", size.x, size.y));
                    }
                }
            }
            let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
            drop(h);
            previews.retire();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn keys_profiles_g5_host_focus_routes_only_visible_workspace() {
        let (dir, host) = workflow_host("keys-profile-focus");
        let mut h = RoundTripUi::new(host);
        h.host.editor.session.edit_prefs(|p| {
            p.shortcut_profiles.migrate(&p.shortcuts);
            p.shortcut_profiles.library.active = "Capture One".into();
        });
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, Module::Library);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.key(egui::Key::Num2, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Develop);
        let tool = h.host.editor.ui.tool;
        h.key(egui::Key::B, egui::Modifiers::NONE);
        assert_eq!(h.host.library.as_ref().unwrap().ui.tool, "brush");
        assert_eq!(h.host.editor.ui.tool, tool);
        h.key(egui::Key::E, egui::Modifiers::NONE);
        assert!(h.host.library.as_ref().unwrap().ui.brush_erase);
        h.host.editor.host_modes = true;
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        let overlay = h.host.library.as_ref().unwrap().ui.mask_overlay;
        h.key(egui::Key::M, egui::Modifiers::NONE);
        assert_eq!(h.host.editor.ui.tool, photocraft_ui_egui::state::Tool::RectMarquee);
        assert_eq!(h.host.library.as_ref().unwrap().ui.mask_overlay, overlay);
        h.key(egui::Key::B, egui::Modifiers::NONE);
        assert_eq!(h.host.editor.ui.tool, photocraft_ui_egui::state::Tool::Brush);
        assert!(h.host.library.as_ref().unwrap().ui.brush_erase);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        h.key(egui::Key::Num2, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Develop);
        h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
        h.step(vec![]);
        h.key(egui::Key::K, egui::Modifiers::COMMAND);
        assert!(h.host.editor.ui.dialogs.is_empty(), "Library's modal owns its keyboard input");
        h.key(egui::Key::Escape, egui::Modifiers::NONE);
        assert!(h.host.library.as_ref().unwrap().ui.dialog.is_none());
        h.key(egui::Key::K, egui::Modifiers::COMMAND);
        assert_eq!(h.host.editor.ui.dialogs.last().unwrap().fields["__prefsui"], "prefs");
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shared_shell_host_shortcut_editor_capture_cancel_and_library_dispatch() {
        let (dir, host) = workflow_host("shared-shell-shortcuts");
        let mut h = RoundTripUi::new(host);
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, Module::Library);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        assert!(h.host.library.as_ref().unwrap().ui.dialog.is_none());
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        let key = "library::view.develop".to_string();
        let fields = &mut h.host.editor.ui.dialog_mut(id).unwrap().fields;
        assert_eq!(fields["__prefsui"], "shortcuts");
        assert_eq!(fields["shortcutScope"], "library");
        fields.insert("filter".into(), json!(key));
        for _ in 0..3 {
            h.step(vec![]);
        }
        let hit = h.view.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("shortcut-capture", &key)))).unwrap();
        let pos = hit.center();
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 900.)).contains_rect(hit));
        for pressed in [true, false] {
            h.step(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ]);
        }
        assert_eq!(h.host.editor.ui.dialog_mut(id).unwrap().fields["capture"], true);
        let photo = h.host.library.as_ref().unwrap().session.active();
        let revision = h.host.editor.session.active().unwrap().revision;
        h.key(egui::Key::D, egui::Modifiers::NONE);
        assert_eq!(h.host.mode, Module::Library, "capturing the default Develop key must not navigate the underlying Library");
        let rating = h.host.library.as_ref().unwrap().session.catalog.photo(photo.unwrap()).unwrap().rating;
        h.host.editor.ui.dialog_mut(id).unwrap().fields.insert("capture".into(), json!(true));
        h.key(egui::Key::Num5, egui::Modifiers::NONE);
        assert_eq!(
            h.host.library.as_ref().unwrap().session.catalog.photo(photo.unwrap()).unwrap().rating,
            rating,
            "capturing a rating key must not edit photos"
        );
        h.host.editor.ui.dialog_mut(id).unwrap().fields.insert("capture".into(), json!(true));
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Library, "capture owns reserved module keys too");
        h.host.editor.ui.dialog_mut(id).unwrap().fields.insert("capture".into(), json!(true));
        h.key(egui::Key::F6, egui::Modifiers::NONE);
        assert_eq!(h.host.editor.ui.dialog_mut(id).unwrap().fields["overrides"][&key], "F6");
        assert_eq!(h.host.mode, Module::Library);
        assert_eq!(h.host.library.as_ref().unwrap().session.active(), photo);
        assert_eq!(h.host.editor.session.active().unwrap().revision, revision);
        photocraft_ui_egui::dialogs::confirm(&mut h.host.editor, id).unwrap();
        for _ in 0..2 {
            h.step(vec![]);
        }
        h.key(egui::Key::F6, egui::Modifiers::NONE);
        assert_eq!(h.host.mode, Module::Develop);
        let entries = lightcraft_ui_egui::menus::menu_entries(h.host.library.as_ref().unwrap());
        assert_eq!(entries.iter().find(|e| e.id == "view.develop").unwrap().shortcut.as_deref(), Some("F6"));
        h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
        h.step(vec![]);
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        h.host.editor.ui.dialog_mut(id).unwrap().fields.insert("overrides".into(), json!({"library::view.develop":"F7"}));
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        h.step(vec![]);
        assert_eq!(h.host.editor.session.prefs().shortcuts[&key], "F6");
        h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
        h.step(vec![]);
        assert!(
            matches!(h.host.library.as_ref().unwrap().ui.dialog, Some(lightcraft_ui_egui::state::Dialog::Settings { .. })),
            "Preferences keeps Library-specific pages"
        );
        let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(h);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn shared_shell_host_exact_save_keeps_sync_settings_and_module_shortcuts() {
        let (dir, path, library, photo) = library_with_photo("shared-shell-host-keys");
        let mut host = host_with_round_trip(library, photo, &path);
        host.editor.session.prefs.edit(|p| p.performance.rendering_mode = Some(photocraft_engine::prefs::RenderingMode::Cpu));
        host.mode = Module::Library;
        set_library_view(host.library.as_mut().unwrap(), false);
        host.library.as_mut().unwrap().run("photo.virtualCopy", json!({"ids":[photo.0]})).unwrap();
        let ids: Vec<_> = host.library.as_ref().unwrap().session.catalog.photos().map(|p| p.id.0).collect();
        host.library.as_mut().unwrap().run("library.select", json!({"ids":ids,"active":photo.0})).unwrap();
        assert!(!unsaved_composites(&host.editor, &host.library.as_ref().unwrap().session).is_empty());
        let mut h = RoundTripUi::new(host);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.key(egui::Key::S, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert!(
            matches!(h.host.library.as_ref().unwrap().ui.dialog, Some(lightcraft_ui_egui::state::Dialog::SyncSettings { .. })),
            "documented more-specific Library shortcut must not be stolen by Save"
        );
        assert!(!dir.join("photo-Edit.psd").exists());
        assert!(h.host.editor.has_unsaved());
        h.host.library.as_mut().unwrap().ui.dialog = None;
        h.step(vec![]);
        h.key(egui::Key::S, egui::Modifiers::COMMAND);
        h.wait_for(|host| !host.editor.has_unsaved());
        assert!(dir.join("photo-Edit.psd").exists(), "exact host Save still publishes its layered copy");
        h.key(egui::Key::Num3, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Library);
        let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(h);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn shared_shell_capture_retains_existing_library_dialog_and_field_drafts() {
        let (dir, host) = workflow_host("shared-shell-under-dialog");
        let mut h = RoundTripUi::new(host);
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, Module::Library);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.host.library.as_mut().unwrap().run("dialog.export", json!({})).unwrap();
        for _ in 0..4 {
            h.step(vec![]);
        }
        let underlying = h.host.library.as_ref().unwrap().ui.dialog.clone();
        let old_cancel = h.host.library.as_ref().unwrap().widgets.iter().find(|(id, _)| id == "button:dialogCancel").unwrap().1;
        h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
        for _ in 0..4 {
            h.step(vec![]);
        }
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        assert!(
            !h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "dialog:window"),
            "underlying window is suspended without discarding state"
        );
        let top = h.view.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", id)))).unwrap();
        let point = old_cancel.center();
        assert!(!top.contains(point), "fixture old Cancel must lie outside the top modal, not on a legitimate host control");
        for pressed in [true, false] {
            h.step(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ]);
        }
        assert_eq!(h.host.library.as_ref().unwrap().ui.dialog, underlying);
        for key in [egui::Key::Escape, egui::Key::Enter] {
            let fields = &mut h.host.editor.ui.dialog_mut(id).unwrap().fields;
            fields.insert("selected".into(), json!("library::view.develop"));
            fields.insert("capture".into(), json!(true));
            h.key(key, egui::Modifiers::NONE);
            assert!(h.host.editor.ui.dialogs.iter().any(|d| d.id == id));
            assert_eq!(h.host.library.as_ref().unwrap().ui.dialog, underlying, "shared capture must not close/confirm underlying Export");
        }
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        assert_eq!(h.host.library.as_ref().unwrap().ui.dialog, underlying);
        assert!(h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "dialog:window"));
        // The ordinary Masking panel also has raw Escape and autofocus paths.
        let lib = h.host.library.as_mut().unwrap();
        lib.ui.dialog = None;
        lib.run("view.develop", json!({})).unwrap();
        lib.run("panel.masking", json!({})).unwrap();
        lib.run("mask.add", json!({"kind":"radial"})).unwrap();
        let mid = lib.session.active_mask.unwrap();
        let photo = lib.session.active().unwrap();
        lib.ui.renaming_mask = Some((mid, "Kept name".into()));
        lib.ui.renaming_component = Some((mid, 0, "Kept component".into()));
        lib.ui.describe = Some(("new".into(), "Kept prompt".into()));
        lib.run("app.shortcuts", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        let develop = h.host.library.as_ref().unwrap().session.develop_of(photo).unwrap();
        for key in [egui::Key::Escape, egui::Key::Enter] {
            let f = &mut h.host.editor.ui.dialog_mut(id).unwrap().fields;
            f.insert("selected".into(), json!("library::view.develop"));
            f.insert("capture".into(), json!(true));
            h.key(key, egui::Modifiers::NONE);
            let lib = h.host.library.as_ref().unwrap();
            assert_eq!(lib.ui.renaming_mask, Some((mid, "Kept name".into())));
            assert_eq!(lib.ui.renaming_component, Some((mid, 0, "Kept component".into())));
            assert_eq!(lib.ui.describe, Some(("new".into(), "Kept prompt".into())));
            assert_eq!(lib.session.develop_of(photo).unwrap(), develop);
        }
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        let lib = h.host.library.as_mut().unwrap();
        lib.ui.renaming_mask = None;
        lib.ui.renaming_component = None;
        lib.ui.describe = None;
        lib.run("panel.keywords", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        h.click("field:keyword");
        h.step(vec![egui::Event::Text("Retained keyword".into())]);
        let keyword_focus = h.view.ctx.memory(|m| m.focused()).unwrap();
        h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        let before = h.host.library.as_ref().unwrap().session.catalog.photo(photo).unwrap().meta.keywords.clone();
        let f = &mut h.host.editor.ui.dialog_mut(id).unwrap().fields;
        f.insert("selected".into(), json!("library::view.develop"));
        f.insert("capture".into(), json!(true));
        // Exercise the stale-focus boundary explicitly: disabled TextEdit loses this
        // old focus during the same Enter pass; it still must not commit a keyword.
        h.view.ctx.memory_mut(|m| m.request_focus(keyword_focus));
        h.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(h.host.library.as_ref().unwrap().session.catalog.photo(photo).unwrap().meta.keywords, before);
        assert_eq!(h.view.ctx.data(|d| d.get_temp::<String>(egui::Id::new("kw-input"))).as_deref(), Some("Retained keyword"));
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        // Info's blur-saved metadata and GPS must survive losing focus to the
        // shared editor, including its dismissal. Only a subsequent genuine blur
        // of the refocused Library field may publish a single undoable edit.
        let size = egui::vec2(1200., 2200.);
        h.host.library.as_mut().unwrap().run("panel.info", json!({})).unwrap();
        for _ in 0..3 {
            h.step_at(size, vec![]);
        }
        for (key, draft, buffer, next_field) in [
            ("title", "Retained title", egui::Id::new(("info-field", "title")), "field:caption"),
            ("gps", "51.5072, -0.1276", egui::Id::new("info-gps"), "field:title"),
        ] {
            let before = serde_json::to_value(&h.host.library.as_ref().unwrap().session.catalog.photo(photo).unwrap().meta).unwrap();
            let undo = h.host.library.as_ref().unwrap().session.undo.len();
            workflow_click(&mut h, size, &format!("field:{key}"));
            for pressed in [true, false] {
                h.step_at(size, vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
            }
            h.step_at(size, vec![egui::Event::Text(draft.into())]);
            let focus = ctx.memory(|m| m.focused()).unwrap();
            assert_eq!(ctx.data(|d| d.get_temp::<String>(buffer)).as_deref(), Some(draft));
            h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
            for _ in 0..3 {
                h.step_at(size, vec![]);
            }
            let shared = h.host.editor.ui.dialogs.last().unwrap().id;
            for capture_key in [egui::Key::Escape, egui::Key::Enter] {
                let fields = &mut h.host.editor.ui.dialog_mut(shared).unwrap().fields;
                fields.insert("selected".into(), json!("library::view.develop"));
                fields.insert("capture".into(), json!(true));
                ctx.memory_mut(|m| m.request_focus(focus));
                for pressed in [true, false] {
                    h.step_at(size, vec![egui::Event::Key { key: capture_key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }]);
                }
                assert_eq!(ctx.data(|d| d.get_temp::<String>(buffer)).as_deref(), Some(draft));
                let lib = h.host.library.as_ref().unwrap();
                assert_eq!(serde_json::to_value(&lib.session.catalog.photo(photo).unwrap().meta).unwrap(), before);
                assert_eq!(lib.session.undo.len(), undo, "capture must not publish metadata");
            }
            photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, shared).unwrap();
            for _ in 0..3 {
                h.step_at(size, vec![]);
            }
            assert_eq!(ctx.data(|d| d.get_temp::<String>(buffer)).as_deref(), Some(draft), "dismissal retains suspended {key}");
            assert_eq!(h.host.library.as_ref().unwrap().session.undo.len(), undo);
            workflow_click(&mut h, size, &format!("field:{key}"));
            workflow_click(&mut h, size, next_field);
            let lib = h.host.library.as_mut().unwrap();
            let meta = &lib.session.catalog.photo(photo).unwrap().meta;
            if key == "gps" {
                assert_eq!(meta.gps, Some((51.5072, -0.1276)));
            } else {
                assert_eq!(meta.title, draft);
            }
            assert_eq!(lib.session.undo.len(), undo + 1, "ordinary resumed blur commits exactly once");
            lib.run("edit.undo", json!({})).unwrap();
            assert_eq!(serde_json::to_value(&lib.session.catalog.photo(photo).unwrap().meta).unwrap(), before);
            h.step_at(size, vec![]);
        }
        let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(h);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn shared_shell_background_export_completes_while_capture_owns_input() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        struct Release(Arc<AtomicBool>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let (dir, mut host) = workflow_host("shared-shell-background-export");
        host.mode = Module::Library;
        set_library_view(host.library.as_mut().unwrap(), false);
        let output = dir.join("exported");
        std::fs::create_dir(&output).unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let signal = started.clone();
        let wait = release.clone();
        host.library.as_mut().unwrap().services.write_shared = Some(Arc::new(move |path, bytes| {
            signal.store(true, Ordering::SeqCst);
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !wait.load(Ordering::SeqCst) {
                if std::time::Instant::now() > until {
                    return Err("bounded export fixture timeout".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            std::fs::write(path, bytes).map_err(|e| e.to_string())
        }));
        let mut h = RoundTripUi::new(host);
        let _release = Release(release.clone());
        for _ in 0..4 {
            h.step(vec![]);
        }
        let photo = h.host.library.as_ref().unwrap().session.active().unwrap();
        h.host.library.as_mut().unwrap().run("app.export", json!({"ids":[photo.0],"dir":output.to_string_lossy(),"format":"png","background":true})).unwrap();
        h.wait_for(|_| started.load(Ordering::SeqCst));
        let cancel = h.host.library.as_ref().unwrap().export.as_ref().unwrap().cancel.clone();
        h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
        h.step(vec![]);
        let underlying = h.host.library.as_ref().unwrap().ui.dialog.clone();
        h.host.library.as_mut().unwrap().run("app.shortcuts", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        let id = h.host.editor.ui.dialogs.last().unwrap().id;
        let f = &mut h.host.editor.ui.dialog_mut(id).unwrap().fields;
        f.insert("selected".into(), json!("library::view.develop"));
        f.insert("capture".into(), json!(true));
        assert!(!h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "button:exportCancel"));
        assert!(!cancel.load(Ordering::SeqCst));
        release.store(true, Ordering::SeqCst);
        h.wait_for(|host| host.library.as_ref().unwrap().export.is_none());
        let lib = h.host.library.as_ref().unwrap();
        let result = lib.last_export_result.as_ref().unwrap();
        let file = result["files"][0]["path"].as_str().unwrap();
        assert!(std::fs::metadata(file).unwrap().len() > 0);
        assert_eq!(result["cancelled"], false);
        assert_eq!(lib.ui.dialog, underlying);
        assert_eq!(h.host.editor.ui.dialog_mut(id).unwrap().fields["capture"], true, "completion must not dismiss or confirm the shared editor");
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
        h.step(vec![]);
        assert_eq!(h.host.library.as_ref().unwrap().ui.dialog, underlying);
        let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
        drop(h);
        previews.retire();
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod bugfix_save_tests {
    use super::*;
    #[test]
    fn bugfix_save_o022_bad_library_setting_preserves_path_other_fields_and_original_backup() {
        let dir = std::env::temp_dir().join(format!("bugfix-save-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("library-ui.json");
        let bytes = br#"{"language":"retired-language","settings":{"libraryPath":"/my/real/library","previewEdge":3840},"autoAdvance":true}"#;
        std::fs::write(&path, bytes).unwrap();
        let (ui, rejected) = read_library_prefs(&path).unwrap();
        assert_eq!(ui.settings.library_path, "/my/real/library");
        assert_eq!(ui.settings.preview_edge, 3840);
        assert!(ui.auto_advance);
        assert_eq!(rejected, ["language"]);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(
            std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).any(|entry| entry.file_name().to_string_lossy().starts_with("library-ui.json.corrupt-"))
        );
        let truncated = br#"{"settings":{"libraryPath":"/my/real/library"},"language":"#;
        std::fs::write(&path, truncated).unwrap();
        assert!(read_library_prefs(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), truncated);
        assert!(read_library_prefs(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
