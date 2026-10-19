use super::*;
use lightcraft_ui_egui::FileDialogRequest;
use photocraft_ui_egui::{FileDialogSpec, Pick};

/// The title and file types of each Library dialog, which the editor's dialog service shows.
pub(super) fn library_dialog(request: FileDialogRequest) -> FileDialogSpec {
    match request {
        FileDialogRequest::Photos => FileDialogSpec::new(Pick::Files).filter("Photos", &add_photos_extensions()),
        FileDialogRequest::Presets => FileDialogSpec::new(Pick::Files)
            .title("Import Presets")
            .filter("Presets & Profiles", &["lcpreset", "xmp", "lrtemplate", "zip", "dng", "lmp", "mplumpack", "cube"]),
        FileDialogRequest::Tracklog => FileDialogSpec::new(Pick::File).title("Auto-Tag from Tracklog").filter("GPS Track Log", &["gpx"]),
        FileDialogRequest::CurvePresets => {
            FileDialogSpec::new(Pick::Files).title("Import Point Curve Presets").filter("Point Curve Presets", &["lccurve", "json"])
        }
        FileDialogRequest::SavePreset { name } => FileDialogSpec::new(Pick::Save).title("Export Presets").filter("Preset", &["lcpreset"]).file_name(name),
        FileDialogRequest::SaveCurvePresets { name } => {
            FileDialogSpec::new(Pick::Save).title("Export Point Curve Presets").filter("Point Curve Presets", &["lccurve"]).file_name(name)
        }
        FileDialogRequest::SaveDenoised { name } => FileDialogSpec::new(Pick::Save).filter("Denoised image", &["tiff", "tif", "dng"]).file_name(name),
        FileDialogRequest::Folder { title: Some(title) } => FileDialogSpec::new(Pick::Folder).title(li_i18n::library::t(&title)),
        FileDialogRequest::Folder { title: None } => FileDialogSpec::new(Pick::Folder).title("Choose Folder"),
    }
}

pub(super) fn services(opens: Opens) -> Services {
    Services {
        trash_file: li_ai::trash::available()
            .then(|| Box::new(|path: &str| li_ai::trash::move_to_trash(std::path::Path::new(path)).map(|_| ()).map_err(|e| e.to_string())) as _),
        file_dialog: Some(Box::new(|request, reply| {
            crate::services::show_file_dialog(photocraft_ui_egui::FileDialogRequest::Described(library_dialog(request)), reply)
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
        write_shared: Some(Arc::new(lightcraft_engine::export::write_file)),
        write: Some(Box::new(lightcraft_engine::export::write_file)),
        png: Some(Box::new(|img: &li_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        backup_library: None,
        restore_library: None,
        doc_layers: Some(Arc::new(crate::doc_layers::load)),
    }
}
