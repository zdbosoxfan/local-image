//! Remove tool commands: add (automatic source), select, move target/source, edit, refresh, delete.

use serde_json::json;

use crate::Session;

fn spots(s: &Session) -> Vec<lightcraft_develop::Spot> {
    s.develop_of(s.active().unwrap()).unwrap().spots.clone()
}

#[test]
fn spots_select_move_edit_refresh_and_delete() {
    let mut s = Session::with_demo();
    // Source-pin editing applies to Clone; automatic Heal/Remove make immutable local
    // patches, covered by the patch tests below.
    let r = s.execute("spot.add", &json!({"mode": "clone", "points": [[0.4, 0.5]], "size": 0.02})).unwrap();
    assert_eq!(r["index"], 0);
    assert_eq!(s.active_spot, Some(0));
    let src0 = spots(&s)[0].source_offset.expect("automatic source resolved");
    assert!(src0.x.hypot(src0.y) > 0.02, "the source is outside the spot: {src0:?}");
    s.execute("spot.add", &json!({"mode": "clone", "points": [[0.7, 0.3]], "source": [0.1, 0.0]})).unwrap();
    assert_eq!(s.active_spot, Some(1));
    assert_eq!(s.execute("library.state", &json!({})).unwrap()["activeSpot"], 1);
    s.execute("spot.select", &json!({"index": 0})).unwrap();
    assert_eq!(s.active_spot, Some(0));
    assert!(s.execute("spot.select", &json!({"index": 9})).is_err());
    // move the target (the source follows: its offset is relative), then the source alone
    s.execute("spot.update", &json!({"move": [0.05, -0.1]})).unwrap();
    let sp = &spots(&s)[0];
    assert!((sp.points[0].x - 0.45).abs() < 1e-9 && (sp.points[0].y - 0.4).abs() < 1e-9);
    assert_eq!(sp.source_offset, Some(src0));
    s.execute("spot.update", &json!({"moveSource": [0.01, 0.02]})).unwrap();
    let o = spots(&s)[0].source_offset.unwrap();
    assert!((o.x - src0.x - 0.01).abs() < 1e-9 && (o.y - src0.y - 0.02).abs() < 1e-9);
    // size / feather / opacity of the selected spot (clamped), one undo step each
    s.execute("spot.update", &json!({"size": 0.03, "feather": 20, "opacity": 150})).unwrap();
    let sp = &spots(&s)[0];
    assert_eq!((sp.size, sp.feather, sp.opacity), (0.03, 20.0, 100.0));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(spots(&s)[0].size, 0.02);
    // refresh: a different source, at least a spot radius away
    let before = spots(&s)[0].source_offset.unwrap();
    let r = s.execute("spot.refreshSource", &json!({})).unwrap();
    let after = spots(&s)[0].source_offset.unwrap();
    assert!((after.x - before.x).hypot(after.y - before.y) > 0.01, "{r}");
    // delete the selected spot: the selection clears; deleting before a selected one shifts it
    s.execute("spot.delete", &json!({})).unwrap();
    assert_eq!((spots(&s).len(), s.active_spot), (1, None));
    assert_eq!(spots(&s)[0].mode, lightcraft_develop::SpotMode::Clone);
    assert!(s.execute("spot.delete", &json!({})).is_err(), "nothing selected");
    s.execute("spot.add", &json!({"points": [[0.2, 0.2]], "source": [0.05, 0.0]})).unwrap();
    s.execute("spot.delete", &json!({"index": 0})).unwrap();
    assert_eq!(s.active_spot, Some(0));
    // another photo: no selection
    s.execute("library.next", &json!({})).unwrap();
    assert_eq!(s.active_spot, None);
}

/// Small, deterministic file-backed photo with a blemish, and no AI host.
fn j13_session() -> Session {
    use std::sync::Arc;

    use lightcraft_catalog::{Op, Photo, Source};
    let mut s = Session::new();
    let img = li_raster::RgbFrame::from_fn(64, 48, |x, y| {
        let dx = x as f32 + 0.5 - 32.0;
        let dy = y as f32 + 0.5 - 24.0;
        if dx.hypot(dy) < 3.0 { [0.02; 3] } else { [0.3 + ((x + y) % 4) as f32 * 0.025; 3] }
    });
    s.media.file_loader = Some(Arc::new(move |_, _| Ok((img.clone(), lightcraft_pipeline::SourceInfo::default()))));
    let id = s.catalog.alloc_photo_id();
    let mut p = Photo::new(id, Source::File { path: "/synthetic/j13-spots.tif".into() }, "j13-spots.tif", "TIFF", 64, 48, "2026-10-14");
    p.content_hash = Some("j13-spots-fixture".into());
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    s
}

#[test]
fn j13_heal_and_remove_commands_create_local_patches_and_undo_regenerate() {
    for mode in ["heal", "remove"] {
        let mut s = j13_session();
        let id = s.active().unwrap();
        let before = s.render_now(id, 64, 48).unwrap().image;
        let r = s.execute("spot.add", &json!({"mode": mode, "points": [[0.5, 0.5]], "size": 0.07, "feather": 20, "seed": 17, "wait": true})).unwrap();
        assert_eq!(r["done"], true);
        assert!(r["job"].is_u64());
        assert_eq!(s.active_spot, Some(0));
        assert_eq!(spots(&s).len(), 1);
        let sp = spots(&s)[0].clone();
        assert!(sp.is_ai(), "patches retain the existing catalog schema");
        assert_eq!(sp.source_offset, None);
        let original_patch = sp.patch.as_ref().unwrap();
        assert_eq!((original_patch.engine.as_str(), original_patch.seed), ("local", 17));
        assert_eq!(crate::enhance::patch_state(&s, id, &sp), crate::enhance::PatchState::Ok);
        assert_eq!(s.undo.len(), 1);
        assert_eq!(s.undo.last().unwrap().label, "Heal");
        let healed = s.render_now(id, 64, 48).unwrap().image;
        assert_ne!(healed.get(32, 24), before.get(32, 24), "the worker patch must reach the real render path");
        assert_eq!(healed.get(0, 0), before.get(0, 0));
        assert!(s.execute("spot.update", &json!({"move": [0.1, 0.0]})).is_err());
        assert!(s.execute("spot.refreshSource", &json!({})).is_err());
        s.execute("spot.update", &json!({"opacity": 40})).unwrap();
        assert_eq!(spots(&s)[0].opacity, 40.0);
        let regenerated = s.execute("spot.regenerate", &json!({"seed": 18, "wait": true})).unwrap();
        assert_eq!(regenerated["done"], true);
        assert_eq!(spots(&s).len(), 1);
        let next = spots(&s)[0].patch.clone().unwrap();
        assert_eq!((next.engine.as_str(), next.seed), ("local", 18));
        assert_ne!(next.key, original_patch.key);
        assert_eq!(spots(&s)[0].opacity, 40.0);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(spots(&s)[0].patch.as_ref().unwrap().key, original_patch.key);
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(spots(&s).is_empty());
        assert_eq!(s.render_now(id, 64, 48).unwrap().image.data, before.data);
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(s.render_now(id, 64, 48).unwrap().image.data, healed.data);
    }
}

#[test]
fn j13_default_remove_is_a_background_job_applied_once() {
    let mut s = j13_session();
    let r = s.execute("spot.add", &json!({"points": [[0.5, 0.5]], "size": 0.07, "seed": 19})).unwrap();
    let id = r["job"].as_u64().expect("automatic remove starts a job");
    assert!(spots(&s).is_empty());
    assert!(s.undo.is_empty(), "the undo step belongs to the completed result");
    let job = s.enhance.job(id).unwrap().clone();
    assert_eq!(job.label, "Heal");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !job.finished() {
        assert!(std::time::Instant::now() < deadline, "small local heal worker did not finish");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let applied = s.enhance_poll();
    assert!(applied.changed && applied.errors.is_empty(), "{applied:?}");
    assert_eq!(spots(&s).len(), 1);
    assert_eq!(s.undo.len(), 1);
    assert_eq!(spots(&s)[0].patch.as_ref().unwrap().engine, "local");
    assert!(!s.enhance_poll().changed);
    assert_eq!(s.undo.len(), 1);
}

#[test]
fn j13_heal_command_matches_existing_local_engine_exactly() {
    let mut regular = j13_session();
    let mut explicit_local = j13_session();
    let mut params =
        json!({"mode": "heal", "points": [[0.5, 0.5], [0.52, 0.5]], "size": 0.07, "feather": 30, "opacity": 75, "seed": 21, "wait": true});
    regular.execute("spot.add", &params).unwrap();
    params["mode"] = json!("ai");
    params["engine"] = json!("local");
    explicit_local.execute("spot.add", &params).unwrap();
    assert_eq!(spots(&regular), spots(&explicit_local), "stroke and patch metadata retain exact LOCAL behavior");
    let actual = regular.render_now(regular.active().unwrap(), 64, 48).unwrap().image;
    let reference = explicit_local.render_now(explicit_local.active().unwrap(), 64, 48).unwrap().image;
    assert_eq!(actual.data, reference.data, "regular Heal renders the identical stored patch");
}

#[test]
fn j13_explicit_sources_and_clone_keep_editable_inline_spots() {
    use lightcraft_develop::SpotMode;
    use lightcraft_geom::Point;

    let mut s = j13_session();
    for (i, (mode, expected)) in [("heal", SpotMode::Heal), ("remove", SpotMode::Remove), ("clone", SpotMode::Clone)].into_iter().enumerate() {
        let r = s.execute("spot.add", &json!({"mode": mode, "points": [[0.5, 0.5]], "source": [0.2, 0.0]})).unwrap();
        assert_eq!(r["index"], i);
        assert!(r.get("job").is_none());
        let sp = &spots(&s)[i];
        assert_eq!(sp.mode, expected);
        assert_eq!(sp.source_offset, Some(Point::new(0.2, 0.0)));
        assert!(sp.patch.is_none());
        s.execute("spot.update", &json!({"moveSource": [0.01, 0.02], "size": 0.03})).unwrap();
        assert_eq!(spots(&s)[i].size, 0.03);
        let moved = spots(&s)[i].source_offset.unwrap();
        assert!((moved.x - 0.21).abs() < 1e-9 && (moved.y - 0.02).abs() < 1e-9);
    }
    assert!(s.enhance.jobs().is_empty());
    let r = s.execute("spot.add", &json!({"mode": "clone", "points": [[0.4, 0.5]], "size": 0.02})).unwrap();
    assert_eq!(r["index"], 3);
    assert!(spots(&s)[3].source_offset.is_some(), "automatic Clone retains its source pin");
    assert!(spots(&s)[3].patch.is_none());
}

#[test]
fn j13_camera_raw_heal_stays_inline_and_survives_descriptor_roundtrip() {
    let mut s = j13_session();
    let dir = std::path::PathBuf::from("target/j13-tests").join(format!(
        "ephemeral-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let src = s.source_now(s.active().unwrap(), crate::media::SourceLevel::Full).unwrap();
    let photo = s.open_ephemeral(&dir, "J13 Filter", src.width, src.height, &src.data, &json!({})).unwrap();
    let before = s.render_now(photo, 64, 48).unwrap().image;
    for mode in ["heal", "remove"] {
        let r = s.execute("spot.add", &json!({"mode": mode, "points": [[0.5, 0.5]], "size": 0.07, "feather": 20})).unwrap();
        assert!(r["index"].is_u64());
        assert!(r.get("job").is_none());
    }
    assert!(s.enhance.jobs().is_empty());
    assert!(spots(&s).iter().all(|sp| sp.patch.is_none() && sp.source_offset.is_none()));
    let healed = s.render_now(photo, 64, 48).unwrap().image;
    assert_ne!(healed.get(32, 24), before.get(32, 24));
    assert_eq!(healed.get(0, 0), before.get(0, 0));
    let descriptor = s.close_ephemeral().unwrap();
    let restored: lightcraft_develop::DevelopSettings = serde_json::from_value(descriptor).unwrap();
    assert_eq!(restored.spots.len(), 2);
    assert!(restored.spots.iter().all(|sp| !sp.is_ai() && sp.patch.is_none()));
    let rendered =
        lightcraft_pipeline::render(&src, &lightcraft_pipeline::SourceInfo::default(), &restored, &lightcraft_pipeline::RenderRequest::fit(64, 48));
    assert_eq!(rendered.image.data, healed.data, "the Camera Raw descriptor remains self-contained");
    std::fs::remove_dir_all(dir).unwrap();
}
