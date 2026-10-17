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

mod filmstrip;
mod roundtrip;
mod services_wiring;
mod settings;
#[cfg(test)]
mod tests;

use filmstrip::*;
use roundtrip::*;
use services_wiring::*;
use settings::*;

/// Files the Library asked the editor to open (Edit in External Editor with no other editor set).
type Opens = Arc<Mutex<Vec<String>>>;
type SaveSourcesKey = (String, u64, u64, Vec<photocraft_ui_egui::develop_layer::LibraryOrigin>);

/// One theme authority: reuse editor semantic colours while retaining Library metrics.
fn library_palette(kind: photocraft_ui_egui::theme::ThemeKind) -> lightcraft_ui_egui::theme::Tokens {
    lightcraft_ui_egui::theme::host_palette(kind)
}

pub struct Host {
    pub editor: PhotocraftApp,
    library: Option<LightcraftApp>,
    automation_media: Option<crate::automation_library::MediaHooks>,
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
    filmstrip_previews: FilmstripPreviews,
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

/// The one application preferences file also owns the Library's UI state (its `library`
/// section), or the marker that the stored section could not be read.
fn library_ui_value(prefs: &photocraft_engine::prefs::Preferences) -> Option<serde_json::Value> {
    (prefs.library.contains_key("settings") || prefs.library.contains_key(crate::services::LIBRARY_UNREADABLE_KEY))
        .then(|| serde_json::to_value(&prefs.library).unwrap_or_default())
}
const LIBRARY_PREFS_PAGES: &[(&str, &str)] =
    &[("library.general", "General"), ("library.import", "Import"), ("library.performance", "Performance"), ("library.interface", "Interface")];
struct LibraryPreferencesPages(std::rc::Rc<std::cell::RefCell<Option<LightcraftApp>>>);
impl photocraft_ui_egui::prefs_ui::HostPreferencesPages for LibraryPreferencesPages {
    fn pages(&self) -> &'static [(&'static str, &'static str)] {
        LIBRARY_PREFS_PAGES
    }
    fn values(&self) -> serde_json::Value {
        self.0
            .borrow()
            .as_ref()
            .and_then(|lib| serde_json::to_value(lightcraft_ui_egui::panels::settings::PreferencesDraft::from_ui(&lib.ui)).ok())
            .unwrap_or_default()
    }
    fn body(&mut self, ui: &mut egui::Ui, section: &str, draft: &mut serde_json::Value) {
        if let Some(lib) = self.0.borrow_mut().as_mut() {
            lightcraft_ui_egui::panels::settings::shared_page(lib, ui, section.trim_start_matches("library."), draft);
        }
    }
    fn prepare(&self, draft: &serde_json::Value) -> Result<Box<dyn FnOnce() + '_>, String> {
        let values = serde_json::from_value::<lightcraft_ui_egui::panels::settings::PreferencesDraft>(draft.clone()).map_err(|e| e.to_string())?;
        let mut library = self.0.borrow_mut();
        if library.is_none() {
            return Err(li_i18n::library::t("The Library is not available for Preferences.").to_string());
        }
        // Keep the borrow through publication so the validated Library cannot disappear or
        // change between the checked write and its infallible runtime installation.
        Ok(Box::new(move || {
            if let Some(lib) = library.as_mut() {
                values.install(&mut lib.ui);
            }
        }))
    }
}

impl Host {
    pub fn new(editor: PhotocraftApp) -> Self {
        let mut editor = editor;
        editor.host_modes = std::env::var_os("LOCAL_IMAGE_NO_LIBRARY").is_none();
        let mode = if editor.host_modes {
            editor
                .session
                .prefs()
                .library
                .get("module")
                .and_then(serde_json::Value::as_str)
                .map(|value| module_preference(Some(value)))
                .unwrap_or(Module::Library)
        } else {
            Module::Compositing
        };
        Self {
            editor,
            library: None,
            automation_media: None,
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
            filmstrip_previews: FilmstripPreviews::default(),
            round_trip_notice: None,
        }
    }

    fn filmstrip_previews_ui(&mut self, ctx: &egui::Context) {
        let requests = photocraft_ui_egui::filmstrip_ui::take_preview_requests();
        if requests.is_empty() {
            if let Some(folder) = &mut self.filmstrip_previews.folder {
                folder.renderer.evict_thumbs(&Default::default(), 0);
            }
            return;
        }
        self.library();
        let Some(lib) = &mut self.library else { return };
        let display = self.editor.session.color.srgb_preview_display().ok();
        if let Some(display) = &display {
            lib.renderer.set_display(display.clone());
        }
        self.filmstrip_previews.service(ctx, lib, &requests, display);
    }

    /// One preferences writer remembers module choice as well as application settings.
    fn remember_module(&mut self, module: Module) {
        let name = match module {
            Module::Library => "library",
            Module::Develop => "develop",
            Module::Compositing => "editor",
        };
        self.editor.session.edit_prefs(|prefs| {
            prefs.library.insert("module".into(), serde_json::json!(name));
        });
    }

    /// One persisted shortcut map and one editor across modules; Library only mirrors it.
    fn shared_shell(&mut self, ctx: &egui::Context) {
        self.editor.host_preferences_pages = if self.editor.host_modes { LIBRARY_PREFS_PAGES } else { &[] };
        if let Some(language) = self.library.as_mut().and_then(|lib| lib.open_shared_language.take()) {
            self.editor.session.edit_prefs(|prefs| prefs.interface.language = language.code().to_string());
        }
        // One language for both halves: the Library follows the shared preference (Auto included).
        let language = li_i18n::Lang::from_pref(&self.editor.session.prefs().interface.language);
        if let Some(lib) = self.library.as_mut() {
            sync_library_model_root(lib, &li_ai::AiSettings::cached_model_dir());
            lib.ui.language = language;
            li_i18n::set_current(language);
        }
        if let Some(lib) = &self.library {
            self.editor.host_preferences_values =
                serde_json::to_value(lightcraft_ui_egui::panels::settings::PreferencesDraft::from_ui(&lib.ui)).unwrap_or_default();
        }
        if let Some(tab) = self.library.as_mut().and_then(|lib| lib.open_shared_preferences.take()) {
            let section = if tab == "attributions" { "attributions".to_string() } else { format!("library.{tab}") };
            photocraft_ui_egui::prefs_ui::open_preferences(&mut self.editor, &section);
        }
        if self.library.as_mut().is_some_and(|lib| std::mem::take(&mut lib.open_shared_updates)) {
            photocraft_ui_egui::updates_ui::start(&mut self.editor, ctx, true);
        }
        if self.library.as_mut().is_some_and(|lib| std::mem::take(&mut lib.open_shared_ai)) {
            if let Some(lib) = self.library.as_mut() {
                lib.ui.dialog = None;
            }
            photocraft_ui_egui::ai_ui::open_local_ai(&mut self.editor);
        }
        photocraft_ui_egui::prefs_ui::set_host_shortcut_catalog(&mut self.editor, ctx, lightcraft_ui_egui::shortcuts::catalog::host_catalog());
        let request = self.library.as_mut().is_some_and(|lib| std::mem::take(&mut lib.open_shared_shortcuts));
        if request {
            let id = photocraft_ui_egui::prefs_ui::open_shortcuts(&mut self.editor, 0);
            if let Some(dialog) = self.editor.ui.dialogs.iter_mut().find(|d| d.id == id) {
                dialog.fields.insert("shortcutScope".into(), serde_json::json!("library"));
            }
        }
        if let Some(lib) = self.library.as_mut() {
            lib.native_menu = self.editor.services.native_menu.is_some();
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
        let saved = library_ui_value(self.editor.session.prefs());
        let library = self.library.get_or_insert_with(|| {
            let mut lib = open_library_app(opens, &mut self.prefs, saved);
            if mode == Module::Develop {
                set_library_view(&mut lib, true);
            }
            lib
        });
        library.set_host_theme(library_palette(theme));
        library.native_menu = self.editor.services.native_menu.is_some();
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

    /// The native menu follows the module that owns the canvas, including the first frame.
    /// The shared backend keeps AppKit updates in place while the Library retains its own
    /// command parameters, enabled state, translations and shortcut rules.
    pub fn sync_native_menu(&mut self, ctx: &egui::Context) {
        if self.editor.services.native_menu.is_none() {
            if let Some(lib) = self.library.as_mut() {
                lib.native_menu = false;
            }
            return;
        }
        li_i18n::set_current(li_i18n::Lang::from_pref(&self.editor.session.prefs().interface.language));
        if self.mode == Module::Compositing {
            photocraft_ui_egui::native_menu::sync(&mut self.editor, ctx);
            return;
        }
        self.library();
        self.shared_shell(ctx);
        let Some(library) = self.library.as_ref() else { return };
        let mut bar = lightcraft_ui_egui::menubar::native_menu_bar(library);
        // Appearance belongs to the application even while the Library owns its command set.
        if let Some(app_menu) = bar.menus.iter_mut().find(|menu| menu.role == li_commands::menu::MenuRole::App) {
            if let Some(language) = app_menu
                .children
                .iter_mut()
                .find(|node| matches!(node, li_commands::menu::Node::Submenu { role: Some(li_commands::menu::ItemRole::Language), .. }))
            {
                *language = photocraft_ui_egui::native_menu::language_menu(&self.editor);
            }
            let at = app_menu.children.iter().position(|node| matches!(node, li_commands::menu::Node::Standard(_))).unwrap_or(app_menu.children.len());
            let at = if at > 0 && matches!(app_menu.children[at - 1], li_commands::menu::Node::Separator) { at - 1 } else { at };
            app_menu.children.insert(at, photocraft_ui_egui::native_menu::appearance_menu(&self.editor));
        }
        if !self.library_native_commands_available() {
            fn disable(nodes: &mut [li_commands::menu::Node]) {
                use li_commands::menu::{HIDE, MINIMIZE, Node};
                for node in nodes {
                    match node {
                        Node::Item(item) if item.id != HIDE && item.id != MINIMIZE => item.enabled = false,
                        Node::Submenu { children, .. } => disable(children),
                        _ => {}
                    }
                }
            }
            for menu in &mut bar.menus {
                disable(&mut menu.children);
            }
        }
        if let Some(menu) = self.editor.services.native_menu.as_mut() {
            menu.sync_bar(&bar);
        }
    }

    fn library_native_commands_available(&self) -> bool {
        photocraft_ui_egui::native_menu::commands_available(&self.editor)
            && self
                .library
                .as_ref()
                .is_none_or(|lib| lib.ui.dialog.is_none() && lib.ui.people.queue.is_none() && lib.ui.people.finder.is_none() && lib.ui.people.merge.is_none())
    }

    fn run_library_native_menu(&mut self, ctx: &egui::Context) {
        if self.mode == Module::Compositing {
            return;
        }
        let Some(menu) = self.editor.services.native_menu.as_mut() else { return };
        for id in menu.drain_clicks() {
            if id == li_commands::menu::MINIMIZE {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            } else if !self.library_native_commands_available() {
                continue;
            } else if id == "file.exit"
                || id.starts_with("window.theme.")
                || id.strip_prefix("app.language.").is_some_and(|code| code == "auto" || li_i18n::Lang::ALL.into_iter().any(|lang| lang.code() == code))
            {
                // Application appearance uses the host palette. A Quit queued from the
                // editor before a module change still uses its unsaved-document review.
                if let Err(error) = photocraft_ui_egui::menus::invoke(&mut self.editor, ctx, &id, serde_json::json!({})) {
                    photocraft_ui_egui::notices::error(&mut self.editor, error);
                }
            } else if let Err(error) = lightcraft_ui_egui::menubar::run_native_item(self.library(), &id) {
                self.library().toast_error(ctx, error);
            }
            self.shared_shell(ctx);
        }
    }

    fn with_preferences_ui(&mut self, ctx: &egui::Context, draw: impl FnOnce(&mut PhotocraftApp)) {
        if !self.editor.host_modes || !self.editor.ui.dialogs.iter().any(|d| photocraft_ui_egui::prefs_ui::is_preferences(&d.fields)) {
            draw(&mut self.editor);
            return;
        }
        // Preferences opened from Compositing also includes the current Library, loaded lazily.
        self.library();
        let Some(library) = self.library.take() else {
            draw(&mut self.editor);
            return;
        };
        let values = serde_json::to_value(lightcraft_ui_egui::panels::settings::PreferencesDraft::from_ui(&library.ui)).unwrap_or_default();
        for dialog in &mut self.editor.ui.dialogs {
            if photocraft_ui_egui::prefs_ui::is_preferences(&dialog.fields) && dialog.fields.get("hostValues").is_none_or(serde_json::Value::is_null) {
                dialog.fields.insert("hostValues".into(), values.clone());
            }
        }
        let lib = std::rc::Rc::new(std::cell::RefCell::new(Some(library)));
        let palette = library_palette(self.editor.ui.theme);
        ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, palette));
        self.editor.host_preferences = Some(Box::new(LibraryPreferencesPages(lib.clone())));
        let before = values;
        if let Some(host) = &self.editor.host_preferences {
            self.editor.host_preferences_pages = host.pages();
        }
        draw(&mut self.editor);
        for (id, _) in LIBRARY_PREFS_PAGES {
            if let Some(rect) = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-prefs-tab", *id)))) {
                lightcraft_ui_egui::widgets::register(ctx, format!("button:settingsTab-{}", id.trim_start_matches("library.")), rect);
            }
        }
        let applied = self.editor.host_preferences.as_ref().is_some_and(|host| before != host.values());
        self.editor.host_preferences = None;
        self.library = lib.borrow_mut().take();
        if self.mode == Module::Compositing
            && let Some(lib) = self.library.as_mut()
        {
            lib.widgets = lightcraft_ui_egui::widgets::take_registry(ctx);
        }
        self.capture_library_preferences(ctx, applied);
    }

    fn capture_library_preferences(&mut self, ctx: &egui::Context, force: bool) {
        let now = ctx.input(|input| input.time);
        if !force && self.editor.session.prefs().library.contains_key("settings") && now - self.prefs.checked < 3.0 {
            return;
        }
        self.prefs.checked = now;
        let Some(lib) = &self.library else { return };
        if self.prefs.blocked && (lib.library_problem.is_some() || lib.ui.settings.library_path.is_empty()) {
            return;
        }
        if let Ok(serde_json::Value::Object(value)) = serde_json::to_value(&lib.ui)
            && value.iter().any(|(key, value)| self.editor.session.prefs().library.get(key) != Some(value))
        {
            self.editor.session.edit_prefs(|prefs| {
                prefs.library.extend(value);
                if lib.library_problem.is_none() && lib.session.library.is_some() {
                    prefs.library.remove(crate::services::LIBRARY_UNREADABLE_KEY);
                }
            });
        }
        photocraft_ui_egui::prefs_ui::persist_background(&mut self.editor, ctx);
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
                        let label = li_i18n::library::t("Edited in Compositing · unsaved").to_string();
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
        // A custom full-canvas modal owns module navigation too. Otherwise a host chord or
        // queued switch could hide its only Cancel action in another workspace.
        if !photocraft_ui_egui::native_menu::commands_available(&self.editor) {
            return;
        }
        let from = self.mode;
        if from == Module::Compositing
            && to == Module::Develop
            && photocraft_ui_egui::develop_layer::active_photo(&self.editor).is_some()
            && !photocraft_ui_egui::develop_layer::active_origin(&self.editor).is_some_and(|origin| matches_origin(&self.library().session, &origin))
        {
            self.editor.ui.status = "This Develop layer belongs to another Library; open the correct Library before returning to Develop".into();
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
        self.remember_module(to);
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
        self.run_library_native_menu(ctx);
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
                    self.remember_module(Module::Compositing);
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
            if let Some(browse) = self.editor.automation_browse.take() {
                let mut hooks = self.automation_media.take();
                let result = crate::automation_library::show(self.library(), browse, &mut hooks);
                self.automation_media = hooks;
                if let Err(error) = result {
                    self.editor.ui.status = error;
                    self.editor.ui.status_error = true;
                    return;
                }
            }
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
                self.editor.ui.status = "This Develop layer belongs to another Library; reopen its photo from the correct Library".into();
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
                        self.remember_module(now);
                    }
                }
            }
            _ => {
                if let Some(lib) = self.library.as_mut() {
                    lib.background_tick(ctx);
                }
            }
        }
        self.capture_library_preferences(ctx, false);
        photocraft_ui_egui::prefs_ui::sync_shared_interface(&mut self.editor, ctx);
        self.restyle(ctx);
        photocraft_ui_egui::prefs_ui::sync_shared_tooltips(&self.editor, ctx);
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
        // Native menu synchronization may create the Library before the first frame. The
        // editor queues our named font families in that frame's logic, and egui activates
        // them on the next pass. Shared title/dialog controls need the same first-frame
        // admission as the editor UI, even when an eagerly created Library owns the canvas.
        if self.frames <= 1 {
            ui.ctx().request_repaint();
            return;
        }
        crate::monitor_profile::configure_surface(frame, &self.editor.session.color);
        self.shared_shell(ui.ctx());
        if self.mode == Module::Compositing || self.library.is_none() {
            let ctx = ui.ctx().clone();
            self.with_preferences_ui(&ctx, |editor| editor.ui(ui, frame));
            self.filmstrip_previews_ui(&ctx);
            self.sync_native_menu(&ctx);
            return;
        }
        let title = match &self.camera_raw {
            Some(cr) => format!("Camera Raw Filter · {}", cr.name),
            None => self.library.as_ref().map(|l| library_title(l, self.mode)).unwrap_or_default(),
        };
        let modal = !self.editor.ui.dialogs.is_empty();
        let Some(library) = self.library.as_mut() else {
            let ctx = ui.ctx().clone();
            self.with_preferences_ui(&ctx, |editor| editor.ui(ui, frame));
            self.shared_shell(&ctx);
            self.sync_native_menu(&ctx);
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
        photocraft_ui_egui::monitor_status::poll(&mut self.editor, ui.ctx());
        if let Some(lib) = self.library.as_mut() {
            match self.editor.session.color.srgb_preview_display() {
                Ok(display) => lib.renderer.set_display(display),
                Err(e) => log::warn!("Library display transform: {e}"),
            }
            ui.add_enabled_ui(!lib.host_modal, |ui| lib.ui(ui));
        }
        self.shared_shell(ui.ctx());
        let ctx = ui.ctx().clone();
        self.with_preferences_ui(&ctx, |editor| photocraft_ui_egui::dialogs::show(editor, &ctx));
        photocraft_ui_egui::updates_ui::show(&mut self.editor, &ctx);
        photocraft_ui_egui::ai_ui::windows(&mut self.editor, ui.ctx());
        photocraft_ui_egui::notices::show(&mut self.editor, ui.ctx());
        // The switched-to-CPU warning appears in every module, not only once Compositing is open.
        photocraft_ui_egui::gpu_status::show_fallback(&mut self.editor, ui.ctx());
        photocraft_ui_egui::overwrite_original::show(&mut self.editor, ui.ctx());
        if let Some(lib) = self.library.as_mut() {
            lib.widgets.extend(lightcraft_ui_egui::widgets::take_registry(ui.ctx()));
        }
        photocraft_ui_egui::panels::host_resize_zones(&self.editor, ui);
        // Include actions and module switches performed during this UI frame before the next
        // native event arrives. One model refresh per frame also keeps checks immediately live.
        self.sync_native_menu(ui.ctx());
    }

    fn on_exit(&mut self) {
        let mut handoff = std::mem::take(&mut self.editor.library_vectorize);
        handoff.cancel(&mut self.editor);
        self.editor.library_vectorize = handoff;
        self.editor.trace_dialog.cancel(&mut self.editor.session);

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
        if let Some(lib) = self.library.as_mut()
            && let Err(e) = lib.session.close_library()
        {
            log::error!("saving the library failed: {e}");
        }
        self.capture_library_preferences(&egui::Context::default(), true);
        self.editor.on_exit();
        if !lightcraft_gpu::quiesce(std::time::Duration::from_secs(3)) {
            log::warn!("GPU work still running at exit");
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
