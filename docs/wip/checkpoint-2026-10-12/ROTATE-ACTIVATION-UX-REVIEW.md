# Rotate View activation — independent UX/lifecycle review

## Corrected V2 — scoped UX source READY

Verified full patch `62c4485cbc6b99066398108472ef48e1a8535c7d643fadb804faad9db19fcd5e` and correction delta `8f167711d76e0072a5fcb159d5f51751623046b35de095de25daa8980b2c391c` in `/tmp/editor-rotate-activation-v2`. All69 source-copy hashes and every listed patch/shared-hunk hash match. **RV-UX1 is resolved by source inspection; no remaining blocker found in this review's UX/lifecycle scope.** Overall source readiness still requires `state_color_triage`'s independent mapping/scrollbar/brush-resize disposition; this note does not substitute for it. No tests/builds/render/native actions executed.

The correction runs existing stale-target mask synchronization, then requires actual AiRemove tool ownership before handling mask keys. Rotate's retained mask is therefore dormant while idle as well as during an owned drag. New real-key coverage establishes a nonempty two-stroke mask/local undo, asserts idle Rotate Enter/Escape do not submit or alter it, explicitly proves Ctrl+Z reaches ordinary document Undo with mask/history retained, and returns via saved F14 to Remove where Enter submits the exact mask once. This is a meaningful negative+positive owner test; no inference is involved.

The Shift pointer case now performs a23-degree free-angle drag from90 to113 degrees and retains the existing document/revision/history/dirty/PNG invariance checks. It documents the agreed free-angle modifier behavior without introducing snap increments. Saved-AI-Remove shortcut coverage now asserts its unique command ID and displayed R as well as existing actual dispatch; the narrow unbound-tool lookup correction leaves ordinary key groups intact.

V2 report correctly labels extra-window options tests as component evidence and leaves actual native extra-window gesture/typed/reset/Match All acceptance outstanding. Existing mandatory physical gates and tolerances remain required. Merge only the supplied shared hunks around U030/PDF/U017 work, retaining the actual-owner Remove key guard and early Rotate Escape ordering. Source READY is not a claim of compilation, native UX or complete U016 runtime acceptance.

## Frozen V1 review (historical finding corrected above)

Reviewed frozen `/tmp/editor-rotate-activation-v1/{REPORT.md,manifest.json,activation.patch,files/}`. Base `3207abd2b23c6f1f4f93b2e06b25b2079d8a8810`; patch SHA256 **76ce97c0dd5847cd5763fb561f122f16dbe38784b45eb4f6835b7e0209693384**. All65 source-copy hashes match the manifest. No builds, tests, rendering, native-app control, network or source edits were performed. This report is the only write.

**V1 disposition: BLOCKED pending corrected frozen snapshot.** One additional lifecycle defect is below. The separate `state_color_triage` review owns camera math/render mapping and the already-reported scrollbar visibility/input guard, brush-resize screen basis and saved-AI-Remove shortcut-label findings. Those must also be corrected; this report does not duplicate or waive them. Source readiness cannot be issued from v1 alone.

## RV-UX1 — P1: retained AI Remove mask still owns idle Rotate keys

Frozen `ai_remove_ui.rs::sync` now preserves Pending while `Tool::RotateView` is selected, which is appropriate for navigation. But `ai_remove_ui::keys` still handles Ctrl+Z, Escape and Enter whenever Pending exists, without requiring the Remove tool. The new early `rotate_view::handle_keys` hook consumes Escape only when a Rotate drag is active.

Concrete sequence: obtain a detected/painted Remove mask, select Rotate View, leave no active rotation drag and put focus on the canvas. Enter satisfies `app.drag.is_none() && !ctx.text_edit_focused()` and calls `remove`, submitting an AI operation from a navigation tool. Escape deletes the retained mask; Ctrl+Z consumes local mask undo. The v1 mask test checks only Escape during an active Rotate drag, when the early hook hides this defect.

Required correction: dormant Remove work must not receive navigation-context keys. Gate Remove keyboard handling by its actual tool/input owner, preserving the captured pending mask while Rotate owns navigation. Returning to Remove must restore normal mask Enter/Escape/local Undo. Preserve the early Rotate drag Escape hook, which is still needed before global job/AI handlers. Coordinate the narrow hunk with the independent U030 owner so neither patch overwrites the other's pending-mask behavior.

Required regression: retained nonempty mask and local undo, idle Rotate with canvas focus, actual Enter/Escape/Ctrl+Z. Assert no AI submission and no mask/undo mutation. Ctrl+Z must still reach the ordinary document Undo owner; **do not** suppress global Undo merely to assert the document never changes after an explicit Undo command. Then return to Remove and prove intended local keyboard behavior, plus the existing active-Rotate-drag Escape restoring initial angle/mask intact. Owner confirmed the finding and the intended correction during review.

## Source paths reviewed without another confirmed UX blocker

- `rotate_view.rs::input`: press captures DocId/viewport/initial angle, not the later drag threshold. Escape restores the initial angle and removes capture; the still-held pointer cannot start again without a new press. Release retains angle. Tool/focus/modal/context mismatch clears stale capture without targeting a new view. It returns ownership before ordinary document-tool dispatch.
- `rotate_view.rs::handle_keys` plus the one-line `lib.rs` hook: owned main-view Escape is resolved before global job/AI Escape handling. It respects canvas keyboard availability and clears blocked/stale captures. The retained-mask idle-key omission is RV-UX1, not a reason to remove the early hook.
- `Tool::RotateView`, engine toolbar registries and View-menu commands are separate from object `Tool::Rotate`. Fresh Pixel R is intended for Rotate View, while explicit saved bindings/unbinding retain authority and Vector preset R remains object Rotate. The other reviewer found the saved-Remove label/group lookup defect caused by the changed null key; wait for that correction instead of considering dispatch-only success sufficient.
- `options_for_view`: typed finite angle/reset changes only the supplied View, with invalid/nonfinite command values refused and no document command. Main options use the active document's view; extra-window controls receive their own cloned/written-back view. `float_window` clears transient capture. `sync_views` continues moving views/windows by document identity after tab changes.
- `view_cmds::arrange`: Match Rotation and Match All copy actual angle and clear destination drag capture, reporting degrees instead of the former constant zero. Fit/Reset distinction remains: fitting preserves the angle, Reset clears the angle rather than transforming document data. Detailed fit/math fidelity is the other reviewer's scope.
- `persona::cancel` clears captures in main and extra views on explicit tool/persona changes. No document/native persistence schema is introduced; View gains default-zero angle and serde-skipped capture. This is UI-state compatibility, not a new promise that reopening an image restores camera rotation.

## Shift clarification — preserve original scope

Root requested Shift behavior review. V1 has no Shift-specific angle constraint. The owner explicitly confirmed that no snap increment was agreed in original U016 or the activation brief: Shift currently leaves free-angle drag unchanged; Shift-to-cycle remains tool-selection behavior. Treat this as a modifier/no-document-edit acceptance case, not authority to invent15-degree snapping or a new control. Original U016 requires real Rotate View, not touchscreen rotation or restart-camera persistence; neither is added to the acceptance scope here.

## Authored tests and their practical limits

The ordinary full-app CPU test exercises real View menu selection, press/motion/release, held-pointer Escape, typed37-degree angle, Reset, unchanged document Arc/revision/history/dirty state and exact PNG export bytes. The R test covers fresh/default/saved override/explicit disabled binding and Vector object/view distinction. Tab reorder/captured DocId tests and truthful Match Rotation inspect the actual application state; first-frame Restore modal blocks rotate/navigation. Compact760×480 at1.25x checks full option rectangles against painted clip and viewport. These are meaningful authored assertions, **not executed results**.

The “extra window controls” test constructs a pair of View values and renders `options_for_view` on the second; it proves helper-local Reset without changing the first. It does **not** exercise a native extra viewport's press/drag/Escape/focus/title controls. The source hook is present, but real independent extra-window typed/reset/drag and Match All remain runtime/native acceptance, alongside explicit tool-switch/focus-loss cases. Shift free-angle/no-edit behavior needs an explicit modifier case if the updated report claims coverage. No test should be marked passed by inspection.

The GPU test is an opt-in required physical RTX5090/Vulkan gate. Eleven preexisting strict physical tests plus that new test were moved to explicit scheduling; the worker provides attribute-only strip-equality proof and twelve exact mandatory invocations. They are not successful skips or waived work. Existing numeric tolerances, hardware identity and zero-angle assertions must remain intact. `state_color_triage` validates mapping/oracle details; this review makes no independent GPU/math claim.

## Integration/re-review boundary

Wait for a corrected immutable snapshot and hash manifest including RV-UX1 and the separate reviewer's findings. Re-review those deltas and affected UI seams before source READY. Apply either full activation patch or main-plus-shared-hunks, never both; shared `lib.rs` early Escape hunk, state Tool/View hunks, menu additions, canvas changes, AI Remove and append-only strings must be merged around current U017/PDF/U030 work. Preserve newer shared-shell/Tab/modal behavior and source identities. No arbitrary full-file replacement from the older snapshot.

Final behavior acceptance still requires the coordinator's scoped compile/lint/affected cases, required physical gate and inspected native/rendered flows. This review does not mark original U016 complete before actual rotation activation works in the application.
