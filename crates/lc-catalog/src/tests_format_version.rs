//! Issue #102: catalog format versions. Only this build's format is read: an older or a newer
//! format is refused with a clear error and left untouched (never read in part, migrated,
//! truncated or set aside).

use crate::journal::{LOG, SNAPSHOT, VERSION, encode_record};
use crate::*;

fn photo(c: &mut Catalog) -> Op {
    let id = c.alloc_photo_id();
    Op::AddPhoto { photo: Box::new(Photo::new(id, Source::File { path: format!("/p/{}.jpg", id.0) }, "p.jpg", "JPEG", 4, 3, "2026-01-01")) }
}

/// A catalog with two photos and the log line for a third op on top of it.
fn parts() -> (Catalog, String, Catalog) {
    let mut c = Catalog::new();
    for _ in 0..2 {
        let op = photo(&mut c);
        c.apply(op).unwrap();
    }
    let base = c.clone();
    let id = c.photos().next().unwrap().id;
    let op = Op::SetRating { id, rating: 4 };
    c.apply(op.clone()).unwrap();
    (base, format!("{}\n", encode_record(3, &op)), c)
}

fn snapshot_version(m: &MemStore) -> u64 {
    let v: serde_json::Value = serde_json::from_slice(&m.get(SNAPSHOT).unwrap()).unwrap();
    v["version"].as_u64().unwrap()
}

fn assert_refused_as_older(m: &MemStore) {
    let before = m.files.lock().unwrap().clone();
    let e = Journal::open(Box::new(m.clone())).err().expect("refused");
    assert!(matches!(e, CatalogError::Older(_)), "{e:?}");
    let msg = e.to_string();
    assert!(msg.contains("earlier version") && msg.contains("left unchanged"), "{msg}");
    assert_eq!(*m.files.lock().unwrap(), before, "nothing modified, nothing renamed");
}

/// Snapshots of formats before this one (v1 was written up to v0.2.0) are refused, not upgraded.
#[test]
fn older_snapshot_is_refused_untouched() {
    let (base, log, _) = parts();
    for version in 0..VERSION {
        let m = MemStore::new();
        m.set(
            SNAPSHOT,
            format!("{{\"format\":\"lightcraft-catalog\",\"version\":{version},\"seq\":2,\"catalog\":{}}}\n", base.to_snapshot()).into_bytes(),
        );
        m.set(LOG, log.clone().into_bytes());
        assert_refused_as_older(&m);
        let e = Journal::open(Box::new(m)).err().unwrap().to_string();
        assert!(e.contains(&format!("v{version}")), "{e}");
    }
}

/// The earliest builds kept only a log (no snapshot): refused, not replayed.
#[test]
fn log_without_snapshot_is_refused_untouched() {
    let mut c = Catalog::new();
    let op = photo(&mut c);
    let m = MemStore::new();
    m.set(LOG, format!("{}\n", encode_record(1, &op)).into_bytes());
    assert_refused_as_older(&m);
    // an empty (or blank) log without a snapshot is a library that has nothing yet: it opens
    let m = MemStore::new();
    m.set(LOG, b"\n".to_vec());
    let (_, loaded, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert!(loaded.is_empty() && !r.created);
    assert_eq!(snapshot_version(&m), u64::from(VERSION));
}

/// A new library gets a snapshot in this format at once, so an older build never sees a bare log
/// it would read in part.
#[test]
fn new_library_is_written_in_this_format() {
    let m = MemStore::new();
    let (_, _, r) = Journal::open(Box::new(m.clone())).unwrap();
    assert!(r.created);
    assert_eq!(snapshot_version(&m), u64::from(VERSION));
}

fn assert_refused_untouched(m: &MemStore) {
    let before = m.files.lock().unwrap().clone();
    let e = Journal::open(Box::new(m.clone())).err().expect("refused");
    assert!(matches!(e, CatalogError::Newer(_)), "{e:?}");
    let msg = e.to_string();
    assert!(msg.contains("newer version of LightCraft") && msg.contains("left unchanged"), "{msg}");
    assert_eq!(*m.files.lock().unwrap(), before, "nothing modified, nothing renamed");
}

#[test]
fn newer_snapshot_is_refused_untouched() {
    let (base, log, _) = parts();
    let m = MemStore::new();
    m.set(
        SNAPSHOT,
        format!("{{\"format\":\"lightcraft-catalog\",\"version\":{},\"seq\":2,\"catalog\":{}}}\n", VERSION + 1, base.to_snapshot()).into_bytes(),
    );
    m.set(LOG, log.into_bytes());
    assert_refused_untouched(&m);
    // even when its catalog doesn't parse as this version's
    let m = MemStore::new();
    m.set(
        SNAPSHOT,
        format!("{{\"format\":\"lightcraft-catalog\",\"version\":{},\"seq\":2,\"catalog\":{{\"photos\":7}}}}\n", VERSION + 1).into_bytes(),
    );
    assert_refused_untouched(&m);
    let e = Journal::open(Box::new(m)).err().unwrap().to_string();
    assert!(e.contains(&format!("v{}", VERSION + 1)), "{e}");
}

/// A log record with a valid CRC whose op this version doesn't know: refused, not taken for a
/// torn tail (truncated) or damage (renamed) — wherever it is in the log.
#[test]
fn unknown_op_in_the_log_is_refused_untouched() {
    let (base, log, _) = parts();
    let future = |seq: u64| {
        let body = r#"{"op":"fromTheFuture","id":1,"what":"something new"}"#;
        format!("{{\"seq\":{seq},\"crc\":{},\"op\":{body}}}\n", crc32fast::hash(body.as_bytes()))
    };
    let snap = format!("{{\"format\":\"lightcraft-catalog\",\"version\":{VERSION},\"seq\":2,\"catalog\":{}}}\n", base.to_snapshot());
    let id = base.photos().next().unwrap().id;
    let after = format!("{}\n", encode_record(5, &Op::SetRating { id, rating: 1 }));
    for log in [format!("{log}{}", future(4)), format!("{log}{}{after}", future(4)), format!("{}{log}", future(3))] {
        let m = MemStore::new();
        m.set(SNAPSHOT, snap.clone().into_bytes());
        m.set(LOG, log.into_bytes());
        assert_refused_untouched(&m);
    }
    // a known op with a field of an unknown shape is "newer" too
    let body = r#"{"op":"setRating","id":1,"rating":"five stars"}"#;
    let m = MemStore::new();
    m.set(SNAPSHOT, snap.into_bytes());
    m.set(LOG, format!("{log}{{\"seq\":4,\"crc\":{},\"op\":{body}}}\n", crc32fast::hash(body.as_bytes())).into_bytes());
    assert_refused_untouched(&m);
}

/// A CRC mismatch is still damage / a torn tail (not "newer").
#[test]
fn bad_crc_is_still_a_torn_tail() {
    let (base, log, _) = parts();
    let m = MemStore::new();
    m.set(
        SNAPSHOT,
        format!("{{\"format\":\"lightcraft-catalog\",\"version\":{VERSION},\"seq\":2,\"catalog\":{}}}\n", base.to_snapshot()).into_bytes(),
    );
    let body = r#"{"op":"fromTheFuture"}"#;
    m.set(LOG, format!("{log}{{\"seq\":4,\"crc\":1,\"op\":{body}}}\n").into_bytes());
    let (_, _, r) = Journal::open(Box::new(m)).unwrap();
    assert!(r.torn_bytes > 0 && r.damaged.is_none(), "{r:?}");
}

/// Guard for the version rule (see `journal` → *Format versions*): adding an [`Op`] variant breaks
/// this match. When it does, bump `journal::VERSION`, add a row to the format table, and add the
/// variant here under the new version.
#[test]
fn op_variants_are_versioned() {
    fn since(op: &Op) -> u32 {
        match op {
            Op::AddPhoto { .. }
            | Op::RemovePhoto { .. }
            | Op::SetRating { .. }
            | Op::SetFlag { .. }
            | Op::SetLabel { .. }
            | Op::SetDevelop { .. }
            | Op::SetMeta { .. }
            | Op::SetDeleted { .. }
            | Op::SetLocal { .. }
            | Op::SetVersions { .. }
            | Op::SetHistory { .. }
            | Op::PushHistory { .. }
            | Op::AddAlbum { .. }
            | Op::RemoveAlbum { .. }
            | Op::RenameAlbum { .. }
            | Op::MoveAlbum { .. }
            | Op::SetAlbumPhotos { .. }
            | Op::SetAlbumCover { .. }
            | Op::SetAlbumRules { .. }
            | Op::AddStack { .. }
            | Op::RemoveStack { .. }
            | Op::SetStack { .. }
            | Op::SetCaptured { .. }
            | Op::SetAnalysis { .. }
            | Op::SetFile { .. }
            | Op::Relink { .. }
            | Op::SetContent { .. }
            | Op::SetLabelName { .. }
            | Op::Batch { .. } => 1,
            Op::SetBrowsed { .. } => 2,
        }
    }
    let newest = since(&Op::SetBrowsed { folder: String::new(), at: None });
    assert_eq!(newest, VERSION, "the newest op's version must be the current format version");
}
