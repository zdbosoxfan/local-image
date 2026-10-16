# U030 AI Remove — frozen source-only v1

Source: /home/zdavidson/.codex/worktrees/ai-remove-workflow/Local Image - Testing. All deltas are against the immutable ai-remove-workflow-source-base seed in original recovery, not a full HEAD diff. The seed manifest records head528b6fd51ebc5dd6b82f63d1838720f58e8f29b1; the additionally authorized unchanged draft_refine fixture uses that exact seed commit as its before version.

Apply workflow.patch plus pending-strings.patch. jobs-terminal-hook.patch is a review-only extraction already included in workflow.patch; do not apply it twice. No other candidate paths may be replaced from file copies.

- workflow.patch SHA256 b6c400d184ae8f64b5129c0debd6b3d0a1b34d730facf4807cefa15727a34b8b
- pending-strings.patch SHA256 82e5ec9d85513d8bad548525a2ae800f1617634b0fcfed2c0726e545e3e5b9a4

## Behavior

Fresh AiOptions, including omitted serialized fields, applies painted removal on release. Explicit saved false remains false. A failure or unavailable engine retains the mask and requires explicit retry rather than submitting on repaint, later engine availability, or another refinement stroke. Restore detector masks stay explicitly manual even after additive painting or local Undo.

The single transient Pending captures document ID, active layer and dimensions. Tool changes retain it. Other tools get their normal keys and no Remove overlay/floating controls. Another active document/layer cannot paint into or submit that mask; Remove shows a return-to-original-target message and Cancel. Returning to the original target resumes it. Closing/deleting/resizing the source invalidates it instead of rebinding it. No per-document persistent store exists.

Clear zeros the mask and keeps an editable Remove workflow, with a local Undo snapshot. It neither edits the document nor exits the tool. To keep Clear undoable for a very large mask, its single latest snapshot may exceed the existing32MiB stroke undo budget after older entries are removed. Cancel discards the transient operation and exits to Move. If a job is in flight, cancellation goes through existing JobsUi cancellation and waits for the terminal outcome so painted work remains available for retry.

Submission retains Pending with its JobId until a matching ai.remove terminal event for its captured document succeeds. Failures and cancellation retain mask/local Undo and disable automatic retry. The approved one-line JobsUi hook runs beside terminal handling; U017 filter-default submitted-snapshot publication is unchanged. Existing Restore optional Colourise completion routing is retained.

The existing camera textured quad is retained for active Remove. Floating actions wrap inside the canvas at compact widths; empty Clear state uses the canvas center rather than an empty document rectangle. The existing engine readiness explanation/setup action is also available in the floating controls when blocked.

## Exact source ownership

- ai_remove_ui.rs: lifecycle, captured target, Clear/Cancel, active ownership, bounded floating layout, terminal handler.
- ai_remove_ui/tests.rs: six additional focused controls and explicit manual fixture setup.
- ai_ui.rs: AiOptions default/comment and one default/serde test only.
- jobs_ui.rs: exactly `crate::ai_remove_ui::on_job_event(app, &e);` after existing filter terminal hook.
- generate_ui/draft_refine/tests.rs: exactly the authorized omitted-field default assertion false→true.
- pending_vector.rs: two append-only strings supplied separately; worker source remains unchanged here.

## Authored acceptance (not executed)

Fresh-default actual pointer down/move/release with submit-count controls; synchronous failure and unavailability preserving document Arc/revision/history and requiring manual retry; matching/mismatched job/document terminal events with failed/cancelled retention and no automatic replay; real Brush switch/return including absence of mask texture/floating controls, Enter/Escape ownership, captured layer/document refusal and closed-source invalidation; changed-dimension invalidation; detected-mask additive refinement/Clear/Undo followed by explicit Enter; actual Clear/Cancel controls contained in canvas at760×480 and1440×900 in Pro/StudioLight at1.25ppp, including blocked Remove. Existing Rotate dormant-mask ordinary Undo regression remains intact.

The async terminal test is a component contract: TEST_REMOVE reports a pending local mock submission, then exact JobEvent values exercise the production terminal handler. It does not claim a real background worker, model, network or native pass. Coordinator must run affected lifecycle and actual job/native acceptance.

TEST-CHANGED: shared existing manual-mask fixture explicitly uses remove_immediately=false. New fresh-default coverage separately verifies true, so this does not hide the changed default. Existing Clear/Cancel test now asserts empty-but-retained mask plus local Undo, Cancel exit and unchanged document/history; document switch now asserts retained captured identity and inactive target instead of silent deletion. All existing HTTP/real command, local Undo, subtraction, selection, texture and detected Restore assertions remain. The one draft_refine assertion intentionally adopts the new omitted-field default; its other assertions are unchanged. No tests removed.

## Static verification / remaining work

Standalone Rust1.98.1 rustfmt completed; git diff --check passed. Reverse apply-check of workflow.patch and forward apply-check of pending-strings.patch pass. Snapshot manifest provides before/after hashes. No Cargo, builds, tests, GPU/model/inference, network, app control, commits or candidate edits performed. Independent review, compilation and runtime acceptance remain required.
