use super::*;

pub(super) fn services(opens: Opens) -> Services {
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
            crate::file_dialog_probe::explain_empty(file_dialog().set_title(li_i18n::library::t(title)).pick_folder()).map(|p| p.to_string_lossy().to_string())
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
            crate::file_dialog_probe::explain_empty(file_dialog().add_filter_nocase("Photos", &add_photos_extensions()).pick_files())
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
        png: Some(Box::new(|img: &li_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        backup_library: None,
        restore_library: None,
        doc_layers: Some(Arc::new(crate::doc_layers::load)),
    }
}
