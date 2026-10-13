# BUILD cl-airemove: AI Remove mask workflow, Liquify / Frame Composition / shape-name polish (Claude)

Branch `wip/cl-airemove`, base `90d89ff3`. Starting point: Codex's saved, unreviewed patches in
`docs/wip/checkpoint-2026-10-12/ai-remove/` and `.../ux-presentation/`. Both were applied, compiled,
reviewed against `UX-NEXT-AFTER-ROTATE.md` Batch A and the job brief, then corrected (below).

## How the patches went in

| Patch | Result |
| --- | --- |
| `ux-presentation/implementation.patch` | Applied cleanly, then changed (Liquify, Frame Composition, boolean names). |
| `ai-remove/workflow.patch` | Applied except `ai_ui.rs` (context conflict) and `generate_ui/draft_refine/tests.rs` (not owned). `ai_ui.rs` re-done by hand: default `remove_immediately: true`, doc comment, and the serde default test. |
| `ai-remove/jobs-terminal-hook.patch` | Already inside `workflow.patch`; not applied twice. **Still needed**: `ai.remove` runs through `jobs::run` (a locked background job). The new non-modal selection jobs (`jobs_ui::non_modal_text`) cover only `select.*`. Without the hook, a failed or cancelled background removal would lose the mask. |
| Both strings patches | Re-appended by hand at the tail of `i18n/pending_vector.rs`, but only strings the UI actually shows (see Strings). |

## U030 AI Remove

| Requirement | Where / how |
| --- | --- |
| Fresh default applies on release; saved `false` survives | `ai_ui.rs` `AiOptions::default` → `true`. `#[serde(default)]` means an omitted field gets `true`; a saved `false` stays `false`. Test `remove_release_default_preserves_explicit_saved_manual_choice`. Note: an existing settings file already contains `"remove_immediately": false`, because the old default was always written out. Those users keep manual mode. That is the "explicit saved false survives" rule; it cannot be told apart from a real choice. |
| A stroke submits once on release | `paint` (called from `ai_ui::finish_stroke` on release) submits only if the mask is non-empty, the stroke was not a subtract, and the mask is not marked "needs confirmation". Test `fresh_remove_submits_only_on_release_...` counts submissions across down, move, release, repaint and Enter. |
| No silent loss on tool switch | **Retained, inactive.** `sync` no longer drops the mask when the tool changes. It drops it only when the captured document is closed, its size changes, or the captured layer is deleted. While another tool is active the mask has no keys, no floating bar and no overlay. Navigation tools (Rotate View, Hand, Zoom) still show the overlay; this is my addition. Returning to Remove resumes the same mask with its local Undo. |
| Clear vs Cancel | **Clear** zeroes the mask and bounds, keeps the workflow, and pushes one local Undo snapshot (Ctrl+Z restores it). It also closes the floating bar, since nothing is left to confirm; the options bar keeps the controls. **Cancel** (button or Esc) discards the mask, its local Undo and any Restore follow-up. Changed from the patch: Cancel **keeps the Remove tool selected**, as Photoshop's Remove tool does, instead of switching to Move. With a job in flight, Cancel cancels the job through `jobs_ui::cancel`, and the terminal event keeps the mask. |
| Undo and subtract preserved | Local stroke Undo and Alt-subtract are unchanged. Existing tests still pass (`bugfix_vector_remove_undo...`, `..._merge_subtract_and_escape...`, `ds_finding_remove_bounds...`). |
| Failures and unavailable engine keep the work | Synchronous error: the mask is restored and marked "needs confirmation", so no automatic retry. Engine not ready: the mask is kept (the patch used `unwrap()`; rewritten without it). Background job failed or cancelled: `on_job_event` (via the `jobs_ui.rs` hook) keeps the mask and clears the job. Only a matching `ai.remove` `Done` for the captured document consumes it. When blocked, the floating bar also shows the existing readiness/setup row. |
| No migration | `Pending` captures document ID, active layer and size. On another document or layer, painting, Enter and Remove do nothing, and the options bar says "Return to the original image and layer to continue removal, or Cancel." Closing or resizing the source drops the mask. It never rebinds. "Remove Selection" (menu, context bar, options bar) and Restore's `detected` no longer replace a pending mask. The first switches to Remove and says "Finish or cancel the current removal mask first."; the second returns that message as its error. |
| Restore masks wait for approval | `detected` sets "needs confirmation". Additive painting, Clear and Undo never auto-submit; Enter or Remove is required. Existing `restoration_detected_mask_fill_once_then_optional_colourise_preview` passes (it already holds `AI_TEST_LOCK`). |
| Layout 760×480 and 1440×900, light and dark | The floating bar's width is capped to the canvas and wraps when narrow. Changed from the patch: actions and engine share **one** wrapped row, so the bar is one row tall at normal widths. The patch's two rows made the bar cover the next stroke position, which broke two existing tests. Test `remove_confirmation_controls_fit_compact_and_large_light_dark_frames` covers Pro and Studio Light at 1.25 ppp, including the blocked engine. |

**Why retain instead of asking Keep/Discard.** Photoshop's Remove tool, with "Remove after each stroke" off, holds unapplied strokes until you click the options-bar checkmark or cancel. It never drops painted strokes behind your back. I did not verify what Photoshop does on a tool switch, so I am not claiming parity there. Retaining is the cleaner choice for three reasons:
- It is the only option that neither interrupts with a modal on every tool switch nor applies or discards anything without a click.
- The mask is not document content, so keeping it costs nothing and cannot edit the image by accident.
- It needs no new dialog, shortcut or central routing change (`lib.rs`, `shortcuts.rs` untouched).

Accepted trade-off: the mask stays invisible under editing tools, and it shows again as soon as Remove is selected.

## UX07 Liquify

The footer shows plain text only: `Strokes: N` (translatable `Strokes: {count}`). Field px/node and last-dab time now appear only when the app is started with `LOCAL_IMAGE_DIAGNOSTICS=1`. This is a developer switch, read once.

The patch put them in a "Performance diagnostics" collapsing fold inside the user's Liquify panel. That is still developer chrome for a Photoshop user. It was also a raw `egui::CollapsingHeader`, because the editor has no shared disclosure widget, and `widgets.rs` is outside this job's ownership. The diagnostic rows are developer-only, so they are plain English and not in the catalogue. The numbers stay available to automation (`dabMs`). Test: `normal_feedback_is_plain_and_metrics_need_the_developer_switch`.

## UX09 Frame Composition

When there is no document or selection, or the selection is a shape, pixel layer or point text, the panel shows **"Select a frame text layer"**. Below it is a wrapped hint: "Frame Composition works on text set in a frame. Drag with the Frame Text Tool to make one." Valid box text keeps its existing working controls; the four existing frame tests pass. Test: `inapplicable_frame_selection_has_a_bounded_nonmutating_empty_state` (220×160, both lines inside the window, no document or journal change).

## U032 names

- **New live shapes** take the kind name from one engine helper, `vector_packages::drawing::layer_name`: Rectangle, Rounded Rectangle, Ellipse, Polygon, Star, Line, Callout and so on, through Polar Grid. The UI preview and the pointer commit both use it, translated through `i18n::t`. `shape.design.create` takes an optional `name`; a non-blank one is kept as given, and a non-string fails before any change. The older `shape.create` path already used kind names (`vector_cmds::default_name`).
- **Boolean and Pathfinder results.** The actual cross-layer provider is `pc-engine/src/pathops_cmds.rs::run`, the Vector Geometry bar. It used to clone the first source's name. Results are now named after the button that made them:
  - Add → **Union** (changed from the patch's "Add", which reads as a command)
  - Subtract, Intersect, Xor, Divide
  - Trim, Merge, Crop, Outline, Minus Back

  Every piece of a multi-result Divide uses the name. An optional `name` overrides it.
- Photoshop's own Layer › Combine Shapes (`layer.combineShapes.*`) works inside one shape layer and keeps that layer's name, which is Photoshop's convention. It is unchanged.
- Undo, redo and native save/load are unchanged. Tests: `boolean_multi_results_use_operation_or_explicit_name_and_undo_restores_sources`, now looped over divide, add, subtract and pathfinder-merge with default, blank and explicit names; `shape_names_follow_kind_preserve_explicit_names_and_survive_native_roundtrip`; `shape_preview_and_pointer_commit_share_the_kind_name`.

## Strings (append-only, `i18n/pending_vector.rs`)

- "Finish or cancel the current removal mask first."
- "Return to the original image and layer to continue removal, or Cancel."
- "Strokes: {count}"
- "Select a frame text layer"
- the Frame Composition hint
- "Callout", "Ellipse Callout"

Not added, from the patches: engine error texts (not `tl!`), the developer diagnostic rows, and the dropped "Performance diagnostics", "Strokes" and "Select frame text".

## TEST-CHANGED

- `ai_remove_ui/tests.rs` `remove_confirmation_strokes_merge_subtract_and_escape_without_edits`. Its last assertion expected that switching to Move **discards** the mask, which is the U030 bug. It now asserts the mask is retained unchanged, and that the document and history are unchanged.
- `ai_remove_ui/tests.rs` `remove_confirmation_selection_clear_cancel_keep_painting_and_document_switch`. It used to expect Clear and Cancel both erase the mask, and a document switch to delete it. Codex's update now asserts:
  - Clear empties the mask and keeps the workflow, and Ctrl+Z restores it.
  - Cancel discards with the document, revision and history unchanged.
  - A document switch keeps the captured target inactive.

  I changed its "tool becomes Move" expectation to "tool stays AI Remove" (see Cancel above).
- The shared manual-mode fixture `harness()` now sets `remove_immediately = false` explicitly (`harness_mode(false)`). Fresh-default coverage is a separate test, so the fixture does not hide the default change.
- No assertion was removed or weakened. No test was ignored.

## Needs from other owners

1. **`crates/pc-ui-egui/src/generate_ui/draft_refine/tests.rs:345`** is not in my ownership. It asserts the old default:
   ```diff
   -    assert!(!old.remove_immediately);
   +    assert!(old.remove_immediately);
   ```
   An old settings file without the field now gets the new fresh default, `true`. This is exactly the line Codex's `workflow.patch` authorised; the other assertions in that test are unchanged. It is the single failure in the pc-ui lib run below, and it is a TEST-CHANGED item for whoever applies it.
2. Follow-ups, not done:
   - The existing Frame Composition controls still use raw `egui` checkbox, `DragValue` and button. Moving them to `widgets::checkbox`, `value_field` and `primary_button`/`secondary_button` would need the typed-entry tests rechecked.
   - The editor has no shared disclosure widget; the library has `disclosure`. Add one to `pc widgets.rs` if more folds appear.

## Validation (Rust 1.98.1, `--offline`, `claude-env.sh`)

- `cargo fmt -p photocraft-engine -p photocraft-ui-egui`: only owned files changed.
- `cargo clippy --offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`: clean.
- `cargo test --offline -p photocraft-engine --lib`: **1454 passed, 0 failed, 27 ignored**.
- `cargo test --offline -p photocraft-ui-egui --lib`: **1548 passed, 1 failed, 51 ignored**. The failure is `draft_refine_zoom_pan_comparison_and_saved_defaults`, the expected old-default assertion in Needs 1.
- Focused before that: `ai_remove_ui` 15/15, `frame_compose`, `liquify_ui`, `vector_packages::drawing`, `rotate_view_mask_retention...`, `restoration_ui`, `every_tl_literal_is_translated` and `bundled_catalogs_are_consistent` all pass; `pathops_cmds` and `vector_packages::drawing` in the engine pass (21).

Not run: native app, real model or GPU (UX-ACCEPTANCE N06 remains a coordinator gate). The mock-socket HTTP tests self-skip if the sandbox denies sockets.
