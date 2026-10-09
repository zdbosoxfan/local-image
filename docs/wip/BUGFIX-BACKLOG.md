# Bug-fix backlog (owner, 2026-10-09: "complete coding all of this, then a big bug-fixing run at the end")

Until every planned feature is merged, merges are gated on: compiles, clippy clean, the job's own new tests pass.
Full workspace + GPU (RTX 5090) suites, benchmarks and these items are handled in the final bug-fixing run.

## Known issues
- Verify PSD v7 slice descriptor enum names (horizontal Left/Cntr/Rght, vertical Top /Cntr/Btom, `bgColorType` "Clr ") against a PSD saved by Photoshop — written from memory in the bug-fix run (`crates/pc-psd`).
- pc-algo features: `ransac` panics when src and dst lengths differ (ignored test in `crates/pc-algo/tests/coverage/ds_features_coverage.rs`).
- Develop GPU tone equalizer: `tone_equalizer_extremes_masks_tiny_odd_and_cached_edits_match` and
  `native_toneeq_5090_extreme_fixture_matches` still fail on the RTX 5090 (a few pixels, max ~25 LSB on a 641×427
  extreme+mask case); dehaze speed fixed (338 ms at 24 MP). Typical edit 257–423 ms under heavy load — re-bench idle.
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

## Suspected bugs from DeepSeek coverage tests (verify first; each is an `#[ignore = "BUG: …"]` test)
- text contains with empty value should not match everything, but current code matches all — `crates/lc-catalog/tests/coverage/ds_rules_coverage.rs`
- encode panics on width=0 due to chunks_mut(0) — `crates/lc-codecs/tests/coverage/ds_jpeg_par_coverage.rs`
- RawProcessing accepts malformed JSON array — `crates/lc-develop/tests/coverage/ds_tools_coverage.rs`
- read_lmp accepts NaN and produces 'NaN' string in settings — `crates/lc-engine/tests/coverage/ds_preset_luminar_coverage.rs`
- parse_sidecar accepts arbitrary non-XMP input — `crates/lc-engine/tests/coverage/ds_sidecar_coverage.rs`
- dl(0, level) panics for level > 0 due to unsigned subtraction underflow — `crates/lc-pipeline/tests/coverage/ds_llf_coverage.rs`
- curve_at panics on empty LUT — `crates/lc-pipeline/tests/coverage/ds_llf_coverage.rs`
- num_levels(0,0) panics due to integer underflow — `crates/lc-pipeline/tests/coverage/ds_llf_coverage.rs`
- installed model built with default ModelDef loses required nodes, so availability incorrectly reports available with empty ObjectInfo — `crates/li-ai/tests/coverage/ds_catalog_coverage.rs`
- ransac panics when src and dst lengths differ — `crates/pc-algo/tests/coverage/ds_features_coverage.rs`
- subsample max=0 panics due to div_ceil(0) — `crates/pc-algo/tests/coverage/ds_segment_coverage.rs`
- display_at does not handle NaN rect coordinates — `crates/pc-engine/tests/coverage/ds_display_color_coverage.rs`
- path_from_resource accepts truncated data and returns empty path instead of None — `crates/pc-io/tests/coverage/ds_vector_map_coverage.rs`
- seed multiplication overflows u64 for large seeds — `crates/pc-paint/tests/coverage/ds_procedural_coverage.rs`
- v6 origin=1 with layer_id=None roundtrips as Some(0) — `crates/pc-psd/tests/coverage/ds_slices_coverage.rs`
- v7 descriptor does not preserve horizontal/vertical align and color — `crates/pc-psd/tests/coverage/ds_slices_coverage.rs`
- utf16_to_byte_lengths returns 2 bytes for an ASCII character — `crates/pc-text/tests/coverage/ds_psd_styles_coverage.rs`
