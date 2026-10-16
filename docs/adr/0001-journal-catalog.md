# 0001. Library catalog: in-memory maps with a JSON snapshot and an operation log

- Status: Accepted (recorded retroactively, 2026-10-16)
- Scope: the Library catalog (photo records, albums, stacks, keywords, develop settings, history)
  in the `lc-catalog` crate

## Context

The Library stores metadata about photos. The photos themselves stay where they are on disk. The
catalog must:

- survive a crash or power loss in the middle of an edit, without a partly written state;
- support undo and redo of every change;
- work on a local disk, a network share and a removable drive;
- be recoverable after a failed write, such as a full disk or a dropped network share.

The catalog is small compared with the photos, and it is changed one step at a time by the user.

## Decision

1. **In-memory state.** The catalog is held in memory as ordered maps keyed by id (`BTreeMap`).
   Photos are stored behind shared pointers so that copies of the catalog are cheap.

2. **Changes are operations.** Every change is an `Op` value. Applying an op returns its inverse.
   The engine keeps the inverses for undo and redo. A step made of several ops is undone by
   applying the inverses in reverse order.

3. **Two files in the library folder.**
   - `catalog.snap` holds the whole catalog as JSON, with the catalog format version and the
     sequence number of the last operation it includes. It is replaced atomically: the new file is
     written to a temporary file, synced, and renamed over the old one.
   - `catalog.log` holds one line per operation after the snapshot: the sequence number, a CRC-32
     of the operation's JSON, and the operation itself.

   Loading reads the snapshot and replays the log records that come after the snapshot's sequence
   number.

4. **Durable appends.** Each append is synced to disk before it returns. If an append fails, the
   change stays in memory, the user is told that it is saved in memory but not on disk, and the
   write is retried.

5. **Compaction.** The log is compacted into a new snapshot when it reaches 2,000 records or
   16 MiB, whichever comes first. The snapshot can be written by a worker thread. Appends continue
   to the same log while it runs, and the records written meanwhile are kept when the log is reset.

6. **Recovery rules.**
   - A torn final record (a crash in the middle of an append) is dropped, and the file is cut back
     to the last whole record.
   - A failed append is cut back to the last whole record before the retry, so a fragment is never
     followed by a record on the same line.
   - Records already contained in the snapshot are skipped. This covers a crash between writing the
     snapshot and resetting the log.
   - A bad record followed by good records is treated as damage. The log is kept beside the library
     as `catalog.log.damaged-<sequence>`, and a fresh snapshot is written so that the library can be
     opened.

7. **Format versions.** The snapshot carries the catalog format version. The current version is 2.
   This build reads only version 2. A snapshot or log from an older version is refused, and so is a
   newer one, with a clear error. Both are refused before anything is modified. There is no
   migration. A change to an operation or to a persisted field requires a new version number.

8. **Single writer.** Opening a library takes an exclusive operating-system file lock on
   `catalog.lock`. A second process that tries to open the same library is refused, and the error
   names the process holding it, read from `catalog.lock.owner`. That owner file is only a hint. On
   a file system that cannot lock, the library opens without the lock and with a warning.

9. **Storage behind a trait.** The journal reads and writes through a small `Store` trait: read,
   atomic write, streamed atomic write, append, truncate. The native implementation works on a
   library folder, and an in-memory implementation is used by tests, including tests that simulate
   crashes. The trait leaves room for other backends. A browser build is described in the project
   documentation; the repository in this worktree does not contain a browser implementation of the
   trait.

## Rationale

These reasons are the ones the code and the project documentation state. The repository does not
contain a written comparison with other storage engines, so the reasons below are not a full
evaluation.

- **Crash safety with rules that can be checked.** The rules are an append-only log with a checksum
  per record, and an atomic replacement for the snapshot. Each failure case has a stated outcome
  (see decision 6), and the test suite covers torn appends, failed appends, background compaction,
  format refusal and the lock.
- **Undo without a second history format.** Every change is already a reversible operation, so
  undo and redo reuse the same records.
- **Plain, inspectable files.** The state is two files that can be copied, backed up and read
  without special tools. Library backups copy the snapshot and the log together.
- **Storage can be swapped.** The catalog logic does not depend on the file system directly.

## Consequences

Accepted costs:

- **Queries are in-memory scans.** A filter or sort walks every photo in the catalog, and there
  are no indexes. The cost grows with the size of the library, and the whole catalog must fit in
  memory.
- **Snapshots are whole-catalog writes.** Compaction serializes the entire catalog. Background
  compaction keeps the UI from waiting for it, but the disk work still grows with the library.
- **Every append waits for the disk.** Each change costs one synced write. This is slow on some
  network shares and USB drives.
- **Format changes are breaking.** Any change to an operation or a persisted field means a new
  format version, and every library in the old format is refused. There is no migration path.
  For Local Image 2, this is why the release starts with a new Library and why catalogs from
  earlier versions are not imported.
- **Locking depends on the file system.** Some network shares offer advisory locks or none at all.
  Where locking is unavailable, two processes can open the same library. The owner file helps to
  tell the user who has the library, but does not prevent the second writer.
- **Two files must travel together.** A backup or a restore must copy the snapshot and the log as
  a pair. Mixing a snapshot from one backup with a log from another is not safe.
- **No ad hoc SQL.** Reports and external tools must use the Rust API. There is no database file
  that other programs can query.
- **The snapshot names its format.** The snapshot's format field is the string `lightcraft-catalog`,
  kept from the earlier product name. Changing it would make existing snapshots unreadable.

## Revisit when

Open a new decision record if any of these become true:

- Queries over a real library take long enough to be noticed by users, and indexes or a different
  store would fix it.
- Whole-catalog snapshots become too slow or too large for the libraries users actually keep.
- A migration from an earlier format is needed, for example to bring older catalogs forward.
- Multi-process or multi-machine use on shared folders becomes a supported workflow, which would
  need a stronger guarantee than the operating-system lock.

## Related documents

- Library safety, backups and restore: `docs/LIBRARY-SAFETY.md`
- Decision index: `docs/adr/README.md`
