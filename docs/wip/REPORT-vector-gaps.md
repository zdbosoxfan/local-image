# Vector toolset gap audit — 2026-10-10

Branch `wip/codex-vector-gaps` (based on `wip/vector`). This is research and planning: no tools were implemented and nothing was built or tested.

## 1. Summary for the owner

The current plan (persona spec §7, architecture, packages 00–31) already covers the **core** of a real vector app. That includes the Pen, Node, Pencil, Corner and shape tools, booleans/Pathfinder, Shape Builder, Knife, Contour, Width, multiple fills and strokes, gradients, symbols, blends, repeats, envelopes, vector brushes, type on a path, crisp zoom, SVG/PDF and Image Trace.

What it was missing is mostly the **everyday Illustrator muscle-memory layer** around that core:

- the Group Selection, Curvature, Add/Delete/Convert Anchor, Rotate/Reflect/Scale/Shear and Free Transform tools;
- Select Same;
- clipping masks (Ctrl+7) and compound paths (Ctrl+8);
- Join/Average/Simplify and the rest of the Object › Path menu, and Expand Appearance;
- Outline view (Ctrl+Y) and isolation mode;
- an eyedropper that copies a whole appearance, Graphic Styles, global swatches and Recolor Artwork.

Beyond that are the bigger "pro" tools: Blob Brush and vector Eraser, the vector liquify tools and Puppet Warp, Live Paint / Affinity Vector Flood Fill, Gradient Mesh, Measure/Area and live path effects. Finally, the toolbar itself needs a vector layout with Illustrator/Affinity shortcuts, a fill/stroke swatch widget and icons for about 60 new tools. Some keys (Cmd+D, Cmd+J, Cmd+Y, Cmd+U, Cmd+B, Cmd+T) mean different things in Photoshop and Illustrator, so they must be **persona-specific**.

Every Essential and Important gap now has a work package (**15 new packages, 32–46**). Niche tools (symbol sprayers, graphs, perspective grid, Flare, Shaper, …) get a one-line plan in §6.

Inventory counts (one row per tool, panel or command family in §3):

| Priority | Covered | Partial | Missing | Total |
|---|---:|---:|---:|---:|
| Essential | 43 | 15 | 8 | 66 |
| Important | 17 | 9 | 23 | 49 |
| Niche | 1 | 6 | 20 | 27 |
| **All** | **61** | **30** | **51** | **142** |

## 2. Method and sources

- **Illustrator.** I read Adobe's official tool reference, [Tools in Illustrator][AI-T], in a browser. It is the page `tool-galleries.html` redirects to. I opened both hidden "additional tools" lists, which cover all six categories and every tool with its shortcut. I also read the official [default keyboard shortcuts][AI-K] page and the topic pages linked in §3 (from the Illustrator desktop help navigation). Plain HTTP fetching of helpx.adobe.com is blocked by its CDN (HTTP 403), so the pages were read in the built-in browser.
- **Affinity Designer 2.** I read the official help at affinity.help: the [index](https://affinity.help/designer2/en-US.lproj/index.html), every Design-tool and shape-tool page (each tool's shortcut text was extracted from its page), and the panel and topic pages cited per row. Some shortcuts in that help are drawn as key images rather than text; see §7. The Pixel and Export personas are excluded except for Slice.
- **VectorCraft** (ArtCraft's vector app). I read the pinned source `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` locally (`~/.local/share/local-image-dev/engine-sources/vectorcraft`, the same pin as the spec). Files read:
  - [`crates/tools/src/catalog.rs`][VC-cat], the full tool catalogue in Illustrator order, with shortcuts;
  - every tool module under `crates/tools/src/` (draw2, xform, distort, extra, meshblend, symbolism);
  - the engine command modules under `crates/engine/src/cmd/` (select, object, path, pathops, expand, style, swatch, recolor, live, buildcmds, effectcmd);
  - `crates/doc/src` and `docs/`.

  Links point at the same pin on GitHub.
- **Our plan.** I read `docs/specs/vector-persona-spec.md` (§3–§11 and the owner decisions) and `docs/specs/vector-architecture.md`, plus the scope and owned files of all 32 packages. I also grepped the current code: the 53-tool `Tool` enum, engine command IDs and `menu_catalog.rs`. A row counts as **COVERED** only when a package (or existing code) names the behaviour.
- **Coverage words.**
  - **COVERED**: planned or existing; the package is named.
  - **PARTIAL**: part exists; the note says what is missing.
  - **MISSING**: absent from code and plan.
- **Priority words.**
  - **Essential**: users expect it in any pro vector app; it is in both Illustrator's default toolbar/menus and Affinity, or a core Illustrator shortcut.
  - **Important**: a well-known pro feature in at least one of the two apps.
  - **Niche**: specialised or rarely used.

## 3. Full inventory

Sources: AI = Adobe Illustrator help, AD = Affinity Designer 2 help, VC = VectorCraft source at the pin. Shortcuts are Windows/Linux (Cmd on macOS). Package numbers 32 and above are new (§4); "niche" rows are planned in §6.

| # | Area | Item | Shortcut | Sources | Coverage | Priority | Package / plan |
|---:|---|---|---|---|---|---|---|
| 1 | Select | Selection (Illustrator) / Move (Affinity) | `V` | [AI-T] [AD-move] [VC-select] | **COVERED**: existing Move; persona-shared | Essential | existing + 01 |
| 2 | Select | Direct Selection (Illustrator) / Node (Affinity) | `A` | [AI-T] [AD-node] [VC-direct] | **COVERED** | Essential | 03 |
| 3 | Select | Group Selection tool (click again = parent group) | (none; Alt with Direct) | [AI-T] [AI-grp] [VC-cat] | **MISSING** | Essential | [32](vector-tasks/32-vector-selection.md) |
| 4 | Select | Select Same (fill, stroke, fill+stroke, stroke weight, opacity, blend mode, appearance, graphic style, shape, W/H/rotation, name) | Select > Same | [AI-sel] [AI-samefs] [AD-selattr] [VC-cselect] | **MISSING**: only pixel `select.similar` | Essential | [32](vector-tasks/32-vector-selection.md) |
| 5 | Select | Select Object (open paths, stray points, clipping masks, text, all on same layers, by type) | Select > Object | [AI-sel] [AD-selattr] [VC-cselect] | **MISSING** | Important | [32](vector-tasks/32-vector-selection.md) |
| 6 | Select | Magic Wand for objects (attribute tolerance) | `Y` | [AI-T] [AI-wand] [VC-wand] [VC-cwand] | **MISSING**: pixel wand only | Important | [32](vector-tasks/32-vector-selection.md) |
| 7 | Select | Lasso for objects and nodes | `Q` | [AI-T] [VC-wand] | **MISSING**: pixel lasso only; 03 has rectangle marquee | Important | [32](vector-tasks/32-vector-selection.md) |
| 8 | Select | Select Inverse / Next Object Above/Below / All on Artboard / Reselect for objects | `Ctrl+Alt+] etc.` | [AI-K] [VC-cselect] | **PARTIAL**: layer selection commands exist; not object-aware in Vector | Important | [32](vector-tasks/32-vector-selection.md) |
| 9 | Select | Artboard tool | `Shift+O` | [AI-T] [AD-ab] [VC-artboard] | **COVERED** | Essential | 16 |
| 10 | Navigate | Hand / View tool | `H, Space` | [AI-T] [AD-view] | **COVERED**: existing shared tool | Essential | existing |
| 11 | Navigate | Zoom tool | `Z` | [AI-T] [AD-zoom] | **COVERED**: existing shared tool | Essential | existing |
| 12 | Navigate | Rotate View tool | `Shift+H` | [AI-T] | **MISSING**: no tool in either persona | Niche | niche |
| 13 | Navigate | Print Tiling tool |  | [AI-T] | **MISSING** | Niche | niche |
| 14 | Draw | Pen (modes, rubber band, continue, close) | `P` | [AI-T] [AI-pen] [AD-pen] [VC-pen] | **COVERED** | Essential | 04 |
| 15 | Draw | Add Anchor Point tool | `+` | [AI-T] [AI-anchor] [VC-anchor] | **MISSING**: 03 only has double-click insert inside the Node tool | Essential | [33](vector-tasks/33-anchor-tools.md) |
| 16 | Draw | Delete Anchor Point tool (keeps shape) | `-` | [AI-T] [AI-anchor] [VC-anchor] | **PARTIAL**: 03 Delete key; no click tool | Essential | [33](vector-tasks/33-anchor-tools.md) |
| 17 | Draw | Anchor Point / Convert tool (click = corner, drag = smooth, break handle) | `Shift+C` | [AI-T] [AI-convert] [VC-anchor] | **PARTIAL**: 03 Sharp/Smooth/Smart buttons; no click/drag tool | Essential | [33](vector-tasks/33-anchor-tools.md) |
| 18 | Draw | Curvature tool | `Shift+~` | [AI-T] [AI-curv] [VC-curv] | **MISSING** | Essential | [33](vector-tasks/33-anchor-tools.md) |
| 19 | Draw | Pencil (fitting, stabiliser, Sculpt/reshape, extend, connect) | `N` | [AI-T] [AI-pencil] [AD-pencil] [VC-gesture] | **COVERED** | Essential | 05 |
| 20 | Draw | Smooth tool |  | [AI-T] [AI-smooth] [VC-gesture] | **COVERED** | Important | 10 |
| 21 | Draw | Path Eraser tool |  | [AI-T] [VC-gesture] | **COVERED** | Important | 10 |
| 22 | Draw | Join tool (scrub to join) |  | [AI-T] [VC-gesture] | **COVERED** | Important | 10 |
| 23 | Draw | Shaper tool + Shaper Group | `Shift+N` | [AI-T] [VC-extra] [VC-cshaper] [VC-docshaper] | **MISSING** | Niche | niche |
| 24 | Draw | Line Segment tool | `\` | [AI-T] [VC-family] | **COVERED** | Essential | 02 |
| 25 | Draw | Arc, Spiral tools |  | [AI-T] [VC-family] | **COVERED** | Important | 02 |
| 26 | Draw | Rectangular Grid, Polar Grid tools |  | [AI-T] [VC-family] | **COVERED** | Niche | 02 |
| 27 | Draw | Rectangle, Rounded Rectangle, Ellipse, Polygon, Star (live shapes) | `M, L` | [AI-T] [AI-live] [AD-rect] [VC-shape] | **COVERED** | Essential | 02 |
| 28 | Draw | Affinity shape set (Triangle, Diamond, Trapezoid, Double/Square Star, Arrow, Donut, Pie, Segment, Crescent, Cog, Cloud, Callouts, Tear, Heart) |  | [AD-shapes] | **COVERED** | Important | 02 |
| 29 | Draw | QR Code tool (Affinity) |  | [AD-qr] | **MISSING** | Niche | niche |
| 30 | Draw | Cat tool (Affinity novelty) |  | [AD-cat] | **MISSING** | Niche | niche (skip) |
| 31 | Draw | Flare tool |  | [AI-T] [VC-extra] | **MISSING** | Niche | niche |
| 32 | Draw | Paintbrush (Illustrator) / Vector Brush (Affinity) | `B` | [AI-T] [AI-brush] [AD-vbrush] [VC-symbolism] | **COVERED** | Essential | 17 |
| 33 | Draw | Blob Brush (draws filled, merging shapes) | `Shift+B` | [AI-T] [AI-blob] [VC-gesture] [VC-cdraw2] | **MISSING** | Important | [40](vector-tasks/40-blob-eraser.md) |
| 34 | Draw | Symbol Sprayer, Shifter, Scruncher, Sizer, Spinner, Stainer, Screener, Styler | `Shift+S` | [AI-T] [VC-symbolism] | **MISSING** | Niche | niche |
| 35 | Draw | Graph tools (Column, Stacked, Bar, Line, Area, Scatter, Pie, Radar) | `J` | [AI-T] [AI-graph] [VC-cgraph] [VC-docgraph] | **MISSING** | Niche | niche |
| 36 | Draw | Slice / Slice Selection for vector art | `Shift+K` | [AI-T] [AD-slice] [VC-slice] [VC-cslices] | **PARTIAL**: Pixel Slice tools exist; not in Vector set, no vector-scale export | Niche | niche |
| 37 | Draw | Perspective Grid / Perspective Selection | `Shift+P / Shift+V` | [AI-T] [AI-persp] [VC-persp] [VC-cperspgrid] | **MISSING** | Niche | niche |
| 38 | Draw | Corner tool / Live Corners widget | `C (Affinity)` | [AD-corner] [AI-live] [VC-corners] | **COVERED** | Essential | 06 |
| 39 | Draw | Contour tool (Affinity) / Offset Path | `O (Affinity)` | [AD-contour] [AI-off] | **COVERED** | Essential | 10 + V1 |
| 40 | Paint | Gradient tool (Illustrator) / Gradient-Fill tool (Affinity), incl. on strokes | `G` | [AI-T] [AI-grad] [AD-grad] [VC-gradtool] | **COVERED** | Essential | 08 + 00 (#1321) |
| 41 | Paint | Transparency tool (Affinity opacity gradient) | `Y (Affinity)` | [AD-transp] | **COVERED** | Important | 08 |
| 42 | Paint | Mesh tool / Create Gradient Mesh | `U` | [AI-T] [AI-mesh] [VC-meshblend] [VC-meshedit] [VC-clive] | **MISSING**: 20 has a mesh envelope, not a mesh paint | Important | [43](vector-tasks/43-gradient-mesh.md) |
| 43 | Paint | Shape Builder | `Shift+M (AI), S (AD)` | [AI-T] [AD-sb] [VC-builder] | **COVERED** | Essential | 10 |
| 44 | Paint | Live Paint Bucket / Live Paint Selection | `K / Shift+L` | [AI-T] [AI-lp] [AI-lpb] [VC-cbuild] | **MISSING** | Important | [42](vector-tasks/42-live-paint.md) |
| 45 | Paint | Vector Flood Fill tool (Affinity) | `R (Affinity)` | [AD-vff] | **MISSING** | Important | [42](vector-tasks/42-live-paint.md) |
| 46 | Paint | Freeform gradient |  | [AI-grad] [VC-freeform] | **MISSING** | Niche | niche |
| 47 | Paint | Fill/Stroke proxies (X toggle, Shift+X swap, D default, / none) | `X, Shift+X, D, /` | [AI-K] [AI-fs] [AD-swp] [AD-tools] [VC-cpaint] | **PARTIAL**: toolbar has foreground/background chips only | Essential | [46](vector-tasks/46-vector-toolbar.md) |
| 48 | Type | Type tool (point) / Artistic Text | `T` | [AI-T] [AI-type] [AD-art] [VC-text] | **COVERED**: existing TextShape::Point | Essential | existing + 12 |
| 49 | Type | Area Type / Frame Text |  | [AI-T] [AD-frame] | **COVERED**: existing TextShape::Box | Essential | existing + 12 (+30) |
| 50 | Type | Type on a Path / Text on a path |  | [AI-T] [AD-pathtext] [VC-pathtype] | **COVERED** | Essential | 12 |
| 51 | Type | Vertical Type |  | [AI-T] | **COVERED**: existing VerticalType | Important | existing |
| 52 | Type | Vertical Area Type / Vertical Type on a Path |  | [AI-T] | **PARTIAL** | Niche | niche |
| 53 | Type | Touch Type | `Shift+T` | [AI-T] | **MISSING** | Niche | niche |
| 54 | Type | Shape text (text inside any shape) |  | [AD-shapetext] | **PARTIAL**: spec: later any shape as a frame | Niche | niche |
| 55 | Type | Character / Paragraph panels and styles | `Ctrl+T / Alt+Ctrl+T` | [AI-char] [AI-para] [AD-char] [AD-para] | **COVERED**: existing type panels/styles | Essential | existing |
| 56 | Type | Create Outlines / Convert to curves for text | `Shift+Ctrl+O` | [AI-K] [AD-c2c] | **COVERED** | Essential | 12 |
| 57 | Modify | Rotate tool (reference point, Alt-click dialog, copy) | `R` | [AI-T] [AI-rot] [VC-xform] | **PARTIAL**: Move tool rotate handles + Edit > Transform; no reference-point tool | Essential | [34](vector-tasks/34-transform-tools.md) |
| 58 | Modify | Reflect tool (axis through reference point) | `O` | [AI-T] [AI-refl] [VC-xform] | **PARTIAL**: Flip H/V only | Essential | [34](vector-tasks/34-transform-tools.md) |
| 59 | Modify | Scale tool (reference point, scale strokes/corners option) | `S` | [AI-T] [AI-scale] [VC-xform] | **PARTIAL**: Move handles only | Essential | [34](vector-tasks/34-transform-tools.md) |
| 60 | Modify | Shear tool |  | [AI-T] [AI-shear] [VC-xform] | **PARTIAL**: 16 Transform panel shear field only | Important | [34](vector-tasks/34-transform-tools.md) |
| 61 | Modify | Reshape tool |  | [AI-T] [VC-extra] | **MISSING** | Important | [34](vector-tasks/34-transform-tools.md) |
| 62 | Modify | Free Transform (scale/rotate/shear, Free Distort, Perspective Distort) | `E` | [AI-T] [AI-distort] [VC-free] | **PARTIAL**: bbox handles exist; no distort/perspective of vector paths | Essential | [34](vector-tasks/34-transform-tools.md) |
| 63 | Modify | Transform Again (vector objects) | `Ctrl+D` | [AI-K] [VC-cobject] | **PARTIAL**: `edit.transform.again` exists for raster/layer transforms | Essential | [34](vector-tasks/34-transform-tools.md) |
| 64 | Modify | Transform Each | `Alt+Shift+Ctrl+D` | [AI-K] [AI-xf] [VC-cobject] | **MISSING** | Important | [34](vector-tasks/34-transform-tools.md) |
| 65 | Modify | Move / Duplicate data entry (Enter with Move tool) | `Enter` | [AD-xf] [AI-xf] [VC-cobject] | **MISSING** | Important | [34](vector-tasks/34-transform-tools.md) |
| 66 | Modify | Point Transform tool (Affinity) | `F (Affinity)` | [AD-pt] | **COVERED** | Important | 16 |
| 67 | Modify | Width tool / Stroke Width tool | `Shift+W (AI), W (AD)` | [AI-T] [AD-sw] [VC-width] | **COVERED** | Essential | 11 |
| 68 | Modify | Warp, Twirl, Pucker, Bloat, Scallop, Crystallize, Wrinkle (vector liquify) | `Shift+R` | [AI-T] [VC-liq] [VC-cdistort] | **MISSING**: raster Liquify only | Important | [41](vector-tasks/41-vector-distort-tools.md) |
| 69 | Modify | Puppet Warp on vector art |  | [AI-T] [AI-xf] [VC-pup] [VC-arap] [VC-docpuppet] | **MISSING**: raster Puppet Warp only | Important | [41](vector-tasks/41-vector-distort-tools.md) |
| 70 | Modify | Measure tool (distance/angle in document units) |  | [AI-T] [AI-meas] [AD-measure] [VC-meas] | **PARTIAL**: Pixel Ruler tool (px) | Important | [44](vector-tasks/44-measure-area.md) |
| 71 | Modify | Area tool (area, perimeter, holes) / measure area |  | [AD-area] [AI-area] | **MISSING** | Important | [44](vector-tasks/44-measure-area.md) |
| 72 | Modify | Eyedropper for appearance (sample, Alt-apply, options) | `I` | [AI-T] [AI-eye] [AI-K] [VC-eye] | **PARTIAL**: colour-only eyedropper | Essential | [38](vector-tasks/38-eyedropper-styles.md) |
| 73 | Modify | Style Picker (Affinity) |  | [AD-style] [AD-styles] | **MISSING** | Important | [38](vector-tasks/38-eyedropper-styles.md) |
| 74 | Modify | Blend tool / Object > Blend | `W, Alt+Ctrl+B` | [AI-T] [AI-blend] [AI-K] [VC-meshblend] | **COVERED** | Important | 18 |
| 75 | Modify | Eraser tool (vector, area erase) | `Shift+E` | [AI-T] [AI-erase] [VC-gesture] [VC-cdraw2] | **MISSING**: 10 has Path Eraser only | Important | [40](vector-tasks/40-blob-eraser.md) |
| 76 | Modify | Scissors tool | `C` | [AI-T] [AI-cut] [VC-anchor] | **COVERED** | Essential | 10 |
| 77 | Modify | Knife tool (freehand/straight) | `K (Affinity)` | [AI-T] [AI-cut] [AD-knife] [VC-cut] | **COVERED** | Essential | 10 |
| 78 | Modify | Line Cut / Mirror Cut / Rectangle Cut (VectorCraft) |  | [VC-cat] [VC-cut] | **PARTIAL**: Line Cut in 10 | Niche | niche |
| 79 | Modify | Dimension tool |  | [AI-T] [AI-dim] | **MISSING** | Niche | niche |
| 80 | Modify | Objects on Path tool |  | [AI-T] | **MISSING** | Niche | niche |
| 81 | Modify | Vector Crop tool (Affinity) |  | [AD-crop] | **COVERED** | Important | 16 |
| 82 | Modify | Place tool / File > Place | `Shift+Ctrl+P` | [AD-place] | **COVERED**: existing Place + 13 SVG place | Important | existing + 13 |
| 83 | Panel | Pathfinder: Unite/Minus Front/Intersect/Exclude + Divide/Trim/Merge/Crop/Outline/Minus Back |  | [AI-pf] [AD-join] | **COVERED** | Essential | V1 + 01 |
| 84 | Panel | Non-destructive compound shapes (Alt + boolean; Affinity compounds) |  | [AI-pf] [AD-compound] | **COVERED** | Important | V1 + 01 |
| 85 | Panel | Align / Distribute (key object, spacing, to artboard) |  | [AI-align] [AD-align] | **COVERED** | Essential | existing + 16 |
| 86 | Panel | Transform panel (X/Y/W/H/rotate/shear, reference point) |  | [AI-xfp] [AD-tp] | **COVERED** | Essential | 16 |
| 87 | Panel | Appearance panel: multiple fills/strokes, per-item opacity/blend |  | [AI-app] [AI-multi] [AD-app] [VC-docapp] | **COVERED** | Essential | 08 |
| 88 | Panel | Appearance effects: live Effect menu (Roughen, Zig Zag, Pucker & Bloat, Twist, Transform, Round Corners, Offset Path, Free Distort) |  | [AI-app] [VC-fxdistort] [VC-fxstylize] [VC-ceffect] | **PARTIAL**: pixel layer FX only; warp via 20 | Important | [45](vector-tasks/45-path-effects.md) |
| 89 | Panel | Stroke panel (weight, caps, joins, align, dashes, arrowheads, profiles) |  | [AI-arrow] [AD-stroke] | **COVERED** | Essential | 08 + 11 |
| 90 | Panel | Gradient panel (linear/radial, stops, midpoints, on stroke) | `Ctrl+F9` | [AI-grad] | **COVERED** | Essential | 08 + 00 |
| 91 | Panel | Transparency panel (opacity, blend, opacity masks) |  | [AI-tr] [AI-om] [AD-mask] | **COVERED**: layer opacity/blend/masks exist; per-item in 08 | Essential | existing + 08 |
| 92 | Panel | Swatches panel with document palette and swatch groups |  | [AI-sw] [AD-swp] [VC-cswatch] | **PARTIAL**: application swatches only | Essential | [39](vector-tasks/39-global-colours-recolor.md) |
| 93 | Panel | Global colours / global swatches (edit once, update everywhere), tints, spot |  | [AI-swabout] [AD-global] [VC-docswatches] | **MISSING** | Essential | [39](vector-tasks/39-global-colours-recolor.md) |
| 94 | Panel | Recolor Artwork / Edit Colors |  | [AI-rec] [VC-crecolor] [VC-colorrecolor] | **MISSING** | Important | [39](vector-tasks/39-global-colours-recolor.md) |
| 95 | Panel | Color Guide / harmony rules | `Shift+F3` | [AI-guide] [VC-harmony] | **MISSING** | Niche | niche |
| 96 | Panel | Graphic Styles (Illustrator) / Styles panel (Affinity) |  | [AI-gs] [AD-stylesp] [AD-styles] [VC-cstyle] | **MISSING**: Photoshop layer-style copy/paste only | Important | [38](vector-tasks/38-eyedropper-styles.md) |
| 97 | Panel | Symbols panel (create, place, edit master, detach) |  | [AI-sym] [AD-symbols] | **COVERED** | Essential | 15 |
| 98 | Panel | Brushes panel | `F5` | [AI-brush] [AD-brushes] | **COVERED** | Important | 17 |
| 99 | Panel | Layers panel (objects as layers, lock/hide, locate) | `F7` | [AI-layers] [AD-layers] | **COVERED**: existing | Essential | existing |
| 100 | Panel | Image Trace panel |  | [AI-trace] | **COVERED**: existing T1 + T2 | Essential | existing + 25-29 |
| 101 | Panel | Constraints panel (Affinity) |  | [AD-cons] | **COVERED** | Important | 16 |
| 102 | Panel | Snapping manager / Smart Guides | `Ctrl+U` | [AI-smart] [AD-snap] | **COVERED** | Essential | 07 |
| 103 | Panel | Isometric panel / axonometric grids (Affinity) |  | [AD-isop] | **MISSING** | Niche | niche |
| 104 | Panel | Assets panel (Affinity) |  | [AD-assets] | **MISSING** | Niche | niche |
| 105 | Panel | Patterns: create/edit pattern swatches |  | [AI-pat] | **PARTIAL**: 08 renders pattern paints; no pattern edit mode | Niche | niche |
| 106 | Object | Group / Ungroup | `Ctrl+G / Shift+Ctrl+G` | [AI-K] | **COVERED**: existing | Essential | existing |
| 107 | Object | Lock / Hide selection | `Ctrl+2 / Ctrl+3` | [AI-K] | **COVERED**: existing layer lock/hide | Essential | existing |
| 108 | Object | Arrange (bring forward/front, send backward/back) | `Ctrl+] / Ctrl+[` | [AI-K] | **COVERED**: existing | Essential | existing |
| 109 | Object | Clipping mask: Make / Release / Edit mask / Edit contents | `Ctrl+7 / Alt+Ctrl+7` | [AI-clip] [AI-K] [VC-cobject] [VC-docclip] | **PARTIAL**: Photoshop clip-to-below and vector masks exist; no object clip group | Essential | [35](vector-tasks/35-clip-compound.md) |
| 110 | Object | Clip by nesting: Move Inside / Move Outside (Affinity) |  | [AD-clip] | **PARTIAL** | Important | [35](vector-tasks/35-clip-compound.md) |
| 111 | Object | Draw Normal / Behind / Inside modes | `Shift+D` | [AI-modes] [AD-clip] | **MISSING** | Important | [35](vector-tasks/35-clip-compound.md) |
| 112 | Object | Compound path: Make / Release | `Ctrl+8 / Alt+Shift+Ctrl+8` | [AI-cmp] [AI-K] [VC-cobject] | **PARTIAL**: combine components inside one layer only | Essential | [35](vector-tasks/35-clip-compound.md) |
| 113 | Object | Fill rule (non-zero / even-odd) toggle |  | [AD-fillmode] [VC-cpath] | **PARTIAL** | Important | [35](vector-tasks/35-clip-compound.md) |
| 114 | Object | Join (paths or endpoints) | `Ctrl+J` | [AI-K] [VC-cpath] | **PARTIAL**: 03 joins nodes; V1 `path.join` engine only | Essential | [36](vector-tasks/36-object-path-commands.md) |
| 115 | Object | Average anchor points | `Alt+Ctrl+J` | [AI-avg] [AI-K] [VC-cpath] | **MISSING** | Important | [36](vector-tasks/36-object-path-commands.md) |
| 116 | Object | Outline Stroke / Expand Stroke |  | [AD-expand] [VC-cexpand] | **COVERED** | Essential | V1 + 01 |
| 117 | Object | Offset Path dialog |  | [AI-off] | **COVERED** | Essential | 10 + V1 |
| 118 | Object | Simplify (dialog with live preview and point count) |  | [AI-simp] [VC-cpathops] | **PARTIAL**: V1 `path.simplify` engine only | Important | [36](vector-tasks/36-object-path-commands.md) |
| 119 | Object | Add Anchor Points / Remove Anchor Points (menu) |  | [AI-anchor] [VC-cpathops] | **MISSING** | Important | [36](vector-tasks/36-object-path-commands.md) |
| 120 | Object | Clean Up (stray points, unpainted objects, empty text) |  | [AI-stray] [VC-cpathops] | **MISSING** | Important | [36](vector-tasks/36-object-path-commands.md) |
| 121 | Object | Divide Objects Below |  | [AI-div] [VC-cpathops] | **MISSING** | Niche | [36](vector-tasks/36-object-path-commands.md) (folded in) |
| 122 | Object | Split Into Grid |  | [AI-div] [VC-cpathops] | **MISSING** | Niche | [36](vector-tasks/36-object-path-commands.md) (folded in) |
| 123 | Object | Object > Expand (live shapes, gradients, strokes, blends, envelopes, symbols, text) as one command |  | [AI-exp] [VC-cexpand] | **PARTIAL**: each package expands its own feature; no unified command | Essential | [36](vector-tasks/36-object-path-commands.md) |
| 124 | Object | Expand Appearance |  | [AI-exp] [AI-app] [VC-cexpand] | **MISSING** | Essential | [36](vector-tasks/36-object-path-commands.md) |
| 125 | Object | Convert to Curves (Affinity) | `Ctrl+Enter (AD)` | [AD-c2c] | **COVERED** | Essential | 02 |
| 126 | Object | Paste in Front / Paste in Back | `Ctrl+F / Ctrl+B` | [AI-K] | **PARTIAL**: Paste in Place exists | Important | [36](vector-tasks/36-object-path-commands.md) |
| 127 | Object | Envelope Distort / Affinity warp groups | `Alt+Ctrl+W/M/C` | [AI-env] [AD-warp] | **COVERED** | Important | 20 |
| 128 | Object | Repeat (radial, grid, mirror) |  | [AI-rep] | **COVERED** | Important | 19 |
| 129 | Object | Live Paint: Make / Merge / Expand / Gap options | `Alt+Ctrl+X` | [AI-lp] [AI-K] [VC-cbuild] | **MISSING** | Important | [42](vector-tasks/42-live-paint.md) |
| 130 | Object | Opacity mask from an object (Make Opacity Mask) |  | [AI-om] | **PARTIAL**: layer masks exist; no 'selected object becomes mask' | Niche | niche |
| 131 | View | Outline view (wireframe) | `Ctrl+Y` | [AI-K] [AI-view] [AD-viewmodes] [VC-uicanvas] | **MISSING** | Essential | [37](vector-tasks/37-view-modes-isolation.md) |
| 132 | View | X-ray view (Affinity: outlines plus faded fills) |  | [AD-viewmodes] | **MISSING** | Important | [37](vector-tasks/37-view-modes-isolation.md) |
| 133 | View | Pixel Preview / Pixel and Retina view modes | `Alt+Ctrl+Y` | [AI-K] [AD-viewmodes] | **PARTIAL**: today's view is pixel; 09 adds crisp vector view; no toggle | Important | [37](vector-tasks/37-view-modes-isolation.md) |
| 134 | View | Per-layer outline (Ctrl-click eye) |  | [AI-K] | **MISSING** | Niche | [37](vector-tasks/37-view-modes-isolation.md) (folded in) |
| 135 | View | Hairline view (Affinity CAD) |  | [AD-viewmodes] | **MISSING** | Niche | niche |
| 136 | View | Isolation mode (double-click group, Esc exits, breadcrumb) | `dbl-click / Esc` | [AI-iso] [AI-K] [AD-iso] [VC-cobject] | **MISSING**: `select.isolateLayers` is only a Layers-panel filter | Essential | [37](vector-tasks/37-view-modes-isolation.md) |
| 137 | View | Crisp vector zoom (resolution-independent view) |  | [AD-viewmodes] | **COVERED** | Essential | 09 |
| 138 | View | Persona switcher and tool sets |  | [AD-persona] | **COVERED** | Essential | 01 |
| 139 | View | Vector toolbar flyouts, Illustrator/Affinity keymaps, icons for new tools |  | [AI-T] [AD-tools] [VC-cat] | **PARTIAL**: 01 owns registry; no layout/keymap/icon set for the new tools | Essential | [46](vector-tasks/46-vector-toolbar.md) |
| 140 | Export | SVG import / export |  | [AI-screens] | **COVERED** | Essential | 13, 14 |
| 141 | Export | PDF import / export |  | [AI-screens] | **COVERED** | Essential | 21, 22 |
| 142 | Export | Export for Screens / per-artboard, @2x assets | `Alt+Ctrl+E` | [AI-screens] | **COVERED**: per-artboard export in 16; scale export through 09 | Important | 16 + 09 |

### Verification of the gaps the owner flagged

| Suspected gap | Verdict | Package |
|---|---|---|
| Curvature tool | Missing | 33 |
| Add/Delete Anchor Point tools | Missing / partial (Node-tool double-click and Delete only) | 33 |
| Group Selection | Missing | 32 |
| Select Same/Similar | Missing (only pixel Select › Similar) | 32 |
| Node Lasso | Missing (Node tool has a rectangle marquee only) | 32 |
| Vector Eraser | Missing (Path Eraser is in 10, area Eraser is not) | 40 |
| Blob Brush | Missing | 40 |
| Rotate/Reflect/Scale/Shear tools | Partial (Move handles, Flip, Transform panel) | 34 |
| Free Transform | Partial (no Free/Perspective Distort on paths) | 34 |
| Puppet Warp | Missing for vectors (raster only) | 41 |
| Reshape | Missing | 34 |
| Warp, Twirl, Pucker, Bloat, Scallop, Crystallize, Wrinkle | Missing for vectors (raster Liquify only) | 41 |
| Live Paint Bucket / Vector Flood Fill | Missing | 42 |
| Gradient Mesh | Missing (20's mesh is an envelope, not a paint) | 43 |
| Perspective Grid | Missing | niche (§6) |
| Eyedropper for appearance | Partial (colour only) | 38 |
| Graphic Styles | Missing | 38 |
| Global swatches | Missing | 39 |
| Recolor Artwork | Missing | 39 |
| Clipping masks / compound paths | Partial (Photoshop clip-to-below and in-layer combine only) | 35 |
| Symbol Sprayer family | Missing | niche (§6) |
| Graph tools | Missing | niche (§6) |
| Flare | Missing | niche (§6) |
| Shaper | Missing | niche (§6) |
| Measure | Partial (pixel Ruler) | 44 |
| Slice for vectors | Partial (pixel Slice tools) | niche (§6) |
| Outline view (Ctrl+Y) | Missing | 37 |
| Isolation mode | Missing (`select.isolateLayers` is only a Layers filter) | 37 |

## 4. New work packages

All 15 mirror packages 00–31 exactly: the same sections and the same verbatim gesture, test, run, attribution and Rules text, copied programmatically from 03. Each owns new, non-overlapping files under `vector_packages/`, `pc-vecfx/`, `pc-vector/` or `pc-doc/src/vector_design/`, plus `integration/NN.md` and `i18n/NN.tsv`. Each asks 00 for a new compiled command slot.

| Package | Priority | Scope (one line) | Key dependencies |
|---|---|---|---|
| [32-vector-selection](vector-tasks/32-vector-selection.md) | Essential | Group Selection, Select › Same / Select › Object, vector Magic Wand, object/node Lasso, vector Select Inverse/Next/All on Artboard | 08 reader, 37 scope, 38 style link |
| [33-anchor-tools](vector-tasks/33-anchor-tools.md) | Essential | Add/Delete/Convert Anchor Point tools and the Curvature tool, plus Pen hover add/delete helpers | 03, V1 |
| [34-transform-tools](vector-tasks/34-transform-tools.md) | Essential | Rotate/Reflect/Scale/Shear with reference point and dialogs, Reshape, Free Transform with Free/Perspective Distort, Transform Again/Each, Move dialog | 02, 11/17, 16, 07 |
| [35-clip-compound](vector-tasks/35-clip-compound.md) | Essential | Make/Release/Edit clipping mask (Ctrl+7), Move Inside/Outside, Draw Normal/Behind/Inside, Make/Release compound path (Ctrl+8), fill rule | V1, 08, 12, 37 |
| [36-object-path-commands](vector-tasks/36-object-path-commands.md) | Essential | Join, Average, Simplify dialog, Add/Remove Anchor Points, Clean Up, Divide Objects Below, Split Into Grid, unified Expand / Expand Appearance, Paste in Front/Back | V1, every feature's expand API |
| [37-view-modes-isolation](vector-tasks/37-view-modes-isolation.md) | Essential | Outline (Ctrl+Y), X-ray, Pixel Preview, per-layer outline, isolation mode with breadcrumb and insertion target | 09, 08 |
| [38-eyedropper-styles](vector-tasks/38-eyedropper-styles.md) | Essential / Important | Appearance Eyedropper with options, Affinity Style Picker, Graphic Styles panel with linked styles | 08, 00 data |
| [39-global-colours-recolor](vector-tasks/39-global-colours-recolor.md) | Essential / Important | Document palette, global colours with tints/spot, ASE, Recolor Artwork dialog | 08, 15, 00 data |
| [40-blob-eraser](vector-tasks/40-blob-eraser.md) | Important | Blob Brush (merging filled strokes) and vector area Eraser | V1, 05 |
| [41-vector-distort-tools](vector-tasks/41-vector-distort-tools.md) | Important | Warp/Twirl/Pucker/Bloat/Scallop/Crystallize/Wrinkle on paths and vector Puppet Warp (ARAP) | V1 |
| [42-live-paint](vector-tasks/42-live-paint.md) | Important | Affinity Vector Flood Fill and Illustrator Live Paint (bucket, selection, gaps, expand) | V1/V1-Q, 09, 00 LiveGroup |
| [43-gradient-mesh](vector-tasks/43-gradient-mesh.md) | Important | Create Gradient Mesh, Mesh tool, Coons/tensor patch paint, export fallbacks | 08, 09, 21 |
| [44-measure-area](vector-tasks/44-measure-area.md) | Important | Vector Measure (units, snapping) and Affinity Area tool, optional drawing scale | 07, 16 |
| [45-path-effects](vector-tasks/45-path-effects.md) | Important | Live Effect › Distort & Transform and Stylize subset (Roughen, Zig Zag, Twist, Pucker & Bloat, Transform, Round Corners, Offset, Free Distort, Convert to Shape) | 08 projection, 06, 36 |
| [46-vector-toolbar](vector-tasks/46-vector-toolbar.md) | Essential | Vector flyout layout, Illustrator/Affinity keymaps, persona-scoped menu accelerators, fill/stroke proxy, icons for new tools | 01, icons-standard |

**What package 00 and the README need** (I did not edit them, because they belong to the running jobs):

- Add rows 32–46 to the README schedule.
- Add compiled slots: `selection`, `anchor_tools`, `transform_tools`, `clip_compound`, `path_commands`, `view_modes`, `styles`, `colours`, `blob_eraser`, `distort_tools`, `live_paint`, `mesh`, `measure`, `path_effects`, `toolbar`.
- Accept these additive data requests, all with serde defaults and skip-if-empty:
  - `Document.graphic_styles` and a layer style link (38);
  - `Document.swatches` and swatch references on solid paints/stops (39);
  - `LiveGroup::LivePaint` (42; touches 18–20's exhaustive matches);
  - a mesh paint variant (43, through 08);
  - `VectorData.effects` (45);
  - an optional drawing scale (44).

  Coordinate these with 15's FORMAT_VERSION 2 change so there is one native-format bump.

**Suggested order:**

1. 46 and 33 (toolbar and pen family, because users touch them first);
2. 32, 34, 35, 36 and 37 (selection, transforms and structure);
3. 38 and 39 (styles and colour);
4. 40–45 (pro tools).

## 5. Vector persona toolbar proposal

This proposal follows Illustrator's Advanced toolbar order, which VectorCraft's catalogue reproduces, folded into Affinity-style compact groups. Each row is one toolbar slot; the first tool is the default visible one, and the others sit in its press-and-hold flyout.

**Keymaps.** There are two Vector presets:
- **Illustrator** is the default, because it has the largest muscle-memory base and VectorCraft uses it.
- **Affinity** is the alternative.

The Pixel persona keeps its Photoshop keys.

**Icons.** All icons follow `docs/wip/CODEX-REPORT-icons-standard.md`: Glossy Duotone faces with the tool family's existing gradient, `#1b1c20` outlines, highlights at most 28%, a 24 px grid, and a matching monochrome version. "Reuse" means the existing asset ID; "new" means an original drawing in package 46.

| Slot | Default tool → flyout | Illustrator key | Affinity key | Icon metaphor |
|---|---|---|---|---|
| 1 Move | **Move** | V | V | In Vector, alias Move to the solid pointer (`mouse-pointer-2`), as both apps do; Pixel keeps the four-way `move` |
| 2 Node | **Node / Direct Selection** → Group Selection → Point Transform | A / – / – | A / – / F | reuse `direct-select` hollow arrow; new hollow arrow + plus badge; new hollow arrow with node square and rotation arc |
| 3 Select by | **Magic Wand** → Lasso (objects/nodes) | Y / Q | – / – | reuse `wand-sparkles`; new rope loop with node squares on the rope (distinct from pixel `lasso`) |
| 4 Artboard | **Artboard** → Vector Crop → Slice → Slice Select | Shift+O / – / Shift+K / – | – | new frame with corner ticks and a name tab; new crop corners over a Bézier shape; reuse `slice-knife`, `slice-select` |
| 5 Pen | **Pen** → Curvature → Add Anchor → Delete Anchor → Anchor Point | P / Shift+~ / + / − / Shift+C | P / – / – / – / – | reuse `pen-tool` nib; new nib drawing a smooth curve through three dots; new nib + plus; nib + minus; new open chevron with a handle line |
| 6 Pencil | **Pencil (vector)** → Smooth → Path Eraser → Join | N | N | new pencil drawing a curve with nodes (separate ID from raster `pencil`); new pencil over wavy-to-straight line; new eraser-tipped pencil over a gapped path; new two path ends bridged |
| 7 Brush | **Vector Brush / Paintbrush** → Blob Brush | B / Shift+B | B / – | new brush with tapering ribbon stroke; new brush with a filled blob |
| 8 Corner | **Corner** → Contour | – | C / O | new square corner rounded by an arc with a handle dot; new nested offset outlines |
| 9 Shapes | **Rectangle** → Rounded Rectangle → Ellipse → Polygon → Star → Triangle → Diamond → Trapezoid → Double Star → Square Star → Arrow → Donut → Pie → Segment → Crescent → Cog → Cloud → Callout Rect → Callout Ellipse → Tear → Heart | M / – / L | M cycles Rect/Rounded/Ellipse | reuse `rectangle-horizontal`, `circle`, `shape-polygon`, `shape-triangle`; new filled glyph per remaining shape (keep `custom-shape` for the Photoshop custom shape) |
| 10 Line | **Line Segment** → Arc → Spiral → Rectangular Grid → Polar Grid | \ | – | reuse `slash`; new arc, spiral, 3×3 grid, concentric-rings-with-spokes |
| 11 Type | **Artistic/Point Type** → Frame/Area Type → Type on a Path → Vertical Type | T | T | reuse `type`; new T in a dashed frame; new T on a curved baseline; reuse `vertical-type` |
| 12 Rotate | **Rotate** → Reflect | R / O | – | new curved arrow around a reference dot; new mirrored triangles across a dashed axis |
| 13 Scale | **Scale** → Shear → Reshape | S | – | new small and large squares with a diagonal arrow; new slanted parallelogram with arrows; new curve with a node pulled by an arrow |
| 14 Width | **Width** → Warp → Twirl → Pucker → Bloat → Scallop → Crystallize → Wrinkle | Shift+W / Shift+R | W / – | new stroke with diamond width point and outward arrows; reuse `forward-warp`, `twirl-cw`, `pucker`, `bloat`; new scalloped, spiked and wavy circles |
| 15 Free Transform | **Free Transform** → Puppet Warp | E | – | new dashed box with corner handles and a diagonal arrow; new figure with three pin heads |
| 16 Shape Builder | **Shape Builder** → Live Paint Bucket / Vector Flood Fill → Live Paint Selection | Shift+M / K / Shift+L | S / R / – | new overlapping circles with a plus cursor; new bucket over overlapping regions; new pointer over a highlighted region |
| 17 Eraser | **Eraser (vector)** → Scissors → Knife → Line Cut | Shift+E / C | – / – / K | new eraser removing part of a filled Bézier shape; new scissors cutting a path at an anchor (`scissors` stays AI Cutout); new blade crossing a shape; new straight dashed cut line |
| 18 Gradient | **Gradient / Fill** → Mesh → Transparency | G / U | G / – / Y | reuse `blend`; new square with a curved mesh grid; new gradient fading to checkerboard |
| 19 Eyedropper | **Eyedropper** → Style Picker → Measure → Area | I | I | reuse `pipette`; new dropper over a style swatch; reuse `ruler`; new hatched shape with a ruler corner |
| 20 Blend | **Blend** | W | – | new two shapes joined by intermediate steps |
| 21 Hand | **Hand** | H | H | reuse `hand` |
| 22 Zoom | **Zoom** | Z | Z | reuse `zoom-in` |
| Below | Fill/Stroke proxy (X, Shift+X, D, /, colour/gradient/none) and Draw Normal/Behind/Inside button (Shift+D) | | | new overlapping fill square and stroke ring; three small draw-mode glyphs |

**Built-in vector sets.** These extend spec §6 and are supplied as layout data to 01:
- **Essentials:** slots 1, 2, 5, 6, 8, 9, 11, 16, 18, 19, 21, 22, with the flyouts trimmed to Move, Node, Pen, Curvature, Pencil, Corner, the shapes, Artistic/Frame Text, Shape Builder, Fill, Transparency, Eyedropper, Hand and Zoom.
- **Illustration:** Essentials plus Vector Brush, Blob Brush, Width, the warp tools, Knife, Eraser, Blend, Contour and Mesh.
- **Layout & Type:** Move, Node, Artboard, Type flyout, shapes, Measure, Hand and Zoom.
- **Photographer (vector side):** unchanged from §6.

**Persona-scoped menu accelerators.** The menus are shared between personas, so these keys differ by persona:

| Key | Pixel (today, Photoshop) | Vector (Illustrator) |
|---|---|---|
| Cmd+D | Deselect | Transform Again |
| Cmd+J | Layer via Copy | Join |
| Cmd+Y | Proof Colors | Outline view |
| Cmd+U | Hue/Saturation | Smart Guides |
| Cmd+B | Color Balance | Paste in Back |
| Cmd+T | Free Transform | Character panel |
| Cmd+Shift+O | Open Folder | Create Outlines |
| Cmd+F, Cmd+2/3, Cmd+7/8 | (not in `menu_catalog.rs`) | Paste in Front, Lock/Hide, Clipping Mask, Compound Path |

The Pixel column was read from the current `menu_catalog.rs`. The Vector column comes from the [Illustrator shortcuts page][AI-K]. Package 46 owns this table and must check `shortcut_dispatch` for non-menu bindings.

## 6. Niche items: one-line plans

- **Symbol Sprayer family (8 tools):** later, on top of 15's instances plus 17's seeded scatter sampling; a "symbol set" is an ordinary group of instances.
- **Graph tools (9):** later, as a data-table dialog that generates plain grouped shapes with 02's generators; no live graph object.
- **Flare:** later, as a generator producing a group of radial-gradient shapes (08).
- **Shaper and Shaper Group:** later, using 26's shape recogniser and 02's generators; a Shaper Group is a V1 non-destructive compound.
- **Perspective Grid / Perspective Selection:** later; port VC `distort/perspective.rs` plane homographies onto 07 snapping, with 20's perspective envelope for existing art.
- **Touch Type; Vertical Area Type / Vertical Type on a Path; shape text:** follow-ups to 12/30 on pc-text per-glyph transforms and frames.
- **Rotate View tool:** a shared Pixel+Vector navigation tool on the existing canvas view transform.
- **Print Tiling:** skip; no print-tiling workflow in this app.
- **Line Cut variants (Mirror Cut, Rectangle Cut):** small extension of 10's cut tools.
- **Dimension tool:** later, annotation objects built on 44's measurements and 07 snapping.
- **Objects on Path:** later, on 12's arc-length layout or as a 19 repeat-along-path mode.
- **Freeform gradient:** an 08 follow-up porting VC `xform/freeform.rs`.
- **Color Guide / harmonies:** a 39 follow-up using VC `color/harmony.rs`.
- **Pattern edit mode:** a 08 follow-up reusing 37's isolation UI for pattern tiles.
- **Opacity mask from a selected object:** small 35 follow-up that converts an object to a layer mask.
- **Slice / Slice Selection in Vector:** share the existing Slice tools in the Vector registry (01) and export slices through 09's scale API.
- **Divide Objects Below, Split Into Grid, per-layer outline:** folded into 36/37 at no extra cost.
- **Hairline view, Isometric panel, Quick Grids, Assets panel (Affinity):** later extensions of 37, 07, 16 and 15 respectively.
- **QR Code tool:** a later 02 generator (pure-Rust encoder required; no new dependency without approval).
- **Cat tool:** skip.

## 7. Limits

- Shortcut images in Affinity's help could not be extracted as text. The Affinity keymap column uses only shortcuts printed as text on the cited pages (V, F, O, C, P, N, W, K, R, S, I, M, H, Z). It also uses A, G, T, B and Y from the tool-cycling/toolbar convention, and 46 must confirm those against the Affinity app's keyboard shortcut list before freezing the preset.
- Priority is a judgment from the two vendors' default toolbars and documentation, not a user study.
- Counts treat a tool family (for example the eight symbolism tools, or the Affinity extra shapes) as one row.

[AD-ab]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_artboard.html
[AD-align]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/align.html
[AD-app]: https://affinity.help/designer2/en-US.lproj/pages/Panels/appearancePanel.html
[AD-area]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_area.html
[AD-art]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_artText.html
[AD-assets]: https://affinity.help/designer2/en-US.lproj/pages/Panels/assetsPanel.html
[AD-brushes]: https://affinity.help/designer2/en-US.lproj/pages/Panels/brushesPanel.html
[AD-c2c]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/converttocurves.html
[AD-cat]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_cat.html
[AD-char]: https://affinity.help/designer2/en-US.lproj/pages/Panels/characterPanel.html
[AD-clip]: https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html
[AD-compound]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/compound.html
[AD-cons]: https://affinity.help/designer2/en-US.lproj/pages/Panels/constraintsPanel.html
[AD-contour]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_contour.html
[AD-corner]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_corner.html
[AD-crop]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_crop.html
[AD-expand]: https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/expandStroke.html
[AD-fillmode]: https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/fillMode.html
[AD-frame]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_frameText.html
[AD-global]: https://affinity.help/designer2/en-US.lproj/pages/Clr/globalClr.html
[AD-grad]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_gradient.html
[AD-iso]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/isolating.html
[AD-isop]: https://affinity.help/designer2/en-US.lproj/pages/Panels/isometricPanel.html
[AD-join]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/join.html
[AD-knife]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_knife.html
[AD-layers]: https://affinity.help/designer2/en-US.lproj/pages/Panels/layersPanel.html
[AD-mask]: https://affinity.help/designer2/en-US.lproj/pages/Layers/LayerMasks.html
[AD-measure]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_measure.html
[AD-move]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_move.html
[AD-node]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_node.html
[AD-para]: https://affinity.help/designer2/en-US.lproj/pages/Panels/paragraphPanel.html
[AD-pathtext]: https://affinity.help/designer2/en-US.lproj/pages/Text/pathText.html
[AD-pen]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pen.html
[AD-pencil]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pencil.html
[AD-persona]: https://affinity.help/designer2/en-US.lproj/pages/Introduction/about_Personas.html
[AD-place]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_placeimage.html
[AD-pt]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pointTransform.html
[AD-qr]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_qrCode.html
[AD-rect]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_rectangle.html
[AD-sb]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_shapeBuilder.html
[AD-selattr]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/selectByAttribute.html
[AD-shapes]: https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/about_geometricShapes.html
[AD-shapetext]: https://affinity.help/designer2/en-US.lproj/pages/Text/shapeText.html
[AD-slice]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_slice.html
[AD-snap]: https://affinity.help/designer2/en-US.lproj/pages/DesignAids/snapping.html
[AD-stroke]: https://affinity.help/designer2/en-US.lproj/pages/Panels/strokePanel.html
[AD-style]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_stylePicker.html
[AD-styles]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/styles.html
[AD-stylesp]: https://affinity.help/designer2/en-US.lproj/pages/Panels/stylesPanel.html
[AD-sw]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_lineWidth.html
[AD-swp]: https://affinity.help/designer2/en-US.lproj/pages/Panels/swatchesPanel.html
[AD-symbols]: https://affinity.help/designer2/en-US.lproj/pages/Panels/symbolsPanel.html
[AD-tools]: https://affinity.help/designer2/en-US.lproj/pages/Panels/toolsPanel.html
[AD-tp]: https://affinity.help/designer2/en-US.lproj/pages/Panels/transformPanel.html
[AD-transp]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_transparency.html
[AD-vbrush]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_Brush.html
[AD-vff]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_vectorFloodFill.html
[AD-view]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pan.html
[AD-viewmodes]: https://affinity.help/designer2/en-US.lproj/pages/GetStarted/view.html
[AD-warp]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/warp.html
[AD-xf]: https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/transform.html
[AD-zoom]: https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_zoom.html
[AI-K]: https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html
[AI-T]: https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html
[AI-align]: https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/align-and-distribute-objects.html
[AI-anchor]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/add-or-remove-anchor-points.html
[AI-app]: https://helpx.adobe.com/illustrator/using/appearance-attributes.html
[AI-area]: https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-area-of-objects.html
[AI-arrow]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/apply-and-edit-strokes/add-arrowheads.html
[AI-avg]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/average-the-position-of-anchor-points.html
[AI-blend]: https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/blended-objects-overview.html
[AI-blob]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/merge-paths-using-the-blob-brush-tool.html
[AI-brush]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/apply-and-edit-strokes/about-brushes.html
[AI-char]: https://helpx.adobe.com/illustrator/desktop/design-with-text/edit-format-text/character-panel-overview.html
[AI-clip]: https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/about-clipping-masks.html
[AI-cmp]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/create-compound-paths.html
[AI-convert]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/convert-anchor-points-on-a-path.html
[AI-curv]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-shapes-with-the-curvature-tool.html
[AI-cut]: https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/cut-objects.html
[AI-dim]: https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/about-dimension-objects.html
[AI-distort]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/distort-objects.html
[AI-div]: https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/divide-or-split-objects.html
[AI-env]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/distort-objects-with-envelopes.html
[AI-erase]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/erase-paths-using-eraser-tool.html
[AI-exp]: https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/expand-objects.html
[AI-eye]: https://helpx.adobe.com/illustrator/desktop/manage-colors/select-and-adjust-colors/select-colors.html
[AI-fs]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/fill-and-stroke-controls.html
[AI-grad]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-gradients/gradients-overview.html
[AI-graph]: https://helpx.adobe.com/illustrator/desktop/automate-visualize-data/visualize-data/create-graphs.html
[AI-grp]: https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-object-groups-and-nested-object-groups.html
[AI-gs]: https://helpx.adobe.com/illustrator/desktop/special-effects-styles/apply-graphic-styles/graphic-styles-panel-overview.html
[AI-guide]: https://helpx.adobe.com/illustrator/desktop/manage-colors/learn-color-basics/color-guide-panel-overview.html
[AI-iso]: https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/isolate-objects.html
[AI-layers]: https://helpx.adobe.com/illustrator/desktop/manage-layers/create-and-organize-layers/layers-panel-overview.html
[AI-live]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-live-shapes/change-corner-radius-of-live-shapes.html
[AI-lp]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/about-live-paint.html
[AI-lpb]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/paint-with-the-live-paint-bucket-tool.html
[AI-meas]: https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-distance-between-two-points.html
[AI-mesh]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-meshes/mesh-objects-overview.html
[AI-modes]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/learn-drawing-basics/drawing-modes-overview.html
[AI-multi]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/create-multiple-fills-and-strokes.html
[AI-off]: https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/offset-duplicate-objects.html
[AI-om]: https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/about-opacity-masks.html
[AI-para]: https://helpx.adobe.com/illustrator/desktop/design-with-text/edit-format-text/paragraph-panel-overview.html
[AI-pat]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-patterns/patterns-overview.html
[AI-pen]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-curves-with-the-pen-tool.html
[AI-pencil]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-freeform-paths-with-the-pencil-tool.html
[AI-persp]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/about-perspective-drawing.html
[AI-pf]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/pathfinder-panel-overview.html
[AI-rec]: https://helpx.adobe.com/illustrator/desktop/manage-colors/modify-colors/recolor-artwork.html
[AI-refl]: https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/reflect-or-flip-objects.html
[AI-rep]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-patterns/repeat-patterns-overview.html
[AI-rot]: https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/rotate-objects.html
[AI-samefs]: https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/select-objects-with-same-fill-and-stroke.html
[AI-scale]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/scale-objects.html
[AI-screens]: https://helpx.adobe.com/illustrator/desktop/save-and-export/export-files-to-different-formats/export-for-screens.html
[AI-sel]: https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-by-characteristics.html
[AI-shear]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/shear-objects.html
[AI-simp]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/manually-simplify-paths.html
[AI-smart]: https://helpx.adobe.com/illustrator/desktop/measure-and-align/grids-and-guides/work-with-smart-guides.html
[AI-smooth]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/create-smooth-paths-with-the-smooth-tool.html
[AI-stray]: https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/find-and-delete-stray-anchor-points.html
[AI-sw]: https://helpx.adobe.com/illustrator/desktop/manage-colors/use-swatches/swatches-panel-overview.html
[AI-swabout]: https://helpx.adobe.com/illustrator/desktop/manage-colors/use-swatches/about-swatches.html
[AI-sym]: https://helpx.adobe.com/illustrator/desktop/manage-objects/traces-mockups-symbols/symbols-panel-options.html
[AI-tr]: https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/transparency-panel-overview.html
[AI-trace]: https://helpx.adobe.com/illustrator/desktop/manage-objects/traces-mockups-symbols/image-trace-panel-options.html
[AI-type]: https://helpx.adobe.com/illustrator/desktop/design-with-text/add-manage-text/add-text-illustrator-text-tools.html
[AI-view]: https://helpx.adobe.com/illustrator/using/viewing-artwork.html
[AI-wand]: https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-magic-wand-tool.html
[AI-xf]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-objects.html
[AI-xfp]: https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-panel-overview.html
[VC-anchor]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/anchor.rs
[VC-arap]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/arap.rs
[VC-artboard]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/artboard.rs
[VC-builder]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/builder.rs
[VC-cat]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/catalog.rs
[VC-cbuild]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/buildcmds.rs
[VC-cdistort]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/distortcmds.rs
[VC-cdraw2]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/draw2.rs
[VC-ceffect]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/effectcmd.rs
[VC-cexpand]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/expand.rs
[VC-cgraph]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/graph.rs
[VC-clive]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/live.rs
[VC-cobject]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/object.rs
[VC-colorrecolor]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/recolor.rs
[VC-corners]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/corners.rs
[VC-cpaint]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/paint.rs
[VC-cpath]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/path.rs
[VC-cpathops]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/pathops.rs
[VC-cperspgrid]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/perspgrid.rs
[VC-crecolor]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/recolor.rs
[VC-cselect]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/select.rs
[VC-cshaper]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/shaper.rs
[VC-cslices]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/slices.rs
[VC-cstyle]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/style.rs
[VC-cswatch]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/swatch.rs
[VC-curv]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/curvature.rs
[VC-cut]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/cut.rs
[VC-cwand]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/wand.rs
[VC-direct]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/direct.rs
[VC-docapp]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/appearance.rs
[VC-docclip]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/clipnest.rs
[VC-docgraph]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/graph.rs
[VC-docpuppet]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/puppet.rs
[VC-docshaper]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/shaper.rs
[VC-docswatches]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/swatches.rs
[VC-extra]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/extra.rs
[VC-eye]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/eyedropper.rs
[VC-family]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/family.rs
[VC-free]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/free.rs
[VC-freeform]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/freeform.rs
[VC-fxdistort]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/distort.rs
[VC-fxstylize]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/stylize.rs
[VC-gesture]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/gesture.rs
[VC-gradtool]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/gradient.rs
[VC-harmony]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/color/src/harmony.rs
[VC-liq]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/liquify.rs
[VC-meas]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/measure.rs
[VC-meshblend]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/meshblend.rs
[VC-meshedit]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/meshedit.rs
[VC-pathtype]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/pathtype.rs
[VC-pen]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/pen.rs
[VC-persp]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/perspective.rs
[VC-pup]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/puppet.rs
[VC-select]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/select.rs
[VC-shape]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/shape.rs
[VC-slice]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/slice.rs
[VC-symbolism]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/symbolism.rs
[VC-text]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/text.rs
[VC-uicanvas]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/ui-egui/src/canvas.rs
[VC-wand]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/wand.rs
[VC-width]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/width.rs
[VC-xform]: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/transform.rs
