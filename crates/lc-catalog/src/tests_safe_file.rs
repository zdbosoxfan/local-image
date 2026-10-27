//! Shared file writes through [`li_fs`] and [`crate::FsStore`]: catalog destination links are
//! resolved and preserved; durable replacement keeps modes and exclusively creates staging files.
//! No-sync writes suit caches; new-file publication never replaces an existing name, including
//! on unsupported volumes.
use li_fs::{
    fail_writes_after, same_file, syncs_on_this_thread, write_atomic, write_atomic_nosync, write_atomic_with, write_new, write_new_unique,
    write_new_with,
};

mod tests {
    use super::*;
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lc-safe-file-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        v.sort();
        v
    }

    #[test]
    fn catalog_adapter_uses_shared_fault_and_sync_policies() {
        let d = temp("adapter");
        let p = d.join("catalog.json");
        let before = li_fs::syncs_on_this_thread();
        write_atomic(&p, b"original").unwrap();
        assert!(li_fs::syncs_on_this_thread() > before);
        {
            let _fault = li_fs::fail_writes_after(3);
            assert!(write_atomic(&p, b"replacement").is_err());
        }
        assert_eq!(fs::read(&p).unwrap(), b"original");
        assert_eq!(names(&d), vec!["catalog.json"]);
        let before = syncs_on_this_thread();
        write_atomic_nosync(&p, b"cache").unwrap();
        assert_eq!(syncs_on_this_thread(), before);
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let d = temp("replace");
        let p = d.join("out.jpg");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two two");
        assert_eq!(names(&d), vec!["out.jpg"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failure_mid_write_keeps_the_old_file() {
        let d = temp("midwrite");
        let p = d.join("photo.jpg");
        fs::write(&p, b"the original bytes").unwrap();
        {
            let _f = fail_writes_after(4);
            let e = write_atomic(&p, &[7u8; 10_000]).unwrap_err();
            assert!(e.to_string().contains("injected"), "{e}");
            assert!(write_new(&d.join("new.dng"), &[1u8; 100]).is_err());
        }
        assert_eq!(fs::read(&p).unwrap(), b"the original bytes", "the target is untouched");
        assert_eq!(names(&d), vec!["photo.jpg"], "no temp file and no partial new file");
        // a fill error aborts the same way
        let r = write_atomic_with(&p, &mut |w| {
            w.write_all(b"half")?;
            Err(io::Error::other("encoder failed"))
        });
        assert!(r.is_err());
        assert_eq!(fs::read(&p).unwrap(), b"the original bytes");
        assert_eq!(names(&d), vec!["photo.jpg"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn recreatable_outputs_are_atomic_without_a_sync() {
        let d = temp("nosync");
        let p = d.join("export.jpg");
        let before = syncs_on_this_thread();
        write_atomic_nosync(&p, b"one").unwrap();
        write_atomic_nosync(&p, b"two two").unwrap();
        assert_eq!(syncs_on_this_thread(), before, "no file or folder sync");
        assert_eq!(fs::read(&p).unwrap(), b"two two");
        assert_eq!(names(&d), vec!["export.jpg"], "no temp file left");
        // still atomic: a failure part-way leaves the previous file and no temp file
        {
            let _f = fail_writes_after(3);
            assert!(write_atomic_nosync(&p, &[9u8; 10_000]).is_err());
        }
        assert_eq!(fs::read(&p).unwrap(), b"two two");
        assert_eq!(names(&d), vec!["export.jpg"]);
        // the durable writers do sync (the file; the folder too where it can be opened)
        let before = syncs_on_this_thread();
        write_atomic(&p, b"three").unwrap();
        let mid = syncs_on_this_thread();
        assert!(mid > before);
        write_new(&d.join("new.dng"), b"dng").unwrap();
        assert!(syncs_on_this_thread() > mid);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn new_files_never_replace_anything() {
        let d = temp("new");
        let p = d.join("a.dng");
        fs::write(&p, b"keep").unwrap();
        assert_eq!(write_new(&p, b"other").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&p).unwrap(), b"keep");
        let mut c = (1..).map(|k| if k == 1 { d.join("a.dng") } else { d.join(format!("a-{k}.dng")) });
        let got = write_new_unique(&mut c, b"fresh").unwrap();
        assert_eq!(got, d.join("a-2.dng"));
        assert_eq!(fs::read(&got).unwrap(), b"fresh");
        assert_eq!(names(&d), vec!["a-2.dng", "a.dng"], "the temp file is gone");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn same_file_sees_through_other_spellings() {
        let d = temp("same");
        let p = d.join("x.jpg");
        fs::write(&p, b"x").unwrap();
        fs::write(d.join("y.jpg"), b"x").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        assert!(same_file(&p, &d.join("sub/../x.jpg")));
        assert!(!same_file(&p, &d.join("y.jpg")));
        assert!(!same_file(&p, &d.join("missing.jpg")));
        let _ = fs::remove_dir_all(&d);
    }
}

#[allow(clippy::unwrap_used)]
mod streaming_tests {
    use super::*;
    use std::{fs, io};
    #[test]
    fn streaming_new_file_checks_bytes_before_publication_and_cleans_failures() {
        let root = std::env::temp_dir().join(format!("li-stream-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("original.raw");
        let before = syncs_on_this_thread();
        write_new_with(&path, &mut |w| w.write_all(b"original"), &mut |tmp| {
            assert!(!path.exists());
            assert_eq!(fs::read(tmp)?, b"original");
            Ok(())
        })
        .unwrap();
        assert!(syncs_on_this_thread() > before);
        assert_eq!(write_new_with(&path, &mut |w| w.write_all(b"replacement"), &mut |_| Ok(())).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let corrupt = root.join("corrupt.raw");
        assert!(write_new_with(&corrupt, &mut |w| w.write_all(b"bad copy"), &mut |_| Err(io::Error::other("different"))).is_err());
        assert!(!corrupt.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

mod store_writer_tests {
    use super::*;
    use crate::Store;
    use std::fs;
    #[test]
    fn fs_store_uses_shared_atomic_writer_for_settings_and_snapshots() {
        let root = std::env::temp_dir().join(format!("li-store-safe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut store = crate::FsStore::open(&root).unwrap();
        store.write_atomic("prefs.json", b"previous").unwrap();
        let fault = fail_writes_after(3);
        assert!(store.write_atomic("prefs.json", b"replacement").is_err());
        assert!(store.write_atomic_with("prefs.json", &mut |w| w.write_all(b"snapshot")).is_err());
        drop(fault);
        assert_eq!(store.read("prefs.json").unwrap().unwrap(), b"previous");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(unix)]
mod store_link_tests {
    use crate::{FsStore, Store};

    #[test]
    fn catalog_writes_preserve_links_and_update_resolved_targets() {
        use std::{fs, os::unix::fs::symlink};

        let root = std::env::temp_dir().join(format!("lc-store-linked-targets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let library = root.join("library");
        let moved = root.join("moved");
        fs::create_dir_all(&moved).unwrap();
        let mut store = FsStore::open(&library).unwrap();
        for name in ["catalog.log", "catalog.snap", "catalog.log.damaged-1"] {
            let target = moved.join(name);
            let link = library.join(name);
            let relative = std::path::Path::new("../moved").join(name);
            fs::write(&target, b"old").unwrap();
            symlink(&relative, &link).unwrap();

            store.write_atomic(name, b"replacement").unwrap();
            assert_eq!(fs::read_link(&link).unwrap(), relative);
            assert_eq!(fs::read(&target).unwrap(), b"replacement");
            store.append(name, b" appended").unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"replacement appended");
            store.truncate(name, 3).unwrap();
            assert_eq!(fs::read(&target).unwrap(), b"rep");
            let length = store
                .write_atomic_with(name, &mut |writer| {
                    assert_eq!(fs::read(&target)?, b"rep");
                    assert_eq!(fs::read_dir(&library)?.count(), 1, "staging stays outside the library");
                    assert_eq!(fs::read_dir(&moved)?.count(), 2, "staging is beside the resolved target");
                    writer.write_all(b"streamed")
                })
                .unwrap();
            assert_eq!(length, 8);
            assert_eq!(fs::read(&target).unwrap(), b"streamed");
            {
                let _fault = super::fail_writes_after(3);
                let error = store.write_atomic(name, b"replacement").unwrap_err();
                assert!(error.to_string().contains("injected"), "{error}");
                let error = store.write_atomic_with(name, &mut |writer| writer.write_all(b"replacement")).unwrap_err();
                assert!(error.to_string().contains("injected"), "{error}");
            }
            assert_eq!(fs::read_link(&link).unwrap(), relative);
            assert_eq!(fs::read(&target).unwrap(), b"streamed");
            assert_eq!(fs::read_dir(&library).unwrap().count(), 1);
            assert_eq!(fs::read_dir(&moved).unwrap().count(), 1, "failed staging files are removed");
            fs::remove_file(link).unwrap();
            fs::remove_file(target).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catalog_writes_preserve_destination_links_and_refuse_staging_links() {
        use li_fs::{AtomicFs, RealFs};

        let dir = std::env::temp_dir().join(format!("lc-store-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target");
        let mut store = FsStore::open(&dir).unwrap();
        for name in ["catalog.log", "catalog.snap", "catalog.log.damaged-1"] {
            std::fs::write(&target, b"keep").unwrap();
            std::os::unix::fs::symlink(&target, dir.join(name)).unwrap();
            store.append(name, b"appended").unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"keepappended");
            store.truncate(name, 0).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"");
            store.write_atomic(name, b"replacement").unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
            store.write_atomic_with(name, &mut |w| w.write_all(b"streamed")).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"streamed");
            assert_eq!(std::fs::read_link(dir.join(name)).unwrap(), target);
            std::fs::remove_file(dir.join(name)).unwrap();
            store.append(name, b"abc").unwrap();
            store.truncate(name, 1).unwrap();
            assert_eq!(std::fs::read(dir.join(name)).unwrap(), b"a");
            store.write_atomic(name, b"new").unwrap();
            store.write_atomic_with(name, &mut |w| w.write_all(b"streamed")).unwrap();
            assert_eq!(std::fs::read(dir.join(name)).unwrap(), b"streamed");
        }
        let staging = dir.join("catalog.snap.tmp");
        std::os::unix::fs::symlink(&target, &staging).unwrap();
        assert_eq!(RealFs.write_new(&staging, b"replacement").unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&target).unwrap(), b"streamed");
        assert_eq!(std::fs::read_link(&staging).unwrap(), target);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(unix)]
mod journal_link_tests {
    use crate::journal::{LOG, SNAPSHOT, encode_record};
    use crate::{FsStore, Journal, Op};
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    fn library(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lc-journal-links-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (mut journal, catalog, _) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        journal.snapshot(&catalog).unwrap();
        root
    }

    #[test]
    fn unrelated_log_link_is_untouched_and_blocks_edits_and_close() {
        let root = library("unrelated");
        let target = root.join("notes.txt");
        let bytes = b"These are unrelated notes, not a catalog.";
        fs::write(&target, bytes).unwrap();
        fs::remove_file(root.join(LOG)).unwrap();
        symlink(&target, root.join(LOG)).unwrap();
        let snapshot = fs::read(root.join(SNAPSHOT)).unwrap();
        let (mut journal, catalog, _) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        assert_eq!(fs::read(&target).unwrap(), bytes);
        let op = Op::Batch { ops: vec![] };
        let error = journal.append(&[op]).unwrap_err().to_string();
        assert!(error.contains(&root.join(LOG).display().to_string()) && error.contains(&target.display().to_string()), "{error}");
        assert_eq!(journal.snapshot(&catalog).unwrap_err().to_string(), error);
        assert_eq!(journal.snapshot_with_unlogged(&catalog, 1).unwrap_err().to_string(), error);
        assert_eq!(journal.snapshot_in_background(&catalog).unwrap_err().to_string(), error);
        assert_eq!(fs::read(root.join(SNAPSHOT)).unwrap(), snapshot);
        assert_eq!(fs::read(&target).unwrap(), bytes);
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn planted_damage_copy_blocks_repair_edits_and_close() {
        let root = library("damage-copy");
        let op = Op::Batch { ops: vec![] };
        let bytes = format!("{}\nbad record\n{}\n", encode_record(1, &op), encode_record(2, &op));
        fs::write(root.join(LOG), &bytes).unwrap();
        let target = root.join("notes.txt");
        fs::write(&target, b"keep my notes").unwrap();
        let copy = root.join("catalog.log.damaged-1");
        symlink(&target, &copy).unwrap();
        let snapshot = fs::read(root.join(SNAPSHOT)).unwrap();
        let (mut journal, catalog, _) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"keep my notes");
        let error = journal.append(&[op]).unwrap_err().to_string();
        assert!(error.contains(&copy.display().to_string()) && error.contains(&target.display().to_string()), "{error}");
        assert_eq!(journal.snapshot(&catalog).unwrap_err().to_string(), error);
        assert_eq!(journal.snapshot_in_background(&catalog).unwrap_err().to_string(), error);
        assert_eq!(fs::read(root.join(LOG)).unwrap(), bytes.as_bytes());
        assert_eq!(fs::read(root.join(SNAPSHOT)).unwrap(), snapshot);
        assert_eq!(fs::read(&target).unwrap(), b"keep my notes");
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn relocated_log_supports_edits_compaction_and_torn_tail_recovery() {
        let root = library("relocated");
        let moved = root.join("another-disk");
        fs::create_dir(&moved).unwrap();
        let target = moved.join(LOG);
        fs::rename(root.join(LOG), &target).unwrap();
        let link = root.join(LOG);
        symlink(&target, &link).unwrap();
        let op = Op::Batch { ops: vec![] };
        let (mut journal, mut catalog, _) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        catalog.apply(op.clone()).unwrap();
        journal.append(std::slice::from_ref(&op)).unwrap();
        assert_eq!(fs::read(&target).unwrap(), format!("{}\n", encode_record(1, &op)).as_bytes());
        journal.snapshot_in_background(&catalog).unwrap();
        journal.wait().unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"");
        drop(journal);
        let fragment = b"{\"seq\":2,\"cr";
        fs::write(&target, fragment).unwrap();
        let (mut journal, catalog, report) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        assert_eq!(report.torn_bytes, fragment.len() as u64);
        assert_eq!(fs::read(&target).unwrap(), b"");
        journal.append(std::slice::from_ref(&op)).unwrap();
        drop(journal);
        let record = format!("{}\n", encode_record(2, &op));
        fs::write(&target, format!("{record}crash residue")).unwrap();
        let (mut journal, _, report) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        assert_eq!(report.replayed, 1);
        assert_eq!(report.torn_bytes, 13);
        assert_eq!(fs::read(&target).unwrap(), record.as_bytes());
        journal.snapshot(&catalog).unwrap();
        assert_eq!(fs::read_link(link).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), b"");
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn linked_log_accepts_only_records_blank_lines_and_crash_residue() {
        let op = Op::Batch { ops: vec![] };
        let record = encode_record(1, &op);
        let gap = encode_record(3, &op);
        let cases = [
            ("empty", String::new(), true),
            ("blank", "\n \r\n".into(), true),
            ("first-torn", "{\"seq\":".into(), true),
            ("complete", record.clone(), true),
            ("later-torn", format!("{record}\nresidue"), true),
            ("blank-residue", format!("{record}\n "), true),
            ("text", "notes\n".into(), false),
            ("blank-tail", " ".into(), false),
            ("blank-text", "\nnotes".into(), false),
            ("bad-terminated", format!("{record}\nresidue\n"), false),
            ("gap", format!("{record}\n{gap}\n"), false),
        ];
        for (tag, bytes, valid) in cases {
            let root = library(tag);
            let target = root.join("target");
            fs::write(&target, &bytes).unwrap();
            fs::remove_file(root.join(LOG)).unwrap();
            symlink(&target, root.join(LOG)).unwrap();
            let (mut journal, catalog, report) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
            assert_eq!(report.write_refusal.is_none(), valid, "{tag}: {report:?}");
            if valid {
                journal.append(std::slice::from_ref(&op)).unwrap();
                let saved = fs::read_to_string(&target).unwrap();
                assert!(saved.lines().all(|line| line.trim().is_empty() || crate::journal::decode_record(line).is_some()), "{tag}: {saved}");
                journal.snapshot(&catalog).unwrap();
                assert_eq!(fs::read(&target).unwrap(), b"");
            } else {
                assert!(journal.append(std::slice::from_ref(&op)).is_err(), "{tag}");
                assert!(journal.snapshot(&catalog).is_err(), "{tag}");
                assert_eq!(fs::read(&target).unwrap(), bytes.as_bytes(), "{tag}");
            }
            assert_eq!(fs::read_link(root.join(LOG)).unwrap(), target);
            drop(journal);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn linked_log_unknown_ops_are_refused_untouched() {
        use crate::CatalogError;

        let body = r#"{"op":"fromTheFuture"}"#;
        let future = format!("{{\"seq\":1,\"crc\":{},\"op\":{body}}}\n", crc32fast::hash(body.as_bytes()));
        for prefix in ["", "bad record\n"] {
            let root = library("newer-log");
            let target = root.join("moved-log");
            let bytes = format!("{prefix}{future}");
            fs::write(&target, &bytes).unwrap();
            fs::remove_file(root.join(LOG)).unwrap();
            symlink(&target, root.join(LOG)).unwrap();
            let snapshot = fs::read(root.join(SNAPSHOT)).unwrap();

            let error = Journal::open(Box::new(FsStore::open(&root).unwrap())).err();
            assert_eq!(fs::read(&target).unwrap(), bytes.as_bytes());
            assert_eq!(fs::read(root.join(SNAPSHOT)).unwrap(), snapshot);
            assert_eq!(fs::read_link(root.join(LOG)).unwrap(), target);
            assert!(matches!(error, Some(CatalogError::Newer(_))), "{prefix:?}: {error:?}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn linked_snapshot_versions_are_refused_untouched() {
        use crate::{CatalogError, journal::VERSION};

        for version in [VERSION + 1, VERSION - 1] {
            let root = library("snapshot-version");
            let link = root.join(SNAPSHOT);
            let target = root.join("moved-snapshot");
            let mut snapshot: serde_json::Value = serde_json::from_slice(&fs::read(&link).unwrap()).unwrap();
            snapshot["version"] = version.into();
            let bytes = serde_json::to_vec(&snapshot).unwrap();
            fs::write(&target, &bytes).unwrap();
            fs::remove_file(&link).unwrap();
            symlink(&target, &link).unwrap();
            let log = fs::read(root.join(LOG)).unwrap();

            let error = Journal::open(Box::new(FsStore::open(&root).unwrap())).err();
            assert_eq!(fs::read(&target).unwrap(), bytes);
            assert_eq!(fs::read(root.join(LOG)).unwrap(), log);
            assert_eq!(fs::read_link(&link).unwrap(), target);
            if version > VERSION {
                assert!(matches!(error, Some(CatalogError::Newer(_))), "{error:?}");
            } else {
                assert!(matches!(error, Some(CatalogError::Older(_))), "{error:?}");
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn snapshot_links_require_a_decodable_catalog() {
        let root = library("snapshot");
        let link = root.join(SNAPSHOT);
        let target = root.join("moved-snapshot");
        fs::rename(&link, &target).unwrap();
        symlink(&target, &link).unwrap();
        let (mut journal, catalog, _) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        journal.snapshot(&catalog).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), target);
        drop(journal);
        fs::write(&target, b"not a snapshot").unwrap();
        let (mut journal, catalog, report) = Journal::open(Box::new(FsStore::open(&root).unwrap())).unwrap();
        assert!(report.write_refusal.as_ref().unwrap().contains(&target.display().to_string()));
        assert!(journal.snapshot(&catalog).is_err());
        assert!(journal.append(&[Op::Batch { ops: vec![] }]).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"not a snapshot");
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }
}
