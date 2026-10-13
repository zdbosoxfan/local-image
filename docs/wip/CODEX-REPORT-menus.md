# Editor menus, toolbar flyouts and Preferences: U018–U022, U031

Base: `90d89ff3`, branch `wip/cx-menus`. Continued from the integrated editor and the interrupted worker's reference patch; the patch was reviewed against current code rather than applied wholesale. No commit, installation, desktop window, new dependency, engine edit or settings-schema change.

## Completed

| Item | Result and evidence |
| --- | --- |
| U018 | Pixel keeps the Photoshop sequence. Vector uses File, Edit, Object, Type, Select, Effect, Image, Layer, Filter, View, Window, Help. Help is last. Removed the separate, duplicate Vector bar from the rendering path; its view modes remain available in View and its panels in Window. `menus.rs::persona_menus`, `menu_bar_with_width`, `invoke_unguarded`; registration-only changes in `vector_integration.rs::menu_items`. |
| U020 | Moved the catalogue's first Geometry child immediately after Combine Shapes, before Align. The registration replaces that row in place. Layer starts with its normal New entries. `menu_catalog.rs:430`; asserted in `menus_ux_order_geometry_and_verified_duplicates` and visible in the 1440-wide Layer screenshots. |
| U021 | The real editor title bar reserves room for module, workspace and caption controls. Translated titles are measured using the active button font; when they do not fit, one Menu button exposes the same ordered tree as submenus. Reuses `menu_nav::level`, scroll arrows, wheel navigation, press-drag release and keyboard navigation. F10 opens the menus when keyboard ownership permits; explicit profile bindings retain F10. Focused titles also open with Enter/Space. Help search works inside overflow. Tests exercise every root by pointer and keyboard at 760×480 and 1440×900; pointer coverage includes Pro and Studio Light. Modal and Tab behavior is preserved. |
| U031 | Filmstrip, Generate, Draft / Refinement and Model Browser report their open state; vector view modes and feature-panel rows also carry checks. Labels and submenu labels share a themed icon gutter; checks are painted with the existing shared check icon and exposed as selected accessibility state. Help search results use the same widget. Kept one Draft / Refinement row. Removed verified Type/Window aliases for Character, Paragraph, Glyphs and their two style panels, duplicate Local AI setup links, the identical project website/GitHub link, and the duplicate New Window row. Working IDs remain invokable. Active editor-profile overrides, including overrides assigned to hidden aliases, appear on the surviving row; Library bindings remain scoped to Library. Distinct Color/Swatches, Properties/Adjustments, Navigator/Info/Histogram, Brushes/Brush Settings and the two subject-selection implementations are preserved. Existing unified Export As behavior and legacy export IDs are retained. |
| U019 | Chose the task's allowed All Tools restriction. Smaller built-in Pixel sets no longer gain shared geometric/type/crop extras. All Tools exposes all shared additions; explicit saved custom sets retain their tools, ordering and identities. A shortcut-selected extra remains visible. All five presets of both personas retain their tools and real flyouts. Saved custom preferences round-trip with both Pencil and VectorPencil, ArtText and VectorCrop. `panels.rs::visible_sections`; engine toolset data/schema/keymaps unchanged. |
| U022 | Removed empty AI Integrations navigation and its visible menu row. Local AI remains under Edit › Preferences. The legacy `edit.preferences.integrations` command routes to the existing `li.localAi` setup; reopening an old Integrations section falls back to General. No setup screen added. `prefs_ui.rs::invoke`, `open_preferences`, `prefs_body`. |

The only shared-module addition is the reusable `widgets::{menu_row_label, menu_row_button, menu_row_check}`, documented in `docs/DESIGN-SYSTEM.md`. It uses themed spacing, the active font, interaction colours and the existing icon; no one-off menu check control or new asset. `pending_vector.rs` only appends the English Menu key, consistent with the translation pause.

## Alias review

The five Type/Window panel aliases share `view_cmds::panel_tab` or `type_panels_ui::menu`, including the same toggle state. Tests invoke each old ID and then its surviving Window ID, verify identical state and no document journal change, and verify custom alias shortcut labels. `ai_ui.rs::menu` routes both Local AI IDs to `open_local_ai`; the old Integrations route now joins that existing setup. `links::url_for` maps both website and GitHub commands to the same project URL; both IDs remain live. The catalogue's New Window for Document row remains the visible Arrange entry, while the old UI ID stays available for shortcuts and automation. Distinct tab choices and operations are not inferred to be aliases merely from similar names. For example, Object › Outline Stroke can use an independent node selection, so it remains distinct from the layer-selection Geometry route.

## Verification

All Cargo commands sourced `~/.local/share/local-image-dev/claude-env.sh`, used the same worktree `target/`, offline mode and four build jobs. Confirmed Cargo and rustc **1.98.1**. One Cargo build lane at a time.

| Check | Result | Log |
| --- | --- | --- |
| Final focused `cargo test --offline -p photocraft-ui-egui menu --lib` | 119 passed, 0 failed, 2 ignored | `target/menus-final-focused.log` |
| Existing long-menu/navigation regressions | 16 passed, 0 failed | `target/menus-navigation.log` |
| `cargo clippy --offline -p photocraft-ui-egui --all-targets -- -D warnings` | Passed | `target/menus-clippy.log` |
| `cargo fmt -p photocraft-ui-egui -- --check` | Passed | `target/menus-format.log` |
| Final full `cargo test --offline -p photocraft-ui-egui` | **1,699 passed, 0 failed, 69 ignored** across the unit/integration binaries and doctests | `target/menus-full-tests.log` |
| Explicit RTX screenshot test, `--ignored --exact --nocapture --test-threads=1` | **1 passed**, 14 PNGs | `target/menus-screenshots.log` |

Full-suite breakdown: unit tests 1,553 passed / 52 ignored; appearance 11 passed; tool contracts 1 passed; GPU integration 89 passed / 16 ignored; general integration 14 passed; layout 8 passed; path text 5 passed / 1 ignored; pen 8 passed; snapping 10 passed; doctests 0. The full run set `LOCAL_IMAGE_REQUIRE_GPU=1 LC_REQUIRE_GPU=1 WGPU_BACKEND=vulkan`. Of the 69 opt-in tests, this task's screenshot test was separately executed successfully; the other 68 are existing unrelated opt-in hardware, performance or integration gates and were left for their owners/coordinator. The full suite was first stopped by the two changed-label/menu-list assertions described below; that failure log was preserved, the related fixtures were corrected, and the final full run passed.

The new test module contains 15 ordinary regression tests plus one explicitly run GPU screenshot test. Related existing tests retain their original behavior checks: the Help-link assertion follows the deduplicated list, and the Tab override test still verifies F8, scrolling, pointer hit ownership and panel restoration, using the clean accessible label instead of an embedded check glyph. Development failure logs are retained under `target/menus-*-failure.log` and `target/menus-full-stale-fixtures.log`; the final run supersedes those fixture failures.

## Screenshots and physical GPU evidence

`menus::conventions_tests::menus_ux_rtx5090_screenshots` requires a discrete adapter named RTX 5090 and explicitly selects Vulkan. Log: `target/menus-screenshots.log`. Adapter receipt: NVIDIA GeForce RTX 5090, NVIDIA driver 615.71.09, PCI 0000:01:00.0, Vulkan. No software-adapter fallback.

Generated and visually inspected **14 PNGs** under [target/menus-ux](../../target/menus-ux/):

- Dark and light menu bars at 760×480 and 1440×900.
- Dark and light compact overflow menus at 760×480.
- Dark and light long Layer menus at both sizes.
- Dark and light Window menus at both sizes; the 1440-wide images show the aligned checked/unchecked gutters for Generate, Draft / Refinement, Model Browser and neighboring rows.

The pictures confirm readable light/dark text, Help last, Geometry directly under Combine Shapes, separate check gutters, clear module/workspace/caption controls and bounded scrolling menus. These are real production UI functions rendered offscreen, without opening or disturbing the owner's desktop app. No interactive-speed or model-performance claim is made.

## Sources followed

- [Current design system](../DESIGN-SYSTEM.md), especially shared widgets, theme metrics, accessibility, scrolling and screenshot verification; [inherited PhotoCraft UI design](../upstream/photocraft/ui-design.md), Menus and Title bar sections.
- Existing `menu_nav.rs`, `menu_nav_tests.rs`, `shortcuts.rs::canvas_keyboard_available`, `shortcut_dispatch.rs::binding_strings`, `view_cmds.rs::panel_tab`, and the current toolset/persona flyout implementation. The existing navigation/ownership path was reused rather than replaced.
- The installed **egui 0.36.2** primary source in the Cargo registry: `src/containers/menu.rs`, popup APIs, `src/widgets/button.rs`, `src/data/output.rs::WidgetInfo::selected`, and epaint `LayoutJob`/leading-space behavior. The compact wrapper is a local composition of that existing menu tree, not a new navigation framework.
- The task's checkpoint status, remaining UX audit and interrupted worker patch/manifests. Worker changes to unrelated error presentation, vector handlers and other feature bodies were not adopted.

## Remaining scope

No unfinished item in this Linux menu task. Unrelated coordinator release, native desktop, other-platform and opt-in GPU/model performance gates remain outside this change. The pre-existing task/status documents were preserved. The full UI suite regenerates `docs/design/vector-tool-icons.png`; its incidental output is restored to the original tracked content because asset regeneration is outside this file ownership.
