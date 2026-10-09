use super::*;
use lightcraft_catalog::{Flag, Op, PhotoId, Source};
use lightcraft_raster::Rgba8;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

fn demo() -> Session {
    let mut s = Session::with_demo();
    s.smart.tagger = Some(Arc::new(mock::MockTagger));
    s
}
fn temp() -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("smart-sort-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn sort(prompts: &[(&str, &str)], sensitivity: Sensitivity) -> SortPreset {
    SortPreset {
        name: "Test".into(),
        categories: prompts
            .iter()
            .map(|(name, prompt)| Category { name: (*name).into(), prompts: vec![(*prompt).into()], ..Default::default() })
            .collect(),
        sensitivity,
        ..Default::default()
    }
}
fn insert(s: &mut Session, id: PhotoId, color: [u8; 4]) -> String {
    let t = s.smart.tagger.clone().unwrap();
    let key = crate::media::content_key(s.catalog.photo(id).unwrap());
    s.smart.store.ensure(t.model_id(), t.dim()).unwrap();
    s.smart.store.insert(t.model_id(), key.clone(), t.embed_image(&Rgba8::filled(16, 16, color)).unwrap()).unwrap();
    key
}

#[test]
fn analysis_is_idempotent_and_virtual_copies_share_embeddings() {
    let mut s = demo();
    let ids: Vec<_> = s.catalog.photos().filter(|p| p.flag != Flag::Reject).map(|p| p.id).take(3).collect();
    let mut copy = s.catalog.photo(ids[0]).unwrap().as_ref().clone();
    copy.id = s.catalog.alloc_photo_id();
    copy.copy_of = Some(ids[0]);
    let copy_id = copy.id;
    s.commit("copy", Op::AddPhoto { photo: Box::new(copy) }).unwrap();
    let mut all = ids.clone();
    all.push(copy_id);
    let params = json!({"ids":all});
    let r = s.execute("smartSort.analyze", &params).unwrap();
    assert_eq!(r["analysed"], 3);
    assert_eq!(r["skipped"], 1);
    assert_eq!(s.smart.store.len("mock-tags"), 3);
    let r = s.execute("smartSort.analyze", &params).unwrap();
    assert_eq!(r["analysed"], 0);
    assert_eq!(r["skipped"], 4);
    let p = sort(&[("Red", "red")], Sensitivity::Loose);
    let result =
        s.execute("smartSort.classify", &json!({"preset":p,"ids":[ids[0],copy_id],"overrides":[{"id":copy_id,"categories":["Red"]}]})).unwrap();
    assert_eq!(result["photos"].as_array().unwrap().len(), 2);
    assert_eq!(result["photos"][0]["manual"], false);
    assert_eq!(result["photos"][1]["manual"], true);
}

#[test]
fn rejected_videos_and_unreadable_inputs_are_handled() {
    let mut s = demo();
    let rejected = s.catalog.photos().find(|p| p.flag == Flag::Reject).unwrap().id;
    let other = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    assert_eq!(s.execute("smartSort.analyze", &json!({"ids":[rejected]})).unwrap()["analysed"], 0);
    assert_eq!(s.execute("smartSort.analyze", &json!({"ids":[rejected],"includeRejected":true})).unwrap()["analysed"], 1);
    let mut missing = s.catalog.photo(other).unwrap().as_ref().clone();
    missing.id = s.catalog.alloc_photo_id();
    missing.source = Source::File { path: "/nonexistent/smart-sort.jpg".into() };
    missing.content_hash = None;
    let missing_id = missing.id;
    s.commit("missing", Op::AddPhoto { photo: Box::new(missing) }).unwrap();
    let r = s.execute("smartSort.analyze", &json!({"ids":[missing_id]})).unwrap();
    assert_eq!(r["failed"].as_array().unwrap().len(), 1);
    let mut video = s.catalog.photo(other).unwrap().as_ref().clone();
    video.id = s.catalog.alloc_photo_id();
    video.kind = MediaKind::Video;
    let video_id = video.id;
    s.commit("video", Op::AddPhoto { photo: Box::new(video) }).unwrap();
    assert_eq!(s.execute("smartSort.analyze", &json!({"ids":[video_id]})).unwrap()["videos"], 1);
    assert!(s.execute("smartSort.analyze", &json!({"faces":true})).is_err());
}

#[test]
fn classify_red_unrelated_and_manual_overrides() {
    let mut s = demo();
    let ids: Vec<_> = s.catalog.photos().filter(|p| p.flag != Flag::Reject).take(2).map(|p| p.id).collect();
    insert(&mut s, ids[0], [255, 0, 0, 255]);
    insert(&mut s, ids[1], [0, 255, 0, 255]);
    let p = sort(&[("Red", "red"), ("Blue", "blue")], Sensitivity::Strict);
    let r = s.execute("smartSort.classify", &json!({"ids":ids,"preset":p})).unwrap();
    assert_eq!(r["photos"][0]["assigned"], json!(["Red"]));
    assert_eq!(r["photos"][1]["assigned"], json!([]));
    assert_eq!(r["counts"]["Unsorted"], 1);
    let r = s.execute("smartSort.classify", &json!({"ids":ids,"preset":p,"overrides":[{"id":ids[0],"categories":[]}]})).unwrap();
    assert_eq!(r["photos"][0]["assigned"], json!([]));
    assert_eq!(r["photos"][0]["manual"], true);
}

#[test]
fn classification_of_a_red_dominant_demo_scene() {
    let mut s = demo();
    let ids: Vec<_> = s.catalog.photos().filter(|p| p.flag != Flag::Reject).map(|p| p.id).collect();
    s.execute("smartSort.analyze", &json!({"ids":ids})).unwrap();
    let t = s.smart.tagger.clone().unwrap();
    let red = ids
        .iter()
        .find(|id| {
            let key = crate::media::content_key(s.catalog.photo(**id).unwrap());
            let v = s.smart.store.get(t.model_id(), &key).unwrap();
            v[0] > v[1] && v[0] > v[2]
        })
        .copied()
        .expect("demo has a red-dominant scene");
    let p = sort(&[("Red", "red"), ("Blue", "blue")], Sensitivity::Strict);
    let r = s.execute("smartSort.classify", &json!({"ids":[red],"preset":p})).unwrap();
    assert_eq!(r["photos"][0]["assigned"], json!(["Red"]));
}

#[test]
fn multi_and_ties_follow_category_order() {
    let t = mock::MockTagger;
    let mut store = Store::default();
    store.ensure(t.model_id(), 8).unwrap();
    let image = t.embed_image(&Rgba8::filled(1, 1, [255, 0, 0, 255])).unwrap();
    let mut p = sort(&[("First", "red"), ("Second", "red")], Sensitivity::Loose);
    p.multi = true;
    let c = classify::Classifier::new(&t, &store, &p).unwrap();
    assert_eq!(c.classify("key", Some(&image), None).unwrap().assigned, ["First", "Second"]);
    p.multi = false;
    let c = classify::Classifier::new(&t, &store, &p).unwrap();
    assert_eq!(c.classify("key", Some(&image), None).unwrap().assigned, ["First"]);
}

#[test]
fn exemplars_move_a_borderline_photo() {
    let t = mock::MockTagger;
    let mut store = Store::default();
    store.ensure(t.model_id(), 8).unwrap();
    let image = t.embed_image(&Rgba8::filled(1, 1, [100, 120, 0, 255])).unwrap();
    let mut p = sort(&[("Red", "red"), ("Green", "green")], Sensitivity::Balanced);
    let before = classify::Classifier::new(&t, &store, &p).unwrap().classify("borderline", Some(&image), None).unwrap();
    assert_eq!(before.assigned, ["Green"]);
    store.insert(t.model_id(), "example".into(), image.clone()).unwrap();
    p.categories[0].exemplars = vec!["example".into()];
    let after = classify::Classifier::new(&t, &store, &p).unwrap().classify("borderline", Some(&image), None).unwrap();
    assert_eq!(after.assigned, ["Red"]);
}

#[test]
fn keywords_are_one_undo_step_and_replace_only_descendants() {
    let mut s = demo();
    let ids: Vec<_> = s.catalog.photos().filter(|p| p.flag != Flag::Reject).take(2).map(|p| p.id).collect();
    let mut meta = s.catalog.photo(ids[0]).unwrap().meta.clone();
    meta.keywords.extend(["Smart Sort|Stale".into(), "Smart Sorter|Keep".into(), "Smart Sort".into(), "Travel|Keep".into()]);
    s.commit("seed", Op::SetMeta { id: ids[0], meta: Box::new(meta) }).unwrap();
    let before: Vec<_> = ids.iter().map(|id| s.catalog.photo(*id).unwrap().meta.keywords.clone()).collect();
    let undo = s.undo.len();
    let p = sort(&[("Red", "red")], Sensitivity::Balanced);
    let params = json!({"preset":p,"assignments":ids.iter().map(|id| json!({"id":id,"categories":["Red"]})).collect::<Vec<_>>()});
    assert_eq!(s.execute("smartSort.applyKeywords", &params).unwrap()["changed"], 2);
    assert_eq!(s.undo.len(), undo + 1);
    assert_eq!(s.undo.last().unwrap().label, "Smart Sort");
    let keys = &s.catalog.photo(ids[0]).unwrap().meta.keywords;
    for k in ["Smart Sort|Red", "Smart Sorter|Keep", "Smart Sort", "Travel|Keep"] {
        assert!(keys.contains(&k.to_string()));
    }
    assert!(!keys.contains(&"Smart Sort|Stale".into()));
    assert_eq!(s.execute("smartSort.applyKeywords", &params).unwrap()["changed"], 0);
    s.execute("edit.undo", &json!({})).unwrap();
    for (id, before) in ids.iter().zip(before) {
        assert_eq!(s.catalog.photo(*id).unwrap().meta.keywords, before);
    }
    let mut append = params;
    append["replace"] = false.into();
    s.execute("smartSort.applyKeywords", &append).unwrap();
    assert!(s.catalog.photo(ids[0]).unwrap().meta.keywords.contains(&"Smart Sort|Stale".into()));
}

#[test]
fn invalid_assignments_are_atomic() {
    let mut s = demo();
    let id = s.catalog.photos().next().unwrap().id;
    let before = s.catalog.photo(id).unwrap().meta.clone();
    let p = sort(&[("Red", "red")], Sensitivity::Balanced);
    assert!(
        s.execute("smartSort.applyKeywords", &json!({"preset":p,"assignments":[{"id":id,"categories":["Red"]},{"id":99999,"categories":["Red"]}]}))
            .is_err()
    );
    assert_eq!(s.catalog.photo(id).unwrap().meta, before);
}

#[test]
fn store_and_presets_roundtrip_through_a_library() {
    let dir = temp();
    let mut s = Session::new();
    s.open_library(&dir, true).unwrap();
    s.smart.tagger = Some(Arc::new(mock::MockTagger));
    let id = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap();
    let key = crate::media::content_key(s.catalog.photo(id).unwrap());
    let before = s.smart.store.get("mock-tags", &key).unwrap().to_vec();
    let mut p = sort(&[("Red", "red")], Sensitivity::Balanced);
    p.name = "My Event".into();
    s.execute("smartSort.savePreset", &json!({"preset":p})).unwrap();
    s.close_library().unwrap();
    drop(s);
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    s.smart.tagger = Some(Arc::new(mock::MockTagger));
    assert_eq!(s.execute("smartSort.status", &json!({})).unwrap()["analysed"]["tags"], 1);
    assert_eq!(s.smart.store.get("mock-tags", &key), Some(before.as_slice()));
    assert_eq!(s.smart.prefs.presets, [p]);
    assert!(!s.smart.prefs.faces_enabled);
    assert_eq!(s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap()["analysed"], 0);
    s.execute("smartSort.deletePreset", &json!({"name":"My Event"})).unwrap();
    assert!(s.smart.prefs.presets.is_empty());
    s.close_library().unwrap();
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn old_prefs_and_model_isolation_and_corrupt_caches() {
    let dir = temp();
    std::fs::write(dir.join("prefs.json"), b"{\"cacheMb\":42}").unwrap();
    let mut s = Session::new();
    s.open_library(&dir, true).unwrap();
    assert_eq!(s.cache_mb, 42);
    assert!(s.smart.prefs.presets.is_empty());
    assert!(!s.smart.prefs.faces_enabled);
    let path = dir.join("AI/embeddings-mock-tags.bin");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for bytes in [b"broken".as_slice(), b"LIEMB1 {\"model\":\"mock-tags\",\"dim\":8}\n\x03\x00key\x00"] {
        std::fs::write(&path, bytes).unwrap();
        let mut cache = Store::new(Some(&dir));
        cache.ensure("mock-tags", 8).unwrap();
        assert_eq!(cache.len("mock-tags"), 0);
        cache.insert("mock-tags", "key".into(), vec![1.0; 8]).unwrap();
        cache.save().unwrap();
        let mut loaded = Store::new(Some(&dir));
        loaded.ensure("mock-tags", 8).unwrap();
        assert_eq!(loaded.len("mock-tags"), 1);
    }
    s.smart.store.ensure("another-model", 8).unwrap();
    assert_eq!(s.smart.store.len("another-model"), 0);
    assert!(s.smart.store.ensure("../../escape", 8).is_err());
    s.close_library().unwrap();
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn store_appends_atomically_and_retries_a_failed_batch() {
    let dir = temp();
    let mut cache = Store::new(Some(&dir));
    cache.ensure("mock-tags", 8).unwrap();
    cache.insert("mock-tags", "one".into(), vec![1.0; 8]).unwrap();
    cache.save().unwrap();
    let path = dir.join("AI/embeddings-mock-tags.bin");
    let before = std::fs::read(&path).unwrap();
    cache.insert("mock-tags", "two".into(), vec![2.0; 8]).unwrap();
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(cache.save().is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    cache.save().unwrap();
    let after = std::fs::read(&path).unwrap();
    assert!(after.starts_with(&before));
    let mut loaded = Store::new(Some(&dir));
    loaded.ensure("mock-tags", 8).unwrap();
    assert_eq!(loaded.len("mock-tags"), 2);
    cache.insert("mock-tags", "one".into(), vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).unwrap();
    cache.save().unwrap();
    let mut loaded = Store::new(Some(&dir));
    loaded.ensure("mock-tags", 8).unwrap();
    assert_eq!(loaded.get("mock-tags", "one").unwrap()[1], 1.0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancelled_analysis_keeps_partial_cache_and_resumes() {
    let dir = temp();
    let mut s = demo();
    s.smart.store = Store::new(Some(&dir));
    let base = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().as_ref().clone();
    let mut ids = Vec::new();
    for n in 0..35 {
        let mut p = base.clone();
        p.id = s.catalog.alloc_photo_id();
        p.content_hash = Some(format!("batch-{n}"));
        ids.push(p.id);
        s.commit("copy", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    let cancel = AtomicBool::new(false);
    let r = analyze(&mut s, &ids, &cancel, &|_, _| cancel.store(true, Ordering::Relaxed)).unwrap();
    assert!(r.cancelled);
    assert_eq!(r.analysed, 32);
    let mut loaded = Store::new(Some(&dir));
    loaded.ensure("mock-tags", 8).unwrap();
    assert_eq!(loaded.len("mock-tags"), 32);
    cancel.store(false, Ordering::Relaxed);
    let r = analyze(&mut s, &ids, &cancel, &|_, _| {}).unwrap();
    assert_eq!(r.analysed, 3);
    assert_eq!(r.skipped, 32);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plans_use_whole_keywords_case_insensitively_and_clean_names() {
    let mut s = demo();
    let id = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    let p = sort(&[("Red", "red")], Sensitivity::Balanced);
    s.execute("smartSort.applyKeywords", &json!({"preset":p,"assignments":[{"id":id,"categories":["Red"]}]})).unwrap();
    let mut folders = plan::keyword_folders(&p);
    folders[0].name = " .Red:*? ".into();
    let mut duplicate = folders[0].clone();
    duplicate.name = " .red:*? ".into();
    folders.push(duplicate);
    let mut prefix = folders[0].clone();
    prefix.name = "Prefix".into();
    prefix.rules.rules = vec![lightcraft_catalog::Rule::Field { field: "keywords".into(), op: "is".into(), value: "smart sort|re".into() }];
    folders.push(prefix);
    let r = s.execute("smartSort.plan", &json!({"folders":folders,"ids":[id]})).unwrap();
    assert_eq!(r["folders"][0]["name"], "Red___");
    assert_eq!(r["folders"][1]["name"], "red___ 2");
    assert_eq!(r["folders"][0]["ids"], json!([id]));
    assert_eq!(r["folders"][1]["ids"], json!([id]));
    assert_eq!(r["folders"][2]["ids"], json!([]));
    for name in ["../escape", "a/../b", "/absolute", "\\absolute"] {
        assert!(plan::sanitize(name).is_err());
    }
    assert_eq!(plan::sanitize(" . ").unwrap(), "Untitled");
    assert_eq!(plan::sanitize("Parent/Child:?").unwrap(), "Parent/Child__");
}

#[test]
fn prompts_defaults_and_empty_categories() {
    assert_eq!(classify::prompt("red"), "a photo of red");
    assert_eq!(classify::prompt("an audience"), "an audience");
    assert_eq!(classify::prompt("the stage"), "the stage");
    assert_eq!(classify::prompt("photo of a cat"), "photo of a cat");
    let p: SortPreset = serde_json::from_value(json!({"name":"Old"})).unwrap();
    assert_eq!(p.keyword_parent, "Smart Sort");
    assert_eq!(p.sensitivity, Sensitivity::Balanced);
    for p in builtin_presets() {
        p.validate().unwrap();
    }
    let t = mock::MockTagger;
    let p = builtin_presets().pop().unwrap();
    let c = classify::Classifier::new(&t, &Store::default(), &p).unwrap();
    let image = t.embed_image(&Rgba8::filled(1, 1, [255, 0, 0, 255])).unwrap();
    assert!(c.classify("key", Some(&image), None).unwrap().assigned.is_empty());
    let r = demo().execute("smartSort.presets", &Value::Null).unwrap();
    assert_eq!(r.as_array().unwrap().len(), 4);
}

#[test]
fn inputs_prefer_embedded_raw_and_support_offline_smart_previews() {
    let mut s = demo();
    let base = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().as_ref().clone();
    let mut raw = base.clone();
    raw.id = s.catalog.alloc_photo_id();
    raw.kind = MediaKind::Raw;
    raw.source = Source::File { path: "/nonexistent/smart-sort.arw".into() };
    raw.develop = Arc::new(raw.camera_defaults());
    let raw_id = raw.id;
    s.commit("raw", Op::AddPhoto { photo: Box::new(raw) }).unwrap();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    s.media.preview_loader = Some(Arc::new(move |_, edge| {
        assert_eq!(edge, 1024);
        seen.fetch_add(1, Ordering::Relaxed);
        Some(Rgba8::filled(16, 8, [255, 0, 0, 255]))
    }));
    let img = prepare_inputs(&mut s, &[raw_id]).pop().unwrap().run().unwrap();
    assert_eq!((img.width, img.height), (16, 8));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    s.media.preview_loader = None;
    let dir = temp();
    s.media.smart_dir = Some(dir.clone());
    let mut photo = base;
    photo.id = s.catalog.alloc_photo_id();
    photo.source = Source::File { path: "/nonexistent/smart-sort-offline.jpg".into() };
    photo.content_hash = None;
    photo.width = 32;
    photo.height = 16;
    let id = photo.id;
    let file = crate::smart::file_name(&photo);
    let bytes = crate::smart::encode(&lightcraft_raster::Rgb32f::filled(32, 16, [0.8, 0.1, 0.05]), None).unwrap();
    std::fs::write(dir.join(file), bytes).unwrap();
    s.commit("offline", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    let r = s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap();
    assert_eq!(r["analysed"], 1);
    assert!(r["failed"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn inputs_fall_back_to_a_cached_thumbnail_when_original_is_offline() {
    let mut s = demo();
    let mut photo = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().as_ref().clone();
    photo.id = s.catalog.alloc_photo_id();
    photo.source = Source::File { path: "/nonexistent/smart-sort-cached.jpg".into() };
    photo.content_hash = None;
    let id = photo.id;
    s.commit("cached", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    let input = prepare_inputs(&mut s, &[id]).pop().unwrap();
    let (cache, key) = input.fallback.as_ref().unwrap().cache.as_ref().unwrap();
    cache.put(*key, Arc::new(Rgba8::filled(32, 16, [0, 0, 255, 255])));
    let img = input.run().unwrap();
    assert_eq!(img.data[0], [0, 0, 255, 255]);
}

#[test]
fn analysis_retries_a_failed_save_even_when_every_photo_is_cached() {
    let dir = temp();
    let mut s = demo();
    s.smart.store = Store::new(Some(&dir));
    let id = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(s.execute("smartSort.analyze", &json!({"ids":[id]})).is_err());
    }
    assert_eq!(s.smart.store.len("mock-tags"), 1);
    let result = s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap();
    assert_eq!(result["analysed"], 0);
    assert_eq!(result["skipped"], 1);
    let mut loaded = Store::new(Some(&dir));
    loaded.ensure("mock-tags", 8).unwrap();
    assert_eq!(loaded.len("mock-tags"), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
