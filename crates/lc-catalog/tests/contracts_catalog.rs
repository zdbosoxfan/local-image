//! Persistence boundaries: journal replay, compacted disk snapshot, and frozen older-format inputs
//! (refused).
use lightcraft_catalog::journal::{LOG, SNAPSHOT, VERSION};
use lightcraft_catalog::*;
use lightcraft_develop::DevelopSettings;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

fn photo(id: u64) -> Photo {
    Photo::new(PhotoId(id), Source::File { path: format!("/contracts/{id}.jpg") }, &format!("{id}.jpg"), "JPEG", 32, 24, "2026-10-11")
}
fn expected_reference() -> Catalog {
    let mut c = Catalog::new();
    let mut p = photo(1);
    p.rating = 4;
    p.flag = Flag::Pick;
    p.label = Some(ColorLabel::Purple);
    p.meta.title = "Contract photo".into();
    p.meta.keywords = vec!["birds".into(), "places|海岸".into()];
    let mut settings = DevelopSettings::default();
    settings.light.exposure = 1.25;
    settings.color.vibrance = 15.;
    settings.profile.id = "lc.vivid".into();
    p.develop = Arc::new(settings);
    c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    let mut a = Album::new(c.alloc_album_id(), "Contract album");
    a.photos = vec![PhotoId(1)];
    a.cover = Some(PhotoId(1));
    c.apply(Op::AddAlbum { album: a }).unwrap();
    c.apply(Op::SetLabelName { label: ColorLabel::Purple, name: Some("Deliver".into()) }).unwrap();
    c.apply(Op::SetBrowsed { folder: "/contracts".into(), at: Some("2026-10-11".into()) }).unwrap();
    c
}
fn equal(actual: &Catalog, expected: &Catalog) {
    // The revision is in-process state; compare every persisted field, including allocators.
    assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
    assert_eq!(actual.photos().collect::<Vec<_>>(), expected.photos().collect::<Vec<_>>());
    assert_eq!(actual.albums().collect::<Vec<_>>(), expected.albums().collect::<Vec<_>>());
}
fn ops() -> Vec<Op> {
    let mut ops = Vec::new();
    for i in 1..=6 {
        let p = photo(i);
        let mut settings = DevelopSettings::default();
        settings.light.exposure = i as f64 / 4.;
        settings.color.vibrance = 17.;
        settings.profile.id = "lc.vivid".into();
        ops.extend([
            Op::AddPhoto { photo: Box::new(p) },
            Op::SetRating { id: PhotoId(i), rating: (i - 1) as u8 },
            Op::SetFlag { id: PhotoId(i), flag: [Flag::None, Flag::Pick, Flag::Reject][i as usize % 3] },
            Op::SetLabel { id: PhotoId(i), label: if i == 6 { None } else { Some(ColorLabel::ALL[i as usize - 1]) } },
            Op::SetMeta {
                id: PhotoId(i),
                meta: Box::new(Meta {
                    keywords: vec![format!("subject|{i}"), "日本語".into()],
                    title: format!("Title {i}"),
                    caption: "Stored caption".into(),
                    ..Default::default()
                }),
            },
            Op::SetDevelop {
                id: PhotoId(i),
                settings: Arc::new(settings.clone()),
                label: "Exposure".into(),
                edited: Some("2026-10-11T12:34:56".into()),
            },
            Op::SetVersions {
                id: PhotoId(i),
                versions: vec![lightcraft_catalog::Version {
                    name: "Alternate".into(),
                    created: "2026-10-11".into(),
                    settings: Arc::new(settings.clone()),
                    auto: false,
                }],
            },
            Op::PushHistory { id: PhotoId(i), step: HistoryStep { label: "Exposure".into(), settings: Arc::new(settings) } },
        ]);
    }
    let mut folder = Album::new(AlbumId(1), "Projects");
    folder.folder = true;
    let mut album = Album::new(AlbumId(2), "Ordered photos");
    album.parent = Some(folder.id);
    album.photos = vec![PhotoId(3), PhotoId(1), PhotoId(6)];
    album.cover = Some(PhotoId(1));
    ops.extend([
        Op::AddAlbum { album: folder },
        Op::AddAlbum { album },
        Op::AddStack { stack: Stack { id: StackId(1), photos: vec![PhotoId(1), PhotoId(2)], collapsed: true } },
        Op::SetLabelName { label: ColorLabel::Red, name: Some("Review".into()) },
    ]);
    ops
}
#[test]
fn journal_replay_keeps_ratings_flags_labels_keywords_albums_and_develop() {
    let store = MemStore::new();
    let (mut journal, mut expected, _) = Journal::open(Box::new(store.clone())).unwrap();
    let ops = ops();
    for op in &ops {
        expected.apply(op.clone()).unwrap();
    }
    journal.append(&ops).unwrap();
    drop(journal);
    let (_, actual, report) = Journal::open(Box::new(store)).unwrap();
    assert_eq!(report.replayed, ops.len());
    equal(&actual, &expected);
}
#[test]
fn disk_snapshot_and_post_snapshot_edits_survive_reopen() {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("catalog-contract-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir(&dir).unwrap();
    let (mut journal, mut expected, _) = Journal::open(Box::new(FsStore::open(&dir).unwrap())).unwrap();
    let ops = ops();
    for op in &ops {
        expected.apply(op.clone()).unwrap();
    }
    journal.append(&ops).unwrap();
    journal.snapshot(&expected).unwrap();
    drop(journal);
    let (mut journal, actual, _) = Journal::open(Box::new(FsStore::open(&dir).unwrap())).unwrap();
    equal(&actual, &expected);
    let tail = [Op::SetRating { id: PhotoId(1), rating: 5 }, Op::RenameAlbum { id: AlbumId(2), name: "Renamed ✓".into() }];
    for op in &tail {
        expected.apply(op.clone()).unwrap();
    }
    journal.append(&tail).unwrap();
    drop(journal);
    let (_, actual, report) = Journal::open(Box::new(FsStore::open(&dir).unwrap())).unwrap();
    assert_eq!(report.replayed, 2);
    equal(&actual, &expected);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn frozen_v2_catalog_reference_loads_exact_records() {
    let store = MemStore::new();
    store.set(SNAPSHOT, include_bytes!("fixtures/contracts/catalog-v2.snap").to_vec());
    let raw: serde_json::Value = serde_json::from_slice(&store.get(SNAPSHOT).unwrap()).unwrap();
    assert_eq!(raw["version"], 2);
    const { assert!(VERSION >= 2) };
    let (mut j, c, _) = Journal::open(Box::new(store.clone())).unwrap();
    equal(&c, &expected_reference());
    j.snapshot(&c).unwrap();
    drop(j);
    let (_, again, _) = Journal::open(Box::new(store)).unwrap();
    equal(&again, &expected_reference());
}
/// Frozen libraries from before format v2 (a v1 snapshot + log, and a versionless log-only
/// journal) are refused as older, with nothing modified: there is no migration.
#[test]
fn frozen_pre_v2_libraries_are_refused_untouched() {
    let v1 = MemStore::new();
    v1.set(SNAPSHOT, include_bytes!("fixtures/contracts/catalog-v1.snap").to_vec());
    v1.set(LOG, include_bytes!("fixtures/contracts/catalog-v1.log").to_vec());
    let v0 = MemStore::new();
    v0.set(LOG, include_bytes!("fixtures/contracts/catalog-v0.log").to_vec());
    for store in [v1, v0] {
        let before = store.files.lock().unwrap().clone();
        match Journal::open(Box::new(store.clone())) {
            Err(CatalogError::Older(_)) => {}
            Err(other) => panic!("expected CatalogError::Older, got {other}"),
            Ok(_) => panic!("a pre-v2 library must not open"),
        }
        assert_eq!(*store.files.lock().unwrap(), before, "nothing modified");
    }
}
