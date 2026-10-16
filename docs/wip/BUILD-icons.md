# BUILD: industry-standard monochrome icons (Claude, 2026-10-13)

Branch `wip/icons`. Brief: replace the hand-drawn Glossy Duotone tool icons with the upstream line set
(Lucide 0.468.0 ISC, PhotoCraft originals, VectorCraft `dc-*`, Tabler outline as fallback), monochrome by
default, draw what nothing covers in Lucide's style, and move Library/Develop off code-painted glyphs.
Source of truth: `docs/wip/RESEARCH-icons.md` plus the owner decision of 2026-10-13.

## Result

- **168 tool icons** in `assets/icons/tools/` (98 Compositing/Develop/Liquify ids, one new: `rotate-view`;
  70 vector ids). **144 sourced, 24 drawn.**
  - Lucide 0.468.0: 92 (unchanged bytes from the pinned tag `f12b0de1`, fetched from a blobless clone of
    lucide-icons/lucide; Path Selection adds `fill="currentColor"`, the Photoshop/Illustrator solid arrow).
  - VectorCraft `dc-*` (`d522c1d7`): 37 (Twirl CCW mirrors `dc-twirl` so the two directions differ).
  - PhotoCraft (`72b499a2`): 9 (gradient, layer-mask x2, perspective-crop, direct-select, eraser-background,
    eraser-magic, bandage-sparkle, gradient for Linear Mask).
  - Lucide via VectorCraft's lucide-static 1.49.0: 3 (pen-tool add/delete badges, `spline-pointer`, which is
    newer than 0.468.0).
  - Tabler 3.44.0 outline: 3 (Polygonal Lasso `lasso-polygon`, Guided Upright `perspective`, Spiral `spiral`).
  - Local Image originals (GPL-3.0-or-later, Lucide rules: 24 grid, 2 px, round caps/joins, no fills): 24 =
    the owner's 19 (Content-Aware Move, Quick Selection, Sponge, Freeze All, Freeze Lasso, Invert Freeze,
    Thaw, Thaw All, Reconstruct All, Skin and Defringe pickers, vector contour, donut, double star, envelope,
    point transform, segment, square star, trapezoid) plus 5: History Brush (owner override), Magnetic Lasso
    (owner: avoid PhotoCraft's CC BY 3.0 glyph), Spot Healing (must differ from Healing; the silhouette test
    requires it), Slice Select (was identical to Object Selection in the same toolbar), vector Corner (was
    identical to Anchor Point).
- **Owner overrides:** Sponge = drawn sponge block with pores; Burn = Lucide `grab` (cupped hand, renamed
  `hand-grab` after 0.468); Rotate View = VectorCraft `dc-rotate-view` (rotating canvas) under its own id,
  so it no longer shares Rotate's artwork; History Brush = drawn brush under an undo arc.
- **Other choices beyond the research table:** History panel (`activity`) uses Lucide `history`, not the
  pulse line; Sharpen uses Lucide `pyramid` (a cone) so it differs from the Triangle shape; Straighten uses
  Lucide `ruler` (0.468 has no level; `ruler-dimension-line` is newer); Spiral uses Tabler `spiral`
  (Lucide `tornado` read as lines at 16 px).
- **Colour/monochrome:** one artwork. Monochrome = theme `icon` colour (default in `color_icons::enabled`);
  colour = the same line icon pulled 70% toward a family hue (`color_icon_data.rs`: `TOOL_FAMILIES`,
  `family_hue`, lighter shade on dark themes, deeper on light). Active tool keeps the existing accent fill
  and border. `assets/icons/color/` (167 files) is deleted.
- **Library/Develop:** all 72 `lc icons.rs` `Icon` variants now draw Lucide SVGs (23 new Lucide 0.468.0
  files in `assets/icons/`, PhotoCraft `star-filled.svg`, and the shared tool icons), rasterized white at the
  physical pixel size, cached per context and tinted. Nothing is code-painted any more: Smart Album, the one
  glyph with no equivalent, is `assets/icons/smart-album.svg` (Lucide `book` + our funnel). The 2 px source
  stroke is drawn at 1.75, as the editor's generic icons already were.
- **Generator retired:** `xtask/src/tool_art.rs` and `tool_art_vector.rs` and the `redraw-tool-icons` /
  `redraw-vector-tool-icons` commands are deleted. Reason: they only produced Glossy Duotone material from
  frozen snapshots; with byte-for-byte upstream files and hand-authored originals there is nothing to
  generate, and an inert generator would rot. Replaced by `cargo xtask icons`, which verifies every tool
  icon against `assets/icons/source-manifest.json` (schema 2: set, source, licence, family, SHA-256),
  licence files, Lucide style rules (stricter for originals: 2 px, no fills, no gradients/transforms,
  licence comment), the two registries and the colour family table.
- **Removed licences/assets:** Krita sources (`assets/icons/upstream/`), `licenses/krita-icons-NOTICE.md`
  and `licenses/CC-BY-SA-4.0.txt` (only icons used them), PhotoCraft's CC BY 3.0
  `assets/icons/lasso-magnetic.svg` and its `LICENSE-noun-magnetic-lasso.txt`. Added
  `licenses/tabler-icons-MIT.txt`. Lucide ISC, Feather MIT and PhotoCraft/VectorCraft MIT/Apache texts were
  already in `licenses/`.

## Files

Owned: `assets/icons/**` (tools/, SOURCES.md, source-manifest.json, 25 new interface SVGs),
`assets/attributions-curated.json`, `assets/attributions.json` (regenerated with `cargo xtask attributions`),
`licenses/` (tabler licence added; Krita/CC-BY-SA removed; `photocraft-ATTRIBUTION.md` rows for
lasso-magnetic removed and smart-album added), `crates/pc-ui-egui/src/{icon_data,tool_icon_data,
color_icon_data,icons}.rs`, `crates/lc-ui-egui/src/icons.rs`, `xtask/src/{main,icons}.rs` (tool_art*.rs
deleted), `docs/DESIGN-SYSTEM.md` section 4, `docs/design/{tool-icons,tool-icons-before-after,
vector-tool-icons}.png`.

Touched outside the listed files, icon-only, because the registry shape changed or the text became false
(please review): `crates/pc-ui-egui/src/{color_icons,mono_tool_icons,tool_icon_sheet,tool_icon_tests}.rs`
(the icon renderers, sheet and tests behind the registries); `vector_packages/fill_stroke_proxy/icons.rs`
(the vector icon registry, now `(tool, name, svg)`); `vector_packages/fill_stroke_proxy.rs` (`tool_button`
only: 6 lines now call `crate::icons::tool_icon` instead of picking colour/mono bytes itself);
`vector_packages/fill_stroke_proxy/tests.rs` (the two icon tests and two ignored GPU tests' tuple patterns);
`assets/ATTRIBUTION-lightcraft.md` (the 140 per-file Glossy vector rows and the Glossy/Krita directory row
replaced by accurate rows; the audit passed either way, but the old rows mislabelled ISC/MIT files as
our GPL artwork); `docs/DESIGN-SYSTEM.md` section 1 (one bullet that described the Glossy set).
Not touched: Match Look files, `edit.rs`, menus, `widgets.rs`, `gpu_canvas.rs`, pc-gpu, lc `render.rs`.

## Tests and checks

- `cargo xtask icons`: 168 tool icons (144 sourced, 24 originals) agree. `cargo xtask assets`: 1900 assets
  attributed, licence files exist. `cargo xtask attributions`: regenerated.
- fmt: `cargo fmt -p photocraft-ui-egui -p lightcraft-ui-egui -p xtask` (only our files changed).
- clippy: `cargo clippy --offline -p photocraft-ui-egui -p lightcraft-ui-egui -p xtask --all-targets
  -- -D warnings`: clean.
- Lib suites: see "Final results" below.

### TEST-CHANGED

1. pc `tool_icon_tests::approved_tool_icons_are_restored_exactly_and_source_replacements_have_filled_faces`
   asserted the 88 Glossy glyphs byte-for-byte and filled family material. The owner retired that artwork.
   Replaced by `tool_icons_are_the_pinned_line_set_with_recorded_provenance` (every manifest icon is
   registered, 24 px currentColor line icon with round caps/joins, originals 2 px without fills and with a
   licence comment, exact counts 24 drawn / 144 sourced); xtask `vendored_tool_icons_match_the_manifest`
   checks every SHA-256.
2. pc `dodge_tool_icon_has_a_solid_paddle_head_in_both_styles` required a filled paddle; line icons have no
   fills. Now `dodge_tool_icon_has_the_lollipop_swirl_in_both_styles`: the swirl's lower turn is inked at
   16/20/40 px in both styles, and the same point is empty in Zoom (same intent: Dodge never reads as Zoom).
3. pc `every_tool_and_group_member_has_a_distinct_colour_icon_and_none_are_unused`: the
   `assets/icons/color` disk/embedding equality is now "every tool icon has exactly one colour family row"
   and "no `assets/icons/color` directory"; all other assertions unchanged.
4. pc `color_icons::all_svgs_parse_render_and_disable_at_toolbar_sizes` and
   `mono_tool_icons::every_tool_style_has_the_same_rendered_silhouette_at_actual_sizes`: the "not empty"
   floor goes from 1/12 to 1/20 of the box (the vector table's existing floor). Measured: Line (one
   diagonal) covers 5.5%, More/Edit Toolbar (three dots) 6.2%, Vertical Type 8.0%; the 1/12 floor was set
   for filled glyphs. The colour check now covers all 168 icons instead of 97.
5. pc GPU tests (`hardware_gpu_renders_compositing_and_develop_colour_icons_at_both_scales`,
   ignored `gpu_tool_icons_upload_and_switch_styles_on_hardware`) iterate `TOOL_FAMILIES` instead of the
   deleted `COLOR_ICONS`; assertions unchanged.
6. fill_stroke_proxy `toolbar46_registered_icon_alpha_and_styles_at_16_20_40` and
   `toolbar46_contact_sheet_original_artwork`: 3-tuple registry; "colour" is the family-tinted raster.
   Assertions unchanged. Two ignored GPU tests: tuple patterns only.
7. xtask: the generator tests went with the generator (`tool_art` and `tool_art_vector` modules). New:
   `vendored_tool_icons_match_the_manifest`, `style_rules_reject_fills_gradients_and_other_strokes_in_originals`.
8. New: `color_icons::family_tint_is_a_subtle_hue_on_the_theme_icon_colour`; lc
   `write_ui_icon_review_sheet` (ignored, writes `target/icons-ux/ui-icons-<tag>.ppm`).

## Visual review (looked at every page)

- `docs/design/tool-icons.png`, `docs/design/tool-icons-before-after.png` (old Glossy vs new mono and
  colour, 20/40 px), `docs/design/vector-tool-icons.png` (16/20/40, colour and mono, dark/light).
  Rendered by running the sheet test directly: `cargo xtask tool-icons` calls `cargo +1.98.1`, which the
  Claude build env (toolchain cargo first on PATH, no rustup proxy) rejects.
- `target/icons-ux/tools-before-after-mono.png`, `tools-before-after-colour.png` (all 168 tool icons, old
  vs new at 16/20/40 px, dark and light; 16 pages each in `target/icons-ux/pages/`) and
  `target/icons-ux/ui-icons-before-after.png` (all 72 Library/Develop icons, painted vs SVG; 6 pages).
- Every icon renders and none is blank (also enforced by the tests). All read at 20 px. Weakest at 16 px:
  VectorCraft's 1.5 px `dc-*` glyphs (Join, Scallop, Smooth, Knife, Path Text) are lighter than Lucide's
  2 px; Skin Tone's face badge and History Brush's brush are small at 16 px but clear at 20.

## Needs from other owners

1. **Monochrome default (owner decision)**: `crates/pc-engine/src/prefs.rs:284`
   `tool_icons: "colour".into()` -> `"monochrome".into()`; `crates/lc-ui-egui/src/state.rs:198`
   `color_tool_icons: true` -> `false`; labels `crates/pc-ui-egui/src/prefs_ui.rs:1076,1345` and
   `crates/lc-ui-egui/src/panels/settings.rs:579`: "Colour (default)" -> "Colour", "Monochrome" ->
   "Monochrome (default)". Then update the tests that click those labels or assert the old default:
   pc `tool_icon_tests::clicking_preferences_switches_the_real_toolbar_pixels_and_saves` (label strings)
   and lc `tool_icon_tests::settings_click_switches_tool_strip_pixels_and_round_trips_at_both_scales`
   (`assert!(old.color_tool_icons)` becomes `!`). `color_icons::enabled` already falls back to monochrome.
2. Cosmetic: `crates/pc-engine/src/toolsets.rs:277` RotateView icon metadata `"vector-rotate"` ->
   `"rotate-view"` (the UI uses its own registry; persona_tests only check the name exists).
3. Stale, not referenced by code any more: `docs/design/tool-icons-approved-mono.json`,
   `docs/design/tool-icons-before.json`, `docs/design/vector-tool-icons-recognition.tsv` (describe the
   retired artwork). `docs/design/tool-icons-pre-sourced.json` is still the "old" column of the
   before/after sheet. `crates/pc-ui-egui/src/vector_packages/fill_stroke_proxy/SOURCES.md` still mentions
   `tool_art.rs` as history.
4. `cargo xtask tool-icons` hardcodes `cargo +1.98.1`; fine on the owner's rustup setup, fails in the
   builder env. Left as is.
5. Match Look: it asked for no icon. If it needs one, add a Lucide 0.468.0 file to `assets/icons/` and an
   `Icon` variant + `svg()` row in `crates/lc-ui-egui/src/icons.rs` (this branch rewrote that file).

## Final results

- `cargo test --offline -p xtask`: 22 passed, 0 failed.
- `cargo test --offline -p lightcraft-ui-egui --lib`: 334 passed, 0 failed, 7 ignored.
- `cargo test --offline -p photocraft-ui-egui --lib`: 1599 passed, 0 failed, 60 ignored (includes the
  non-ignored hardware GPU colour-icon test and both tool-button click tests).
- clippy (`--all-targets -D warnings`) on photocraft-ui-egui, lightcraft-ui-egui, xtask: clean.
- `cargo xtask icons`: 168 (144 sourced, 24 originals) agree; `cargo xtask assets`: 1900 assets attributed.
