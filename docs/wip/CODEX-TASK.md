# Codex task: final-review bug-fix run — Library and Develop behaviour + Lightroom Classic conventions

Three independent reviews of the test build are in /home/zdavidson/.local/share/local-image-dev/reviews/:
- opus-final-review.md (Opus: O-### bugs, U-### UX) and opus-subreview-file-input.md (file-input items 1–12 + lower priority)
- astra-final-review.md (gpt-6-astra: F01–F14, speed proposals, UX, AI matrix; evidence under
  .claude/worktrees/codex-final-review-astra/docs/wip/final-review/ and target/final-review/)
- fable-partial-vector-ui.md (Fable: FB-1…FB-5)
Already fixed and landed (don't redo): O-001/F02, O-002, O-003, O-004, O-005 (hotfix e698208d) and the 4 known failing tests.

YOUR ITEMS: O-013, O-023, O-025, O-026, O-027, O-028, U-003, U-004, U-005, U-007, U-008, U-009, F09, plus any other O-### in the Medium/Low lists in lc-* crates or apps/local-image library/develop files. Keymap and menus must follow Lightroom Classic (E Loupe, D Develop, R Crop, Q Spot, K Brush, L Lights Out, Ctrl+Shift+E Export, Ctrl+E Edit In, Ctrl+Shift+S Sync Settings reachable from menu/button/key).

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
Report: docs/wip/CODEX-REPORT-bugfix-develop.md — table: item, verdict after re-check, fix, test; exact commands + output.
