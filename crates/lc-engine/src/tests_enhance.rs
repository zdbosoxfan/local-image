//! local-image: AI Remove and AI Denoise through the commands, with a mock AI engine and a stand-in
//! denoiser — one undo step per result, copies leave them behind, foreign or missing results never
//! render, and the photo renders as before without them.

use std::sync::Arc;

use serde_json::json;

use crate::Session;
use crate::enhance::{AiHost, DenoiseState, Download, JobCtl, PatchState, RemoveEngine, RemoveRequest, RemoveResult, denoise_state, patch_state};

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
    fn start_model_download(&self, _: &str) -> Result<(), String> {
        Err("not in tests".into())
    }
    fn model_download(&self, _: &str) -> Option<Download> {
        None
    }
    fn cancel_model_download(&self, _: &str) {}
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

#[test]
fn denoise_through_the_commands() {
    let mut s = session();
    // (the last raw demo photo: `enhance::denoise`'s own tests use the first, with the same
    // stand-in model, so the stored results differ)
    let id = s.visible_cloned().into_iter().rev().find(|id| crate::enhance::denoise::can_denoise(&s, *id).is_ok()).unwrap();
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    assert!(matches!(denoise_state(&s, id), DenoiseState::NoModel { .. }));
    assert!(s.execute("enhance.denoise", &json!({"wait": true})).is_err(), "no model");
    // a stand-in: everything becomes mid grey
    s.enhance.denoiser = Some(Arc::new(|px, _, _, _| Ok(vec![[0.18, 0.18, 0.18]; px.len()])));
    assert_eq!(denoise_state(&s, id), DenoiseState::Ready);
    let before = centre(&mut s);
    s.execute("enhance.denoise", &json!({"wait": true})).unwrap();
    let d = s.develop_of(id).unwrap();
    let r = d.enhance.ai.expect("the result is referenced");
    assert_eq!(d.enhance.denoise, crate::enhance::session::DEFAULT_AMOUNT);
    assert_eq!(s.undo.last().map(|u| u.label.as_str()), Some("AI Denoise"));
    assert_eq!(denoise_state(&s, id), DenoiseState::Done { amount: 60.0 });
    let half = centre(&mut s);
    assert_ne!(half, before);
    s.execute("develop.set", &json!({"control": "enhance.denoise", "value": 100})).unwrap();
    let full = centre(&mut s);
    let grey = |c: [u8; 4]| (c[0] as i32 - c[2] as i32).abs();
    // A nonlinear tone curve can increase display chroma of the mixed source. The full
    // replacement must be neutral, and the partial amount must keep some source colour.
    assert!(grey(full) <= 1 && grey(half) > grey(full), "{before:?} {half:?} {full:?}");
    // the result gone: the photo renders from its own pixels and Develop offers to run it again
    crate::enhance::store::delete(crate::enhance::store::Kind::Denoise, &r.key.to_string());
    crate::enhance::store::delete(crate::enhance::store::Kind::Denoise, &crate::enhance::denoise::preview_key(&r.key.to_string()));
    assert_eq!(denoise_state(&s, id), DenoiseState::Missing);
    s.execute("develop.set", &json!({"control": "enhance.denoise", "value": 60})).unwrap();
    assert_eq!(centre(&mut s), before);
    // copies keep the amount, not the result
    let c = lightcraft_develop::extract_groups(&s.develop_of(id).unwrap(), &[lightcraft_develop::SettingsGroup::Detail]);
    assert!(c["enhance"].get("ai").is_none());
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
