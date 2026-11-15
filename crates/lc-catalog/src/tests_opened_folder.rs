//! A library's files are written to the folder it was opened from, not to whatever its path leads
//! to later (a share mounted over the folder, or the folder moved away and another put at its
//! path). A folder's identity is its device and inode number, so the tests that move a folder run
//! on Unix.
#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::journal::{LOG, SNAPSHOT};
use crate::lock::OWNER;
use crate::*;

/// The start of the sentence a refused write ends with.
const REFUSED: &str = "is no longer the folder this library was opened from";

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lc-opened-folder-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The files in `dir` with their bytes, by name.
fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())
        })
        .collect();
    files.sort();
    files
}

/// Another library's folder where `dir` was, and the folder that was there moved to the path
/// returned: what a share mounted over `dir` looks like.
fn put_another_folder_at(dir: &Path) -> PathBuf {
    let away = dir.with_extension("away");
    std::fs::rename(dir, &away).unwrap();
    std::fs::create_dir(dir).unwrap();
    std::fs::write(dir.join(SNAPSHOT), b"their snapshot").unwrap();
    std::fs::write(dir.join(LOG), b"their log\n").unwrap();
    std::fs::write(dir.join(OWNER), b"their note").unwrap();
    away
}

/// A copy of `dir` (its note too) put at its path, the folder that was opened moved to the path
/// returned. `before_swap` runs between the copy and the swap.
fn put_a_copy_at(dir: &Path, before_swap: impl FnOnce()) -> PathBuf {
    let copy = dir.with_extension("copy");
    std::fs::create_dir(&copy).unwrap();
    for (name, bytes) in files_in(dir) {
        std::fs::write(copy.join(name), bytes).unwrap();
    }
    before_swap();
    let away = dir.with_extension("away");
    std::fs::rename(dir, &away).unwrap();
    std::fs::rename(&copy, dir).unwrap();
    away
}

/// Every way a store writes is refused in a folder that replaced the opened one, and nothing is
/// written in either folder.
#[test]
fn a_store_writes_nothing_into_a_folder_that_replaced_the_one_it_opened() {
    let base = scratch("store");
    let dir = base.join("lib");
    let mut store = FsStore::open(&dir).unwrap();
    store.write_atomic(SNAPSHOT, b"ours").unwrap();
    store.append(LOG, b"1\n").unwrap();
    let away = put_another_folder_at(&dir);
    let (theirs, ours) = (files_in(&dir), files_in(&away));

    let results = [
        ("write_atomic", store.write_atomic(SNAPSHOT, b"x")),
        ("write_atomic_with", store.write_atomic_with(SNAPSHOT, &mut |w| w.write_all(b"x")).map(|_| ())),
        ("write_atomic of the log", store.write_atomic(LOG, b"")),
        ("write_atomic of a settings file", store.write_atomic("prefs.json", b"{}")),
        ("truncate", store.truncate(LOG, 0)),
        ("append", store.append(LOG, b"2\n")),
    ];
    assert_eq!(files_in(&dir), theirs, "the folder at the path now is untouched");
    assert_eq!(files_in(&away), ours, "and so is the folder that was opened: saving stops");
    for (what, r) in results {
        let e = r.expect_err(what);
        assert!(e.to_string().contains(REFUSED), "{what}: {e}");
    }

    // the folder that was opened is back at the path: saving goes on, into it
    std::fs::rename(&dir, base.join("lib.theirs")).unwrap();
    std::fs::rename(&away, &dir).unwrap();
    store.append(LOG, b"2\n").expect("saving resumes");
    assert_eq!(std::fs::read(dir.join(LOG)).unwrap(), b"1\n2\n");
    let _ = std::fs::remove_dir_all(&base);
}

/// A compaction that starts after the folder was replaced writes through a second handle on the
/// same files: it is bound to the folder the store opened too.
#[test]
fn the_snapshot_writer_of_a_store_is_bound_to_the_folder_the_store_opened() {
    let base = scratch("worker");
    let dir = base.join("lib");
    let store = FsStore::open(&dir).unwrap();
    let _away = put_another_folder_at(&dir);
    let theirs = files_in(&dir);
    let mut worker = store.background_writer().expect("a store on disk has one");
    let e = worker.write_atomic_with(SNAPSHOT, &mut |w| w.write_all(b"ours")).expect_err("refused");
    assert!(e.to_string().contains(REFUSED), "{e}");
    assert_eq!(files_in(&dir), theirs);
    let _ = std::fs::remove_dir_all(&base);
}

/// A snapshot whose folder is swapped while it is being written is not published there.
#[test]
fn a_folder_swapped_during_a_streamed_save_is_not_written_to() {
    let base = scratch("stream-swap");
    let dir = base.join("lib");
    let mut store = FsStore::open(&dir).unwrap();
    let mut away = None;
    let result = store.write_atomic_with(SNAPSHOT, &mut |w| {
        w.write_all(b"our pending snapshot")?;
        away = Some(put_another_folder_at(&dir));
        Ok(())
    });
    assert!(result.expect_err("refused").to_string().contains(REFUSED));
    assert_eq!(std::fs::read(dir.join(SNAPSHOT)).unwrap(), b"their snapshot");
    assert_eq!(files_in(&dir).len(), 3, "no staging file is left in their folder");
    let _ = std::fs::remove_dir_all(&base);
}

/// A journal's save (the closing snapshot) in a folder that replaced the opened one fails with the
/// reason and writes nothing; with the folder back, the same call saves.
#[test]
fn a_journal_cannot_snapshot_into_a_folder_that_replaced_the_one_it_opened() {
    let base = scratch("journal");
    let dir = base.join("lib");
    let (mut journal, catalog, _) = Journal::open(Box::new(FsStore::open(&dir).unwrap())).unwrap();
    let away = put_another_folder_at(&dir);
    let theirs = files_in(&dir);
    let e = journal.snapshot(&catalog).expect_err("the path leads to another library's folder");
    assert!(e.to_string().contains(REFUSED), "{e}");
    assert_eq!(files_in(&dir), theirs);

    std::fs::rename(&dir, base.join("lib.theirs")).unwrap();
    std::fs::rename(&away, &dir).unwrap();
    journal.snapshot(&catalog).expect("the folder is back");
    let _ = std::fs::remove_dir_all(&base);
}

/// Dropping the lock leaves another program's owner note alone: one in a folder mounted over the
/// locked one, and one written over ours.
#[test]
fn the_lock_removes_only_its_own_note() {
    let base = scratch("lock-note");
    let dir = base.join("lib");
    std::fs::create_dir(&dir).unwrap();
    let lock = LibraryLock::acquire(&dir, "Local Image").unwrap();
    let _away = put_another_folder_at(&dir);
    drop(lock);
    assert_eq!(std::fs::read(dir.join(OWNER)).unwrap(), b"their note", "a note in another folder");

    let other = base.join("other");
    std::fs::create_dir(&other).unwrap();
    let lock = LibraryLock::acquire(&other, "Local Image").unwrap();
    std::fs::write(other.join(OWNER), b"written over ours").unwrap();
    drop(lock);
    assert_eq!(std::fs::read(other.join(OWNER)).unwrap(), b"written over ours", "a note that isn't ours");
    let _ = std::fs::remove_dir_all(&base);
}

/// A copy of the library's folder, with this session's note, put at the path counts as the
/// library: an append goes into the copy, never on through a file kept open in the folder that
/// was moved away.
#[test]
fn an_append_goes_into_a_note_bearing_copy_not_the_folder_moved_away() {
    let base = scratch("append-copy");
    let dir = base.join("lib");
    std::fs::create_dir(&dir).unwrap();
    let lock = LibraryLock::acquire(&dir, "Local Image").unwrap();
    let mut store = FsStore::in_folder(lock.folder().unwrap().clone());
    store.append(LOG, b"1\n").unwrap();
    let away = put_a_copy_at(&dir, || {});
    store.append(LOG, b"2\n").unwrap();
    assert_eq!(std::fs::read(dir.join(LOG)).unwrap(), b"1\n2\n", "the copy at the path gets it");
    assert_eq!(std::fs::read(away.join(LOG)).unwrap(), b"1\n", "the folder moved away doesn't");
    drop(lock);
    let _ = std::fs::remove_dir_all(&base);
}

/// The copy's log lacks what was appended after the copy was made: appending to it would leave a
/// gap, and cutting back a failed append must not touch it either. It is refused; the snapshot
/// the journal then takes replaces it and keeps every change.
#[test]
fn an_append_refuses_a_log_that_is_not_the_one_it_wrote_and_a_snapshot_keeps_every_change() {
    for longer in [false, true] {
        let base = scratch(&format!("log-length-{longer}"));
        let dir = base.join("lib");
        std::fs::create_dir(&dir).unwrap();
        let lock = LibraryLock::acquire(&dir, "Local Image").unwrap();
        let (mut journal, mut catalog, _) = Journal::open(Box::new(FsStore::in_folder(lock.folder().unwrap().clone()))).unwrap();
        let add = |n| Op::AddAlbum { album: Album::new(AlbumId(n), format!("album {n}")) };
        let apply = |catalog: &mut Catalog, n| {
            catalog.apply(add(n)).unwrap();
            add(n)
        };
        let first = apply(&mut catalog, 1);
        journal.append(&[first]).unwrap();
        let mut second = Some(apply(&mut catalog, 2));
        let copy = dir.with_extension("copy");
        let away = put_a_copy_at(&dir, || {
            journal.append(&[second.take().unwrap()]).unwrap();
            if longer {
                let mut bytes = std::fs::read(copy.join(LOG)).unwrap();
                bytes.extend_from_slice(&[b'x'; 4096]);
                std::fs::write(copy.join(LOG), bytes).unwrap();
            }
        });
        let replacement = std::fs::read(dir.join(LOG)).unwrap();
        let acknowledged = std::fs::read(away.join(LOG)).unwrap();
        let third = apply(&mut catalog, 3);
        let e = journal.append(&[third]).expect_err("the log at the path is not the one this session wrote");
        assert!(e.to_string().contains("it is another file"), "{e}");
        assert_eq!(std::fs::read(dir.join(LOG)).unwrap(), replacement, "the failed append's cleanup leaves it alone");
        assert_eq!(std::fs::read(away.join(LOG)).unwrap(), acknowledged);
        journal.snapshot_with_unlogged(&catalog, 1).unwrap();
        let fourth = apply(&mut catalog, 4);
        journal.append(&[fourth]).unwrap();
        drop(journal);
        drop(lock);
        let (reopened, loaded, _) = Journal::open(Box::new(FsStore::open(&dir).unwrap())).unwrap();
        assert_eq!(reopened.seq(), 4);
        for n in 1..=4 {
            assert_eq!(loaded.album(AlbumId(n)).unwrap().name, format!("album {n}"));
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}

/// A folder that holds this session's note is the opened one whatever its numbers say (some file
/// systems number folders anew at every mount); without the note it is refused.
#[test]
fn a_store_writes_into_a_folder_with_other_numbers_while_it_holds_this_sessions_note() {
    let dir = scratch("renumbered");
    let note = b"the note this session's lock wrote";
    let folder = OpenedFolder::recorded_under_other_numbers(&dir);
    folder.vouch(note);
    let mut store = FsStore::in_folder(folder);
    let e = store.write_atomic(SNAPSHOT, b"x").expect_err("other numbers, and no note in the folder");
    assert!(e.to_string().contains(REFUSED), "{e}");
    assert!(files_in(&dir).is_empty(), "nothing was written");
    std::fs::write(dir.join(OWNER), note).unwrap();
    store.write_atomic(SNAPSHOT, b"one").expect("the note vouches for the folder");
    store.append(LOG, b"1\n").unwrap();
    std::fs::write(dir.join(OWNER), b"theirs").unwrap();
    assert!(store.append(LOG, b"2\n").unwrap_err().to_string().contains(REFUSED));
    assert_eq!(std::fs::read(dir.join(LOG)).unwrap(), b"1\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A library reached through a link is the folder the link leads to: a link that stays is no
/// reason to refuse, and one pointed at another folder is.
#[test]
fn a_link_to_the_folder_is_followed() {
    let base = scratch("link");
    let (one, two, link) = (base.join("one"), base.join("two"), base.join("link"));
    std::fs::create_dir(&one).unwrap();
    std::fs::create_dir(&two).unwrap();
    std::os::unix::fs::symlink(&one, &link).unwrap();
    let folder = OpenedFolder::at(&link).unwrap();
    folder.check().expect("the link still leads to it");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&two, &link).unwrap();
    assert!(folder.check().is_err(), "the link leads to another folder now");
    let _ = std::fs::remove_dir_all(&base);
}
