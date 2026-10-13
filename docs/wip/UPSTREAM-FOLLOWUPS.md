# Upstream-port follow-ups and pre-existing failures (2026-10-13)

Collected while integrating the up-* batches into `claude/v2-continue`. Per-batch details are in
`docs/wip/UPSTREAM-PORT-<job>.md`.

## Tests already failing on v2 6024e4c7 (verified on the base, not caused by the ports)

- photocraft-io `--test it`: `psd_block_layout::odd_layer_blocks_carry_their_pad_inside_the_length`,
  `roundtrip::raster_layer_blocks_preserved` (odd-length Zzzz block padding), and
  `vector_fallback::bugfix_save_f03_plain_shape_separate_vector_mask_psd_psb_pixels_and_loss_notice`.
- photocraft-text `--test on_path`: `on_path_bidi_marks_ligatures_and_vertical_glyphs_stay_shaped`.
- photocraft-engine lib: `toolsets::tests::persona_pixel_snapshot_and_vector_photographer_contract` and
  `vector_toolbar_layout::tests::toolbar46_frozen_pixel_tool_keys_and_query_no_journal` (Rotate View / AI Remove "R"
  key after the Rotate View integration), and
  `vector_packages::clip_compound::tests::dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip`
  (no `<clipPath>` in SVG output).
- Plus Codex's three focused UI failures listed in `RESUME-LINUX-V2.md`.

## Cross-owner changes still to apply

- pc-gpu `src/plan.rs` (~1659): Color Lookup guard → `photocraft_compose::adjust::lut3d_len(*size).is_some_and(|len| table.len() >= len)`.
- pc-gpu `src/bounds.rs` `effect_region`: use `photocraft_compose::effect_source_bounds` + port 980cf05b's GPU parity test.
- pc-doc `video.rs`: non-finite fps → 30 in `Timeline::new` / `clamp` (e1a874d9).
- pc-ui-egui `puppet_ui.rs`: refuse a remesh with no triangles (64db46f5); bound the live preview by `MAX_WARP_PIXELS` (3f7d8afc).
- pc-ui-egui Save for Web: "Text is HTML" slice cells are now escaped unless `trustedSliceHtml: true`; decide whether the dialog offers it.
- pc-engine `document_preset_cmds.rs`: assert tying `file.new`'s 300000 limit to the new .pcraft loader defaults (6e9ba651).
- pc-doc / pc-compose / pc-gpu / pc-engine: non-format half of fd21e734 (`Pattern::is_paintable`, `MAX_PAINT_PIXELS`).
- pc-text: f32 `TySh.bounds` (rest of 6f1c4481), with its coverage tests.
- pc-compose `multichannel.rs` + pc-io `channel_map.rs`: DisplayInfo alpha import (rest of 6547ca0b), landing together.
- docs/PORTS.md: add rows for every ported sha (lists in the UPSTREAM-PORT reports).
- lightcraft-ui-egui lib: `headless::tests::full_screen_preview_and_info_overlay_by_keyboard` already fails on base (Right in full screen jumps to photo 21 instead of 14).
- `lightcraft-engine --features sam` does not compile on base (`apply_outcome` defined in both enhance/session.rs:78 and segment/mod.rs:593). The shipped app does not enable `sam`, so SAM 3 masks are currently off; fix or drop the feature.
- Optional (apps/local-image): 790a341's refusal to import a whole drive or home folder from launch arguments.
- **PRIORITY, live data loss (found by the Develop review):** with Auto Sync on and 2+ photos selected, any layer/mask edit replaces the entire `masks` array on every other selected photo. `json_delta` takes arrays whole: `lc-engine/src/lib.rs` `auto_sync_ops` ~610 and `json_delta` ~788. Fix: drop `masks` from the auto-sync delta (layers sync only through explicit Sync/Copy with a dialog), plus a regression test where photo B's own masks survive an Auto Sync edit on A.
- Full-workspace run artifact: `xtask assets::repository_assets_have_attribution_coverage` scans the filesystem and trips over UI-test screenshot output written to `apps/local-image/target/library-workflow-ux/*.png` (260 files) during the same run. It passes on a clean tree. Fix: have the asset walker skip `target/` directories (or use `git ls-files`), so test output can't fail the release gate.
- `photocraft-io heif_unsupported::heic_without_the_feature_says_it_is_not_in_this_build` fails whenever the workspace is tested with `local-image/heif`, which is how the release gate runs (feature unification turns on `photocraft-codecs/heif`). Gate the test on the feature being off, or test the message through a non-heif build.
- **Caused by the up-engine port:** `photocraft-gpu --test gpu parity::blur_lut_spans_multiple_rows` asserts `tent_kernel(2051.0)` yields >4096 taps. PhotoCraft f82a55f0 now clamps box widths to `MAX_REACH` (512), matching the already-clamped apron, so that precondition can't be reached. This is our own test (not upstream). Rework it to cover the multi-row LUT path another way, or confirm the path is dead and remove it with a TEST-CHANGED note. Don't widen the clamp.
- `photocraft-engine develop_layer_cmds::...internal_develop_profile_preserves_managed_pixels...` failed in the full parallel run but passes alone (on base and on the merge), so it's flaky under load. Investigate the ordering/shared state.
- 6547ca0b DisplayInfo alpha import is still pending. Both halves are ready in FIXUP-fu-psd.md: the pc-io `channel_map.rs` diff and the pc-compose `multichannel.rs` `inks` change. They must land together, along with upstream 78c61100 (13-byte DisplayInfo records for resource 1077; 14 bytes only for the old 1007).
- li-seg: `Segmenter::predict` / `predict_sky` take no cancel flag, so Cancel discards the result but the model run still finishes. Add `&AtomicBool`/Interrupt support. Do it in the threading-policy batch after cx-bgremove merges.
- i18n (next translation pass): "Selecting subject…", "Selecting object…", "Selecting sky…" and "Selecting…" in `jobs_ui::non_modal_text`.
