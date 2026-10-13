# BUILD: design-system (Claude)

Owner instruction: "Make sure all of this new stuff follows the design system and hasn't been custom
coded. If a new element is needed make it look like the rest of the app, and create an element that is
reusable rather than one off code."

Branch `wip/design-system`, worktree `.claude/worktrees/design-system`, from `claude/v2-continue`
(46fbd385). Scope: every UI change since 6024e4c7 in `crates/pc-ui-egui`, `crates/lc-ui-egui`,
`apps/local-image` (`git diff 6024e4c7..HEAD --stat`).

## What "the design system" is in this codebase

`docs/DESIGN-SYSTEM.md` and `docs/UI-DESIGN.md` describe the predecessor web app (Fluent UI React,
"Local Remove"): they do not apply to the egui app. The real design system is code:

- Editor (pc-ui-egui): `theme.rs` `Tokens` (colours, `radius_sm/radius/radius_lg`, `ROW_GAP`,
  `medium()/semibold()/mono()` fonts), `widgets.rs` (`pill_tab`, `primary_button`/`secondary_button`,
  `dialog_buttons`, `checkbox`, `toggle`, `dropdown`, `section_label`, `hairline`, `vline`,
  `checker`, `swatch_popup`, `value_field`, `slider_row`), `icons.rs` (`button`, `button_chrome`,
  `rail_button`, `tool_button`, Lucide SVGs in `assets/icons`), `notices.rs`, status bar.
- Library/Develop (lc-ui-egui): `theme.rs` `Tokens` (chrome/inset/field/button/hover/tool_active,
  text/text_label/text_dim/text_disabled, accent, caution, `overlay_bg/overlay_fg` for badges over
  photo pixels; host palette mapped from the editor theme in `apps/local-image/src/library_host.rs`),
  `widgets.rs` (`slider`/`numeric_slider`, `section_header`, `flyout_row`, `text_button`,
  `segmented`, `icon_button`/`tool_button`, `dropdown`, `stars`, `preview_only_notice`, automation
  `register`), `icons.rs` (vector pen glyphs).

Font sizes: both crates pass literal sizes to the font helpers (`t.font(12.0)`,
`theme::semibold(12.5)`), on a fixed scale (10.5, 11, 11.5, 12, 12.5, 13, 14). A size on that scale
through the helper is the house style and is not counted as a violation below; colours, radii,
strokes and hand-painted controls are.

## Step 1: audit

(a) = uses existing widgets/tokens correctly; (b) = hand-paints or hard-codes what an existing
widget/token provides; (c) = genuinely new element.

Line numbers are at `claude/v2-continue` (46fbd385), before this job's changes.

### Editor (pc-ui-egui)

| Element | Verdict | Finding (file:line) | Existing equivalent |
| --- | --- | --- | --- |
| Swatches footer / panel-menu / clear-search icons | (b)+(c) | `swatches_panel.rs:754` private `icon_button`: a second square icon button next to `icons::button` (`icons.rs:235`), adding a disabled reason tooltip, focus highlight and a label separate from the tooltip. Nothing shared offers "disabled with a reason". | `icons::button`/`button_chrome` (`icons.rs:217-247`), used by every panel footer (`brushes_tab.rs:392-401`, `actions.rs:136-153`, `comps_ui.rs:148-163`) |
| Swatches footer separator | (b) | `swatches_panel.rs:1202-1204` paints a 1 px `rect_filled(.., t.separator)` | `widgets::hairline` (`widgets.rs:545`) |
| Swatches "User / Document" scope | (b) | `swatches_panel.rs:765-786` builds a segmented control from two `egui::Button`s with hard-coded corner radii (`CornerRadius { nw: 4, .. }`) and a hard-coded 22 pt height | the options bar's Pixel / Vector switch (`vector_packages/persona.rs:285-298`): `egui::Button::selectable` pair, 1 pt apart, theme radii and height |
| Swatch chips (grid and list) and Fill/Stroke target buttons | (b)+(c) | `swatches_panel.rs:721-753` grid chip = an `egui::Button` filled with the swatch colour; list chip and `targets_picker` (`:354-357`) pad the label with spaces and paint an unbordered colour square over the button; translucent colours have no checkerboard. Every colour sample in the editor is painted ad hoc (`vector_ui.rs:835` 26×18 r2, `swatches_doc.rs:51` 28×24 r2, `panels.rs:1622` 26×26 r6, `panels.rs:380` chips r5) | none shared: a colour tile is the new element |
| Swatch "Live colour" corner marker | (a)* | `swatches_panel.rs:741-745` fixed white/black triangle over the swatch colour (Photoshop's global/spot marker). It sits on a document colour, not chrome, like the checkerboard; kept | — |
| Swatch editor, panel menu, context menus, Recent Colours, errors | (a) | `egui::Button`/`ui.checkbox` inside popups and the editor, `CollapsingHeader`, `t.danger` labels | menus elsewhere use plain egui buttons too |
| Shared background-removal settings | (b) | `background_ui.rs:179-180` Refine / Color Decontaminate use `ui.checkbox`; neighbouring panels use `widgets::checkbox` (`panels.rs` ×20, `generate_ui.rs` ×3, `prefs_ui.rs` ×4) | `widgets::checkbox` |
| Background-removal model download (`tier_download`) | (b) | `background_ui.rs:120-143` plain `ui.button` Download/Cancel, untyped info and error labels | Local AI's model rows (`ai_ui.rs:636-680`): `widgets::secondary_button`, info `text_faint` 11 pt, errors `t.danger` 11.5 pt, progress text |
| Model dropdown with tooltips, output dropdown, menu | (a) | `widgets::dropdown_with_tooltips`, `widgets::dropdown`, `section_label`, `hairline` | — |
| Batch Remove Backgrounds section (`generate_ui.rs` batch window) | (b) | `generate_ui.rs:2711-2738` "File Naming", "Format", "Action" and "Example:" labels in the default colour while the rest of the window ("Save to:", image count) labels with `t.text_dim` | same window `:2741` |
| Shortcut editor Scope / Profile row | (b) | `prefs_ui.rs:1514-1590` Scope and Profile are raw `egui::ComboBox`es (egui's triangle, no fixed width) while the same dialog's other pickers use `widgets::dropdown` (`:1773`); Duplicate / Rename / Delete Profile are raw `egui::Button`s; "Profile" label and the profile notes in the default colour while "Search" is `t.text_dim` | `widgets::dropdown`, `widgets::secondary_button` |
| Preferences "Keyboard Shortcuts…" entry | (b) | `prefs_ui.rs:1157` raw `ui.button` | `widgets::secondary_button` |
| Shortcut list rows, conflict column | (a) | `Button::selectable` rows and `t.warning`/`t.text_faint` small labels follow the existing list | — |
| Tablet compatibility setting | (a) | `prefs_ui.rs` `widgets::dropdown` row with tooltip, as other enum prefs | — |
| Performance › Processing | (a) | Grid of `t.text_dim` labels + `widgets::dropdown`, heading `semibold(12.5)` like "Graphics" (`prefs_ui.rs:1209`) | — |
| Brushes import summary, "approximate" note | (a) | status-bar text and the preset tooltip (`brushes_tab.rs`, `preset_files_ui.rs`) | — |
| "Selecting subject…" status with Cancel | (b) | `jobs_ui.rs:440-445` the status bar's × is hand-painted (allocate + hover fill + icon) and has no accessible name; the readout text itself reuses `status_progress` | `icons::button` (same `button_chrome`, names the button) |
| Autosave failure notice | (a) | `prefs_ui.rs` `autosave_failed` posts through `notices::post` | — |
| File dialog "no backend" notice | (a) | `library_host.rs:1102-1107` routes to the Library toast or `notices::error` | — |
| Puppet Warp refusals | (a) | errors returned to the existing status/notice path (`puppet_ui.rs`) | — |
| New picker/tool icons (`tool_icon_data.rs`, `color_icon_data.rs`) | (a) | added through the icon sheets | — |

### Library / Develop (lc-ui-egui)

| Element | Verdict | Finding (file:line) | Existing equivalent |
| --- | --- | --- | --- |
| Layer bar "+" and ⋮ buttons | (b) | `layer_bar.rs:192-204` private `small_button` duplicates `widgets::icon_button` (`widgets.rs:480`) only to keep its automation id (`button:layerNew`, `button:layerMenu:{id}`) instead of `icon:{id}` | `widgets::icon_button` |
| Layer bar header "▾ Layers (n)" and the "N whole-image tools" line | (b)+(c) | `layer_bar.rs:399-418` and `:87-99`: two hand-painted copies of the same small disclosure row (chevron + semibold 12.5 title, hover colour) | `section_header` (`widgets.rs:355`) is the 52 pt section header, too big for an inline row: a small disclosure row is the new element |
| Layer rows, Background row | (a) | selection `t.tool_active`, hover `t.hover`×0.7, radius 4, as the Masking list (`masking.rs:200-212`) | — |
| Mask thumbnail (`mask_thumb`) | (b) | `layer_bar.rs:296-339` image-dependent masks get a hard-coded `from_gray(40)` tile and `from_gray(225)` icon. The black/white coverage itself is mask data (black = no effect), like the checkerboard, and stays | `overlay_bg`/`overlay_fg` tokens (badges over image content, `grid.rs:596`, `profiles.rs:139`) |
| Canvas layer pill | (b)+(c) | `layer_bar.rs:633-653` paints a `t.chrome` pill over the photo: in the light palette that is a light pill on the photo, unlike every other canvas badge | photo badges use `overlay_bg/overlay_fg` or a black scrim: `detail.rs:301-305` (hover preset name), `grid.rs:596`, `mod.rs:88` toast. None is shared: an image badge is the new element |
| WB / Skin / Defringe / Point Color picker badge on the canvas | (b) | `detail.rs:806-813` literal `from_black_alpha(200)`, `Color32::WHITE` icon and ring | same image badge |
| WB / Skin Tone / Defringe picker icons | (a) | `icons.rs` new `Icon::WhiteBalance/SkinTone/Defringe` drawn with the shared pen; buttons through `widgets::tool_button` | — |
| "Global only" lines and "Select Background" link | (a) | `layer_bar.rs:41-63` dimmed `text_disabled` label + egui link in the theme's hyperlink colour | `preview_only_notice` is a bordered panel notice for a different purpose |
| Skin Tone swatch + readout | (b)+(c) | `edit.rs:1057-1061` hand-painted colour tile (r4, `button_border`). Same pattern twice more in Develop: film base (`edit.rs:685-688`, r3) and Point Color samples (`edit.rs:1663-1669`, inset r3 + `t.text` ring when selected) | none shared: colour tile is the new element |
| Skin Tone range strips | (c) | `edit.rs:1086-1112` hand-painted 64-step strip with a selected-window marker in literal `Color32::BLACK`/`WHITE` | none |
| Skin Tone layout insets | (b) | `edit.rs:1035`, `:1086` frames use a 12 pt inset and the two checkboxes (`:1115-1128`) none, so they sit flush with the panel edge, while the sliders and every other Develop row use the 24/22 pt panel inset (Point Color `edit.rs:1652`, `:1690`) | `Margin { left: 24, right: 22, .. }` |
| Skin Tone hint, labels, checkboxes, Clear | (a) | `t.font(11.0)`/`t.text_label`, `ui.checkbox`, `text_button`, as Point Color and Masking | — |
| Curve editor readability | (a) | `edit.rs:1173-1400`: plot `t.grid_bg`, grid `t.field_border`, channel colours from `Tokens::curve_color` (AA-checked against the plot), selection `t.tool_active`. Replaced the old literal greys | — |
| Clarity mode segmented | (a) | `widgets::segmented` with the Vignette inset | — |
| Mask panel "New Filled/Empty Layer", overlay "Only while drawing" | (a) | `text_button`, `ui.checkbox` with ids, as the rest of Masking | — |

### Pre-existing, out of scope (not new since 6024e4c7), noted for later

- lc `detail.rs:301-305`, `grid.rs:630-660`, `compare.rs:325`, `host_composite.rs:78` paint photo
  badges with literal `from_black_alpha(..)`/`Color32::WHITE`; the hover-preset label is swapped to
  the new badge here (a clean swap), the others are left.
- lc curve channel selector (`edit.rs:1185-1225`) is a hand-painted radio group; it predates this
  range (only its selected fill is new).
- pc colour samples outside the Swatches panel (`vector_ui.rs:835`, `panels.rs:380` chips,
  `gradient_ui.rs:566`) could move to the new colour tile later.
- lc theme doesn't set `hyperlink_color`; Library links use egui's default blue.

## Step 2: fixes

### New reusable widgets

Editor, `crates/pc-ui-egui/src/widgets.rs`:

- `icon_action(ui, icon, box_size, label, tip, disabled: Option<&str>)`: compact square icon
  button for panel footers/headers, on the shared `icons::button_chrome` (hover and Tab focus),
  with an accessible `label` separate from the tooltip and a disabled state that explains itself
  (no clicks or focus, `text_faint` icon, the reason as tooltip, disabled for screen readers).
  Replaces the Swatches panel's private `icon_button`. Used by: Swatches footer New Group / New
  Swatch / Delete, the ☰ panel menu, Clear search (5 places).
- `color_tile(ui, rect, color)`: the one way a colour value is painted as a colour: `radius_sm`,
  `field_border` outline, transparency checkerboard under translucent colours. Paint-only (the
  owning button/chip keeps the click and the accessible name). Used by: Swatches grid chips (now a
  framed chip with the tile inside, so hover/focus/selected outline show), list rows,
  Foreground/Background (Fill/Stroke) target buttons.

Library/Develop, `crates/lc-ui-egui/src/widgets.rs`:

- `color_swatch(ui, id, color, size, selected: Option<bool>, label)`: colour sample tile,
  `button_border` outline, radius `SWATCH_RADIUS` (3); `Some(_)` makes it a clickable choice with
  a `text` ring when selected and a `text_dim` ring on hover/focus. Registered under the given
  automation id, accessible label. Used by: Skin Tone reference, Negative film base (older, clean
  swap), Point Color samples (older, clean swap).
- `range_strip(ui, id, label, sample, windows)`: caption + full-width colour strip with the
  selected window(s) outlined in the `overlay_bg`/`overlay_fg` pair (was literal black/white).
  Used by: Skin Tone hue, saturation and lightness ranges.
- `disclosure(ui, id, title, open, width, secondary)`: one-line chevron + semibold title,
  hover/focus colour, elides long titles, collapsing-header accessibility. Used by: the layer bar
  header "▾ Layers (n)" (`button:layerCollapse`, accessible name still "Layers") and the layer
  view's "N whole-image tools" line (`layerGlobalGroup`).
- `image_badge(painter, at, icon, text, max_width)`: badge over photo pixels in the
  `overlay_bg`/`overlay_fg` pair (same in light and dark chrome), 24 pt, radius 6, elides text.
  Used by: the canvas active-layer pill (`activeLayerPill`, was a `chrome` pill that turned light
  in the light palette), the WB/Skin/Defringe/Point Color picker cursor badge (was literal
  black/white), and the preset-hover name label in `detail.rs` (older, clean swap).
- `icon_button_as(ui, automation_id, icon, size, tooltip)`: `icon_button` registered under a full
  automation id. Replaces the layer bar's private `small_button` for "+" (`button:layerNew`) and
  ⋮ (`button:layerMenu:{id}`). `icon_button`/`tool_button` now also light up on Tab focus (as
  `small_button` did), no other change.

### Replaced with existing widgets/tokens

- Swatches: scope toggle is the persona switcher's `Button::selectable` pair with the theme's
  radii and height (accessible names "User Swatches" / "Document Swatches" kept; Document
  disabled without a document, with its reason). `widgets::pill_tab` was tried first but needs
  the editor fonts, which the Swatches test harness doesn't install (16 tests panicked on
  `FontFamily::Name("medium")`); the persona idiom is the closer match anyway and needed no test
  change. Footer separator is `widgets::hairline`.
- Background removal: Refine / Color Decontaminate are `widgets::checkbox`; the model download
  row uses `widgets::secondary_button` (Download, Cancel), a progress bar with percentage, info
  in `text_faint` 11 pt and errors in `danger`, as Local AI's model rows.
- Batch Remove Backgrounds: File Naming / Format / Action labels `text_dim`, the example
  `text_faint`, as the window's other labels.
- Shortcut editor: Scope is `widgets::dropdown`, Profile `widgets::dropdown_menu` (user profile
  names are never translated), both 180 pt; Duplicate / Rename / Delete Profile are
  `widgets::secondary_button` on their own row (so they keep their size at 480 pt); the dialog's
  other commands (Use Default, Delete Shortcut, Reset All to Defaults, Show All Menu Items) are
  `secondary_button` too so the dialog is consistent (clean swap, same labels and stored rects);
  labels and notes `text_dim`. Preferences' "Keyboard Shortcuts…" entry is a `secondary_button`.
- Job status bar: the × Cancel is `icons::button` (adds the accessible name "Cancel (Esc)").
- Layer bar mask thumbnails: image-dependent masks use `overlay_bg`/`overlay_fg` (was
  `from_gray(40)`/`from_gray(225)`); the black/white mask coverage stays (mask data).
- Skin Tone: rows use the 24/22 pt panel inset (were 12 pt and 0 pt).

Behaviour, automation ids and accessibility labels are unchanged except where noted (new names
added: "Cancel (Esc)", "Skin reference", "Film base", "Point color sample N", "<label> range").

### Widget duplication between the two crates (later consolidation, not merged here)

pc and lc each have: an icon button (`icons::button` + `widgets::icon_action` vs
`widgets::icon_button`/`icon_button_as`), a colour tile (`color_tile` vs `color_swatch`), a
dropdown (`widgets::dropdown` vs `widgets::dropdown`), a segmented/tab control (`pill_tab` vs
`segmented`), a separator (`hairline` vs `divider`), a section label/header, a checkerboard
(`widgets::checker` vs none). They draw from different token sets (pc `Tokens` vs lc `Tokens`, the
host maps one to the other), so merging needs a shared token trait first. No cross-crate
dependency was added.

## Screenshots

Harnesses (both `#[ignore]`, run explicitly; `DESIGN_SYSTEM_PHASE=before|after`):
`crates/pc-ui-egui/src/design_system_shots.rs` (`design_system_editor_screens`, RTX 5090 Vulkan,
egui_kittest; Pro = dark, Studio Light = light) and `crates/lc-ui-egui/src/tests_design_system.rs`
(`design_system_develop_screens`, Library/Develop headless CPU raster; default = dark,
`tests_develop_c::light()` = light). Output in `target/design-system/`, 36 before + 36 after:

- `{before,after}-editor-swatches-{dark,light}-{250,400}.png` (Swatches panel, a swatch selected)
- `{before,after}-editor-shortcuts-{dark,light}-{480,900}.png` (shortcut editor Scope/Profile row)
- `{before,after}-editor-batch-bg-{dark,light}-{520,640}.png` (Remove Backgrounds window)
- `{before,after}-develop-layers-{dark,light}-{250,400}.png` (layer bar: Background, Filled, Color Range)
- `{before,after}-develop-pill-{dark,light}-{250,400}.png` (canvas active-layer pill)
- `{before,after}-develop-global-only-{dark,light}-{250,400}.png` (B&W / Vignette / Grain lines)
- `{before,after}-develop-global-group-{dark,light}-{250,400}.png` ("5 whole-image tools" open)
- `{before,after}-develop-skin-{dark,light}-{250,400}.png` (Skin Tone with a reference picked)
- `{before,after}-develop-picker-{dark,light}-{250,400}.png` (Skin Tone picker badge on the canvas)

Inspected: Swatches chips are now framed tiles with outlines (light theme white swatch now has an
edge), scope toggle matches the persona switch; shortcut profile commands keep full size at 480
pt on their own row and match the dialog's other commands; batch window download button is the
outlined secondary button, info line faint, checkboxes the editor's; layer bar header/lines are
pixel-near identical (same metrics, shared code); canvas pill is a dark badge in the light
palette too (was a light pill on the photo); Skin Tone rows now align with the sliders (24 pt
inset; the checkboxes were flush with the panel edge) and the range markers use the overlay pair.

## Tests

New: `widgets::tests::icon_action_names_itself_and_disables_with_a_reason` (pc),
`widgets::access_tests::design_system_widgets_describe_themselves_and_register` (lc), plus the
two ignored screenshot harnesses above.

Final runs (rustfmt clean; `cargo clippy --offline -p photocraft-ui-egui -p lightcraft-ui-egui
--all-targets -- -D warnings` clean):

- `cargo test --offline -p photocraft-ui-egui --lib`: 1531 passed, 0 failed, 51 ignored (after
  the scope-toggle fix above; the first run had the 16 Swatches font panics).
- `cargo test --offline -p lightcraft-ui-egui --lib`: 331 passed, 0 failed, 6 ignored; the new lc
  access test was added after that run and passes on its own (1 passed). The only lc change after
  the suite run is `color_swatch`'s accessibility role for display-only tiles (Label → Other, so
  the name is exposed).

TEST-CHANGED: none. No existing test or query was edited.

## Needs from other owners

None. No change was needed in `tests_develop_*.rs`, `canvas.rs`, `gpu_canvas.rs`,
`canvas_brush_batch_tests.rs` or engine crates; all automation ids the Develop tests use
(`button:layerNew`, `button:layerCollapse`, `button:layerMenu:{id}`, `layerGlobalGroup`,
`layerBackground:*`, `activeLayerPill`, `layerThumb:{id}`, `[layer-]skinToneSwatch`,
`[layer-]skinToneRange:{Hue,Saturation,Lightness}`, `pointColor:{i}`, `negative:filmBase`,
`icon:skinTone`) are kept.
