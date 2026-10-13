# BUILD ui-offthread: AI selections off the UI thread (Claude, 2026-10-13)

Job: audit finding F13 (`docs/wip/AUDIT-multithreading.md` §E). Branch `wip/ui-offthread`.

## 1. F13 citations verified (all correct at base 7c4d800b)

| Cited | Found |
|---|---|
| `smartselect_cmds.rs:164-175` `select_subject` | yes (164) |
| `smartselect_cmds.rs:148-162` `object_selection` | yes (148; click mode 150-157) |
| `smartselect_cmds.rs:88-111` `quick_selection` -> `seg::quick_prior` | yes (88; prior at 103-108) |
| `select_extra_cmds.rs:49-70` `select_sky` | yes (49) |
| `seg.rs:47-52` `probability`, `:132-137` `sky_probability` | yes |
| `seg.rs:33-42` `rgba8` (composite + 8-bit) | yes |
| `seg.rs:98-120` `QUICK_PRIOR` | yes |
| `cutout_cmds.rs:182` `jobs::edit_job` | yes |
| `ai_cmds.rs:452-657` jobs | yes (`edit_job` 452, `run` 479/541/583/657) |
| `jobs_ui.rs:56-90` `run` only backgrounds opt-in commands | yes (58-90) |

None of the four used `jobs::run`, so `Session::start` (the UI path) ran them inline.

## 2. What changed

Status: DONE (F13 fixed). No jobs-plumbing change was needed: the existing `jobs::run` API covers it.

### Engine (`crates/pc-engine`)
- `smartselect_cmds.rs`
  - **Select Subject** (learned and classic): `jobs::run(.., lock_document = true, ..)`. The worker
    gets the document `Arc` (copy-on-write snapshot), the active layer, Sample All Layers and the
    model handle; the apply closure combines the region with the selection by the mode
    (replace/add/subtract/intersect) as one undo step ("Select Subject").
  - **Object Selection click mode**: same, label "Object Selection". Rectangle (GrabCut) mode is
    unchanged (inline; not part of F13).
  - **Quick Selection**: only the stroke whose prior needs the model becomes a job; strokes with a
    cached prior, or without Subject Assist / a model, run inline exactly as before (keeps
    brushing immediate). Cache check is a cheap value compare on the UI thread.
  - Stages check cancellation between model, edge refinement and the classical fallback; progress
    messages "Selecting subject" / "Selecting object" / "Refining edge".
- `select_extra_cmds.rs`: **Select Sky** (learned and classic) as a `jobs::run` job, label "Select
  Sky"; mode applied and coverage computed in the apply closure; "No sky was detected" result kept.
- `seg.rs`
  - `SubjectModel`: the model handle, resolved on the caller's thread when the command starts and
    moved into the job (the worker loads the model via `li_seg::shared` as before).
  - One probability cache (`PROBABILITY`, replaces `QUICK_PRIOR`) keyed by model identity + canvas +
    sampled pixels (the layer's surface, or all layers for Sample All / composite), shared by Quick
    Selection's prior, Object Selection clicks, Select Subject and Remove Background. "No clear
    subject" is cached too. The lock is not held while the model runs.
    - Key choice vs "document revision": `DocState::revision` also bumps on selection and view
      changes, so an Add-mode click would invalidate it. The pixel key (as `QUICK_PRIOR` used)
      hits across selection changes, undo back to the same pixels and different documents with the
      same pixels, and misses on any pixel or layer change. Single entry (a 24 MP map is ~96 MB).
  - `MockModel` + `mock_subject_model_on_thread` (`#[doc(hidden)]`, the same thread-local test-hook
    pattern as `develop_layer_cmds::gate_next_work`): counts runs and can hold each run on a gate.
  - `wanted()` uses the handle (so mocks count as installed); public `probability`, `subject`,
    `object_at`, `quick_prior` keep their signatures; new `*_with(&SubjectModel, ..)` variants.

### Shell (`crates/pc-ui-egui/src/jobs_ui.rs`)
- `non_modal_text(command)`: AI selection jobs show no modal progress dialog (it would block the
  canvas); the status bar shows "Selecting subject…" / "Selecting object…" / "Selecting sky…" /
  "Selecting…" with the existing × Cancel, and Esc cancels (existing `tick`).
- On completion a selection job's `message` ("No sky was detected") becomes the status text.
- UI handlers (`retouch_ui.rs` tool gestures/options bar, Select menu via `menus::invoke`,
  `panels.rs` button) already go through `app.run` -> `jobs_ui::run` -> `Session::start`, so they
  needed no change.

### Behaviour notes
- Canvas stays responsive: pan/zoom are view-only (`view.*` is document-independent).
- Another selection or any edit command on the busy document is refused with the existing job-lock
  message: “Select Subject” is still running on this document; wait for it to finish or cancel it (Esc).
- Stale results: the job snapshot pointer check (`apply_job`) discards the result if the document
  changed anyway ("the document changed while it ran; the result was discarded").
- Automation: `Session::execute` (scripts, actions, CLI, MCP inline) runs the job inline and
  returns the result synchronously, exactly like cutout. The control channel's `run_waiting`
  already waits on `last_started` jobs, so control requests still get the final result.
- Cancel unlocks the document at once and discards the result. The model call itself
  (`li_seg::Segmenter::predict`) cannot be interrupted, so a cancelled worker finishes that one
  inference in the background (its map still lands in the cache, which helps the next try).

## 3. Tests

| Test | Covers | Result |
|---|---|---|
| `smartselect_cmds::tests::select_subject_runs_as_a_job_the_caller_does_not_wait_for` | `start` returns while the model is held; job lock refuses another selection/edit; one undo step | ok |
| `..::cancelling_an_object_click_unlocks_at_once_and_changes_nothing` | Cancel | ok |
| `..::a_selection_job_on_a_document_that_changed_is_discarded` | stale-document rejection | ok |
| `..::the_selection_mode_is_applied_when_the_job_finishes` | add / intersect / subtract on completion; empty Add leaves history | ok |
| `..::repeated_object_clicks_reuse_the_model_map` | cache hit on repeated click (and after an Add click / Select Subject), miss on Sample All and on new pixels | ok |
| `..::quick_selection_backgrounds_only_the_stroke_that_runs_the_model` | Quick Selection prior job, later strokes inline | ok |
| `..::automation_still_gets_the_result_synchronously` | `execute` path returns results | ok |
| `select_extra_cmds::tests::sky_runs_as_a_background_job_and_adds_on_completion` | Sky job, Add mode, one undo step | ok |
| `jobs_ui_tests::select_subject_keeps_the_canvas_responsive_and_esc_cancels_it` | kittest: menu start returns at once, frames keep coming while pending, "Selecting subject…" shown, no modal, zoom works, other selection refused, Esc cancels | ok |
| `jobs_ui_tests::a_finished_select_subject_job_is_one_undo_step` | kittest: completion, one undo step, status | ok |

No real model needed (mock model); the opt-in real-model pattern was not used.

## 4. Threading checklist (`REVIEW-threading-policy-fable.md` §4)
- No new pools/threads: the existing named `photocraft-job-N` job thread (rule 3). No rayon
  closure waits (rule 5); the mock gate wait is on the job thread, not in rayon.
- UI thread now does only: parameter parsing, the `installed()` lookup it did before, an `Arc`
  clone, and (Quick Selection only) a cache-key compare (rule 4).
- Cancel flag checked between stages; progress via `JobCtx` (rule 9). Inference itself is not
  interruptible (li-seg API, see §6).
- Not done here (no infrastructure yet in pc-engine): heavy-job permit (rule 6) and thread
  priority class (rule 7) — the job thread is normal priority like every other pc-engine job.
- Not measured on real 24/60 MP files (rule 12): no machine time budgeted for model runs in this
  job; the UI-thread cost is now independent of image size except the key compare.
- Results are identical to the previous inline code (same functions, same order); nothing
  accumulates across threads (rule 11).

## 5. Final checks
- `cargo fmt -p photocraft-engine -p photocraft-ui-egui`: only the touched files changed.
- `cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`: clean.
- `cargo test --offline -p photocraft-engine --lib`: 1434 passed, 0 failed, 27 ignored.
- `cargo test --offline -p photocraft-ui-egui --lib`: 1480 passed, 0 failed, 48 ignored.
- `Cargo.lock` unchanged; no new dependencies.

## 6. Needs from other owners
- li-seg (segmentation-model owner): `Segmenter::predict` / `predict_sky` take no cancellation
  flag. Adding an `&AtomicBool` (or `Interrupt`) checked between the tract run stages, or at least
  before the upscale, would let Cancel stop the CPU work instead of only discarding it.
- i18n (next translation pass): "Selecting subject…", "Selecting object…", "Selecting sky…",
  "Selecting…" (in `jobs_ui::non_modal_text`, shown via `tl!`) need catalog entries.

## 7. TEST-CHANGED
None. Existing assertions unchanged.
