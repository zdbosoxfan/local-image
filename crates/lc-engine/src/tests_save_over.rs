//! Save Over Original (`photo.saveOverOriginal`) and the export additions that go with the new
//! Export dialog (Same folder as original photo, Add to This Catalog / Add to Stack).

use std::path::{Path, PathBuf};

use lightcraft_catalog::PhotoId;
use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, Samples};
use serde_json::json;

use crate::Session;
use crate::export::{Conflict, Destination, ExportFormat, ExportOptions};
use crate::tests_xmp::{synthetic_dng_with, temp_dir};

/// A library on disk (`<dir>/lib`) with `file` (written into `dir`) imported and selected.
fn library_with(tag: &str, file: &str, bytes: &[u8]) -> (Session, PhotoId, PathBuf, PathBuf) {
    let dir = temp_dir(tag);
    let src = dir.join(file);
    std::fs::write(&src, bytes).unwrap();
    let mut s = Session::new().with_fs();
    s.open_library(dir.join("lib"), false).unwrap();
    s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    (s, id, dir, src)
}

fn gradient(w: usize, h: usize) -> lightcraft_raster::Rgba8 {
    lightcraft_raster::Rgba8::from_fn(w, h, |x, y| [(40 + x * 3) as u8, (30 + y * 4) as u8, 90, 255])
}

fn mean(bytes: &[u8]) -> f32 {
    let d = lightcraft_codecs::decode(bytes, Default::default()).unwrap().to_srgb8();
    d.data.iter().map(|p| (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0).sum::<f32>() / d.data.len() as f32
}

/// The steps that would bring the old settings back.
fn undo_touches(s: &Session, id: PhotoId) -> bool {
    s.undo.iter().chain(&s.redo).any(|e| {
        let mut hit = false;
        e.op.photo_ids(&mut |p| hit |= p == id);
        hit
    })
}

#[test]
fn saving_over_a_jpeg_bakes_the_edits_and_keeps_a_backup() {
    // a 4:2:0 JPEG (as most cameras write them)
    let img = gradient(48, 32);
    let original = lightcraft_codecs::encode_jpeg(&EncodeImage::rgba8(&img), 90, ChromaSubsampling::S420, &EncodeMeta::default()).unwrap();
    let (mut s, id, dir, src) = library_with("saveover-jpeg", "IMG_1.jpg", &original);
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
    s.execute("crop.set", &json!({"rect": [0.0, 0.0, 0.5, 1.0]})).unwrap();
    let mut settings = (*s.catalog.photo(id).unwrap().develop).clone();
    settings.spots.push(lightcraft_develop::Spot {
        mode: lightcraft_develop::SpotMode::Ai,
        patch: Some(lightcraft_develop::AiPatch {
            key: "missing-test-patch".into(),
            source: "old-pixels".into(),
            rect: [0.0, 0.0, 0.1, 0.1],
            engine: "test".into(),
            seed: 1,
            geometry: String::new(),
        }),
        ..Default::default()
    });
    settings.masks.push(lightcraft_develop::Mask::default());
    s.commit("Retouch", lightcraft_catalog::Op::SetDevelop { id, settings: settings.into(), label: "Retouch".into(), edited: None }).unwrap();
    s.execute("version.create", &json!({"name": "Old pixels"})).unwrap();
    let hash_before = s.catalog.photo(id).unwrap().content_hash.clone();

    let plan = s.execute("photo.saveOverOriginalPlan", &json!({})).unwrap();
    assert_eq!((plan["mode"].as_str(), plan["format"].as_str()), (Some("overwrite"), Some("JPEG")), "{plan}");
    assert!(plan["backupDir"].as_str().unwrap().ends_with("originals-backup"), "{plan}");
    assert_eq!(plan["backupTemporary"], false);

    // never without an explicit confirmation
    let err = s.execute("photo.saveOverOriginal", &json!({})).unwrap_err().to_string();
    assert!(err.contains("confirm"), "{err}");
    assert_eq!(std::fs::read(&src).unwrap(), original, "untouched without confirm");
    assert!(s.execute("photo.saveOverOriginal", &json!({"id": "invalid", "confirm": true})).is_err());
    assert_eq!(std::fs::read(&src).unwrap(), original, "invalid IDs must not fall back to the active photo");

    let r = s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
    assert_eq!(r["mode"], "overwrite", "{r}");
    // the backup is the original, byte for byte, inside the library
    let backup = PathBuf::from(r["backup"].as_str().unwrap());
    assert!(backup.starts_with(dir.join("lib").join("originals-backup")), "{}", backup.display());
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    // the original now holds the render: cropped, brighter, still a 4:2:0 JPEG
    let now = std::fs::read(&src).unwrap();
    assert_ne!(now, original);
    let d = lightcraft_codecs::decode(&now, Default::default()).unwrap();
    assert_eq!(d.format, lightcraft_codecs::Format::Jpeg);
    assert_eq!((d.width, d.height), (24, 32), "the crop is baked in");
    assert_eq!(crate::cmd::save_over::jpeg_chroma(&now), Some(ChromaSubsampling::S420), "the original's chroma subsampling");
    assert!(mean(&now) > mean(&original) + 10.0, "the exposure is baked in");
    // no temp file left beside it
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert!(names.iter().all(|n| n == "IMG_1.jpg" || n == "lib"), "{names:?}");

    // the library caught up: new size and hash, edits reset (they are in the pixels now)
    let p = s.catalog.photo(id).unwrap();
    assert_eq!((p.width, p.height), (24, 32));
    assert_ne!(p.content_hash, hash_before);
    assert_eq!(p.develop.light.exposure, 0.0);
    assert_eq!(p.develop.crop, Default::default(), "crop reset");
    assert!(p.develop.spots.is_empty());
    assert!(p.develop.masks.is_empty());
    assert!(p.versions.is_empty(), "versions cannot restore patches made for old pixels");
    assert_eq!(p.history.len(), 1, "history starts over");
    assert!(!undo_touches(&s, id), "undoing an old edit would apply it twice");
    // the guard still protects the (new) original from ordinary exports
    assert!(s.execute("export.checkTarget", &json!({"path": src.to_string_lossy()})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_over_preserves_source_profile_and_camera_metadata() {
    // A nonstandard curve on P3 primaries must not become standard sRGB (or standard P3).
    let profile = lightcraft_codecs::icc::write_matrix_trc(&lightcraft_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
    let metadata = lightcraft_meta::Metadata {
        make: Some("Camera Maker".into()),
        model: Some("Body".into()),
        serial_number: Some("12345".into()),
        lens_make: Some("Lens Maker".into()),
        lens_serial_number: Some("67890".into()),
        gps: Some(lightcraft_meta::Gps { latitude: 42.0, longitude: -71.0, altitude: Some(83.0) }),
        copyright: Some("© Photographer".into()),
        ..Default::default()
    };
    let exif = lightcraft_meta::write_exif(&metadata);
    let xmp = lightcraft_meta::write_xmp(&metadata, None);
    let original = lightcraft_codecs::encode_jpeg(
        &EncodeImage::rgba8(&gradient(24, 16)),
        95,
        ChromaSubsampling::S444,
        &EncodeMeta { icc: Some(&profile), exif: Some(&exif), xmp: Some(&xmp), ..Default::default() },
    )
    .unwrap();
    let (mut s, id, dir, src) = library_with("saveover-profile", "P3.jpg", &original);
    s.execute("crop.set", &json!({"rect": [0.0, 0.0, 0.5, 1.0]})).unwrap();
    s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
    let bytes = std::fs::read(&src).unwrap();
    let decoded = lightcraft_codecs::decode(&bytes, Default::default()).unwrap();
    assert_eq!(decoded.icc.as_deref(), Some(profile.as_slice()));
    let saved = lightcraft_meta::extract(&bytes);
    assert_eq!(saved.make, metadata.make);
    assert_eq!(saved.serial_number, metadata.serial_number);
    assert_eq!(saved.lens_make, metadata.lens_make);
    assert_eq!(saved.lens_serial_number, metadata.lens_serial_number);
    assert_eq!(saved.gps, metadata.gps);
    assert_eq!(saved.copyright, metadata.copyright);
    assert_eq!((saved.width, saved.height), (Some(12), Some(16)));
    assert_eq!(s.catalog.photo(id).unwrap().develop.crop, Default::default());
    drop(s);
    let mut reopened = Session::new().with_fs();
    reopened.open_library(dir.join("lib"), false).unwrap();
    assert_eq!(reopened.catalog.photo(id).unwrap().develop.crop, Default::default());
    drop(reopened);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn save_over_webp_and_float_tiff_preserve_depth_and_profile() {
    let webp = lightcraft_codecs::encode_webp_lossless(&EncodeImage::rgba8(&gradient(16, 12)), &EncodeMeta::default()).unwrap();
    let samples = vec![0.2f32; 16 * 12 * 3];
    let profile = lightcraft_codecs::icc::write_matrix_trc(&lightcraft_color::ADOBE_RGB, &lightcraft_codecs::Trc::Linear);
    let tiff = lightcraft_codecs::encode_tiff(
        &EncodeImage::new(16, 12, 3, Samples::F32(&samples)),
        lightcraft_codecs::TiffCompression::Deflate,
        &EncodeMeta { icc: Some(&profile), ..Default::default() },
    )
    .unwrap();
    for (tag, name, original, format, depth) in [
        ("saveover-webp", "image.webp", webp, lightcraft_codecs::Format::WebP, 8),
        ("saveover-float", "image.tif", tiff, lightcraft_codecs::Format::Tiff, 32),
    ] {
        let (mut s, _, dir, src) = library_with(tag, name, &original);
        s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
        let d = lightcraft_codecs::decode(&std::fs::read(&src).unwrap(), Default::default()).unwrap();
        assert_eq!((d.format, d.bit_depth), (format, depth));
        if depth == 32 {
            assert!(d.float);
            assert_eq!(d.icc.as_deref(), Some(profile.as_slice()));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn save_over_source_profile_gpu_matches_cpu() {
    let _gpu = crate::tests_gpu::gpu_state();
    if !lightcraft_gpu::available() {
        eprintln!("skipped: no GPU adapter");
        return;
    }
    struct RestoreGpu(bool);
    impl Drop for RestoreGpu {
        fn drop(&mut self) {
            lightcraft_gpu::set_enabled(self.0);
        }
    }
    let _restore = RestoreGpu(lightcraft_gpu::enabled());
    let profile = lightcraft_codecs::icc::write_matrix_trc(&lightcraft_color::DISPLAY_P3, &lightcraft_codecs::Trc::Gamma(2.0));
    let original =
        lightcraft_codecs::encode_png(&EncodeImage::rgba8(&gradient(48, 32)), &EncodeMeta { icc: Some(&profile), ..Default::default() }).unwrap();
    let mut outputs = Vec::new();
    for (tag, enabled) in [("saveover-gpu", true), ("saveover-cpu", false)] {
        lightcraft_gpu::set_enabled(enabled);
        let (mut s, _, dir, src) = library_with(tag, "P3.png", &original);
        s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.3})).unwrap();
        s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
        let decoded = lightcraft_codecs::decode(&std::fs::read(&src).unwrap(), Default::default()).unwrap();
        assert_eq!(decoded.icc.as_deref(), Some(profile.as_slice()));
        outputs.push(decoded.to_srgb8());
        let _ = std::fs::remove_dir_all(dir);
    }
    let differences = outputs[0].data.iter().zip(&outputs[1].data).flat_map(|(a, b)| (0..3).map(move |i| a[i].abs_diff(b[i]))).collect::<Vec<_>>();
    let mean = differences.iter().map(|d| *d as f64).sum::<f64>() / differences.len() as f64;
    let max = *differences.iter().max().unwrap();
    assert!(mean < 0.5 && max <= 3, "source-profile GPU/CPU output differs: mean {mean}, max {max}");
}

#[test]
fn failed_backup_or_replacement_leaves_original_and_edits_intact() {
    let original =
        lightcraft_codecs::encode_jpeg(&EncodeImage::rgba8(&gradient(32, 24)), 10, ChromaSubsampling::S420, &EncodeMeta::default()).unwrap();
    for (tag, limit, backup_expected) in [("saveover-backup-fail", 0, false), ("saveover-write-fail", original.len() as u64, true)] {
        let (mut s, id, dir, src) = library_with(tag, "image.jpg", &original);
        s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
        let hash = s.catalog.photo(id).unwrap().content_hash.clone();
        let fault = lightcraft_catalog::safe_file::fail_writes_after(limit);
        let err = s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap_err().to_string();
        drop(fault);
        assert!(err.contains("unchanged"), "{err}");
        assert_eq!(std::fs::read(&src).unwrap(), original);
        assert_eq!(s.catalog.photo(id).unwrap().content_hash, hash);
        assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 1.0);
        let backup = dir.join("lib/originals-backup");
        let files = std::fs::read_dir(&backup)
            .unwrap()
            .flat_map(|e| std::fs::read_dir(e.unwrap().path()).unwrap())
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), usize::from(backup_expected));
        if backup_expected {
            assert_eq!(std::fs::read(&files[0]).unwrap(), original);
        }
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn save_over_resets_an_existing_manual_xmp_sidecar() {
    let png = crate::export::encode_image(&gradient(16, 12), &ExportOptions { format: ExportFormat::Png, ..Default::default() }).unwrap();
    let (mut s, id, dir, _) = library_with("saveover-xmp", "image.png", &png);
    s.xmp.auto_write = false;
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
    s.save_sidecar(id).unwrap();
    s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
    let (op, _) = s.read_sidecar_op(id).unwrap().expect("sidecar still exists");
    s.commit("Read Metadata", op).unwrap();
    assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 0.0, "the sidecar cannot apply the baked edits twice");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saving_over_png_and_tiff_keeps_format_and_bit_depth() {
    // a 16-bit PNG
    let (w, h) = (20usize, 12usize);
    let v: Vec<u16> = (0..w * h * 3).map(|i| (i * 97 % 60_000) as u16 + 2000).collect();
    let png = lightcraft_codecs::encode_png(&EncodeImage::new(w as u32, h as u32, 3, Samples::U16(&v)), &EncodeMeta::default()).unwrap();
    let (mut s, id, dir, src) = library_with("saveover-png", "scan.png", &png);
    s.execute("develop.set", &json!({"control": "light.exposure", "value": -0.5})).unwrap();
    let r = s.execute("photo.saveOverOriginal", &json!({"id": id.0, "confirm": true})).unwrap();
    assert_eq!(r["format"], "PNG", "{r}");
    let d = lightcraft_codecs::decode(&std::fs::read(&src).unwrap(), Default::default()).unwrap();
    assert_eq!((d.format, d.bit_depth), (lightcraft_codecs::Format::Png, 16), "16-bit kept");
    assert!(std::fs::read(r["backup"].as_str().unwrap()).unwrap() == png);
    assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 0.0);
    let _ = std::fs::remove_dir_all(&dir);

    // an 8-bit TIFF
    let tif = crate::export::encode_image(&gradient(16, 16), &ExportOptions { format: ExportFormat::Tiff, ..Default::default() }).unwrap();
    let (mut s, id, dir, src) = library_with("saveover-tiff", "page.tif", &tif);
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.5})).unwrap();
    let r = s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
    assert_eq!(r["format"], "TIFF", "{r}");
    let d = lightcraft_codecs::decode(&std::fs::read(&src).unwrap(), Default::default()).unwrap();
    assert_eq!((d.format, d.bit_depth), (lightcraft_codecs::Format::Tiff, 8), "8-bit kept");
    assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raws_are_never_replaced_a_jpeg_goes_beside_them() {
    let dng = synthetic_dng_with(None, Default::default());
    let (mut s, id, dir, src) = library_with("saveover-raw", "Shot.dng", &dng);
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.7})).unwrap();
    let plan = s.execute("photo.saveOverOriginalPlan", &json!({})).unwrap();
    assert_eq!((plan["mode"].as_str(), plan["besideName"].as_str()), (Some("beside"), Some("Shot.jpg")), "{plan}");
    assert!(s.execute("photo.saveOverOriginal", &json!({})).is_err(), "confirm is needed here too");
    let r = s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap();
    assert_eq!(r["mode"], "beside", "{r}");
    let out = PathBuf::from(r["path"].as_str().unwrap());
    assert_eq!(out, dir.join("Shot.jpg"));
    assert_eq!(lightcraft_codecs::sniff(&std::fs::read(&out).unwrap()), Some(lightcraft_codecs::Format::Jpeg));
    assert_eq!(std::fs::read(&src).unwrap(), dng, "the raw is untouched");
    assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 0.7, "its edits too");
    // added to the library, stacked on top of the raw
    let new = PhotoId(r["id"].as_u64().expect("imported"));
    let stack = s.catalog.stack_of(new).expect("stacked");
    assert_eq!(stack.photos.first(), Some(&new));
    assert!(stack.photos.contains(&id));
    // again: the next free name, never over the first JPEG
    let r = s.execute("photo.saveOverOriginal", &json!({"id": id.0, "confirm": true, "import": false})).unwrap();
    assert_eq!(Path::new(r["path"].as_str().unwrap()), dir.join("Shot-2.jpg"));
    assert!(r["id"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_copy_beside_keeps_the_original() {
    let jpeg = crate::export::encode_image(&gradient(20, 20), &ExportOptions::default()).unwrap();
    let (mut s, id, dir, src) = library_with("saveover-beside", "IMG_2.jpg", &jpeg);
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
    let r = s.execute("photo.saveOverOriginal", &json!({"confirm": true, "beside": true})).unwrap();
    assert_eq!(Path::new(r["path"].as_str().unwrap()), dir.join("IMG_2-2.jpg"), "{r}");
    assert_eq!(std::fs::read(&src).unwrap(), jpeg);
    assert_eq!(s.catalog.photo(id).unwrap().develop.light.exposure, 1.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn demo_photos_and_virtual_copies_have_no_original_to_replace() {
    let mut s = Session::with_demo();
    assert!(s.execute("photo.saveOverOriginal", &json!({"confirm": true})).unwrap_err().to_string().contains("demo"));
    let jpeg = crate::export::encode_image(&gradient(8, 8), &ExportOptions::default()).unwrap();
    let (mut s, _, dir, _) = library_with("saveover-copy", "IMG_3.jpg", &jpeg);
    s.execute("photo.virtualCopy", &json!({})).unwrap();
    let copy = s.catalog.photos().find(|p| p.copy_of.is_some()).unwrap().id;
    let err = s.execute("photo.saveOverOriginal", &json!({"id": copy.0, "confirm": true})).unwrap_err().to_string();
    assert!(err.contains("virtual copy"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Export ▸ Export To: Same folder as original photo (+ Put in Subfolder); the original stays
/// protected, and Add to This Catalog / Add to Stack import the files stacked on their sources.
#[test]
fn export_to_the_same_folder_as_the_original() {
    let jpeg = crate::export::encode_image(&gradient(24, 16), &ExportOptions::default()).unwrap();
    let (mut s, id, dir, src) = library_with("export-samefolder", "IMG_4.jpg", &jpeg);
    let o = ExportOptions { same_folder: true, subfolder: "Exports".into(), ..Default::default() };
    let none = Destination::default();
    let planned = crate::export::planned_paths(&s.catalog, &[id], &o, &none);
    let want = format!("{}/Exports/IMG_4.jpg", dir.to_string_lossy());
    assert_eq!(planned, vec![Some(want.clone())]);
    let files = crate::export::export_batch(&mut s, &[id], &o, &none, &mut crate::export::write_file, &|p| Path::new(p).exists()).unwrap();
    assert_eq!(files[0]["path"], want.as_str());
    assert_eq!(files[0]["photo"], id.0);
    // straight into the folder, {name}, Overwrite: refused (that is the original)
    let over = ExportOptions { same_folder: true, conflict: Conflict::Overwrite, ..Default::default() };
    assert!(crate::export::export_batch(&mut s, &[id], &over, &none, &mut crate::export::write_file, &|p| Path::new(p).exists()).is_err());
    assert_eq!(std::fs::read(&src).unwrap(), jpeg);
    // add the export to the library, stacked on its source
    let r = s.execute("export.addToLibrary", &json!({"files": files, "stack": true})).unwrap();
    let new = PhotoId(r["imported"][0].as_u64().expect("imported"));
    let st = s.catalog.stack_of(new).expect("stacked");
    assert_eq!(st.photos.first(), Some(&new));
    assert!(st.photos.contains(&id));
    // a demo photo has no folder of its own
    let mut demo = Session::with_demo();
    let did = demo.active().unwrap();
    let err = crate::export::export_batch(&mut demo, &[did], &o, &none, &mut |_, _| Ok(()), &|_| false).unwrap_err();
    assert!(err.contains("no folder"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
