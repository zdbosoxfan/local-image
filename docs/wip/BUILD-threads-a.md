# BUILD threads-a: one sized compute pool, shared tract executor, background priority (Claude)

Branch `wip/threads-a`. Authority: `docs/wip/REVIEW-threading-policy-fable.md` (§2 rules, §4 checklist, §6 test matrix) and `docs/wip/AUDIT-multithreading.md` (F1, sections A and F). Owner principle followed: everything is derived at run time from the detected hardware with floors, ceilings and fractions only; Preferences values are optional overrides, all Automatic by default.

## 1. Compute profile (`crates/lc-sysmem`)

New modules, safe API, `unsafe` only in `sys.rs` (one call per OS query, each with a SAFETY note):

- `profile.rs`: `Hardware` (logical, physical, P/E cores, RAM, which values fell back) and the frozen `ComputeProfile` (adds `threads_auto`, `threads` in effect and where it came from, background priority, memory % override, whether the pool was built). `init(Overrides)` detects once, builds rayon's global pool and logs one line; `profile()` returns the frozen copy (or a provisional one that never touches rayon, so a later `init` can still build the pool); `ram_available()` is re-read on every call.
- Detection per platform, as in the review's table:
  - Linux: logical from `available_parallelism()` (cgroup quotas, affinity); physical = distinct SMT sibling sets (`core_cpus_list`, or the older `thread_siblings_list`), falling back to distinct (package, cluster, core_id); P/E from `/sys/devices/cpu_core/cpus` + `cpu_atom/cpus` (Intel hybrid) or `cpu_capacity` (ARM); RAM from `/proc/meminfo` `MemTotal` / `MemAvailable`.
  - macOS: `hw.physicalcpu`, `hw.nperflevels`, `hw.perflevel0/1.physicalcpu`, `hw.memsize`; available = free + speculative + purgeable + file-backed pages × `hw.pagesize` (sysctl counters instead of `host_statistics64`, so no Mach types are needed; same quantity).
  - Windows: `GetLogicalProcessorInformationEx(RelationProcessorCore)` (one record per core, `EfficiencyClass` for P/E), `GlobalMemoryStatusEx`.
  - Fallbacks: logical 4, physical = logical (never halved), RAM 8 GB, available = total / 2. Named in the log line.
- Pool sizing, pure function `auto_threads(logical, physical)`: `C = clamp(min(physical, logical) − reserve, 2, max(logical, 2))`, reserve 1 when that minimum is ≥ 4. Overrides: `LOCAL_IMAGE_THREADS`, then `RAYON_NUM_THREADS` (testing, taken as given), then the Preferences value (capped at logical), then Auto.
- `priority.rs`: `ThreadClass { Interactive, Utility, Background }`, `set_current_thread_class` (Linux nice 0/+5/+10 with SCHED_BATCH for Background via `libc`; macOS QoS default/utility/background; Windows normal / below normal / background mode). Never raises a thread above its current nice value except when asked for Interactive. Fails soft: logs once, returns false. Before lowering a thread it makes sure rayon's global pool exists, so a lazily started pool can't inherit the low priority. Preferences "Normal" background priority turns the lowering off.
- `background_pool()` / `in_background_pool(f)`: one shared low-priority rayon pool (size C, workers set to Background in `start_handler`) for indexing fan-out, replacing the private per-task pools (rule 7's "second pool built with a handler").
- This machine (Ryzen 9800X3D): `16 logical / 8 physical cores, RAM 60.2 GB; compute threads 7 (auto)`.

## 2. One global rayon pool, built first in `main`

- `apps/local-image/src/compute_policy.rs` (new): reads `performance.system` from the saved preferences leniently, calls `lightcraft_sysmem::init`, then `li_seg::init_executor()`. `main.rs`: one `mod` line and one call right after the start-up log line (before the monitor-profile thread, preset loading, GPU start-up and eframe). Checked: nothing before it uses rayon (argument parsing, crash hook, `app_log::init`; no ctor-style statics anywhere).
- CLI `main.rs`: `lightcraft_sysmem::init(Overrides::default())` first (env overrides apply; the CLI has no logger, so the line isn't printed there).

## 3. tract on the shared pool (`crates/li-seg`)

- `init_executor()` now sets `Executor::RayonGlobal` (tract dispatches into whichever pool the calling thread belongs to: the compute pool, or the background pool for indexing). Still a one-time `Once`; logs which executor. `LOCAL_IMAGE_SEG_THREADS=1` → `Executor::SingleThread`; `n > 1` → a private n-thread pool, measurement only (documented). Verified: single-thread mode runs Fast in 6.55 s vs ~2.2 s on the pool.
- Every tract loader now calls `init_executor()` (faces, CLIP, upscaler, denoise, restoration, segmenter), so the CLI and any path that never opens a Segmenter also get the shared executor.

## 4. Private pools and scoped-thread helpers

| Site | Change |
|---|---|
| `lc-engine/src/smart_sort/mod.rs` (2 pools, cores/2) | `in_background_pool` |
| `lc-engine/src/smart_sort/people.rs` (cores/2) | `in_background_pool` |
| `lc-ui-egui/src/people_task.rs` (cores/2) | `in_background_pool`; its thread set to Background |
| `pc-codecs/src/codecs/png.rs` `parallel_idat` (up to 32 OS threads) | rayon `par_iter` over bands; band count now fixed at 64 (a property of the image), so the PNG bytes no longer depend on the machine's core count (rule 11) |
| `pc-io/src/pixels.rs` `par_map` (one OS thread per item), `bands` | rayon `into_par_iter` (same order and panic propagation); `bands` sized by `current_num_threads()` |
| `pc-format/src/store.rs` `par_compress` (up to 32 OS threads) | rayon `par_iter` in input order (was worker-interleaved order); a panic still becomes the same `FormatError` |
| `lc-ui-egui/src/softpaint.rs` (scoped threads ≤ 16) | rayon `into_par_iter` over the same band groups (pixel output unchanged: one group per band set) |
| `pc-compose/src/effects.rs` `rows_par` | keeps its OS threads (documented reason), capped at `current_num_threads()` (≤ 16) |
| `pc-ui-egui/src/gpu_canvas.rs:1187` | NOT changed: canvas file owned by the brush builder (see Needs) |

## 5. Background priority (rule 7) applied

- Utility: pc-engine `jobs::run` workers (`photocraft-job-N`), lc-engine `lc-build-previews`, `lc-smart-previews`, `ai-job-N` (Develop AI jobs; only the spawned path, never the inline `wait` path).
- Background: `sam3-download`, `smart-sort-faces`, every background-pool worker (smart sort tagging fan-out, face indexing fan-out).
- Limitation (by design of rule 7, noted for later): rayon fan-out started from a Utility thread (e.g. export, preview build) still runs on the global pool at normal priority; only the job thread's own work is lowered. A "Utility pool" via `install` would be the next step if measurements call for it.

## 6. Cancel for selections

- li-seg: `Segmenter::predict_with_cancel` / `predict_sky_with_cancel(…, &AtomicBool)`. tract 0.23.8 has no interrupt API, but `SimpleState::run_plan_with_eval` takes a per-node evaluator: the runner checks the flag before every graph node, before preprocessing and before the full-size upscale, and returns the `li_seg::Cancelled` error (`li_seg::is_cancelled`). `predict` / `predict_sky` keep their signatures (never cancelled).
- pc-engine `seg.rs`: `probability_with`, `subject_with`, `object_at_with` take the job's flag; a cancelled run returns `None` and caches nothing (a cancelled map can't be mistaken for "no subject"); new `sky_probability_with_cancel`. The mock model honours the flag like the real one.
- `smartselect_cmds.rs`: Select Subject, Object Selection (click) and Quick Selection pass `ctx.cancellation_flag()`; `learned()` checks the job before reporting "nothing found", so a cancel is reported as a cancel.

## 7. Settings

- `pc-engine/src/prefs.rs`: one new field `performance.system` (`SystemResources { threads: 0 = Auto, backgroundPriority: low|normal, memoryPercent: 0 = Auto }`, serde defaults), plus its choice and range entries (separate small hunks). Named after GIMP's "System Resources" page; the key's generated label ("System") already has a translation in every complete language (`preference_labels_are_translated`); the custom block is titled "Processing". Memory % is stored and exposed in the profile (`memory_percent()`, `memory_budget()`); the permit work is a later batch.
- `pc-ui-egui/src/prefs_ui.rs`: a separate `processing_rows` block under Performance (after Graphics): three dropdowns showing "Automatic (N)" values, the detected hardware line, "Applies at next launch.". Nested object, so the generic grid skips it; Reset Section resets it.

## Tests added

| Test | What it shows |
|---|---|
| `lc-sysmem profile::tests::pool_size_for_the_test_matrix_machine_classes` | C for 2c/4t, 4c no-SMT, 4c/8t, 8C/16T, M1, M4 Max, hybrid 6P+8E, i9 8P+16E, 32C/64T, SMT off, 2-CPU container, 1 CPU |
| `…detection_failure_uses_the_documented_fallbacks`, `linux_smt_on_and_off`, `linux_older_kernels_use_thread_siblings_and_core_ids`, `linux_intel_hybrid_counts_p_and_e_cores`, `linux_arm_capacity_marks_big_cores`, `linux_missing_or_odd_sysfs_is_unknown_not_guessed`, `meminfo_total_and_available`, `macos_sysctl_topology_and_memory`, `windows_core_records` | detection with mocked sysfs/proc/sysctl/Win32 inputs, including fallbacks |
| `…thread_overrides_env_then_setting_then_auto`, `the_profile_logs_what_it_found_and_decided`, `this_machine_detects_something_sane` | override precedence, log line, live detection |
| `priority::tests::priority_classes_apply_to_the_calling_thread_only`, `background_pool_workers_run_at_background_priority` | priority smoke test (Linux reads nice and policy back from `/proc/thread-self/stat`) |
| `li-seg tests::tract_output_is_identical_on_one_and_many_threads` | a conv net through tract on the shared executor: byte-identical at 1, 2, 4, 7 threads |
| `li-seg tests::cancel_stops_predict_at_the_next_model_stage`, `cancel_from_another_thread_ends_a_long_predict_promptly` | cancel stops at the next node (deterministic) and promptly from another thread (13 ms vs a 64 ms full run) |
| `pc-codecs parallel_tests::parallel_png_bytes_do_not_depend_on_the_thread_count` | rayon kernel: PNG bytes identical at 1, 2, 5, 8 threads |
| `pc-engine smartselect_cmds::tests::cancelling_a_selection_stops_the_model_run_promptly` | the worker ends without the gate opening (before: the mock waited up to 60 s), nothing cached, the next run calls the model again |
| `local-image compute_policy::tests::*` | preference → overrides mapping, lenient reading |
| `pc-ui-egui prefs_ui::tests::processing_overrides_show_auto_values_and_round_trip` | the section renders "Automatic (N)", values round-trip, bad values set by path are refused |

## Measurements (rule 12), this machine, dev profile

Ryzen 7 9800X3D (8C/16T), 60 GB. The machine was shared with other builders; load average is given with each run. "Before" = the previous code (tract on its own 8-thread pool; rayon default 16 threads). "After" = C = 7 shared pool + `RayonGlobal`.

Background removal, `crates/li-seg/examples/bg_timing.rs` (new), portrait.jpg 1920×1199, warm median of 3:

| Run | Load | Fast (RMBG-1.4) | Better (InSPyReNet) |
|---|---|---|---|
| Before (private 8-thread tract pool) | 3 | 1.94 s | 9.87 s |
| After, shared pool 7 | 4.3 | 2.29 s | 11.06 s |
| After, shared pool 8 (same thread count as before) | 4.2 | 1.90 s | 11.38 s |
| Interleaved under load: old style (`LOCAL_IMAGE_SEG_THREADS=8`) / new (pool 7) | 9-12 | 2.21, 2.15 / 2.26, 2.27 s | 12.61, 11.56 / 12.82, (19.4 outlier) s |
| After, `LOCAL_IMAGE_SEG_THREADS=1` | 16 | 6.55 s | 28.47 s |

Reading: `RayonGlobal` costs nothing versus a private pool at the same thread count (8: 1.90 s vs 1.94 s); with one thread less (7) inference is within about 5 % under equal load, and nothing runs a second pool alongside the app's other work any more.

Develop export, `lc-engine` example `render_bench` `ONLY=batch` (2 Sony ILCE-1 ARW, decode + render + JPEG, CPU path, `RAYON_NUM_THREADS` emulating the pool size):

| Run | Load | Wall | Process CPU |
|---|---|---|---|
| Before, 16 threads (rayon default) | 3 | 8.85 s | 62.6 s |
| Before, GPU path | 3-8 | 3.35 s (CPU path 9.61 s) | 8.2 s |
| 16 threads, interleaved | 21 / 24 | 13.50 / 16.74 s | 62.6 / 63.5 s |
| 7 threads (= C), interleaved | 23 / 35 | 14.35 / 11.79 s | 54.3 / 51.9 s |

Reading: at C = 7 the same export uses about 15 % less CPU time; wall time under heavy load is noise-dominated (11.8-14.3 s vs 13.5-16.7 s). A quiet-machine after number for the export could not be taken: the load average stayed above 15 for the rest of the session.

## Needs from other owners

1. `crates/lc-ui-egui/src/export_task.rs:79`, `smart_sort_task.rs:94`, `import.rs:316` (and the two `std::thread::spawn(job)` at `import.rs:403/464` if they are long jobs): first line of the spawned closure, e.g.
   `std::thread::Builder::new().name("export".into()).spawn(move || { lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility); work() })` — Utility for export and import, Background for `smart-sort` (its fan-out is already in the background pool). lc-ui-egui already depends on lightcraft-sysmem.
2. `crates/pc-ui-egui/src/ai_ui.rs:305` (`model-download`), `:338` (`seg-download`), `model_browser.rs:465` (`model-install`): Background; `generate_ui.rs:2893` (`batch-cutout`), `restoration_ui.rs:193` (`photo-restoration`), `ai_ui/enhance_settings.rs:149` (`upscale-custom`): Utility. Same one-line form.
3. `crates/pc-ui-egui/src/gpu_canvas.rs:1187` (brush/canvas owner): `std::thread::available_parallelism().map_or(4, |n| n.get()).min(16)` → `rayon::current_num_threads().min(16)` (or rayon `par_chunks` if the loop allows).
4. `crates/pc-engine/src/select_extra_cmds.rs:76` (Select Sky job): `crate::seg::sky_probability(&doc, active, all || layer.is_none())` → `crate::seg::sky_probability_with_cancel(&doc, active, all || layer.is_none(), ctx.cancellation_flag())`, then keep `ctx.check()?`.
5. `crates/pc-engine/src/cutout_cmds.rs:188` (Remove Background job): `crate::seg::subject(doc, Some(id), sample_all)` → `crate::seg::SubjectModel::current().and_then(|m| crate::seg::subject_with(&m, doc, Some(id), sample_all, ctx.cancellation_flag()))`, and check `ctx.cancelled()` before falling back to the classical heuristic.
6. i18n owner: new English-only labels in `prefs_ui.rs` (consts `PROCESSING*`, `DETECTED`): "Processing", "Processing threads", "Background work priority", "Memory for processing", "Detected: {logical} logical / {physical} physical cores{hybrid}, {ram} GB RAM". "Automatic", "Low", "Normal", "Applies at next launch." reuse existing translations.
7. Later batches (not in this scope, noted by the review): `lc-preview/src/pool.rs:100` and `lc-ui-egui/src/render.rs` should size JobPool from C (e.g. `clamp(C / 2, 2, 8)`) instead of `available_parallelism()`; `lc-engine/src/import.rs:405` probe and `lc-catalog/src/local.rs:253` likewise; `lc-ui-egui/src/menus.rs:1048` ("CPU threads" in the about/system info) could show the profile line.

## Cargo.lock

Only dependency edges changed; no package was added. lightcraft-sysmem → libc (Linux only), log, rayon; lightcraft-sysmem added to local-image, local-image-cli, lightcraft-engine, lightcraft-ui-egui, photocraft-engine, photocraft-ui-egui; rayon added to photocraft-io and photocraft-format. All are already-locked, pure-Rust crates (libc is Rust bindings, no C build). Reason: the profile, pool and priority helpers live in the one crate allowed `unsafe`, and the crates that use them need the edge; pc-io and pc-format need rayon for the shared pool.

## TEST-CHANGED

None. No existing assertion was changed or weakened.

## Final checks

- `cargo fmt` on all 12 touched packages (only my files had diffs).
- `cargo clippy --offline --all-targets -- -D warnings` on lightcraft-sysmem, li-seg, photocraft-engine, lightcraft-engine (also `--features sam --lib`), lightcraft-ui-egui, photocraft-ui-egui, photocraft-codecs, photocraft-io, photocraft-format, photocraft-compose, local-image, local-image-cli: clean.
- Lib suites, once:

| Package | Result |
|---|---|
| lightcraft-sysmem | 18 passed |
| li-seg | 80 passed, 8 ignored (opt-in real models) |
| photocraft-codecs | 40 passed |
| photocraft-io | 130 passed, 1 ignored |
| photocraft-format | 52 passed |
| photocraft-compose | 185 passed |
| photocraft-engine | 1438 passed, 27 ignored; integration `it prefs_usage` 4 passed |
| lightcraft-engine | 499 passed, 4 ignored |
| lightcraft-ui-egui | 327 passed, 5 ignored |
| photocraft-ui-egui | full run 1495 passed, 4 failed, 49 ignored. 2 of the 4 were mine (`preference_labels_are_translated` for the first key name "compute"; a query in my new test) and pass after the fix (focused rerun). The other 2 are outside this job and come from the base: `i18n::tests::every_tl_literal_is_translated` (untranslated "Colour brightness/hue/saturation" in `panels.rs`) and `toolsets_ui::persona::tests::persona_clicks_and_shortcuts_keep_document_dock_selection_and_history` (finds two "Background" labels; the swatches panel from c5849aa5 adds one) |
| local-image (bins) | 144 passed, 2 ignored |
| local-image-cli | 0 lib tests |

- A test run rewrote `docs/design/vector-tool-icons.png` (a pc-ui-egui test artifact): restored, not committed. `apps/local-image/target/` (test screenshots) left untracked.
