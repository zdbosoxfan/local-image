# Shared Color/Swatches — implementation and verification

Implementation in the assigned files is complete. The desktop drop-dispatch
hook requires the ownership exception described below; full suites are not green.
No commits made. The pre-existing document palette/ASE/global-colour code was
retained and extended rather than replaced. The task file's existing changes
were left alone.

## Implementation

1. Pixel and Vector use the same panel and HSB picker. Only target labels differ:
   Foreground/Background and Fill/Stroke. Pixel clicks set tool colours; Vector
   clicks apply the selected Fill/Stroke. The same explicit Apply to Selected
   Objects button links native shape/text paints in either mode. User and document palettes are available
   in either persona. Native document operations still use captured
   `swatch.document.edit` transactions and stable `SwatchBook` IDs. Text support
   was added to the existing live-colour transaction path, including character
   runs and cache/PSD refresh. A tooltip explains the painted-pixel limitation.
2. Collapsible folders, add-current-colour, rename/edit, delete, within/between
   folder drag/drop, list/grid view and case-insensitive name search. Explicit
   options, delete and move controls provide keyboard alternatives to dragging
   and context menus. User preferences wrap the existing `SwatchBook` alongside
   recent colours, list/grid choice and an initialization flag, so intentionally
   empty palettes remain empty on restart. Document storage remains unchanged.
3. Bundled MIT libraries: Open Color, CSS named colours (including SVG names),
   and Tailwind CSS v3.4.17. Upstream licences were fetched and read at the pinned
   commits recorded in `assets/attributions-curated.json`; original source
   snapshots and licence texts are in `assets/palettes/`. Generated libraries:
   216 web-safe colours; RGB, CMYK, Pastel, Light, Pure, Dark and Darker. No Adobe
   palette data or proprietary/trademarked books are bundled. Other suggested
   libraries are optional candidates, not copied without per-file verification.
4. ASE reuses the existing codec. New ACO v1/v2 (including v2-only and the usual
   concatenated v1/v2 form), GPL and KPL colorset.xml codecs are in pc-color.
   The UI supplies the existing pure-Rust pc-format ZIP reader/writer for KPL
   (STORE and DEFLATE input, CRC verification, bounded decompression). KPL groups,
   spot flags, Lab v1/v2 encoding and embedded profiles.xml/ICC files are handled.
   CMYK/Lab/Gray imports use the existing CMS and document/working profile.
   Unknown ACO colour models produce warnings; private vendor colour structures
   cannot be decoded from Adobe's public specification. ASE and KPL preserve spot
   flags; ACO/GPL explain their flag/folder losses. KPL exports portable sRGB
   colours, preserving names, folders and spots. GPL quantizes to 8-bit sRGB.
5. Narrow wrapping layouts, named accessible buttons/text fields, six-digit hex
   editing, explicit disabled explanations, stale-editor cancellation and
   import/export errors. Parser budgets: 16 MiB, 4096 entries, 1024 UTF-16 name
   units, bounded XML attributes/depth/tokens, no DTD/external entities, and
   aggregate KPL decompressed byte budget. No dependencies added; Rust only.

## Source references and interpretation

- [Adobe Photoshop File Formats — Color Swatches](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/):
  ACO sections, unsigned RGB/HSB and inverted CMYK, signed Lab components, names.
- [GIMP GPL/ACO reader](https://github.com/GNOME/gimp/blob/master/app/core/gimppalette-load.c):
  GPL headers and rows; ACO Gray ink inversion and wide CMYK fallback (Adobe's
  public Gray description does not specify the direction).
- [Selapa ASE description](https://www.selapa.net/swatches/colors/fileformats.php),
  plus the existing pinned VectorCraft reference in `crates/pc-color/src/ase/SOURCES.md`.
- [Krita KPL manual](https://docs.krita.org/en/general_concepts/file_formats/file_kpl.html),
  [KoColorSet saveKpl/loadKplGroup](https://github.com/KDE/krita/blob/master/libs/pigment/resources/KoColorSet.cpp),
  and Krita's RGB/CMYK/Lab/Gray `colorToXML` implementations under
  `plugins/color/lcms2engine/colorspaces/`: ZIP members, groups, spots, positions,
  profile inventory and Lab 1.0 correction. No C/C++ code is compiled or shipped.
- Bundled source licences:
  [Open Color](https://github.com/yeun/open-color/blob/3a716ee1f5ff5456db33cb8a6e964afdca1e7bc3/LICENSE),
  [CSS names](https://github.com/colorjs/color-name/blob/9ec3195524951f58bc1d33797fccc0f8ecfe070c/LICENSE),
  [Tailwind CSS](https://github.com/tailwindlabs/tailwindcss/blob/4f9f603e12b51cc53b8a09c7739b8f88c8eb87eb/LICENSE).
- egui 0.36 source/API and the established Brushes panel supplied the typed
  drag/drop, accessible widget labels and popup conventions. Screenshot renderer
  explicitly selects a discrete Vulkan RTX 5090; egui_kittest's default software
  preference is not used.

## Verification

Environment: sourced `~/.local/share/local-image-dev/claude-env.sh`; cargo
`+1.98.1 --offline`, unchanged `CARGO_BUILD_JOBS=4`, one build lane, same target.

- Final added parser tests: **5 passed**; native palette model **1 passed**;
  engine swatch/persistence regressions **3 passed**; egui interaction tests
  **10 passed**. All **19 added tests pass**.
- Strict all-target clippy passed for all four changed crates. After final
  picker/parser corrections the affected UI and colour lint checks passed again.
- Full suites ran **once per changed crate**, with `--no-fail-fast` so later
  targets still ran after failures:

  | Crate | Passed | Failed | Ignored |
  | --- | ---: | ---: | ---: |
  | photocraft-color | 30 | 0 | 0 |
  | photocraft-doc | 165 | 0 | 0 |
  | photocraft-engine | 2073 | 4 | 33 |
  | photocraft-ui-egui | 1611 | 19 | 65 |

  These are the actual full-run counts, not invented totals after focused fixes.
  Three UI failures from the new picker were then fixed: duplicate Saturation
  labels behind Camera Raw, and idle Vector Fill display changing the drawing
  default used by Draw Inside. Colour controls now have distinct labels; selected
  Fill/Stroke is shown through temporary picker values, without silently changing
  drawing defaults. The **10 Camera Raw cases**, the **actual RTX 5090 Draw Inside
  case**, and **all 10 swatch UI cases** subsequently passed. No test expectation
  was weakened. The remaining **4 engine / 16 UI failures** are outside the
  enumerated ownership; see the exact list below. This is not an all-green gate.
- Logs: `target/swatches-ux/logs/`, including all four full-suite logs, strict
  clippy logs, focused final logs and `full-suite-summary.json`. Failures retained.
- Attribution generator: `cargo +1.98.1 xtask attributions` passed; generated
  `assets/attributions.json` includes the new Swatch Libraries section.
- GPU test added: hardware-only panel rendering requiring discrete RTX 5090 and
  Vulkan. Actual adapter: NVIDIA GeForce RTX 5090, driver **615.71.09**, Vulkan.
- Final screenshots **all visually inspected**, including wrapping, grid rows,
  target chips, readable theme contrast, clipping and vertical scrolling:
  `target/swatches-ux/dark-250.png`, `light-250.png`, `dark-520.png`,
  `light-520.png`. Narrow view reserves room for footer instructions and scrolls
  its remaining rows; normal width shows the complete starter group.
- Real panel click path + preference write + warm GPU render/readback measured
  **3.651 ms** on that adapter (`target/swatches-ux/runtime.txt`). This is one
  headless UI diagnostic including readback, not a desktop frame-rate/compositor
  benchmark. No ComfyUI jobs or GUI windows were opened; no app was stopped.

## Remaining suite failures outside swatch ownership

The first three engine failures are already recorded on the base in
`docs/wip/UPSTREAM-FOLLOWUPS.md`; the vectorize failure also uses unchanged
production/test files. The Rotate View, Gallery numeric representation and
New Document Close-query failures are recorded in `docs/wip/RESUME-LINUX-V2.md`.
Other listed failures were reproduced in untouched areas; they were not silently
classified as verified baseline passes, fixed by changing their expectations, or
repeatedly rerun. The task explicitly assigns other test fixes to another builder.


Engine:

- `toolsets::tests::persona_pixel_snapshot_and_vector_photographer_contract`
- `vector_packages::clip_compound::tests::dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip`
- `vector_toolbar_layout::tests::toolbar46_frozen_pixel_tool_keys_and_query_no_journal`
- `vectorize::v1_paths_and_vector_masks_survive_vectorize`

UI:

- `brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture`
- `brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library`
- `canvas::rotate_view::tests::rotate_view_extra_window_controls_and_text_focus_are_local`
- `constructive::tests::constructive_ui_contour_numeric_type_enter_and_escape`
- `context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work`
- `control::tests::ui_set_rejects_unknown_fields_before_changing_anything`
- `fill_stroke_proxy::tests::toolbar46_frozen_pixel_menu_keys_and_all_chords_parse`
- `fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40`
- `gallery_ui::tests::remembered_gallery_hex_glow_color_survives_success_restart_and_paint`
- `generate_ui::draft_refine::tests::draft_refine_shared_aspect_presets_lock_and_model_grid`
- `new_doc_ui::remembered_tests::remembered_new_recent_uses_real_create_close_and_recall_buttons`
- `tool_tips::tests::every_tool_has_a_complete_entry`
- `toolsets_ui::persona::tests::persona_clicks_and_shortcuts_keep_document_dock_selection_and_history`
- `pdf_export_app_gpu::pdf_export_app_real_gpu_preview_release_and_menu_dialog`
- `psd_advanced_fallback::vector_fallback_app_real_gpu_native_reopen_preview_release_warning_click`
- `psd_advanced_fallback::vector_fallback_native_width_brush_blend_repeat_envelope_full_app_gpu_shared_projection`

## Integration boundary still under review

The only remaining feature integration is one line in the shell. The shell routes dropped files in `lib.rs` before drawing any panel, through
`file_open.rs`, and currently tries to open swatch files as images. The owned
panel can read/import palette drops, but cleanly preventing the competing
image-open path requires a small dispatch hook in one of those files, outside
this task's enumerated ownership. The exact one-line patch is prepared in
`target/swatches-ux/desktop-drop-hook.patch`. The dispatcher is exercised by
an egui_kittest pre-panel drop harness, including same-frame duplicate
prevention and leaving image drops/positions outside the panel untouched.
This will be resolved or reported explicitly;
no outside-ownership edit has been made.

KPL models outside RGB/CMYK/Lab/Gray and out-of-range HDR colours are rejected
explicitly to fit the existing bounded swatch model. External proprietary/Adobe
or Krita application-generated corpora were not provided; codec validation uses
independent synthetic files plus the published/reference schemas.

## UX polish (Claude, 2026-10-13)

The panel is now laid out like Photoshop's: the swatch grid/list is the main content with search above
it; there is a compact User | Document segmented control; the ☰ panel menu holds views, Swatch Libraries ▸,
Import/Export, New Swatch/Group, Swatch Options…, Folder Options…, Move to… ▸, Apply to Selected Objects
and Delete Swatch; a footer has New Group / New Swatch / Delete Selected Swatch icons (disabled with
reasons); swatches and folders have right-click menus; the hint lines moved into tooltips. Every earlier
command is still reachable, and every one can be reached by keyboard through the ☰ menu and the focusable
footer icons.

Full details, tests, TEST-CHANGED notes and screenshots are in `docs/wip/BUILD-ux-polish.md`.
