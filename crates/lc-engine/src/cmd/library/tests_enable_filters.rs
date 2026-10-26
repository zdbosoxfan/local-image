use crate::Session;
use lightcraft_catalog::{Flag, Op, Photo, PhotoId, Source};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    for (id, name, rating, flag) in [(1, "atrium.jpg", 5, Flag::Pick), (2, "atrium-2.jpg", 2, Flag::None), (3, "stairs.jpg", 5, Flag::Pick)] {
        let mut photo = Photo::new(PhotoId(id), Source::Demo { scene: 0 }, name, "JPEG", 2, 2, "2026-10-25T12:00:00");
        photo.content_hash = Some(format!("synthetic-{id}"));
        photo.rating = rating;
        photo.flag = flag;
        s.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    }
    s.execute("library.sort", &json!({"ascending":true})).unwrap();
    s.selection = crate::Selection::single(PhotoId(1));
    s
}

#[test]
fn toggle_restores_combined_filter_and_visible_order_without_catalog_edits() {
    let mut s = session();
    s.execute("library.filter", &json!({"text":"atrium", "rating":5, "flag":"pick"})).unwrap();
    let filter = s.filter.clone();
    let (generation, ids) = s.visible_shared();
    assert_eq!(ids.as_ref(), &[PhotoId(1)]);
    let revision = s.catalog.revision;
    let undo = s.undo.len();
    let off = s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(off, json!({"enabled":false, "count":3}));
    assert_eq!(s.filter, Default::default());
    let (unfiltered_generation, unfiltered) = s.visible_shared();
    assert_ne!(generation, unfiltered_generation);
    assert_eq!(unfiltered.as_ref(), &[PhotoId(1), PhotoId(2), PhotoId(3)]);
    assert_eq!(s.execute("library.toggleFilters", &json!({})).unwrap(), json!({"enabled":true, "count":1}));
    assert_eq!(s.filter, filter);
    assert_eq!(s.visible_cloned().as_slice(), ids.as_ref());
    assert_eq!(s.catalog.revision, revision);
    assert_eq!(s.undo.len(), undo);
}

#[test]
fn empty_toggle_keeps_enabled_and_sources_still_bound_the_view() {
    let mut s = session();
    assert_eq!(s.execute("library.toggleFilters", &json!({})).unwrap(), json!({"enabled":true, "count":3}));
    let album = s.execute("album.create", &json!({"name":"Subset"})).unwrap()["id"].as_u64().unwrap();
    s.execute("album.addPhotos", &json!({"id":album, "ids":[1,2]})).unwrap();
    for (source, all, filtered) in [
        (crate::LibrarySource::Picks, vec![PhotoId(1), PhotoId(3)], vec![PhotoId(1), PhotoId(3)]),
        (crate::LibrarySource::Album(lightcraft_catalog::AlbumId(album)), vec![PhotoId(1), PhotoId(2)], vec![PhotoId(1)]),
    ] {
        s.source = source;
        s.execute("library.filter", &json!({"rating":5})).unwrap();
        assert_eq!(s.visible_cloned(), filtered);
        s.execute("library.toggleFilters", &json!({})).unwrap();
        assert_eq!(s.visible_cloned(), all);
        s.execute("library.toggleFilters", &json!({})).unwrap();
        assert_eq!(s.visible_cloned(), filtered);
        s.execute("library.clearFilter", &json!({})).unwrap();
    }
    s.catalog.apply(Op::SetDeleted { id: PhotoId(2), deleted: true, deleted_at: None }).unwrap();
    s.source = crate::LibrarySource::RecentlyDeleted;
    s.execute("library.filter", &json!({"rating":5})).unwrap();
    assert!(s.visible().is_empty());
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.visible_cloned(), vec![PhotoId(2)]);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert!(s.visible().is_empty());
}

#[test]
fn explicit_filter_clear_and_preset_forget_shelf_only_after_success() {
    let mut s = session();
    s.execute("library.filter", &json!({"rating":5})).unwrap();
    s.execute("filter.savePreset", &json!({"name":"Stars"})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    for (command, params) in
        [("library.filter", json!({"rating":"bad"})), ("library.filter", json!({"flag":42})), ("filter.applyPreset", json!({"name":"missing"}))]
    {
        assert!(s.execute(command, &params).is_err());
        assert!(s.filters_off());
        assert_eq!(s.visible().len(), 3);
    }
    s.execute("library.filter", &json!({"text":"stairs"})).unwrap();
    assert!(!s.filters_off());
    assert_eq!(s.visible_cloned(), vec![PhotoId(3)]);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.execute("filter.applyPreset", &json!({"name":"Stars"})).unwrap();
    assert!(!s.filters_off());
    assert_eq!(s.visible_cloned(), vec![PhotoId(1), PhotoId(3)]);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.execute("library.clearFilter", &json!({})).unwrap();
    assert_eq!(s.execute("library.toggleFilters", &json!({})).unwrap(), json!({"enabled":true,"count":3}));
    assert_eq!(s.filter, Default::default());
}

#[test]
fn find_similar_enables_its_hits_and_query_only_preserves_shelf() {
    let mut s = session();
    for id in [PhotoId(1), PhotoId(2), PhotoId(3)] {
        let key = crate::media::content_key(s.catalog.photo(id).unwrap());
        let mut signature = [0.0; 64];
        signature[if id == PhotoId(3) { 1 } else { 0 }] = 1.0;
        s.signatures.insert(key, signature);
    }
    s.execute("library.filter", &json!({"text":"stairs"})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    let shelf = s.shelved_filter.clone();
    let hits = s.execute("library.findSimilar", &json!({"id":1,"filter":false})).unwrap();
    assert_eq!(hits["photos"].as_array().unwrap().len(), 2);
    assert_eq!(s.shelved_filter, shelf);
    assert_eq!(s.visible().len(), 3);
    s.execute("library.findSimilar", &json!({"id":1})).unwrap();
    assert!(!s.filters_off());
    assert_eq!(s.visible_cloned(), vec![PhotoId(1), PhotoId(2)]);
}

#[test]
fn shelved_keyword_follows_rename_merge_and_deletion_without_losing_other_criteria() {
    let mut s = session();
    s.execute("photo.setMeta", &json!({"ids":[1],"addKeywords":["Travel|Hall"]})).unwrap();
    s.execute("library.filter", &json!({"keyword":"Travel|Hall","rating":5})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.execute("keyword.rename", &json!({"from":"Travel","to":"Places"})).unwrap();
    s.execute("keyword.merge", &json!({"from":["Places"],"into":"Architecture"})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.filter.keyword.as_deref(), Some("Architecture|Hall"));
    assert_eq!(s.visible_cloned(), vec![PhotoId(1)]);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.execute("keyword.delete", &json!({"keyword":"Architecture"})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.filter.keyword, None);
    assert_eq!(s.filter.rating, 5);
    assert_eq!(s.visible_cloned(), vec![PhotoId(1), PhotoId(3)]);
    s.execute("library.clearFilter", &json!({})).unwrap();
    s.execute("library.filter", &json!({"keyword":"Unused"})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.execute("keyword.delete", &json!({"keyword":"Unused"})).unwrap();
    assert!(!s.filters_off(), "removing the only criterion leaves filtering enabled");
    assert_eq!(s.execute("library.toggleFilters", &json!({})).unwrap(), json!({"enabled":true,"count":3}));
}

#[test]
fn shelved_folder_follows_component_paths_even_when_browse_is_elsewhere() {
    let mut s = session();
    for (path, moved) in [("/shoot/a/inside", "/archive/b/inside"), ("/shoot/ab", "/shoot/ab")] {
        s.source = crate::LibrarySource::All;
        s.browse = Some(crate::Browse { path: "/elsewhere".into(), subfolders: true });
        s.execute("library.filter", &json!({"folder":path,"rating":5})).unwrap();
        s.execute("library.toggleFilters", &json!({})).unwrap();
        super::super::browse::follow_folder(&mut s, "/shoot/a", "/archive/b");
        assert_eq!(s.browse.as_ref().unwrap().path, "/elsewhere");
        s.execute("library.toggleFilters", &json!({})).unwrap();
        assert_eq!(s.filter.folder.as_deref(), Some(moved));
        assert_eq!(s.filter.rating, 5);
    }
    s.execute("library.filter", &json!({"folder":"/shoot/a/inside", "rating":5})).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.browse = Some(crate::Browse { path: "/shoot/a/inside".into(), subfolders: true });
    super::super::browse::follow_folder(&mut s, "/shoot/a", "/archive/b");
    assert_eq!(s.browse.as_ref().unwrap().path, "/archive/b/inside");
    assert_eq!(s.filter, Default::default(), "moving the browsed folder keeps the active filter off");
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.filter.folder.as_deref(), Some("/archive/b/inside"));
    assert_eq!(s.filter.rating, 5);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.source = crate::LibrarySource::Folder;
    s.browse = Some(crate::Browse { path: "/current".into(), subfolders: true });
    let file_name = s.catalog.photo(PhotoId(1)).unwrap().file_name.clone();
    s.catalog.apply(Op::Relink { id: PhotoId(1), source: Source::File { path: "/current/atrium.jpg".into() }, file_name, format: None }).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.filter.folder, None);
    assert_eq!(s.visible_cloned(), vec![PhotoId(1)]);
}

#[test]
fn shelved_people_ids_resolve_existing_merge_projection() {
    let mut s = session();
    let jane = s.smart.people.data.add("Jane".into()).unwrap();
    let john = s.smart.people.data.add("Jane".into()).unwrap();
    for (photo, person) in [(PhotoId(1), jane), (PhotoId(2), john)] {
        let mut meta = s.catalog.photo(photo).unwrap().meta.clone();
        meta.person_ids = vec![person];
        meta.regions.push(lightcraft_meta::Region {
            kind: lightcraft_meta::RegionKind::Face,
            name: Some("Jane".into()),
            rect: lightcraft_meta::Rect::from_xywh(0.1, 0.1, 0.2, 0.2),
            description: None,
            auto: false,
        });
        s.catalog.apply(Op::SetMeta { id: photo, meta: Box::new(meta) }).unwrap();
    }
    s.execute("library.filter", &json!({"personIds":[john]})).unwrap();
    assert_eq!(s.visible_cloned(), vec![PhotoId(2)]);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    let mut merged = s.smart.people.data.clone();
    merged.people.retain(|person| person.id != john);
    merged.merged_ids.insert(john, jane);
    s.commit_people("Merge people", merged).unwrap();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.filter.person_ids, vec![john]);
    assert_eq!(s.visible_cloned(), vec![PhotoId(1), PhotoId(2)]);
}

#[test]
fn library_switch_keeps_session_filter_but_restart_drops_shelf() {
    let dir = std::env::temp_dir().join(format!("lc-enable-filters-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::new();
    s.open_library(dir.join("first"), true).unwrap();
    s.execute("library.filter", &json!({"rating":5})).unwrap();
    let filter = s.filter.clone();
    let (generation, _) = s.visible_shared();
    s.open_library(dir.join("second"), true).unwrap();
    assert_eq!(s.filter, filter);
    assert_ne!(s.visible_shared().0, generation);
    let filtered = s.visible_cloned();
    s.execute("library.toggleFilters", &json!({})).unwrap();
    s.open_library(dir.join("first"), true).unwrap();
    assert!(s.filters_off());
    assert!(s.visible().len() > filtered.len());
    s.execute("library.toggleFilters", &json!({})).unwrap();
    assert_eq!(s.visible_cloned(), filtered);
    s.execute("library.toggleFilters", &json!({})).unwrap();
    drop(s);
    let mut fresh = Session::new();
    fresh.open_library(dir.join("first"), false).unwrap();
    assert!(!fresh.filters_off());
    assert_eq!(fresh.filter, Default::default());
    assert!(fresh.visible().len() > filtered.len());
    drop(fresh);
    std::fs::remove_dir_all(dir).unwrap();
}
