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

#[test]
fn both_legacy_folder_layouts_load_and_combined_preset_roundtrips() {
    let tags: FolderDef = serde_json::from_value(json!({
        "name":"Tags", "custom":true, "tags":["Stage","Speakers"], "combineAll":true, "personIds":[7]
    }))
    .unwrap();
    assert!(tags.custom && tags.combine_all && tags.use_rules);
    assert!(!tags.people_enabled && !tags.everyone && !tags.people_or && !tags.unsorted);
    let people: FolderDef = serde_json::from_value(json!({
        "name":"People", "peopleEnabled":true, "personIds":[7,8], "everyone":true, "peopleOr":true, "useRules":false
    }))
    .unwrap();
    assert!(people.people_enabled && people.everyone && people.people_or);
    assert!(!people.use_rules && !people.custom && !people.combine_all && !people.unsorted);
    assert!(people.tags.is_empty());
    let preset: SortPreset = serde_json::from_value(json!({
        "name":"Combined", "firstMatch":true, "folders":[tags], "peopleLayout":[people], "folderPattern":"{event}/{folder}"
    }))
    .unwrap();
    let back: SortPreset = serde_json::from_value(serde_json::to_value(&preset).unwrap()).unwrap();
    assert_eq!(back, preset);
    assert!(back.first_match);
    assert_eq!(back.folders[0].tags, ["Stage", "Speakers"]);
    assert_eq!(back.people_layout[0].person_ids, [7, 8]);
    let old: SortPreset = serde_json::from_value(json!({"name":"Old", "peopleLayout":[people]})).unwrap();
    assert!(!old.first_match);
    assert!(old.folders.is_empty());
    assert_eq!(old.folder_pattern, tokens::DEFAULT_FOLDER_PATTERN);
    assert_eq!(old.people_layout, [people]);
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

#[test]
fn tag_any_all_use_max_min_and_each_tag_gets_exemplar_boost() {
    let t = mock::MockTagger;
    let mut store = Store::default();
    store.ensure(t.model_id(), 8).unwrap();
    let red = t.embed_image(&Rgba8::filled(1, 1, [255, 0, 0, 255])).unwrap();
    let mut preset = sort(&[("Mixed", "red"), ("Blue", "blue")], Sensitivity::Balanced);
    preset.categories[0].prompts.push("blue".into());
    let any = classify::Classifier::new(&t, &store, &preset).unwrap().classify("r", Some(&red), None).unwrap();
    assert_eq!(any.assigned, ["Mixed"]);
    preset.categories[0].match_all = true;
    let all = classify::Classifier::new(&t, &store, &preset).unwrap().classify("r", Some(&red), None).unwrap();
    assert!(all.scores["Mixed"] < any.scores["Mixed"]);
    for i in 0..10 {
        store.insert(t.model_id(), format!("e{i}"), red.clone()).unwrap();
        preset.categories[0].exemplars.push(format!("e{i}"));
    }
    let learned = classify::Classifier::new(&t, &store, &preset).unwrap().classify("r", Some(&red), None).unwrap();
    assert_eq!(learned.assigned, ["Mixed"]);
    let podium = t.embed_text("a photo of podium").unwrap();
    let preset = sort(&[("Podium", "podium"), ("Crowd", "crowd")], Sensitivity::Balanced);
    assert_eq!(classify::Classifier::new(&t, &store, &preset).unwrap().classify("podium", Some(&podium), None).unwrap().assigned, ["Podium"]);
}

#[test]
fn tag_sets_and_folder_layout_persist_with_old_defaults() {
    let dir = temp();
    {
        let mut s = demo();
        s.open_library(&dir, true).unwrap();
        s.execute("smartSort.saveTagSet", &json!({"name":"My speakers","tags":["podium","microphone","podium",""]})).unwrap();
        s.execute("smartSort.renameTagSet", &json!({"name":"My speakers","to":"Speakers set"})).unwrap();
        let mut preset = sort(&[("A", "red"), ("B", "blue")], Sensitivity::Loose);
        preset.name = "Saved layout".into();
        preset.first_match = true;
        preset.categories[0].match_all = true;
        preset.folders = plan::default_folders(&preset);
        preset.folders[0].enabled = false;
        preset.folders[1].name = "Blue photos".into();
        preset.folders.push(FolderDef {
            name: "Combined".into(),
            custom: true,
            tags: vec!["A".into(), "B".into()],
            person_ids: vec![42],
            rules: plan::tag_rules("Smart Sort", &["A".into(), "B".into()], false),
            ..Default::default()
        });
        s.execute("smartSort.savePreset", &json!({"preset":preset})).unwrap();
    }
    {
        let mut s = demo();
        s.open_library(&dir, false).unwrap();
        assert_eq!(s.smart.tag_sets[0].name, "Speakers set");
        assert_eq!(s.smart.tag_sets[0].tags, ["podium", "microphone"]);
        let saved = &s.smart.prefs.presets[0];
        assert!(saved.first_match && saved.categories[0].match_all);
        assert!(!saved.folders[0].enabled);
        assert_eq!(saved.folders[1].name, "Blue photos");
        assert_eq!(saved.folders[3].person_ids, [42]);
        s.execute("smartSort.deleteTagSet", &json!({"name":"Speakers set"})).unwrap();
        assert!(s.smart.tag_sets.is_empty());
        assert!(s.execute("smartSort.deleteTagSet", &json!({"name":"Conference / Speakers"})).is_err());
    }
    let old: SortPreset = serde_json::from_value(json!({"name":"Old","categories":[{"name":"A","prompts":["a person on stage"]}]})).unwrap();
    assert!(!old.first_match && old.folders.is_empty() && !old.categories[0].match_all);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn folder_export_overlap_unsorted_custom_rules_and_guards() {
    let mut s = demo();
    let ids: Vec<_> = s.catalog.photos().filter(|p| p.flag != Flag::Reject).map(|p| p.id).take(3).collect();
    let preset = sort(&[("A", "red"), ("B", "blue")], Sensitivity::Balanced);
    s.execute(
        "smartSort.applyKeywords",
        &json!({"preset":preset,"assignments":[{"id":ids[0],"categories":["A","B"]},{"id":ids[1],"categories":["A"]},{"id":ids[2],"categories":[]}]}),
    )
    .unwrap();
    let mut folders = plan::default_folders(&preset);
    folders[2].enabled = true;
    let dir = temp();
    for first in [false, true] {
        let root = dir.join(if first { "first" } else { "each" });
        let result =
            s.execute("smartSort.export", &json!({"dir":root,"ids":ids,"folders":folders,"firstMatch":first,"format":"png","longEdge":64})).unwrap();
        assert_eq!(result.as_array().unwrap().len(), if first { 3 } else { 4 });
        assert!(result.as_array().unwrap().iter().all(|r| r.get("path").is_some()), "{result}");
        assert_eq!(std::fs::read_dir(root.join("A")).unwrap().count(), 2);
        if !first {
            assert_eq!(std::fs::read_dir(root.join("B")).unwrap().count(), 1);
        }
        assert_eq!(std::fs::read_dir(root.join("Unsorted")).unwrap().count(), 1);
    }
    folders[0].enabled = false;
    folders.push(FolderDef {
        name: "Custom".into(),
        custom: true,
        tags: vec!["A".into(), "B".into()],
        rules: plan::tag_rules("Smart Sort", &["A".into(), "B".into()], false),
        ..Default::default()
    });
    let p = plan::plan(&s.catalog, &ids, &folders).unwrap();
    assert!(!p.iter().any(|f| f.name == "A"));
    assert_eq!(p.iter().find(|f| f.name == "Custom").unwrap().ids.len(), 2);
    folders[3].rules = plan::tag_rules("Smart Sort", &["A".into(), "B".into()], true);
    let p = plan::plan(&s.catalog, &ids, &folders).unwrap();
    assert_eq!(p.iter().find(|f| f.name == "Custom").unwrap().ids, [ids[0]]);
    folders[3].name = "../escape".into();
    assert!(plan::plan(&s.catalog, &ids, &folders).is_err());
    assert!(plan::sanitize("C:\\escape").is_err());
    let lib = dir.join("library");
    s.open_library(&lib, true).unwrap();
    assert!(plan::check_destination(&s, &lib.join("out").to_string_lossy()).is_err());
    assert!(plan::check_destination(&s, &lib.join("../library/out").to_string_lossy()).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&lib, dir.join("link")).unwrap();
        assert!(plan::check_destination(&s, &dir.join("link/out").to_string_lossy()).is_err());
    }
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn category_whitespace_keeps_keywords_and_folder_rules_consistent() {
    let mut s = demo();
    let id = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    let preset = sort(&[("  Red  ", "red")], Sensitivity::Loose);
    s.execute("smartSort.applyKeywords", &json!({"preset":preset,"assignments":[{"id":id,"categories":["  Red  "]}]})).unwrap();
    let folders = plan::keyword_folders(&preset);
    let planned = plan::plan(&s.catalog, &[id], &folders).unwrap();
    assert_eq!(planned[0].name, "Red");
    assert_eq!(planned[0].ids, [id]);
    let duplicate = sort(&[("Red", "red"), (" Red ", "blue")], Sensitivity::Loose);
    assert!(duplicate.validate().is_err());
}

#[test]
fn sensitivity_and_exemplar_edits_reuse_text_inference() {
    struct Counting(Arc<std::sync::atomic::AtomicUsize>);
    impl Tagger for Counting {
        fn model_id(&self) -> &str {
            "mock-tags"
        }
        fn dim(&self) -> usize {
            8
        }
        fn embed_image(&self, img: &Rgba8) -> Result<Vec<f32>, String> {
            mock::MockTagger.embed_image(img)
        }
        fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            mock::MockTagger.embed_text(text)
        }
    }
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let t = CachedTagger::new(Arc::new(Counting(calls.clone())));
    let mut store = Store::default();
    store.ensure(t.model_id(), t.dim()).unwrap();
    let mut preset = sort(&[("Red", "red")], Sensitivity::Balanced);
    classify::Classifier::new(&t, &store, &preset).unwrap();
    let before = calls.load(Ordering::Relaxed);
    preset.sensitivity = Sensitivity::Loose;
    store.insert(t.model_id(), "example".into(), t.embed_image(&Rgba8::filled(1, 1, [255, 0, 0, 255])).unwrap()).unwrap();
    preset.categories[0].exemplars.push("example".into());
    classify::Classifier::new(&t, &store, &preset).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), before);
    preset.categories[0].prompts.push("blue".into());
    classify::Classifier::new(&t, &store, &preset).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), before + 1);
}

fn p2b_photos(s: &mut Session, times: &[Option<&str>]) -> Vec<PhotoId> {
    times
        .iter()
        .enumerate()
        .map(|(index, time)| {
            let id = s.catalog.alloc_photo_id();
            let mut photo =
                lightcraft_catalog::Photo::new(id, Source::Demo { scene: 1 }, &format!("example-{index}.jpg"), "JPEG", 64, 48, "2026-10-09");
            photo.captured = time.map(str::to_string);
            photo.content_hash = Some(format!("p2b-{}", id.0));
            s.commit("test photo", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
            id
        })
        .collect()
}
#[test]
fn p2b_sessions_rename_preset_persistence_and_old_defaults() {
    let dir = temp();
    let mut s = demo();
    s.open_library(&dir, true).unwrap();
    let mut preset = sort(&[("A", "red")], Sensitivity::Balanced);
    preset.name = "Sessions saved".into();
    preset.sessions.enabled = true;
    preset.sessions.export_folders = true;
    preset.sessions.names.insert("2026-10-09T09:00:00".into(), "Morning keynote".into());
    preset.event_name = "Conference".into();
    preset.file_pattern = "{event}_{folder}_{seq:4}".into();
    preset.bursts.enabled = true;
    preset.bursts.strictness = bursts::BurstStrictness::Loose;
    preset.bursts.also_stack_in_library = true;
    preset.folders = plan::default_folders(&preset);
    preset.folders[0].session = Some("2026-10-09T09:00:00".into());
    s.execute("smartSort.savePreset", &json!({"preset":preset})).unwrap();
    s.close_library().unwrap();
    drop(s);
    let mut reloaded = demo();
    reloaded.open_library(&dir, false).unwrap();
    assert_eq!(reloaded.smart.prefs.presets[0], preset);
    let old: SortPreset = serde_json::from_value(json!({"name":"Old"})).unwrap();
    assert!(!old.sessions.enabled && !old.bursts.enabled);
    assert_eq!(old.folder_pattern, "{event}/{folder}");
    assert!(old.file_pattern.is_empty() && old.event_name.is_empty());
    drop(reloaded);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn p2b_sessions_commands_narrow_tag_custom_people_and_no_time_folders() {
    let mut s = demo();
    let ids = p2b_photos(&mut s, &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:10:00"), Some("2026-10-09T09:35:00"), None]);
    let mut preset = sort(&[("A", "red")], Sensitivity::Loose);
    preset.sessions.enabled = true;
    preset.sessions.names.insert("2026-10-09T09:35:00".into(), "Afternoon".into());
    let result = s.execute("smartSort.sessions", &json!({"preset":preset,"ids":ids})).unwrap();
    assert_eq!(result["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(result["sessions"][0]["photos"], json!(&ids[..2]));
    assert_eq!(result["sessions"][1]["name"], "Afternoon");
    assert_eq!(result["noTime"], json!([ids[3]]));
    let assignments: Vec<_> = ids.iter().map(|id| json!({"id":id,"categories":["A"]})).collect();
    s.execute("smartSort.applyKeywords", &json!({"preset":preset,"assignments":assignments})).unwrap();
    preset.folders = plan::default_folders(&preset);
    preset.folders[0].session = Some("2026-10-09T09:35:00".into());
    preset.folders.push(FolderDef {
        name: "Custom".into(),
        custom: true,
        rules: plan::tag_rules("Smart Sort", &["A".into()], false),
        session: Some(sessions::NO_TIME.into()),
        ..Default::default()
    });
    let result = s.execute("smartSort.plan", &json!({"sortPreset":preset,"ids":ids})).unwrap();
    assert_eq!(result["folders"][0]["ids"], json!([ids[2]]));
    assert_eq!(result["folders"][1]["ids"], json!([ids[3]]));
    preset.sessions.export_folders = true;
    preset.folders.clear();
    let result = s.execute("smartSort.plan", &json!({"sortPreset":preset,"ids":ids})).unwrap();
    assert_eq!(result["folders"][2]["name"], "No time");
    assert_eq!(result["folders"][2]["ids"], json!([ids[3]]));
    // A saved session restriction is dormant when sessions are disabled.
    preset.sessions.enabled = false;
    preset.folders = vec![FolderDef { name: "All".into(), session: Some("stale".into()), ..Default::default() }];
    assert_eq!(plan::plan_preset(&s.catalog, &ids, &preset).unwrap()[0].ids, ids);
}
#[test]
fn p2b_three_example_photos_define_category_without_tags() {
    let mut s = demo();
    let ids = p2b_photos(&mut s, &[None; 5]);
    s.smart.store.ensure("mock-tags", 8).unwrap();
    for (index, id) in ids.iter().enumerate() {
        let key = crate::media::content_key(s.catalog.photo(*id).unwrap());
        let direction = if index < 4 {
            mock::MockTagger.embed_image(&Rgba8::filled(16, 16, [255, 0, 0, 255])).unwrap()
        } else {
            mock::MockTagger.embed_text("blue").unwrap()
        };
        s.smart.store.insert("mock-tags", key, direction).unwrap();
    }
    let result = s
        .execute(
            "smartSort.folderFromExamples",
            &json!({"preset":sort(&[("Other","blue")],Sensitivity::Balanced),"ids":&ids[..3],"name":"Red examples"}),
        )
        .unwrap();
    let preset: SortPreset = serde_json::from_value(result["preset"].clone()).unwrap();
    assert!(preset.categories[1].prompts.is_empty());
    assert_eq!(preset.categories[1].exemplars.len(), 3);
    let result = s.execute("smartSort.classify", &json!({"preset":preset,"ids":ids})).unwrap();
    for p in &result["photos"].as_array().unwrap()[..4] {
        assert_eq!(p["assigned"], json!(["Red examples"]));
    }
    assert_eq!(result["photos"][4]["assigned"], json!(["Other"]));
    assert!(s.execute("smartSort.folderFromExamples", &json!({"ids":[]})).is_err());
}
#[test]
fn p2b_nested_export_tokens_and_duplicate_resolved_folders() {
    let mut s = demo();
    let ids = p2b_photos(&mut s, &[Some("2026-10-09T09:00:00")]);
    let mut preset = SortPreset {
        event_name: "Launch:day".into(),
        folder_pattern: "{event}/{date}".into(),
        file_pattern: "{folder}_{seq:4}".into(),
        ..Default::default()
    };
    preset.folders = vec![FolderDef { name: "A".into(), ..Default::default() }, FolderDef { name: "B".into(), ..Default::default() }];
    let dir = temp();
    let result = s.execute("smartSort.export", &json!({"sortPreset":preset,"ids":ids,"dir":dir,"format":"png","longEdge":32})).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
    assert!(dir.join("Launch_day/2026-10-09/A_0001.png").exists(), "{result}");
    assert!(dir.join("Launch_day/2026-10-09 (2)/B_0001.png").exists(), "{result}");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn p2b_bursts_commands_best_all_and_catalog_one_undo() {
    let mut s = demo();
    let ids =
        p2b_photos(&mut s, &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:00:01"), Some("2026-10-09T09:01:00"), Some("2026-10-09T09:01:01")]);
    for (index, id) in ids.iter().enumerate() {
        let key = insert(&mut s, *id, [255, 0, 0, 255]);
        s.smart.store.set_sharpness(key, index as f32);
    }
    let mut preset = SortPreset { folders: vec![FolderDef { name: "All".into(), ..Default::default() }], ..Default::default() };
    preset.bursts.enabled = true;
    let groups = s.execute("smartSort.bursts", &json!({"preset":preset,"ids":ids})).unwrap();
    assert_eq!(groups.as_array().unwrap().len(), 2);
    assert_eq!(groups[0]["best"], json!(ids[1]));
    assert_eq!(groups[1]["best"], json!(ids[3]));
    let best = s.execute("smartSort.plan", &json!({"sortPreset":preset,"ids":ids})).unwrap();
    assert_eq!(best["folders"][0]["ids"], json!([ids[1], ids[3]]));
    preset.bursts.export = bursts::BurstExport::All;
    let all = s.execute("smartSort.plan", &json!({"sortPreset":preset,"ids":ids})).unwrap();
    assert_eq!(all["folders"][0]["ids"], json!(ids));
    let undo = s.undo.len();
    let selection = s.selection.clone();
    let result = s.execute("smartSort.stackBursts", &json!({"preset":preset,"ids":ids})).unwrap();
    assert_eq!(result["stacks"], 2);
    assert_eq!(s.undo.len(), undo + 1);
    assert_eq!(s.selection, selection);
    assert_eq!(s.catalog.stacks().count(), 2);
    s.undo_step().unwrap();
    assert_eq!(s.catalog.stacks().count(), 0);
    s.redo_step().unwrap();
    assert_eq!(s.catalog.stacks().count(), 2);
}
#[test]
fn p2b_analysis_sharpness_cache_survives_reload() {
    let dir = temp();
    let mut store = Store::new(Some(&dir));
    store.ensure("mock", 2).unwrap();
    store.insert("mock", "key".into(), vec![1.0, 0.0]).unwrap();
    store.set_sharpness("key".into(), 12.0);
    store.save().unwrap();
    let mut reloaded = Store::new(Some(&dir));
    reloaded.ensure("mock", 2).unwrap();
    assert_eq!(reloaded.sharpness("key"), Some(12.0));
    assert_eq!(reloaded.get("mock", "key"), Some(&[1.0, 0.0][..]));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn p2b_missing_embedding_breaks_consecutive_burst() {
    let mut s = demo();
    let ids = p2b_photos(&mut s, &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:00:01"), Some("2026-10-09T09:00:02")]);
    insert(&mut s, ids[0], [255, 0, 0, 255]);
    insert(&mut s, ids[2], [255, 0, 0, 255]);
    assert_eq!(bursts::for_session(&mut s, &ids, bursts::BurstStrictness::Normal).unwrap().len(), 3);
}

#[test]
fn p2b_legacy_embeddings_gain_sharpness_without_model_inference() {
    struct CachedOnly;
    impl Tagger for CachedOnly {
        fn model_id(&self) -> &str {
            "mock-tags"
        }
        fn dim(&self) -> usize {
            8
        }
        fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
            mock::MockTagger.embed_text(text)
        }
        fn embed_image(&self, _: &Rgba8) -> Result<Vec<f32>, String> {
            Err("cached vectors must be reused".into())
        }
    }
    let mut s = demo();
    let id = s.catalog.photos().find(|p| p.flag != Flag::Reject).unwrap().id;
    let key = insert(&mut s, id, [255, 0, 0, 255]);
    let before = s.smart.store.get("mock-tags", &key).unwrap().to_vec();
    s.smart.tagger = Some(Arc::new(CachedOnly));
    let result = s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap();
    assert_eq!(result["analysed"], 1);
    assert_eq!(result["failed"], json!([]));
    assert!(s.smart.store.sharpness(&key).is_some());
    let after = s.smart.store.get("mock-tags", &key).unwrap();
    for (a, b) in before.iter().zip(after) {
        assert!((a - b).abs() < 1e-6);
    }
    assert_eq!(s.execute("smartSort.analyze", &json!({"ids":[id]})).unwrap()["skipped"], 1);
}
