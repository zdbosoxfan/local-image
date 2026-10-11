# Codex task: final-review bug-fix run — AI features

Three independent reviews of the test build are in /home/zdavidson/.local/share/local-image-dev/reviews/:
- opus-final-review.md (Opus: O-### bugs, U-### UX) and opus-subreview-file-input.md (file-input items 1–12 + lower priority)
- astra-final-review.md (gpt-6-astra: F01–F14, speed proposals, UX, AI matrix; evidence under
  .claude/worktrees/codex-final-review-astra/docs/wip/final-review/ and target/final-review/)
- fable-partial-vector-ui.md (Fable: FB-1…FB-5)
Already fixed and landed (don't redo): O-001/F02, O-002, O-003, O-004, O-005 (hotfix e698208d) and the 4 known failing tests.

YOUR ITEMS: F06, F07, F12, F13, O-024 (AI Remove/Enhance result applied to the wrong photo after switching), O-044, Astra's AI matrix follow-ups: RealVisXL V5 Lightning must use the model author's documented sampler/steps/CFG (cite the HF card), and Qwen AI Remove leaves a visible rectangular fill boundary (blend it like the Klein path). Real-model checks may use the local ComfyUI one job at a time; check GET /queue is empty first.

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
Report: docs/wip/CODEX-REPORT-bugfix-ai.md — table: item, verdict after re-check, fix, test; exact commands + output.
