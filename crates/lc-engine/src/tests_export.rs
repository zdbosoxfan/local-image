//! Export end to end: output sizes, original + XMP, DNG.

use serde_json::json;

use crate::Session;
use crate::export::{ExportFormat, ExportOptions, export_photo};
use crate::tests_xmp::{synthetic_dng_with, temp_dir};

struct EnhanceHost(std::sync::Arc<std::sync::Mutex<Vec<crate::enhance::UpscaleRequest>>>);
impl crate::enhance::AiHost for EnhanceHost {
    fn remove_engines(&self) -> Vec<crate::enhance::RemoveEngine> {
        vec![]
    }
    fn remove(&self, _: &crate::enhance::RemoveRequest, _: &lightcraft_sysmem::JobCtl) -> Result<crate::enhance::RemoveResult, String> {
        Err("unused".into())
    }
    fn upscale(&self, req: &crate::enhance::UpscaleRequest, ctl: &lightcraft_sysmem::JobCtl) -> Result<Vec<[u8; 4]>, String> {
        ctl.check()?;
        self.0.lock().unwrap().push(req.clone());
        Ok(vec![[50, 120, 200, 255]; req.target.0 * req.target.1])
    }
}

#[test]
fn enhance_export_restores_native_pixels_before_encoding_and_defaults_load() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    // Small native fixture exercises the same export path without three full-photo encodes.
    let mut photo = (**s.catalog.photo(id).unwrap()).clone();
    let reduction = f64::from(photo.width.max(photo.height)) / 96.0;
    photo.width = (f64::from(photo.width) / reduction).round() as u32;
    photo.height = (f64::from(photo.height) / reduction).round() as u32;
    s.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id }).unwrap();
    s.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    s.enhance.host = Some(std::sync::Arc::new(EnhanceHost(calls.clone())));
    let native = crate::export::output_size(s.catalog.photo(id).unwrap(), &ExportOptions::default());
    for method in ["faithful", "rescue", "generative"] {
        let opts = ExportOptions::from_json(
            &json!({"format": "png", "percent": 200, "dontEnlarge": false, "enhance": method, "bitDepth": 16, "colorSpace": "displayP3"}),
        );
        let out = export_photo(&mut s, id, &opts, 1).unwrap();
        assert_eq!((out.width, out.height), (native.0 * 2, native.1 * 2));
        let last = calls.lock().unwrap().last().unwrap().clone();
        assert_eq!((last.width, last.height), native);
        assert_eq!(last.rgba.len(), native.0 * native.1);
        assert_eq!(last.method, method);
        assert_eq!(ExportOptions::from_json(&opts.to_json()).enhance, opts.enhance);
    }
    assert_eq!(serde_json::from_value::<ExportOptions>(json!({})).unwrap().enhance, None);
    let old_count = calls.lock().unwrap().len();
    let smaller = ExportOptions::from_json(&json!({"percent": 50, "enhance": "rescue"}));
    export_photo(&mut s, id, &smaller, 1).unwrap();
    assert_eq!(calls.lock().unwrap().len(), old_count);
    let ctl = lightcraft_sysmem::JobCtl::new();
    ctl.cancel();
    let opts = ExportOptions::from_json(&json!({"percent": 200, "dontEnlarge": false, "enhance": "faithful"}));
    let prepared = crate::export::prepare_export(&mut s, id, &opts, 1).unwrap();
    assert_eq!(prepared.run_with_ctl(&ctl).err().unwrap(), crate::enhance::CANCELLED);
}

#[test]
fn full_size_export_of_a_cropped_photo_is_not_upscaled() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let (pw, ph) = {
        let p = s.catalog.photo(id).unwrap();
        (p.width as usize, p.height as usize)
    };
    s.execute("crop.set", &json!({"rect": [0.25, 0.25, 0.75, 0.75]})).unwrap();
    let e = export_photo(&mut s, id, &ExportOptions::default(), 1).unwrap();
    assert!(e.width.abs_diff(pw / 2) <= 1 && e.height.abs_diff(ph / 2) <= 1, "{}×{} from {pw}×{ph}", e.width, e.height);
    // and the size modes apply to the cropped size
    let o = ExportOptions::from_json(&json!({"percent": 50}));
    let e = export_photo(&mut s, id, &o, 1).unwrap();
    assert!(e.width.abs_diff(pw / 4) <= 1, "{} vs {}", e.width, pw / 4);
    let o = ExportOptions::from_json(&json!({"width": 200, "height": 200, "format": "png"}));
    let e = export_photo(&mut s, id, &o, 1).unwrap();
    assert_eq!(e.width.max(e.height), 200);
    // the print resolution lands in the file
    let i = e.bytes.windows(4).position(|w| w == b"pHYs").expect("pHYs");
    assert_eq!(u32::from_be_bytes(e.bytes[i + 4..i + 8].try_into().unwrap()), 9449, "240 ppi");
}

#[test]
fn original_export_copies_the_file_and_writes_its_edits_beside_it() {
    let dir = temp_dir("export-orig");
    let src = dir.join("Shot.dng");
    let dng = synthetic_dng_with(None, Default::default());
    std::fs::write(&src, &dng).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.7})).unwrap();

    let o = ExportOptions::from_json(&json!({"format": "original", "naming": "{name}-{seq}"}));
    let e = export_photo(&mut s, id, &o, 3).unwrap();
    assert_eq!(e.file_name, "Shot-003.dng");
    assert_eq!(e.bytes, dng, "the original, byte for byte");
    assert_eq!(e.sidecars.len(), 1);
    let (ext, xmp) = &e.sidecars[0];
    assert_eq!(*ext, "xmp");
    let name = "Shot-003.xmp";
    let copy = dir.join("copy");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join(&e.file_name), &e.bytes).unwrap();
    std::fs::write(copy.join(name), xmp).unwrap();
    let mut s2 = Session::new().with_fs();
    s2.execute("library.import", &json!({"paths": [copy.join(&e.file_name).to_string_lossy()]})).unwrap();
    assert_eq!(s2.catalog.photos().next().unwrap().develop.light.exposure, 0.7, "edits travel in the sidecar");

    // DNG: re-encoded raw data with the edits embedded
    let o = ExportOptions { format: ExportFormat::Dng, ..Default::default() };
    let e = export_photo(&mut s, id, &o, 1).unwrap();
    assert_eq!(e.file_name, "Shot.dng");
    assert!(e.sidecars.is_empty());
    let back = lightcraft_raw::decode(&e.bytes).expect("our DNG decodes");
    let orig = lightcraft_raw::decode(&dng).unwrap();
    assert_eq!((back.width, back.height), (orig.width, orig.height));
    assert_eq!(back.data, orig.data, "lossless");
    let out = dir.join("out.dng");
    std::fs::write(&out, &e.bytes).unwrap();
    let mut s2 = Session::new().with_fs();
    s2.execute("library.import", &json!({"paths": [out.to_string_lossy()]})).unwrap();
    assert_eq!(s2.catalog.photos().next().unwrap().develop.light.exposure, 0.7, "edits embedded in the DNG are read back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn original_and_dng_need_a_file() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    for f in ["original", "dng"] {
        let err = export_photo(&mut s, id, &ExportOptions::from_json(&json!({"format": f})), 1).err().unwrap();
        assert!(err.contains("no original file"), "{err}");
    }
}

#[test]
fn dng_export_rejects_non_raw_photos() {
    let dir = temp_dir("export-dng-jpeg");
    let src = dir.join("a.png");
    let img = li_raster::Rgba8::from_fn(32, 24, |x, y| [(x * 8) as u8, (y * 10) as u8, 90, 255]);
    std::fs::write(&src, crate::export::encode_image(&img, &ExportOptions { format: ExportFormat::Png, ..Default::default() }).unwrap()).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    let err = export_photo(&mut s, id, &ExportOptions { format: ExportFormat::Dng, ..Default::default() }, 1).err().unwrap();
    assert!(err.contains("needs a raw photo"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn presets_conflicts_and_subfolders() {
    use crate::export::{Destination, export_batch};
    let mut s = Session::with_demo();
    let ids: Vec<_> = s.visible().iter().copied().take(2).collect();
    // built-ins and user presets; the call's own params override the preset's
    let names: Vec<String> =
        s.execute("export.presets", &json!({})).unwrap().as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    assert!(names.contains(&"JPEG (Small)".to_string()) && names.contains(&"Original + Settings".to_string()), "{names:?}");
    let p = s.export_params(&json!({"preset": "jpeg (small)", "quality": 50})).unwrap();
    assert_eq!((p["longEdge"].as_u64(), p["quality"].as_u64(), p.get("preset")), (Some(2048), Some(50), None));
    assert!(s.export_params(&json!({"preset": "nope"})).unwrap_err().contains("unknown export preset"));
    assert!(s.execute("export.savePreset", &json!({"name": "JPEG (Large)", "params": {}})).is_err(), "built-in names are reserved");
    s.execute("export.savePreset", &json!({"name": "Web", "params": {"format": "png", "width": 64, "ids": [1], "dir": "/x"}})).unwrap();
    let web = s.export_params(&json!({"preset": "Web"})).unwrap();
    assert_eq!(web, json!({"format": "png", "width": 64}), "targets are not part of a preset");
    s.execute("export.savePreset", &json!({"name": "web", "params": {"format": "webp"}})).unwrap();
    assert_eq!(s.export_presets.len(), 1, "same name (any case) replaces");
    s.execute("export.deletePreset", &json!({"name": "WEB"})).unwrap();
    assert!(s.export_presets.is_empty());

    // a fake file system: what exists, what was written
    let existing = ["out/Sub/LC01347.png".to_string()];
    let to = Destination { dir: "out".into(), exact: None };
    let batch = |s: &mut Session, ids: &[_], o: &ExportOptions| {
        let mut written: Vec<String> = Vec::new();
        let mut write = |p: &str, _: &[u8]| {
            written.push(p.to_string());
            Ok(())
        };
        let files = export_batch(s, ids, o, &to, &mut write, &|p| existing.contains(&p.to_string())).unwrap();
        (files, written)
    };
    let o = ExportOptions::from_json(&json!({"format": "png", "width": 32, "subfolder": "Sub", "naming": "LC01347"}));
    // both photos get the same name from the template: the first is renamed past the existing
    // file, the second past the first
    let (files, written) = batch(&mut s, &ids, &o);
    assert_eq!(written, ["out/Sub/LC01347-2.png", "out/Sub/LC01347-3.png"]);
    assert_eq!(files[0]["width"], 32);
    let (files, written) = batch(&mut s, &ids, &ExportOptions { conflict: crate::export::Conflict::Skip, ..o.clone() });
    assert_eq!(files.iter().filter(|f| f["skipped"] == "out/Sub/LC01347.png").count(), 2, "{files:?}");
    assert!(written.is_empty());
    let (_, written) = batch(&mut s, &ids[..1], &ExportOptions { conflict: crate::export::Conflict::Overwrite, ..o });
    assert_eq!(written, ["out/Sub/LC01347.png"]);
}

#[test]
fn prepared_exports_run_on_another_thread() {
    fn send<T: Send>(_: &T) {}
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let o = ExportOptions::from_json(&json!({"format": "png", "width": 48}));
    let items: Vec<_> = (1..=2).map(|seq| crate::export::prepare_export(&mut s, id, &o, seq).unwrap()).collect();
    send(&items);
    let to = crate::export::Destination::default();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let files = std::thread::spawn(move || {
        let mut written = Vec::new();
        let mut write = |p: &str, b: &[u8]| {
            written.push((p.to_string(), b.len()));
            Ok(())
        };
        // cancel after the first photo
        crate::export::run_batch(items, &o, &to, &mut write, &|_| false, false, &mut |done, name| {
            seen2.lock().unwrap().push((done, name.to_string()));
            done < 1
        })
        .unwrap()
    })
    .join()
    .unwrap();
    assert_eq!(files.len(), 1, "cancelled before the second: {files:?}");
    assert_eq!(files[0]["width"], 48);
    assert_eq!(seen.lock().unwrap().len(), 2);
}

/// Issue #78: a GPU render whose work never ran (a driver that drops a submission without an
/// error) gave an all-black export. The export must come from the CPU instead — the same
/// image — with the reason recorded.
#[test]
fn export_falls_back_to_the_cpu_when_gpu_work_is_lost() {
    // The failed operation falls back; the health model may admit a retry after backoff.
    let _gpu = crate::tests_gpu::gpu_state();
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.4})).unwrap();
    s.execute("develop.set", &json!({"control": "effects.clarity", "value": 20})).unwrap();
    let o = ExportOptions::from_json(&json!({"format": "png"}));
    let mean = |b: &[u8]| {
        let d = lightcraft_codecs::decode(b, Default::default()).unwrap().image;
        d.data.iter().map(|p| (p[0] + p[1] + p[2]) as f64).sum::<f64>() / (3 * d.data.len()) as f64
    };
    let gpu = lightcraft_gpu::available();
    let healthy = mean(&export_photo(&mut s, id, &o, 1).unwrap().bytes);
    lightcraft_gpu::inject_fault(lightcraft_gpu::Fault::DropWork);
    let faulted = export_photo(&mut s, id, &o, 1).unwrap().bytes;
    if gpu {
        let why = lightcraft_gpu::last_fallback().unwrap_or_default();
        assert!(why.contains("incomplete"), "{why}");
        // A large CPU fallback can itself exceed the retry delay. Do not assert a permanent
        // process latch; force the reference backend so pixel equivalence stays meaningful.
    }
    lightcraft_gpu::set_enabled(false);
    let cpu = export_photo(&mut s, id, &o, 1).unwrap().bytes;
    lightcraft_gpu::set_enabled(true);
    lightcraft_gpu::reset_failures();
    let px = |b: &[u8]| lightcraft_codecs::decode(b, Default::default()).unwrap().image.data;
    assert!(px(&faulted) == px(&cpu), "the fallback is the CPU render");
    let m = mean(&faulted);
    assert!(m > 0.02, "not black: mean {m}");
    assert!((m - healthy).abs() < 0.01, "GPU {healthy} vs CPU {m}");
}

/// A JPEG original `dir/IMG_1.jpg` imported into a library on disk.
fn library_with_jpeg(tag: &str) -> (Session, lightcraft_catalog::PhotoId, std::path::PathBuf, std::path::PathBuf, Vec<u8>) {
    let dir = temp_dir(tag);
    let src = dir.join("IMG_1.jpg");
    let img = li_raster::Rgba8::from_fn(48, 32, |x, y| [(x * 5) as u8, (y * 7) as u8, 120, 255]);
    let bytes = crate::export::encode_image(&img, &ExportOptions::default()).unwrap();
    std::fs::write(&src, &bytes).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [src.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    s.execute("develop.set", &json!({"ids": [id.0], "control": "light.exposure", "value": 1.0})).unwrap();
    (s, id, dir, src, bytes)
}

fn disk_batch(
    s: &mut Session,
    id: lightcraft_catalog::PhotoId,
    o: &ExportOptions,
    to: &crate::export::Destination,
) -> Result<Vec<serde_json::Value>, String> {
    crate::export::export_batch(s, &[id], o, to, &mut crate::export::write_file, &|p| std::path::Path::new(p).exists())
}

/// Issue #93: exporting into the photo's own folder as `{name}` with "Overwrite" replaced the
/// original with the re-encoded render. It is refused now, whatever the policy or exact path.
#[test]
fn export_never_overwrites_an_original() {
    use crate::export::{Conflict, Destination};
    let (mut s, id, dir, src, original) = library_with_jpeg("export-guard");
    let folder = Destination { dir: dir.to_string_lossy().to_string(), exact: None };
    let o = ExportOptions { naming: "{name}".into(), conflict: Conflict::Overwrite, ..Default::default() };
    let err = disk_batch(&mut s, id, &o, &folder).unwrap_err();
    assert!(err.contains("original of IMG_1.jpg"), "{err}");
    assert_eq!(std::fs::read(&src).unwrap(), original, "the original is byte-identical");

    // the exact path of the control channel / MCP, also spelled another way
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    for exact in [src.to_string_lossy().to_string(), dir.join("sub/../IMG_1.jpg").to_string_lossy().to_string()] {
        let to = Destination { dir: String::new(), exact: Some(exact) };
        assert!(disk_batch(&mut s, id, &o, &to).unwrap_err().contains("never writes over an original"));
    }
    // the "Original" format re-writing the file onto itself (and its sidecar)
    let orig = ExportOptions { format: ExportFormat::Original, ..o.clone() };
    assert!(disk_batch(&mut s, id, &orig, &folder).is_err());
    assert_eq!(std::fs::read(&src).unwrap(), original);
    assert!(s.execute("export.checkTarget", &json!({"path": src.to_string_lossy()})).is_err());
    assert!(s.execute("export.checkTarget", &json!({"path": dir.join("free.jpg").to_string_lossy()})).is_ok());

    // Unique moves past it; Overwrite still replaces an ordinary earlier export
    let unique = ExportOptions { conflict: Conflict::Unique, ..o.clone() };
    let files = disk_batch(&mut s, id, &unique, &folder).unwrap();
    assert!(files[0]["path"].as_str().unwrap().ends_with("IMG_1-2.jpg"), "{files:?}");
    let earlier = dir.join("IMG_1-2.jpg");
    std::fs::write(&earlier, b"an earlier export").unwrap();
    let named = ExportOptions { naming: "{name}-2".into(), ..o };
    disk_batch(&mut s, id, &named, &folder).unwrap();
    assert_ne!(std::fs::read(&earlier).unwrap(), b"an earlier export", "an ordinary file is overwritten");
    assert_eq!(std::fs::read(&src).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Issue #93: the XMP sidecar of an "Original" export ignored the conflict policy.
#[test]
fn export_sidecars_follow_the_conflict_policy() {
    use crate::export::{Conflict, Destination};
    let (mut s, id, dir, _src, original) = library_with_jpeg("export-sidecar");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let theirs = out.join("IMG_1.xmp");
    std::fs::write(&theirs, b"someone else's sidecar").unwrap();
    let to = Destination { dir: out.to_string_lossy().to_string(), exact: None };
    let o = ExportOptions { format: ExportFormat::Original, naming: "{name}".into(), ..Default::default() };

    // Unique: the photo and its sidecar both move to the next free name
    let files = disk_batch(&mut s, id, &ExportOptions { conflict: Conflict::Unique, ..o.clone() }, &to).unwrap();
    assert!(files[0]["path"].as_str().unwrap().ends_with("IMG_1-2.jpg"), "{files:?}");
    assert!(files[0]["sidecars"][0].as_str().unwrap().ends_with("IMG_1-2.xmp"), "{files:?}");
    assert_eq!(std::fs::read(&theirs).unwrap(), b"someone else's sidecar");
    assert_eq!(std::fs::read(out.join("IMG_1-2.jpg")).unwrap(), original);
    // Skip: a taken sidecar name skips the photo
    std::fs::remove_file(out.join("IMG_1-2.jpg")).unwrap();
    std::fs::remove_file(out.join("IMG_1-2.xmp")).unwrap();
    let files = disk_batch(&mut s, id, &ExportOptions { conflict: Conflict::Skip, ..o.clone() }, &to).unwrap();
    assert!(files[0]["skipped"].as_str().is_some(), "{files:?}");
    assert!(!out.join("IMG_1.jpg").exists());
    assert_eq!(std::fs::read(&theirs).unwrap(), b"someone else's sidecar");
    // Overwrite: replaced, as asked
    disk_batch(&mut s, id, &ExportOptions { conflict: Conflict::Overwrite, ..o }, &to).unwrap();
    assert!(std::fs::read_to_string(&theirs).unwrap().contains("xmpmeta"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Issue #93: the native writer truncated the target first; a failure mid-write (full disk,
/// unplugged drive) now leaves the previous file intact and no partial one.
#[test]
fn a_failed_export_write_keeps_the_previous_file() {
    use crate::export::{Conflict, Destination};
    let (mut s, id, dir, _src, _) = library_with_jpeg("export-midwrite");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let prev = out.join("IMG_1.jpg");
    std::fs::write(&prev, b"last week's export").unwrap();
    let to = Destination { dir: out.to_string_lossy().to_string(), exact: None };
    let o = ExportOptions { conflict: Conflict::Overwrite, ..Default::default() };
    {
        let _fault = li_fs::fail_writes_after(100);
        let err = disk_batch(&mut s, id, &o, &to).unwrap_err();
        assert!(err.contains("injected"), "{err}");
    }
    assert_eq!(std::fs::read(&prev).unwrap(), b"last week's export");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1, "no temp file left behind");
    disk_batch(&mut s, id, &o, &to).unwrap();
    assert_ne!(std::fs::read(&prev).unwrap(), b"last week's export");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn export_protects_the_target_of_an_imported_symlink() {
    use crate::export::{Conflict, Destination};
    let (original_session, _, dir, src, original) = library_with_jpeg("export-symlink-original");
    drop(original_session);
    let alias = dir.join("alias.jpg");
    std::os::unix::fs::symlink(&src, &alias).unwrap();
    let sidecar = dir.join("metadata.xmp");
    std::fs::write(&sidecar, b"<x/>").unwrap();
    std::os::unix::fs::symlink(&sidecar, alias.with_extension("xmp")).unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [alias.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    s.execute("develop.set", &json!({"ids": [id.0], "control": "light.exposure", "value": 1.0})).unwrap();
    let o = ExportOptions { conflict: Conflict::Overwrite, ..Default::default() };
    let to = Destination { dir: String::new(), exact: Some(src.to_string_lossy().to_string()) };
    let result = disk_batch(&mut s, id, &o, &to);
    assert!(result.is_err(), "export replaced the original through its imported alias: {result:?}");
    assert_eq!(std::fs::read(&src).unwrap(), original);
    assert!(s.execute("export.checkTarget", &json!({"path": src.to_string_lossy()})).is_err());
    assert!(s.execute("export.checkTarget", &json!({"path": sidecar.to_string_lossy()})).is_err());
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"<x/>");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Issue #134: exports can always be made again, so they are written atomically but not synced to
/// disk file by file; an edit copy that becomes a library photo still is.
#[test]
fn exports_are_atomic_without_a_sync() {
    use crate::export::{Conflict, Destination};
    use li_fs::syncs_on_this_thread;
    let (mut s, id, dir, _src, _) = library_with_jpeg("export-nosync");
    let out = dir.join("out");
    let to = Destination { dir: out.to_string_lossy().to_string(), exact: None };
    let o = ExportOptions { conflict: Conflict::Unique, ..Default::default() };
    let before = syncs_on_this_thread();
    for _ in 0..3 {
        disk_batch(&mut s, id, &o, &to).unwrap();
    }
    assert_eq!(syncs_on_this_thread(), before, "no per-file sync for exports");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3, "three exports, no temp files");
    crate::export::write_file_durable(&out.join("edit.tif").to_string_lossy(), b"tiff").unwrap();
    assert!(syncs_on_this_thread() > before, "the durable writer syncs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn people_names_export_opt_in_metadata_and_original_sidecar() {
    use lightcraft_catalog::{Op, Photo, Source};
    use lightcraft_meta::{Rect, Region, RegionKind};
    let mut s = Session::new();
    let id = s.catalog.alloc_photo_id();
    let dir = temp_dir("people-names-export");
    let path = dir.join("original.png");
    let image = li_raster::Rgba8::filled(16, 16, [80, 90, 100, 255]);
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &Default::default()).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut p = Photo::new(id, Source::File { path: path.to_string_lossy().into_owned() }, "original.png", "PNG", 16, 16, "2026-10-09");
    p.meta.regions = vec![Region { kind: RegionKind::Face, name: Some("Jane Doe".into()), rect: Rect::UNIT, description: None, auto: true }];
    p.meta.keywords = vec!["Event".into()];
    s.commit("fixture", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    for on in [false, true] {
        let opts = ExportOptions::from_json(&json!({"includePeopleNames":on}));
        let metadata = crate::export::export_metadata(s.catalog.photo(id).unwrap(), &opts).unwrap();
        assert_eq!(metadata.keywords.contains(&"People|Jane Doe".to_string()), on);
        assert!(metadata.regions.is_empty());
        let exported = export_photo(&mut s, id, &ExportOptions { format: ExportFormat::Original, ..opts }, 1).unwrap();
        assert_eq!(exported.bytes, bytes);
        let sidecar = std::str::from_utf8(&exported.sidecars[0].1).unwrap();
        assert_eq!(sidecar.contains("People|Jane Doe"), on);
        assert!(!sidecar.contains("personIds"));
        assert!(!sidecar.contains("embedding"));
    }
    assert_eq!(s.catalog.photo(id).unwrap().meta.keywords, ["Event"]);
    let old: ExportOptions = serde_json::from_value(json!({})).unwrap();
    assert!(!old.include_people_names);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Upstream LightCraft db7e378: "Remove location info" holds for the file-copying formats too: the
/// sidecar and the DNG's packet carry no location, and an original that embeds GPS is refused
/// rather than copied.
#[test]
fn remove_location_holds_for_original_and_dng_exports() {
    let dir = temp_dir("export-noloc");
    let gps =
        lightcraft_meta::Metadata { gps: Some(lightcraft_meta::Gps { latitude: 48.85, longitude: 2.35, altitude: None }), ..Default::default() };
    // a raw with no embedded GPS, located in the catalog
    let raw = dir.join("Raw.dng");
    std::fs::write(&raw, synthetic_dng_with(None, Default::default())).unwrap();
    // a JPEG with GPS in its EXIF
    let jpg = dir.join("Geo.jpg");
    let img = li_raster::Rgba8::from_fn(32, 24, |x, y| [(x * 8) as u8, (y * 10) as u8, 90, 255]);
    let exif = lightcraft_meta::write_exif(&gps);
    let meta = lightcraft_codecs::EncodeMeta { exif: Some(&exif), ..Default::default() };
    std::fs::write(&jpg, lightcraft_codecs::encode_jpeg(&lightcraft_codecs::EncodeImage::rgba8(&img), 90, Default::default(), &meta).unwrap())
        .unwrap();
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [raw.to_string_lossy(), jpg.to_string_lossy()]})).unwrap();
    let id = |s: &Session, n: &str| s.catalog.photos().find(|p| p.file_name == n).unwrap().id;
    let (rid, jid) = (id(&s, "Raw.dng"), id(&s, "Geo.jpg"));
    s.execute("photo.setMeta", &json!({"ids": [rid.0, jid.0], "gps": [48.85, 2.35], "city": "Paris", "location": "Louvre"})).unwrap();
    let opts = |f: &str, remove: bool| ExportOptions::from_json(&json!({"format": f, "removeLocation": remove}));

    // original raw: the sidecar keeps the location unless it is removed
    let side = |e: &crate::export::Exported| std::str::from_utf8(&e.sidecars[0].1).unwrap().to_string();
    let kept = side(&export_photo(&mut s, rid, &opts("original", false), 1).unwrap());
    let kept_meta = lightcraft_meta::parse_xmp(&kept).unwrap().metadata;
    assert!(kept_meta.gps.is_some() && kept_meta.city.is_some() && kept.contains("Louvre"));
    let gone = side(&export_photo(&mut s, rid, &opts("original", true), 1).unwrap());
    let gone_meta = lightcraft_meta::parse_xmp(&gone).unwrap().metadata;
    assert!(gone_meta.gps.is_none() && gone_meta.city.is_none() && gone_meta.sublocation.is_none(), "{gone_meta:?}");
    assert!(!gone.contains("Louvre") && !gone.contains("Paris"), "{gone}");
    // the catalog keeps its location
    assert!(s.catalog.photo(rid).unwrap().meta.gps.is_some());

    // dng: the embedded packet
    let e = export_photo(&mut s, rid, &opts("dng", false), 1).unwrap();
    assert!(lightcraft_meta::extract(&e.bytes).gps.is_some());
    let e = export_photo(&mut s, rid, &opts("dng", true), 1).unwrap();
    let m = lightcraft_meta::extract(&e.bytes);
    assert!(m.gps.is_none() && m.city.is_none(), "{m:?}");

    // original JPEG embedding GPS: copied as is normally, refused when the location must go
    let e = export_photo(&mut s, jid, &opts("original", false), 1).unwrap();
    assert!(lightcraft_meta::extract(&e.bytes).gps.is_some());
    let err = export_photo(&mut s, jid, &opts("original", true), 1).err().unwrap();
    assert!(err.contains("can't remove the location") && err.contains("export as JPEG"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dsfix_original_export_refuses_textual_xmp_and_iptc_locations() {
    let dir = temp_dir("dsfix-export-places");
    let image = li_raster::Rgba8::from_fn(2, 2, |_, _| [20, 30, 40, 255]);
    for (i, place) in ["sublocation", "city", "state", "country"].into_iter().enumerate() {
        let mut metadata = lightcraft_meta::Metadata::default();
        match place {
            "sublocation" => metadata.sublocation = Some("Private".into()),
            "city" => metadata.city = Some("Private".into()),
            "state" => metadata.state = Some("Private".into()),
            _ => metadata.country = Some("Private".into()),
        }
        let xmp = lightcraft_meta::write_xmp(&metadata, None);
        let bytes = lightcraft_codecs::encode_jpeg(
            &lightcraft_codecs::EncodeImage::rgba8(&image),
            90,
            Default::default(),
            &lightcraft_codecs::EncodeMeta { xmp: Some(&xmp), ..Default::default() },
        )
        .unwrap();
        // Check the XMP-only and IPTC-only encodings through the actual Original export.
        for iptc in [false, true] {
            let bytes = if iptc {
                let dataset = [92, 90, 95, 101][i];
                let iim = [0x1c, 2, dataset, 0, 7, b'P', b'r', b'i', b'v', b'a', b't', b'e'];
                let mut ps = b"Photoshop 3.0\0".to_vec();
                ps.extend_from_slice(b"8BIM\x04\x04\0\0");
                ps.extend_from_slice(&(iim.len() as u32).to_be_bytes());
                ps.extend_from_slice(&iim);
                let plain =
                    lightcraft_codecs::encode_jpeg(&lightcraft_codecs::EncodeImage::rgba8(&image), 90, Default::default(), &Default::default())
                        .unwrap();
                let mut jpeg = plain[..2].to_vec();
                jpeg.extend_from_slice(&[0xff, 0xed]);
                jpeg.extend_from_slice(&((ps.len() + 2) as u16).to_be_bytes());
                jpeg.extend(ps);
                jpeg.extend_from_slice(&plain[2..]);
                jpeg
            } else {
                bytes.clone()
            };
            let file = dir.join(format!("places-{i}-{iptc}.jpg"));
            std::fs::write(&file, &bytes).unwrap();
            let mut session = Session::new().with_fs();
            let id = session.execute("library.import", &json!({"paths":[file.to_string_lossy()]})).unwrap()["imported"][0].as_u64().unwrap();
            let id = lightcraft_catalog::PhotoId(id);
            let keep = ExportOptions::from_json(&json!({"format":"original"}));
            assert_eq!(export_photo(&mut session, id, &keep, 1).unwrap().bytes, bytes);
            let remove = ExportOptions::from_json(&json!({"format":"original","removeLocation":true}));
            assert!(export_photo(&mut session, id, &remove, 1).err().unwrap().contains("can't remove the location"), "{place}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

pub(crate) fn synthetic_psd(version: u16, resources: &[(u16, &[u8])]) -> Vec<u8> {
    let mut section = Vec::new();
    for (id, bytes) in resources {
        section.extend_from_slice(b"8BIM");
        section.extend_from_slice(&id.to_be_bytes());
        section.extend_from_slice(&[0, 0]);
        section.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_be_bytes());
        section.extend_from_slice(bytes);
        if !bytes.len().is_multiple_of(2) {
            section.push(0);
        }
    }
    let mut out = b"8BPS".to_vec();
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&u32::try_from(section.len()).unwrap().to_be_bytes());
    out.extend_from_slice(&section);
    if version == 2 {
        out.extend_from_slice(&0u64.to_be_bytes());
    } else {
        out.extend_from_slice(&0u32.to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&[40, 80, 120]);
    out
}

pub(crate) fn synthetic_psd_with_metadata(version: u16, metadata: &lightcraft_meta::Metadata) -> Vec<u8> {
    let exif = lightcraft_meta::write_exif(metadata);
    let xmp = lightcraft_meta::write_xmp(metadata, None);
    synthetic_psd(version, &[(1058, &exif), (1060, xmp.as_bytes())])
}

#[test]
fn psb_import_retains_capture_time_and_all_metadata() {
    let dir = temp_dir("psb-metadata");
    let metadata = lightcraft_meta::Metadata {
        make: Some("Test".into()),
        model: Some("Camera".into()),
        lens_model: Some("Test lens".into()),
        capture_time: lightcraft_meta::DateTime::parse_iso("2023-05-14T14:40:39+02:00"),
        gps: Some(lightcraft_meta::Gps { latitude: 45.0, longitude: 7.0, altitude: None }),
        title: Some("Building".into()),
        keywords: vec!["city".into(), "architecture".into()],
        rating: Some(4),
        label: Some("Red".into()),
        ..Default::default()
    };
    let path = dir.join("Building.psb");
    std::fs::write(&path, synthetic_psd_with_metadata(2, &metadata)).unwrap();
    let mut session = Session::new().with_fs();
    let result = session.execute("library.import", &json!({"paths": [path]})).unwrap();
    assert_eq!(result["failed"], json!([]), "{result}");
    let photo = session.catalog.photos().next().unwrap();
    assert_eq!((photo.width, photo.height), (1, 1));
    assert_eq!(photo.captured.as_deref(), Some("2023-05-14T14:40:39+02:00"));
    assert_eq!(photo.meta.camera, "Test Camera");
    assert_eq!(photo.meta.lens, "Test lens");
    assert_eq!(photo.meta.gps, Some((45.0, 7.0)));
    assert_eq!(photo.meta.title, "Building");
    assert_eq!(photo.meta.keywords, metadata.keywords);
    let id = photo.id;
    let all = session.execute("photo.allMetadata", &json!({"id": id.0})).unwrap();
    assert!(all["exif"].as_array().unwrap().iter().any(|row| row["tag"] == json!(0x9003)), "{all}");
    for (name, value) in [("dc:title", "Building"), ("xmp:Rating", "4"), ("xmp:Label", "Red")] {
        assert!(all["xmp"].as_array().unwrap().iter().any(|row| row["name"] == name && row["value"] == value), "{all}");
    }
    drop(session);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn remove_location_refuses_a_psd_that_embeds_gps() {
    let dir = temp_dir("export-noloc-psd");
    let metadata =
        lightcraft_meta::Metadata { gps: Some(lightcraft_meta::Gps { latitude: 45.0, longitude: 7.0, altitude: None }), ..Default::default() };
    let path = dir.join("Geo.psd");
    let bytes = synthetic_psd_with_metadata(1, &metadata);
    std::fs::write(&path, &bytes).unwrap();
    let mut session = Session::new().with_fs();
    session.execute("library.import", &json!({"paths": [path]})).unwrap();
    let id = session.catalog.photos().next().unwrap().id;
    let original = export_photo(&mut session, id, &ExportOptions::from_json(&json!({"format": "original"})), 1).unwrap();
    assert_eq!(original.bytes, bytes);
    let error = export_photo(&mut session, id, &ExportOptions::from_json(&json!({"format": "original", "removeLocation": true})), 1).err().unwrap();
    assert!(error.contains("embeds location info") && error.contains("export as JPEG"), "{error}");
    drop(session);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn remove_location_refuses_a_psd_with_unreadable_resources_or_xmp() {
    let dir = temp_dir("export-noloc-malformed-psd");
    for (name, mut bytes) in [("resources.psd", synthetic_psd(1, &[])), ("xmp.psb", synthetic_psd(2, &[(1060, b"<x:xmpmeta><broken")]))] {
        if name == "resources.psd" {
            bytes.splice(34..34, *b"BAD!");
            bytes[30..34].copy_from_slice(&4u32.to_be_bytes());
        }
        let path = dir.join(name);
        let version = if name.ends_with("psb") { 2 } else { 1 };
        std::fs::write(&path, synthetic_psd(version, &[])).unwrap();
        let mut session = Session::new().with_fs();
        session.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
        let id = session.catalog.photos().next().unwrap().id;
        // The original can change after import; export must verify the bytes it will copy.
        std::fs::write(&path, &bytes).unwrap();
        let error =
            export_photo(&mut session, id, &ExportOptions::from_json(&json!({"format": "original", "removeLocation": true})), 1).err().unwrap();
        assert!(error.contains("can't verify location metadata") && error.contains("malformed PSD/PSB"), "{error}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn remove_location_refuses_psd_xmp_with_duplicate_or_unquoted_gps() {
    let dir = temp_dir("export-ambiguous-psd-xmp");
    for (index, attrs) in
        [r#"exif:GPSLatitude="" exif:GPSLatitude="45,0N" exif:GPSLongitude="7,0E""#, r#"exif:GPSLatitude="45,0N" exif:GPSLongitude=7,0E"#]
            .iter()
            .enumerate()
    {
        for (version, extension) in [(1, "psd"), (2, "psb")] {
            let path = dir.join(format!("ambiguous-{index}.{extension}"));
            std::fs::write(&path, synthetic_psd(version, &[])).unwrap();
            let mut session = Session::new().with_fs();
            session.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
            let id = session.catalog.photos().next().unwrap().id;
            let xmp = format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:exif="http://ns.adobe.com/exif/1.0/" {attrs}/></rdf:RDF>"#
            );
            std::fs::write(&path, synthetic_psd(version, &[(1060, xmp.as_bytes())])).unwrap();
            let error =
                export_photo(&mut session, id, &ExportOptions::from_json(&json!({"format": "original", "removeLocation": true})), 1).err().unwrap();
            assert!(error.contains("can't verify location metadata") && error.contains("malformed PSD/PSB"), "{error}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
