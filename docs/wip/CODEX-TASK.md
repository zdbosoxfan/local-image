# Codex task: hotfix the critical regressions found in the test build (f73bc0ca)

The owner is testing build f73bc0ca right now. Claude Opus's final review reproduced these regressions headless on that
build. Full details (root cause, file:line, repro) are in
/home/zdavidson/.local/share/local-image-dev/reviews/opus-final-review.md — read findings O-001, O-002, O-003, O-004
(and O-005/O-006 if they share the root cause with O-002). Repro drivers: /tmp/claude-1000/opus-review-snapshots/ and the
scratchpad pcsnap/ drivers mentioned there.

Fix ONLY these, minimally, each with a regression test that fails before and passes after:
- O-001: Pixel options bar empty for most tools (vector tool-options scroll area at crates/pc-ui-egui/src/panels.rs:693
  takes the whole row). Both personas' option bars must show their controls.
- O-002: drawn shapes (Rectangle/Ellipse/Polygon/Line) don't visibly move/transform on the canvas in either persona while
  exports show the new position (stale canvas cache/projection).
- O-003: in the Vector persona, typing into an active text edit is captured as tool shortcuts. Text editing must own the
  keyboard (same as Pixel persona / Photoshop / Affinity).
- O-004: Save as PSD / layered TIFF fails after placing SVG/PDF/Affinity or using Repeat, Blend, Vector Crop; must save
  (use the PSD advanced-vector fallback from package 23 with its editability-loss report) so the Library round trip's
  -Edit.psd save works.
Do not touch anything else. Rules: RUSTC_WRAPPER=li-rustc-gate (already set), Rust 1.98.1, --offline, no CARGO_BUILD_JOBS
cap. Run your new regression tests plus the existing tests of the files you changed, once; workspace check; strict clippy
on touched crates. Leave changes uncommitted; the merge queue lands them. Report: docs/wip/CODEX-REPORT-hotfix-critical.md
with exact commands and outputs.
