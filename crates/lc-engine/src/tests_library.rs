//! Persistent library: commands survive a restart (with and without a clean close).

use serde_json::json;

use crate::Session;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lc-engine-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn open(dir: &std::path::Path, seed: bool) -> Session {
    let mut s = Session::new();
    s.open_library(dir, seed).unwrap();
    s
}

#[test]
fn edits_survive_restart_without_close() {
    let dir = temp_dir("crash");
    let mut s = open(&dir, true);
    assert!(s.library.as_ref().unwrap().report.created);
    assert!(s.catalog.len() > 5, "seeded demo");
    let id = s.selection.active.unwrap();
    s.execute("photo.rate", &json!({"rating": 5})).unwrap();
    s.execute("photo.flag", &json!({"flag": "reject"})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.25})).unwrap();
    // a slider drag: one undo step, logged once at the end
    s.execute("develop.beginInteraction", &json!({"label": "Contrast"})).unwrap();
    for v in [5, 10, 20, 33] {
        s.execute("develop.set", &json!({"control": "light.contrast", "value": v})).unwrap();
    }
    s.execute("develop.endInteraction", &json!({})).unwrap();
    s.execute("album.create", &json!({"name": "Keepers", "addSelected": true})).unwrap();
    s.execute("preset.create", &json!({"name": "Bright"})).unwrap();
    s.execute("edit.undo", &json!({})).ok();
    s.execute("edit.redo", &json!({})).ok();
    let expect = s.catalog.to_snapshot();
    let info = s.execute("library.info", &json!({})).unwrap();
    assert!(info["logRecords"].as_u64().unwrap() >= 5, "{info}");
    drop(s); // no close: like a crash

    let s2 = open(&dir, true);
    let r = &s2.library.as_ref().unwrap().report;
    assert!(!r.created && r.replayed >= 5, "{r:?}");
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let p = s2.catalog.photo(id).unwrap();
    assert_eq!((p.rating, p.develop.light.exposure, p.develop.light.contrast), (5, 1.25, 33.0));
    assert!(s2.catalog.albums().any(|a| a.name == "Keepers" && a.photos == vec![id]));
    assert!(s2.presets.iter().any(|p| p.name == "Bright" && !p.builtin));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Compaction at the threshold runs on a worker thread (issue #37): commands go on (and stay
/// durable) meanwhile, the frame loop's `persist_if_dirty` finishes it, and a crash at any time
/// keeps every command.
#[test]
fn threshold_compaction_runs_in_the_background() {
    let dir = temp_dir("bg-compact");
    let mut s = open(&dir, true);
    s.library.as_mut().unwrap().journal_mut().policy = lightcraft_catalog::SnapshotPolicy { max_records: 8, max_bytes: u64::MAX };
    let ids: Vec<_> = s.catalog.photos().map(|p| p.id).collect();
    let mut background = 0;
    for k in 0..60usize {
        s.selection = crate::Selection::single(ids[k % ids.len()]);
        s.execute("photo.rate", &json!({"rating": k % 6})).unwrap();
        if s.execute("library.info", &json!({})).unwrap()["persistence"]["snapshotRunning"].as_bool().unwrap() {
            background += 1;
        }
        if k == 30 {
            // a crash while a compaction may be in flight
            let expect = s.catalog.to_snapshot();
            let copy = temp_dir("bg-compact-crash");
            std::fs::create_dir_all(&copy).unwrap();
            for f in ["catalog.snap", "catalog.log"] {
                if dir.join(f).exists() {
                    std::fs::copy(dir.join(f), copy.join(f)).unwrap();
                }
            }
            let s2 = open(&copy, true);
            assert_eq!(s2.catalog.to_snapshot(), expect);
            let _ = std::fs::remove_dir_all(&copy);
        }
    }
    assert!(background > 0, "compactions ran in the background");
    for _ in 0..10_000 {
        s.persist_if_dirty();
        if !s.library.as_ref().unwrap().journal().snapshot_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let info = s.execute("library.info", &json!({})).unwrap();
    let p = &info["persistence"];
    assert!(p["snapshots"].as_u64() >= Some(2) && p["lastSnapshot"]["background"] == true, "{p}");
    // the log holds exactly the records after the snapshot
    assert_eq!(info["logRecords"].as_u64().unwrap(), info["seq"].as_u64().unwrap() - info["snapshotSeq"].as_u64().unwrap(), "{info}");
    let expect = s.catalog.to_snapshot();
    drop(s); // no close: like a crash
    let s2 = open(&dir, true);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_writes_snapshot_and_view_state() {
    let dir = temp_dir("close");
    let mut s = open(&dir, true);
    let ids = s.visible_cloned();
    s.execute("library.select", &json!({"ids": [ids[3].0]})).unwrap();
    s.execute("photo.rate", &json!({"rating": 2})).unwrap();
    let expect = s.catalog.to_snapshot();
    s.close_library().unwrap();
    assert_eq!(std::fs::metadata(dir.join("catalog.log")).unwrap().len(), 0);
    drop(s); // one session per library (issue #99)

    let s2 = open(&dir, true);
    let r = &s2.library.as_ref().unwrap().report;
    assert_eq!(r.replayed, 0);
    assert_eq!(s2.catalog.to_snapshot(), expect);
    assert_eq!(s2.selection.active, Some(ids[3]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Thumbnails rendered in one session are served from the disk cache in the next, without
/// loading the source; an edit changes the key.
#[test]
fn thumbnails_come_from_the_disk_cache_after_restart() {
    let dir = temp_dir("thumbs");
    let mut s = open(&dir, true);
    let ids: Vec<_> = s.visible_cloned().into_iter().take(3).collect();
    for id in &ids {
        let job = s.thumb_job(*id, 230).unwrap();
        assert_eq!(job.request.max_w, 256, "bucketed");
        let r = job.run();
        assert!(r.loaded.is_some(), "first render decodes the source");
        s.accept(&r);
        let img = r.rendered.unwrap().image;
        assert_eq!(img.width.max(img.height), 256);
    }
    drop(s);

    let mut s = open(&dir, true);
    for id in &ids {
        let r = s.thumb_job(*id, 240).unwrap().run();
        assert!(r.loaded.is_none(), "served from cache");
        let img = r.rendered.unwrap().image;
        assert_eq!(img.width.max(img.height), 256);
    }
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["cache"]["disk"]["hits"], 3, "{info}");
    // an edit is a new key: rendered (from the source), then cached
    s.execute("library.select", &json!({"ids": [ids[0].0]})).unwrap();
    s.execute("develop.set", &json!({"control": "light.exposure", "value": 0.5})).unwrap();
    let r = s.thumb_job(ids[0], 230).unwrap().run();
    assert!(r.loaded.is_some());
    // exact-size renders (loupe, before/after, exports) bypass the thumbnail cache
    assert!(s.render_job(ids[0], 230, 230, true, true).unwrap().cache.is_none());
    assert!(s.render_job(ids[0], 230, 230, false, true).unwrap().cache.is_none());
    s.execute("library.clearPreviews", &json!({})).unwrap();
    assert!(s.thumb_job(ids[1], 230).unwrap().run().loaded.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_library_without_seed_is_empty_and_compacts() {
    let dir = temp_dir("empty");
    let mut s = open(&dir, false);
    assert!(s.catalog.is_empty());
    s.execute("album.create", &json!({"name": "A"})).unwrap();
    s.execute("library.compact", &json!({})).unwrap();
    let info = s.execute("library.info", &json!({})).unwrap();
    assert_eq!(info["logRecords"], 0);
    assert_eq!(info["snapshotSeq"], info["seq"]);
    // persistence timings (issue #37): the append and the compaction were measured
    let p = &info["persistence"];
    assert!(p["appends"].as_u64() >= Some(1), "{p}");
    assert!(p["snapshots"].as_u64() >= Some(1), "{p}");
    assert!(p["lastSnapshot"]["bytes"].as_u64() > Some(0), "{p}");
    assert!(p["lastSnapshot"]["totalMs"].as_f64() >= p["lastSnapshot"]["serializeMs"].as_f64(), "{p}");
    drop(s); // one session per library (issue #99)
    let s2 = open(&dir, true);
    assert_eq!(s2.catalog.len(), 0, "an existing library is never seeded");
    assert_eq!(s2.catalog.albums().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_favorites_and_recent_survive_reopen() {
    let dir = temp_dir("profiles");
    let mut s = open(&dir, true);
    s.execute("profile.favorite", &json!({"id": "lc.bw.sepia"})).unwrap();
    s.execute("profile.favorite", &json!({"id": "lc.vivid"})).unwrap();
    s.execute("profile.favorite", &json!({"id": "lc.vivid"})).unwrap(); // toggled off again
    assert!(s.execute("profile.favorite", &json!({"id": "nope"})).is_err());
    for id in ["lc.neutral", "lc.vivid", "lc.film.cool-fade", "lc.mono", "lc.cine.teal-amber", "lc.muted.bleached", "lc.vivid"] {
        s.execute("develop.profile", &json!({"id": id})).unwrap();
    }
    let want_recent = ["lc.vivid", "lc.muted.bleached", "lc.cine.teal-amber", "lc.mono", "lc.film.cool-fade"];
    assert_eq!(s.profile_recent, want_recent, "newest first, deduplicated, at most 5");
    drop(s); // no clean close: every command persisted already
    let mut s = open(&dir, false);
    assert_eq!(s.profile_favorites, ["lc.bw.sepia"]);
    assert_eq!(s.profile_recent, want_recent);
    let menu = s.execute("profiles.menu", &json!({})).unwrap();
    assert_eq!(menu["favorites"][0]["name"], "Sepia Tone");
    assert_eq!(menu["recent"].as_array().unwrap().len(), 5);
    assert_eq!(menu["groups"][0]["name"], "Basic");
    let list = s.execute("profiles.list", &json!({})).unwrap();
    assert!(list.as_array().unwrap().iter().any(|p| p["id"] == "lc.bw.sepia" && p["favorite"] == true));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Smart albums with a rule set: all / any / none, nested groups, validation, live updates.
#[test]
fn smart_album_rule_sets() {
    let mut s = crate::Session::with_demo();
    s.clock = Box::new(|| "2026-10-02T12:00:00".to_string());
    let fields = s.execute("album.ruleFields", &serde_json::json!({})).unwrap();
    assert!(fields.as_array().unwrap().iter().any(|f| f["field"] == "keywords" && f["ops"].as_array().unwrap().len() > 3));
    let bad = s.execute(
        "album.createSmart",
        &serde_json::json!({"name": "Bad", "rules": {"ruleSet": {"rules": [{"field": "rating", "op": "contains", "value": 1}]}}}),
    );
    assert!(bad.is_err(), "operators are checked");
    let r = s
        .execute(
            "album.createSmart",
            &serde_json::json!({"name": "Good ones", "rules": {"ruleSet": {"match": "all", "rules": [
                {"field": "rating", "op": "gte", "value": 4},
                {"group": {"match": "none", "rules": [{"field": "flag", "op": "is", "value": "reject"}]}}
            ]}}}),
        )
        .unwrap();
    let id = lightcraft_catalog::AlbumId(r["id"].as_u64().unwrap());
    let want = s.catalog.photos().filter(|p| !p.deleted && p.rating >= 4 && p.flag != lightcraft_catalog::Flag::Reject).count();
    assert!(want > 0);
    assert_eq!(s.catalog.album_count(id), want);
    // live: a new 5-star photo joins
    let other = s.catalog.photos().find(|p| p.rating < 4 && !p.deleted).unwrap().id;
    s.execute("photo.rate", &serde_json::json!({"ids": [other.0], "rating": 5})).unwrap();
    assert_eq!(s.catalog.album_count(id), want + 1);
    // the same rules work as a library filter (agents: library.filter {ruleSet})
    s.execute("library.filter", &serde_json::json!({"ruleSet": {"rules": [{"field": "rating", "op": "is", "value": 5}]}})).unwrap();
    assert!(s.visible_cloned().iter().all(|id| s.catalog.photo(*id).unwrap().rating == 5));
    let list = s.execute("albums.list", &serde_json::json!({})).unwrap();
    let a = list.as_array().unwrap().iter().find(|a| a["name"] == "Good ones").unwrap().clone();
    assert!(a.to_string().contains("rating is ≥ 4"), "{a}");
}

/// The filter bar's label multi-select: any of the chosen labels.
#[test]
fn filter_any_of_several_labels() {
    use lightcraft_catalog::ColorLabel;
    let mut s = crate::Session::with_demo();
    let ids: Vec<u64> = s.catalog.photos().take(3).map(|p| p.id.0).collect();
    for (id, l) in ids.iter().zip(["red", "yellow", "green"]) {
        s.execute("photo.label", &serde_json::json!({"ids": [id], "label": l})).unwrap();
    }
    s.execute("library.filter", &serde_json::json!({"labels": ["red", "yellow"]})).unwrap();
    let vis = s.visible_cloned();
    let want = s.catalog.photos().filter(|p| p.in_library() && matches!(p.label, Some(ColorLabel::Red | ColorLabel::Yellow))).count();
    assert_eq!(want, 2);
    assert_eq!(vis.len(), want);
    assert!(vis.iter().all(|id| matches!(s.catalog.photo(*id).unwrap().label, Some(ColorLabel::Red | ColorLabel::Yellow))));
    assert!(s.filter.describe().contains("label red or yellow"));
    s.execute("library.filter", &serde_json::json!({"labels": []})).unwrap();
    assert!(s.visible_cloned().len() > want);
}

/// Quick Collection / target album: B toggles the selection in it (created on first use);
/// another album can be the target; clear empties it; all undoable.
#[test]
fn quick_collection_and_target_album() {
    let mut s = crate::Session::with_demo();
    let ids: Vec<u64> = s.catalog.photos().take(3).map(|p| p.id.0).collect();
    assert!(s.catalog.quick_collection().is_none());
    s.execute("library.select", &serde_json::json!({"ids": [ids[0], ids[1]]})).unwrap();
    let r = s.execute("album.toggleTarget", &serde_json::json!({})).unwrap();
    assert_eq!((r["added"].as_bool(), r["count"].as_u64(), r["name"].as_str()), (Some(true), Some(2), Some("Quick Collection")));
    let quick = s.catalog.quick_collection().expect("created");
    // again: both are in it, so they come out
    assert_eq!(s.execute("album.toggleTarget", &serde_json::json!({})).unwrap()["count"], 0);
    s.execute("edit.undo", &serde_json::json!({})).unwrap();
    assert_eq!(s.catalog.album_count(quick), 2);
    // a regular album as the target
    let alb = s.execute("album.create", &serde_json::json!({"name": "Picks"})).unwrap()["id"].as_u64().unwrap();
    s.execute("album.setTarget", &serde_json::json!({"id": alb})).unwrap();
    s.execute("album.toggleTarget", &serde_json::json!({"ids": [ids[2]]})).unwrap();
    assert_eq!(s.catalog.album_count(lightcraft_catalog::AlbumId(alb)), 1);
    assert_eq!(s.catalog.album_count(quick), 2, "the Quick Collection is untouched");
    let smart = s.execute("album.createSmart", &serde_json::json!({"name": "S", "rules": {"rating": 3}})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("album.setTarget", &serde_json::json!({"id": smart})).is_err());
    s.execute("album.setTarget", &serde_json::json!({"id": null})).unwrap();
    s.execute("album.clearQuick", &serde_json::json!({})).unwrap();
    assert_eq!(s.catalog.album_count(quick), 0);
}

/// Scaling check (ignored: `cargo test --release -p lightcraft-engine -- --ignored scale --nocapture`):
/// the per-frame / per-click library queries on a 100k-photo catalog.
#[test]
#[ignore]
fn scale_100k_library_queries() {
    use lightcraft_catalog::{Op, Photo, PhotoId, Source};
    use std::time::Instant;
    let mut s = crate::Session::new();
    let n = 100_000u64;
    let mut ops = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut p = Photo::new(
            PhotoId(i + 1),
            Source::Demo { scene: (i % 20) as u32 },
            &format!("IMG_{i:06}.jpg"),
            "JPEG",
            6000,
            4000,
            "2026-01-01T00:00:00",
        );
        p.captured = Some(format!("20{:02}-{:02}-{:02}T{:02}:00:00", 10 + i % 16, 1 + i % 12, 1 + i % 28, i % 24));
        p.rating = (i % 6) as u8;
        p.meta.keywords = vec![format!("kw{}", i % 500), "travel|italy".into()];
        p.meta.camera = format!("Model {}", i % 30);
        ops.push(Op::AddPhoto { photo: Box::new(p) });
    }
    let t = Instant::now();
    s.commit("Add", Op::Batch { ops }).unwrap();
    eprintln!("add 100k: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let time = |what: &str, f: &mut dyn FnMut()| {
        let t = Instant::now();
        f();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        eprintln!("{what}: {ms:.1} ms");
        ms
    };
    let mut worst: Vec<(String, f64)> = Vec::new();
    let mut m = |w: &str, f: &mut dyn FnMut()| worst.push((w.to_string(), time(w, f)));
    m("visible (all)", &mut || {
        let _ = s.visible_cloned();
    });
    m("visible (cached)", &mut || {
        let _ = s.visible_cloned();
    });
    m("filter rating ≥ 4", &mut || {
        s.execute("library.filter", &serde_json::json!({"rating": 4})).unwrap();
        let _ = s.visible_cloned();
    });
    m("filter ruleSet keyword + date", &mut || {
        s.execute("library.filter", &serde_json::json!({"rating": 0, "ruleSet": {"rules": [{"field": "keywords", "op": "contains", "value": "kw42"}, {"field": "captureDate", "op": "is", "value": "2015"}]}})).unwrap();
        let _ = s.visible_cloned();
    });
    s.execute("library.filter", &serde_json::json!({"ruleSet": null})).unwrap();
    m("date groups", &mut || {
        let _ = s.execute("library.groups", &serde_json::json!({})).unwrap();
    });
    m("keyword tree", &mut || {
        let _ = s.execute("keyword.list", &serde_json::json!({})).unwrap();
    });
    m("keyword suggestions", &mut || {
        let _ = s.catalog.keyword_suggestions(&["kw1".to_string()], "kw", 12);
    });
    let alb = s.execute("album.createSmart", &serde_json::json!({"name": "S", "rules": {"rating": 5}})).unwrap()["id"].as_u64().unwrap();
    m("smart album count", &mut || {
        let _ = s.catalog.album_count(lightcraft_catalog::AlbumId(alb));
    });
    m("albums.list", &mut || {
        let _ = s.execute("albums.list", &serde_json::json!({})).unwrap();
    });
    let manual = lightcraft_catalog::AlbumId(s.execute("album.create", &serde_json::json!({"name": "Half"})).unwrap()["id"].as_u64().unwrap());
    let members: Vec<PhotoId> = (1..=n).rev().step_by(2).map(PhotoId).collect();
    s.commit("Add to Album", Op::SetAlbumPhotos { id: manual, photos: members }).unwrap();
    m("album 50k (manual order)", &mut || {
        s.source = crate::LibrarySource::Album(manual);
        let _ = s.visible_cloned();
    });
    m("albums_of × 10k", &mut || {
        for i in 1..=10_000 {
            let _ = s.catalog.albums_of(PhotoId(i));
        }
    });
    s.source = crate::LibrarySource::All;
    m("sort file name", &mut || {
        s.sort.key = lightcraft_catalog::SortKey::FileName;
        let _ = s.visible_cloned();
    });
    s.sort = Default::default();
    m("search text (2 tokens)", &mut || {
        s.execute("library.filter", &serde_json::json!({"text": "Italy MODEL"})).unwrap();
        let _ = s.visible_cloned();
    });
    s.execute("library.filter", &serde_json::json!({"text": ""})).unwrap();
    m("select all", &mut || {
        s.execute("library.selectAll", &serde_json::json!({})).unwrap();
    });
    m("rate 100k selected", &mut || {
        s.execute("photo.rate", &serde_json::json!({"rating": 3})).unwrap();
    });
    m("undo", &mut || {
        s.execute("edit.undo", &serde_json::json!({})).unwrap();
    });
    worst.sort_by(|a, b| b.1.total_cmp(&a.1));
    eprintln!("slowest: {:?}", &worst[..3]);
}

fn open_at(dir: &std::path::Path, now: &'static str) -> Session {
    let mut s = Session::new();
    s.clock = Box::new(move || now.to_string());
    s.open_library(dir, false).unwrap();
    s
}

/// Recently Deleted retention: with `purgeDeletedDays` set, opening the library removes photos
/// deleted that many days ago from the catalog (their files stay on disk) as one undo step.
/// Deletions without a recorded time (made by earlier builds) start counting at the first open
/// that sees them, so they are never purged then.
#[test]
fn recently_deleted_is_purged_after_the_retention_period() {
    use lightcraft_catalog::{AlbumId, Op, Photo, PhotoId, Source};
    let dir = temp_dir("purge-deleted");
    let originals = temp_dir("purge-deleted-originals");
    std::fs::create_dir_all(&originals).unwrap();
    let file = originals.join("a.jpg");
    std::fs::write(&file, b"original bytes").unwrap();

    let mut s = open_at(&dir, "2026-01-01T10:00:00");
    let source = |i: u64| if i == 1 { Source::File { path: file.to_string_lossy().to_string() } } else { Source::Demo { scene: i as u32 } };
    let ops = (1..=5).map(|i| Op::AddPhoto { photo: Box::new(Photo::new(PhotoId(i), source(i), "a.jpg", "JPEG", 4, 3, "2026-01-01T00:00:00")) });
    s.commit("Add", Op::Batch { ops: ops.collect() }).unwrap();
    let (a, b, c, d, e) = (PhotoId(1), PhotoId(2), PhotoId(3), PhotoId(4), PhotoId(5));
    s.selection = crate::Selection::single(e);
    let album = AlbumId(s.execute("album.create", &json!({"name": "Keep"})).unwrap()["id"].as_u64().unwrap());
    s.execute("album.addPhotos", &json!({"id": album.0, "ids": [a.0, e.0]})).unwrap();
    s.execute("photo.delete", &json!({"ids": [a.0, b.0]})).unwrap();
    assert_eq!(s.catalog.photo(a).unwrap().deleted_at.as_deref(), Some("2026-01-01T10:00:00"));
    s.clock = Box::new(|| "2026-01-20T10:00:00".to_string());
    s.execute("photo.delete", &json!({"ids": [c.0]})).unwrap();
    // deleting again keeps the time it went to Recently Deleted
    s.execute("photo.delete", &json!({"ids": [a.0]})).unwrap();
    assert_eq!(s.catalog.photo(a).unwrap().deleted_at.as_deref(), Some("2026-01-01T10:00:00"));
    // as a build before deletion times wrote it
    s.commit("Delete", Op::SetDeleted { id: d, deleted: true, deleted_at: None }).unwrap();
    s.persist().unwrap();
    // off by default: opening much later removes nothing and records nothing
    drop(s);
    let s = open_at(&dir, "2026-06-01T10:00:00");
    assert_eq!(s.purge_deleted_days, 0);
    assert!(s.undo.is_empty() && s.catalog.len() == 5);
    assert!(s.catalog.photo(d).unwrap().deleted_at.is_none());
    drop(s);

    let mut s = open_at(&dir, "2026-01-25T10:00:00");
    let r = s.execute("library.preferences", &json!({"purgeDeletedDays": 30})).unwrap();
    assert_eq!(r["purgeDeletedDays"], 30);
    assert_eq!(s.purge_deleted(None).unwrap(), vec![], "nothing is 30 days old yet");
    assert_eq!(s.catalog.photo(d).unwrap().deleted_at.as_deref(), Some("2026-01-25T10:00:00"), "unknown: counted from now");
    assert!(s.undo.is_empty(), "giving a time is not an undo step");
    drop(s);

    let mut s = open_at(&dir, "2026-02-05T10:00:00");
    assert_eq!(s.purge_deleted_days, 30, "the preference is saved with the library");
    assert!(s.catalog.photo(a).is_none() && s.catalog.photo(b).is_none(), "35 days in Recently Deleted");
    assert!(s.catalog.photo(c).is_some() && s.catalog.photo(d).is_some() && s.catalog.photo(e).is_some());
    assert_eq!(s.catalog.album(album).unwrap().photos, vec![e]);
    assert!(file.exists() && std::fs::read(&file).unwrap() == b"original bytes", "originals are never touched");
    assert_eq!(s.undo_entry().map(|u| u.label.as_str()), Some("Empty Recently Deleted"));
    s.execute("edit.undo", &json!({})).unwrap();
    let back = s.catalog.photo(a).unwrap();
    assert!(back.deleted && back.deleted_at.as_deref() == Some("2026-01-01T10:00:00"));
    assert_eq!(s.catalog.album(album).unwrap().photos, vec![a, e], "album membership comes back");
    assert!(s.catalog.album_has(album, a));
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(s.catalog.photo(a).is_none() && !s.catalog.album_has(album, a));
    s.execute("edit.undo", &json!({})).unwrap();
    // 0 = never, whatever the photos' age
    assert!(s.purge_deleted(Some(0)).unwrap().is_empty());
    drop(s);

    // the undo was saved: the next open purges them again, and only them
    let s = open_at(&dir, "2026-02-05T10:00:00");
    assert!(s.catalog.photo(a).is_none() && s.catalog.photo(b).is_none() && s.catalog.photo(c).is_some());
    assert!(file.exists());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&originals);
}

/// Issue #99: a library is open in one session at a time. A second opener is refused (nothing
/// read or written), the owning session can reopen it, and it is free again once closed.
#[test]
fn a_library_open_elsewhere_is_refused() {
    let dir = temp_dir("locked");
    let mut s = open(&dir, true);
    s.execute("photo.rate", &json!({"rating": 3})).unwrap();
    let log = std::fs::read(dir.join("catalog.log")).unwrap();

    let mut other = Session::new();
    let e = other.open_library(&dir, true).unwrap_err();
    assert!(matches!(e, crate::EngineError::LibraryInUse(_)), "{e:?}");
    assert!(e.to_string().contains("already open in"), "{e}");
    assert!(other.library.is_none());
    assert_eq!(std::fs::read(dir.join("catalog.log")).unwrap(), log, "the refused opener wrote nothing");

    // the session that has it open may reopen it (Settings → Open Library on the same folder)
    s.close_library().unwrap();
    s.open_library(&dir, true).unwrap();
    assert!(other.open_library(&dir, true).is_err(), "still locked after reopening");
    s.execute("photo.rate", &json!({"rating": 4})).unwrap();
    let expect = s.catalog.to_snapshot();

    drop(s);
    other.open_library(&dir, true).unwrap();
    assert_eq!(other.catalog.to_snapshot(), expect);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn library_identity_persists_and_copied_catalog_locations_remain_distinct() {
    let root = temp_dir("persistent-identity");
    let a = root.join("a");
    let b = root.join("b");
    let first = {
        let mut session = Session::new();
        session.open_library(&a, false).unwrap();
        session.library_identity().to_string()
    };
    let mut session = Session::new();
    session.open_library(&a, false).unwrap();
    assert_eq!(session.library_identity(), first);
    std::fs::create_dir_all(&b).unwrap();
    std::fs::copy(a.join("library-identity.json"), b.join("library-identity.json")).unwrap();
    session.open_library(&b, false).unwrap();
    assert_ne!(session.library_identity(), first, "a copied catalog must not share document bindings with its original location");
    let second = session.library_identity().to_owned();
    drop(session);
    let mut session = Session::new();
    session.open_library(&b, false).unwrap();
    assert_eq!(session.library_identity(), second);
    drop(session);
    std::fs::write(a.join("library-identity.json"), b"broken identity").unwrap();
    let mut session = Session::new();
    session.open_library(&a, false).unwrap();
    assert!(session.library_identity().starts_with("unavailable:"));
    assert!(!session.take_library_warnings().is_empty());
    assert_eq!(std::fs::read(a.join("library-identity.json")).unwrap(), b"broken identity");
    drop(session);
    assert_ne!(Session::new().library_identity(), Session::new().library_identity());
    std::fs::remove_dir_all(root).unwrap();
}
