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
- **Threading policy batch (A):** cx-bgremove used `Executor::multithread(n)` (capped at 8). Switch to `Executor::RayonGlobal` with a sized global rayon pool per REVIEW-threading-policy-fable.md rules 1–2.
- **Qwen AI cutout:** the live ComfyUI probe on the RTX 5090 returned unusable alpha (0..3/0..4), so the client's usable-cutout check fails. This is an existing backend/workflow issue; investigate the Qwen cutout graph. The owner wants Qwen kept.
- **Swatches:** bundled libraries so far are Open Color, CSS names, Tailwind, web-safe and the generated PS-style groups. Optional: add GIMP / Inkscape / Krita / Tango / Material palettes after per-file licence checks.
- Thread policy, next batch (C, with the export/preview/import pipelines):
  - size JobPool / Renderer from the compute profile (`lc-preview/src/pool.rs` ~100, `lc-ui-egui/src/render.rs`)
  - size the import probe (`lc-engine/src/import.rs` ~405) and `lc-catalog/src/local.rs` ~253
  - show the profile line in `lc-ui-egui/src/menus.rs` ~1048
  - add a Utility-priority rayon pool for Utility jobs if measurements show it's needed
- UX polish batch:
  - Swatches panel: replace the stacked text buttons with Photoshop-style footer icons plus panel menu items
  - Develop layer bar: Background as a list row, mask thumbnails, and collapse the repeated "applies to the whole image" notes into one line
- **Large soft brushes are slow (real UX problem):** a 2500 px soft round brush on a 24 MP doc takes about 430 ms per UI frame (p95 626 ms); 1000 px takes about 79 ms. The cost is in live-tail reconstruction, original-surface restoration and compositing in `pc-engine/src/brush_cmds.rs` (cx-brush report §4). Needs a dedicated perf job: GPU dab compositing, or incremental tail and restore.
- Wayland pen: compatibility mode re-execs without WAYLAND_DISPLAY when a pen is detected. Verify it on the owner's KDE Wayland with a real tablet.
- **Exports aren't byte-reproducible:** the ICC profile header embeds the current time (moxcms encoder via `lc-codecs::icc::write_named`). Set a fixed or derived creation date so identical exports give identical files.
- **Preferences:** add "Concurrent exports/previews/imports: Auto / 1", mapping to `pipeline::Shape::serial()` and `ImportJob::copy_workers`. Serial helps card readers and spinning disks.
- **Match Look review check:** no cap on the number of saved looks (owner). Only per-file safety bounds on imported look files are allowed.
