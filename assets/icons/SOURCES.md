# Approved mixed tool glyphs

The owner chose the OLD approved Glossy Duotone set as the base. Of 94 tool IDs,
88 retain the approved colour and monochrome bytes exactly. Six reviewed replacements
use pinned upstream SVG geometry: Crop, Gradient, Brush, Eraser, Ellipse Marquee and
Rectangle Marquee. No other sourced glyph is selected or vendored.

The immutable colour snapshot is `docs/design/tool-icons-pre-sourced.json`;
the matching mono snapshot is `docs/design/tool-icons-approved-mono.json`.
Both originate at Local Image commit `2b3989c01b34a55ac9e908bdeed315cfb7ac2cb0` (GPL-3.0-or-later).

Run the offline Rust generator with `cargo +1.98.1 xtask redraw-tool-icons`,
then `cargo +1.98.1 xtask tool-icons`. Use `CARGO_NET_OFFLINE=true`,
`CARGO_BUILD_JOBS=3` and the worktree-local test-data environment documented in the report.

Source path data, coordinates, fill rules and transforms stay unchanged. Only paint
changes: opaque family and neutral-metal gradients, dark outlines, and highlights.
Crop uses its exact two open paths as broad material bands (3.5-unit dark border,
2.2-unit family face, 0.5-unit highlight), matching the approved crop material.
The five Krita glyphs use filled native paths. Their paint widths are recorded in
`source-manifest.json`. The same layers and opacities generate matching monochrome
silhouettes. Native viewBoxes are normalised to 24×24 with approved 1.5-unit padding.
This follows the existing resvg/usvg renderer and the SVG painting model:
https://www.w3.org/TR/SVG2/painting.html .

Curated and generated attribution list only the two imported sets and six files.
Krita source files explicitly embed CC-BY-SA-4.0 and creator metadata; originals
retain it. Adaptations use GPL-3.0-or-later through the Creative Commons one-way
GPLv3 compatibility designation: https://creativecommons.org/compatible-licenses/ .
Lucide Crop is ISC with MIT Feather portions; both notices are retained.
The existing generic icon inventory keeps its separate original notices.

## Imported files

| Tool | Official source | Pinned commit | Creator | Licence | SHA-256 |
|---|---|---|---|---|---|
| Gradient (`blend`) | https://github.com/KDE/krita: `krita/pics/tools/SVG/16/dark_krita_tool_gradient.svg` | `ab3502b67a5b07771d3127aeaf9cc522af5d626a` (v5.2.6) | Andrei Rudenko | CC-BY-SA-4.0 | `7b60bd12ca1d0fc03a1b175be8d1ce3f23f91b39ce965f9c9e2211e67b209bcd` |
| Brush (`brush`) | https://github.com/KDE/krita: `krita/pics/tools/SVG/16/dark_krita_tool_freehand.svg` | `ab3502b67a5b07771d3127aeaf9cc522af5d626a` (v5.2.6) | Timothée Giet | CC-BY-SA-4.0 | `bf1f26cd8e9a558ddf5e2f52a32c1b5a528add6b026e594a41f6c64e7e7515c8` |
| Ellipse Marquee (`circle-dashed`) | https://github.com/KDE/krita: `krita/pics/tools/SVG/16/dark_tool_elliptical_selection.svg` | `ab3502b67a5b07771d3127aeaf9cc522af5d626a` (v5.2.6) | Andrei Rudenko | CC-BY-SA-4.0 | `15f5c528f75d25e31307df5f1aaac7cceddac78130c62766ef8e1594faca70d3` |
| Crop (`crop`) | https://github.com/lucide-icons/lucide: `icons/crop.svg` | `f12b0de177fbc2a6795e99be065887e72b237123` (0.468.0) | Lucide contributors; Cole Bemis (Feather) | ISC AND MIT | `9229f7832445ee0caa15eecd6323397a3ebb5c9a124f923212605ad3529d64b8` |
| Eraser (`eraser`) | https://github.com/KDE/krita: `krita/pics/misc-dark/dark_draw-eraser.svg` | `ab3502b67a5b07771d3127aeaf9cc522af5d626a` (v5.2.6) | Timothée Giet | CC-BY-SA-4.0 | `a03a5be760feca566af0ca272bb2f7859bb5af99ff6dcea48c71efa83b8e0e62` |
| Rectangle Marquee (`square-dashed`) | https://github.com/KDE/krita: `krita/pics/tools/SVG/16/dark_tool_rect_selection.svg` | `ab3502b67a5b07771d3127aeaf9cc522af5d626a` (v5.2.6) | Andrei Rudenko | CC-BY-SA-4.0 | `ff8e238967fec4196eb03bc59b7e6aac7f7f8e2c9786f7ffba59a712e9caef3e` |

## Complete selection

| Tool ID | Selected artwork | Reason |
|---|---|---|
| `activity` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `arrow-left-right` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `background-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `blend` | krita/krita/pics/tools/SVG/16/dark_krita_tool_gradient.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `bloat` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `blur` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `brush` | krita/krita/pics/tools/SVG/16/dark_krita_tool_freehand.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `circle` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `circle-dashed` | krita/krita/pics/tools/SVG/16/dark_tool_elliptical_selection.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `circle-dot` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `color-range` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `crop` | lucide/icons/crop.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `custom-shape` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `depth-range` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `develop-edit` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `direct-select` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `dodge` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `edit-toolbar` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `eraser` | krita/krita/pics/misc-dark/dark_draw-eraser.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `eraser-background` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `eraser-magic` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `flame` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `forward-warp` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `freeze` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `freeze-all` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `freeze-lasso` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `guided-upright` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `hand` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `healing` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `history-brush` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `info` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `invert-freeze` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `keywords` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `landscape-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `lasso` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `lasso-magnetic` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `lasso-select` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `linear-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `luminance-range` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `message-square` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `more` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `mouse-pointer-2` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `move` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `object-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `object-selection` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `paint-bucket` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `palette` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `pen-tool` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `pencil` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `pentagon` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `people-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `perspective-crop` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `pipette` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `presets` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `prompt-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `pucker` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `push-left` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `quick-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `quick-selection` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `radial-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `reconstruct` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `reconstruct-all` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `rectangle-horizontal` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `red-eye` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `remove` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `ruler` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `scissors` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `shape-polygon` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `shape-triangle` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `sharpen` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `sky-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `slash` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `slice-knife` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `slice-select` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `smooth` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `smudge` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `sparkles` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `sponge` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `spot-healing` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `square-dashed` | krita/krita/pics/tools/SVG/16/dark_tool_rect_selection.svg | Coordinator accepted this sourced glyph; retain its geometry with filled Glossy Duotone paint. |
| `stamp` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `straighten` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `subject-mask` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `thaw` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `thaw-all` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `twirl-ccw` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `twirl-cw` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `targeted-adjust` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `type` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `versions` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `vertical-type` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `wand-sparkles` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
| `zoom-in` | Approved Local Image base | Owner-approved OLD artwork retained as the base; no sourced replacement is clearly better. |
