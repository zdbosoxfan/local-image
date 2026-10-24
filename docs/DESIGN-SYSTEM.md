# Design system

Local Image 2 is an all-Rust egui 0.36 app. **Our design system is the one we inherited from the two apps
we forked, and we keep it:** the editor (Compositing) follows **PhotoCraft's** system, Library and Develop
follow **LightCraft's**. The owner likes how the app looks, so do not invent a new style. The current code
is the ground truth; this page records what it is, where it came from and where we deliberately differ.
(The retired Fluent/React web app had another system; none of it applies.) Line numbers are at the time of
writing; search for the names if they have drifted.

## 1. Provenance and principles

| Part | Follows | Source document |
| --- | --- | --- |
| Editor tokens, themes, widgets, Photoshop layout grammar, icons, fonts, "verify visually" | PhotoCraft | `docs/upstream/photocraft/ui-design.md` (latest: `docs/ui-design.md` in the upstream repo), UI rules 5 and 6 in its `AGENTS.md` |
| Library/Develop tokens, sliders, panel look, asset and icon rules | LightCraft | no UI design doc exists upstream; the code is the spec: `crates/ui-egui/src/theme.rs` header (dark values measured from the reference app), `docs/upstream/lightcraft/AGENTS.md` (asset rules, "look at the result"), `docs/ui-parity.md` |
| Adobe-asset ban, attribution, `cargo xtask assets` | both | the two `AGENTS.md` files |

Crates: `pc-ui-egui` (editor; upstream `photocraft-ui-egui`) and `lc-ui-egui` (library; upstream
`lightcraft-ui-egui`). Each has `theme.rs`, `widgets.rs`, `icons.rs`. `apps/local-image/src/library_host.rs:49`
(`library_palette`) maps the editor's active theme onto the library's `Tokens`, so one theme choice
recolours both. Use your own crate's module; do not reach across.

**Where we differ from upstream (deliberate):**

- **Themes.** Upstream PhotoCraft ships 14 (Pro, Pro Medium, Pro Light Gray, Pro Light, Studio, Studio Light,
  Classic, Adwaita, Adwaita Dark, Solarized Dark, Breeze Light/Dark, Soft Dark/Light) and LightCraft four
  (Charcoal, Midnight, Silver, Paper). We ship the editor's five (`ThemeKind`, `pc theme.rs:17`: Pro,
  Pro Medium (default), Studio, Studio Light, Classic) and the library takes its palette from the host.
- **New shared widgets** added here to replace one-off painting: editor `icon_action`, `color_tile`; library
  `color_swatch`, `range_strip`, `disclosure`, `image_badge`, `icon_button_as`, and the `overlay_bg`/`overlay_fg`
  tokens. Upstream has none of them. Upstream widgets we do not have: `elided_label`, `panel_footer`,
  `compact_button`, `text_field`. The rule against hand-painted controls is upstream's ("colours and radii
  come from tokens, never hard-coded") made stricter (section 8).
- **Library icons** were drawn in code upstream; we draw them from Lucide SVGs (`lc icons.rs`), and tool
  images are the monochrome Lucide/PhotoCraft/VectorCraft line set shared by both crates (section 4).

**Principles (from the upstream apps and the owner).**

1. **Feel at home.** A Lightroom, Capture One, Photoshop or Affinity user finds things where they expect,
   with familiar names and shortcuts. PhotoCraft's goal is Photoshop parity; LightCraft's is Lightroom's.
2. **Consistency.** One checkbox, one dropdown, one icon button, one colour tile.
3. **No one-off UI.** A screen that paints its own control drifts and misses focus, disabled,
   accessibility and theme behaviour. See rule 8.
4. **The photo is the hero.** Chrome is flat and neutral; colour on screen should be the image's.
5. **Photoshop layout grammar (editor, from PhotoCraft).** Essentials dock order (`pc dock.rs:76`); options-bar
   labels end with a colon (`pc panels.rs:1205`); document tabs read `name @ 12.5% (RGB/8)` (`pc canvas.rs:1417`);
   tool groups carry a corner triangle.

## 2. Tokens

Source: PhotoCraft's `ui-design.md` (Themes, Rules) for the editor; LightCraft's `theme.rs` for the library.

**Where.** `li-ui-kit` owns the canonical `Copy` token union (`src/theme.rs::Tokens`),
`ThemeKind`, Library-only semantic roles (`LibraryRoles`) and Library/Develop layout (`Metrics`).
`pc-ui-egui::theme` re-exports that backend; `lc-ui-egui::theme::Tokens` is a flattened compatibility
view for existing struct literals, with lossless conversion and no independent palette values.
Both style applicators live in the kit. The host's `library_palette` delegates to
`lc-ui-egui::theme::host_palette`, which calls the kit's Library profile and contrast mapping.
Continue using your UI crate's facade while widgets migrate to the shared backend.

**Themes.** `ThemeKind` (`pc theme.rs:17`): `Pro` (darkest), `ProMedium` (default, Photoshop mid-grey),
`Studio`, `StudioLight`, `Classic` (bevelled, square). `Tokens::dark()` (`:328`) tells dark from light.
`Tokens::pro` / `bevel` switch Photoshop-style tab strips and Classic bevels. The library has one dark
default and a light palette supplied by the host (`Tokens.dark`).

**Colour roles (editor).** Surfaces: `chrome` (title bar, toolbars), `canvas`, `dock`, `card`/`card_border`,
`field`/`field_border`, `row_selected`, `tab_strip`. States: `hover`, `pressed`. Text: `text`, `text_dim`,
`text_faint` (also the disabled colour), `icon`. Accent: `accent`, `accent_soft`, `accent_border`,
`accent_text`; primary button `primary_bg`/`primary_text`. Status: `danger`, `warning`. Lines:
`separator`, `shadow`. Scopes: `histogram_bg`, `histogram_level` (use `histogram_color`, `scope_hue`).
**Library** uses `chrome`, `canvas`, `grid_bg`, `inset`, `field`/`field_border`, `button`/`button_border`,
`hover`, `pressed`, `tool_active`, `divider`, `text`/`text_label`/`text_dim`/`text_disabled`, `accent`,
`caution`, and `overlay_bg`/`overlay_fg` for anything drawn over photo pixels (badges, scrims), which must
read the same in light and dark chrome. Image data (a swatch's colour, mask black/white, the
transparency checker) is content, not chrome, and may be literal.

**Shape.** Radii: `radius_sm` / `radius` / `radius_lg` = Pro 3/4/6, Studio 6/8/12, Classic 0/0/0
(`pc theme.rs:100`). Library: buttons and rows 4, windows and menus 6, swatch tile 3 (`SWATCH_RADIUS`),
badge 6 (`BADGE_RADIUS`). Strokes are 1 pt everywhere (`Stroke::new(1.0, ..)`); never thicker for chrome.
Spacing: editor `item_spacing` 8x6, `button_padding` 10x4, `interact_size` 24, `ROW_GAP` 4 between stacked
panel rows (`pc theme.rs:547`), `TOOLTIP_DELAY` 0.35 s (`:543`). Library: item spacing 6x4, button padding
8x3, interact height 22 (`lc theme.rs:201`), metrics `panel_w` 270, `section_h` 57, `slider_row_h` 45.

**Type scale.** Faces: Inter (UI), Inter SemiBold (`theme::semibold`/`Tokens::semibold`), Inter Medium
(editor `medium`), JetBrains Mono for numbers (`mono`). CJK fallbacks load from craft-fonts
(`pc theme.rs:334`, `lc theme.rs:142`). Sizes are one fixed scale: 10.5, 11, 11.5, 12, 12.5, 13, 14 pt
(headings 15 editor / 16 library). Pass them through the helpers (`t.font(12.0)`, `theme::semibold(12.5)`),
never `FontId::new(..)` by hand. Preferences > Interface > UI Font Size scales every face at once
(`pc theme.rs:414`), so do not multiply sizes yourself.

**Add a token.** Add common/editor roles to `li-ui-kit::theme::Tokens` and its theme constructors.
Library-only roles belong in `LibraryRoles`, layout in `Metrics`; update the shared Library profile
mapping and the flattened compatibility conversions when a Library role is exposed to old callers.
Use the role and check contrast in light and dark. The kit's frozen pre-merge goldens guard every
existing value, style and font metric. Do not add a token for a one-off; first try an existing role.
Debug builds still support `PHOTOCRAFT_THEME_FILE` through the editor facade.

**Font backend.** `li-ui-kit::fonts` builds both inherited fallback profiles from the existing
`pc-text::fonts` statics; no UI crate embeds another copy of the Latin fonts. The shared scaling
plugin retains the saved-size first-frame fix. Locale-specific craft-font ordering and the editor's
lazy system CJK source selection remain in their existing UI adapters.

## 3. Widget catalogue

Editor widgets are PhotoCraft's (`widgets.rs`) except where marked new; library widgets are LightCraft's
except where marked new. **New here** (not upstream): `icon_action`, `color_tile`, `status_chip`,
`color_swatch`, `range_strip`, `disclosure`, `image_badge`, `icon_button_as`.

Rule of thumb: reach for these before touching `egui::Button`, `ui.checkbox` or `Painter` directly. Editor
file `crates/pc-ui-egui/src/widgets.rs` (`pc`), library file `crates/lc-ui-egui/src/widgets.rs` (`lc`).

The Library and editor command menu rows share `li-commands::widgets`: themed icon gutter,
button enablement, accessible checked state and row rendering. Each shell supplies translated
labels, effective shortcut text and its existing monochrome icon painter. The editor's
`widgets::menu_row_*` and `command_menu_item` remain compatibility adapters; menu navigation,
scrolling and saved menu tints stay with their established surfaces.

### Editor (`pc`)

| Widget | Purpose and usage | Line |
| --- | --- | --- |
| `card` / `card_ex` | Dock card with pill-tab header, collapsible; `card(ui, id, &tabs, &mut sel, \|ui, i\| ..)` | 37 / 59 |
| `module_switcher` | Shared title-bar module picker; the first control in the right-to-left group in every module/persona | widgets.rs |
| `preferences_tab` | Shared accessible Preferences navigation for application, Library/Develop and AI pages | widgets.rs |
| `pill_tab` (`_translated`) | One tab in a strip or a two-way switch: `pill_tab(ui, "Layers", on)` | 141 |
| `surface` | Flat or Classic-bevelled box behind custom content | 22 |
| `drop_line` | Accent insertion line while dragging onto a row | 10 |
| `slider` / `slider_row` (`_translated`) | Thin slider; labelled row with value field and optional gradient track | 251 / 312 |
| `value_field` (`_with_precision`) | Monospace number field with unit suffix, drag to scrub | 168 / 174 |
| `toggle` | Switch for an immediate on/off setting | 346 |
| `cpu_estimate` / `cpu_batch_estimate` | Shared CPU-AI preflight notices; hardware/tensor-sized estimate, theme warning/caution, interactive refusal vs cancellable batch warning | widgets.rs |
| `checkbox` | The only checkbox: `widgets::checkbox(ui, &mut on, "Label")` | 705 |
| `primary_button` / `secondary_button` | Filled main action / outlined action, `(ui, label, min_width)` | 375 / 380 |
| `dialog_buttons`, `DialogButton`, `ButtonRole` | Every modal's button row, in the platform's order (Default, Alternate, Cancel, Apply) | 435 / 414 / 387 |
| `dropdown` | Compact choice list: `dropdown(ui, id, &mut cur, &[(v, "Label")], width)` | 570 |
| `dropdown_with_tooltips` | Same, each option with a hint | 637 |
| `dropdown_menu` | Dropdown shell for grouped or custom rows | 585 |
| `command_menu_item` | Canvas/panel command row with the shared check gutter, active-profile shortcut text and viewport-wrapped `disabled_reason_tooltip`; caller passes translated text | widgets.rs |
| `menu_scroll` | Body of a context menu that scrolls instead of running off screen | 594 |
| `swatch_popup` | Colour picker popup anchored to a swatch | 612 |
| `icon_action` | Square footer/header icon button with accessible label and disabled-with-reason: `icon_action(ui, "plus", 24.0, "New Swatch", tip, None)` | 660 |
| `status_chip` (new), `Tone` | Small status pill: shape icon plus text in the tone's colour (`Neutral`, `Good` accent, `Warning`, `Danger`), named for accessibility; Set Up Models and the Generate model picker: `status_chip(ui, "circle-dashed", "Not tested", Tone::Neutral)` | widgets.rs |
| `color_control` | Named colour value; both the tile and label open the shared colour picker popup, with one combined anchor and accessibility labels | 699 |
| `color_tile` | Paint a colour value (checkerboard under alpha, `radius_sm`, outline). Paint only; owner supplies click and name | 679 |
| `checker` | Transparency checkerboard | 690 |
| `section_label` (`_translated`) | Small-caps section heading in a panel | 534 |
| `hairline` / `vline` | Horizontal / vertical separators | 545 / 552 |
| `rgb_histogram` | Histogram plot on scope tokens | 523 |
| `hue_stops` | Hue spectrum stops for colour sliders | 559 |
| `chevron_icon` | Dropdown chevron (do not use egui's triangle) | 735 |
| `fmt_num`, `fmt_num2`, `parse_num` | Number display and typed-arithmetic parsing | 744 / 750 / 758 |

Also in `pc-ui-egui/src/icons.rs`: `icons::button` (`:235`, square icon button), `rail_button` (`:249`),
`tool_button` (`:290`, tool images), and `button_chrome` (`:217`, the shared hover/selected fill).

### Library (`lc`)

| Widget | Purpose and usage | Line |
| --- | --- | --- |
| `slider` / `numeric_slider` | Lightroom slider row: label, value, ring thumb; double-click resets, Shift fine | 147 / 162 |
| `section_header` | Collapsible 52 pt panel section: `section_header(ui, id, "Basic", open, enabled)` | 355 |
| `disclosure` | One-line chevron + title for a fold inside a panel; `DISCLOSURE_H` 24 | 548 |
| `flyout_row` | Inset row with icon tile that opens a sub-editor (Curve, Color Mixer) | 396 |
| `text_button` | Small bordered text button (Auto, B&W) | 421 |
| `segmented` | Equal-width segmented control, `per_row` wraps | 445 |
| `icon_button` / `icon_button_as` | Icon-only button; `_as` keeps a legacy automation id like `button:layerNew` | 480 / 486 |
| `tool_button` | Tool-strip button with tool image | 490 |
| `modal` | Shared Preferences fallback with a stable top anchor and egui modal focus/backdrop | widgets.rs |
| `panel_content` | Standard inset for Develop section content and Add tool actions (`theme::PANEL_CONTENT_MARGIN`) | widgets.rs |
| `checkbox`, `choice_menu` | Shared Preferences checkbox and dropdown shell; callers retain legacy automation IDs | widgets.rs |
| `dropdown` | Borderless text + chevron button that opens a menu | 710 |
| `color_swatch` | Colour sample tile, optionally a selectable choice; `SWATCH_RADIUS` 3 | 587 |
| `range_strip` | Caption over a colour strip with selected window(s) outlined; `RANGE_STRIP_H` 14 | 620 |
| `image_badge` | Badge over photo pixels (`overlay_bg`/`overlay_fg`); `BADGE_H` 24, `BADGE_RADIUS` 6 | 654 |
| `stars` | Five-star rating row | 685 |
| `divider` | Thin divider between groups | 388 |
| `cpu_estimate` / `cpu_batch_estimate` | Shared CPU-AI preflight notices, styled with `text_dim` / `caution`; mask and Smart Sort controls | widgets.rs |
| `preview_only_notice` | Amber panel notice for a raw shown from its embedded preview | 45 |
| `register` / `take_registry` | Record a widget's automation id and rect for the control protocol and tests | 17 / 22 |
| `hex`, `lerp`, `BAND_COLORS`, `nudged` | Colour and value helpers | 70 / 75 / 82 / 348 |

Library also provides `checkbox(ui, id, value, label)`, `choice(ui, id, value, options)` and
`text_field(ui, id, value, label)` and `field_with_action` in `widgets.rs`. Import safety, catalog backup preferences and
proof-sheet dialogs use these shared controls. They inherit the active light/dark theme, expose
accessible labels, register automation IDs, and text fields adapt to the available width. Fields with an action stack the button below at narrow widths.

### Shared widget backend

`li-ui-kit::widgets` owns the paired icon action, colour tile/swatch, choice shell, selection/tab,
separator, heading, text button and checkbox controls, plus the automation registry and disabled
reason tooltip. `Profile::Editor` and `Profile::Library` select the inherited theme vocabulary and
layout; both old `widgets.rs` modules are compatibility adapters for these pairs. Callers retain
translation, tool-icon selection, numeric edit ownership, and document/session state.

`li-ui-kit::widget_style` holds the inherited reusable geometry and type tokens. Colours come from
`theme::Tokens`; both palettes keep their established appearance. Transparent Library colour tiles
now use the same transparency checker as Editor tiles. Selection controls and section-enable controls
also expose their selected/checked state and keyboard focus.

`li-ui-kit::slider` owns range mapping, drag/release/fine-motion accumulation, nudge quantization,
and gradient/track painting. Editor and hollow Library thumbs retain their profiles; Library numeric
owner/draft guards remain in the Library adapter. Library tracks use continuous gradients instead of
48 sampled rectangles; this reaches both gradient endpoints exactly.

`li-ui-kit::histogram` borrows existing 256-bin histograms (`u32`/`u64`) with Editor ribbon and Library
sqrt/luma profiles. Scope colour roles live in `Tokens`. Clipping responds to click and keyboard.
Domain statistics and cached metadata stay in their callers.

`li-ui-kit::notices` owns the bounded session model (3 active notices, 256 history entries), operation
completion and history view. Editor keeps its notice row; Library keeps transient HUD timing and adds
persistent Details / History. History never enters saved settings and dismissal never erases diagnostics.

## 4. Icons

Backend: `li-ui-kit::icons` interns the existing registries by SVG content, prepares white SVGs
lazily, rasterizes through the existing egui_extras/resvg implementation, and shares one per-context
texture cache across image widgets, Library painters and colour tools. The normal resident limits
are 1024 entries and 8 MiB; textures used in the current frame remain alive until they can be retired.
Keys use physical pixels, stroke profile, tint, disabled state and texture options. Interface strokes
retain 1.75 px; tools retain their sourced 2 px (or 1.5 px) geometry. Public IDs and artwork are unchanged.

Owner decision 2026-10-13: industry-standard **monochrome line icons**, the set PhotoCraft and VectorCraft
ship. The hand-drawn Glossy Duotone tool icons and LightCraft's code-painted glyphs are retired.

- **Sources, in order.** Lucide **0.468.0** (ISC, pinned at `f12b0de1`; Lucide renamed icons later, so
  never mix in newer names without recording them), then PhotoCraft originals and VectorCraft `dc-*` icons
  (MIT OR Apache-2.0), then **Tabler** outline (MIT) as the same-style fallback. Only where none has the
  metaphor (or the owner rejected upstream's) do we draw a **Local Image original** (GPL-3.0-or-later).
  Never: Remix Icon (custom licence), PhotoCraft's `lasso-magnetic` (CC BY 3.0), GNOME CC-BY-SA-3.0
  artwork, or anything from Adobe (Photoshop, Lightroom and Illustrator only inform the metaphors).
- **Style.** 24×24 viewBox, `stroke="currentColor"`, 2 px stroke, round caps and joins, `fill="none"`,
  Lucide's optical padding (shapes inside 2..22). Upstream files are copied byte for byte; VectorCraft's
  `dc-*` glyphs keep their 1.5 px stroke. Originals follow Lucide's rules exactly and must read at
  16 and 20 px: one idea per icon, a small badge at most.
- **Colour.** Icons take the theme's `icon` colour (Tokens); the active tool keeps the button's
  `accent_soft` fill and `accent_border` (Photoshop's highlight), and disabled tools use `text_faint`.
  **Monochrome is the default.** The Preferences "Colour" style is a subtle family tint of the same line
  icon, never separate artwork: `li_ui_kit::color_icons::tint` pulls `icon` 70% toward the family hue from
  `li_ui_kit::color_icon_data` (`TOOL_FAMILIES`, `family_hue`; a lighter shade on dark themes, a deeper one on
  light themes). Selection tools are blue, retouch green, paint orange, shapes cyan, transform/crop
  purple, measure slate, samplers yellow, Liquify pink; navigation and panels stay neutral.
- **Size.** Draw at about half the button box (`box_size * 0.52`); tool buttons draw 20 to 24 px
  (`(box_size * 0.62).clamp(20, 24)`). Rasterize at the physical pixel size (both renderers already do).
- **Use.** Editor: `icons::button(ui, "name", box, selected, "Tooltip")`, `icons::tool_button`, or
  `widgets::icon_action`; tool images via `icons::tool_icon` / `icons::paint_tool`. Library/Develop:
  `widgets::icon_button(ui, id, Icon::X, size, active, enabled, "Tooltip")` and `icons::paint_tool`;
  `lc icons::paint` draws each `Icon` from its SVG (`icons::svg`). Never type a character
  (`+`, `x`, arrows) as an icon and never hand-paint a glyph with the painter.
- **Add a tool icon.** Copy the upstream SVG unchanged (or draw an original) to
  `assets/icons/tools/<id>.svg`; add its row (set, source, licence, family, SHA-256) to
  `assets/icons/source-manifest.json` and to the table in `assets/icons/SOURCES.md`; register it in
  `li-ui-kit/src/tool_icon_data.rs` (or `li-ui-kit/src/vector_icon_data.rs`) and its family in
  `li-ui-kit/src/color_icon_data.rs` (both shells use these directly). Then run
  `cargo xtask icons` (hashes, licences, line style and registries), `cargo xtask attributions`, and
  `cargo xtask tool-icons`, and look at `docs/design/tool-icons.png` at 16 and 20 px on both themes.
- **Add an interface icon.** Put the Lucide 0.468.0 SVG in `assets/icons/<name>.svg` unchanged.
  Editor: register it in `li-ui-kit/src/icon_data.rs` (`ICONS`). Library/Develop: add an `Icon`
  variant and its file in `li-ui-kit/src/library_icons.rs` (`svg`). List it in `assets/icons/SOURCES.md`.
- **Attribution.** Every asset needs a licence row or `cargo xtask assets` (part of `cargo xtask ci`)
  fails. Licence texts: `licenses/lucide-ISC.txt`, `licenses/feather-MIT.txt`,
  `licenses/photocraft-LICENSE-{MIT,APACHE}`, `licenses/vectorcraft-LICENSE-{MIT,APACHE}`,
  `licenses/tabler-icons-MIT.txt`; originals are under `LICENSE`.

## 5. Layout rules

- **Panel inset.** Develop panels indent content `Margin { left: 24, right: 22 }` (see
  `lc panels/masking.rs:77`, `layer_bar.rs:43`, `develop_tools.rs:16`). A row that skips the inset sits
  flush against the panel edge; this has been a real bug. Use the same inset for new Develop rows.
- **Dock widths.** Editor dock: 250 to 520 pt, default 290/300 (`pc panels.rs:1332`). Library right panel:
  250 to 520, default 270; left sidebar 200 to 480 (`lc state.rs:238`). Everything must work at **250 pt
  (narrow)** and **400 pt (normal)**: no clipped labels, no horizontal scrollbar, long text elides with
  "...", buttons that do not fit wrap to their own row.
- **HiDPI.** Test at 1.0 and 1.5 pixels-per-point (examples: `pc menu_nav_tests.rs:10`). Size in points,
  never pixels; icons are rasterised at the exact on-screen size so they stay crisp.
- **Scrolling.** A column that can overflow is wrapped in a vertical `ScrollArea`; context menus use
  `widgets::menu_scroll`. A dock group never grows with its content; taller content scrolls inside it (`pc dock.rs:4`).
- **Rows.** Stack panel rows with `ROW_GAP`; align labels, fields and sliders on one left edge.

## 6. Accessibility

- Every interactive widget has an accessible name. Icon-only buttons need a label as well as a tooltip
  (`icon_action(.., label, tip, ..)`; `icons::button` uses the tooltip text); tiles and strips get one via
  `widget_info`/the `label` argument (`color_swatch`, `range_strip`).
- A disabled control says why: pass the reason as the tooltip (`icon_action(.., Some("Open a document first"))`),
  never just grey it out. Disabled must also mean no click, no focus, and disabled for screen readers.
- Everything is reachable by keyboard: Tab order follows reading order, focus shows the same fill as hover,
  Enter/Space activate, Esc closes. Dialog buttons come from `dialog_buttons` for the right order and Tab walk.
- Text contrast must stay readable on every theme; derive colours from tokens (`text_on`, `readable_text`
  in `lc theme.rs:252`). Translate user-visible text with `tl!("...")` (editor) / `li_i18n::library::t` (library), both from `li-i18n`; the
  `_translated` widget variants are for text already translated.

## 7. Testing

- **Behaviour.** `egui_kittest` harnesses drive the real UI and query by accessible name or automation id
  (`pc-ui-egui` tests such as `props_layout_tests.rs`, `menu_nav_tests.rs`; widget tests live beside the
  widget, e.g. `widgets::tests`). A new widget gets a test that it names itself, responds, and, if it can
  be disabled, explains why.
- **Library/Develop.** The `headless.rs` driver (`lc-ui-egui/src/headless.rs`) runs the whole app offscreen
  with CPU rasterisation; tests click by automation id (`ui.clickWidget`) and read rectangles.
- **Screenshots.** Upstream's rule ("verify every visual change": PhotoCraft `ui-design.md`, `AGENTS.md` rule 6;
  LightCraft "look at the result") applies: use the offscreen `snapshot` example
  (`crates/pc-ui-egui/examples/snapshot.rs`) or the control channel's `ui.screenshot` (`headless: true` for the
  library). UI changes are checked as pictures in **both themes at both widths** (dark and light, 250
  and 400 pt). Reference harnesses, run with `--ignored`: `pc design_system_shots.rs` (kittest, GPU, Pro vs
  Studio Light) and `lc tests_design_system.rs` (headless, default dark vs `tests_develop_c::light()`);
  output goes to `target/design-system/`. Look at them; passing assertions do not prove it looks right.
- Do not run the full workspace build for a UI tweak; run the owning crate's lib tests.

## 8. The rule

**Never hard-code colours/sizes or hand-paint a control that a widget provides; if a new element is needed,
add one documented reusable widget to the shared module and use it everywhere it applies.**

In practice: no `Color32::from_rgb` or `from_gray` in chrome, no `rect_filled` for a button, no raw
`ui.checkbox`/`ComboBox`/`ui.button` where `widgets::` has one, no private copy of a widget in a panel file.
A new widget goes in `widgets.rs` with a doc comment saying what it is for and where it is used, themes
through tokens, names itself for accessibility, and replaces every ad hoc copy in the same change.

Optics uses `lc-ui-egui::widgets::checkbox` (registered accessibility/test ID),
`develop_body` (theme `DEVELOP_INSET`) and `panel_note` (token small type/dim text).
These shared helpers style the profile master, lens-database switch and profile
availability note in both themes. Dropdowns elide labels to the available width.
The RAW Look row uses the same eliding dropdown and parameter slider widgets;
it keeps their widths within the available panel body instead of fixed ComboBox
minimum widths. The headless lens test verifies declared panel/window bounds.
Shared navigation metrics retain the existing pill typography/padding/height as
`PILL_TAB_FONT_SIZE`, `PILL_TAB_PADDING`, `PILL_TAB_HEIGHT`. Preferences navigation uses
`PREFERENCES_TAB_HEIGHT`, `PREFERENCES_TAB_INSET`, `PREFERENCES_TAB_TEXT_MARGIN`.
Library Preferences rows share `PREFERENCES_BODY_WIDTH`; choices use `BUTTON_FONT_SIZE`,
`CONTROL_FONT_SIZE` and `DROPDOWN_ARROW_SPACE`. These metrics preserve the existing visual sizes.
`widgets::profile_choice` is the shared camera/lens profile picker: a labelled, width-bounded
combo using theme text and panel inset tokens. Its choices carry group headings and automation
ids (`combo:<id>` / `choice:<id>:<profile>`), and it is used in Develop and the profile browser.
