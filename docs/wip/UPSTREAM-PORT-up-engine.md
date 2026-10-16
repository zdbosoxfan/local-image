# Upstream port: up-engine (Claude)

Upstream: PhotoCraft (storytold/photocraft, MIT OR Apache-2.0), commits after e5e3e397.
Ownership: pc-compose, pc-engine, pc-paint, pc-raster, pc-algo, pc-color.
All ports are by hand onto our code; upstream attribution kept in comments ("Upstream <sha>").

## MUST

### 1. af3c99d8: Clone Stamp / Healing Brush refuse a far source (#1111): PORTED
- `crates/pc-engine/src/retouch_cmds.rs`: `clone_sample` is now fallible; a translation that
  saturates `Rect::translate` (window narrower than the stroke rect) returns a BadParams error
  instead of indexing past the sampled buffer. All three callers (Clone Stamp, our Seamless clone
  branch, Healing Brush) propagate it. pc-paint needed no change (upstream changed only engine).
- Test: `retouch_cmds::tests::clone_and_heal_with_a_far_source_are_plain_errors`: PASS.

### 2. 5049a1d0: never crash on hostile numbers (LUT size, Quick Selection, link ids): PORTED (CPU/engine/algo parts)
- `crates/pc-compose/src/adjust.rs`: Color Lookup guard uses a checked `lut3d_len(size)` (new
  `pub fn` in pc-compose; upstream put it in `photocraft_doc::adjust`, pc-doc is not ours).
- `crates/pc-algo/src/segment/quick.rs`: `saturating_add(1)` on the bbox edges.
- `crates/pc-algo/src/vanishing.rs`: same hardening on the clone footprint.
- `crates/pc-engine/src/layer_multi_cmds.rs`: Link Layers picks a fresh group id that never
  overflows / reuses an existing group; first group read with `first()`.
  (`duplicated_for_move` already used `checked_add`; unchanged.)
- Tests: `adjust::tone_tests::color_lookup_with_an_overflowing_stored_size_is_the_identity`,
  `adjust::tone_tests::lut3d_len_rejects_tiny_and_overflowing_sizes`,
  `segment::tests::quick_select_with_points_far_off_canvas_selects_nothing`,
  `layer_multi_cmds::tests::link_layers_never_reuses_a_group_when_the_largest_id_is_u64_max`: all PASS.
- NEEDS-OTHER-OWNER: GPU planner (pc-gpu `plan.rs`) and PSD writer (pc-io `adjust_map.rs`) parts.

### 3. f82a55f0: oversized Satin / Bevel sizes render instead of panicking (#1543): PORTED
- `crates/pc-compose/src/effects.rs`: new `box_width` (clamp to 1..=MAX_REACH, NaN → 1) used by
  `box_weights` (so `tent_kernel`, which the GPU also uses) and `box_geom` (CPU tent).
- Tests: `effects::tests::tent_kernel_is_two_boxes` (extended with huge/inf/NaN widths),
  `layer_style::tests::oversized_satin_and_bevel_sizes_render` (pc-engine): PASS.

### 4. 2842973d: compose: no overflow / unbounded allocation on hostile coordinates: PORTED
- `adjust.rs` `lut3d_sample` guard; `effects.rs` `Map::shifted` clamps distances; `lib.rs`
  `MAX_RENDER_PIXELS` (2^30) cap + i64 tile comparison in `render_tiled_with`; `masks.rs`
  saturating vector-mask area; `pattern.rs` saturating tile sample; `proxy.rs` saturating
  downsample + bounds-checked row copy.
- Tests: `lut3d_guard_tests::degenerate_or_short_tables_leave_the_colour_alone`,
  `effects::tests::shifting_by_hostile_distances_does_not_overflow`,
  `masks::tests::hostile_path_coordinates_do_not_overflow`,
  `pattern::tests::sampling_at_hostile_phases_does_not_overflow`,
  `tests::render_refuses_rects_beyond_the_pixel_cap`: PASS.

### 5. 61b6276c (export path-safety parts only): PORTED
- `crates/pc-engine/src/web_cmds.rs`:
  - Generate Image Assets: `asset_path_components` rejects `..`, `.`, empty, absolute, drive/UNC,
    `:`/NUL, trailing dot/space and Windows device names in layer-name and `default` folder paths
    (both `/` and `\` split on every OS); `asset_output_path` confines writes below the output dir,
    refuses symlinked folders/files below it (a symlinked chosen root is allowed). Rejected before
    rendering.
  - Save for Web: `imagesFolder` validated as a relative folder; every image/spacer/HTML write goes
    through the confined `web_output_path`; reserved device names get a `_` suffix; spacer `src`
    HTML-escaped; slice URLs limited to http(s)/ftp(s)/mailto/tel/relative (`javascript:`,
    `data:` etc. are dropped); "Text is HTML" cell text is escaped unless the call passes
    `trustedSliceHtml: true`, which is never remembered in `last_web`. Command spec updated.
  - Not ported (outside scope): print spool/tempfile (needs a new dependency), automation/MCP
    bounds, format store, droplet launchers, the browser-writer variant (ours has no writer API).
- Tests (pc-engine `web_cmds::tests`): `reserved_document_and_slice_names_export_with_portable_names`,
  `slice_html_escapes_imported_markup_by_default_and_requires_trusted_opt_in`,
  `slice_links_allow_normal_urls_and_drop_active_schemes`,
  `images_folder_stays_inside_export_directory_and_is_escaped_in_html`,
  `images_folder_rejects_existing_symlink_outside_export_directory`,
  `image_assets_reject_traversal_from_names_and_defaults`,
  `image_assets_reject_existing_symlinks_in_folders_and_files`: PASS (existing web tests still pass).
- UX note for the UI owner: Slice Options "Text is HTML" cells are now escaped in Save for Web
  unless the UI passes `trustedSliceHtml: true` (upstream also leaves it unwired).

### 6. 81f97450: Rasterize Smart Object keeps the pixels: PORTED
- `crates/pc-engine/src/extra_cmds.rs` `rasterize_one`: a "smart" branch re-renders the smart
  object when its cache is empty, takes the rendered cache (converted to the document format),
  clears `fill_cache` and drops the SoLd/SoLE/PlLd PSD blocks. Fill Content keeps the old path.
- Test: `extra_cmds::tests::rasterize_smart_object_layer` (upstream's cases plus one with the cache
  explicitly dropped): PASS.

## SHOULD

### 7. f1433f4a: restore the Lab-mix flag after a tile (#1112): PORTED
- `crates/pc-compose/src/psblend.rs`: `with_lab_mix` sets `LAB_MIX`, runs, restores the previous
  value with a drop guard. `crates/pc-compose/src/lib.rs`: the three set/clear pairs in
  `render_tiled_with` use it. (pc-gpu `fx.rs` already restores the flag itself: no GPU twin.)
- Test: `tests::rendering_restores_the_lab_mix_flag_it_found`: PASS.

### 8. 3cc0d484: Flatten All Layer Effects keeps layer and vector masks (#3804): PORTED
- `crates/pc-engine/src/file_cmds.rs`: `bake(.., apply_mask)`; Flatten All Layer Effects bakes
  without the masks and keeps `mask` / `vector_mask` live; Flatten All Masks unchanged.
- Test: `file_cmds::tests::flatten_all_layer_effects_keeps_masks` (8/16/32-bit, undo/redo,
  .pcraft round trip): PASS.
- Caveat (same as upstream): effects whose shape depends on the mask (stroke/shadow/glow at a
  masked edge) are now baked from the unmasked pixels and then clipped by the kept mask, so they
  can differ slightly from before at mask edges. Colour/gradient/pattern overlays are exact.

### 9. fb36a3fd: never overwrite preset/actions files that failed to load (#3346): PORTED
- `crates/pc-engine/src/preset_store.rs`: group files skipped on load (unreadable, newer version,
  missing tips) are remembered and never rewritten (warning instead); `actions.json` that exists
  but fails to load is never rewritten (warning instead).
- Tests: `preset_store::tests::a_group_file_that_failed_to_load_is_not_overwritten`,
  `preset_store::tests::an_actions_file_that_failed_to_load_is_not_overwritten`: PASS.

### 10. Engine parameter bounds
- 278e2941 channel.merge repeated source closed an unrelated document (data loss): PORTED.
  `channel_cmds.rs` rejects a repeated index before anything closes.
  Test `channel_cmds::tests::merge_rejects_a_repeated_source_and_keeps_unrelated_documents`: PASS.
- 2425431c layer.delete keeps the last layer: PORTED (`commands.rs`). Upstream's integration
  test file moved into a unit module `commands::layer_delete_last_tests` (4 tests): PASS.
- 0954b8c0 Link Layers never overflows / joins an unrelated group: PORTED
  (`layer_multi_cmds.rs`; `unused_link_group` kept local since pc-doc isn't ours; also fixes the
  `counts[&g]` index). Tests: `link_layers_beside_a_maximal_stored_group`,
  `unlink_a_maximal_stored_group`, `link_layers_when_every_group_is_huge`,
  `link_layers_near_the_maximum`, `link_layers_assigns_working_groups`,
  `unused_link_group_never_wraps`: PASS.
- e1a874d9 Timeline duration/fps bounds, stepping never panics: PORTED (`timeline_cmds.rs`, local
  saturating `step_timeline`). Tests `create_rejects_out_of_range_params`,
  `set_props_rejects_out_of_range_params`, `step_is_safe_on_any_stored_timeline`: PASS.
  NEEDS-OTHER-OWNER: pc-doc `Timeline::clamp/new` should also treat a non-finite fps as 30.
- 0c2c268c crop i32 origin/edge overflow: PORTED (`image_cmds.rs`: `translate_doc` checks fit,
  `crop_doc` uses `checked_neg`, explicit crop edge `checked_add`). Test
  `image_cmds::tests::crop_rejects_origins_that_cannot_move_to_zero`: PASS.
- fd74419a explicit crop past 300000 px per side refused: PORTED (`image_cmds.rs`). Test
  `image_cmds::tests::crop_refuses_a_canvas_side_past_the_new_document_limit`: PASS.
- 3ac39e1e Content-Aware fill margin overflow / unbounded window: PORTED (`fill_cmds.rs`
  `sampling_margin` + `window_size`, used by `edit_menu_cmds.rs` and
  `retouch_cmds/content_aware_move.rs`). Tests `fill_cmds::tests::content_aware_huge_extent_is_an_error`,
  `edit_menu_cmds::tests::content_aware_fill_huge_window_is_an_error`: PASS.
- 64db46f5 Puppet Warp empty mesh: PORTED algo part (`pc-algo/src/puppet.rs`). Tests
  `puppet::tests::an_expansion_that_empties_the_mesh_warps_nothing`,
  `distort_cmds::tests::puppet_with_an_expansion_that_empties_the_mesh_keeps_the_layer`: PASS.
  NEEDS-OTHER-OWNER: the pc-ui-egui `puppet_ui.rs` part (refuse a remesh with no triangles, restore
  previous density/expansion).
- 3f7d8afc Puppet Warp pins too far: PORTED (`puppet_warp` returns `Option`, `MAX_WARP_PIXELS`;
  `distort_cmds.rs` `LayerFn` returns `Result`, `cage_cmds.rs` caller wrapped in `Ok`). Tests
  `puppet::tests::a_pin_dragged_absurdly_far_is_refused_before_rendering`,
  `distort_cmds::tests::puppet_refuses_pins_that_stretch_the_content_too_far`: PASS.
  Note for the UI owner: the live Puppet Warp preview (if it renders the mesh directly) is not
  bounded by this.
- c88dbd26 live strokes reject bad coordinates before the preview changes: PORTED for
  `LiveStroke` (`brush_cmds.rs` shared `check_coords`). Ours has no `LiveRetouch` / `LiveDab`, so
  those parts don't apply. Tests `brush_cmds::tests::stroke_coordinate_validation_keeps_the_existing_bounds`,
  `brush_cmds::tests::live_brush_and_pencil_reject_invalid_batches_without_changing_the_preview`: PASS.
- 7fec6e94 Brush Scatter above 1000 %: PORTED (`pc-paint` `MAX_SCATTER`, `bounded_for_render`
  caps scatter, shared saturating `rect_around` for `retouch::dab_rect`; ours already guarded
  `BrushContext::dab_rect` against non-finite centres. `pc-engine` `validate_brush` replaces
  `validate_brush_size`, also checked between setBrush's two merges). Tests
  `brush_tests::huge_scatter_is_capped_and_renders_at_every_depth`,
  `brush_tests::ordinary_scatter_is_unchanged_by_the_cap`,
  `brush_tests::dab_rect_of_a_far_or_non_finite_centre_does_not_overflow`,
  `brush_cmds::tests::out_of_range_scatter_is_rejected_and_the_maximum_still_paints`: PASS.

### 11. 87486746: file.new / fill_rect at huge sizes shares one tile (#705): PORTED
- `crates/pc-raster/src/lib.rs` `Surface::fill_rect`: fully covered tiles share one `Arc<Tile>`;
  edge tiles written as before; revision and cached content bounds invalidated.
- Tests: `tests::fill_rect_shares_covered_tiles_and_edits_stay_local` (pc-raster, plus a
  revision/bounds check), `tests::file_new_at_the_size_limit_does_not_fill_every_tile`
  (pc-engine, 300000² in ~2 s): PASS.

### 12. Effect / mask cache keys
- f586e713 vector-mask cache keyed by content, not address: PORTED (hardening; latent in ours).
  `crates/pc-compose/src/lib.rs` `RenderVectorMasks` keeps (mask clone, slot) pairs.
  Test `tests::vector_mask_cache_is_keyed_by_content_not_address` (loop-local clones reuse one
  stack address): PASS.
- 57681126 combined-mask cache invalidation after pixel edits: PORTED (needed now that item 11
  replaces tiles wholesale). `crates/pc-compose/src/masks.rs` cache entries pin the input mask
  surface so its tile addresses can't be reused. Test
  `masks::tests::pixel_edits_refresh_cached_masks_at_every_depth`: PASS.
- 980cf05b group effects follow children's effects: PORTED CPU side.
  `crates/pc-compose/src/lib.rs` new `pub fn effect_source_bounds` used by `effect_maps` region
  and `empty_in`. Test `tests::group_effects_reach_past_children_effects`: PASS.
  NEEDS-OTHER-OWNER (GPU twin): `pc-gpu/src/bounds.rs` `effect_region` should use
  `photocraft_compose::effect_source_bounds(layer, canvas)` instead of `layer_bounds` (upstream
  980cf05b), plus its parity test.
- b0451df3 effect cache follows TSL / Blend If: ALREADY-FIXED for Blend If (our
  `layer_identity` already hashes `blend_if`); the `layer.advanced` part doesn't apply (we have no
  Advanced Blending yet; port it with Advanced Blending).

### 13. 3953d783: selection distance transforms correct past 4K: PORTED
- New `crates/pc-algo/src/selection/distance.rs` (+ `distance/tests.rs`, upstream's tests incl.
  brute-force oracle, old-transform equivalence on rect/partial bands, >4K zero distance,
  65537-px rows, worker-count independence); `selection.rs` re-exports `distance::edt` and drops
  the old f32 transform. Output identical to the old transform on the test patterns upstream
  says are identical; corrected where the old one was wrong. Benchmark test stays `#[ignore]`.
- Tests: `selection::distance::tests::*`: PASS.

## Needs from other owners

- pc-gpu `src/plan.rs:1659` (5049a1d0): the GPU Color Lookup guard still computes
  `(*size as usize).pow(3) * 3`, which overflows for a hostile stored size. Replace the guard with
  `photocraft_compose::adjust::lut3d_len(*size).is_some_and(|len| table.len() >= len)` (new
  `pub fn` in pc-compose; upstream has it as `photocraft_doc::adjust::lut3d_len`).
- pc-gpu `src/bounds.rs` `effect_region` (980cf05b): use
  `photocraft_compose::effect_source_bounds(layer, canvas)` instead of `layer_bounds(layer, canvas)`
  so the canvas matches the CPU for a styled group whose children have outside strokes; port
  upstream's parity test.
- pc-doc `src/video.rs` (e1a874d9): `Timeline::new` / `Timeline::clamp` should use a
  `valid_fps` that also rejects non-finite values (`fps.is_finite() && fps > 0.0`, else 30), and
  optionally host `Timeline::step` (the engine has a local equivalent).
- pc-ui-egui `puppet_ui.rs` (64db46f5): refuse a remesh (density / expansion change) that leaves
  no triangles and restore the previous density, expansion and mesh; the live preview is also
  not bounded by `MAX_WARP_PIXELS` (3f7d8afc) if it renders the mesh directly.
- pc-ui-egui (61b6276c): Save for Web now escapes Slice Options "Text is HTML" cells unless the
  call passes `trustedSliceHtml: true` (never remembered). If we want the dialog to offer it, add an
  explicit "trusted document" checkbox; upstream leaves it unwired.
- pc-io adjust_map writer: no change needed (ours already bounds the LUT size to 2..=256).

## Not ported / out of scope

- 61b6276c non-export parts (print spool `tempfile` dependency, automation/MCP input bounds,
  pc-format store, droplet launchers) and the "assets fire on every save" behaviour: outside this
  job's ownership/scope; the tempfile part would need a new dependency.

## Final results

- `cargo fmt` on photocraft-raster, -algo, -paint, -compose, -engine: clean (no changes outside
  the ported code).
- `cargo clippy --offline -p photocraft-raster -p photocraft-algo -p photocraft-paint
  -p photocraft-compose -p photocraft-engine --all-targets -- -D warnings`: clean (exit 0).
- `cargo test --offline --lib`:
  - photocraft-raster: 21 passed, 0 failed.
  - photocraft-algo: 297 passed, 0 failed, 4 ignored.
  - photocraft-paint: 78 passed, 0 failed, 1 ignored.
  - photocraft-compose: 184 passed, 0 failed.
  - photocraft-engine: 1420 passed, 3 failed, 27 ignored. The 3 failures are PRE-EXISTING on the
    unmodified branch (verified by re-running them with all changes set aside):
    `toolsets::tests::persona_pixel_snapshot_and_vector_photographer_contract` and
    `vector_toolbar_layout::tests::toolbar46_frozen_pixel_tool_keys_and_query_no_journal`
    (RotateView tool / AiRemove "R" shortcut snapshot drift),
    `vector_packages::clip_compound::tests::dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip`
    (SVG export has no `<clipPath>`). Not touched by this job.
- No integration test files were edited (upstream's `tests/layer_delete_last.rs` became a unit
  module in `commands.rs`).
- No Cargo.toml / Cargo.lock changes; no new dependencies.

## TEST-CHANGED

- `crates/pc-algo/src/puppet.rs` existing tests: `puppet_warp(..)` now returns `Option`, so the
  existing calls gained `.unwrap()` (same assertions, upstream 3f7d8afc does the same).
- `crates/pc-compose/src/effects.rs` `tent_kernel_is_two_boxes`: extended with the huge/inf/NaN
  width cases (no existing assertion changed).
- No existing assertion was weakened, removed or ignored.

## Ported upstream shas → our files

| sha | our files |
|---|---|
| af3c99d8 | pc-engine/src/retouch_cmds.rs, retouch_cmds/tests.rs |
| 5049a1d0 | pc-compose/src/adjust.rs; pc-algo/src/segment/quick.rs, segment/tests.rs, vanishing.rs; pc-engine/src/layer_multi_cmds.rs |
| f82a55f0 | pc-compose/src/effects.rs; pc-engine/src/layer_style.rs |
| 2842973d | pc-compose/src/{adjust,effects,lib,masks,pattern,proxy,tests}.rs |
| 61b6276c (export path safety only) | pc-engine/src/web_cmds.rs, web_cmds/tests.rs |
| 81f97450 | pc-engine/src/extra_cmds.rs |
| f1433f4a | pc-compose/src/psblend.rs, lib.rs, tests.rs |
| 3cc0d484 | pc-engine/src/file_cmds.rs, file_cmds/tests.rs |
| fb36a3fd | pc-engine/src/preset_store.rs, preset_store/tests.rs |
| 278e2941 | pc-engine/src/channel_cmds.rs, channel_cmds/tests.rs |
| 2425431c | pc-engine/src/commands.rs |
| 0954b8c0 | pc-engine/src/layer_multi_cmds.rs |
| e1a874d9 | pc-engine/src/timeline_cmds.rs |
| 0c2c268c, fd74419a | pc-engine/src/image_cmds.rs |
| 3ac39e1e | pc-engine/src/fill_cmds.rs, edit_menu_cmds.rs, edit_menu_cmds/tests.rs, retouch_cmds/content_aware_move.rs |
| 64db46f5, 3f7d8afc | pc-algo/src/puppet.rs; pc-engine/src/distort_cmds.rs, distort_cmds/tests.rs, cage_cmds.rs |
| c88dbd26 | pc-engine/src/brush_cmds.rs, brush_cmds/tests.rs |
| 7fec6e94 | pc-paint/src/{brush,lib,render,retouch,brush_tests}.rs; pc-engine/src/brush_cmds.rs, brush_cmds/tests.rs, retouch_cmds.rs, vector_cmds.rs |
| 87486746 | pc-raster/src/lib.rs; pc-engine/src/tests.rs |
| f586e713 | pc-compose/src/lib.rs, tests.rs |
| 57681126 | pc-compose/src/masks.rs |
| 980cf05b (CPU side) | pc-compose/src/lib.rs, tests.rs |
| 3953d783 | pc-algo/src/selection.rs, selection/distance.rs, selection/distance/tests.rs |
| b0451df3 | ALREADY-FIXED (blend_if) / N/A (advanced) — no change |
