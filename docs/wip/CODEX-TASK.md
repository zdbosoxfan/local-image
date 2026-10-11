# Codex task: final-review bug-fix run — Saving, round trip and data safety

Three independent reviews of the test build are in /home/zdavidson/.local/share/local-image-dev/reviews/:
- opus-final-review.md (Opus: O-### bugs, U-### UX) and opus-subreview-file-input.md (file-input items 1–12 + lower priority)
- astra-final-review.md (gpt-6-astra: F01–F14, speed proposals, UX, AI matrix; evidence under
  .claude/worktrees/codex-final-review-astra/docs/wip/final-review/ and target/final-review/)
- fable-partial-vector-ui.md (Fable: FB-1…FB-5)
Already fixed and landed (don't redo): O-001/F02, O-002, O-003, O-004, O-005 (hotfix e698208d) and the 4 known failing tests.

YOUR ITEMS: O-007, O-008 (= Astra F01), O-009, O-010, F03 (PSD vector mask lost on save/reopen), O-020, O-021, O-022, O-029, O-031, O-045, O-046, O-047, O-048, plus U-006 (Delete with no prompt: follow Lightroom Classic's Remove / Delete from Disk / Cancel).

For each item: re-check it against the CURRENT code (the build moved since f73bc0ca); if real, fix it minimally with a
regression test that fails before and passes after; if not real or already fixed, say why. Stay inside your area's files;
if a fix needs a change in another area's files, keep it minimal and say so in the report. Reference-app behaviour
(Lightroom Classic / Photoshop / Affinity / Illustrator) is the UX standard; cite official help where you rely on it.
New user-visible strings go on the i18n pending lists.

Rules: builds via RUSTC_WRAPPER=li-rustc-gate (already set), Rust 1.98.1, --offline, no CARGO_BUILD_JOBS cap. Run your new
tests and the existing tests of files you changed, once; workspace check; strict clippy on touched crates. Don't remove or
weaken tests; if a test expectation is genuinely wrong, change it and add a line starting "TEST-CHANGED:" (or
"TESTS-REMOVED:") in the report with the reason — the merge queue rejects undeclared test removals. Never open GUI windows,
never stop the running Local Image app or ComfyUI. Leave changes uncommitted; the merge queue lands them.
Report: docs/wip/CODEX-REPORT-bugfix-save.md — table: item, verdict after re-check, fix, test; exact commands + output.
