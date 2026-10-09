# Bug-fix backlog (owner, 2026-10-09: "complete coding all of this, then a big bug-fixing run at the end")

Until every planned feature is merged, merges are gated on: compiles, clippy clean, the job's own new tests pass.
Full workspace + GPU (RTX 5090) suites, benchmarks and these items are handled in the final bug-fixing run.

## Known issues
- Develop GPU tone equalizer: `tone_equalizer_extremes_masks_tiny_odd_and_cached_edits_match` and
  `native_toneeq_5090_extreme_fixture_matches` still fail on the RTX 5090 (a few pixels, max ~25 LSB on a 641×427
  extreme+mask case); dehaze speed fixed (338 ms at 24 MP). Typical edit 257–423 ms under heavy load — re-bench idle.
- pc-algo HDR: `mtb_offset` reports a nonzero offset for identical images on ties; HDR merge panics on empty input
  (tests in `crates/pc-algo/tests/panorama_hdr_coverage.rs`, ignored) — a Sonnet fix may already be merged; re-check.
- pc-algo inpaint: `mvc_membrane` caps channels at 8 (test ignored; low impact).
- Translation review (DeepSeek, ~1,850 guarded fixes) parked on `wip/ds-i18n-review`; 2 pinned-term tests to reconcile.
  Owner: translations are lowest priority.
- `scripts/generate-trace-stage-fixtures.py` (pc-trace golden regeneration helper) was lost with its worktree; recreate
  from `crates/pc-trace/tests/golden/SOURCES.md`.
- GPU validation pending on the 5090 for: Develop tone-equalizer/dehaze follow-up (wip/codex-gpu-toneeq), colour icons,
  any compositor/vector rendering changes since the last 5090 run.
- Vectorizer competitor gates (vtracer/potrace/Inkscape references) — results pending.
- Flaky/unknown: occasional NVIDIA shader-compiler hang (`banded_full_size_render_matches`), crashes logged in
  `libnvidia-glcore` from test processes.
- Coverage tests from the DeepSeek dispatcher: any `#[ignore = "BUG: …"]` they add is a backlog item.
