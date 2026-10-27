//! Settings that copy, sync, presets and resets must resolve for each photo on its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use li_raster::RgbFrame;
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_develop::Upright;
use lightcraft_geom::Homography;
use lightcraft_pipeline::{OutputDepth, OutputSpace, SourceInfo};
use serde_json::json;

use crate::Session;
use crate::media::SourceLevel;

const A: PhotoId = PhotoId(1);
const B: PhotoId = PhotoId(2);
const C: PhotoId = PhotoId(3);

/// Dark grid lines on a light ground, rolled by `deg` about the centre: a building shot with the
/// camera tilted sideways.
fn rolled_grid(w: usize, h: usize, deg: f64) -> RgbFrame {
    let (sin, cos) = deg.to_radians().sin_cos();
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    RgbFrame::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
        let off_line = |t: f64| {
            let m = t.rem_euclid(48.0);
            m.min(48.0 - m)
        };
        let d = off_line(dx * cos + dy * sin).min(off_line(-dx * sin + dy * cos));
        [(0.05 + 0.6 * ((d - 1.0) / 1.5).clamp(0.0, 1.0)) as f32; 3]
    })
}

/// How far photo `path` is rolled.
fn roll(path: &str) -> f64 {
    match path.chars().next() {
        Some('a') => 4.0,
        Some('b') => -6.0,
        _ => 9.0,
    }
}

/// Photos A, B and C rolled +4, -6 and +9 degrees, all selected, A active.
fn rolled_photos() -> Session {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|path: &str, _| Ok((rolled_grid(720, 480, roll(path)), SourceInfo::default()))));
    for (id, path) in [(A, "a.tif"), (B, "b.tif"), (C, "c.tif")] {
        let p = Photo::new(id, Source::File { path: path.into() }, path, "TIFF", 720, 480, "2026-01-01");
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    s.execute("library.select", &json!({"ids": [A.0, B.0, C.0], "active": A.0})).unwrap();
    s
}

/// Wait for the background Upright analyses and store them; the photos changed.
fn settle(s: &mut Session) -> usize {
    let t0 = std::time::Instant::now();
    while s.upright.busy() {
        assert!(t0.elapsed() < Duration::from_secs(60), "the Upright analysis finishes");
        std::thread::sleep(Duration::from_millis(2));
    }
    s.upright_poll()
}

fn transform_of(s: &Session, id: PhotoId) -> Option<Homography> {
    s.develop_of(id).unwrap().geometry.upright_transform
}

/// The transform photo `id`'s own preview-level source gives for `mode`.
fn own_transform(s: &mut Session, id: PhotoId, mode: Upright) -> Homography {
    let src = s.source_now(id, SourceLevel::Preview).unwrap();
    let info = s.source_info(id);
    lightcraft_pipeline::upright::auto_transform(&src, &info, &s.develop_of(id).unwrap(), mode)
}

fn max_diff(a: &Homography, b: &Homography) -> f64 {
    a.0.iter().zip(b.0.iter()).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

/// `targets` got `mode` at once without a transform (the command never analyses them itself);
/// once the background analysis is stored each carries the one from its own lines.
fn assert_solved_for_itself(s: &mut Session, targets: &[PhotoId], mode: Upright, how: &str) {
    for id in targets {
        let d = s.develop_of(*id).unwrap();
        assert_eq!(d.geometry.upright, mode, "{how}");
        assert!(d.upright_unsolved(), "{how}: photo {} is analysed in the background, not by the command", id.0);
    }
    assert_eq!(settle(s), targets.len(), "{how}");
    let a = transform_of(s, A).expect("A's analysis is stored");
    for id in targets {
        let t = transform_of(s, *id).unwrap_or_else(|| panic!("{how}: photo {} carries its own transform", id.0));
        assert!(max_diff(&a, &t) > 0.05, "{how}: photo {} must not get A's transform: {a:?} vs {t:?}", id.0);
        let want = own_transform(s, *id, mode);
        assert!(max_diff(&t, &want) < 1e-6, "{how}: photo {}'s transform comes from its own lines: {t:?} vs {want:?}", id.0);
    }
}

fn clear_upright(s: &mut Session, id: PhotoId) {
    let mut d = (*s.develop_of(id).unwrap()).clone();
    d.geometry = Default::default();
    s.catalog.apply(Op::SetDevelop { id, settings: Arc::new(d), label: "test".into(), edited: None }).unwrap();
}

#[test]
fn sync_paste_and_presets_solve_upright_for_each_photo() {
    let mut s = rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();

    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert_solved_for_itself(&mut s, &[B, C], Upright::Vertical, "Sync");

    clear_upright(&mut s, B);
    s.execute("develop.copy", &json!({"groups": ["geometry"]})).unwrap();
    s.execute("develop.paste", &json!({"ids": [B.0]})).unwrap();
    assert_solved_for_itself(&mut s, &[B], Upright::Vertical, "Paste");

    clear_upright(&mut s, B);
    let preset = s.execute("preset.create", &json!({"name": "Upright", "groups": ["geometry"]})).unwrap();
    s.execute("preset.apply", &json!({"id": preset["id"], "ids": [B.0]})).unwrap();
    assert_solved_for_itself(&mut s, &[B], Upright::Vertical, "Preset");

    clear_upright(&mut s, B);
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    s.execute("geometry.upright", &json!({"mode": "level"})).unwrap();
    assert_solved_for_itself(&mut s, &[B, C], Upright::Level, "Auto Sync");
}

#[test]
fn a_synced_upright_and_its_analysis_undo_as_one_step() {
    let mut s = rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    let steps = s.undo.len();
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert_eq!(settle(&mut s), 2);
    assert_eq!(s.undo.len(), steps + 1, "the analysis joins the Sync step");
    assert_eq!(s.undo.last().unwrap().label, "Sync Settings");
    s.execute("edit.undo", &json!({})).unwrap();
    for id in [B, C] {
        assert_eq!(s.develop_of(id).unwrap().geometry, Default::default(), "photo {} is as it was before the Sync", id.0);
    }
}

#[test]
fn an_analysis_skips_photos_changed_meanwhile_and_stays_its_own_step() {
    let mut s = rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    // C's Upright is changed before the analysis is stored
    s.execute("library.select", &json!({"ids": [C.0], "active": C.0})).unwrap();
    s.execute("geometry.upright", &json!({"mode": "level"})).unwrap();
    let c = transform_of(&s, C);
    let steps = s.undo.len();
    assert_eq!(settle(&mut s), 1, "only B is stored");
    assert_eq!(s.undo.len(), steps + 1, "a step of its own after another edit");
    assert_eq!(s.undo.last().unwrap().label, "Upright");
    assert!(transform_of(&s, B).is_some());
    assert_eq!(transform_of(&s, C), c, "C keeps what its own Upright command analysed");
}

#[test]
fn a_cancelled_analysis_leaves_upright_to_the_renders() {
    let mut s = rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    s.execute("develop.cancelUpright", &json!({})).unwrap();
    assert_eq!(settle(&mut s), 0);
    assert!(s.develop_of(B).unwrap().upright_unsolved());
    assert_eq!(s.execute("develop.uprightProgress", &json!({})).unwrap()["running"], false);
}

#[test]
fn an_unreadable_target_leaves_upright_to_its_render() {
    let mut s = rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    s.media.file_loader = Some(Arc::new(|_: &str, _| Err("offline".into())));
    s.media.forget(B);
    s.media.forget(C);
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert_eq!(settle(&mut s), 0);
    let b = s.develop_of(B).unwrap();
    assert_eq!(b.geometry.upright, Upright::Vertical);
    assert_eq!(b.geometry.upright_transform, None, "never A's transform");
    assert!(b.upright_unsolved());
}

/// A full-size render of a photo whose Upright is not stored yet (pasted, from XMP or an import
/// preset) uses the analysis of its preview-level source, as the loupe's render does, not one of
/// the original's own pixels: the keystone doesn't jump between the loupe and a 1:1 view or export.
#[test]
fn full_size_renders_analyse_upright_like_the_loupe() {
    let mut s = rolled_photos();
    // the original's pixels lean differently from the preview's, so the two analyses differ
    s.media.file_loader = Some(Arc::new(|path: &str, edge| {
        let full = if edge > SourceLevel::Preview.max_edge() { 5.0 } else { 0.0 };
        Ok((rolled_grid(720, 480, roll(path) + full), SourceInfo::default()))
    }));
    s.execute("library.select", &json!({"ids": [B.0], "active": B.0})).unwrap();
    let mut d = (*s.develop_of(B).unwrap()).clone();
    d.geometry.upright = Upright::Level;
    s.catalog.apply(Op::SetDevelop { id: B, settings: Arc::new(d.clone()), label: "xmp".into(), edited: None }).unwrap();

    let full = s.detail_job(B, 720, 480, true).unwrap();
    assert_eq!(full.level, SourceLevel::Full);
    assert!(full.upright_source.is_some(), "an unsolved full-size render carries the preview-level source");
    let unsolved = full.run().rendered.unwrap().image;

    let solved_with = |s: &mut Session, h: Homography| {
        let mut d = d.clone();
        d.geometry.upright_transform = Some(h);
        s.catalog.apply(Op::SetDevelop { id: B, settings: Arc::new(d), label: "t".into(), edited: None }).unwrap();
        let job = s.detail_job(B, 720, 480, true).unwrap();
        assert!(job.upright_source.is_none(), "a stored transform needs no analysis");
        job.run().rendered.unwrap().image
    };
    let preview = own_transform(&mut s, B, Upright::Level);
    let original = {
        let src = s.source_now(B, SourceLevel::Full).unwrap();
        lightcraft_pipeline::upright::auto_transform(&src, &s.source_info(B), &d, Upright::Level)
    };
    assert!(max_diff(&preview, &original) > 0.02, "the test needs analyses that differ: {preview:?} vs {original:?}");
    let diff = |a: &li_raster::Rgba8, b: &li_raster::Rgba8| {
        let sum: f64 = a.data.iter().zip(&b.data).flat_map(|(x, y)| (0..3).map(move |c| f64::from(x[c].abs_diff(y[c])))).sum();
        sum / (a.data.len() * 3) as f64
    };
    let like_loupe = solved_with(&mut s, preview);
    let like_original = solved_with(&mut s, original);
    assert!(diff(&unsolved, &like_loupe) < 0.5, "the 1:1 render keeps the loupe's keystone");
    assert!(diff(&unsolved, &like_original) > 2.0 * diff(&unsolved, &like_loupe).max(0.5), "not the original's own analysis");
}

#[test]
fn hover_previews_of_photos_with_equal_settings_are_told_apart() {
    let mut s = rolled_photos();
    let mut look = (*s.develop_of(A).unwrap()).clone();
    look.light.exposure = 0.7;
    let a = s.preview_job(A, 800, 600, true, &look).unwrap().key;
    let b = s.preview_job(B, 800, 600, true, &look).unwrap().key;
    assert_eq!(s.develop_of(A), s.develop_of(B), "both photos hold the same settings");
    assert_ne!(a, b, "the same hovered look on another photo is another render");
    assert_eq!(a, s.preview_job(A, 800, 600, true, &look).unwrap().key, "the same hover on the same photo reuses its render");
}

/// A drag's end outside a command (`end_interaction`, as the menus call it) with Auto Sync on
/// starts the synced photos' Upright analysis at once, and it undoes with the drag.
#[test]
fn an_auto_synced_drag_end_analyses_upright_within_its_own_step() {
    let mut s = rolled_photos();
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    let steps = s.undo.len();
    s.begin_interaction("Upright Drag").unwrap();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    s.end_interaction().unwrap();
    assert_eq!(s.upright.jobs().len(), 1, "the analysis starts without waiting for another command");
    assert_eq!(settle(&mut s), 2);
    assert_eq!(s.undo.len(), steps + 1, "one step with the drag");
    assert_eq!(s.undo.last().unwrap().label, "Upright Drag");
    let a = transform_of(&s, A).unwrap();
    for id in [B, C] {
        assert!(transform_of(&s, id).is_some_and(|t| max_diff(&a, &t) > 0.05), "photo {} has its own analysis", id.0);
    }
}

fn mean(img: &li_raster::Rgba8) -> f64 {
    img.data.iter().map(|p| f64::from(p[0])).sum::<f64>() / img.data.len() as f64
}

/// A smart preview stands in for an original that can't be read (a NAS share dropped) only where
/// a stand-in is wanted: never as the 1:1 view, and never kept as the original, so once the
/// original reads again the 1:1 view and exports use its own pixels.
#[test]
fn a_smart_preview_never_stands_in_for_the_original_at_full_size() {
    let dir = std::env::temp_dir().join(format!("lc-proxy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.tif");
    // the original is there (not known to be offline), but reading it fails
    std::fs::write(&path, b"").unwrap();
    let readable = Arc::new(AtomicBool::new(false));
    let mut s = Session::new();
    let r = readable.clone();
    s.media.file_loader = Some(Arc::new(move |_: &str, _| match r.load(Ordering::Relaxed) {
        true => Ok((RgbFrame::filled(720, 480, [0.6; 3]), SourceInfo::default())),
        false => Err("the share is not responding".into()),
    }));
    let p = Photo::new(A, Source::File { path: path.to_string_lossy().into() }, "a.tif", "TIFF", 720, 480, "2026-01-01");
    let proxy = crate::smart::encode(&RgbFrame::filled(360, 240, [0.05; 3]), None).unwrap();
    std::fs::write(dir.join(crate::smart::file_name(&p)), proxy).unwrap();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.media.smart_dir = Some(dir.clone());
    let export = |s: &mut Session| s.export_job(A, 720, 480, OutputSpace::Srgb, OutputDepth::U8).unwrap();

    let detail = s.detail_job(A, 720, 480, true).unwrap();
    assert!(!detail.source.is_proxy());
    let r = detail.run();
    assert!(r.rendered.is_err(), "no 1:1 view from the smart preview");
    s.accept(&r);
    let r = export(&mut s).run();
    let proxy = mean(&r.rendered.as_ref().expect("an export of an unreadable original uses its smart preview").image);
    s.accept(&r);
    assert!(!export(&mut s).source.is_proxy(), "the smart preview is not kept as the original");

    readable.store(true, Ordering::Relaxed);
    let exported = mean(&export(&mut s).run().rendered.unwrap().image);
    let detail = mean(&s.detail_job(A, 720, 480, true).unwrap().run().rendered.unwrap().image);
    assert!(exported > proxy + 60.0, "the export reads the original again: {exported} vs the proxy's {proxy}");
    assert!((detail - exported).abs() < 1.0, "and so does the 1:1 view: {detail} vs {exported}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The loupe's 1:1 render has the full-size export's pixels: a 1-pixel checkerboard with capture
/// and output sharpening comes out the same both ways, its 1-pixel detail intact.
#[test]
fn the_1_to_1_view_shows_the_exports_pixels() {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|_: &str, _| {
        let checker = RgbFrame::from_fn(600, 400, |x, y| [if (x + y) % 2 == 0 { 0.5 } else { 0.1 }; 3]);
        Ok((checker, SourceInfo { raw: true, sensor_scale: 1.0, ..Default::default() }))
    }));
    let mut p = Photo::new(A, Source::File { path: "a.tif".into() }, "a.tif", "TIFF", 600, 400, "2026-01-01");
    let mut d = lightcraft_develop::DevelopSettings::default();
    (d.raw.capture.enabled, d.raw.capture.radius) = (true, 1.0);
    d.detail.sharpen_amount = 40.0;
    p.develop = Arc::new(d);
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    let detail = s.detail_job(A, 600, 400, true).unwrap();
    assert_eq!(detail.level, SourceLevel::Full);
    let detail = detail.run().rendered.unwrap().image;
    let export = s.export_job(A, 600, 400, OutputSpace::Srgb, OutputDepth::U8).unwrap().run().rendered.unwrap().image;
    assert_eq!((detail.width, detail.height), (export.width, export.height));
    let max = detail.data.iter().zip(&export.data).flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c]))).max().unwrap();
    assert!(max <= 3, "1:1 and export differ by {max} levels");
    let (x, y) = (300, 200);
    let step = detail.get(x, y)[1].abs_diff(detail.get(x + 1, y)[1]);
    assert!(step > 60, "neighbouring pixels keep the checkerboard: {step}");
}

/// A raw as import leaves it: measured as-shot white balance 5200 K / +4, embedded lens
/// corrections, the camera defaults (sharpening, colour noise reduction, lens profile on).
fn imported_raw() -> (Session, PhotoId) {
    let mut s = Session::new();
    let id = PhotoId(7);
    let mut p = Photo::new(id, Source::File { path: "a1.arw".into() }, "a1.arw", "ARW", 64, 48, "2026-01-01");
    p.kind = lightcraft_catalog::MediaKind::Raw;
    p.measured_wb = true;
    p.as_shot_wb = Some((5200.0, 4.0));
    p.embedded_lens = Some(lightcraft_develop::EmbeddedLens::default());
    p.develop = Arc::new(p.camera_defaults());
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.execute("library.select", &json!({"ids": [id.0], "active": id.0})).unwrap();
    (s, id)
}

#[test]
fn reset_returns_a_raw_to_its_camera_defaults() {
    let (mut s, id) = imported_raw();
    s.execute("develop.set", &json!({"values": {"light.exposure": 1.0, "detail.sharpenAmount": 90.0, "detail.nrColor": 0.0}})).unwrap();
    s.execute("develop.reset", &json!({})).unwrap();
    let p = s.catalog.photo(id).unwrap();
    assert_eq!(*p.develop, p.camera_defaults());
    let d = &p.develop;
    assert_eq!((d.detail.sharpen_amount, d.detail.nr_color, d.optics.lens_profile), (40.0, 25.0, true));
    assert!(!p.is_edited(), "a reset photo reads as unedited");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.develop_of(id).unwrap().light.exposure, 1.0, "Reset is one undo step");
}

#[test]
fn resetting_a_control_uses_the_photos_own_default() {
    let (mut s, id) = imported_raw();
    s.execute("develop.set", &json!({"control": "detail.sharpenAmount", "value": 90})).unwrap();
    s.execute("develop.resetControl", &json!({"control": "detail.sharpenAmount"})).unwrap();
    assert_eq!(s.develop_of(id).unwrap().detail.sharpen_amount, 40.0);

    s.execute("develop.set", &json!({"control": "wb.temp", "value": 3000})).unwrap();
    assert_eq!(s.develop_of(id).unwrap().wb.mode, lightcraft_develop::WbMode::Custom);
    s.execute("develop.resetControl", &json!({"control": "wb.temp"})).unwrap();
    let wb = s.develop_of(id).unwrap().wb;
    assert_eq!((wb.mode, wb.temp, wb.tint), (lightcraft_develop::WbMode::AsShot, 5200.0, 4.0), "back to As Shot");

    s.execute("develop.set", &json!({"values": {"wb.temp": 3000, "wb.tint": 20}})).unwrap();
    s.execute("develop.resetControl", &json!({"control": "wb.temp"})).unwrap();
    let wb = s.develop_of(id).unwrap().wb;
    assert_eq!((wb.mode, wb.temp, wb.tint), (lightcraft_develop::WbMode::Custom, 5200.0, 20.0), "only Temp returns");

    s.execute("develop.set", &json!({"values": {"detail.sharpenAmount": 0, "detail.nrColor": 0, "detail.nrLuminance": 30}})).unwrap();
    s.execute("develop.resetSection", &json!({"section": "detail"})).unwrap();
    let d = s.develop_of(id).unwrap();
    assert_eq!((d.detail.sharpen_amount, d.detail.nr_color, d.detail.nr_luminance), (40.0, 25.0, 0.0));
}

#[test]
fn resetting_a_rendered_photo_keeps_the_generic_defaults() {
    let mut s = Session::new();
    let id = PhotoId(9);
    let p = Photo::new(id, Source::File { path: "a.jpg".into() }, "a.jpg", "JPEG", 64, 48, "2026-01-01");
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.execute("library.select", &json!({"ids": [id.0], "active": id.0})).unwrap();
    s.execute("develop.set", &json!({"values": {"detail.sharpenAmount": 60, "light.exposure": 0.5}})).unwrap();
    s.execute("develop.resetControl", &json!({"control": "detail.sharpenAmount"})).unwrap();
    assert_eq!(s.develop_of(id).unwrap().detail.sharpen_amount, 0.0);
    s.execute("develop.reset", &json!({})).unwrap();
    assert_eq!(*s.develop_of(id).unwrap(), lightcraft_develop::DevelopSettings::default());
}
