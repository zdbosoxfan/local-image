# J9 `JobCtl` (part a) — Claude builder report

Status: done. Duplicate-systems audit D14, slice 1 (primitives + adapters) and slice 2 (li-ai / li-seg on it; lc-fetch reaches it through li-ai after J8). Branch `wip/cl-j09-jobctl`, one commit.

## What merged

One job primitive, `lightcraft_sysmem::jobs` (`crates/lc-sysmem/src/jobs.rs`, new; re-exported at the crate
root). Both halves of the app already depend on `lc-sysmem`, so nothing new is pulled into the app.

| Type | What it is | Replaces / backs |
|---|---|---|
| `JobCtl<S = ()>` | Owned, `Clone` handle shared by a job and its UI: `cancel`, `is_cancelled`, `check`, `cancel_flag`, `interrupt`, `fraction` / `set_fraction`, `set_step`, `set_stage`, `set_message`, `status` | `li-ai::JobControl` (now a thin adapter), `cull::Control` (new `Work::run_job`); lc-fetch builds on it through li-ai after J8 (see below) |
| `JobStatus<S>` | `{ stage, step: Option<(u32,u32)>, message }` + `fraction()` | `li-ai::Progress` is now `JobStatus<li_ai::Stage>` (type alias, same fields) |
| `Interrupt<'a>` | The borrowed `Copy` view hot loops take (moved here verbatim from `pc-raster`, plus two sources) | `pc-raster::Interrupt` is now a re-export |
| `Cancelled` | The one cancel error, shown as "Cancelled" | `pc-raster::Cancelled`, `li-ai::Cancelled`, `li-seg::Cancelled` are now re-exports of it |

API for J25 (small on purpose; all documented in the module docs):
- Make a job: `JobCtl::new()` (no stages) or `JobCtl::<MyStage>::default()`. Clone it into the worker.
- Worker: `job.check()?` / `job.is_cancelled()`; report with `set_fraction`, `set_step(done, total)`,
  `set_stage(s)` (a new stage resets step + fraction), `set_message`.
- Pixel algorithms: pass `&job.interrupt()` where they take a `pc-raster::Interrupt` (lock-free: reads the
  job's two atomics directly, no callbacks).
- APIs that take `&AtomicBool` (lc-fetch, li-seg `predict_*_with_cancel`, pc-engine seg): pass
  `job.cancel_flag()`. A bare flag can also become an `Interrupt::flag(&flag)`.
- UI: `job.fraction()` (None = indeterminate), `job.status()` for stage/step/message, `job.cancel()`.
- The status mutex survives a panicking worker (poison is ignored), so a crashed job still reads.

Per crate:
- `pc-raster/src/interrupt.rs`: now `pub use lightcraft_sysmem::jobs::{Cancelled, Interrupt};` — every
  existing import (`photocraft_raster::{Interrupt, Cancelled}`) unchanged; its two existing tests kept as-is
  and pass against the moved type.
- `li-ai/src/comfy.rs`: `JobControl` = newtype over `JobCtl<Stage>` with `Deref` (all 230+ call sites
  unchanged). It keeps exactly two li-ai behaviours: `set_step` enters `Stage::Sampling`, `check()` returns
  `anyhow::Result` (`is::<Cancelled>()`, text "Cancelled"). New: `ctl()` and `From<JobCtl<Stage>>`, so a
  runner-owned `JobCtl` can drive li-ai jobs; LaMa's `&|| ctl.check()` + `set_step` path is unchanged.
- `li-seg/src/lib.rs`: `Cancelled` is the shared type; `is_cancelled(&anyhow::Error)` now also recognises a
  cancel raised by li-ai or a pc-raster algorithm in the same job. The `&AtomicBool` args stay (JobCtl hands
  out its flag). The LaMa job had not touched these (checked `35c587cd`).
- `lc-fetch`: **not edited.** My base (`edfcbf91`) predates J8, which on `claude/v2-continue` rewrote lc-fetch
  into a thin mirror-list layer over `li-ai::download` driven by a `li_ai::JobControl` — i.e. after both
  merges lc-fetch already runs on `JobCtl` through the adapter. A `download_job(&JobCtl)` I first wrote
  against the old lc-fetch was dropped so this branch merges without conflicts (checked with
  `git merge-tree` against `claude/v2-continue`). Follow-up listed below.
- `lc-engine/src/cmd/cull.rs`: one loop (`run_with`) behind `Work::run(&Control)` (old, still used by
  `lc-ui-egui/src/tasks.rs`) and new `Work::run_job(&JobCtl)` (step = photos done / total). The synchronous
  `photo.analyze` command now runs through `run_job`.

## User-visible changes
- `pc-raster` and `li-seg` cancel errors now read "Cancelled" instead of "cancelled" (li-ai's spelling,
  which its tests and the editor's job status already expect). No code compares against the lower-case text
  of these two types (checked: the lower-case comparisons in lc-engine/lc-ui-egui are their own strings or
  lc-fetch's `DownloadError::Cancelled`, untouched).
- `li-ai::JobControl::progress()` after a worker panicked mid-update now returns the last status instead of
  an empty default.

## Quality / speed numbers
- Cancel-check cost in a hot loop (200 M `Interrupt::cancelled()` calls, progress every 65 536, dev profile
  = optimized, x86-64, throwaway measurement test, not committed): callbacks view 0.96 ns/check,
  `JobCtl::interrupt()` view 0.39 ns/check (2.5× cheaper: atomics, no indirect call). The callbacks path
  gained one enum match vs the old struct; within noise at this size.
- Memory: unchanged order (one `Arc` with two atomics + a mutexed status per job, same as `JobControl`).

## Checks run
- `cargo check --offline --workspace --all-targets`: clean.
- `cargo fmt` on the 6 touched packages; `cargo clippy --offline -p lightcraft-sysmem -p photocraft-raster -p
  li-ai -p li-seg -p lightcraft-fetch -p lightcraft-engine --all-targets -- -D warnings`: clean.
- Tests (filtered, only new + parity gate):
  - lc-sysmem `jobs::` — 5 new, pass.
  - pc-raster `interrupt::` — 2 existing, unchanged, pass against the moved type.
  - (an lc-fetch `download_job` test passed too, but was dropped with that code — see lc-fetch above.)
  - lc-engine `cull_run_job_counts_photos_and_stops_when_cancelled` — new, pass.
  - li-ai `job_control_is_a_view_of_the_shared_jobctl` — new, pass.
  - **Parity gate** li-ai `download::tests_cancel` — 6 existing, file unchanged, all pass; plus the 4
    existing `ds_comfy_coverage::job_control_*` tests (incl. `to_string() == "Cancelled"`), pass.

TEST-CHANGED: none.

## Outside edits
- `crates/lc-sysmem/src/lib.rs`: `pub mod jobs;` + root re-exports + one doc line.
- `crates/pc-raster/Cargo.toml`: new `lightcraft-sysmem` dependency edge (workspace crate; no new external
  dependency). `Cargo.lock`: +1 line (that edge).

## Left (and why)
- lc-fetch (post-J8, on `claude/v2-continue`): `download` still takes a caller `&AtomicBool` and bridges it
  into a fresh `JobControl` with a polling watcher. Add `download_job(.., job: &JobCtl<S>, ..)` that hands
  the caller's job straight to li-ai (`JobControl::from(...)` needs `JobCtl<Stage>`, so either make the
  li-ai download functions take `&JobCtl<S>` generically or keep one bridge) and move
  `lc-engine/src/segment/download.rs` to it; then drop the watcher. Needs the merged tree.
- `crates/lc-ui-egui/src/tasks.rs` still drives culling through `cull::Control` (J25 owns that file): move it
  to `JobCtl` + `Work::run_job`, then delete `Control` and `Work::run`.
- `lc-engine::enhance::{Job, JobCtl}` (its own `AtomicU32` progress/cancel) is J25's file: replace with
  `lightcraft_sysmem::JobCtl`.
- `crates/li-seg/src/denoise.rs` still defines its own `Cancelled` (not in my files): make it
  `pub use lightcraft_sysmem::Cancelled` so `li_seg::is_cancelled` also catches denoise cancels. Other
  li-seg modules signal cancel with ad-hoc strings (`restoration.rs`, `faces/cluster.rs`, `enhance.rs`);
  move them to `Cancelled` when those files are next owned.
- `pc-engine/src/seg.rs` converts errors to `li_seg::Cancelled`; now the same type as pc-raster's, so those
  conversions can collapse when the runner job touches it.
- Runner (`pc-engine::jobs` model hosted by both Sessions) and the one progress/cancel widget: J25 and the
  UI-kit slice.
