# Rotate View activation v2 — corrected frozen source handoff

Base: 3207abd2b23c6f1f4f93b2e06b25b2079d8a8810. Source checkout: /home/zdavidson/.codex/worktrees/editor-rotate-activation/Local Image - Testing. The v1 snapshot remains immutable. BASE-REPORT-v1.md retains the initial scope, adapter inventory and explicit scheduling declarations; this report supersedes its correction-sensitive statements.

## Integration

If v1 is already integrated, apply only corrections-from-v1.patch. Otherwise apply activation.patch OR integration-main.patch plus all six shared-hunks, never both. Shared hunk boundaries still protect U017 New-document defaults/openers and PDF Services/module registration. lib.rs remains exactly the previously granted early Rotate Escape call. File copies are review artifacts, not wholesale shared-file replacement instructions.

Full patch SHA256: 62c4485cbc6b99066398108472ef48e1a8535c7d643fadb804faad9db19fcd5e

Incremental correction SHA256: 8f167711d76e0072a5fcb159d5f51751623046b35de095de25daa8980b2c391c

manifest.json includes all 69 source/copy hashes and every patch hash. The correction is eight files; four newly included existing files are brush_resize.rs, shortcuts.rs, vector_integration.rs and ai_remove_ui/tests.rs. No native schemas, Library, compositor, model or PDF paths added.

## Review corrections

1. Rotated scrollbar ownership: the vector raw-input guard previously predicted unrotated bars while painting used rotated camera bounds. A tall image at 90 degrees exposed a real bottom bar and an imaginary right bar. scrollbars.rs now computes axis spans once through a shared helper for show/visible_bar_rects. vector_integration.rs builds the same exact-frame camera/size input and uses the visible rectangles. The actual-pointer regression checks bottom-bar pan with no vector capture, DocArc/revision/history change, then a right-strip vector stroke and Undo as the positive control.
2. Screen-axis brush resize: inverse-rotated input made horizontal screen movement change hardness at 90 degrees. brush_resize.rs captures camera basis and zoom at press, maps document deltas back to screen axes, and retains diameter scaling/hardness/coalescing semantics. canvas.rs publishes the exact primary View before captured tool routing, covering first Fit/resize/clamp state. Authored pointer controls cover Ctrl+Alt primary and Alt-secondary, both reflections, at 90 degrees and 1.25 pixels/point, with horizontal-only diameter, vertical-only hardness, one coalesced call, no paint or document/history change.
3. Saved Remove shortcut labels: using the null character as a shared key group resolved AiRemove to the first unbound tool. shortcuts.rs now gives unbound tools their own IDs; ordinary shared-letter groups and library:: applicability stay intact. Existing actual saved-R dispatch now also asserts the correct AiRemove ID and displayed R.
4. Dormant Remove-mask key ownership: keeping the mask during Rotate exposed Remove's Enter/Escape/Undo handlers while Remove was inactive. keys() still runs stale-target maintenance, then requires Tool::AiRemove before handling mask keys. A real key regression proves idle Rotate Enter/Escape retain mask and submit nothing; Ctrl+Z performs ordinary document Undo while retaining mask history; returning through an explicit saved F14 Remove binding makes Enter submit the exact mask once. The submission service is a local test callback, with no inference or network.

The actual Shift pointer control preserves the agreed free-angle behavior (23-degree movement after a 90-degree drag reaches 113 degrees) and existing document/export invariance assertions. No snapping increment was invented.

TEST-CHANGED: strengthens two new activation tests with saved-label and actual Shift-drag assertions; adds three focused regressions. No prior assertions, cases or numerical/hardware controls removed. There are now nine normal canvas::rotate_view::tests plus the new brush_resize and ai_remove_ui controls. Full-app physical GPU acceptance and all twelve explicit filters in PHYSICAL-GATES.md are unchanged and still required.

## Evidence and remaining acceptance

Standalone rustfmt completed for corrected Rust. git diff --check and reverse git apply --check on both full and incremental patches pass. All 69 worktree and copied-file hashes match. All 65 immutable v1 copies still match their original manifest. These are source checks only: no Cargo, compilation, test execution, GPU activity, inference, application control or commits performed.

Independent review and coordinator runtime acceptance remain required. Extra-window options_for_view tests are component evidence only; actual native extra-window gesture/typed/reset and Match All remain runtime acceptance, not an asserted native pass. Existing generated CPU/GPU screenshots are produced only when the coordinator runs the harness; none was created by this source-only worker.
