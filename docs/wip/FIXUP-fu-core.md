# Fix-ups: fu-core (engine, Library and tooling) — Claude

Branch wip/fu-core (based on claude/v2-continue). Complete.

## 1. PRIORITY: Auto Sync replaced other photos' layers (data loss): FIXED

- Diagnosis: `Session::auto_sync_ops` (crates/lc-engine/src/lib.rs) builds the sync delta with
  `json_delta`, which recurses into objects but takes arrays whole. `masks` is an array, so any
  layer/mask edit on the active photo put the active photo's entire `masks` array into the delta,
  and `transfer_settings` then replaced every other selected photo's layer stack with it.
- Fix: `masks` is removed from the Auto Sync delta along with `spots` / `red_eye` / `version`.
  Layers reach other photos only through explicit Sync / Copy-Paste / presets (unchanged).
  Global sliders still auto sync.
- New test: `tests_layers::auto_sync_keeps_each_photos_own_layers` (B keeps its own two layers
  through a global slider change, setTools, setOpacity, add and delete on A; global exposure and
  contrast still reach B). Fails without the fix, passes with it.
- TEST-CHANGED (both encoded the data-loss behaviour):
  - `tests_layers::copy_paste_presets_and_auto_sync_carry_layers`: the last block expected an
    Auto Sync layer tool change to reach the other photo; it now asserts the active photo got the
    change and the other photo's layers are unchanged.
  - `tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks`: expected
    the Auto Sync AI mask edit to give B a Sky mask; it now asserts B keeps its own (Subject,
    recomputed) mask. The paste / sync / preset checks in that test are unchanged.
- Note (not changed): other arrays (`point_colors`, curve point lists, `disabled_sections`) are
  still taken whole by Auto Sync. They are global settings (like Lightroom syncing a whole curve),
  so that is the intended behaviour, but a photo's own Point Color list is replaced when the
  active photo's list changes.

## 2. photocraft-engine lib failures

### 2a. Rotate View / AI Remove "R" (toolsets + toolbar46 snapshots): FIXED (stale snapshots)
- Diagnosis: the Rotate View integration (30fbdff2) already applied the Photoshop rule in the
  engine: `prefs::TOOL_SHORTCUTS` gives `tools.select.RotateView` = R and `tools.select.AiRemove`
  no key of its own; `toolsets.rs` added RotateView to All Tools / Photographer / Essentials /
  Retouching / AI and to the vector registry. The two tests compared against snapshots written
  before that (no RotateView anywhere, AiRemove = R). The engine table is correct, so only the
  snapshots were stale. AI Remove stays a bindable row with no default (a user's saved key for it
  still shows in Preferences and can be cleared); it reaches J through the UI's healing group.
- TEST-CHANGED (snapshot refresh, no assertion removed):
  - `crates/pc-engine/tests/fixtures/persona/pixel-toolbar.json`: RotateView after Hand in
    `allTools` and `photographer`.
  - `crates/pc-engine/src/vector_toolbar_layout/pixel-tool-keys.json`: `tools.select.RotateView`
    = R after Hand; `tools.select.AiRemove` = null (was R).
  - `toolsets::tests::persona_pixel_snapshot_and_vector_photographer_contract`: the vector
    Photographer list includes RotateView after Hand (it is registered for Vector since 30fbdff2).
- New test: `prefs::tests::tool_group_default_keys_are_unique_and_r_is_rotate_view` (no two tool
  groups share a default key; R = Rotate View, J = healing group, AI Remove has no own default;
  `edit.keyboardShortcuts` reports no default tool-key conflict).
- Needs from the UI owner (pc-ui-egui): see "Needs from other owners".

### 2b. clip35 SVG export has no `<clipPath>`: FIXED (exporter regression, pc-svg)
- Diagnosis: a real exporter regression, not a stale test. Make Clipping Mask creates an ordinary
  pass-through (Auto isolation) group with a vector mask. Since 77f58614 (later reshaped by
  f63fb309 / 9095d440) `Writer::scan_plain` flattens the whole document to one PNG whenever a
  pass-through group has opacity, a mask, a vector mask, a vector clip or effects, so the SVG had
  a single `<image>` and no `<clipPath>`. That is only needed when a descendant composites
  against the backdrop through the pass-through group (blend modes, adjustments, effects, Blend
  If, channel limits, text gamma). With plain Normal descendants, pass-through equals an isolated
  group with the same opacity/mask/clip (lerp(B, G over B, a) = B(1 - a*Ga) + a*G), so native
  output is exact.
- Fix: `crates/pc-svg/src/export.rs`: new `pass_through_matches_isolated(children)`; the
  pass-through flatten rule applies only when it is false. NOTE: pc-svg is not in this job's
  listed ownership (and not in the other builders' lists); the item asked to fix the right side,
  and this is the right side. Please review that file at integration.
- New test: `crates/pc-svg/tests/export/blends.rs`
  `pass_through_masked_group_of_normal_children_keeps_native_clip_path` (native clipPath, no
  image, matches the raster oracle; a Multiply child still takes the warned fallback).
  pc-svg `--test export` 41/41 and lib 16/16 pass.
- The pc-engine test also checks PDF (`W` clipping operator); pc-pdf has the same rule, so the
  test passes only once pc-pdf gets the mirror change (Needs from other owners). Verified: with the
  mirror change applied locally (then reverted) the clip35 test passes and pc-pdf `--test export`
  passes 36/36.

### 2c. develop_layer_cmds flaky test: FIXED (two time/backend dependencies in the test)
- Diagnosis (reproduced: ~1 in 9 runs of the `develop` filter with 8 threads): the test develops
  the photo once as `expected`, then opens it three more times and compared with `assert_eq!`:
  1. ICC bytes: `photocraft_io::raw::develop_with_settings` embeds a ProPhoto profile from
     `lightcraft_codecs::icc::write_named`, whose header creation time (bytes 24..36) is the
     clock. Renders either side of a second boundary differ only there; slow parallel runs cross
     one often. This was the observed failure (`... 0, 42, 0, 23` vs `0, 42, 0, 22`).
  2. Pixels: each Develop render may run on the GPU or fall back to the CPU (lc-gpu falls back per
     render on out-of-memory without latching, likely when the shared GPU is busy). GPU and CPU
     agree within lc-gpu's parity bound (3/255), not bit for bit; measured for this photo: max
     1/65535 over 82 samples.
- Fix (test-only, `crates/pc-engine/src/develop_layer_jobs_tests.rs`): `icc_without_date` masks
  the creation time; `max_pixel_diff(..) <= RENDER_PARITY` (3/255) replaces exact pixel equality,
  and the Convert case now also asserts a real conversion moves pixels by more than 4x that bound
  (measured 0.10), so the check still catches a converted/discarded photo. Same fix in the sibling
  `develop_open_job_matches_the_synchronous_document_and_link`, which had the same two exact
  comparisons. Verified with a forced 1.1 s gap between renders (temporary, removed): passes.
- TEST-CHANGED: exact ICC equality -> equality without the header date; exact pixel equality ->
  within the GPU/CPU parity bound, plus the new "conversion is detected" assertion.
- Product note (lc-codecs, not ours): `write_named` stamping the clock makes embedded profiles,
  and so exports, non-reproducible byte for byte. A fixed header date would make them stable.

## 3. photocraft-engine `--test it` `vectorize::v1_paths_and_vector_masks_survive_vectorize`: FIXED (stale test)
- Diagnosis: the failing line is `assert_eq!(photocraft_format::FORMAT_VERSION, 2)`; the format is
  v3 since f63fb309 (independent group isolation), which updated pc-format's own version
  asserts but missed this one. Nothing about paths, vector masks or vectorize regressed.
- TEST-CHANGED: `crates/pc-engine/tests/it/vectorize.rs` expects version 3 (comment added); every
  path / vector-mask / layer-count assertion is unchanged. `--test it vectorize`: 6/6 pass.

## 4. photocraft-gpu `parity::blur_lut_spans_multiple_rows`: dead path removed, test reworked
- Diagnosis: the only effect-map LUTs are blur kernels (`tent_kernel`) and contour LUTs.
  Since f82a55f0 every box width is clamped to `MAX_REACH` (512), so the widest tent is
  `tent_kernel(512)` = 1025 taps; contour LUTs (`curve_lut`, `ranged_lut`) are exactly 4096. No
  effect-map LUT can exceed one 4096-wide row, so the multi-row upload in `Kit::run`
  (crates/pc-gpu/src/lib.rs) was dead for effects. (The shader's `lut_at` row addressing stays: it
  is live for the CMYK Levels/Curves CMS tables, which are uploaded by the adjustment program path
  and covered by the existing CMYK tone parity tests.) The clamp is kept.
- Fix: `Kit::run` uploads the effect-map LUT as a single row (debug-asserted to fit); new
  `fx::FX_LUT_ROW`. New unit test `fx::tests::every_effect_map_lut_fits_one_row` (widest tent
  at MAX_REACH fits; sizes 2051 / 1e9 / MAX / inf / NaN never exceed it; contour LUTs are 4096;
  every stage LUT of drop / inner shadow programs with huge sizes and a custom contour fits).
- TEST-CHANGED: `parity::blur_lut_spans_multiple_rows` → `parity::oversized_blur_is_clamped_like_the_cpu`:
  same document and GPU/CPU parity check; the impossible `> 4096 taps` precondition is replaced by
  "size 2051 gives the MAX_REACH kernel and fits one row".
- Results (one at a time, `--exact`): `parity::oversized_blur_is_clamped_like_the_cpu` PASS,
  `parity::layer_effects_every_kind` PASS; lib `every_effect_map_lut_fits_one_row` PASS.

## 5. Cross-owner leftovers: all PORTED
- **5049a1d0 (GPU half)**: `crates/pc-gpu/src/plan.rs` Color Lookup guard and table slice use
  `photocraft_compose::adjust::lut3d_len` (checked n³ × 3). Test (upstream's, plus a size-2
  positive case): `plan::tests::color_lookup_with_an_overflowing_stored_size_uploads_no_table`: PASS.
- **980cf05b (GPU twin)**: `crates/pc-gpu/src/bounds.rs` `effect_region` grows
  `photocraft_compose::effect_source_bounds` (a group's children with their own effects) instead
  of `layer_bounds`. Upstream's parity case as its own test
  `parity::group_effects_follow_a_childs_stroke` (`--test gpu`): FAILED before the change (max diff
  42/255 at the child's stroke), PASS after; `parity::layer_effects_on_groups_clipping_and_fills`
  still PASS. (Our `effect_region` has no #761 "keep the whole region" sizing; not ported here,
  it is a separate upstream change.)
- **e1a874d9 (pc-doc half)**: `crates/pc-doc/src/video.rs` `valid_fps` (finite and > 0, else 30)
  in `Timeline::new` / `clamp`, and `Timeline::step` (saturating). pc-engine `timeline_cmds.rs`
  now calls `t.step(delta)` and its local copy `step_timeline` is removed (same code). Tests:
  `video::tests::step_saturates_and_non_finite_fps_is_30` (pc-doc), engine
  `timeline_cmds::tests::*` incl. `step_is_safe_on_any_stored_timeline`: PASS.
- **6e9ba651 (engine half)**: we have no `document_preset_cmds.rs`; `file.new` is in
  `crates/pc-engine/src/commands.rs`. New `MAX_NEW_DIMENSION` (300000) used by `file.new` plus the
  compile-time assert tying it to `photocraft_format::MAX_CANVAS_SIDE / MAX_CANVAS_PIXELS`.
  Upstream test `tests::the_largest_new_document_reopens_from_pcraft` (300000² transparent
  document round-trips through .pcraft): PASS.
- **fd21e734 (non-format half)**:
  - pc-doc `pattern.rs`: `MAX_EDGE` (30000), `MAX_PAINT_PIXELS` (2^26), `Pattern::pixel_count`,
    `Pattern::is_paintable`. Test `pattern::tests::paintable_patterns_are_non_empty_and_bounded`.
  - pc-compose `pattern.rs`: `PreparedPatterns::get` and `Tile::new` refuse unpaintable patterns
    (ours has no `pattern_rgba` / `Tile::new_in`). Test
    `pattern::tests::patterns_too_large_to_paint_are_skipped_without_allocating`.
  - pc-gpu: the three pattern lookups in `plan.rs` and the Bevel texture in `fx.rs` filter with
    `is_paintable` (GPU paints nothing exactly where the CPU does).
  - pc-engine: `brush.texturePattern` refuses (no `u32` overflow), `average_color` reads a bounded
    corner with u64-safe math, Fill / Paint Bucket messages say "empty or too large to paint". (Ours
    has no Pattern Stamp command.) Test
    `pattern_cmds::tests::a_pattern_too_large_to_paint_is_refused_not_allocated`: PASS.
  - Note: pc-format's loader keeps its local 30000 const (format half, up-formats); it could now
    use `photocraft_doc::pattern::MAX_EDGE` (pc-format owner, cosmetic).

## 6. SAM 3 (`lightcraft-engine --features sam`): COMPILES, tests pass; NOT enabled in the app
- Diagnosis: two private `Session::apply_outcome` methods in the same `impl` space: the AI
  Denoise / Remove one in `enhance/session.rs` and the SAM worker one in `segment/mod.rs` (only
  compiled with `sam`, so the clash went unnoticed).
- Fix: the SAM one is renamed `apply_segment_outcome` (`crates/lc-engine/src/segment/mod.rs`,
  definition and both callers). No behaviour change.
- Results: `cargo check -p lightcraft-engine --features sam --all-targets`: clean.
  `cargo test -p lightcraft-engine --features sam --lib -- segment`: 13/13 pass (incl. the
  sam-only `a_failing_download_ends_with_an_error_and_never_blocks`). `lightcraft-segment` lib:
  7/7 pass. Model-dependent tests (`tests/reference.rs`) need the weights and were not run.
- Is it safe to enable? Technically yes, with caveats; this is a product decision:
  - Build: on Linux the feature adds only pure-Rust crates (candle 0.9.2 pinned to avoid the
    Oniguruma C library, gemm, rustls-rustcrypto crypto for the downloader); no new `-sys`/`cc`
    crates (compared `cargo tree` with and without). Cargo.lock unchanged. On macOS candle uses
    Metal through `objc2` (bindings, not C libraries).
  - Model: `facebook/sam3` `model.safetensors`, 3.44 GB (3,439,938,512 bytes, SHA-256 pinned),
    plus two small tokenizer files (not yet pinned, TODO in `lc-segment/src/fetch/mod.rs`).
    Downloaded only when the user asks; never bundled.
  - Licence: the weights are under Meta's **SAM License** (custom, not OSI open source; the user
    must accept it; redistributors of the weights must ship it). The code is Apache-2.0 (port of
    HF transformers). The Hugging Face repo is gated, so there is **no default download
    location**: `DEFAULT_MIRRORS` is empty, so in-app download only works with
    `LIGHTCRAFT_SAM3_MIRRORS` or a mirrors file, or a hand-installed model folder. Enabling the
    feature without a CDN means users see "no download location is configured".
  - Runtime: per-photo encode ~3 s on an M4 Pro GPU (Metal) plus ~6 s first load; on Linux and
    Windows it runs on the **CPU** (candle has no wgpu backend), so expect a much slower first
    analysis per photo (tens of seconds is plausible; not measured here). Several GB of RAM
    while loaded (unloaded after 10 idle minutes). Everything runs on a worker thread; the UI
    stays responsive.
  - Needed before turning it on: a hosted mirror that follows the SAM License, pinned tokenizer
    hashes, the UI licence prompt (the engine already supports the `acknowledged` flow), and a
    CPU timing check on the target PCs.

## 7. xtask `assets::repository_assets_have_attribution_coverage`: FIXED
- Diagnosis: the audit lists `git ls-files --cached --others --exclude-standard`; `.gitignore`
  only ignores the root `/target/`, so UI-test screenshots written to
  `apps/local-image/target/library-workflow-ux/*.png` during a full run were "new assets".
- Fix (`xtask/src/assets.rs`): tracked files are always audited (this keeps the deliberately
  tracked and attributed `crates/pc-pdf/target/refvec/pdf21/*.png` references covered);
  untracked files inside a `target/` directory at any depth are skipped as build/test output;
  other untracked assets are still audited. New test
  `assets::tests::untracked_build_output_is_not_an_asset_source`.
- Verified: with 129 untracked SVGs under `apps/local-image/target/` the audit passes; the same
  files untracked outside `target/` still fail it (then removed).
- Related hazard (not ours, pc-pdf owner): pc-pdf's export tests write into the tracked
  `crates/pc-pdf/target/refvec/pdf21/` (they rewrote ~25 tracked PNG/PDFs and added ~200
  untracked files in this worktree when run; I restored them). Reference outputs that tests
  overwrite should not be tracked, or the tests should write elsewhere.

## 8. docs/PORTS.md: DONE
- New section "PhotoCraft and LightCraft upstream fixes (2026-10-13)" with 252 rows (one per
  upstream file of each fix commit and the file of ours that carries it; full commit hashes;
  `MIT OR Apache-2.0`, LightCraft `crates/fetch` Apache-2.0; date 2026-10-13). Covers every ported
  sha in the up-raw, up-formats, up-engine, up-library and up-linux reports, plus c20b81b2's
  store.rs half (0d3ca425) and this job's ports (5049a1d0, 980cf05b, e1a874d9, 6e9ba651,
  fd21e734 halves).
- How the rows were built: for each sha, the upstream files it changed were mapped to ours
  (`crates/<x>` -> `crates/pc-<x>` / `crates/lc-<x>`, `tests/<f>` -> `tests/{it,gpu,coverage}/<f>`,
  `apps/*` -> `apps/local-image`) and kept only where that batch's merge actually changed our file;
  then checked against each report by hand: removed the shared RENDER_CACHE_VERSION line
  (lc-engine media.rs) and pairs the reports say were not ported (61b6276c fill/image cmds,
  c88dbd26 retouch, 0a474cc import half, cdca2b9 lib.rs, 3952fcef ui lib.rs, 82c58fab cjk.rs,
  e2d35de main.rs); added the moved files (LightCraft `apps/lightcraft/src/dialog_filter.rs` ->
  `crates/pc-ui-egui/src/dialog_filter.rs`, 6e9ba651 `document_preset_cmds.rs` -> our
  `commands.rs` / `tests.rs`). Not listed: ALREADY-FIXED items with no code (8fd4aa4c, b0451df3)
  and 58f7e74 (nothing to port).
- `assets/attributions.json` regenerated with `cargo xtask attributions` (offline): the xtask
  tests `every_port_is_attributed` / `attributions_json_is_up_to_date` require every PORTS.md row
  there. Diff is additions only (the new port entries); the 758-crate list is unchanged.

## Coordinator follow-ups (after fu-ui)
- pixel-tool-keys.json (RotateView = R after Hand, AiRemove = null) and pixel-toolbar.json
  (RotateView after Hand in allTools and photographer): already done in item 2a, same content.
- `prefs::TOOL_SHORTCUTS`: the J row is now labelled "Healing and Remove Tools" (no locale
  catalog or test referenced the old label).
- PORTS.md: added rows for 64db46f5 (`crates/ui-egui/src/puppet_ui.rs`) and 3f7d8afc
  (`crates/algo/src/puppet.rs`, the `MAX_WARP_PIXELS` bound the UI preview uses) ->
  `crates/pc-ui-egui/src/puppet_ui.rs`; 6f1c4481 (rest) -> `crates/pc-text/src/tests.rs`,
  `crates/pc-text/tests/coverage/ds_psd_coverage.rs`, `crates/pc-io/tests/it/psd_type_bounds.rs`
  (the `crates/pc-text/src/psd.rs` row for 6f1c4481 already existed). attributions.json regenerated.

## Needs from other owners
- **pc-pdf** (`crates/pc-pdf/src/export.rs`, the PDF builder): mirror the pc-svg fix so a
  pass-through group with Normal-only descendants keeps native clipping. In the group arm of the
  flatten check (around line 674), append `&& !pass_through_matches_isolated(&g.children)` to the
  `g.isolation == Auto && l.blend == PassThrough && !closed_isolated_group(l) && (...)` term, and
  add the same `pass_through_matches_isolated` function as in `crates/pc-svg/src/export.rs`
  (exact diff saved at the scratchpad `fu-core`-adjacent file `pc-pdf-passthrough.diff`; also
  reproduced in this report's item 2b). Verified locally: with it,
  `photocraft-engine` `clip_compound::tests::dsfix_v060_clip35_*` passes and pc-pdf
  `--test export` passes 36/36. Until then that one pc-engine lib test still fails (PDF part only).
- **pc-pdf**: its export tests write into the tracked `crates/pc-pdf/target/refvec/pdf21/`
  (rewrite tracked references, add untracked ones). Move outputs elsewhere or stop tracking them.
- **pc-ui-egui**: nothing new from item 2a (fu-ui already did the UI side: Rotate View = R, AI
  Remove in the J healing slot). If `Tool::AiRemove.key()` is 'J', `tool_shortcut_id` makes its
  id `tools.select.SpotHealing`, which matches the engine table.
- **lc-codecs** (optional): `icc::write_named` stamps the clock into the ICC header, so the same
  render embeds different bytes a second later (non-reproducible exports).
- **pc-format** (cosmetic): convert.rs's local 30000 pattern-edge const could use
  `photocraft_doc::pattern::MAX_EDGE` now.
- **Product**: SAM 3 decision (item 6).

## Final results
- `cargo fmt -p` lightcraft-engine, photocraft-engine, -gpu, -doc, -compose, -svg, xtask: applied.
- `cargo clippy --offline -p lightcraft-engine -p photocraft-engine -p photocraft-gpu
  -p photocraft-doc -p photocraft-compose -p photocraft-svg -p xtask --all-targets -- -D warnings`:
  clean; also `-p lightcraft-engine --features sam`: clean.
- Lib suites: lightcraft-engine 496 passed / 0 failed (4 ignored); photocraft-doc 60/0;
  photocraft-compose 185/0; photocraft-gpu 34/0; photocraft-svg 16/0; xtask 23/0;
  photocraft-engine 1425 passed / 1 failed (27 ignored): the failure is
  `dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip`, now only
  at its PDF assertion (needs the pc-pdf change above). The other three base failures are fixed.
- Integration files edited: photocraft-engine `--test it` 47 passed / 0 failed (5 ignored);
  photocraft-svg `--test export` 41/0; photocraft-gpu `--test gpu` run one at a time with
  `--exact`: `oversized_blur_is_clamped_like_the_cpu`, `group_effects_follow_a_childs_stroke`,
  `layer_effects_every_kind`, `layer_effects_on_groups_clipping_and_fills`: all PASS.
- Sam: lightcraft-engine `--features sam --lib -- segment` 13/0; lightcraft-segment lib 7/0.
- No Cargo.toml / Cargo.lock changes.

## TEST-CHANGED (summary)
1. lc-engine `tests_layers::copy_paste_presets_and_auto_sync_carry_layers` (Auto Sync no longer carries layers).
2. lc-engine `tests_enhance::final_review_paste_sync_auto_sync_and_presets_recompute_ai_masks` (same).
3. pc-engine snapshots `pixel-toolbar.json`, `pixel-tool-keys.json` and the vector Photographer list in `toolsets::tests` (RotateView).
4. pc-engine `develop_layer_jobs_tests`: ICC compared without the header date; pixels within the GPU/CPU parity bound, plus a new "conversion is detected" assertion.
5. pc-engine `tests/it/vectorize.rs`: FORMAT_VERSION 3.
6. pc-gpu `parity::blur_lut_spans_multiple_rows` -> `oversized_blur_is_clamped_like_the_cpu`.

## Files outside the listed ownership
- `crates/pc-svg/src/export.rs`, `crates/pc-svg/tests/export/blends.rs` (item 2b; pc-svg had no owner listed).
- `assets/attributions.json` (generated from docs/PORTS.md by `cargo xtask attributions`; required by xtask tests).
