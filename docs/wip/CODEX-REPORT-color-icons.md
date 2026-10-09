# Glossy Duotone tool icons

Continued on 2026-10-09 from `1a8dd802` ("Checkpoint after the machine crash"). The coordinator subsequently checkpointed this continuation as `f8502f4c` and merged the working branch as `14a35c90`. The existing implementation was retained and reviewed. Final report and contact-sheet updates remain uncommitted; this session made no commits.

## Artwork and coverage

- 93 original hand-authored SVGs in `assets/icons/color/`, with 24 × 24 viewBoxes, family gradients, dark outlines, and restrained highlights. Colour families cover selection, paint, retouch, transform/crop, type/pen, sampling, Liquify, measurement, and navigation.
- All 53 Compositing tools and their grouped members, all 15 Liquify variants (11 strip tools and four global actions), Edit Toolbar, and Quick Mask have colour icons.
- Library/Develop covers the tool strip, retouch modes, all 13 mask-shape variants, crop helpers, and point-colour sampling. It uses string tool IDs and `RightPanel`, rather than a separate Tool enum; exhaustive matches cover its panel and mask enums.
- Formerly shared glyphs have distinct artwork, including Blur/Smooth, Smudge/Forward Warp, Healing/Spot Healing, and Type/Vertical Type. This continuation replaced the near-identical Luminance Range rectangle with a sun silhouette to distinguish it from Linear Mask.
- `docs/design/tool-icons.png` is the complete contact sheet: every icon at actual 24 and 48 px on `#1f2023` and `#f2f2f4`, rendered by resvg. Regenerate offline with `CARGO_NET_OFFLINE=true cargo +1.98.1 xtask tool-icons`.
- The regenerated contact sheet was visually reviewed, including the distinct Luminance Range silhouette. The original `target/icon-ref/` boards and NOTES were unavailable in this worktree and the original repository after the earlier interruptions; the established artwork and palette were preserved.

## Rendering and preferences

- Separate embedded colour data and a shared pure-Rust usvg/resvg renderer preserve SVG colours without tinting. The cache belongs to each egui context and keys on name, logical size, display scale, and disabled state. Disabled tools are grayscale with 45% alpha.
- Existing selected and hover backgrounds remain visible. Generic UI controls keep their existing monochrome rendering. Toolbar drawing changes remain localized to tool helpers.
- Both applications expose Tool icons: Colour (default) / Monochrome in Interface preferences. Serde defaults preserve loading of older settings; persistence, resetting, and switching back are tested.
- New preference labels are present in all 12 Compositing and four Develop translation catalogs.
- `icons::tool_icon(ui, name, size)` is available for toolbar and rich-tooltip icon slots; Develop returns `None` when callers should use their existing vector fallback.
- Develop reuses the already-present workspace `egui_extras` SVG renderer. The checkpoint adds only that existing package to Develop's dependency list; this continuation adds no dependencies, unsafe code, C/C++, or `-sys` packages.

## Attribution and tooling

- Both the SVG directory and contact sheet are registered as original Local Image contributor work, GPL-3.0-or-later, added 2026-10-09, with the requested credit for style direction explored using OpenAI image generation.
- Curated attributions and the generated in-app attribution data include the icon family and contact sheet.
- The checkpoint added the previously documented but unimplemented `cargo xtask assets` audit, including inherited relocated asset paths and licence-file checks. Existing development screenshots/example illustrations and ComfyUI fixture thumbnails also have notices, allowing the complete inventory to pass.
- `cargo xtask tool-icons` runs the small resvg contact-sheet test without opening windows. Both new commands are now documented in xtask's source usage guide.

## Validation

Fresh validation was performed after the coordinator's merge and cache reseed, offline with Rust 1.98.1, `CARGO_BUILD_JOBS=3`, one cargo process at a time, and `RUST_TEST_THREADS=1`. Per the coordinator's latest instruction, runtime tests were restricted to icon/preferences, the i18n literal check, and the contact-sheet renderer; full suites remain with the coordinator.

| Check | Result |
|---|---|
| `cargo check --offline` for photocraft-ui-egui, lightcraft-ui-egui, photocraft-engine, and xtask | Passed |
| `tool_icon` filter: Compositing | 9 passed (four icon/UI tests and five supporting CPU painter tests) |
| `tool_icon` filter: Library/Develop | 4 passed |
| `tool_icon` filter: preference engine | 1 passed |
| `icons::tests` filter: Compositing | 4 passed (SVG rendering, disabled state, cache, legacy monochrome coverage) |
| `icons::tests` filter: Library/Develop | 3 passed (SVG rendering, disabled state, cache, vector glyph regression) |
| `every_tl_literal_is_translated` | 1 passed |
| `write_tool_icon_contact_sheet -- --ignored --nocapture` | 1 passed; all 93 icons rendered at 24/48 px on dark/light |
| Asset audit | Passed: 333 assets attributed; referenced licence files exist |
| Attribution regeneration | Passed: 690 Rust crates, nine Assets entries; generated JSON matches the merge |
| Clippy, all targets, all four task crates, `-D warnings` | Passed |

Total: **23 targeted tests passed, zero failed**. The contact-sheet test is intentionally ignored during normal test runs because it writes a repository artifact; it was run explicitly and passed.

Test commands used the same three-package selection to reuse dependency-feature caches:

```sh
cargo +1.98.1 test --offline --lib \
  -p photocraft-ui-egui -p lightcraft-ui-egui -p photocraft-engine <filter>
```

Filters: `tool_icon`, `icons::tests`, `every_tl_literal_is_translated`, and `write_tool_icon_contact_sheet` (the last adds `-- --ignored --nocapture`). The required audits used `cargo +1.98.1 run -q --offline -p xtask -- assets` and `-- attributions`. Clippy uses `CARGO_TARGET_DIR=target/clippy`; its cache was populated from the warm worktree cache before linting.

The checks cover every SVG at 20/24/48 px, nonempty renders, grayscale/alpha for disabled tools, cache identities across size/scale/state, exhaustive tool coverage, unique mappings, unused/unembedded assets, real preference clicks, CPU-rendered colour saturation changes at both display scales, persistence, disabled clicks, and Develop retouch/mask selection clicks. The continuation's tests also verify rendered silhouette differences independently of colour and real Liquify strip/global-image clicks in both modes.

GPU tests added: none. The change concerns CPU SVG rasterization and existing egui texture uploads; no GPU algorithm or shader changes. All UI tests are headless; no desktop application was opened or stopped.

Earlier attempts were interrupted by gate/process termination and the machine-wide OOM event; the results above come from the final merged branch. No final test was killed by the memory watchdog. Short-lived xtask/contact-sheet commands emitted external watchdog accounting warnings but returned success and completed their checks; the build also reported the existing unavailable craft-fonts warning. These environment messages did not produce Rust diagnostics or failed tests.

The checkpoint's narrowly scoped mock-server test skip for denied local sockets remains in place; its unrelated AI flows were outside this final targeted run. No icon-task work remains pending.
