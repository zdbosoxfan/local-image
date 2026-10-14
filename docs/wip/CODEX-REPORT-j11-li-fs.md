# J11 — one atomic/durable file backend

Continued the existing branch at `75f4caa0`, preserving the landed Library safety work (`b2837234` is an ancestor), unified preferences, and cancellable editor staging. Changes remain uncommitted. Read the audit §1, D10 and the J11 roadmap row. The audit's AI-settings fsync claim is stale: the existing branch already synced those settings; this task preserves that durability and fixes cleanup/publication behavior.

## Consolidation

New `crates/li-fs` owns file publication: `Policy::{Durable, Atomic}`, exclusively created same-directory staging, buffered streaming, bounded-chunk cancellation/progress, `StagedWrite`, Windows permission-denied rename retry, target-symlink resolution, preserved modes, verified never-replace writes, unique-name selection, file identity, checked directory sync, injected filesystem operations, and thread-local write faults/sync counters. The original `lc-catalog::safe_file` and `pc-format::atomic` APIs are thin re-exports; their existing tests remain intact. `pc-format::store::write_atomic` already delegates to that API and required no edit.

Byte writes and streamed writes use the same owned-file fill/flush/sync implementation. Staging cleanup handles encoder/progress panics, cancellation and ordinary failures. Collision errors never delete another writer's file. Explicit permission changes use the injected filesystem rather than ambient operations. Native writable handles remain open across chmod and sync, including write-only modes; Windows cleanup clears an inherited readonly bit on owned replacement scratch files when needed. After a new hard-link publication, scratch cleanup has a separate path: temporarily clear readonly, unlink scratch, then restore and sync the published inode's original permissions through an already-open writable handle obtained from the owned scratch name, so a competing replacement of the final path cannot redirect chmod. The generic drop guard is disarmed so it cannot later clear permissions on that shared inode. All post-publication cleanup failures carry a typed `was_published` marker and explicitly say the complete file was already published; trash retains both its payload and `.trashinfo` recovery record in that case.

| D10 site | Disposition |
|---|---|
| `lc-catalog::safe_file`, `FsStore` | Shared re-exports, including safety job's `write_new_with` and `sync_dir_checked`; original parent-exists contract retained. |
| `pc-format::atomic`, `store.rs` | Shared re-exports; automatic parent creation, fault seam, staged export API and cancellation type retained. |
| `li-ai::settings`, other users of its writer | Durable shared adapter with automatic parent creation. Settings serialization/unknown fields unchanged. |
| `lc-preview::disk` | Shared atomic-only writer; zero syncs. Cache cleanup recognizes only scratch with its own key/shard target hint, plus legacy names. |
| `lc-engine::camera_profiles` | Shared durable writer; identical JSON. |
| `lc-engine::rename` | New-file fallback copy now uses existing streamed, verified `import_move::copy_verified` instead of allocating the entire source and exposing partial destination bytes. Move/rollback logic retained. |
| `lc-engine::import_move`, `smart_sort::faces_store`, `sidecar` | Already on `safe_file`; migrated transitively. Face scratch cleanup recognizes the shared name prefix/suffix. |
| `lc-engine::smart`, `cmd::browse`, remaining `rename` operations | Actual migration/move and case-only rename transactions; not file-content writers. Retained their rollback and no-replace move contracts. |
| `apps/local-image::library_host`, `services` | Already shared writer users; migrated transitively. |
| `apps/local-image::desktop_install` | Shared durable staging; explicit 0755 launcher / 0644 data modes set before publication. |
| `apps/local-image::app_log` | Log rotation moves an existing file; no duplicate content writer. Retained. |
| `pc-ui-egui::generate_ui` | Shared cancellable staging; final job cancellation check still precedes commit. No UI changes. |
| `pc-automation::workspace` | `AtomicFs` adapter performs every operation through the held `cap-std` directory. No ambient canonicalization, parent creation or permission changes; absent-parent and escape rejection retained. |
| `li-seg::{denoise,lib}` | Model package, companion files and custom-model inventory use shared durable replacement; bytes/TSV preserved. |
| `li-seg::restoration` | Shared durable streamed staging, then pin verification and final cancellation check before commit. Reject destination symlinks. |
| `li-ai::enhance::selection` | Choices JSON and validated custom model use shared durable writes; existing lock and validation retained. |
| `li-ai::trash` | Copy fallback uses shared streamed, verified never-replace publication, retaining source permissions. `.trashinfo` remains an exclusive name reservation; symlink copies and actual moves retain their separate semantics. |
| `li-ai::library` | All three staged generation files durable; sync staging directory, retry transaction's directory rename, then sync parent. Retained one directory publication for image/thumbnail/metadata together. |

## Compatibility and deliberate changes

- Existing catalogs, native documents, settings, profile JSON, generation entries and custom-model TSV retain their formats. Byte goldens and reload tests cover the changed callers. No image rendering, UI flow, owner preferences or Match Look files changed.
- Library and AI replacement now follow an existing destination symlink and preserve its mode, matching editor behavior. Dangling links fail unchanged rather than being replaced. Failed mode preservation aborts before publication instead of silently weakening permissions.
- Never-replace writes support FAT32/exFAT, SD cards, backup drives and shared Mac/Windows volumes. They prefer safe atomic no-replace rename through the already-locked `rustix` 1.1.5 (`renameat2(RENAME_NOREPLACE)` on Linux, `renameatx_np(RENAME_EXCL)` on macOS). If that operation is unavailable/rejected with EINVAL/ENOTSUP/ENOSYS, hard-link publication is another atomic option; when links are unavailable too, `OpenOptions::create_new` exclusively reserves the **final** name, followed by bounded streaming, fsync and byte-for-byte read-back verification. This never overwrites another file. Readers may briefly see a partial **new** file on the exclusive-copy path. Errors, cancellation and panics close and remove only this writer's exclusively created destination; a collision never constructs a cleanup guard. The old replacing check-then-rename fallback remains forbidden. Windows std rename has replacement semantics, so this code uses links where supported and exclusive copies otherwise, without adding unsafe FFI.
- A directory-sync error after replacement has already published the new bytes; commit succeeds instead of falsely reporting the old file unchanged. This matches previous editor/Library behavior and changes the old AI adapter's post-publication error behavior. `sync_dir_checked` remains available where transactions need an explicit durability check before pruning backups.
- Custom restoration installation explicitly refuses destination symlinks because its existing pin verifier rejects symlink models. The source and referent remain unchanged.
- New English diagnostics are listed in `docs/wip/i18n-pending-j11-li-fs.md`; translations deferred.

## Evidence and scope

Followed established repository implementations and the Rust standard library's authoritative contracts: [rename replaces existing destinations and cannot cross filesystems](https://doc.rust-lang.org/std/fs/fn.rename.html), [exclusive creation prevents existence-check races](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new), [explicit file synchronization](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all), and [hard-link publication](https://doc.rust-lang.org/std/fs/fn.hard_link.html). [rustix's safe no-replace rename wrapper](https://docs.rs/rustix/1.1.5/rustix/fs/fn.renameat_with.html) supplies the atomic operation on Linux/macOS; the pinned crate's Apple implementation maps `NOREPLACE` to `RENAME_EXCL`. Safe std's Windows rename replaces destinations and cannot be used as a no-replace fallback. Exclusive final-name creation maintains compatibility where atomic publication is unsupported. No reference-app UX behavior was redesigned, so no Lightroom/Photoshop/Affinity/Capture One UI claims are made.

Necessary edits beyond the named implementation files: new backend and its tests; workspace/crate manifests and `Cargo.lock` for wiring; `pc-format/src/atomic/tests.rs` for adapter parity; this report and pending-diagnostics list. All remaining code changes are D10's listed files. No new registry packages, C/-sys dependencies or unsafe code. `li-fs` reuses the existing `photocraft-raster::Interrupt` type to keep the public staging API source-compatible. The pre-existing `CODEX-TASK.md` worktree modification was left untouched.

GPU tests added/run: **0**, appropriate to filesystem-only changes. No app windows opened, running app killed, owner photos read, external messages sent, full suites run, benchmarks run, or commits created. The task's final minimum-testing rule overrides its earlier full-crate-suite requirement.

## Measured before/after

A single small comparison sample used frozen `HEAD` implementations under `target/refvec/j11` and the newly built libraries, on this worktree's actual filesystem. The settings comparison calls the real old/new `AiSettings::save_to` application persistence path, including unchanged JSON construction and writing. No GUI was launched and no interactive-frame performance claim is made. Sample source, baseline files, build log and results remain under `target/refvec/j11/` and are not versioned.

| Check | Before | After |
|---|---:|---:|
| AI settings (4 saves, 4,373 bytes each), total wall time | 30,928 µs | 24,009 µs |
| Same AI settings sample, mean per save | 7,732 µs | 6,002 µs |
| Settings byte mismatch count | — | **0 / 4** |
| Durable settings sync calls | 2 per save in original implementation | **8 observed for 4 saves** (file + directory each) |
| Atomic-only 64 KiB output (4 writes), total wall time | 131 µs | 85 µs |
| Atomic-only sample byte mismatch count | — | **0 / 4** |
| Atomic-only sync calls | 0 | **0 observed** |
| Library write through a 0600 symlink destination: link retained | false | **true** |
| Same write: original referent updated | false | **true** |
| Resulting destination mode | 0644 | **0600** |
| Scratch files left by a caught streamed-encoder panic | 1 | **0** |
| Concurrent never-replace publishers (4 writers) | Original hard-link path already supported one winner | **1 winner, 3 AlreadyExists, complete bytes** |
| Simulated transient rename lock | Library had no retry | **2 failures then success on attempt 3** |
| Simulated unsupported rename and hard links | Original Library used a replacing fallback | **Verified exclusive copy succeeds; 0 replacing rename attempts; existing names unchanged** |

These four-file latency samples are sanity measurements on a shared machine, not a benchmark or evidence of a universal speedup. Safety determines the backend choice. Fixed buffering retains hardware-independent bounded memory: renamed-sidecar fallback copies previously allocated the entire source; now the existing streaming helper uses a 1 MiB fill buffer, followed by two 1 MiB read-back buffers. Those are allocation bounds inspected in code, not measured process RSS. Durable replacements also sync preserved permissions before commit; on files that already exist this adds one file sync, deliberately prioritizing durable modes over minimum save latency.

## Final validation

All commands used `PATH=~/.cargo/bin:$PATH`, `cargo +1.98.1`, offline compilation, the worktree's existing `target`, and the launcher's `CARGO_BUILD_JOBS=4`. One cargo process ran at a time.

- `cargo +1.98.1 check --offline` with the ten touched packages: **passed**. Final log: `target/j11-check.log`.
- `cargo +1.98.1 clippy --offline` with those same ten packages, `--all-targets -- -D warnings`: **passed**. Final log: `target/j11-clippy.log`. Capability-only follow-up after readonly scratch cleanup: `target/j11-clippy-cap-final.log`.
- `cargo +1.98.1 fmt -p <each touched crate> -- --check`: **passed** (`target/j11-fmt.log`).
- `cargo +1.98.1 test --offline -p li-fs -p lightcraft-catalog -p photocraft-format -p li-ai -p li-seg -p lightcraft-preview -p lightcraft-engine -p photocraft-automation -p local-image j11_`: **31 distinct new tests passed, 0 failed, 0 ignored**. Existing tests were filtered out. Log: `target/j11-tests.log`. The two capability tests were rerun after its final cleanup change, recorded in `target/j11-tests-cap-final.log`. The 15 core tests were rerun after the Windows hard-link cleanup fix (`target/j11-tests-core-final.log`); the core/AI tests were checked again after pinning the inode handle and preserving trash records (`target/j11-tests-cleanup-final.log`). Two further real-filesystem recovery tests inject failure after successful exclusive publication and verify both the typed outcome marker and retention of the trash recovery record. Command: `cargo +1.98.1 test --offline -p li-fs -p li-ai j11_published_cleanup`; log: `target/j11-tests-published-final.log`. That initial phase brought the distinct new passing Linux tests to **33**; the coordinator volume correction below adds seven, for **40 total**. One additional Windows-only readonly publication regression is included for a future native Windows run; it is not included in the Linux passing count.

| Package | New passing tests |
|---|---:|
| `li-fs` | 23 (16 initial + 7 volume correction) |
| `li-ai` | 6 |
| `li-seg` | 4 |
| `lightcraft-catalog` | 1 |
| `photocraft-format` | 1 |
| `lightcraft-preview` | 1 |
| `lightcraft-engine` | 1 |
| `photocraft-automation` | 2 |
| `local-image` | 1 |

An initial 13-test backend smoke run preceded caller integration; subsequent core refinements justified the final focused rerun. No full crate/workspace/GPU suites or cargo benchmarks were run. The UI crate's production change is only export publication; its backend staging semantics are covered by the new cancellation/drop tests, and it passes check/strict all-targets Clippy. No controls or interactions changed, so no click/drag tests were added.

TEST-CHANGED: `j11_new_file_publication_never_falls_back_to_replacing_rename` no longer expects a "use hard links" error; its legacy mock exposes neither safe rename nor verified copy, and the test still prohibits replacing rename. Actual unsupported-volume success is covered by the new `j11_volumes_` tests. `j11_published_cleanup_error_keeps_complete_new_file_and_marks_its_outcome` now accepts no scratch after atomic rename or one complete scratch after link/copy and checks every remaining file's complete bytes. Original catalog/editor test assertions remain unchanged.

Remaining limits: native Linux filesystem paths and injected failures were exercised. Windows retry behavior was tested through the injectable seam, and Windows readonly replacement/hard-link cleanup was reviewed, but no native Windows run is claimed. Unsupported-hard-link volumes use verified exclusive final-name copying. `rustix` 1.1.5 was already locked/transitively built and is now a direct dependency for Linux/macOS; no registry package/version, C/-sys package or unsafe code was added. Real FAT/exFAT mount testing was attempted without root but skipped because loop-device setup was unavailable, as detailed below. The existing optional-font build-script warning remains outside this task's scope. `git diff --check` passes for task-authored changes; the supplied, untouched `CODEX-TASK.md` already has a trailing-blank-line warning.


## Coordinator priority correction — FAT/exFAT support

Removed the previous unsupported-volume failure limitation. This follow-up changes only `li-fs` implementation/tests/manifest, its existing-package lockfile edge, and this report/pending diagnostics; the application adapters continue calling the same APIs.

The new cancellable `write_new_with_interrupted` API also checks during fallback copying/read-back and immediately before declaring completion. The fallback holds one read/write handle across permission changes, copying, verification and fsync; source permissions are applied before any visible content bytes, so privacy is preserved during the partial-file window. Working buffers remain bounded at roughly 2 MiB regardless of file size. `AlreadyExists` returns before ownership/cleanup is acquired. A genuine no-replace-rename permission error is returned rather than bypassed.

Seven new `j11_volumes_` tests cover:

- Linux's actual safe no-replace rename consumes scratch and rejects an occupied destination unchanged.
- Simulated EINVAL and ENOTSUP rename rejection plus missing hard links successfully publish identical bytes through exclusive copying, with observed file syncs and collision protection.
- Eight simultaneous fallback publishers: **exactly one winner, seven AlreadyExists**, 65,537 complete identical bytes, no loser destination.
- Three write-fault boundaries, mid-copy cancellation and a progress panic: **no stray final destination**, source intact, occupied destination never removed.
- Deliberate post-flush byte corruption is detected by read-back and removed; cancellation at final progress also removes the completed-but-unaccepted destination.
- Genuine rename PermissionDenied fails unchanged.
- Empty/binary fixtures and restrictive readonly/private modes retain exact bytes and permissions, including while the final name is visible.

Validation follows the minimal rule: `cargo +1.98.1 check --offline -p li-fs` and `-p local-image`; strict `cargo +1.98.1 clippy --offline -p li-fs --all-targets -- -D warnings`; only authored tests filtered by `j11_`/`j11_volumes_` and the final mode test's exact prefix. Logs are `target/j11-volumes-{check,check-app,clippy,tests,tests-final,modes-test}.log`. The first run found a read-back test timing error (tampering before a buffered flush was correctly overwritten by that flush); the failure log is preserved as `target/j11-volumes-tests.failed-readback.log`. The production path now reports a post-flush, post-sync verification progress checkpoint, and the tamper fixture targets that checkpoint. The corrected six-test run passed; the additional mode fixture also passed (1/1) and is recorded separately. Original 16 core authored tests also passed during the initial integration run. Distinct authored Linux tests across the completed task: **40** (33 initial + 7 volume tests). No full suites, benchmarks or GPU runs were added.

Real-volume check: formatted **two 64 MiB** scratch images with installed `mkfs.exfat` and `mkfs.vfat -F 32` under `target/refvec/j11-volumes`. Attempted mounting each inside `unshare --user --map-root-user --mount --propagation private`, without sudo/root privileges. Both format operations succeeded; both mounts exited **32**, "failed to setup loop device". Therefore actual mounted exFAT/vfat write validation is **skipped**, not claimed passed. Exact commands/results are preserved in `target/refvec/j11-volumes/attempt.json`; no host filesystem was mounted or modified. Native macOS/Windows runs are likewise not claimed.
