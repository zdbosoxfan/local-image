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
    let hash_before = s.catalog.photo(id).unwrap().content_hash.clone();

    let plan = s.execute("photo.saveOverOriginalPlan", &json!({})).unwrap();
    assert_eq!((plan["mode"].as_str(), plan["format"].as_str()), (Some("overwrite"), Some("JPEG")), "{plan}");
    assert!(plan["backupDir"].as_str().unwrap().ends_with("originals-backup"), "{plan}");
    assert_eq!(plan["backupTemporary"], false);

    // never without an explicit confirmation
    let err = s.execute("photo.saveOverOriginal", &json!({})).unwrap_err().to_string();
    assert!(err.contains("confirm"), "{err}");
    assert_eq!(std::fs::read(&src).unwrap(), original, "untouched without confirm");

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
    assert_eq!(p.history.len(), 1, "history starts over");
    assert!(!undo_touches(&s, id), "undoing an old edit would apply it twice");
    // the guard still protects the (new) original from ordinary exports
    assert!(s.execute("export.checkTarget", &json!({"path": src.to_string_lossy()})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
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
    eprintln!("after first: {:?} new source {:?}", std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>(), s.catalog.photo(new).map(|p| p.source.clone()));
    let r = s.execute("photo.saveOverOriginal", &json!({"confirm": true, "import": false})).unwrap();
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
