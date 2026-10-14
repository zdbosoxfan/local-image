# J25 jobs runner + Library tasks (part b) — Claude builder report

Status: done. Duplicate-systems audit D14, slices 3 (Library tasks on the runner model with cancel) and 4
(one progress widget), plus the J9 follow-ups. Branch `wip/cl-j25-jobs-runner`, one commit.

Owner decision applied mid-job (2026-10-14, "nothing old needs to be supported"): no adapters or
thin forwards were kept — the duplicate primitives were deleted and every caller moved to
`lightcraft_sysmem::JobCtl`.

## What merged

One job handle for the whole app: `lightcraft_sysmem::JobCtl` (J9). Deleted duplicates:

| Removed | Was | Callers now |
|---|---|---|
| `pc-engine::jobs::JobCtx` | editor runner's own atomics + mutex (monotonic progress, message, cancel, `stage`) | work closures take `&JobCtl`: `report(f, msg)`, `check()?`, `stage(lo, hi, msg, f)`, `is_cancelled()`, `cancel_flag()` (≈200 call sites in pc-engine / pc-ui-egui, mechanical) |
| `lc-engine::enhance::JobCtl` (+ its `Arc` wrapper) | Develop AI jobs' own `AtomicU32` progress / cancel / message | `Job.ctl: JobCtl`; `AiHost::{remove, upscale}` take `&lightcraft_sysmem::JobCtl`; export renderer too |
| `lc-engine::cmd::cull::{Control, Work::run}` | culling's own flag + counter | `Work::run_job(&JobCtl)` only |
| `lc-ui-egui::tasks::Tasks.culling` + its egui window | per-feature culling progress | a normal runner task with a job |
| Export `cancel: Arc<AtomicBool>` + `progress: Arc<Mutex<(usize,String)>>` + separate `enhance_ctl` | three handles for one export | one `job: JobCtl` (step = photos started / total, message = file); the same job reaches AI enlargement, so one Cancel stops both |
| Smart Sort / People `cancel` flag + `progress` mutex | per-feature | one `job: JobCtl` each (`add_step` from parallel workers) |
| `pc-ui-egui::jobs_ui::bar` (and private copies of percent / indeterminate) | editor-only progress bar | `li_ui_kit::progress::{bar, shown, percent}` |

Added to the shared primitive (`crates/lc-sysmem/src/jobs.rs`, J9's file — outside edit):
`advance` (monotonic fraction, `fetch_update`), `report(fraction, message)` (the editor's rule: never
backwards; empty message keeps the old one), `stage(lo, hi, msg, f)` (an `Interrupt` mapped onto a
sub-range — moved from `JobCtx`), `add_step(n)` (exact counting from rayon workers; replaces the
mutexed counters), and `impl From<Cancelled> for String` so `job.check()?` works in Develop's
`Result<_, String>` workers. pc-engine gets `impl From<Cancelled> for EngineError` (→ `Cancelled`).

The Library runner (`lc-ui-egui/src/tasks.rs`) now follows the editor model: work on a worker
(Utility thread class, like editor jobs) against what it captured; result applied on the UI thread;
every task has a `JobCtl` (`spawn_job` hands it to the work; `spawn` is the variant for work that
can't check it); `Tasks::cancel(label|None)`, `Tasks::job`, `Tasks::status`; a cancelled task's
result is discarded and a "<task> cancelled" toast shown; a library change cancels (not just
drops) every task.

One task panel (`tasks::progress`, bottom-left, where the export panel was) lists every running
Library task — culling, export, Smart Sort, faces, slow file-system tasks (after 0.25 s, like the
editor's dialog delay) — each a `li_ui_kit::progress::job_row` (title, Cancel → "Stopping…",
bar, detail, %). Automation ids kept: `button:exportCancel`, `button:cullCancel`; new
`button:taskCancel:<label>`, `button:smartSortTaskCancel`, `button:peopleTaskCancel`. Smart Sort /
faces rows are hidden while the Smart Sort dialog (which has its own progress) is open.

New reusable widget: `crates/li-ui-kit/src/progress.rs` (`bar`, `shown`, `percent`, `JobLine`,
`job_row` with Library / Editor profile).

## User-visible changes
- Library: one task panel for export, Assisted Culling, Smart Sort and faces (previously the
  export panel, a separate "Assisted Culling" window, and no panel for faces/Smart Sort outside the
  dialog). Rows appear after 0.25 s. Slow file-system tasks (Find Missing Photos, auto-import
  listing on a slow share) now show and can be cancelled (result discarded).
- The panel is not drawn in full-screen preview (like the import progress); export used to draw
  there. Export completion is still collected and toasted.
- Develop AI jobs: progress never goes backwards; non-finite progress is ignored (was clamped to
  0/1); cancel error text is "Cancelled" (`enhance::CANCELLED`, was "cancelled").
- Smart Sort counts photos that could not be prepared in its done count, so the bar reaches 100%.
- During an export with AI enlargement, the detail line shows the AI stage while it runs.

## Quality / speed numbers
- Progress reporting cost (throwaway micro-measurement, 50 M calls, dev profile, not committed):
  `JobCtl::report(f, "")` 3.4 ns vs the old `JobCtx` `fetch_max` path 4.2 ns; `report(f, same msg)`
  9.5 ns (mutex + compare, as `JobCtx` did). Cancel checks unchanged (atomic load).
- Memory: one `Arc` per job instead of 2–3 per Library task (export had flag + mutex + enhance ctl).
- Parallel face/Smart Sort progress: exact (`add_step` under the status lock) instead of a mutexed
  tuple per task.

## Checks run
- `cargo check --offline --workspace --all-targets`: clean.
- `cargo fmt` on the 8 touched packages; `cargo clippy --offline -p lightcraft-sysmem -p li-ui-kit
  -p photocraft-engine -p photocraft-algo -p lightcraft-engine -p lightcraft-ui-egui -p
  photocraft-ui-egui -p local-image --all-targets -- -D warnings`: clean.
- Tests (filtered):
  - lc-sysmem `jobs::` — 8 pass (3 new: `jobctl_advance_never_goes_backwards`,
    `jobctl_add_step_counts_from_parallel_workers`, `jobctl_report_and_stage_map_progress_into_their_range`).
  - li-ui-kit `progress::` — 2 new, pass.
  - lc-ui-egui — 2 new (`jobs_runner_cancel_stops_worker_discards_result_and_panel_shows_progress`,
    `jobs_runner_panel_lists_export_and_culling_with_their_cancel_ids`) + parity: culling cancel,
    library-origin (tasks, export, import), export modal poll, `ds2_` people/smart sort, `tests_ai`:
    26 pass.
  - **Parity gates**: pc-engine `jobs::tests` (the editor runner, 18 pass / 1 ignored as before) and
    `final_review_bridge_stops_on_unwind_and_preserves_results`; pc-ui-egui `jobs_ui::tests` (14);
    lc-engine `enhance::` + `tests_enhance` + `tests_export` + `cull_run_job` (45);
    lc-engine coverage `ds_enhance_coverage` (17). All pass.

TEST-CHANGED (assertions kept, moved to the new type):
- `pc-engine/src/jobs_tests.rs` `progress_is_monotonic_and_reaches_one`: `fraction()` is now
  `Option` (`Some(0.5)`, `Some(1.0)`), message via `status().message`.
- `pc-engine/src/ai_cmds.rs` `final_review_bridge_stops_on_unwind_and_preserves_results`: message
  via `status().message`.
- `lc-engine/tests/coverage/ds_enhance_coverage.rs` `job_ctl_*` (4 tests) now test the shared type:
  `job_ctl_set_clamps_finite_and_non_finite_floats` — ±infinity are now **ignored** (was clamped to
  0/1), NaN ignored; added `report` clamping + monotonic checks. `check()` compared via
  `map_err(String::from)` against `CANCELLED`. Other tests there: `ctl.check().map(..)` gains
  `.map_err(String::from)`.
- `lc-engine/src/enhance/mod.rs` tests: same `.map_err(String::from)`; `ctl.set` → `ctl.report`.
- `lc-ui-egui/src/tasks.rs` `bugfix_develop_culling_runs_as_cancellable_background_task`: cancels
  through `app.tasks.cancel(Some(CULLING))` (was the deleted `Control.cancel` flag).
- `lc-ui-egui/src/export_task.rs` (2 tests), `people_task.rs` (2 tests), `tests_people.rs`
  (2 lines), `apps/local-image/src/library_host.rs` (1 test): read/cancel the task's `job`
  instead of the deleted flag/mutex fields; same assertions.
- Mechanical renames in other tests (`JobCtx::new()` → `JobCtl::new()`, `.progress(` → `.report(`,
  `.cancelled()` → `.is_cancelled()`): `cutout_output_tests.rs`, `trace_output/tests.rs`,
  `background_ui_tests.rs`, `generate_ui/panel_controls_tests.rs`, `jobs_ui_tests.rs`,
  `tests_enhance.rs`, `tests_export.rs`, `tests_gpu.rs`, `lc-ui-egui/src/tests_ai.rs`.

## Outside edits
Owned files: `pc-engine/src/jobs.rs`, `lc-engine/src/enhance/mod.rs`, `lc-ui-egui/src/{tasks,
export_task,smart_sort_task,people_task}.rs`, `pc-ui-egui/src/jobs_ui.rs`. Outside (all mechanical
caller moves forced by deleting the duplicates, or the shared widget):
- `crates/lc-sysmem/src/jobs.rs` — `advance`, `report`, `stage`, `add_step`, `From<Cancelled> for String` + tests.
- `crates/li-ui-kit/src/progress.rs` (new) + `lib.rs` (`pub mod progress;`).
- `crates/lc-engine/src/cmd/cull.rs` — `Control` and `Work::run` deleted.
- JobCtx → JobCtl callers: pc-engine `ai_cmds, cutout_cmds, cutout_output_tests, develop_layer_cmds,
  edit_menu_cmds, file_cmds, jobs_tests, photo_cmds, preset_import_cmds, retouch_cmds/content_aware_move,
  select_extra_cmds, smartselect_cmds, vector_packages/{pdf_export, svg_export, trace_output,
  trace_output/tests, vector_export_job}`; pc-ui-egui `background_ui, background_ui_tests,
  generate_ui, generate_ui/panel_controls_tests, jobs_ui_tests, lib, save_worker,
  vector_packages/{pdf_import, vector_export}`; pc-algo `tests_ext.rs` (one comment).
- enhance JobCtl → shared callers: lc-engine `enhance/{cmds, denoise, remove, session}, export,
  tests_enhance, tests_export, tests_gpu, tests/coverage/ds_enhance_coverage`; lc-ui-egui
  `panels/enhance, tests_ai`; `apps/local-image/src/develop_ai.rs`.
- Task field changes: lc-ui-egui `control.rs`, `panels/smart_sort.rs`, `panels/people_workspace.rs`,
  `tests_people.rs`; `apps/local-image/src/library_host.rs` (test).
- `docs/wip/i18n-pending-j25-jobs-runner.md` (new strings).
No new dependencies; Cargo.lock unchanged; no `unsafe`.

## Left (and why)
- `apps/local-image/src/develop_ai.rs` still mirrors li-ai progress onto the Develop job with a
  polling watcher thread (100 ms). Hand the Develop job to li-ai directly once li-ai's download /
  remove / enhance APIs take `&JobCtl<S>` generically (li-ai's `JobControl` needs `JobCtl<Stage>`;
  li-ai is not in this job's files).
- li-ai `JobControl` newtype (J9) remains — by the owner's new rule it should go too, but it has
  230+ call sites in li-ai / pc-ui-egui owned by other jobs; suggest its own job.
- `pc-ui-egui::jobs_ui::dialog` / `status_progress` / `open_card` keep their own layouts (modal
  dialog, status-bar strip, opening tab card) and draw with the shared bar/percent; only the
  Library uses `job_row` so far.
- Import (`import.rs`) and preview build progress still draw their own bars (not in this job's files);
  moving them onto a task `JobCtl` + the task panel is a follow-up.
- Ad-hoc `"cancelled"` strings in lc-engine `import.rs` / `merge.rs` are not `Cancelled` yet.
