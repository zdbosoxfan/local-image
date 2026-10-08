//! The Library mode: LightCraft's photo library (import, grid, loupe, compare, survey, ratings,
//! flags, colour labels, keywords, collections, smart collections, stacks, develop, export) hosted
//! next to the editor in one window, Lightroom-style.
//!
//! [`Host`] owns both apps and shows one at a time. The mode switch sits in each mode's title bar.
//! While a mode is hidden, its background work goes on (`background_tick`): library imports,
//! exports and saves, and the editor's background jobs. Edit in Local Image (Photo › Edit in…)
//! renders a 16-bit TIFF that is stacked with the original, then opens it in the editor in this
//! process. Back in the Library, the edited copy is reloaded.
//!
//! The library opens lazily, the first time the Library mode is shown.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lightcraft_ui_egui::{LightcraftApp, Services, UiState};
use photocraft_ui_egui::PhotocraftApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Library,
}

/// Files the Library asked the editor to open (Edit in Local Image).
type Opens = Arc<Mutex<Vec<String>>>;

pub struct Host {
    pub editor: PhotocraftApp,
    library: Option<LightcraftApp>,
    prefs: PrefsWriter,
    mode: Mode,
    /// The mode whose visuals are applied to the context.
    styled: Option<Mode>,
    opens: Opens,
    frames: u64,
}

impl Host {
    pub fn new(editor: PhotocraftApp) -> Self {
        let mut editor = editor;
        editor.host_modes = std::env::var_os("LOCAL_IMAGE_NO_LIBRARY").is_none();
        let mode = if editor.host_modes && last_mode_was_library() { Mode::Library } else { Mode::Editor };
        Self { editor, library: None, prefs: PrefsWriter::default(), mode, styled: None, opens: Arc::default(), frames: 0 }
    }

    fn library(&mut self) -> &mut LightcraftApp {
        let opens = self.opens.clone();
        self.library.get_or_insert_with(|| open_library_app(opens, &mut self.prefs))
    }

    fn switch(&mut self, ctx: &egui::Context, to: Mode) {
        if self.mode == to {
            return;
        }
        self.mode = to;
        remember_mode(to);
        if to == Mode::Library {
            // Back from the editor: pick up edited copies saved there.
            let lib = self.library();
            let ids = std::mem::take(&mut lib.ui.external_edits);
            if !ids.is_empty()
                && let Ok(r) = lib.session.execute("photo.reload", &serde_json::json!({"ids": ids}))
                && r["reloaded"].as_array().is_some_and(|a| !a.is_empty())
            {
                lib.toast(ctx, "Updated the edits saved in the editor");
            }
        }
        ctx.request_repaint();
    }

    fn restyle(&mut self, ctx: &egui::Context) {
        if self.styled == Some(self.mode) {
            return;
        }
        match self.mode {
            Mode::Editor => self.editor.restyle(ctx),
            Mode::Library => {
                if let Some(l) = &self.library {
                    l.restyle(ctx);
                }
            }
        }
        self.styled = Some(self.mode);
    }
}

impl eframe::App for Host {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frames += 1;
        // The editor installs the fonts (a superset of the Library's) on its first frame.
        if self.frames == 1 || self.mode == Mode::Editor {
            self.editor.logic(ctx, frame);
        } else {
            self.editor.background_tick(ctx);
        }
        // Quitting from the Library with unsaved documents: the editor asks about them.
        if self.mode == Mode::Library && ctx.input(|i| i.viewport().close_requested()) && self.editor.has_unsaved() {
            self.switch(ctx, Mode::Editor);
            self.editor.logic(ctx, frame);
        }
        let opened: Vec<String> = std::mem::take(&mut *self.opens.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        if !opened.is_empty() {
            self.editor.open_paths(&opened);
            self.switch(ctx, Mode::Editor);
        }
        if std::mem::take(&mut self.editor.switch_to_library) {
            self.switch(ctx, Mode::Library);
        }
        match self.mode {
            Mode::Library if self.frames > 1 => {
                self.library().logic(ctx);
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
            (Mode::Library, Some(lib)) => lib.raw_input_hook(raw),
            _ => self.editor.raw_input_hook(ctx, raw),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.mode == Mode::Editor || self.library.is_none() {
            self.editor.ui(ui, frame);
            return;
        }
        let title = self.library.as_ref().map(library_title).unwrap_or_default();
        if photocraft_ui_egui::panels::library_title_bar(&mut self.editor, ui, &title) {
            let ctx = ui.ctx().clone();
            self.switch(&ctx, Mode::Editor);
            ctx.request_repaint();
        }
        if let Some(lib) = self.library.as_mut() {
            lib.ui(ui);
        }
        photocraft_ui_egui::panels::host_resize_zones(&self.editor, ui);
    }

    fn on_exit(&mut self) {
        self.editor.on_exit();
        if let Some(lib) = self.library.as_mut() {
            if let Err(e) = self.prefs.save(lib) {
                eprintln!("local-image: {e}");
            }
            if let Err(e) = lib.session.close_library() {
                eprintln!("local-image: saving the library failed: {e}");
            }
        }
    }
}

fn library_title(lib: &LightcraftApp) -> String {
    let n = lib.session.catalog.len();
    format!("Library · {n} photo{}", if n == 1 { "" } else { "s" })
}

// ------------------------------------------------------------------------------ settings

fn config_dir() -> Option<PathBuf> {
    crate::app_dirs::config_dir()
}

/// Which mode was shown last (`<config>/mode`), so the app reopens where the user was.
fn last_mode_was_library() -> bool {
    config_dir().and_then(|d| std::fs::read_to_string(d.join("mode")).ok()).is_some_and(|s| s.trim() == "library")
}

fn remember_mode(m: Mode) {
    if std::env::var_os("LOCAL_IMAGE_NO_PREFS").is_some() {
        return;
    }
    if let Some(d) = config_dir() {
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join("mode"), if m == Mode::Library { "library" } else { "editor" });
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
    }
}
