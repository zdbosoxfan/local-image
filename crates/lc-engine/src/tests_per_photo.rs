//! Settings that copy, sync, presets and resets must resolve for each photo on its own.

use std::sync::Arc;

use li_raster::RgbFrame;
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_develop::Upright;
use lightcraft_geom::Homography;
use lightcraft_pipeline::SourceInfo;
use serde_json::json;

use crate::Session;
use crate::media::SourceLevel;

const A: PhotoId = PhotoId(1);
const B: PhotoId = PhotoId(2);

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

/// Photo A rolled +4 degrees and photo B rolled -6 degrees, both selected, A active.
fn two_rolled_photos() -> Session {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|path: &str, _| {
        let deg = if path.starts_with("a") { 4.0 } else { -6.0 };
        Ok((rolled_grid(720, 480, deg), SourceInfo::default()))
    }));
    for (id, path) in [(A, "a.tif"), (B, "b.tif")] {
        let p = Photo::new(id, Source::File { path: path.into() }, path, "TIFF", 720, 480, "2026-01-01");
        s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    s.execute("library.select", &json!({"ids": [A.0, B.0], "active": A.0})).unwrap();
    s
}

fn transform_of(s: &Session, id: PhotoId) -> Option<Homography> {
    s.develop_of(id).unwrap().geometry.upright_transform
}

/// The transform B's own preview-level source gives for `mode`.
fn own_transform(s: &mut Session, id: PhotoId, mode: Upright) -> Homography {
    let src = s.source_now(id, SourceLevel::Preview).unwrap();
    let info = s.source_info(id);
    lightcraft_pipeline::upright::auto_transform(&src, &info, &s.develop_of(id).unwrap(), mode)
}

fn max_diff(a: &Homography, b: &Homography) -> f64 {
    a.0.iter().zip(b.0.iter()).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

fn assert_solved_for_itself(s: &mut Session, mode: Upright, how: &str) {
    let a = transform_of(s, A).expect("A's analysis is stored");
    let b = transform_of(s, B).unwrap_or_else(|| panic!("{how}: B carries its own transform"));
    assert_eq!(s.develop_of(B).unwrap().geometry.upright, mode, "{how}");
    assert!(max_diff(&a, &b) > 0.05, "{how}: B must not get A's transform: {a:?} vs {b:?}");
    let want = own_transform(s, B, mode);
    assert!(max_diff(&b, &want) < 1e-6, "{how}: B's transform comes from B's lines: {b:?} vs {want:?}");
}

fn clear_upright(s: &mut Session, id: PhotoId) {
    let mut d = (*s.develop_of(id).unwrap()).clone();
    d.geometry = Default::default();
    s.catalog.apply(Op::SetDevelop { id, settings: Arc::new(d), label: "test".into(), edited: None }).unwrap();
}

#[test]
fn sync_paste_and_presets_solve_upright_for_each_photo() {
    let mut s = two_rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();

    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert_solved_for_itself(&mut s, Upright::Vertical, "Sync");

    clear_upright(&mut s, B);
    s.execute("develop.copy", &json!({"groups": ["geometry"]})).unwrap();
    s.execute("develop.paste", &json!({"ids": [B.0]})).unwrap();
    assert_solved_for_itself(&mut s, Upright::Vertical, "Paste");

    clear_upright(&mut s, B);
    let preset = s.execute("preset.create", &json!({"name": "Upright", "groups": ["geometry"]})).unwrap();
    s.execute("preset.apply", &json!({"id": preset["id"], "ids": [B.0]})).unwrap();
    assert_solved_for_itself(&mut s, Upright::Vertical, "Preset");

    clear_upright(&mut s, B);
    s.execute("develop.autoSync", &json!({"on": true})).unwrap();
    s.execute("geometry.upright", &json!({"mode": "level"})).unwrap();
    assert_solved_for_itself(&mut s, Upright::Level, "Auto Sync");
}

#[test]
fn an_unreadable_target_leaves_upright_to_its_render() {
    let mut s = two_rolled_photos();
    s.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    s.media.file_loader = Some(Arc::new(|_: &str, _| Err("offline".into())));
    s.media.forget(B);
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    let b = s.develop_of(B).unwrap();
    assert_eq!(b.geometry.upright, Upright::Vertical);
    assert_eq!(b.geometry.upright_transform, None, "never A's transform");
    assert!(b.upright_unsolved());
}

#[test]
fn hover_previews_of_photos_with_equal_settings_are_told_apart() {
    let mut s = two_rolled_photos();
    let mut look = (*s.develop_of(A).unwrap()).clone();
    look.light.exposure = 0.7;
    let a = s.preview_job(A, 800, 600, true, &look).unwrap().key;
    let b = s.preview_job(B, 800, 600, true, &look).unwrap().key;
    assert_eq!(s.develop_of(A), s.develop_of(B), "both photos hold the same settings");
    assert_ne!(a, b, "the same hovered look on another photo is another render");
    assert_eq!(a, s.preview_job(A, 800, 600, true, &look).unwrap().key, "the same hover on the same photo reuses its render");
}
