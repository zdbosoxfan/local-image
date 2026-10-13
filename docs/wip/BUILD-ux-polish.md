# BUILD: ux-polish (Claude), 2026-10-13

Job: make the editor Swatches panel feel like Photoshop's and the Develop layer bar feel like Capture One's
Layers tool, without dropping any capability. Worktree `.claude/worktrees/ux-polish`, branch `wip/ux-polish`.
Details per panel: `docs/wip/CODEX-REPORT-swatches.md` and `docs/wip/CODEX-REPORT-develop-a.md` (the "UX polish" sections).

## What I looked at first

- Before screenshots, rendered with the existing harnesses (egui_kittest + RTX 5090 for the editor; the
  Library/Develop `Headless` CPU painter): `target/ux-polish/before-*.png` (8 Swatches, 24 layer-bar shots).
- Swatches before: the 7 stacked text buttons, the "Swatches menu" button, and 2 hint lines took more height
  than the swatches. Layer bar before: Background was a button beside ▾ and +, rows had centred names, there
  was a second "+" under the selected row, and the five whole-image sections repeated a 3-line notice.
  At 250 pt the "Radial Gradient 1" row's ⋮ was clipped by the panel edge.
- Reference layouts (web):
  - Photoshop Swatches: a folder icon (new group), + (new swatch from the foreground colour) and a bin at
    the bottom; commands, views, import/export and legacy libraries live in the panel's flyout (☰) menu.
    [GoSkills: The Swatches Panel](https://goskills.com/Course/Photoshop-Basic/Lesson/191/The-Swatches-Panel),
    [Brendan Williams Creative](https://www.bwillcreative.com/?p=90740),
    [Adobe: Swatches panel overview (the shared Adobe panel model)](https://helpx.adobe.com/illustrator/desktop/manage-colors/use-swatches/swatches-panel-overview.html),
    [Photoshop Essentials: Swatches in Photoshop 2020](https://photoshopessentials.com/?p=29561).
  - Capture One Layers: Background is the base layer and the list sits on top of it; a checkbox per layer
    shows or hides it; one opacity slider applies to the selected layer; layer tools come from a + and a
    right-click menu; some tools (e.g. Grain) only work on Background.
    [Capture One: Layers and masks](https://support.captureone.com/hc/en-us/articles/360002601658),
    [Capture One: Layer opacity](https://support.captureone.com/hc/en-us/articles/360002615497),
    [Image Alchemist: Capture One layers explained](https://imagealchemist.net/capture-one-layers-explained/),
    [Capture One community: tool functionality in layers](https://support.captureone.com/hc/fr/community/posts/360009396238-tool-functionality-in-layers).

## Result

1. **Swatches** (`crates/pc-ui-egui/src/swatches_panel.rs`):
   - The grid/list is the main content. A search field with an icon and a clear (x) button sits above it.
   - A compact **User | Document** segmented control has the ☰ panel menu at its right.
   - A slim footer has three icon buttons: **New Group** (folder-plus), **New Swatch** (+) and
     **Delete Selected Swatch** (trash). Each has a tooltip. Disabled buttons give the reason in the tooltip
     and report the disabled state to screen readers.
   - The ☰ menu holds: Grid View / List View, Swatch Libraries ▸, Import Swatches…, Export Swatches ▸,
     New Swatch from Current Colour, New Swatch Group, Swatch Options…, Folder Options…, Move to… ▸,
     Apply to Selected Objects and Delete Swatch.
   - Right-click menus:
     - On a swatch: Use as Foreground/Background (Fill/Stroke in Vector), Swatch Options…, Apply to Selected
       Objects (document scope), Move to Folder ▸, Move Earlier/Later, Delete Swatch.
     - On a folder: New Swatch in Folder, Rename Folder…, Delete Folder and Swatches.
   - The hint lines now live in tooltips (swatch tooltip; Document segment tooltip).
   - No icons were added. The existing registry icons are `align-justify` (☰), `folder-plus`, `plus`, `trash`,
     `search` and `x`.
2. **Develop layer bar** (`crates/lc-ui-egui/src/panels/layer_bar.rs`, with the global-only wrapper call in `edit.rs`):
   - With no layers the bar is one line: Background row and +.
   - With layers it shows a "▾ Layers (n) … +" header, then the rows. **Background is the first row**,
     styled like the others. Each layer row has an eye, a **mask thumbnail**, a left-aligned truncating
     name, an opacity value and a ⋮ menu.
   - Under the list, for the selected layer: the Opacity slider, then a row with Mask ▾ (the old per-row "+"
     component menu), Overlay and "Only used tools".
   - Collapsing the list leaves one line: "▸ Layers · <active layer> +".
   - The five whole-image sections are now one collapsed line: "▸ 5 whole-image tools · Select Background".
     Expanding it lists the five, and each name also selects Background.
   - B&W, Vignette and Grain each show one dimmed line inside their section: "Vignette … whole image only ·
     Select Background". I chose in-place lines over moving these three into the group. They sit inside
     sections the user has already opened (Color, Effects). Capture One also greys out unsupported tools
     where they normally appear, so they don't vanish from where people look. Each costs 1 line instead of 3.
   - Thumbnails come from the mask's own geometry (Full, Linear, Radial and Brush, combined with
     add/subtract/intersect and invert, using the pipeline's formulas). They are drawn as a 12×8-cell
     vertex-coloured mesh, which takes microseconds and never waits for a render. AI and range masks show
     their kind icon. A layer with no mask shows a black frame.

## Checks

- `cargo fmt -p lightcraft-ui-egui -p photocraft-ui-egui`: done.
- `cargo clippy --offline -p lightcraft-ui-egui -p photocraft-ui-egui --all-targets -- -D warnings`: clean.
- `cargo test --offline -p photocraft-ui-egui --lib`: **1505 passed, 0 failed, 49 ignored**. This includes the
  GPU swatch screenshot tests on the RTX 5090.
- `cargo test --offline -p lightcraft-ui-egui --lib`: **330 passed, 0 failed, 5 ignored**. This includes
  `tests_develop_a`, `tests_develop_c`, `tests_masking` and `tests_panels`.
- The first lc run found a 1 pt overflow: the new + rows made the right panel 1 pt wider
  (`tests_panels` width asserts). I fixed the gap arithmetic and the full suite above passed after the fix.
- The pc suite rewrites `docs/design/vector-tool-icons.png` as a side effect. That is not my file, so I
  restored it and did not commit it.

## New tests

- Swatches (`swatches_panel_tests.rs`):
  - `ux_swatches_footer_icons_disable_until_selection_and_act`: footer icons, their disabled state,
    + into the selected folder, delete, new group, and keyboard Enter on a focused icon.
  - `ux_swatches_panel_menu_items`: every ☰ item; the selection commands are disabled until something is selected.
  - `ux_swatches_context_menus_on_swatch_and_group`
  - `ux_swatches_segmented_scope_search_clear_and_hints_in_tooltips`
  - `ux_swatches_empty_ungrouped_appears_last_while_dragging`
  - `ux_polish_swatches_screens`: screenshots.
- Layer bar:
  - `tests_develop_a::ux_layer_bar_one_line_thumbnails_and_whole_image_group`: one line without layers;
    Background is the first row; thumbnail placement plus pixel checks (Filled = white, Empty = black);
    a single +; the collapsed group, its link and the expanded items; the collapsed list line.
  - `layer_bar::thumb_tests::thumbnail_coverage_follows_the_mask_geometry`
  - `tests_develop_a::ux_polish_layer_bar_screens`: screenshots.

## TEST-CHANGED (intent preserved)

- `swatches_panel_tests.rs`:
  - "Add Current Colour" → footer "New Swatch"; "New Folder" → footer "New Group".
  - "Swatches menu" → "Swatches panel menu".
  - "List with names" → ☰ "List View", plus an added assert that list view is on.
  - "Swatch Options…" and "Apply to Selected Objects" are now reached through the ☰ menu (`menu()` helper).
    Every assertion is unchanged.
- `tests_develop_a.rs::develop_a_filled_background_and_global_only_links`: `contains("This tool applies to the
  whole image.")` → `contains("whole image only")`. The old 3-line notice is now one line per tool. The
  vignette and B&W link clicks and their assertions are unchanged.

## Screenshots (`target/ux-polish/`, inspected)

- before: `before-swatches-{dark,light}-{250,400}-{1,1.5}x.png`;
  `before-layers-{none,two,sections}-{dark,light}-{250,400}-{1,1.5}x.png`
- after: the same names with `after-`, plus `after-swatches-{menu,context}-dark-250-{1,1.5}x.png` and
  `after-layers-group-open-{dark,light}-{250,400}-{1,1.5}x.png`.
- Regenerate them with `UX_POLISH_PHASE=after cargo test --offline -p photocraft-ui-egui --lib ux_polish_swatches_screens`
  (RTX 5090) and `... -p lightcraft-ui-egui --lib ux_polish_layer_bar_screens`.

## Behaviour notes

- Swatches: right-click on a swatch now opens its menu and selects it without recolouring, as in Photoshop.
  Before, it also set the secondary colour. The secondary colour is still available through Alt-click and
  the menu's "Use as Background/Stroke". Recent Colours chips keep right-click = secondary, since they have no menu.
- Swatches: an empty "Ungrouped" header is hidden. While a swatch is being dragged it appears after the
  folders as a drop target, so nothing above the pointer moves. A focused swatch also takes Delete/Backspace.
- Layer bar: at 250 pt the whole-image group's "Select Background" link drops under the title (two short
  lines) instead of overlapping it. At 400 pt it is one line.

## Needs from other owners

- `edit.rs` (not the global-only part): at 250 pt the Clarity mode segmented control (Natural / Punch / Neutral)
  overflows the panel's right edge. This is visible in `before-`/`after-layers-sections-*-250-*`. Its
  `segmented(…)` call sits outside the section's 24/22 inner frame. Wrapping it in the same
  `Frame::NONE.inner_margin(Margin { left: 24, right: 22, .. })` used for the Vignette style control would fix it.
