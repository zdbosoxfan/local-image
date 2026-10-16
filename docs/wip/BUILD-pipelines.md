# BUILD pipelines: export, previews and import use bounded parallel stages (Claude)

Branch `wip/pipelines`. Authority: `docs/wip/REVIEW-threading-policy-fable.md` (rules 3, 5, 6, 9, 10, 11, 12; §4 checklist), `docs/wip/AUDIT-multithreading.md` F6, F9, F9b; compute profile from `docs/wip/BUILD-threads-a.md` / `-b.md`.

(Written incrementally.)

## 1. Heavy-job permits (`crates/lc-sysmem/src/permits.rs`, additive)

- `HeavyJobs` / `Permit`: job slots `heavy_job_cap(C) = clamp(ceil(C / 4), 1, 4)` plus reserved bytes that must fit in `headroom = min(budget − RSS, ram_available() − 1 GB)`, re-read on every admission; nothing reserved → admitted whatever the size (one job always runs).
- `ceil` instead of the review's `C / 4`: with `C / 4` the owner's 9800X3D (`C = 7`) would get 1, while the review's own test matrix expects 2–4 there; `ceil` gives 2-core → 1, `C = 7` → 2, M4 Max `C = 15` → 4.
- `Permit::finish_heavy(bytes)` gives the job slot back after the render and keeps only the rendered output's bytes; `shrink` after encoding; drop releases the rest. So on a 1-slot machine the next render overlaps the previous item's encode and write.
- `heavy_jobs()`: one process-wide set from the profile, shared by export, preview builds and import copies. `admit(bytes, stop)` waits on a condvar, re-reads memory every 50 ms, returns `None` on cancel. Never called from a rayon task.
- `LOCAL_IMAGE_MEMORY_GB=n` (testing only): plan as if the machine had `n` GB, half of it used by others (the detection fallback). With `LOCAL_IMAGE_THREADS=2` this is the low-end simulation.

## 2. F9 export batch (`crates/lc-engine/src/export.rs`)

- `PreparedExport::run_with_ctl` is now two stages: `develop` (read + decode + develop, AI enlargement; or read the original / decode the raw for Original / DNG) and `encode_developed` (encode / write the DNG). Same results; `run_with_ctl` = develop then encode.
- `run_batch_with_ctl` = `run_batch_shaped(…, heavy_jobs(), Shape::auto(items, max_jobs, C))`:
  - folders and per-item errors (demo photo without a folder, bad Smart Sort subfolder) are worked out first, in order, and reported when the writer reaches them, as before; nothing after a batch-ending error is rendered.
  - feeder thread (`export-feed`): admits photos in input order, each with a permit sized from its pixels (`render_cost`: 36 B/px of the source for the decoded f32 image and working copies, 12 B/px of the output + deep samples), never more than `Shape::window` photos ahead of the writer.
  - render threads (`export-render-N`, one per job slot): `develop`; the permit then shrinks to the rendered output (`finish_heavy`), freeing the slot for the next render.
  - encode threads (`export-encode-N`, slots + 1, within `C`): `encode_developed`; the permit shrinks to the file's bytes.
  - write stage = the caller's thread: takes results in input order (early ones wait in a map with their permits), then the unchanged naming / conflict / originals-guard / `write` / sidecar code, `progress(i, name)` before each photo exactly as before (returning false cancels).
  - all stage threads are named OS threads in the Utility class, joined by `sync_channel(1)`; rayon only inside develop/encode; no blocking inside rayon tasks. Each stage's shared receiver is dropped with its last thread, so when the writer stops (done, cancel, error) every stage unwinds; the feeder's window/permit waits re-check the stop flag every 50 ms.
  - cancel: the feeder stops admitting, render/encode threads drop items at their next one, the writer stops before the next write. Writes stay `write_file` (temp file + rename), so no partial files.
  - per-item errors (render, encode, write) are reported in place; `stop_on_error` still ends the batch at the failed photo after the ones before it.
  - `pipeline::GPU_GATE`: a batch render whose develop may use the GPU decodes outside the gate and develops inside it, one at a time (lc-gpu supports concurrent renders, but several full-size renders could exhaust VRAM and a failed GPU render switches the GPU off for the process). Linear-float and soft-proof requests (which lc-gpu declines) skip the gate. Interactive renders never take it.
  - `pipeline::AI_GATE`: AI enlargement stays one at a time (audit: D = 1 for upscale exports).
  - browser build (no threads): the writer renders each photo inline when it reaches it (old behaviour).
  - a stage thread that can't be started ends the batch with "the export stopped unexpectedly…" rather than silently writing fewer files.
- Untouched: `export_task.rs` (FJ-005 library-generation check, cancel button, progress), per-file atomic publish, conflict handling, originals guard.
- Finding (not mine to fix): exported bytes are not reproducible run to run because the embedded ICC profile header carries the current time (moxcms writer, via `lc-codecs::icc`). Tests blank those 12 bytes (`without_icc_dates`); JPEG keeps the profile uncompressed, PNG compresses it, so the byte-identity tests use JPEG/TIFF.

## 3. F6 Build Previews / Smart Previews (`crates/lc-engine/src/cmd/previews.rs`)

- `for_each_bounded(name, items, permits, cancelled, work)`: `permits.max_jobs()` workers (≤ items; one runs inline), each item admitted with a permit for its cost; cancel stops admission, started items finish; workers are named Utility OS threads.
- Build Previews: one item = one photo (grid thumbnail + loupe render); cost `preview_cost` = raw data 2 B/px + `render_cost` at the built edge; loupe renders go through `pipeline::run_render_job` (GPU gate). Progress/cancel unchanged (`state.done/failed/cancel`).
- Smart Previews: builds run through `for_each_bounded`; outcomes are collected per position and counted in job order (so `failed` keeps its order); discard stays a simple loop.
- The Library's visible-grid thumbnails (`JobPool` in `Renderer`) take no permit, so they never wait behind a batch.

## 4. F9b import copy + verify (`crates/lc-engine/src/import.rs`)

- The per-file loop stays serial and in file order for everything that decides identity: path duplicates, Local promotion, probe failures, content duplicates, sidecar reading, rename sequence numbers (`self.seq`), organize folders, Move placement, Kept notes. Only the copy itself (`copy_into`: create-new, copy, fsync, verify against the probe hash) and the optional Copy-as-DNG conversion become `CopyTask`s.
- `Copies`: tasks are queued in file order on a `sync_channel(workers)` and taken up in order by `workers = heavy_jobs().max_jobs()` named Utility threads (`lc-import-copy-N`); each takes a permit (`copy_cost`: 16 MB of buffers, + 6× the file for DNG conversion). One worker (or the browser build, or no thread could start) = copied right away on the submitting thread, i.e. the old behaviour.
- Duplicates within a batch keep the serial rule exactly: a later file with the same content hash as a queued copy waits for that copy's outcome; a successful copy makes it a duplicate (`existing: None`), a failed copy (which imported nothing) lets it import as before.
- Names stay deterministic: files whose copies could take each other's names share a key (`name_key`: destination folder + name without extension, case or `-N`; covers `IMG.jpg`/`IMG-1.jpg`/`img.JPG` and raw → `.dng`), and a copy waits for the previous copy with its key before it starts (always an earlier task, waited for before taking a permit, so no deadlock). A Move placement waits the same way for an earlier copy with its key.
- Cancel: workers check the flag when they take a task up (inside the receive lock), so the copies made are always a prefix; the batch's items are cut at the first skipped copy, exactly where the serial import would have stopped. Items after the cut are dropped, a Move placed after it is rolled back (`import_move::rollback`), no uncatalogued copy is left.
- `ReadyFile`/`PreparedItem` order is file order; `commit_prepared`, Recently-Deleted restore, duplicate reporting and undo are unchanged (they consume the same list).
- `done` progress: a copied file counts when its copy finishes (others when decided), so the bar follows the real work.
- The bounded folder walker (`expand_within`, 790a341 port) is untouched.
- Probe pool (`probe_all`): `profile().threads` clamped to 1..=8 (was `available_parallelism` 1..=8).

## 5. Pool sizing from the compute profile

- `lc-preview` `JobPool::threads_for(C) = clamp(C, 2, 8)` (new); `default_threads()` kept for hosts without a profile.
- `lc-ui-egui` `Renderer::default()`: `JobPool::threads_for(lightcraft_sysmem::profile().threads).min(6)` (was `available_parallelism − 1`, 2..=8, min 6). Same 6 on the 9800X3D (C = 7) and the M4 Max (C = 15); 2 on a 2-core laptop (was 3); 3 on a 4c/8t laptop with C = 3 (was 6). Visible-grid renders take no batch permit.
- `lc-catalog` `local::set_compute_threads(C)` (new) sizes `untouched_of`'s scoped threads (`clamp(C, 1, 8)`); until it is called the logical CPU count is used as before. lc-catalog has no dependency on lightcraft-sysmem (adding one would change Cargo.lock), so the app must call it once at start-up: see Needs.

## 6. Tests (rule 11)

| Test | Shows |
|---|---|
| `lc-sysmem permits::tests::job_cap_follows_the_compute_threads` | slots for C = 1…63 (2-core → 1, C = 7 → 2, C = 15 → 4, ceiling 4) |
| `…headroom_is_the_smaller_of_budget_and_available_ram`, `admission_is_limited_by_memory_and_job_slots`, `one_job_always_runs_whatever_the_memory`, `a_finished_render_frees_its_slot_and_most_of_its_memory`, `a_waiting_admission_wakes_on_release_and_stops_on_cancel`, `the_process_permits_come_from_the_profile` | admission with a mocked `ram_available` (changed between admissions), the one-job floor, slot release after render, wake-up and cancel |
| `lc-engine export::tests::pipeline_depth_does_not_change_the_exported_files_or_their_order` | 7 renders (JPEG and 16-bit TIFF): same paths, same bytes, same order, same progress calls with `Shape::serial()`, 3+3 threads and `Shape::auto`; all permits released |
| `…one_failed_photo_does_not_stop_the_others_and_is_reported_in_place` | an unreadable original among five: its error in place, the other four (+ sidecars) written in order, at depth 1 and N; `stop_on_error` still stops there after the earlier ones |
| `…cancelling_mid_batch_leaves_only_whole_files` | real files on disk, cancel by `progress` and by the Cancel control while later photos are in flight: exactly the first three files, byte-identical to a serial run, no temp/partial files |
| `…memory_admission_bounds_the_photos_in_flight` | zero headroom: never two renders at once, batch completes; cost estimates grow with pixels/depth |
| `…pipeline_shape_follows_the_permits_and_cores` | shapes for 2-core, C = 7, C = 15, one photo |
| `lc-engine cmd::previews::tests::bounded_batches_run_every_item_once_within_the_permits`, `preview_cost_grows_with_the_size_built` | every item once, ≤ slots at once, one at a time when memory allows one, cancel stops admission |
| `lc-engine tests_import::parallel_import_copies_give_the_same_catalog_as_one_at_a_time` | Copy import of 19 files with 1 and 4 copy threads: identical report and catalog (names of four colliding `IMG`/`img-1`/`IMG-1.PNG` files, a duplicate within the batch, a Recently-Deleted photo restored) |
| `…cancelling_a_parallel_import_keeps_only_what_was_finished` | cancel during a 3-thread import: no copy left in the library that isn't catalogued |
| `lc-preview tests::pool_size_follows_the_compute_budget`, `lc-catalog local::pool_tests::checks_use_the_compute_budget_once_set` | pool sizes from C |

Existing tests kept passing unchanged, including `prepared_exports_run_on_another_thread` (progress calls and cancel), `bugfix_save_o029_…`, the originals-guard, sidecar-conflict and atomic-write export tests, `build_previews_fills_the_cache`, the smart-preview and Recently-Deleted import tests, and `tests_import_move::copy_import_verifies_each_copy`. That last one injects its copy fault thread-locally (`import_move::inject`), so the copy threads only start when there are at least two files (`workers = min(slots, files)`); a single file is copied on the import thread as before.

TEST-CHANGED: none.

## 7. Measurements (rule 12)

Machine: Ryzen 7 9800X3D (8C/16T), 60 GB, RTX 5090; profile `C = 7`, heavy-job slots 2. Shared with other builders, so load average (1-min, start → end) is shown with each run. Dev profile (`opt-level 1`, deps `2`), so absolute times are slower than a release build; before and after are the same build profile. Harness: `crates/lc-engine/examples/pipeline_bench.rs` (new): fresh on-disk library, 20 distinct ARW files (the CC0 `rawpixls-4465.ARW`, 60 MP Sony, copied 20 times with a different trailer each so the content hashes differ; no client photos were used), each phase in its own process: Import (Copy), Build Previews (standard 2048 px, `wait`), Export (full-size JPEG, default quality). Peak RSS = the process's `VmHWM`. The copies and work folders were deleted afterwards.

Interleaved runs (before = base commit 3941af39 binary, after = this branch):

| Phase | Round | Before | After | Load |
|---|---|---|---|---|
| Export 20 × 60 MP JPEG | 1 | 119.3 s, RSS 1.75 GB | 46.4 s, RSS 3.71 GB | before 24→33, after 29→18 |
| | 2 | 67.0 s, RSS 1.75 GB | 39.9 s, RSS 3.67 GB | before 14→10, after 8.6→7.4 |
| | 3 (quietest) | 60.9 s, RSS 1.73 GB | 42.2 s, RSS 4.24 GB | before 4.2→5.7, after 6.5→6.6 |
| Build Previews 20 | 1 | 56.9 s, RSS 1.16 GB | 37.8 s, RSS 1.32 GB | 21→24 / 32→29 |
| | 2 | 27.6 s, RSS 0.87 GB | 19.0 s, RSS 1.31 GB | 18→14 / 10→8.6 |
| | 3 | 26.9 s, RSS 0.86 GB | 15.3 s, RSS 1.09 GB | 5.5→4.2 / 5.7→6.5 |
| Import (Copy) 20 × 63 MB | 1 | 5.24 s | 2.15 s | 21 / 33 |
| | 2 | 3.30 s | 1.35 s | 18 / 10 |
| | 3 | 1.35 s | 0.63 s | 5.5 / 5.7 |

Reading: on the quietest round export is 1.44× faster (60.9 → 42.2 s; process CPU about the same, 226 → 231 s: the same work, overlapped), Build Previews 1.76× (26.9 → 15.3 s), import copies about 2× (IO, mostly page cache here; the first-ever run, with the inputs still being written back, took 19 s before). Peak RSS rises because two 60 MP renders are in flight (each ~1.7 GB in this build): 1.7 → 3.7–4.2 GB for export, well inside the 42 GB budget; on a small machine the permits keep it at one (below).

Low-end simulation (`LOCAL_IMAGE_THREADS=2`, `LOCAL_IMAGE_MEMORY_GB=8`; the before binary has no memory simulation, it only gets the 2 threads):

| Phase | Before | After | Load |
|---|---|---|---|
| Export | 123.7 s, RSS 1.73 GB | 121.7 s, RSS 1.79 GB | 5.6→2.4 / 2.1→1.9 |
| Build Previews | 34.5 s, RSS 0.86 GB | 34.8 s, RSS 0.87 GB | 6.6→5.6 / 2.4→2.1 |
| Import | 0.98 s | 0.96 s, RSS 0.26 GB | 6.6 / 2.4 |
| Export, 2 threads, real RAM | 124.5 s, RSS 1.73 GB | 113.8 s, RSS 1.79 GB | 3.6→4.6 / 1.6→3.6 |

Reading: with 8 GB, a 60 MP render (estimated 2.9 GB) leaves no room for a second, so admission runs one render at a time (render still overlaps the previous photo's encode when the rendered output fits) and peak RSS stays where it was: stable, no swap. With 2 threads and plenty of RAM the render/encode overlap gives about 9 %.

Not measured: UI frame time during the jobs (headless harness; the stages are Utility-class OS threads and the UI thread never waits on them), GPU-busy time, a second machine class.

## 8. Merge checklist (review §4)

- [x] No new `ThreadPoolBuilder` / `available_parallelism` scoped helpers / `Executor::multithread`; rayon only inside stages (global pool).
- [x] Nothing new on the UI thread; `Renderer::default` only reads the frozen profile.
- [x] No `send`/`recv`/join/permit wait inside a rayon closure: every blocking wait is on a named stage OS thread or the job's own thread.
- [x] Long-lived/blocking work on named threads in the Utility class (`export-feed`, `export-render-N`, `export-encode-N`, `lc-build-previews-N`, `lc-smart-previews-N`, `lc-import-copy-N`).
- [x] Heavy items take a permit; in-flight counts and windows derive from `C` and the memory headroom (only floors, ceilings and the 1 GB system reserve are constants).
- [x] Cancel checked per item; progress via the existing callbacks/atomics; the UI never joins a worker.
- [x] Output identical at depth 1 and N (export bytes, import catalog), reduction order not affected (each item independent; writes and catalog items in input order).
- [x] GPU: batch develops serialized through `GPU_GATE`; decode overlaps; interactive renders not gated.
- [x] Memory measured (peak RSS above); bounded channels (depth 1), bounded reorder window, no unbounded queue.
- [x] Measured on this machine at automatic settings and a 2-thread / 8 GB simulation (numbers above).
- [x] Pure Rust: no dependency or Cargo.lock change.
- [x] Fallbacks: no memory reading → `ram_available()` falls back to total / 2 (profile); no RSS → budget term alone; no thread could start → export reports an error, previews and import run inline; browser build → inline.

## 9. Needs from other owners

1. **App start-up (`apps/local-image/src/compute_policy.rs`, after `lightcraft_sysmem::init`; and `apps/local-image-cli/src/main.rs` after its `init`)**: one line `lightcraft_catalog::local::set_compute_threads(profile.threads);` (where `profile` is the value `init` returned). Until then `lc-catalog`'s `untouched_of` keeps using the logical CPU count (capped at 8), as before.
2. **lc-codecs / export reproducibility (finding)**: every exported file embeds an ICC profile whose header holds the current date and time (moxcms profile encoder via `lc-codecs::icc`), so two exports of the same photo a second apart differ in 12 bytes (JPEG/TIFF) or in the compressed `iCCP` chunk (PNG). Harmless for users, but it prevents byte-for-byte reproducible exports; a fixed creation date (e.g. the photo's capture date or a constant) would fix it.
3. **Preferences (`pc-ui-egui prefs_ui.rs` owner, later)**: the review's "Concurrent exports / preview builds: Auto / 1" choice can map to `export::pipeline::Shape::serial()` and `HeavyJobs` with one slot; not wired in this job.
4. **Import onto slow media**: copies use the heavy-job slots (2 on this machine). On a card reader or spinning disk parallel copies can be slower than one; the same "Concurrent: 1" preference would cover it (`ImportJob::copy_workers`).

## 10. Final checks

- `cargo fmt` on lightcraft-sysmem, lightcraft-engine, lightcraft-catalog, lightcraft-preview, lightcraft-ui-egui (only my files changed).
- `cargo clippy --offline --all-targets -- -D warnings` on the same five packages: clean.
- Lib suites, once (after the last code change for lc-engine):

| Package | Result |
|---|---|
| lightcraft-sysmem | 25 passed |
| lightcraft-catalog | 82 passed, 1 ignored |
| lightcraft-preview | 13 passed |
| lightcraft-engine | 508 passed, 4 ignored |
| lightcraft-ui-egui | 327 passed, 5 ignored |

Files: `crates/lc-sysmem/src/{permits.rs (new), lib.rs}`, `crates/lc-engine/src/{export.rs, import.rs, cmd/previews.rs, tests_import.rs}`, `crates/lc-engine/examples/pipeline_bench.rs` (new), `crates/lc-preview/src/{pool.rs, tests.rs}`, `crates/lc-catalog/src/local.rs`, `crates/lc-ui-egui/src/render.rs`. `export_task.rs` and the UI `import.rs` needed no change (progress semantics kept: `progress(i, name)` in input order; import `done` / `total` counters).
