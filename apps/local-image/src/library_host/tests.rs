#[test]
fn j23_raw_standin_uses_shared_container_orientation_before_development() {
    let mut jpeg = Vec::new();
    let rgb = image::RgbImage::from_fn(48, 32, |x, y| image::Rgb([(x * 5) as u8, (y * 7) as u8, 60]));
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode(rgb.as_raw(), 48, 32, image::ExtendedColorType::Rgb8).unwrap();
    // TIFF root orientation plus JPEG interchange offsets; the JPEG has no EXIF tag.
    let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
    bytes.extend_from_slice(&3u16.to_le_bytes());
    for (tag, kind, value) in [(274u16, 3u16, 6u32), (513, 4, 50), (514, 4, jpeg.len() as u32)] {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&jpeg);
    let dir = PathBuf::from("target").join(format!("j23-raw-preview-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("synthetic.arw");
    std::fs::write(&path, &bytes).unwrap();
    let old = photocraft_ui_egui::filmstrip_ui::thumbnail(&path).unwrap();
    let preview = lightcraft_engine::files::fs_preview_loader()(path.to_str().unwrap(), 128).unwrap();
    assert_eq!(old.size, [48, 32], "legacy filmstrip ignores RAW container orientation");
    assert_eq!((preview.width, preview.height), (32, 48), "shared preview applies container orientation before sensor decode");
    let source = lightcraft_codecs::decode(&jpeg, Default::default()).unwrap().to_srgb8();
    // Clockwise rotation: first output pixel is the old bottom-left pixel.
    assert_eq!(preview.data[0], source.data[31 * 48]);
    eprintln!("J23 RAW orientation: old 48x32 landscape, shared 32x48 portrait, first oriented pixel exact");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn j23_host_developed_thumbnail_is_library_texture_and_preserves_selection() {
    use photocraft_ui_egui::filmstrip_ui;
    let (dir, path, mut lib, photo) = library_with_photo("j23-catalog");
    lib.run("develop.set", json!({"control":"light.exposure", "value":1.5})).unwrap();
    lib.run("crop.set", json!({"rect":[0.0,0.0,0.5,1.0]})).unwrap();
    lib.renderer.keep_pixels = true;
    let before_selection = lib.session.selection.clone();
    let before_catalog = serde_json::to_value(&lib.session.catalog).unwrap();
    let mut host = Host::new(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    host.library = Some(lib);
    host.mode = Module::Compositing;
    filmstrip_ui::set_folder(&dir, vec![PathBuf::from(&path)]);
    let mut h = RoundTripUi::new(host);
    h.wait_for(|host| host.library.as_ref().unwrap().renderer.thumb(photo).is_some());
    let lib = h.host.library.as_mut().unwrap();
    let expected = lib.session.thumb_job(photo, 128).unwrap().run().rendered.unwrap().image;
    let tex = lib.renderer.thumb(photo).unwrap();
    let actual = tex.pixels.as_ref().unwrap();
    assert_eq!(actual.size, [expected.width, expected.height]);
    let golden = egui::ColorImage::from_rgba_unmultiplied(actual.size, &expected.as_bytes());
    assert_eq!(actual.pixels, golden.pixels, "0 differing channels against Library's developed and cropped render");
    assert!(actual.size[0] < actual.size[1]);
    let legacy_start = std::time::Instant::now();
    let legacy = filmstrip_ui::thumbnail(std::path::Path::new(&path)).unwrap();
    let legacy_ms = legacy_start.elapsed().as_secs_f64() * 1000.0;
    let bytes: Vec<_> = legacy.pixels.iter().flat_map(|p| p.to_array()).collect();
    let old = image::RgbaImage::from_raw(legacy.size[0] as u32, legacy.size[1] as u32, bytes).unwrap();
    let old = image::imageops::crop_imm(&old, 0, 0, old.width() / 2, old.height()).to_image();
    let old = image::imageops::resize(&old, actual.size[0] as u32, actual.size[1] as u32, image::imageops::FilterType::Triangle);
    let legacy_error = actual
        .pixels
        .iter()
        .zip(old.pixels())
        .map(|(p, old)| p.to_array().iter().zip(old.0).take(3).map(|(a, b)| a.abs_diff(b) as f64).sum::<f64>())
        .sum::<f64>()
        / (actual.pixels.len() * 3) as f64;
    assert!(legacy_error > 5.0);
    eprintln!(
        "J23 catalog golden: old dimensions {:?}, developed {:?}, old normalized-crop RGB MAE={legacy_error:.3}, new MAE=0, legacy decode {legacy_ms:.3} ms",
        legacy.size, actual.size
    );
    assert!(h.host.filmstrip_previews.folder.is_none(), "catalog thumbnails start no extra workers/cache");
    assert_eq!(h.host.library.as_ref().unwrap().session.selection, before_selection);
    assert_eq!(serde_json::to_value(&h.host.library.as_ref().unwrap().session.catalog).unwrap(), before_catalog);
    let started = std::time::Instant::now();
    for _ in 0..5 {
        h.step(vec![]);
    }
    eprintln!(
        "J23 real headless host path: settled frame {:.3} ms, shared textures {}, folder workers 0",
        started.elapsed().as_secs_f64() * 200.0,
        h.host.library.as_ref().unwrap().renderer.thumb_textures()
    );
    // Settled frames must neither rebuild nor resubmit the thumbnail.
    assert_eq!(h.host.library.as_ref().unwrap().renderer.in_flight(), 0);
    let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
    drop(h);
    previews.retire();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn j23_folder_cache_warm_reopen_skips_decode_and_refreshes_same_size_save() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = PathBuf::from("target").join(format!("j23-folder-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.png");
    image::RgbaImage::from_fn(192, 128, |x, y| image::Rgba([(x * 255 / 191) as u8, (y * 255 / 127) as u8, 80, 255])).save(&path).unwrap();
    let counts = Arc::new(AtomicUsize::new(0));
    let mut session = lightcraft_engine::Session::new().with_fs();
    session.media.attach_disk_cache(&dir.join("thumbs"), 4 << 20);
    let load = session.media.file_loader.clone().unwrap();
    let reads = counts.clone();
    session.media.file_loader = Some(Arc::new(move |path, edge| {
        reads.fetch_add(1, Ordering::SeqCst);
        load(path, edge)
    }));
    let cold_job = folder_thumb_job(&mut session, path.to_str().unwrap(), 256).unwrap();
    let cache_key = cold_job.cache.as_ref().unwrap().1;
    let start = std::time::Instant::now();
    let cold = cold_job.run().rendered.unwrap().image;
    let cold_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(counts.load(Ordering::SeqCst), 1);
    let mut reopen = lightcraft_engine::Session::new().with_fs();
    reopen.media.attach_disk_cache(&dir.join("thumbs"), 4 << 20);
    let reads = counts.clone();
    reopen.media.file_loader = Some(Arc::new(move |_, _| {
        reads.fetch_add(1, Ordering::SeqCst);
        Err("warm cache must not decode".into())
    }));
    let start = std::time::Instant::now();
    let warm = folder_thumb_job(&mut reopen, path.to_str().unwrap(), 256).unwrap().run().rendered.unwrap().image;
    let warm_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(counts.load(Ordering::SeqCst), 1, "warm disk reuse performs zero source decodes");
    assert_eq!((cold.width, cold.height), (warm.width, warm.height));
    let delta: Vec<_> = cold.as_bytes().iter().zip(warm.as_bytes()).map(|(a, b)| a.abs_diff(b)).collect();
    let mae = delta.iter().map(|d| *d as f64).sum::<f64>() / delta.len() as f64;
    assert!(mae < 2.0, "small sample cache MAE {mae}");
    eprintln!(
        "J23 folder sample: cold render {cold_ms:.3} ms, warm reopen including probe/hash {warm_ms:.3} ms, source decodes cold=1/warm=0, disk MAE={mae:.4}, max={}",
        delta.iter().max().unwrap()
    );
    // Equal-sized BMP pixel changes must get a new true content key despite the same name.
    let bmp = dir.join("saved.bmp");
    image::RgbImage::from_pixel(16, 16, image::Rgb([180, 10, 10])).save(&bmp).unwrap();
    let bytes = std::fs::metadata(&bmp).unwrap().len();
    let mut a = lightcraft_engine::Session::new().with_fs();
    a.media.rendered = reopen.media.rendered.clone();
    let old_job = folder_thumb_job(&mut a, bmp.to_str().unwrap(), 128).unwrap();
    let old_key = old_job.cache.as_ref().unwrap().1;
    let old = old_job.run().rendered.unwrap().image;
    image::RgbImage::from_pixel(16, 16, image::Rgb([10, 10, 180])).save(&bmp).unwrap();
    assert_eq!(std::fs::metadata(&bmp).unwrap().len(), bytes);
    let mut b = lightcraft_engine::Session::new().with_fs();
    b.media.rendered = reopen.media.rendered.clone();
    let new_job = folder_thumb_job(&mut b, bmp.to_str().unwrap(), 128).unwrap();
    assert_ne!(new_job.cache.as_ref().unwrap().1, old_key);
    assert_ne!(new_job.run().rendered.unwrap().image.as_bytes(), old.as_bytes());
    assert!(reopen.media.rendered.get(cache_key).is_some());
    session.media.rendered.retire();
    reopen.media.rendered.retire();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn j23_host_folder_service_deduplicates_limits_textures_and_preserves_catalog() {
    use lightcraft_ui_egui::render::Slot;
    use photocraft_ui_egui::filmstrip_ui::PreviewRequest;
    let dir = PathBuf::from("target").join(format!("j23-service-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plain.png");
    image::RgbaImage::from_pixel(48, 32, image::Rgba([60, 100, 180, 255])).save(&path).unwrap();
    let mut library = LightcraftApp::new(lightcraft_engine::Session::new().with_fs(), Services::default());
    library.session.media.attach_disk_cache(&dir.join("thumbs"), 4 << 20);
    let before = serde_json::to_value(&library.session.catalog).unwrap();
    let request = PreviewRequest { original: path.clone(), source: path.clone(), epoch: 1, revision: 0, edge: 128, priority: 10 };
    let ctx = egui::Context::default();
    let mut service = FilmstripPreviews::default();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        service.service(&ctx, &mut library, std::slice::from_ref(&request), None);
        let id = service.entries[&path].id;
        if service.folder.as_ref().unwrap().renderer.thumb(id).is_some() {
            break;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let id = service.entries[&path].id;
    let texture = service.folder.as_ref().unwrap().renderer.thumb(id).unwrap().tex.id();
    for _ in 0..5 {
        service.service(&ctx, &mut library, std::slice::from_ref(&request), None);
    }
    let folder = service.folder.as_ref().unwrap();
    assert_eq!(folder.renderer.thumb(id).unwrap().tex.id(), texture);
    assert!(!folder.renderer.is_pending(Slot::Thumb(id)));
    assert_eq!(folder.renderer.thumb_textures(), 1);
    assert!(Arc::ptr_eq(&folder.session.media.rendered, &library.session.media.rendered));
    assert_eq!(serde_json::to_value(&library.session.catalog).unwrap(), before);
    assert!(library.session.selection.ids.is_empty());
    service.service(&ctx, &mut library, &[], None);
    assert!(service.entries.is_empty());
    assert_eq!(service.folder.as_ref().unwrap().renderer.thumb_textures(), 0);
    drop(service);
    library.session.media.rendered.retire();
    std::fs::remove_dir_all(dir).unwrap();
}

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
fn shared_decode_add_photos_picker_uses_readable_registry() {
    let extensions = add_photos_extensions();
    for ext in ["heic", "heif", "hif", "jxl", "arq", "crw"] {
        assert!(extensions.contains(&ext), "{ext}");
    }
    assert!(!extensions.contains(&"avif"));
}

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
            if let photocraft_doc::LayerContent::Smart(smart) = &layer.content { smart.develop.as_ref().filter(|link| link.photo == Some(copy)) } else { None }
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
    let doc = |n: &str| photocraft_doc::Document::new(n, photocraft_doc::Size::new(4, 4), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
    editor.session.open_document(doc("a"), Some("/photos/a.psd".into()));
    editor.session.open_document(doc("b"), Some("/photos/b.psd".into()));
    assert_eq!(editor.session.active_index(), Some(1));
    assert!(focus_open_document(&mut editor, "/photos/a.psd"));
    assert_eq!(editor.session.active_index(), Some(0));
    assert!(!focus_open_document(&mut editor, "/photos/c.psd"));
    assert_eq!(editor.session.active_index(), Some(0));
}

// Library ↔ Compositing round trip

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
    app.ui.settings.library_path = dir.join("library").to_string_lossy().to_string();
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
fn module_shortcuts_cannot_hide_custom_filter_modals() {
    for command in ["filter.filterGallery", "filter.liquify"] {
        let mut editor = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        editor.run("file.new", json!({"width":32,"height":24,"background":"transparent"})).unwrap();
        editor.run("edit.fill", json!({"color":"#3366cc"})).unwrap();
        let mut host = Host::new(editor);
        host.editor.host_modes = true;
        host.mode = Module::Compositing;
        let mut h = RoundTripUi::new(host);
        for _ in 0..3 {
            h.step(vec![]);
        }
        let ctx = h.view.ctx.clone();
        photocraft_ui_egui::distort_ui::menu(&mut h.host.editor, &ctx, command, &json!({})).unwrap().unwrap();
        h.key(egui::Key::Num1, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
        assert_eq!(h.host.mode, Module::Compositing);
        assert!(!photocraft_ui_egui::native_menu::commands_available(&h.host.editor));
        photocraft_ui_egui::distort_ui::menu(&mut h.host.editor, &ctx, command, &json!({"ui":{"cancel":true}})).unwrap().unwrap();
        assert!(photocraft_ui_egui::native_menu::commands_available(&h.host.editor));
    }
}

#[test]
fn native_library_startup_waits_for_the_first_named_font_pass() {
    use eframe::App;
    use li_commands::menu::{Backend, Event, MenuBar};
    use photocraft_ui_egui::native_menu::NativeMenu;
    struct Menu;
    impl Backend for Menu {
        fn sync(&mut self, _: &MenuBar) {}
        fn drain(&mut self) -> Vec<Event> {
            Vec::new()
        }
    }
    for kind in [photocraft_ui_egui::theme::ThemeKind::ProMedium, photocraft_ui_egui::theme::ThemeKind::StudioLight] {
        for mode in [Module::Library, Module::Develop] {
            let (dir, mut host) = workflow_host("native-library-font-startup");
            workflow_theme(&mut host, kind);
            host.mode = mode;
            set_library_view(host.library.as_mut().unwrap(), mode == Module::Develop);
            host.editor.custom_titlebar = true;
            host.editor.services.native_menu = Some(NativeMenu::new(Box::new(Menu)));
            // Unlike RoundTripUi, the native creator has not preinstalled any fonts.
            let ctx = egui::Context::default();
            host.sync_native_menu(&ctx);
            assert!(host.library.is_some(), "native menu synchronization eagerly opens Library");
            let mut frame = eframe::Frame::_new_kittest();
            for pass in 0..2 {
                let raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(egui::vec2(1200., 900.), 1., pass as f64 / 60., vec![]);
                let mut output = ctx.run_ui(raw, |ui| {
                    let medium_active = ui.ctx().fonts(|fonts| fonts.definitions().families.contains_key(&egui::FontFamily::Name("medium".into())));
                    assert_eq!(medium_active, pass > 0, "{kind:?}/{mode:?}: fonts become active after the first pass");
                    host.logic(ui.ctx(), &mut frame);
                    host.ui(ui, &mut frame);
                });
                output.textures_delta.clear();
                let title_drawn = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", mode.digit())))).is_some();
                assert_eq!(title_drawn, pass > 0, "{kind:?}/{mode:?}: shared title waits for installed fonts");
            }
            drop(host);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn native_menu_keys_and_quit_click_reach_the_library_after_startup() {
    use li_commands::menu::{Backend, Event, MenuBar};
    use li_keys::Chord;
    use photocraft_ui_egui::native_menu::NativeMenu;
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
    h.step(vec![]); // The editor's startup logic does not run in this workspace.
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

#[test]
fn j21_native_library_menu_tracks_active_module_parameters_and_modal_state() {
    use li_commands::menu::{Backend, Event, MenuBar};
    use photocraft_ui_egui::native_menu::NativeMenu;
    struct Queue {
        events: Arc<Mutex<Vec<Event>>>,
        bar: Arc<Mutex<MenuBar>>,
    }
    impl Backend for Queue {
        fn sync(&mut self, bar: &MenuBar) {
            *self.bar.lock().unwrap() = bar.clone();
        }
        fn drain(&mut self) -> Vec<Event> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
    }
    let (dir, mut host) = workflow_host("native-active-module");
    host.mode = Module::Library;
    let photo = host.library.as_ref().unwrap().session.catalog.photos().next().unwrap().id;
    host.library.as_mut().unwrap().session.selection = lightcraft_engine::Selection::single(photo);
    let events = Arc::new(Mutex::new(Vec::new()));
    let bar = Arc::new(Mutex::new(MenuBar::default()));
    host.editor.services.native_menu = Some(NativeMenu::new(Box::new(Queue { events: events.clone(), bar: bar.clone() })));
    let mut h = RoundTripUi::new(host);
    h.host.sync_native_menu(&h.view.ctx);
    let rate = li_commands::menu::CommandMenuNode::key("photo.rate", &json!({"rating": 4}));
    assert!(bar.lock().unwrap().find(&rate).unwrap().enabled);
    assert!(bar.lock().unwrap().find("file.open").is_none());
    assert!(h.host.library.as_ref().unwrap().native_menu, "native Library must hide in-window menus from startup");
    h.step(vec![]);
    h.step(vec![]);
    assert!(!h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "menu:File"));

    assert!(bar.lock().unwrap().find("window.theme.toggle").is_some(), "application Appearance remains available in Library");
    assert!(bar.lock().unwrap().find("app.language.auto").is_some(), "application Language retains system-language selection");
    let old_theme = h.host.editor.ui.theme;
    events.lock().unwrap().push(Event::Click("window.theme.toggle".into()));
    h.step(vec![]);
    assert_ne!(h.host.editor.ui.theme, old_theme);

    let editor_layers = h.host.editor.session.active().unwrap().doc.layers.len();
    events.lock().unwrap().push(Event::Click(rate.clone()));
    h.step(vec![]);
    assert_eq!(catalog(&h.host).catalog.photo(photo).unwrap().rating, 4);
    assert_eq!(bar.lock().unwrap().find(&rate).unwrap().checked, Some(true));
    events.lock().unwrap().push(Event::Click("edit.undo".into()));
    h.step(vec![]);
    assert_eq!(catalog(&h.host).catalog.photo(photo).unwrap().rating, 0, "Library Undo must use Library history");
    assert_eq!(h.host.editor.session.active().unwrap().doc.layers.len(), editor_layers);

    events.lock().unwrap().push(Event::Click("app.settings".into()));
    h.step(vec![]);
    assert!(!h.host.editor.ui.dialogs.is_empty(), "native Preferences must use shared host dialog");
    assert!(!bar.lock().unwrap().find(&rate).unwrap().enabled);
    events.lock().unwrap().push(Event::Click(rate.clone()));
    h.step(vec![]);
    assert_eq!(catalog(&h.host).catalog.photo(photo).unwrap().rating, 0, "queued native clicks cannot bypass a modal");
    h.host.editor.ui.dialogs.clear();
    h.step(vec![]);
    assert!(bar.lock().unwrap().find(&rate).unwrap().enabled);

    h.host.mode = Module::Compositing;
    h.step(vec![]);
    assert!(bar.lock().unwrap().find("file.open").is_some());
    assert!(bar.lock().unwrap().find(&rate).is_none());
    h.host.mode = Module::Develop;
    h.step(vec![]);
    assert!(bar.lock().unwrap().find(&rate).is_some());
    assert!(bar.lock().unwrap().find("file.open").is_none());

    h.host.editor.services.native_menu = None;
    h.step(vec![]);
    assert!(!h.host.library.as_ref().unwrap().native_menu);
    assert!(h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "menu:File"), "in-window Library menus return without a native backend");
    let filmstrip = h.host.library.as_ref().unwrap().ui.filmstrip;
    h.click("menu:View");
    h.click("menuItem:view.filmstrip");
    assert_ne!(h.host.library.as_ref().unwrap().ui.filmstrip, filmstrip, "fallback menus remain clickable");
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
        h.host.editor.session.prefs.edit(|p| p.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Gpu);
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
    let img =
        photocraft_codecs::decode(&std::fs::read(&path).unwrap()).unwrap().convert(photocraft_codecs::ChannelLayout::Rgb, photocraft_codecs::SampleType::U8);
    assert!(img.data().as_chunks::<3>().0.iter().all(|p| p[0] > 240 && p[1] < 12 && p[2] < 12));
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn overwrite_original_gpu_real_canvas_and_library_reload() {
    use eframe::{egui_wgpu, wgpu};
    let setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    let egui_wgpu::WgpuSetup::CreateNew(new) = &setup else { panic!("GPU setup") };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: new.instance_descriptor.backends, ..wgpu::InstanceDescriptor::new_without_display_handle() });
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
    host.editor.session.prefs.edit(|p| p.performance.rendering_mode = mode);
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
    assert!(
        h.host.editor.ui.notices.iter().any(|notice| notice.title == "Saved photo-Edit.psd"
            && notice.lines.iter().any(|line| line.contains(saved.to_str().unwrap()) && line.contains("stacked")))
    );
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
    // Skip before loading those drivers; on a host with a GPU enumerate normally.
    #[cfg(target_os = "linux")]
    if !std::path::Path::new("/dev/dri").is_dir() && !std::path::Path::new("/dev/nvidiactl").exists() {
        eprintln!("skipped: no GPU adapter for Compositing → Library round trip");
        return;
    }
    let setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    let egui_wgpu::WgpuSetup::CreateNew(new) = &setup else { panic!("expected GPU setup") };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: new.instance_descriptor.backends, ..wgpu::InstanceDescriptor::new_without_display_handle() });
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
        assert!(!h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "button:roundTripSave"), "completed result has no duplicate action card");
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
    editor.session.prefs.edit(|p| p.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Cpu);
    let mut host = Host::new(editor);
    host.editor.host_modes = true;
    host.library = Some(library);
    host.mode = Module::Compositing;
    host.open_editor_photo(photo.0, &path, &settings, "photo.png").unwrap();
    assert!(!host.editor.has_unsaved(), "theme fixture has no user edits to bypass");
    (dir, host)
}
#[test]
fn j17_host_theme_union_survives_real_module_switches() {
    use photocraft_ui_egui::theme::{ThemeKind, Tokens as EditorTokens};
    let (dir, host) = workflow_host("j17-theme");
    let mut h = RoundTripUi::new(host);
    let size = egui::vec2(1200., 900.);
    for kind in [ThemeKind::ProMedium, ThemeKind::StudioLight] {
        workflow_theme(&mut h.host, kind);
        for mode in [Module::Library, Module::Develop, Module::Compositing] {
            let started = std::time::Instant::now();
            h.host.switch(&h.view.ctx, mode);
            for _ in 0..3 {
                h.step_at(size, vec![]);
            }
            let micros = started.elapsed().as_micros();
            if mode == Module::Compositing {
                assert_eq!(EditorTokens::get(&h.view.ctx).kind, kind);
            } else {
                assert_eq!(lightcraft_ui_egui::theme::Tokens::get(&h.view.ctx), library_palette(kind));
            }
            println!("J17 real host CPU UI sample: {kind:?}/{mode:?}, switch + 3 frames {micros} us");
        }
    }
    let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
    drop(h);
    previews.retire();
    std::fs::remove_dir_all(dir).unwrap();
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
                    assert_eq!(image[((canvas.left() + 3.) as usize, (canvas.bottom() - 4.) as usize)], editor.canvas, "{kind:?}: Library body follows host");
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
        editor.session.prefs.edit(|p| p.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Cpu);
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
    let delta = [foreground.r() as f64 - background.r() as f64, foreground.g() as f64 - background.g() as f64, foreground.b() as f64 - background.b() as f64];
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
        editor.session.prefs.edit(|p| p.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Cpu);
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
            view.ctx.global_style_mut(|style| style.animation_time = 0.0);
            let canvas = egui::Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(630., 300.));
            app.canvas_rect = Some(canvas);
            app.ui.toast = Some(("Versions Off".into(), 2.));
            let mut rectangles = Vec::new();
            // Shared egui Areas need their sizing/font frames before visual assertions.
            for _ in 0..4 {
                let raw = lightcraft_ui_egui::headless::HeadlessView::raw_input(egui::vec2(640., 320.), 1., 0.1, vec![]);
                view.run(raw, |ui| {
                    ui.painter().rect_filled(canvas, 0., background);
                    lightcraft_ui_egui::panels::toast(&mut app, ui.ctx());
                    rectangles = lightcraft_ui_egui::widgets::take_registry(ui.ctx());
                });
            }
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
    let preferences = h.host.editor.ui.dialogs.last().unwrap().id;
    h.key(egui::Key::K, egui::Modifiers::COMMAND);
    assert_eq!(h.host.editor.ui.dialogs.len(), 1, "the shared Preferences modal owns keyboard input");
    assert_eq!(h.host.editor.ui.dialogs[0].id, preferences);
    assert_eq!(h.host.editor.ui.dialogs[0].fields["section"], "library.general");
    assert!(h.host.library.as_ref().unwrap().host_modal);
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
#[ignore = "LRC before/after screenshots; run explicitly without opening windows"]
fn lrc_ux_screenshots() {
    let phase = std::env::var("LRC_UX_PHASE").unwrap_or_else(|_| "after".into());
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/lrc-ux");
    std::fs::create_dir_all(&output).unwrap();
    for (theme, name) in [(photocraft_ui_egui::theme::ThemeKind::Pro, "dark"), (photocraft_ui_egui::theme::ThemeKind::StudioLight, "light")] {
        for width in [250.0, 400.0] {
            let (dir, host) = workflow_host(&format!("lrc-shots-{name}-{width}"));
            let mut h = RoundTripUi::new(host);
            let ctx = h.view.ctx.clone();
            h.host.editor.session.edit_prefs(|prefs| {
                prefs.interface.theme = match theme {
                    photocraft_ui_egui::theme::ThemeKind::StudioLight => photocraft_engine::prefs::Theme::StudioLight,
                    _ => photocraft_engine::prefs::Theme::Pro,
                }
            });
            h.host.editor.set_theme(&ctx, theme);
            h.host.library.as_mut().unwrap().ui.right_width = width;
            h.host.library.as_mut().unwrap().ui.histogram = false;
            h.host.library.as_mut().unwrap().run("album.create", json!({"name":"Portraits","addSelected":true})).unwrap();
            h.host.library.as_mut().unwrap().run("album.createSmart", json!({"name":"Rated Photos","rules":{}})).unwrap();
            for mode in [Module::Library, Module::Develop, Module::Compositing] {
                h.host.switch(&ctx, mode);
                h.host.library.as_mut().unwrap().ui.left_panel = mode == Module::Library;
                for _ in 0..6 {
                    h.step(vec![]);
                }
                let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
                let rgba = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
                let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
                std::fs::write(output.join(format!("{phase}-{}-{name}-{}.png", mode.digit(), width as u32)), bytes).unwrap();
            }
            h.host.switch(&ctx, Module::Library);
            h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
            for _ in 0..6 {
                h.step(vec![]);
            }
            let image = h.view.paint(&h.host.library.as_ref().unwrap().renderer.cpu_textures());
            let rgba = lightcraft_raster::Rgba8::from_fn(image.size[0], image.size[1], |x, y| image.pixels[y * image.size[0] + x].to_array());
            let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &Default::default()).unwrap();
            std::fs::write(output.join(format!("{phase}-preferences-{name}-{}.png", width as u32)), bytes).unwrap();
            drop(h);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

fn lrc_click_rect(h: &mut RoundTripUi, rect: egui::Rect) {
    let pos = rect.center();
    h.step(vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        h.step(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
    }
    h.step(vec![]);
}
fn lrc_click_dialog_button(h: &mut RoundTripUi, role: photocraft_ui_egui::widgets::ButtonRole) {
    let rect = h.view.ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("dialog-button-role", role as u8)))).expect("dialog button");
    lrc_click_rect(h, rect);
}
#[test]
fn shared_interface_apply_updates_library_and_develop_before_switching_modules() {
    use photocraft_ui_egui::widgets::ButtonRole;
    for mode in [Module::Library, Module::Develop] {
        let (dir, host) = workflow_host("shared-interface-apply");
        let mut h = RoundTripUi::new(host);
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, mode);
        for _ in 0..4 {
            h.step(vec![]);
        }
        let id = photocraft_ui_egui::prefs_ui::open_preferences(&mut h.host.editor, "interface");
        for _ in 0..4 {
            h.step(vec![]);
        }
        let values = h.host.editor.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
        values["interface"]["theme"] = json!("studioLight");
        values["interface"]["uiScale"] = json!("150");
        values["interface"]["uiFontSize"] = json!("large");
        values["interface"]["toolIcons"] = json!("colour");
        values["interface"]["showTooltips"] = json!(false);
        values["interface"]["language"] = json!("ja");
        for _ in 0..2 {
            h.step(vec![]);
        }
        lrc_click_dialog_button(&mut h, ButtonRole::Apply);
        for _ in 0..4 {
            h.step(vec![]);
        }
        assert_eq!(h.host.mode, mode, "Apply must not require a workspace switch");
        assert_eq!(h.host.editor.ui.theme, photocraft_ui_egui::theme::ThemeKind::StudioLight);
        assert_eq!(lightcraft_ui_egui::theme::Tokens::get(&ctx), library_palette(photocraft_ui_egui::theme::ThemeKind::StudioLight));
        assert_eq!(ctx.zoom_factor(), 1.5);
        assert_eq!(ctx.fonts(|fonts| fonts.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
        assert_eq!(ctx.data(|data| data.get_temp::<bool>(egui::Id::new(("li-ui-kit", "colour-tool-icons")))), Some(true));
        assert_eq!(ctx.data(|data| data.get_temp::<li_i18n::Lang>(egui::Id::new("photocraft-ui-language"))), Some(li_i18n::Lang::Ja));
        assert!(ctx.global_style().interaction.tooltip_delay.is_infinite());
        // An unapplied new draft is still reversible in the same module.
        let values = h.host.editor.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
        values["interface"]["theme"] = json!("pro");
        values["interface"]["uiScale"] = json!("100");
        lrc_click_dialog_button(&mut h, ButtonRole::Cancel);
        for _ in 0..3 {
            h.step(vec![]);
        }
        assert_eq!(h.host.editor.ui.theme, photocraft_ui_egui::theme::ThemeKind::StudioLight);
        assert_eq!(ctx.zoom_factor(), 1.5);
        drop(h);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn lrc_one_preferences_shell_drafts_apply_cancel_and_one_store_in_every_module() {
    use photocraft_ui_egui::widgets::ButtonRole;
    let (dir, host) = workflow_host("lrc-preferences");
    let mut h = RoundTripUi::new(host);
    for mode in [Module::Library, Module::Develop, Module::Compositing] {
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, mode);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.host.library.as_mut().unwrap().run("app.settings", json!({"tab":"interface"})).unwrap();
        for _ in 0..4 {
            h.step(vec![]);
        }
        assert!(h.host.library.as_ref().unwrap().ui.dialog.is_none());
        assert_eq!(h.host.editor.ui.dialogs.len(), 1);
        assert_eq!(h.host.editor.ui.dialogs[0].fields["section"], "library.interface");
        for tab in ["general", "import", "performance", "interface"] {
            h.click(&format!("button:settingsTab-{tab}"));
            assert_eq!(h.host.editor.ui.dialogs[0].fields["section"], format!("library.{tab}"));
        }
        let old = h.host.library.as_ref().unwrap().ui.settings.film_names;
        h.click("check:settings.filmNames");
        assert_eq!(h.host.library.as_ref().unwrap().ui.settings.film_names, old, "editing a draft is reversible");
        lrc_click_dialog_button(&mut h, ButtonRole::Apply);
        assert_eq!(h.host.library.as_ref().unwrap().ui.settings.film_names, !old);
        assert_eq!(h.host.editor.ui.dialogs.len(), 1);
        assert_eq!(h.host.editor.session.prefs().library["settings"]["filmNames"], !old);
        lrc_click_dialog_button(&mut h, ButtonRole::Cancel);
        assert!(h.host.editor.ui.dialogs.is_empty());
        // Reopening retains the applied value, then Cancel discards another real edit.
        h.host.library.as_mut().unwrap().run("app.settings", json!({"tab":"interface"})).unwrap();
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.click("check:settings.filmNames");
        lrc_click_dialog_button(&mut h, ButtonRole::Cancel);
        assert_eq!(h.host.library.as_ref().unwrap().ui.settings.film_names, !old);
    }
    // The unified file reloads all valid old UI fields through the same lenient loader.
    let stored = library_ui_value(h.host.editor.session.prefs()).unwrap();
    let (loaded, rejected) = photocraft_engine::prefs::lenient::<UiState>(stored).unwrap();
    assert!(rejected.is_empty());
    assert_eq!(loaded.settings, h.host.library.as_ref().unwrap().ui.settings);
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn lrc_module_picker_right_edge_matches_library_develop_pixel_and_vector_personas() {
    let (dir, host) = workflow_host("lrc-picker");
    let mut h = RoundTripUi::new(host);
    for size in [egui::vec2(1000.0, 900.0), egui::vec2(1600.0, 1000.0)] {
        let mut expected: Option<f32> = None;
        for mode in [Module::Library, Module::Develop, Module::Compositing] {
            let ctx = h.view.ctx.clone();
            h.host.switch(&ctx, mode);
            for _ in 0..4 {
                h.step_at(size, vec![]);
            }
            let rect = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", 3u8)))).unwrap();
            if let Some(right) = expected {
                assert!((rect.right() - right).abs() < 0.1, "{mode:?}: module picker shifted");
            } else {
                expected = Some(rect.right());
            }
            let target = if mode == Module::Library { 2u8 } else { 1u8 };
            let button = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", target)))).unwrap();
            let pos = button.center();
            for pressed in [true, false] {
                h.step_at(
                    size,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                    ],
                );
            }
            h.step_at(size, vec![]);
            assert_eq!(h.host.mode, if target == 1 { Module::Library } else { Module::Develop });
        }
    }
    let ctx = h.view.ctx.clone();
    h.host.switch(&ctx, Module::Compositing);
    for _ in 0..4 {
        h.step(vec![]);
    }
    let pixel = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", 3u8)))).unwrap();
    photocraft_ui_egui::toolsets_ui::persona::select(&mut h.host.editor, &ctx, photocraft_engine::toolsets::Persona::Vector);
    for _ in 0..4 {
        h.step(vec![]);
    }
    let vector = ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", 3u8)))).unwrap();
    assert_eq!(pixel, vector);
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lrc_module_clicks_remain_interactive_with_the_real_gpu_canvas() {
    use eframe::{egui_wgpu, wgpu};
    let setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    let egui_wgpu::WgpuSetup::CreateNew(new) = &setup else { panic!("GPU setup") };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor { backends: new.instance_descriptor.backends, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    let rs = block_on(egui_wgpu::RenderState::create(&config, &instance, None, Default::default())).expect("real GPU required");
    let info = rs.adapter.get_info();
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "software adapters do not count");
    let (dir, mut host) = workflow_host("lrc-gpu-clicks");
    host.editor.session.edit_prefs(|prefs| prefs.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Gpu);
    host.editor.set_wgpu(rs.clone());
    let mut h = RoundTripUi::new(host);
    h.frame.wgpu_render_state = Some(rs.clone());
    let probe = photocraft_ui_egui::gpu_canvas::GpuCanvas::new(&rs);
    h.wait_for(|host| probe.read_texels(host.editor.session.active().unwrap().doc.id.0).is_some());
    assert!(h.host.editor.gpu_active());
    for target in [Module::Library, Module::Develop, Module::Compositing] {
        let rect = h.view.ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("host-mode-rect", target.digit())))).unwrap();
        let started = std::time::Instant::now();
        lrc_click_rect(&mut h, rect);
        assert_eq!(h.host.mode, target);
        eprintln!("LRC module click {:?}: {:?}; {} ({:?})", target, started.elapsed(), info.name, info.backend);
    }
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lrc_shared_interface_language_and_library_language_commands_use_one_preference() {
    let (dir, host) = workflow_host("lrc-language");
    let mut h = RoundTripUi::new(host);
    let ctx = h.view.ctx.clone();
    h.host.switch(&ctx, Module::Library);
    for _ in 0..4 {
        h.step(vec![]);
    }
    h.host.editor.session.edit_prefs(|prefs| prefs.interface.language = "ja".into());
    h.step(vec![]);
    assert_eq!(h.host.library.as_ref().unwrap().ui.language, li_i18n::Lang::Ja);
    h.host.library.as_mut().unwrap().run("app.language.french", json!({})).unwrap();
    h.step(vec![]);
    assert_eq!(h.host.editor.session.prefs().interface.language, "fr");
    assert_eq!(h.host.library.as_ref().unwrap().ui.language, li_i18n::Lang::Fr);
    drop(h);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lrc_library_preferences_are_lenient_preserve_paths_and_unreadable_settings_block() {
    let (dir, mut host) = workflow_host("lrc-library-prefs");
    let location = dir.join("library").to_string_lossy().to_string();
    let stored = json!({"language":"retired-language","settings":{"libraryPath":location,"previewEdge":3840},"autoAdvance":true,"view":"detail","openSections":["light","color"]});
    let (loaded, rejected) = decode_library_ui(stored.clone()).unwrap();
    assert_eq!(rejected, ["language"]);
    assert_eq!(loaded.settings.library_path, location);
    assert_eq!(loaded.settings.preview_edge, 3840);
    assert!(loaded.auto_advance);
    host.editor.session.edit_prefs(|prefs| prefs.library = serde_json::from_value(stored.clone()).unwrap());
    host.library = None;
    assert_eq!(host.library().ui.settings.library_path, location);
    assert!(!host.editor.session.prefs().library.contains_key("recoveredOriginal"));
    assert!(decode_library_ui(json!({"settings":{"libraryPath":123}})).is_err(), "damaged paths do not create a different catalog");
    // Unreadable Library settings show the Library-problem screen; no Library is opened or created.
    host.editor.session.edit_prefs(|prefs| {
        prefs.library.clear();
        prefs.library.insert(crate::services::LIBRARY_UNREADABLE_KEY.into(), json!("The stored Library settings could not be read."));
    });
    host.prefs = PrefsWriter::default();
    host.library = None;
    let lib = host.library();
    assert!(lib.library_problem.is_some());
    assert!(lib.session.library.is_none());
    drop(host);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_ai_preferences_open_from_library_and_develop_settings() {
    let (dir, host) = workflow_host("shared-ai-preferences");
    let mut h = RoundTripUi::new(host);
    for mode in [Module::Library, Module::Develop] {
        let ctx = h.view.ctx.clone();
        h.host.switch(&ctx, mode);
        for _ in 0..4 {
            h.step(vec![]);
        }
        h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
        for _ in 0..3 {
            h.step(vec![]);
        }
        h.click("button:settingsAI");
        assert!(h.host.library.as_ref().unwrap().ui.dialog.is_none());
        let dialog = h.host.editor.ui.dialogs.last().unwrap();
        assert_eq!(dialog.fields["section"], "ai.connection");
        let id = dialog.id;
        photocraft_ui_egui::dialogs::cancel(&mut h.host.editor, id).unwrap();
    }
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
    assert_eq!(h.host.library.as_ref().unwrap().session.catalog.photo(photo.unwrap()).unwrap().rating, rating, "capturing a rating key must not edit photos");
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
    assert_eq!(h.host.editor.session.prefs().shortcut_overrides(true)[&key], "F6");
    h.host.library.as_mut().unwrap().run("app.settings", json!({})).unwrap();
    h.step(vec![]);
    assert!(h.host.library.as_ref().unwrap().ui.dialog.is_none());
    assert_eq!(h.host.editor.ui.dialogs.len(), 1);
    assert_eq!(h.host.editor.ui.dialogs[0].fields["section"], "library.general", "the shared Preferences shell keeps Library-specific pages");
    assert!(h.host.editor.host_preferences_pages.iter().any(|(id, _)| *id == "library.performance"));
    let previews = h.host.library.as_ref().unwrap().session.media.rendered.clone();
    drop(h);
    previews.retire();
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn shared_shell_host_exact_save_keeps_sync_settings_and_module_shortcuts() {
    let (dir, path, library, photo) = library_with_photo("shared-shell-host-keys");
    let mut host = host_with_round_trip(library, photo, &path);
    host.editor.session.prefs.edit(|p| p.performance.rendering_mode = photocraft_engine::prefs::RenderingMode::Cpu);
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
    assert!(!h.host.library.as_ref().unwrap().widgets.iter().any(|(id, _)| id == "dialog:window"), "underlying window is suspended without discarding state");
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
    let cancel = h.host.library.as_ref().unwrap().export.as_ref().unwrap().job.clone();
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
    assert!(!cancel.is_cancelled());
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
