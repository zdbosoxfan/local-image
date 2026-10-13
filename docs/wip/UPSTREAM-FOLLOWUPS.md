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
