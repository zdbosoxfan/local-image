use crate::Session;
use lightcraft_catalog::{Album, Op, Photo, PhotoId, Source};
use serde_json::json;

fn relative_clock_session(captures: &[&str]) -> (Session, std::sync::Arc<std::sync::Mutex<String>>, Vec<PhotoId>) {
    let mut s = Session::new();
    let now = std::sync::Arc::new(std::sync::Mutex::new("2026-10-24T12:00:00".to_string()));
    let clock = now.clone();
    s.clock = Box::new(move || clock.lock().unwrap().clone());
    let mut ids = Vec::new();
    for (index, captured) in captures.iter().enumerate() {
        let id = PhotoId(index as u64 + 1);
        let name = format!("synthetic-{index}.jpg");
        let mut photo = Photo::new(id, Source::Demo { scene: 0 }, &name, "JPEG", 2, 2, "2026-10-01T00:00:00");
        photo.captured = Some((*captured).to_string());
        s.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        ids.push(id);
    }
    (s, now, ids)
}

#[test]
fn relative_clock_filter_expires_without_catalog_or_history_mutation() {
    let (mut s, now, ids) = relative_clock_session(&["2026-10-23T12:00:30"]);
    s.execute("library.filter", &json!({"ruleSet": {"rules": [{"field": "captureDate", "op": "inLast", "value": 1}]}})).unwrap();
    let (before_generation, before) = s.visible_shared();
    assert_eq!(before.as_ref(), ids.as_slice());
    let revision = s.catalog.revision;
    let undo = s.undo.len();
    let redo = s.redo.len();

    *now.lock().unwrap() = "2026-10-24T12:01:00".to_string();
    let (after_generation, after) = s.visible_shared();

    assert!(after.is_empty(), "a relative filter must expire when its minute changes");
    assert_ne!(after_generation, before_generation, "the changed list gets a new visible generation");
    assert_eq!(s.catalog.revision, revision, "clock expiry is not a catalog edit");
    assert_eq!(s.undo.len(), undo);
    assert_eq!(s.redo.len(), redo);
}

#[test]
fn relative_clock_smart_album_sources_expire() {
    let (mut s, now, ids) = relative_clock_session(&["2026-10-23T12:00:30"]);
    let album = s
        .execute(
            "album.createSmart",
            &json!({"name":"Recent", "rules":{"ruleSet":{"rules":[{"group":{"rules":[{"field":"captureDate","op":"inLast","value":1}]}}]}}}),
        )
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute("library.source", &json!({"kind":"album", "id":album})).unwrap();
    assert_eq!(s.visible_cloned(), ids);
    let revision = s.catalog.revision;
    *now.lock().unwrap() = "2026-10-24T12:01:00".into();
    assert!(s.visible_cloned().is_empty());
    assert_eq!(s.catalog.revision, revision);
}

#[test]
fn relative_clock_source_total_expires_before_or_after_visible_refresh() {
    for refresh_visible in [false, true] {
        let (mut s, now, _) = relative_clock_session(&["2026-10-23T12:00:30"]);
        let album = s
            .execute(
                "album.createSmart",
                &json!({"name": "Recent", "rules": {"ruleSet": {"rules": [{"field": "captureDate", "op": "inLast", "value": 1}]}}}),
            )
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        s.execute("library.source", &json!({"kind": "album", "id": album})).unwrap();
        s.execute("library.filter", &json!({"text": "synthetic"})).unwrap();
        assert_eq!(s.source_total(), Some(1), "the initial total is computed without asking for visible photos");
        let revision = s.catalog.revision;

        *now.lock().unwrap() = "2026-10-24T12:01:00".to_string();
        if refresh_visible {
            s.execute("library.refresh", &json!({})).unwrap();
            assert!(s.visible_cloned().is_empty());
        }
        assert_eq!(s.source_total(), Some(0), "refresh_visible={refresh_visible}");
        assert_eq!(s.catalog.revision, revision, "refresh_visible={refresh_visible}");
    }
}

#[test]
fn relative_clock_absolute_and_same_minute_views_keep_their_generations() {
    let (mut absolute, absolute_now, _) = relative_clock_session(&["2026-10-23T12:00:30"]);
    absolute.execute("library.filter", &json!({"ruleSet": {"rules": [{"field": "captureDate", "op": "is", "value": "2026-10-23"}]}})).unwrap();
    let (absolute_generation, absolute_ids) = absolute.visible_shared();
    *absolute_now.lock().unwrap() = "2026-10-25T12:10:00".to_string();
    let (after_absolute_generation, after_absolute_ids) = absolute.visible_shared();
    assert_eq!(after_absolute_generation, absolute_generation);
    assert_eq!(after_absolute_ids, absolute_ids);

    let (mut relative, relative_now, _) = relative_clock_session(&["2026-10-23T12:00:30"]);
    relative.execute("library.filter", &json!({"ruleSet": {"rules": [{"field": "captureDate", "op": "inLast", "value": 1}]}})).unwrap();
    let (relative_generation, relative_ids) = relative.visible_shared();
    *relative_now.lock().unwrap() = "2026-10-24T12:00:45".to_string();
    let (same_minute_generation, same_minute_ids) = relative.visible_shared();
    assert_eq!(same_minute_generation, relative_generation);
    assert_eq!(same_minute_ids, relative_ids);
}

#[test]
fn relative_clock_unrelated_smart_album_does_not_expire_album_filter_view() {
    let (mut s, now, ids) = relative_clock_session(&["2026-10-23T12:00:30"]);
    let manual = s.catalog.alloc_album_id();
    s.catalog.apply(Op::AddAlbum { album: Album::new(manual, "Manual") }).unwrap();
    s.catalog.apply(Op::SetAlbumPhotos { id: manual, photos: vec![ids[0]] }).unwrap();
    s.execute(
        "album.createSmart",
        &json!({"name": "Unrelated recent", "rules": {"ruleSet": {"rules": [{"field": "captureDate", "op": "inLast", "value": 1}]}}}),
    )
    .unwrap();
    s.execute("library.filter", &json!({"ruleSet": {"rules": [{"field": "album", "op": "is", "value": manual.0}]}})).unwrap();
    let (generation, visible) = s.visible_shared();
    assert_eq!(visible.as_ref(), ids.as_slice());

    *now.lock().unwrap() = "2026-10-24T12:01:00".to_string();
    let (after_generation, after_visible) = s.visible_shared();

    assert_eq!(after_generation, generation, "an unrelated relative smart album must not expire this view");
    assert_eq!(after_visible, visible);
}
