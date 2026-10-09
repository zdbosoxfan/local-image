# Glossy Duotone tool icons

Continued on 2026-10-09 from `1a8dd802` ("Checkpoint after the machine crash"). The checkpoint's implementation was retained and reviewed; this continuation's changes remain uncommitted.

## Artwork and coverage

- 93 original hand-authored SVGs in `assets/icons/color/`, with 24 × 24 viewBoxes, family gradients, dark outlines, and restrained highlights. Colour families cover selection, paint, retouch, transform/crop, type/pen, sampling, Liquify, measurement, and navigation.
- All 53 Compositing tools and their grouped members, all 15 Liquify variants (11 strip tools and four global actions), Edit Toolbar, and Quick Mask have colour icons.
- Library/Develop covers the tool strip, retouch modes, all 13 mask-shape variants, crop helpers, and point-colour sampling. It uses string tool IDs and `RightPanel`, rather than a separate Tool enum; exhaustive matches cover its panel and mask enums.
- Formerly shared glyphs have distinct artwork, including Blur/Smooth, Smudge/Forward Warp, Healing/Spot Healing, and Type/Vertical Type. This continuation replaced the near-identical Luminance Range rectangle with a sun silhouette to distinguish it from Linear Mask.
- `docs/design/tool-icons.png` is the complete contact sheet: every icon at actual 24 and 48 px on `#1f2023` and `#f2f2f4`, rendered by resvg. Regenerate offline with `CARGO_NET_OFFLINE=true cargo +1.98.1 xtask tool-icons`.
- The checkpoint's contact sheet was visually reviewed. The original `target/icon-ref/` boards and NOTES were lost with the build-directory removal/reboot and are absent from this worktree and the original repository; the established artwork and palette were preserved.

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

Fresh-session validation is running; this section will be replaced with results before handoff.

Tests cover every SVG at 20/24/48 px, nonempty renders, grayscale/alpha for disabled tools, cache identities across size/scale/state, exhaustive tool coverage, unique mappings, unused/unembedded assets, real preference clicks, CPU-rendered colour saturation changes at both display scales, persistence, disabled clicks, and Develop retouch/mask selection clicks. This continuation also adds silhouette comparisons independent of family colour, and real Liquify strip/global-image clicks in both icon modes.

GPU tests added: none. The change concerns CPU SVG rasterization and existing egui texture uploads; no GPU algorithm or shader changes.

The checkpoint's narrowly scoped mock-server test skip for denied local sockets remains in place; other setup failures still fail. All UI validation is headless, and no running desktop application was opened or stopped.
