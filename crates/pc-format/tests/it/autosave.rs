//! Background autosave and crash recovery.

use crate::common;
use std::sync::Arc;

use common::*;
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{DocId, Document};
use photocraft_format::autosave::RecoveryInfo;
use photocraft_format::*;

#[cfg(unix)]
#[test]
fn recovery_folders_and_sidecars_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp_dir("private-recovery");
    let dir = root.join("Recovery");
    let doc = Document::new("Private edits", photocraft_doc::Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
    save_once(&dir, "private", doc.clone());
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("private.pcraft")), 0o700);
    assert_eq!(mode(&dir.join("private.json")), 0o600);
    std::fs::set_permissions(dir.join("private.json"), std::fs::Permissions::from_mode(0o644)).unwrap();
    save_once(&dir, "private", doc.clone());
    assert_eq!(mode(&dir.join("private.json")), 0o600);
    assert_eq!(recover(&list_recovery(&dir)[0]).unwrap(), doc);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_listing_ignores_oversized_and_non_regular_sidecars() {
    let dir = temp_dir("bounded-recovery");
    let doc = Document::new("Metadata", photocraft_doc::Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
    save_once(&dir, "bounded", doc);
    let sidecar = dir.join("bounded.json");
    let bytes = std::fs::read(&sidecar).unwrap();
    let mut oversized = vec![b' '; 1 << 20];
    oversized.extend(&bytes);
    std::fs::write(&sidecar, oversized).unwrap();
    assert!(list_recovery(&dir).is_empty(), "oversized metadata was accepted");
    #[cfg(unix)]
    {
        let outside = dir.join("metadata.txt");
        std::fs::write(&outside, &bytes).unwrap();
        std::fs::remove_file(&sidecar).unwrap();
        std::os::unix::fs::symlink(&outside, &sidecar).unwrap();
        assert!(list_recovery(&dir).is_empty(), "symlinked metadata was accepted");
        std::fs::remove_file(&sidecar).unwrap();
        assert!(std::process::Command::new("mkfifo").arg(&sidecar).status().unwrap().success());
        assert!(list_recovery(&dir).is_empty(), "FIFO metadata was accepted");
    }
    std::fs::remove_file(&sidecar).unwrap();
    std::fs::write(&sidecar, bytes).unwrap();
    assert_eq!(list_recovery(&dir).len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recovery_discard_rejects_path_components() {
    let root = temp_dir("recovery-key");
    let dir = root.join("Recovery");
    let outside = root.join("outside.pcraft");
    std::fs::create_dir(&dir).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("canary"), b"untouched").unwrap();
    std::fs::write(root.join("outside.json"), b"untouched").unwrap();
    let entry = RecoveryEntry {
        info: RecoveryInfo { key: "../outside".into(), document_name: String::new(), original_path: None, saved_at: 0, revision: 0 },
        bundle: outside.clone(),
    };
    assert!(discard_recovery(&dir, &entry).is_err());
    assert_eq!(std::fs::read(outside.join("canary")).unwrap(), b"untouched");
    assert_eq!(std::fs::read(root.join("outside.json")).unwrap(), b"untouched");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn autosave_then_recover() {
    let dir = temp_dir("recovery");
    let doc = Arc::new(rich_doc(ColorMode::Rgb, SampleType::U16));
    let saver = Autosaver::new(&dir, "doc-1");
    saver.request(doc.clone(), 7, Some("/work/a.pcraft".into()), SaveOptions::default());
    let r = saver.flush().expect("a save ran");
    let stats = r.unwrap();
    assert!(stats.tiles_written > 0);
    let entries = list_recovery(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].info.revision, 7);
    assert_eq!(entries[0].info.original_path.as_deref(), Some("/work/a.pcraft"));
    assert_eq!(entries[0].info.document_name, "Rich");
    assert_eq!(recover(&entries[0]).unwrap(), *doc);
    discard_recovery(&dir, &entries[0]).unwrap();
    assert!(list_recovery(&dir).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repeated_autosaves_are_incremental_and_coalesced() {
    let dir = temp_dir("coalesce");
    let saver = Autosaver::new(&dir, "k");
    let mut doc = rich_doc(ColorMode::Rgb, SampleType::U8);
    for i in 0..5 {
        let id = doc.layers[1].id;
        doc.layer_mut(id).unwrap().surface_mut().unwrap().write_pixel(i, 0, &[1.0, 0.0, 0.0, 1.0]);
        saver.request(Arc::new(doc.clone()), i as u64, None, SaveOptions::default());
    }
    saver.flush().unwrap().unwrap();
    let e = list_recovery(&dir);
    assert_eq!(e[0].info.revision, 4, "newest snapshot wins");
    assert_eq!(recover(&e[0]).unwrap(), doc);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn discard_removes_everything() {
    let dir = temp_dir("discard");
    let saver = Autosaver::new(&dir, "x y/z");
    saver.request(Arc::new(rich_doc(ColorMode::Grayscale, SampleType::U8)), 1, None, SaveOptions::default());
    let path = saver.bundle_path();
    assert!(path.file_name().unwrap().to_string_lossy().starts_with("x_y_z"));
    saver.discard().unwrap().wait();
    assert!(list_recovery(&dir).is_empty());
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_ignores_junk() {
    let dir = temp_dir("junk");
    std::fs::write(dir.join("bogus.json"), b"{}").unwrap();
    std::fs::write(dir.join("other.txt"), b"x").unwrap();
    assert!(list_recovery(&dir).is_empty());
    assert!(list_recovery(&dir.join("missing")).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

/// `doc` as document `id` with pixel (x, 0) of its paint layer set, so versions are told apart.
fn edit(mut doc: Document, id: u64, x: i32) -> Document {
    doc.id = DocId(id);
    let layer = doc.layers[1].id;
    doc.layer_mut(layer).unwrap().surface_mut().unwrap().write_pixel(x, 0, &[1.0, 0.0, 0.0, 1.0]);
    doc
}

fn version(id: u64, x: i32, mode: ColorMode) -> Document {
    edit(rich_doc(mode, SampleType::U8), id, x)
}

/// Save one snapshot under `key` and wait for it.
fn save_once(dir: &std::path::Path, key: &str, doc: Document) {
    let saver = Autosaver::new(dir, key);
    saver.request(Arc::new(doc), 1, None, SaveOptions::default());
    saver.flush().unwrap().unwrap();
}

/// The recovered documents by entry key, ids cleared (they're reassigned on open).
fn recovered(store: &RecoveryStore) -> std::collections::BTreeMap<String, Document> {
    store
        .recover()
        .into_iter()
        .map(|(e, mut doc)| {
            doc.id = DocId(0);
            (e.info.key, doc)
        })
        .collect()
}

fn same(mut doc: Document) -> Document {
    doc.id = DocId(0);
    doc
}

#[test]
fn recovered_entries_survive_a_second_crash_until_saved_or_closed() {
    let dir = temp_dir("second-crash");
    let (a, b) = (version(1, 3, ColorMode::Rgb), version(2, 5, ColorMode::Grayscale));
    // Launch 1 autosaves two unsaved documents, then crashes (dropping waits for the writes).
    {
        let mut launch = RecoveryStore::new(&dir);
        launch.autosave(&Arc::new(a.clone()), 4, None);
        launch.autosave(&Arc::new(b.clone()), 6, Some("/work/b.pcraft".into()));
    }
    assert_eq!(list_recovery(&dir).len(), 2);
    // Launch 2 recovers both and crashes before its first autosave: nothing may be lost.
    let keys: Vec<String> = {
        let mut launch = RecoveryStore::new(&dir);
        let docs = recovered(&launch);
        assert_eq!(docs.len(), 2);
        assert_eq!(list_recovery(&dir).len(), 2, "recovering must not delete the entries");
        for (i, key) in docs.keys().enumerate() {
            launch.adopt(10 + i as u64, key);
        }
        docs.into_keys().collect()
    };
    // Launch 3 still has both, pixels intact; one is edited and autosaved, the other closed.
    let edited = edit(a.clone(), 20, 9);
    {
        let mut launch = RecoveryStore::new(&dir);
        let docs = recovered(&launch);
        assert_eq!(docs.values().cloned().collect::<Vec<_>>(), vec![same(a.clone()), same(b.clone())]);
        launch.adopt(20, &keys[0]);
        launch.adopt(21, &keys[1]);
        launch.autosave(&Arc::new(edited.clone()), 9, None);
        launch.discard(21).unwrap().wait();
        assert_eq!(list_recovery(&dir).len(), 1, "closing removes the adopted entry");
    }
    // The autosave replaced the adopted entry in place: no duplicate, newest content.
    let entries = list_recovery(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].info.key.as_str(), entries[0].info.revision), (keys[0].as_str(), 9));
    // Launch 4 recovers it once more and saves it: the entry goes.
    let mut launch = RecoveryStore::new(&dir);
    assert_eq!(recovered(&launch).into_values().collect::<Vec<_>>(), vec![same(edited)]);
    launch.adopt(30, &keys[0]);
    launch.discard(30).unwrap().wait();
    assert!(list_recovery(&dir).is_empty());
    drop(launch);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn new_documents_never_overwrite_surviving_entries() {
    let dir = temp_dir("collision");
    // An earlier launch (including builds that keyed entries by bare document id) left `doc-1`.
    let old = version(1, 3, ColorMode::Rgb);
    save_once(&dir, "doc-1", old.clone());
    // A store from another launch left an entry for its own document 1 too.
    let other = version(1, 6, ColorMode::Grayscale);
    RecoveryStore::new(&dir).autosave(&Arc::new(other.clone()), 2, None);
    assert_eq!(list_recovery(&dir).len(), 2);
    // This launch adopts `doc-1` for its document 5, and its own new document 1 autosaves.
    let fresh = version(1, 8, ColorMode::Rgb);
    {
        let mut launch = RecoveryStore::new(&dir);
        launch.adopt(5, "doc-1");
        launch.autosave(&Arc::new(fresh.clone()), 3, None);
    }
    let docs = recovered(&RecoveryStore::new(&dir));
    assert_eq!(docs.len(), 3, "the new document got an entry of its own");
    assert_eq!(docs.get("doc-1"), Some(&same(old)), "the adopted entry is untouched");
    assert!(docs.values().any(|d| *d == same(other.clone())));
    assert!(docs.values().any(|d| *d == same(fresh.clone())));
    // Closing the new document removes only its own entry.
    let mut launch = RecoveryStore::new(&dir);
    launch.adopt(5, "doc-1");
    launch.autosave(&Arc::new(fresh), 3, None);
    launch.discard(1).unwrap().wait();
    assert_eq!(list_recovery(&dir).len(), 3);
    launch.discard(5).unwrap().wait();
    assert_eq!(list_recovery(&dir).len(), 2);
    drop(launch);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sidecars_cannot_point_at_other_bundles() {
    let dir = temp_dir("sidecar-key");
    let inner = dir.join("Recovery");
    save_once(&dir, "outside", rich_doc(ColorMode::Rgb, SampleType::U8));
    save_once(&inner, "k", rich_doc(ColorMode::Rgb, SampleType::U8));
    let info = std::fs::read_to_string(inner.join("k.json")).unwrap();
    // A copied sidecar (its name isn't its key) and one whose key leaves the directory.
    std::fs::write(inner.join("copy.json"), &info).unwrap();
    std::fs::write(inner.join("escape.json"), info.replace("\"k\"", "\"../outside\"")).unwrap();
    let entries = list_recovery(&inner);
    assert_eq!(entries.iter().map(|e| e.info.key.as_str()).collect::<Vec<_>>(), ["k"]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Once the manifest is committed the save has succeeded: an unreferenced object that cannot be
/// removed (here a folder squatting on a tile name, like a locked or vanished file) must not turn
/// the directory save (autosave's path) into a reported failure. Ported from PhotoCraft c20b81b2.
#[test]
fn failed_cleanup_after_commit_does_not_fail_the_save() {
    let dir = temp_dir("cleanup-best-effort").join("doc.pcraft");
    std::fs::create_dir_all(&dir).unwrap();
    let doc = rich_doc(ColorMode::Rgb, SampleType::U8);
    let mut w = PcraftWriter::new();
    w.save_dir(&doc, &dir, &SaveOptions::default()).unwrap();
    let stuck = dir.join("tiles").join("0000000000000000000000000000000000000000000000000000000000000000.zst");
    std::fs::create_dir_all(stuck.join("inner")).unwrap();
    let stats = w.save_dir(&doc, &dir, &SaveOptions::default()).expect("the committed save reports success");
    assert_eq!(stats.objects_removed, 0, "nothing was actually removed");
    assert_eq!(load_path(&dir).unwrap(), doc);
    std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

/// A session about to end without asking (its window's graphics device was lost) writes
/// everything queued before it returns, so the next launch recovers it.
#[test]
fn flush_writes_every_queued_autosave_before_returning() {
    let dir = temp_dir("flush");
    let (a, b) = (version(1, 3, ColorMode::Rgb), version(2, 5, ColorMode::Grayscale));
    let mut store = RecoveryStore::new(&dir);
    store.autosave_checked(&Arc::new(a.clone()), 4, None).unwrap();
    store.autosave_checked(&Arc::new(b.clone()), 6, Some("/work/b.pcraft".into())).unwrap();
    let mut done = store.flush();
    done.sort_by_key(|(id, ..)| *id);
    assert_eq!(done, vec![(1, 4, Ok(())), (2, 6, Ok(()))]);
    // On disk while the store is still alive, not only once it is dropped.
    let docs = recovered(&RecoveryStore::new(&dir));
    assert_eq!(docs.len(), 2);
    assert!(docs.values().any(|d| *d == same(a.clone())) && docs.values().any(|d| *d == same(b.clone())));
    // A later autosave (trying a failed write again) starts a new worker on the same entry.
    let newer = version(1, 4, ColorMode::Rgb);
    store.autosave_checked(&Arc::new(newer.clone()), 5, None).unwrap();
    assert_eq!(store.flush(), vec![(1, 5, Ok(()))]);
    let docs = recovered(&RecoveryStore::new(&dir));
    assert_eq!(docs.len(), 2, "the same entry, not a new one");
    assert!(docs.values().any(|d| *d == same(newer.clone())) && !docs.values().any(|d| *d == same(a.clone())));
    drop(store);
    std::fs::remove_dir_all(dir).unwrap();
}
