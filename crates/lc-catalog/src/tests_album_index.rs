//! The album membership index ([`Catalog::album_has`]) agrees with the albums' photo lists
//! through every op, failed batches, undo / redo and journal replay.

use proptest::prelude::*;

use crate::*;

fn photo(c: &mut Catalog) -> PhotoId {
    let id = c.alloc_photo_id();
    c.apply(Op::AddPhoto { photo: Box::new(Photo::new(id, Source::File { path: format!("/p/{}.jpg", id.0) }, "p.jpg", "JPEG", 4, 3, "2026-01-01")) })
        .unwrap();
    id
}

/// Every (album, photo) pair, including ids nothing has, answers as the album's list does.
fn assert_index(c: &Catalog) {
    let max_photo = c.photos().map(|p| p.id.0).max().unwrap_or(0) + 2;
    let max_album = c.albums().map(|a| a.id.0).max().unwrap_or(0) + 2;
    for a in 1..=max_album {
        let al = c.album(AlbumId(a));
        for p in 1..=max_photo {
            let want = al.is_some_and(|al| !al.is_smart() && al.photos.contains(&PhotoId(p)));
            assert_eq!(c.album_has(AlbumId(a), PhotoId(p)), want, "album {a}, photo {p}");
        }
    }
    for p in c.photos() {
        let want: Vec<AlbumId> =
            c.albums().filter(|a| a.smart.as_ref().map_or(a.photos.contains(&p.id), |r| !p.deleted && r.matches(p, c))).map(|a| a.id).collect();
        assert_eq!(c.albums_of(p.id), want);
    }
}

/// A random membership change against the live catalog (may be invalid; then it changes nothing).
fn op_for(c: &mut Catalog, kind: u8, a: u8, b: u8) -> Op {
    let photos: Vec<PhotoId> = c.photos().map(|p| p.id).collect();
    let albums: Vec<AlbumId> = c.albums().map(|a| a.id).collect();
    let pid = photos.get(a as usize % photos.len().max(1)).copied().unwrap_or(PhotoId(99));
    let aid = albums.get(b as usize % albums.len().max(1)).copied().unwrap_or(AlbumId(99));
    let some = |m: u64| photos.iter().copied().filter(|p| (p.0 + u64::from(b)).is_multiple_of(m)).collect::<Vec<_>>();
    match kind % 9 {
        0 => {
            let id = c.alloc_photo_id();
            Op::AddPhoto { photo: Box::new(Photo::new(id, Source::Demo { scene: 0 }, "n.jpg", "JPEG", 4, 3, "2026-01-01")) }
        }
        1 => {
            let id = c.alloc_album_id();
            let folder = b.is_multiple_of(5);
            Op::AddAlbum { album: Album { folder, photos: if folder { vec![] } else { some(2) }, ..Album::new(id, "A") } }
        }
        2 => Op::SetAlbumPhotos { id: aid, photos: some(3) },
        // duplicates and ids of photos that aren't in the catalog
        3 => Op::SetAlbumPhotos { id: aid, photos: vec![pid, pid, PhotoId(1000 + u64::from(a))] },
        4 => Op::RemoveAlbum { id: aid },
        5 => c.delete_permanently_ops(pid),
        6 => c.delete_photos_permanently_ops(&some(4)),
        // a batch whose last op fails: the membership change before it is rolled back
        7 => Op::Batch { ops: vec![Op::SetAlbumPhotos { id: aid, photos: some(2) }, Op::RemovePhoto { id: PhotoId(5000) }] },
        _ => {
            let id = c.alloc_album_id();
            Op::AddAlbum { album: Album { smart: Some(Box::new(Filter { rating: 0, ..Default::default() })), ..Album::new(id, "S") } }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn index_follows_ops_undo_redo_and_replay(actions in proptest::collection::vec((0u8..12, 0u8..255, 0u8..255, 0u8..6), 1..60)) {
        let mut c = Catalog::new();
        for _ in 0..6 {
            photo(&mut c);
        }
        let start = c.clone();
        // built before anything changes, so the ops below must keep it up to date
        assert_index(&c);
        let (mut undo, mut redo, mut log): (Vec<Op>, Vec<Op>, Vec<Op>) = (vec![], vec![], vec![]);
        for (kind, a, b, ctl) in actions {
            match ctl {
                0 => {
                    if let Some(inv) = undo.pop() {
                        log.push(inv.clone());
                        redo.push(c.apply(inv).unwrap());
                    }
                }
                1 => {
                    if let Some(op) = redo.pop() {
                        log.push(op.clone());
                        undo.push(c.apply(op).unwrap());
                    }
                }
                _ => {
                    let op = op_for(&mut c, kind, a, b);
                    if let Ok(inv) = c.apply(op.clone()) {
                        log.push(op);
                        undo.push(inv);
                        redo.clear();
                    }
                }
            }
            assert_index(&c);
        }
        // replaying the log onto an indexed copy of the start keeps the index in step too
        let mut replayed = start.clone();
        assert_index(&replayed);
        let text: String = log.iter().map(Catalog::op_to_log_line).collect();
        replayed.replay(&text).unwrap();
        prop_assert_eq!(replayed.to_snapshot(), c.to_snapshot());
        assert_index(&replayed);
        // and a catalog loaded from the snapshot builds the same one
        assert_index(&Catalog::from_snapshot(&c.to_snapshot()).unwrap());
        // undoing everything returns to the start
        while let Some(inv) = undo.pop() {
            c.apply(inv).unwrap();
            assert_index(&c);
        }
        prop_assert!(c.photos().eq(start.photos()) && c.albums().eq(start.albums()));
    }
}

/// The index is derived state: it never reaches the snapshot, and catalogs compare equal whether
/// or not theirs was built.
#[test]
fn index_is_not_persisted_or_compared() {
    let mut c = Catalog::new();
    let p = photo(&mut c);
    let id = c.alloc_album_id();
    c.apply(Op::AddAlbum { album: Album { photos: vec![p], ..Album::new(id, "A") } }).unwrap();
    let mut fresh = Catalog::from_snapshot(&c.to_snapshot()).unwrap();
    fresh.revision = c.revision;
    assert!(c.album_has(id, p));
    assert_eq!(c, fresh);
    assert!(!c.to_snapshot().contains("index"));
    assert!(fresh.clone().album_has(id, p), "a clone builds its own");
}
