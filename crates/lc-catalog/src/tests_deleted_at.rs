//! When photos went to Recently Deleted ([`Photo::deleted_at`]): v2 libraries written before it
//! was recorded open unchanged (the time is unknown), new times round-trip, undo restores them,
//! and [`Catalog::deleted_before`] picks only known, old-enough deletions.

use crate::journal::{LOG, SNAPSHOT, VERSION};
use crate::*;

/// A v2 library as builds before `deleted_at` wrote it: photo 1 deleted in the snapshot, photo 2
/// deleted by a log record, photo 3 in the library.
fn v2_fixture() -> MemStore {
    let photo = |id: u64, deleted: bool| {
        format!(
            r#"{{"id":{id},"source":{{"type":"file","path":"/p/{id}.jpg"}},"file_name":"{id}.jpg","kind":"image","format":"JPEG","width":4,"height":3,"file_size":0,"captured":null,"imported":"2026-01-01T00:00:00","rating":0,"flag":"none","label":null,"develop":{{}},"edited":null,"versions":[],"history":[],"deleted":{deleted}}}"#
        )
    };
    let catalog = format!(
        r#"{{"photos":{{"1":{},"2":{},"3":{}}},"albums":{{}},"next_photo":4,"next_album":1}}"#,
        photo(1, true),
        photo(2, false),
        photo(3, false)
    );
    let m = MemStore::new();
    m.set(SNAPSHOT, format!(r#"{{"format":"lightcraft-catalog","version":{VERSION},"seq":1,"catalog":{catalog}}}"#).into_bytes());
    let body = r#"{"op":"setDeleted","id":2,"deleted":true}"#;
    m.set(LOG, format!("{{\"seq\":2,\"crc\":{},\"op\":{body}}}\n", crc32fast::hash(body.as_bytes())).into_bytes());
    m
}

#[test]
fn v2_library_without_deletion_times_opens_unchanged() {
    let m = v2_fixture();
    let before = m.files.lock().unwrap().clone();
    let (mut j, c, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert_eq!((r.replayed, r.failed, r.torn_bytes, r.damaged.as_deref()), (1, 0, 0, None), "{r:?}");
    assert_eq!(*m.files.lock().unwrap(), before, "opening rewrote nothing");
    let p = |id: u64| c.photo(PhotoId(id)).unwrap();
    assert!(p(1).deleted && p(2).deleted && !p(3).deleted);
    assert!(p(1).deleted_at.is_none() && p(2).deleted_at.is_none(), "unknown");
    assert_eq!(c.undated_deleted(), vec![PhotoId(1), PhotoId(2)]);
    assert!(c.deleted_before("2099-01-01T00:00:00", 1).is_empty(), "an unknown time is never old");
    // written back, they stay in the old shape
    let snap = c.to_snapshot();
    assert!(!snap.contains("deleted_at"), "{snap}");
    assert_eq!(Catalog::from_snapshot(&snap).unwrap().to_snapshot(), snap);
    j.snapshot(&c).unwrap();
    let (_, again, _) = Journal::open(Box::new(m)).unwrap();
    assert_eq!(again.to_snapshot(), snap);
}

#[test]
fn deletion_times_round_trip_through_the_journal() {
    let m = MemStore::new();
    let (mut j, mut c, _) = Journal::open(Box::new(m.clone())).unwrap();
    let mut ops = Vec::new();
    for id in 1..=2 {
        ops.push(Op::AddPhoto { photo: Box::new(Photo::new(PhotoId(id), Source::Demo { scene: 0 }, "a.jpg", "JPEG", 4, 3, "2026-01-01")) });
    }
    ops.push(Op::SetDeleted { id: PhotoId(1), deleted: true, deleted_at: Some("2026-03-01T09:30:00".into()) });
    ops.push(Op::SetDeleted { id: PhotoId(2), deleted: true, deleted_at: None });
    for op in &ops {
        c.apply(op.clone()).unwrap();
    }
    j.append(&ops).unwrap();
    // an op without a time is logged without the field, exactly as v2 builds wrote it
    let log = String::from_utf8(m.get(LOG).unwrap()).unwrap();
    assert!(log.contains(r#"{"op":"setDeleted","id":2,"deleted":true}"#), "{log}");
    assert!(log.contains(r#""deleted_at":"2026-03-01T09:30:00""#), "{log}");
    let (mut j, replayed, _) = Journal::open(Box::new(m.clone())).unwrap();
    assert_eq!(replayed.to_snapshot(), c.to_snapshot());
    assert_eq!(replayed.photo(PhotoId(1)).unwrap().deleted_at.as_deref(), Some("2026-03-01T09:30:00"));
    j.snapshot(&replayed).unwrap();
    let (_, from_snapshot, _) = Journal::open(Box::new(m)).unwrap();
    assert_eq!(from_snapshot.to_snapshot(), c.to_snapshot());
}

#[test]
fn undo_restores_the_previous_deletion_time() {
    let mut c = Catalog::new();
    let id = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(Photo::new(id, Source::Demo { scene: 0 }, "a.jpg", "JPEG", 4, 3, "2026-01-01")) }).unwrap();
    let del = c.apply(Op::SetDeleted { id, deleted: true, deleted_at: Some("2026-02-01T00:00:00".into()) }).unwrap();
    assert_eq!(del, Op::SetDeleted { id, deleted: false, deleted_at: None });
    let restamp = c.apply(Op::SetDeleted { id, deleted: true, deleted_at: Some("2026-05-01T00:00:00".into()) }).unwrap();
    assert_eq!(restamp, Op::SetDeleted { id, deleted: true, deleted_at: Some("2026-02-01T00:00:00".into()) });
    c.apply(restamp).unwrap();
    assert_eq!(c.photo(id).unwrap().deleted_at.as_deref(), Some("2026-02-01T00:00:00"));
    c.apply(del).unwrap();
    let p = c.photo(id).unwrap();
    assert!(!p.deleted && p.deleted_at.is_none());
}

#[test]
fn deleted_before_selects_known_old_deletions() {
    let mut c = Catalog::new();
    let at = [Some("2026-01-01T12:00:00"), Some("2026-01-20T12:00:00"), None, Some("not a date"), Some("2026-01-02T12:00:00")];
    for (i, t) in at.iter().enumerate() {
        let id = c.alloc_photo_id();
        c.apply(Op::AddPhoto { photo: Box::new(Photo::new(id, Source::Demo { scene: 0 }, "a.jpg", "JPEG", 4, 3, "2026-01-01")) }).unwrap();
        // the last one was deleted and restored: not in Recently Deleted
        c.apply(Op::SetDeleted { id, deleted: i != 4, deleted_at: t.map(String::from) }).unwrap();
    }
    let now = "2026-01-31T12:00:00";
    assert_eq!(c.deleted_before(now, 30), vec![PhotoId(1)], "exactly 30 days counts");
    assert_eq!(c.deleted_before(now, 11), vec![PhotoId(1), PhotoId(2)]);
    assert_eq!(c.deleted_before(now, 12), vec![PhotoId(1)]);
    assert!(c.deleted_before(now, 31).is_empty());
    assert!(c.deleted_before(now, 0).is_empty(), "0 = never");
    assert!(c.deleted_before("garbage", 1).is_empty());
    assert_eq!(c.undated_deleted(), vec![PhotoId(3)]);
}
