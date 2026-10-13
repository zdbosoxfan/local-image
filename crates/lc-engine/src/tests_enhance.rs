//! AI Remove through the commands with a mock engine — one undo step per result,
//! copies leave patches behind, foreign or missing results never
//! render, and the photo renders as before without them.

use std::sync::Arc;

use serde_json::json;

use crate::Session;
use crate::enhance::{AiHost, JobCtl, PatchState, RemoveEngine, RemoveRequest, RemoveResult, patch_state};

/// Paints the masked area magenta.
struct Mock;

impl AiHost for Mock {
    fn remove_engines(&self) -> Vec<RemoveEngine> {
        vec![
            RemoveEngine { key: "mock".into(), label: "Mock".into(), problem: None },
            RemoveEngine { key: "off".into(), label: "Off".into(), problem: Some("The AI engine (ComfyUI) is not running.".into()) },
        ]
    }
    fn remove(&self, req: &RemoveRequest, ctl: &JobCtl) -> Result<RemoveResult, String> {
        ctl.set(0.5, "Sampling");
        let rgb = req.rgb.iter().zip(&req.mask).map(|(c, m)| if *m > 127 { [250, 0, 250] } else { *c }).collect();
        Ok(RemoveResult { rgb, alpha: req.mask.iter().map(|m| if *m > 127 { 255 } else { 0 }).collect() })
    }
}

fn session() -> Session {
    let mut s = Session::with_demo();
    s.enhance.host = Some(Arc::new(Mock));
    s
}

fn spots(s: &Session) -> Vec<lightcraft_develop::Spot> {
    s.develop_of(s.active().unwrap()).unwrap().spots.clone()
}

fn centre(s: &mut Session) -> [u8; 4] {
    let id = s.active().unwrap();
    let r = s.render_now(id, 160, 160).unwrap();
    r.image.get(r.image.width / 2, r.image.height / 2)
}

fn magenta(c: [u8; 4]) -> bool {
    c[0] as i32 > c[1] as i32 + 60 && c[2] as i32 > c[1] as i32 + 60
}

#[test]
fn bugfix_develop_regenerate_follows_patch_after_earlier_spot_deleted() {
    let mut s = session();
    s.execute("spot.add", &json!({"mode": "heal", "points": [[0.2, 0.2]]})).unwrap();
    s.execute("spot.add", &json!({"mode": "ai", "points": [[0.5, 0.5]], "engine": "mock", "seed": 7, "wait": true})).unwrap();
    let stroke =
        crate::enhance::AiStroke { points: spots(&s)[1].points.clone(), polygon: vec![], size: 0.02, feather: 50.0, opacity: 100.0, mask: None };
    // Inline worker completion is deliberately left unpolled while the user deletes another spot.
    s.start_ai_remove(s.active().unwrap(), stroke, Some("mock"), Some(8), Some(1), true).unwrap();
    s.execute("spot.delete", &json!({"index": 0})).unwrap();
    let polled = s.enhance_poll();
    assert!(polled.changed && polled.errors.is_empty(), "{polled:?}");
    assert_eq!(spots(&s)[0].patch.as_ref().unwrap().seed, 8);
}

#[test]
fn bugfix_develop_remove_rejects_replaced_source() {
    let mut s = session();
    let id = s.active().unwrap();
    let stroke = crate::enhance::AiStroke {
        points: vec![lightcraft_geom::Point::new(0.5, 0.5)],
        polygon: vec![],
        size: 0.02,
        feather: 50.0,
        opacity: 100.0,
        mask: None,
    };
    s.start_ai_remove(id, stroke, Some("mock"), Some(7), None, true).unwrap();
    let mut photo = (**s.catalog.photo(id).unwrap()).clone();
    photo.file_size += 1;
    photo.content_hash = Some("replacement-photo".into());
    s.catalog
        .apply(lightcraft_catalog::Op::SetContent {
            id,
            width: photo.width,
            height: photo.height,
            file_size: photo.file_size,
            content_hash: photo.content_hash,
            preview_only: photo.preview_only,
        })
        .unwrap();
    s.enhance_poll();
    assert!(spots(&s).is_empty(), "a result from the previous source must be discarded");
}

#[test]
fn ai_remove_is_one_undo_step_and_regenerates() {
    let mut s = session();
    let before = centre(&mut s);
    assert!(!magenta(before));
    let r = s.execute("spot.add", &json!({"mode": "ai", "points": [[0.5, 0.5]], "size": 0.05, "engine": "mock", "seed": 7, "wait": true})).unwrap();
    assert_eq!(r["done"], true);
    let sp = spots(&s);
    assert_eq!(sp.len(), 1);
    assert!(sp[0].is_ai());
    let patch = sp[0].patch.clone().expect("a patch");
    assert_eq!((patch.engine.as_str(), patch.seed), ("mock", 7));
    assert_eq!(s.active_spot, Some(0));
    assert_eq!(patch_state(&s, s.active().unwrap(), &sp[0]), PatchState::Ok);
    assert!(magenta(centre(&mut s)), "the patch renders");
    assert_eq!(s.undo.last().map(|u| u.label.as_str()), Some("AI Remove"));
    // it can't be moved, only faded, regenerated or deleted
    assert!(s.execute("spot.update", &json!({"move": [0.1, 0.0]})).is_err());
    assert!(s.execute("spot.refreshSource", &json!({})).is_err());
    s.execute("spot.update", &json!({"opacity": 40})).unwrap();
    assert_eq!(spots(&s)[0].opacity, 40.0);
    s.execute("spot.regenerate", &json!({"seed": 8, "wait": true})).unwrap();
    let sp = spots(&s);
    assert_eq!(sp.len(), 1);
    assert_eq!(sp[0].patch.as_ref().unwrap().seed, 8);
    assert_ne!(sp[0].patch.as_ref().unwrap().key, patch.key);
    // undo: back to the first patch, then to no spot (and the photo as before)
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(spots(&s)[0].patch.as_ref().unwrap().key, patch.key);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(spots(&s).is_empty());
    assert_eq!(centre(&mut s), before);
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(magenta(centre(&mut s)));
}

#[test]
fn lassos_jobs_and_engines() {
    let mut s = session();
    // a lasso, in the background
    let r = s.execute("spot.add", &json!({"mode": "ai", "polygon": [[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]], "engine": "mock"})).unwrap();
    let job = r["job"].as_u64().unwrap();
    assert!(s.execute("enhance.jobs", &json!({})).unwrap()["jobs"].as_array().unwrap().iter().any(|j| j["id"] == job));
    while s.enhance.busy() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let p = s.enhance_poll();
    assert!(p.changed && p.errors.is_empty(), "{p:?}");
    assert_eq!(spots(&s)[0].polygon.len(), 4);
    // an engine that isn't ready says why; no engine at all, likewise
    let e = s.execute("spot.add", &json!({"mode": "ai", "points": [[0.2, 0.2]], "engine": "off", "wait": true})).unwrap_err();
    assert!(e.to_string().contains("not running"), "{e}");
    s.enhance.host = None;
    assert!(s.execute("spot.add", &json!({"mode": "ai", "points": [[0.2, 0.2]], "wait": true})).is_err());
    let st = s.execute("enhance.status", &json!({})).unwrap();
    assert_eq!(st["spots"][0]["state"], "ok");
}

#[test]
fn copies_and_other_photos_never_get_ai_results() {
    let mut s = session();
    s.execute("spot.add", &json!({"mode": "ai", "points": [[0.5, 0.5]], "size": 0.05, "engine": "mock", "wait": true})).unwrap();
    s.execute("spot.add", &json!({"mode": "heal", "points": [[0.2, 0.2]]})).unwrap();
    let a = s.active().unwrap();
    let ids = s.visible_cloned();
    let b = *ids.iter().find(|x| **x != a).unwrap();
    s.execute("develop.copy", &json!({"groups": ["spots"]})).unwrap();
    s.execute("library.select", &json!({"ids": [b.0]})).unwrap();
    s.execute("develop.paste", &json!({})).unwrap();
    let pasted = s.develop_of(b).unwrap();
    assert_eq!(pasted.spots.len(), 1, "the heal spot only");
    assert!(!pasted.spots[0].is_ai());
    // settings carried over by hand: the patch belongs to another photo and isn't rendered
    let mut d = (*s.develop_of(b).unwrap()).clone();
    d.spots.clear();
    s.set_develop(b, d.clone(), "test").unwrap();
    let clean = centre(&mut s);
    d.spots = s.develop_of(a).unwrap().spots.iter().filter(|x| x.is_ai()).cloned().collect();
    s.set_develop(b, d, "test").unwrap();
    assert_eq!(patch_state(&s, b, &s.develop_of(b).unwrap().spots[0]), PatchState::Foreign);
    assert_eq!(centre(&mut s), clean, "rendered as without the AI spot");
}

/// A session whose active photo is `img` (from a file loader that makes it up), with no AI host.
fn synthetic(img: lightcraft_raster::Rgb32f) -> (Session, lightcraft_catalog::PhotoId) {
    use lightcraft_catalog::{Op, Photo, Source};
    let mut s = Session::with_demo();
    s.enhance.host = None;
    let (w, h) = (img.width, img.height);
    let img = Arc::new(img);
    s.media.file_loader = Some(Arc::new(move |_, _| Ok(((*img).clone(), lightcraft_pipeline::SourceInfo::default()))));
    let id = s.catalog.alloc_photo_id();
    let now = (s.clock)();
    let mut p = Photo::new(id, Source::File { path: "/synthetic/blemish.tif".into() }, "blemish.tif", "TIFF", w as u32, h as u32, &now);
    p.content_hash = Some("synthetic-blemish".into());
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    assert_eq!(s.active(), Some(id));
    (s, id)
}

/// Fine diagonal texture (two periods and a hash noise), about 0.3 on average.
fn texture(x: usize, y: usize) -> f32 {
    let n = ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 1000) as f32 / 1000.0;
    let stripes = if ((x + y) / 4).is_multiple_of(2) { 0.06 } else { -0.06 };
    0.3 + stripes + (n - 0.5) * 0.04
}

/// Mean and standard deviation of the green channel in the disc of radius `r` around `c`.
fn disc_stats(img: &lightcraft_raster::Rgb32f, c: (f32, f32), r: f32) -> (f32, f32) {
    let v: Vec<f32> = (0..img.height)
        .flat_map(|y| (0..img.width).map(move |x| (x, y)))
        .filter(|(x, y)| (*x as f32 + 0.5 - c.0).hypot(*y as f32 + 0.5 - c.1) < r)
        .map(|(x, y)| img.get(x, y)[1])
        .collect();
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    (mean, (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt())
}

#[test]
fn content_aware_heal_needs_no_ai_engine() {
    // a textured photo with a dark blemish (radius 8 px) at (200, 150)
    let (w, h) = (400, 300);
    let blemish = (200.0f32, 150.0f32);
    let src = lightcraft_raster::Rgb32f::from_fn(w, h, |x, y| {
        if (x as f32 + 0.5 - blemish.0).hypot(y as f32 + 0.5 - blemish.1) < 8.0 { [0.02; 3] } else { [texture(x, y); 3] }
    });
    let (texture_mean, texture_sd) = disc_stats(&lightcraft_raster::Rgb32f::from_fn(w, h, |x, y| [texture(x, y); 3]), blemish, 8.0);
    let (dark, _) = disc_stats(&src, blemish, 8.0);
    assert!(dark < 0.05);
    let (mut s, id) = synthetic(src.clone());
    // a heal stroke over it (radius 12 px), made on this computer
    let r = s
        .execute(
            "spot.add",
            &json!({"mode": "ai", "engine": "local", "points": [[0.5, 0.5], [0.501, 0.5]], "size": 0.03, "feather": 20, "wait": true}),
        )
        .unwrap();
    assert_eq!(r["done"], true, "{r}");
    let sp = spots(&s);
    assert_eq!(sp.len(), 1);
    let patch = sp[0].patch.clone().expect("a patch");
    assert_eq!(patch.engine, "local");
    assert_eq!(patch_state(&s, id, &sp[0]), PatchState::Ok);
    assert_eq!(s.undo.last().map(|u| u.label.as_str()), Some("Heal"));
    assert_eq!(s.active_spot, Some(0));
    // the photo with the patch: the blemish is gone, the texture goes on through it
    let healed = |s: &Session| {
        let d = s.develop_of(id).unwrap();
        let (frame, mut img) = lightcraft_pipeline::patches::transformed(&src, &lightcraft_pipeline::SourceInfo::default(), &d);
        lightcraft_pipeline::patches::apply_spots(&mut img, &d.spots, &frame);
        img
    };
    let img = healed(&s);
    let (mean, sd) = disc_stats(&img, blemish, 8.0);
    assert!((mean - texture_mean).abs() < 0.04, "the blemish is gone: {mean} (texture {texture_mean})");
    assert!(sd > texture_sd * 0.4, "texture kept: sd {sd} (texture {texture_sd})");
    assert_eq!(img.get(20, 20), src.get(20, 20), "the rest untouched");
    // Regenerate: another variation, still without an AI engine
    s.execute("spot.regenerate", &json!({"seed": 9, "wait": true})).unwrap();
    let sp = spots(&s);
    assert_eq!(sp.len(), 1);
    assert_ne!(sp[0].patch.as_ref().unwrap().key, patch.key);
    // a second heal over half of the first sees the first one's result: nothing dark comes back
    s.execute("spot.add", &json!({"mode": "ai", "engine": "local", "points": [[0.52, 0.5]], "size": 0.03, "feather": 20, "wait": true})).unwrap();
    assert_eq!(spots(&s).len(), 2);
    let img = healed(&s);
    let darkest = (130..170).flat_map(|y| (180..240).map(move |x| (x, y))).map(|(x, y)| img.get(x, y)[1]).fold(1.0f32, f32::min);
    assert!(darkest > 0.12, "nothing of the blemish shows again: {darkest}");
    // AI Remove proper still needs the engine
    assert!(s.execute("spot.add", &json!({"mode": "ai", "points": [[0.2, 0.2]], "wait": true})).is_err());
}

#[test]
fn final_review_remove_rejects_replaced_photo_and_library_jobs_are_discarded() {
    use crate::enhance::{AiStroke, JobKind, Outcome, RemoveDone};
    use lightcraft_catalog::{Op, Source};
    let mut s = session();
    let id = s.active().unwrap();
    let source = crate::enhance::source_hash(s.catalog.photo(id).unwrap());
    let stroke = AiStroke { points: vec![], polygon: vec![], size: 0.1, feather: 0.0, opacity: 100.0, mask: None };
    let outcome = Outcome::Remove(RemoveDone {
        stroke: stroke.clone(),
        key: "aa".repeat(16),
        source,
        rect: [0.2, 0.2, 0.4, 0.4],
        engine: "mock".into(),
        seed: 1,
        geometry: crate::enhance::remove::geometry_tag(&s.develop_of(id).unwrap()),
    });
    s.enhance.spawn(id, JobKind::Remove { stroke: stroke.clone() }, "AI Remove", true, Box::new(move |_| Ok(outcome)));
    let mut replacement = (**s.catalog.photo(id).unwrap()).clone();
    replacement.source = Source::File { path: "/another/photo.jpg".into() };
    replacement.content_hash = Some("different-photo".into());
    s.catalog.apply(Op::RemovePhoto { id }).unwrap();
    s.catalog.apply(Op::AddPhoto { photo: Box::new(replacement) }).unwrap();
    s.enhance_poll();
    assert!(s.develop_of(id).unwrap().spots.is_empty(), "same ID with different source must not receive an old result");
    let dir = std::env::temp_dir().join(format!("review-ai-library-{}", std::process::id()));
    let job = s.enhance.spawn(id, JobKind::DenoiseExport, "Export", true, Box::new(|_| Ok(Outcome::DenoiseExport("old.tif".into()))));
    s.open_library(&dir, true).unwrap();
    assert!(s.enhance.jobs().is_empty(), "opening must discard both pending and completed old jobs");
    assert!(job.ctl.cancelled());
    s.enhance.spawn(id, JobKind::DenoiseExport, "Export", true, Box::new(|_| Ok(Outcome::DenoiseExport("old.tif".into()))));
    s.close_library().unwrap();
    assert!(s.enhance.jobs().is_empty());
    drop(s);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks() {
    use lightcraft_develop::{DevelopSettings, Mask, MaskComponent, MaskShape, SegMask};
    let mut s = Session::with_demo();
    let a = s.active().unwrap();
    let b = *s.visible_cloned().iter().find(|id| **id != a).unwrap();
    let mask = Mask {
        id: 1,
        name: "Subject".into(),
        components: vec![MaskComponent {
            name: None,
            op: Default::default(),
            invert: false,
            shape: MaskShape::Subject { seg: Some(SegMask::from_logits(2, &[4.0; 4])) },
        }],
        ..Default::default()
    };
    let source = DevelopSettings { masks: vec![mask], ..Default::default() };
    s.set_develop(a, source.clone(), "Source").unwrap();
    s.execute("develop.copy", &json!({"groups":["masks"]})).unwrap();
    s.execute("develop.paste", &json!({"ids":[b.0]})).unwrap();
    let check = |s: &Session| {
        assert!(
            matches!(s.develop_of(b).unwrap().masks[0].components[0].shape, MaskShape::Subject { seg: None }),
            "target must use its own selection"
        );
    };
    check(&s);
    s.execute("library.select", &json!({"ids":[a.0,b.0], "active":a.0})).unwrap();
    s.execute("develop.sync", &json!({"groups":["masks"]})).unwrap();
    check(&s);
    s.auto_sync = true;
    let mut modified = source.clone();
    modified.masks[0].components[0].shape = MaskShape::Sky { seg: Some(SegMask::from_logits(2, &[8.0; 4])) };
    s.set_develop(a, modified, "AI mask edit").unwrap();
    // Auto Sync never carries layers (TEST-CHANGED: B used to get A's Sky mask); B keeps its own
    check(&s);
    s.auto_sync = false;
    // Imported old presets can still contain a source photo's cached selections.
    s.presets.push(lightcraft_develop::Preset {
        id: "review-ai".into(),
        name: "AI".into(),
        group: "User".into(),
        settings: json!({"masks": source.masks}),
        favorite: false,
        builtin: false,
    });
    s.set_develop(b, DevelopSettings::default(), "Reset target").unwrap();
    s.execute("preset.apply", &json!({"id":"review-ai", "ids":[b.0]})).unwrap();
    check(&s);
    assert!(matches!(s.develop_of(a).unwrap().masks[0].components[0].shape, MaskShape::Sky { seg: Some(_) }), "source retains its selection");
}

#[test]
fn final_review_changing_selection_keeps_the_original_remove_target() {
    let mut s = session();
    let a = s.active().unwrap();
    let b = *s.visible_cloned().iter().find(|id| **id != a).unwrap();
    use crate::enhance::{AiStroke, JobKind, Outcome, RemoveDone};
    let stroke = AiStroke { points: vec![], polygon: vec![], size: 0.1, feather: 0.0, opacity: 100.0, mask: None };
    let source = crate::enhance::source_hash(s.catalog.photo(a).unwrap());
    let outcome = Outcome::Remove(RemoveDone {
        stroke: stroke.clone(),
        key: "aa".repeat(16),
        source,
        rect: [0.2, 0.2, 0.4, 0.4],
        engine: "mock".into(),
        seed: 42,
        geometry: crate::enhance::remove::geometry_tag(&s.develop_of(a).unwrap()),
    });
    s.enhance.spawn(a, JobKind::Remove { stroke }, "AI Remove", true, Box::new(move |_| Ok(outcome)));
    s.execute("library.select", &json!({"ids":[b.0]})).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while s.enhance.busy() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!s.enhance.busy());
    let result = s.enhance_poll();
    assert!(result.changed && result.errors.is_empty(), "{result:?}");
    assert_eq!(s.develop_of(a).unwrap().spots.len(), 1);
    assert!(s.develop_of(b).unwrap().spots.is_empty());
    assert_eq!(s.active(), Some(b));
}
