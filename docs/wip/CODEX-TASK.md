# Codex task: final-review bug-fix run — Vector tools and Pixel/Vector seams

Three independent reviews of the test build are in /home/zdavidson/.local/share/local-image-dev/reviews/:
- opus-final-review.md (Opus: O-### bugs, U-### UX) and opus-subreview-file-input.md (file-input items 1–12 + lower priority)
- astra-final-review.md (gpt-6-astra: F01–F14, speed proposals, UX, AI matrix; evidence under
  .claude/worktrees/codex-final-review-astra/docs/wip/final-review/ and target/final-review/)
- fable-partial-vector-ui.md (Fable: FB-1…FB-5)
Already fixed and landed (don't redo): O-001/F02, O-002, O-003, O-004, O-005 (hotfix e698208d) and the 4 known failing tests.

YOUR ITEMS: O-006, O-014, O-015, O-016, O-030, O-032, O-033, O-035, O-036, O-037, O-038, F04, F05, F08, F11 (= FB-1), FB-2, FB-3, FB-4, FB-5 (owner chose per-persona shortcuts, so Vector-persona chords may differ, but user-customised shortcuts must not be dropped and printed labels must match what keys do), U-010, U-012, plus any other O-### in the Medium/Low lists whose file is a vector_* / pc-vector / pc-pathops / pc-vecfx file.

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
Report: docs/wip/CODEX-REPORT-bugfix-vector.md — table: item, verdict after re-check, fix, test; exact commands + output.

## Retained upstream task handoff (already landed)

The coordinator branch also carried this completed build-gate assignment; its implementation and report are retained by this merge. The active assignment above remains the Vector task.

> # Codex task: final-review bug-fix run — Test and build-gate health

> YOUR ITEMS: Astra's report says the workspace had 11 failing tests and failing clippy and attribution gates at f73bc0ca — reproduce on the current head and fix all of them (real cause, not test weakening). Also: register the anchor-tools full-app GPU module in crates/pc-ui-egui/tests/gpu/main.rs (handoff 33) and make the gradient-mesh GPU/native gates runnable instead of #[ignore] where the handoff (43) says they should run (mesh/tests.rs:137, mesh/gpu_tests.rs:103,139, engine mesh/gpu_tests.rs:36). End state: workspace tests, strict clippy all-targets, attributions and deps-pure all green.

> Report: docs/wip/CODEX-REPORT-bugfix-gates.md — table: item, verdict after re-check, fix, test; exact commands + output.
