use super::*;

pub(super) fn config_dir() -> Option<PathBuf> {
    // Host startup tests must not read/write the owner's settings or clear their filter cache.
    if cfg!(test) {
        return Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/lrc-test-config").join(std::process::id().to_string()));
    }
    lightcraft_sysmem::app_dirs::config_dir()
}

pub(super) fn module_preference(value: Option<&str>) -> Module {
    match value.map(str::trim) {
        Some("develop") => Module::Develop,
        Some("editor") => Module::Compositing,
        _ => Module::Library,
    }
}

pub(super) fn validate_library_ui(ui: UiState, rejected: Vec<String>) -> Result<(UiState, Vec<String>), String> {
    if rejected.iter().any(|key| key.is_empty() || key == "settings" || key == "settings.libraryPath") {
        return Err(li_i18n::library::t("the stored Library path could not be recovered").into());
    }
    Ok((ui.sanitized(), rejected))
}
pub(super) fn decode_library_ui(value: serde_json::Value) -> Result<(UiState, Vec<String>), String> {
    let (ui, rejected) = photocraft_engine::prefs::lenient::<UiState>(value).map_err(|error| error.to_string())?;
    validate_library_ui(ui, rejected)
}

/// Keep already-open Develop sessions on the same effective model root as AI and Compositing.
pub(super) fn sync_library_model_root(lib: &mut LightcraftApp, root: &std::path::Path) {
    if lib.session.quick_seg_dir.as_deref() != Some(root) {
        lib.session.quick_seg_dir = Some(root.to_path_buf());
        lib.session.segmenter.dir = Some(root.join("sam3"));
        lib.session.segmenter.mirrors_file = Some(root.join("sam3-mirrors.txt"));
    }
}

/// Library settings state: the save cadence, and whether unreadable settings block opening a
/// Library. Application settings use the editor writer.
#[derive(Default)]
pub(super) struct PrefsWriter {
    /// A three-second cadence; serialising UI layout is not per-frame work.
    pub(super) checked: f64,
    pub(super) blocked: bool,
    pub(super) notice: Option<String>,
}

/// The library folder: `LOCAL_IMAGE_LIBRARY`, else the one last opened (Library settings), else
/// `~/Pictures/Local Image Library`.
pub(super) fn library_dir(prefs: Option<&UiState>) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LOCAL_IMAGE_LIBRARY").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = prefs.map(|u| u.settings.library_path.clone()).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE") } else { std::env::var_os("HOME") }?;
    Some(PathBuf::from(home).join("Pictures").join("Local Image Library"))
}

pub(super) fn open_library_app(opens: Opens, prefs_writer: &mut PrefsWriter, saved: Option<serde_json::Value>) -> LightcraftApp {
    use lightcraft_engine::Session;
    use lightcraft_ui_egui::panels::library_problem::LibraryProblem;
    let prefs = match saved {
        Some(value) => match value.get(crate::services::LIBRARY_UNREADABLE_KEY).and_then(serde_json::Value::as_str).map(str::to_string) {
            Some(error) => {
                prefs_writer.blocked = true;
                prefs_writer.notice = Some(error);
                None
            }
            None => match decode_library_ui(value) {
                Ok((ui, rejected)) => {
                    if !rejected.is_empty() {
                        prefs_writer.notice =
                            Some(li_i18n::library::t("Some Library settings used defaults: {fields}.").replace("{fields}", &rejected.join(", ")));
                    }
                    Some(ui)
                }
                Err(error) => {
                    prefs_writer.blocked = true;
                    prefs_writer.notice = Some(error);
                    None
                }
            },
        },
        None => None,
    };
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
    session.segmenter.dir = Some(li_ai::AiSettings::cached_model_dir().join("sam3"));
    session.segmenter.mirrors_file = Some(li_ai::AiSettings::cached_model_dir().join("sam3-mirrors.txt"));
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
    li_i18n::set_current(app.ui.language);
    app.host_fonts = true;
    app.integrated_titlebar = false;
    app.library_problem = problem;
    if let Some(notice) = prefs_writer.notice.take() {
        app.toast_error(&egui::Context::default(), notice);
    }
    app
}

/// Library › Add Photos dialog extensions: everything the folder scan imports
/// ([`lightcraft_engine::import::EXTENSIONS`]) plus `heif`. On Linux they also match upper and
/// mixed case (`DSC_0001.NEF`, `IMG_0001.JPG`): see [`photocraft_ui_egui::dialog_filter`].
pub(super) const ADD_PHOTOS_EXTS: &[&str] = lightcraft_codecs::PHOTO_EXTENSIONS;

pub(super) fn add_photos_extensions() -> Vec<&'static str> {
    ADD_PHOTOS_EXTS.iter().copied().filter(|ext| lightcraft_codecs::is_readable_extension(ext, true)).collect()
}
