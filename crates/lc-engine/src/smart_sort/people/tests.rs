use super::*;
use crate::smart_sort::{Category, SortPreset, Tagger, mock::MockTagger};
use lightcraft_catalog::{MediaKind, Photo, Rule, RuleSet, Source};
use lightcraft_meta::{Rect, Region, RegionKind};
use serde_json::{Value, json};
use std::sync::atomic::AtomicUsize;

fn temp() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("people-engine-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn image(colors: &[[u8; 3]]) -> Rgba8 {
    let mut image = Rgba8::filled(16 * colors.len().max(1), 16, [0, 0, 0, 255]);
    for (i, color) in colors.iter().enumerate() {
        for y in 0..8 {
            for x in 0..8 {
                image.set(i * 16 + x, y, [255; 4]);
                image.set(i * 16 + x + 8, y, [color[0], color[1], color[2], 255]);
            }
        }
    }
    image
}
fn session() -> Session {
    let mut s = Session::new();
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    s
}
fn add(s: &mut Session, colors: &[[u8; 3]]) -> PhotoId {
    let id = s.catalog.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("{}.jpg", id.0), "JPEG", 1600, 1000, "2026-10-09T10:00:00");
    p.content_hash = Some(format!("face-photo-{}", id.0));
    p.captured = Some(format!("2026-10-09T10:{:02}:00", id.0 % 60));
    let key = crate::media::content_key(&p);
    s.commit("fixture", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    // Fixture injection still respects the opt-in boundary; real analysis is tested below.
    s.smart.prefs.faces_enabled = true;
    let model = s.smart.people.ready().unwrap();
    s.smart.people.store.insert(&model, key, MockFaces.faces(&image(colors)).unwrap()).unwrap();
    id
}
fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap()
}
fn pick(s: &mut Session, photo: PhotoId, name: &str) -> u64 {
    run(s, "smartSort.findPerson", json!({"name":name,"faces":[{"photo":photo,"face":0}]}))["person"].as_u64().unwrap()
}
fn list(s: &mut Session) -> Value {
    run(s, "smartSort.people", json!({"showEveryone":true}))
}
fn photo_ids(v: &Value) -> Vec<PhotoId> {
    v["matches"].as_array().unwrap().iter().map(|r| serde_json::from_value(r["photo"].clone()).unwrap()).collect()
}

#[test]
fn opt_in_default_and_command_registry_privacy() {
    let mut s = session();
    let status = run(&mut s, "smartSort.status", json!({}));
    assert_eq!(status["faces"]["enabled"], false);
    assert_eq!(status["analysed"]["faces"], 0);
    assert!(s.execute("smartSort.facesAnalyze", &json!({})).is_err());
    assert!(s.execute("smartSort.facesInPhoto", &json!({"photo":1})).is_err());
    assert_eq!(s.smart.people.store.len("mock-faces"), 0);
    let commands = serde_json::to_value(s.commands()).unwrap().to_string();
    for id in [
        "facesEnable",
        "facesAnalyze",
        "facesInPhoto",
        "people",
        "findPerson",
        "confirmFace",
        "rejectFace",
        "mergePeople",
        "splitPerson",
        "peopleSeedHeadshots",
        "clearFaceData",
    ] {
        assert!(commands.contains(&format!("smartSort.{id}")));
    }
    run(&mut s, "smartSort.facesEnable", json!({"enabled":true}));
    let a = add(&mut s, &[[255, 0, 0]]);
    let r = run(&mut s, "smartSort.facesInPhoto", json!({"photo":a}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 1);
    assert!(!r.to_string().contains("embedding"));
    assert!(!r.to_string().contains("centroid"));
}

#[test]
fn deterministic_clusters_counts_covers_hidden_pin_and_ignore() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[255, 0, 0]]);
    add(&mut s, &[[255, 0, 0]]);
    let d = add(&mut s, &[[0, 255, 0]]);
    add(&mut s, &[[0, 255, 0]]);
    let v = run(&mut s, "smartSort.people", json!({}));
    assert_eq!(v["people"].as_array().unwrap().len(), 1);
    assert_eq!(v["hidden"], 1);
    let all = list(&mut s);
    assert_eq!(all["people"][0]["count"], 3);
    assert_eq!(all["people"][1]["count"], 2);
    let red = all["people"][0]["id"].as_u64().unwrap();
    let green = all["people"][1]["id"].as_u64().unwrap();
    assert_eq!(all["people"][0]["cover"]["photo"], json!(a));
    assert_eq!(all["people"][0]["folderEnabled"], false);
    let repeat = list(&mut s);
    assert_eq!(repeat, all);
    let counts = run(&mut s, "smartSort.people", json!({"showEveryone":true,"folderIds":[a,d]}));
    assert_eq!(counts["people"][0]["folderCount"], 1);
    run(&mut s, "smartSort.peoplePin", json!({"person":green}));
    assert_eq!(list(&mut s)["people"][0]["id"], green);
    run(&mut s, "smartSort.peopleIgnore", json!({"person":red}));
    let v = list(&mut s);
    assert_eq!(v["people"].as_array().unwrap().len(), 1);
    assert_eq!(v["ignored"][0]["id"], red);
    run(&mut s, "smartSort.peopleIgnore", json!({"person":red,"enabled":false}));
    assert_eq!(list(&mut s)["people"].as_array().unwrap().len(), 2);
    let limited = run(&mut s, "smartSort.people", json!({"showEveryone":true,"limit":1}));
    assert_eq!(limited["remaining"], 1);
    assert!(s.execute("smartSort.people", &json!({"minPhotos":11})).is_err());
    let (mut clone, _) = ordered_fixture();
    let original = list(&mut clone);
    clone.smart.people.data = PeopleData { model: "mock-faces".into(), ..Default::default() };
    let reordered = list(&mut clone);
    assert_eq!(reordered["people"], original["people"]);
    assert_ne!(a, b);
}
fn ordered_fixture() -> (Session, Vec<PhotoId>) {
    let mut s = session();
    let ids = vec![add(&mut s, &[[255, 0, 0]]), add(&mut s, &[[0, 255, 0]]), add(&mut s, &[[255, 0, 0]]), add(&mut s, &[[0, 255, 0]])];
    (s, ids)
}

#[test]
fn single_photo_people_are_hidden_by_default_and_can_be_shown_and_picked() {
    let mut s = session();
    let photo = add(&mut s, &[[0, 0, 255]]);
    let hidden = run(&mut s, "smartSort.people", json!({}));
    assert!(hidden["people"].as_array().unwrap().is_empty());
    assert_eq!(hidden["hidden"], 1);
    let everyone = list(&mut s);
    let id = everyone["people"][0]["id"].as_u64().unwrap();
    assert_eq!(everyone["people"][0]["count"], 1);
    assert_eq!(everyone["people"][0]["cover"]["photo"], json!(photo));
    let minimum = run(&mut s, "smartSort.people", json!({"minPhotos":1}));
    assert_eq!(minimum["people"], everyone["people"]);
    assert_eq!(pick(&mut s, photo, "Blue"), id);
    assert_eq!(s.smart.people.data.people.len(), 1);
}

#[test]
fn naming_is_one_undo_step_and_catalog_people_and_rules_work() {
    let (mut s, ids) = ordered_fixture();
    let bubbles = list(&mut s);
    let person = bubbles["people"][0]["id"].as_u64().unwrap();
    let before = s.smart.people.data.clone();
    let undo = s.undo.len();
    run(&mut s, "smartSort.namePerson", json!({"cluster":person,"name":"Jane Doe"}));
    assert_eq!(s.undo.len(), undo + 1);
    let named = s.catalog.people();
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].name, "Jane Doe");
    assert_eq!(named[0].count, 2);
    let rule = Rule::Field { field: "person".into(), op: "is".into(), value: json!("jane doe") };
    let idrule = Rule::Field { field: "person".into(), op: "is".into(), value: json!([person]) };
    assert!(rule.matches(s.catalog.photo(ids[0]).unwrap(), &s.catalog));
    assert!(idrule.matches(s.catalog.photo(ids[0]).unwrap(), &s.catalog));
    assert!(!idrule.matches(s.catalog.photo(ids[1]).unwrap(), &s.catalog));
    assert!(s.catalog.photo(ids[0]).unwrap().meta.regions[0].auto);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.smart.people.data, before);
    assert!(s.catalog.people().is_empty());
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.catalog.people()[0].name, "Jane Doe");
}

#[test]
fn finding_a_face_locates_existing_bubble_and_reject_never_returns() {
    let (mut s, ids) = ordered_fixture();
    list(&mut s);
    let person = pick(&mut s, ids[0], "Jane");
    let r = run(&mut s, "smartSort.findPerson", json!({"person":person}));
    assert_eq!(photo_ids(&r), [ids[0], ids[2]]);
    assert_eq!(s.smart.people.data.people.len(), 2);
    run(&mut s, "smartSort.rejectFace", json!({"person":person,"photo":ids[2],"face":0}));
    run(&mut s, "smartSort.confirmFace", json!({"person":person,"photo":ids[0],"face":0}));
    let r = run(&mut s, "smartSort.findPerson", json!({"person":person}));
    assert_eq!(photo_ids(&r), [ids[0]]);
    assert!(s.execute("smartSort.confirmFace", &json!({"person":person,"photo":ids[2],"face":0})).is_err());
    assert!(!s.catalog.photo(ids[2]).unwrap().meta.person_ids.contains(&person));
}

#[test]
fn uncertain_queue_least_sure_first_confirm_reranks_and_gates_folders() {
    let mut s = session();
    let anchor = add(&mut s, &[[255, 0, 0]]);
    let weak = add(&mut s, &[[128, 222, 0]]);
    let strong = add(&mut s, &[[255, 128, 0]]);
    let stranger = add(&mut s, &[[0, 0, 255]]);
    let person = pick(&mut s, anchor, "Jane");
    let r = run(&mut s, "smartSort.findPerson", json!({"person":person,"queue":true}));
    assert_eq!(photo_ids(&r), [weak, strong]);
    assert_eq!(r["matches"][0]["sure"], false);
    assert!(!s.catalog.photo(weak).unwrap().meta.person_ids.contains(&person));
    assert!(!photo_ids(&r).contains(&stranger));
    let before = s.smart.people.data.person(person).unwrap().centroid.clone();
    run(&mut s, "smartSort.confirmFace", json!({"person":person,"photo":weak,"face":0}));
    assert!(s.catalog.photo(weak).unwrap().meta.person_ids.contains(&person));
    assert_ne!(before, s.smart.people.data.person(person).unwrap().centroid);
    let r = run(&mut s, "smartSort.findPerson", json!({"person":person,"queue":true}));
    assert!(!photo_ids(&r).contains(&weak));
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.catalog.photo(weak).unwrap().meta.person_ids.contains(&person));
}

#[test]
fn merge_split_suggestions_and_undo_restore_identity_and_regions() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[255, 0, 0]]);
    let weak = add(&mut s, &[[140, 213, 0]]);
    let first = pick(&mut s, a, "Jane");
    let second = run(&mut s, "smartSort.findPerson", json!({"name":"Other","faces":[{"photo":b,"face":0}]}))["person"].as_u64().unwrap();
    assert_ne!(first, second);
    let r = run(&mut s, "smartSort.findPerson", json!({"person":first,"queue":true}));
    assert_eq!(r["probablySame"][0]["person"], second);
    let before = s.smart.people.data.clone();
    run(&mut s, "smartSort.mergePeople", json!({"ids":[first,second]}));
    assert_eq!(s.smart.people.data.people.len(), 1);
    assert_eq!(s.catalog.people()[0].count, 2);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.smart.people.data, before);
    run(&mut s, "edit.redo", json!({}));
    let mut merged = s.smart.people.data.clone();
    let result = run(&mut s, "smartSort.splitPerson", json!({"person":first,"faces":[{"photo":b,"face":0}],"name":"John"}));
    let split = result["newPerson"].as_u64().unwrap();
    assert_eq!(s.catalog.photo(a).unwrap().meta.person_ids, [first]);
    assert_eq!(s.catalog.photo(b).unwrap().meta.person_ids, [split]);
    assert!(s.catalog.photo(weak).unwrap().meta.person_ids.is_empty(), "a split cannot confirm review-only matches");
    assert!(run(&mut s, "smartSort.findPerson", json!({"person":first,"queue":true}))["probablySame"].as_array().unwrap().is_empty());
    merged.next_id = s.smart.people.data.next_id;
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.smart.people.data, merged);
    let result = run(&mut s, "smartSort.splitPerson", json!({"person":first,"faces":[{"photo":b,"face":0}]}));
    assert!(result["newPerson"].as_u64().unwrap() > split);
}

#[test]
fn xmp_regions_seed_names_but_preserve_imported_and_manual_regions() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[255, 0, 0]]);
    let mut meta = s.catalog.photo(a).unwrap().meta.clone();
    let rect = s.smart.people.store.get("mock-faces", &crate::media::content_key(s.catalog.photo(a).unwrap())).unwrap()[0].bounds();
    let imported = Region { rect, kind: RegionKind::Face, name: Some("Jane Doe".into()), description: Some("imported".into()), auto: false };
    meta.regions.push(imported.clone());
    meta.regions.push(Region {
        rect: Rect::new(0.8, 0.8, 0.9, 0.9),
        kind: RegionKind::Face,
        name: Some("Nonoverlap".into()),
        description: None,
        auto: false,
    });
    s.commit("XMP", Op::SetMeta { id: a, meta: Box::new(meta) }).unwrap();
    list(&mut s);
    assert_eq!(s.smart.people.data.people.len(), 1);
    let p = &s.smart.people.data.people[0];
    assert_eq!(p.name, "Jane Doe");
    assert_eq!(p.confirmed_faces.len(), 1);
    assert!(s.catalog.photo(a).unwrap().meta.regions.contains(&imported));
    assert_eq!(s.catalog.photo(b).unwrap().meta.regions[0].name.as_deref(), Some("Jane Doe"));
}

#[test]
fn people_folders_any_everyone_tag_and_or_toggle_and_unknown_notice() {
    let mut s = session();
    let jane = add(&mut s, &[[255, 0, 0]]);
    let john = add(&mut s, &[[0, 255, 0]]);
    let both = add(&mut s, &[[255, 0, 0], [0, 255, 0]]);
    let a = pick(&mut s, jane, "Jane");
    let b = pick(&mut s, john, "John");
    let result = run(&mut s, "smartSort.personFolder", json!({"person":a,"enabled":false}));
    assert!(result["folders"].as_array().unwrap().is_empty());
    let result = run(&mut s, "smartSort.personFolder", json!({"person":a,"enabled":true}));
    assert_eq!(result["folders"].as_array().unwrap().len(), 1);
    let folder =
        crate::smart_sort::FolderDef { name: "People".into(), people_enabled: true, person_ids: vec![a, b], use_rules: false, ..Default::default() };
    let mut folders = vec![folder.clone()];
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([jane, john, both]));
    folders[0].everyone = true;
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([both]));
    let mut meta = s.catalog.photo(john).unwrap().meta.clone();
    meta.keywords.push("Speakers".into());
    s.commit("tag", Op::SetMeta { id: john, meta: Box::new(meta) }).unwrap();
    folders[0].everyone = false;
    folders[0].person_ids = vec![a];
    folders[0].use_rules = true;
    folders[0].rules =
        RuleSet { rules: vec![Rule::Field { field: "keywords".into(), op: "is".into(), value: json!("Speakers") }], ..Default::default() };
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([]));
    folders[0].people_or = true;
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([jane, john, both]));
    folders[0].rules = RuleSet::default();
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([jane, both]));
    folders[0].person_ids.clear();
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":[jane,john,both]}));
    assert_eq!(r["folders"][0]["ids"], json!([]));
    let mut unknown = folder;
    unknown.person_ids = vec![999];
    let r = run(&mut s, "smartSort.plan", json!({"folders":[unknown],"ids":[jane]}));
    assert_eq!(r["folders"][0]["ids"], json!([]));
    assert_eq!(r["notices"].as_array().unwrap().len(), 1);
    run(&mut s, "smartSort.peopleIgnore", json!({"person":a}));
    let r = run(&mut s, "smartSort.personFolder", json!({"person":a,"enabled":true}));
    assert!(r["folders"].as_array().unwrap().is_empty());
}

#[test]
fn unnamed_bubbles_can_go_in_folders_and_preset_layout_roundtrips() {
    let (mut s, ids) = ordered_fixture();
    let v = list(&mut s);
    let person = v["people"][0]["id"].as_u64().unwrap();
    let folders = run(&mut s, "smartSort.personFolder", json!({"person":person,"enabled":true}))["folders"].clone();
    let r = run(&mut s, "smartSort.plan", json!({"folders":folders,"ids":ids}));
    assert_eq!(r["folders"][0]["ids"].as_array().unwrap().len(), 2);
    assert!(s.catalog.people().is_empty());
    let mut preset = SortPreset { name: "People layout".into(), ..Default::default() };
    preset.people_layout = serde_json::from_value(folders).unwrap();
    let back: SortPreset = serde_json::from_value(serde_json::to_value(&preset).unwrap()).unwrap();
    assert_eq!(back.people_layout[0].person_ids, [person]);
    assert!(back.people_layout[0].people_enabled);
}

#[test]
fn face_gates_unknown_counts_manual_override_and_zero_faces() {
    let tagger = MockTagger;
    let mut s = session();
    let id = add(&mut s, &[]);
    s.smart.tagger = Some(Arc::new(MockTagger));
    let key = crate::media::content_key(s.catalog.photo(id).unwrap());
    s.smart.store.ensure(tagger.model_id(), 8).unwrap();
    s.smart.store.insert(tagger.model_id(), key, tagger.embed_image(&Rgba8::filled(1, 1, [255, 0, 0, 255])).unwrap()).unwrap();
    let mut preset = SortPreset {
        categories: vec![Category { name: "Red".into(), prompts: vec!["red".into()], min_faces: Some(1), ..Default::default() }],
        ..Default::default()
    };
    let r = run(&mut s, "smartSort.classify", json!({"ids":[id],"preset":preset}));
    assert_eq!(r["photos"][0]["assigned"], json!([]));
    s.smart.prefs.faces_enabled = false;
    let r = run(&mut s, "smartSort.classify", json!({"ids":[id],"preset":preset}));
    assert_eq!(r["photos"][0]["assigned"], json!(["Red"]));
    s.smart.prefs.faces_enabled = true;
    preset.categories[0].max_faces = Some(0);
    let r = run(&mut s, "smartSort.classify", json!({"ids":[id],"preset":preset,"overrides":[{"id":id,"categories":["Red"]}]}));
    assert_eq!(r["photos"][0]["assigned"], json!(["Red"]));
}

#[test]
fn face_cache_atomic_append_failed_retry_corruption_and_no_faces_records() {
    let dir = temp();
    let mut store = FacesStore::new(Some(&dir));
    store.ensure("mock-faces").unwrap();
    store.insert("mock-faces", "a".into(), MockFaces.faces(&image(&[[255, 0, 0]])).unwrap()).unwrap();
    store.save().unwrap();
    let path = dir.join("AI/faces-mock-faces.bin");
    let before = std::fs::read(&path).unwrap();
    store.insert("mock-faces", "b".into(), vec![]).unwrap();
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(store.save().is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    store.save().unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(&before));
    let mut loaded = FacesStore::new(Some(&dir));
    loaded.ensure("mock-faces").unwrap();
    assert_eq!(loaded.len("mock-faces"), 2);
    assert_eq!(loaded.get("mock-faces", "b"), Some([].as_slice()));
    for bytes in [b"broken".as_slice(), b"LIFACE1 {\"model\":\"mock-faces\",\"dim\":128}\n\x01\x00a\x01\x00"] {
        std::fs::write(&path, bytes).unwrap();
        let mut cache = FacesStore::new(Some(&dir));
        cache.ensure("mock-faces").unwrap();
        assert_eq!(cache.len("mock-faces"), 0);
        cache.insert("mock-faces", "new".into(), vec![]).unwrap();
        cache.save().unwrap();
        let mut again = FacesStore::new(Some(&dir));
        again.ensure("mock-faces").unwrap();
        assert_eq!(again.len("mock-faces"), 1);
    }
    assert!(store.ensure("../escape").is_err());
    let mut bad = MockFaces.faces(&image(&[[255, 0, 0]])).unwrap();
    bad[0].score = f32::NAN;
    assert!(store.insert("mock-faces", "bad".into(), bad).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn people_json_rejections_flags_names_and_opt_in_persist_per_library() {
    let dir = temp();
    let mut s = session();
    s.open_library(&dir, false).unwrap();
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    run(&mut s, "smartSort.facesEnable", json!({"enabled":true}));
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[255, 0, 0]]);
    let id = pick(&mut s, a, "Jane");
    run(&mut s, "smartSort.rejectFace", json!({"person":id,"photo":b,"face":0}));
    run(&mut s, "smartSort.peoplePin", json!({"person":id}));
    run(&mut s, "smartSort.peoplePasteNames", json!({"text":"Jane,Speaker\nJohn\nJane,Other"}));
    s.smart.people.save().unwrap();
    let data = s.smart.people.data.clone();
    s.close_library().unwrap();
    drop(s);
    let mut s = Session::new();
    s.open_library(&dir, false).unwrap();
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    assert!(s.smart.prefs.faces_enabled);
    s.smart.people.ready().unwrap();
    assert_eq!(s.smart.people.data, data);
    let r = run(&mut s, "smartSort.findPerson", json!({"person":id}));
    assert_eq!(photo_ids(&r), [a]);
    assert_eq!(s.smart.people.data.name_suggestions.len(), 2);
    s.close_library().unwrap();
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn clear_deletes_all_versions_preserves_tags_and_only_removes_auto_regions() {
    let dir = temp();
    let mut s = session();
    s.smart.people = PeopleEngine::new(Some(&dir));
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    let a = add(&mut s, &[[255, 0, 0]]);
    pick(&mut s, a, "Jane");
    let mut meta = s.catalog.photo(a).unwrap().meta.clone();
    let manual = Region { rect: Rect::UNIT, kind: RegionKind::Face, name: Some("Manual".into()), description: None, auto: false };
    meta.regions.push(manual.clone());
    s.commit("manual", Op::SetMeta { id: a, meta: Box::new(meta) }).unwrap();
    s.smart.people.save().unwrap();
    std::fs::write(dir.join("AI/faces-old.bin"), b"old").unwrap();
    std::fs::write(dir.join("AI/embeddings-tags.bin"), b"tags").unwrap();
    run(&mut s, "smartSort.facesEnable", json!({"enabled":false}));
    run(&mut s, "smartSort.clearFaceData", json!({"removeNames":true}));
    assert!(!dir.join("AI/faces-mock-faces.bin").exists());
    assert!(!dir.join("AI/faces-old.bin").exists());
    assert!(!dir.join("AI/people.json").exists());
    assert!(dir.join("AI/embeddings-tags.bin").exists());
    assert_eq!(s.catalog.photo(a).unwrap().meta.regions, [manual]);
    assert!(s.smart.people.data.people.is_empty());
    assert!(s.undo.iter().all(|e| e.people.is_none()));
    run(&mut s, "edit.undo", json!({}));
    assert!(!dir.join("AI/people.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_people_json_recovers_and_failed_change_is_atomic() {
    let dir = temp();
    std::fs::create_dir_all(dir.join("AI")).unwrap();
    std::fs::write(dir.join("AI/people.json"), b"corrupt").unwrap();
    let mut s = session();
    s.smart.people = PeopleEngine::new(Some(&dir));
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    let a = add(&mut s, &[[255, 0, 0]]);
    assert!(s.smart.people.data.people.is_empty());
    let id = pick(&mut s, a, "Jane");
    let before = s.smart.people.data.clone();
    let meta = s.catalog.photo(a).unwrap().meta.clone();
    let undo = s.undo.len();
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(s.execute("smartSort.namePerson", &json!({"person":id,"name":"Changed"})).is_err());
    }
    assert_eq!(s.smart.people.data, before);
    assert_eq!(s.catalog.photo(a).unwrap().meta, meta);
    assert_eq!(s.undo.len(), undo);
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(s.execute("edit.undo", &json!({})).is_err());
    }
    assert_eq!(s.undo.len(), undo);
    assert_eq!(s.smart.people.data, before);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn real_analysis_jobs_cancel_resume_deduplicate_and_save_after_all_cached() {
    let dir = temp();
    let mut s = session();
    s.smart.people = PeopleEngine::new(Some(&dir));
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    s.smart.prefs.faces_enabled = true;
    let mut ids = Vec::new();
    for n in 0..35 {
        let id = s.catalog.alloc_photo_id();
        let mut p = Photo::new(id, Source::File { path: format!("/nonexistent/{n}.arw") }, "face.arw", "RAW", 1600, 1000, "2026-10-09");
        p.kind = MediaKind::Raw;
        p.develop = Arc::new(p.camera_defaults());
        p.content_hash = Some(format!("batch-{n}"));
        s.commit("raw", Op::AddPhoto { photo: Box::new(p) }).unwrap();
        ids.push(id);
    }
    s.media.preview_loader = Some(Arc::new(|_, edge| {
        assert_eq!(edge, 1024);
        Some(image(&[[255, 0, 0]]))
    }));
    let cancel = AtomicBool::new(false);
    let result = analyze(&mut s, &ids, &cancel, &|_, _| cancel.store(true, Ordering::Relaxed)).unwrap();
    assert!(result.cancelled);
    assert_eq!(result.analysed, 32);
    let mut cache = FacesStore::new(Some(&dir));
    cache.ensure("mock-faces").unwrap();
    assert_eq!(cache.len("mock-faces"), 32);
    cancel.store(false, Ordering::Relaxed);
    let result = analyze(&mut s, &ids, &cancel, &|_, _| {}).unwrap();
    assert_eq!(result.analysed, 3);
    assert_eq!(result.skipped, 32);
    let mut copy = s.catalog.photo(ids[0]).unwrap().as_ref().clone();
    copy.id = s.catalog.alloc_photo_id();
    copy.copy_of = Some(ids[0]);
    let copy_id = copy.id;
    s.commit("copy", Op::AddPhoto { photo: Box::new(copy) }).unwrap();
    assert_eq!(analyze(&mut s, &[copy_id], &cancel, &|_, _| {}).unwrap().skipped, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mock_headshots_seed_exactly_one_face_name_list_and_gallery_matches() {
    let dir = temp();
    for (name, color) in [("Jane", [255, 0, 0]), ("John", [0, 255, 0]), ("Ana", [0, 0, 255])] {
        let img = image(&[color]);
        let bytes = png(&img).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
    }
    let empty = png(&image(&[])).unwrap();
    std::fs::write(dir.join("empty.png"), empty).unwrap();
    let two = png(&image(&[[255, 0, 0], [0, 255, 0]])).unwrap();
    std::fs::write(dir.join("two.png"), two).unwrap();
    let mut s = session();
    s.smart.prefs.faces_enabled = true;
    let r = run(&mut s, "smartSort.peopleSeedHeadshots", json!({"dir":dir}));
    assert_eq!(r["people"].as_array().unwrap().len(), 3);
    assert_eq!(r["skipped"].as_array().unwrap().len(), 2);
    let bubbles = run(&mut s, "smartSort.people", json!({}));
    assert_eq!(bubbles["people"].as_array().unwrap().len(), 3);
    assert!(s.smart.people.data.people.iter().all(|p| p.seed_faces.len() == 1 && !p.folder_enabled));
    let jane = add(&mut s, &[[255, 0, 0]]);
    let john = add(&mut s, &[[0, 255, 0]]);
    let id = s.smart.people.data.people.iter().find(|p| p.name == "Jane").unwrap().id;
    let r = run(&mut s, "smartSort.findPerson", json!({"person":id}));
    assert_eq!(photo_ids(&r), [jane]);
    assert!(!photo_ids(&r).contains(&john));
    assert_eq!(run(&mut s, "smartSort.peopleSeedHeadshots", json!({"dir":dir}))["people"], json!([]));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "coordinator: set LI_SEG_TEST_MODELS to verified YuNet/SFace directory"]
fn real_weight_people_end_to_end() {
    let model_dir = PathBuf::from(std::env::var("LI_SEG_TEST_MODELS").expect("LI_SEG_TEST_MODELS"));
    let model = RealFaces(FaceModels::load(&model_dir).unwrap());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/upstream/lightcraft/images");
    let decode = |name: &str| lightcraft_codecs::decode_thumbnail(&std::fs::read(root.join(name)).unwrap(), 1024).unwrap().image;
    let mother = decode("ba-migrant-mother.jpg");
    let mut doubled = Rgba8::new(mother.width * 2, mother.height);
    for y in 0..mother.height {
        for x in 0..doubled.width {
            doubled.set(x, y, mother.get(x % mother.width, y));
        }
    }
    let faces = model.faces(&doubled).unwrap();
    assert!(faces.len() >= 2);
    assert!(faces.iter().all(|f| f.score >= 0.9));
    assert!(faces.iter().enumerate().any(|(i, a)| faces.iter().skip(i + 1).any(|b| similarity(&a.embedding, &b.embedding) >= 0.6)));
    assert!(model.faces(&decode("ba-tetons.jpg")).unwrap().is_empty());
    let mut s = Session::new();
    s.smart.prefs.faces_enabled = true;
    s.smart.people.tagger = Some(Arc::new(model));
    let mut ids = Vec::new();
    for n in 0..3 {
        let id = s.catalog.alloc_photo_id();
        let mut photo = Photo::new(id, Source::File { path: format!("/fixture/{n}.arw") }, "fixture.arw", "RAW", 1600, 1000, "2026-10-09");
        photo.kind = MediaKind::Raw;
        photo.develop = Arc::new(photo.camera_defaults());
        photo.content_hash = Some(format!("real-face-{n}"));
        s.commit("fixture", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        ids.push(id);
    }
    let mountain = decode("ba-tetons.jpg");
    s.media.preview_loader = Some(Arc::new(move |path, _| Some(if path.ends_with("2.arw") { mountain.clone() } else { mother.clone() })));
    let cancel = AtomicBool::new(false);
    assert_eq!(analyze(&mut s, &ids, &cancel, &|_, _| {}).unwrap().analysed, 3);
    assert_eq!(analyze(&mut s, &ids, &cancel, &|_, _| {}).unwrap().analysed, 0);
    let face_rows = run(&mut s, "smartSort.facesInPhoto", json!({"photo":ids[0]}));
    assert!(!face_rows["faces"].as_array().unwrap().is_empty());
    assert!(run(&mut s, "smartSort.facesInPhoto", json!({"photo":ids[2]}))["faces"].as_array().unwrap().is_empty());
    let id = pick(&mut s, ids[0], "Mother");
    let matches = photo_ids(&run(&mut s, "smartSort.findPerson", json!({"person":id})));
    assert!(matches.contains(&ids[0]) && matches.contains(&ids[1]));
    assert!(!matches.contains(&ids[2]));
    assert!(s.catalog.photo(ids[1]).unwrap().meta.regions.iter().any(|r| r.auto && r.name.as_deref() == Some("Mother")));
}

fn png(image: &Rgba8) -> lightcraft_codecs::Result<Vec<u8>> {
    lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(image), &lightcraft_codecs::EncodeMeta::default())
}

#[test]
fn analysis_handles_videos_unreadable_unknown_and_retries_pending_save() {
    let dir = temp();
    let mut s = session();
    s.smart.people = PeopleEngine::new(Some(&dir));
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    s.smart.prefs.faces_enabled = true;
    let a = s.catalog.alloc_photo_id();
    let mut p = Photo::new(a, Source::File { path: "/nonexistent/face.arw".into() }, "face.arw", "RAW", 1600, 1000, "2026-10-09");
    p.kind = MediaKind::Raw;
    p.develop = Arc::new(p.camera_defaults());
    s.commit("raw", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    let cancel = AtomicBool::new(false);
    let r = analyze(&mut s, &[a, PhotoId(999)], &cancel, &|_, _| {}).unwrap();
    assert_eq!(r.failed.len(), 2);
    s.media.preview_loader = Some(Arc::new(|_, _| Some(image(&[[255, 0, 0]]))));
    {
        let _fault = lightcraft_catalog::safe_file::fail_writes_after(4);
        assert!(analyze(&mut s, &[a], &cancel, &|_, _| {}).is_err());
    }
    assert_eq!(s.smart.people.store.len("mock-faces"), 1);
    let r = analyze(&mut s, &[a], &cancel, &|_, _| {}).unwrap();
    assert_eq!(r.analysed, 0);
    assert_eq!(r.skipped, 1);
    assert!(dir.join("AI/faces-mock-faces.bin").exists());
    let mut video = s.catalog.photo(a).unwrap().as_ref().clone();
    video.id = s.catalog.alloc_photo_id();
    video.kind = MediaKind::Video;
    let id = video.id;
    s.commit("video", Op::AddPhoto { photo: Box::new(video) }).unwrap();
    assert_eq!(analyze(&mut s, &[id], &cancel, &|_, _| {}).unwrap().videos, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn merge_keeps_saved_folder_ids_working_and_undo_restores_them() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[0, 255, 0]]);
    let jane = pick(&mut s, a, "Jane");
    let john = pick(&mut s, b, "John");
    let folder =
        crate::smart_sort::FolderDef { name: "Saved".into(), people_enabled: true, person_ids: vec![john], use_rules: false, ..Default::default() };
    run(&mut s, "smartSort.mergePeople", json!({"ids":[jane,john]}));
    let r = run(&mut s, "smartSort.plan", json!({"folders":[folder],"ids":[a,b]}));
    assert_eq!(r["folders"][0]["ids"], json!([a, b]));
    assert_eq!(r["notices"], json!([]));
    run(&mut s, "edit.undo", json!({}));
    let r = run(&mut s, "smartSort.plan", json!({"folders":[folder],"ids":[a,b]}));
    assert_eq!(r["folders"][0]["ids"], json!([b]));
}

#[test]
fn thumbnail_and_rotated_detections_map_to_full_upright_mwg_frame() {
    let mapping = FaceMapping {
        orientation: lightcraft_geom::Orientation::Normal,
        crop: lightcraft_geom::CropGeometry { rect: Rect::new(0.25, 0.25, 0.75, 0.75), angle: 0. },
        width: 6000.,
        height: 4000.,
    };
    let actual = mapping.rect([0.1, 0.2, 0.3, 0.4]);
    for (a, b) in actual.into_iter().zip([0.3, 0.35, 0.15, 0.2]) {
        assert!((a - b).abs() < 1e-6);
    }
    let mapping = FaceMapping { orientation: lightcraft_geom::Orientation::Rotate90, crop: Default::default(), width: 4000., height: 6000. };
    let original = Rect::new(0.1, 0.2, 0.4, 0.6);
    let rotated = lightcraft_geom::Orientation::Rotate90.map_norm_rect(original);
    let actual = mapping.rect([rotated.x0 as f32, rotated.y0 as f32, rotated.width() as f32, rotated.height() as f32]);
    for (a, b) in actual.into_iter().zip([0.1, 0.2, 0.3, 0.4]) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn offline_cropped_thumbnail_analysis_stores_uncropped_face_rects() {
    let mut s = session();
    s.smart.prefs.faces_enabled = true;
    let id = s.catalog.alloc_photo_id();
    let mut photo = Photo::new(id, Source::File { path: "/nonexistent/cropped.jpg".into() }, "cropped.jpg", "JPEG", 6000, 4000, "2026-10-09");
    let mut develop = (*photo.develop).clone();
    develop.crop.geometry.rect = Rect::new(0.25, 0.25, 0.75, 0.75);
    photo.develop = Arc::new(develop);
    s.commit("fixture", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    let input = crate::smart_sort::prepare_inputs(&mut s, &[id]).pop().unwrap();
    let (cache, key) = input.fallback.as_ref().unwrap().cache.as_ref().unwrap();
    cache.put(*key, Arc::new(image(&[[255, 0, 0]])));
    let cancel = AtomicBool::new(false);
    assert_eq!(analyze(&mut s, &[id], &cancel, &|_, _| {}).unwrap().analysed, 1);
    let key = crate::media::content_key(s.catalog.photo(id).unwrap());
    let face = &s.smart.people.store.get("mock-faces", &key).unwrap()[0];
    for (actual, expected) in face.rect.into_iter().zip([0.25, 0.25, 0.25, 0.25]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn clear_without_remove_names_keeps_names_and_legacy_people_centroids_load() {
    let dir = temp();
    let mut s = session();
    s.smart.people = PeopleEngine::new(Some(&dir));
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    let a = add(&mut s, &[[255, 0, 0]]);
    let b = add(&mut s, &[[255, 0, 0]]);
    let id = pick(&mut s, a, "Jane");
    s.smart.people.save().unwrap();
    let data = json!({"version":1,"people":[{"id":id,"name":"Jane","faces":[[crate::media::content_key(s.catalog.photo(a).unwrap()),0]],"rejected":[]}],"nextId":id+1});
    std::fs::write(dir.join("AI/people.json"), serde_json::to_vec(&data).unwrap()).unwrap();
    let mut loaded = PeopleEngine::new(Some(&dir));
    loaded.tagger = Some(Arc::new(MockFaces));
    loaded.ready().unwrap();
    assert_eq!(loaded.data.person(id).unwrap().centroid.len(), 128);
    assert_eq!(matches(&s.catalog, &loaded, &loaded.data, id).unwrap().len(), 2);
    let before = s.catalog.photo(a).unwrap().meta.regions.clone();
    run(&mut s, "smartSort.clearFaceData", json!({}));
    assert_eq!(s.catalog.photo(a).unwrap().meta.regions, before);
    assert!(s.catalog.photo(a).unwrap().meta.person_ids.is_empty());
    assert!(s.catalog.photo(b).unwrap().meta.person_ids.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
