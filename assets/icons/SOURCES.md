# Tool icon sources

Owner decision 2026-10-13: the hand-drawn Glossy Duotone tool icons are retired. Tool icons are the
industry-standard monochrome line set used upstream: **Lucide 0.468.0** (ISC, pinned at
`f12b0de177fbc2a6795e99be065887e72b237123`), **PhotoCraft** originals and **VectorCraft** `dc-*` icons
(MIT OR Apache-2.0), with **Tabler Icons 3.44.0** outline (MIT) as the same-style fallback. Where no set
has the metaphor, or the owner rejected upstream's, the icon is a **Local Image original** drawn to
Lucide's rules (24×24 viewBox, 2 px stroke, round caps and joins, no fills, Lucide's optical padding;
GPL-3.0-or-later). Avoided: Remix Icon (custom licence), PhotoCraft's `lasso-magnetic` (CC BY 3.0) and
GNOME CC-BY-SA-3.0 artwork. No Adobe artwork is used; Photoshop/Lightroom/Illustrator only informed the
metaphors.

Upstream files are copied byte for byte (two recorded exceptions: Path Selection adds a solid fill,
Twirl CCW mirrors `dc-twirl`). `source-manifest.json` holds every icon's set, source file, licence,
colour family and SHA-256; `cargo xtask icons` checks the files, hashes, licence texts, line style and
registries against it. Colour mode is not separate artwork: it tints the same line icon by family
(`crates/pc-ui-egui/src/color_icon_data.rs`); monochrome, in the theme's icon colour, is the default.

Licence texts: `licenses/lucide-ISC.txt` and `licenses/feather-MIT.txt` (Lucide, Feather-derived
paths), `licenses/photocraft-LICENSE-{MIT,APACHE}`, `licenses/vectorcraft-LICENSE-{MIT,APACHE}`,
`licenses/tabler-icons-MIT.txt`; Local Image originals are under `LICENSE` (GPL-3.0-or-later).

## Adding or replacing a tool icon

1. Prefer Lucide 0.468.0 (`icons/<name>.svg` at the pinned commit), then PhotoCraft/VectorCraft, then
   Tabler outline. Copy the file unchanged to `assets/icons/tools/<id>.svg`, keeping our id.
2. Only if nothing fits, draw it: Lucide's template (24×24, `fill="none"`, `stroke="currentColor"`,
   `stroke-width="2"`, round caps/joins), shapes inside 2..22, legible at 16 and 20 px, and a comment
   `Local Image original, GPL-3.0-or-later: <what it shows>`.
3. Add its manifest row (set, source, licence, family, `sha256`), a row below, and register it in
   `crates/pc-ui-egui/src/tool_icon_data.rs` (or the vector table) and `color_icon_data.rs`.
4. `cargo xtask icons`, `cargo xtask attributions`, then `cargo xtask tool-icons` and look at the sheets.

## Counts

168 tool icons: Lucide 92, VectorCraft 37, Local Image originals 24, PhotoCraft 9, Lucide (lucide-static 1.49.0, as vendored by VectorCraft) 3, Tabler Icons (outline) 3.
Sourced 144, drawn 24.

## Every tool icon

| Id | Tool | Set | Source | Licence | Note |
|---|---|---|---|---|---|
| `activity` | History | Lucide | [history](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/history.svg) | ISC | History panel: Lucide `history` (clock with an undo arrow) rather than the pulse line. |
| `arrow-left-right` | Content-Aware Move | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set has Content-Aware Move. |
| `background-mask` | Background Mask | Lucide | [image](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/image.svg) | ISC |  |
| `blend` | Gradient | PhotoCraft | [gradient](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/gradient.svg) | MIT OR Apache-2.0 |  |
| `bloat` | Bloat | VectorCraft | [dc-bloat](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-bloat.svg) | MIT OR Apache-2.0 |  |
| `blur` | Blur | Lucide | [droplet](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/droplet.svg) | ISC |  |
| `brush` | Brush | Lucide | [brush](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/brush.svg) | ISC |  |
| `circle` | Ellipse Shape | Lucide | [circle](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/circle.svg) | ISC |  |
| `circle-dashed` | Ellipse Marquee | Lucide | [circle-dashed](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/circle-dashed.svg) | ISC |  |
| `circle-dot` | Count | Lucide | [hash](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/hash.svg) | ISC |  |
| `color-range` | Colour Range | Lucide | [palette](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/palette.svg) | ISC |  |
| `crop` | Crop | Lucide | [crop](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/crop.svg) | ISC |  |
| `custom-shape` | Custom Shape | Lucide | [shapes](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/shapes.svg) | ISC |  |
| `defringe-picker` | Defringe | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set has a defringe eyedropper. |
| `depth-range` | Depth Range | Lucide | [layers-2](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/layers-2.svg) | ISC |  |
| `develop-edit` | Develop Edit | Lucide | [sliders-horizontal](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/sliders-horizontal.svg) | ISC |  |
| `direct-select` | Direct Selection | PhotoCraft | [direct-select](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/direct-select.svg) | ISC (Lucide-derived; PhotoCraft marks MIT OR Apache-2.0) |  |
| `dodge` | Dodge | Lucide | [lollipop](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/lollipop.svg) | ISC |  |
| `edit-toolbar` | Edit Toolbar | Lucide | [ellipsis](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/ellipsis.svg) | ISC |  |
| `eraser` | Eraser | Lucide | [eraser](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/eraser.svg) | ISC |  |
| `eraser-background` | Background Eraser | PhotoCraft | [eraser-background](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/eraser-background.svg) | ISC (Lucide-derived; PhotoCraft marks MIT OR Apache-2.0) |  |
| `eraser-magic` | Magic Eraser | PhotoCraft | [eraser-magic](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/eraser-magic.svg) | ISC (Lucide-derived; PhotoCraft marks MIT OR Apache-2.0) |  |
| `flame` | Burn | Lucide | [grab](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/grab.svg) | ISC | Owner: Burn is Photoshop's cupped hand, not a flame (Lucide 0.468 name `grab`, later `hand-grab`). |
| `forward-warp` | Forward Warp | Lucide | [waves](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/waves.svg) | ISC |  |
| `freeze` | Freeze Mask | Lucide | [snowflake](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/snowflake.svg) | ISC |  |
| `freeze-all` | Freeze All | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify mask action; no open set. |
| `freeze-lasso` | Freeze Lasso | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify mask action; no open set. |
| `guided-upright` | Guided Upright | Tabler Icons (outline) | [perspective](https://github.com/tabler/tabler-icons/blob/v3.44.0/icons/outline/perspective.svg) | MIT | Tabler fallback: Lucide 0.468 has no perspective glyph. |
| `hand` | Hand | Lucide | [hand](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/hand.svg) | ISC |  |
| `healing` | Healing | Lucide | [bandage](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/bandage.svg) | ISC |  |
| `history-brush` | History Brush | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Owner: a brush with an undo arc, not upstream's clock. |
| `info` | Info | Lucide | [info](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/info.svg) | ISC |  |
| `invert-freeze` | Invert Freeze | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify mask action; no open set. |
| `keywords` | Keywords | Lucide | [tag](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/tag.svg) | ISC |  |
| `landscape-mask` | Landscape Mask | Lucide | [mountain](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/mountain.svg) | ISC |  |
| `lasso` | Lasso | Lucide | [lasso](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/lasso.svg) | ISC |  |
| `lasso-magnetic` | Magnetic Lasso | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Owner: avoid PhotoCraft's CC BY 3.0 Noun Project glyph; drawn in Lucide style instead. |
| `lasso-select` | Patch | Lucide | [lasso-select](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/lasso-select.svg) | ISC |  |
| `linear-mask` | Linear Mask | PhotoCraft | [gradient](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/gradient.svg) | MIT OR Apache-2.0 |  |
| `luminance-range` | Luminance Range | Lucide | [sun](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/sun.svg) | ISC |  |
| `mask` | Masking | PhotoCraft | [layer-mask](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/layer-mask.svg) | MIT OR Apache-2.0 |  |
| `message-square` | Note | Lucide | [sticky-note](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/sticky-note.svg) | ISC |  |
| `more` | More | Lucide | [ellipsis](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/ellipsis.svg) | ISC |  |
| `mouse-pointer-2` | Path Selection | Lucide | [mouse-pointer-2](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/mouse-pointer-2.svg) | ISC | Solid arrow = Path Selection, hollow = Direct Selection (Photoshop/Illustrator). |
| `move` | Move | Lucide | [move](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/move.svg) | ISC |  |
| `object-mask` | Object Mask | Lucide | [square-dashed-mouse-pointer](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/square-dashed-mouse-pointer.svg) | ISC |  |
| `object-selection` | Object Selection | Lucide | [square-dashed-mouse-pointer](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/square-dashed-mouse-pointer.svg) | ISC |  |
| `paint-bucket` | Paint Bucket | Lucide | [paint-bucket](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/paint-bucket.svg) | ISC |  |
| `palette` | Mixer Brush | Lucide | [palette](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/palette.svg) | ISC |  |
| `pen-tool` | Pen | Lucide | [pen-tool](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pen-tool.svg) | ISC |  |
| `pencil` | Pencil | Lucide | [pencil](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pencil.svg) | ISC |  |
| `pentagon` | Polygonal Lasso | Tabler Icons (outline) | [lasso-polygon](https://github.com/tabler/tabler-icons/blob/v3.44.0/icons/outline/lasso-polygon.svg) | MIT | Tabler fallback: an angular lasso; Lucide has none. |
| `people-mask` | People Mask | Lucide | [users](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/users.svg) | ISC |  |
| `perspective-crop` | Perspective Crop | PhotoCraft | [perspective-crop](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/perspective-crop.svg) | MIT OR Apache-2.0 |  |
| `pipette` | Eyedropper | Lucide | [pipette](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pipette.svg) | ISC |  |
| `presets` | Presets | Lucide | [swatch-book](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/swatch-book.svg) | ISC |  |
| `prompt-mask` | Prompt Mask | Lucide | [message-square-text](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/message-square-text.svg) | ISC |  |
| `pucker` | Pucker | VectorCraft | [dc-pucker](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-pucker.svg) | MIT OR Apache-2.0 |  |
| `push-left` | Push Left | Lucide | [move-left](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/move-left.svg) | ISC |  |
| `quick-mask` | Quick Mask | PhotoCraft | [layer-mask](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/layer-mask.svg) | MIT OR Apache-2.0 |  |
| `quick-selection` | Quick Selection | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set has Quick Selection (brush tip in a dashed selection). |
| `radial-mask` | Radial Mask | Lucide | [circle-dashed](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/circle-dashed.svg) | ISC |  |
| `reconstruct` | Reconstruct | Lucide | [undo-2](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/undo-2.svg) | ISC |  |
| `reconstruct-all` | Reconstruct All | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify action; no open set. |
| `rectangle-horizontal` | Rectangle Shape | Lucide | [rectangle-horizontal](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/rectangle-horizontal.svg) | ISC |  |
| `red-eye` | Red Eye | Lucide | [scan-eye](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/scan-eye.svg) | ISC |  |
| `remove` | Spot Removal | PhotoCraft | [bandage-sparkle](https://github.com/storytold/photocraft/blob/72b499a28ff8f089fecc87963561e32ec272ec26/assets/icons/bandage-sparkle.svg) | ISC (Lucide-derived; PhotoCraft marks MIT OR Apache-2.0) |  |
| `rotate-view` | Rotate View | VectorCraft | [dc-rotate-view](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-rotate-view.svg) | MIT OR Apache-2.0 | Owner: a rotating canvas, not a compass. |
| `ruler` | Ruler | Lucide | [ruler](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/ruler.svg) | ISC |  |
| `scissors` | AI Cutout | Lucide | [scissors](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/scissors.svg) | ISC |  |
| `shape-polygon` | Polygon Shape | Lucide | [hexagon](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/hexagon.svg) | ISC |  |
| `shape-triangle` | Triangle Shape | Lucide | [triangle](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/triangle.svg) | ISC |  |
| `sharpen` | Sharpen | Lucide | [pyramid](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pyramid.svg) | ISC | A sharp cone, distinct from the Triangle shape. |
| `sky-mask` | Sky Mask | Lucide | [cloud-sun](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/cloud-sun.svg) | ISC |  |
| `skin-picker` | Skin Tone Selector | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set has a skin-tone eyedropper. |
| `slash` | Line Shape | Lucide | [slash](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/slash.svg) | ISC |  |
| `slice-knife` | Slice | Lucide | [slice](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/slice.svg) | ISC |  |
| `slice-select` | Slice Select | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Must differ from Object Selection; no open set has a selectable slice knife. |
| `smooth` | Smooth | Lucide | [spline](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/spline.svg) | ISC |  |
| `smudge` | Smudge | Lucide | [pointer](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pointer.svg) | ISC |  |
| `sparkles` | AI Remove | Lucide | [sparkles](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/sparkles.svg) | ISC |  |
| `sponge` | Sponge | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Owner: a sponge, not upstream's cloud. No open set has one. |
| `spot-healing` | Spot Healing | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Must differ from Healing (Lucide bandage); no open set has Photoshop's spot-and-bandage. |
| `square-dashed` | Rectangle Marquee | Lucide | [square-dashed](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/square-dashed.svg) | ISC |  |
| `stamp` | Clone Stamp | Lucide | [stamp](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/stamp.svg) | ISC |  |
| `straighten` | Straighten | Lucide | [ruler](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/ruler.svg) | ISC | Lucide 0.468 has no level; `ruler-dimension-line` is newer than the pin. |
| `subject-mask` | Subject Mask | Lucide | [user](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/user.svg) | ISC |  |
| `targeted-adjust` | Targeted Adjustment | Lucide | [crosshair](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/crosshair.svg) | ISC |  |
| `thaw` | Thaw Mask | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify mask brush; no open set. |
| `thaw-all` | Thaw All | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Liquify mask action; no open set. |
| `twirl-ccw` | Twirl CCW | VectorCraft | [dc-twirl](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-twirl.svg) | MIT OR Apache-2.0 | Mirrored dc-twirl, so the two Twirl directions differ. |
| `twirl-cw` | Twirl CW | VectorCraft | [dc-twirl](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-twirl.svg) | MIT OR Apache-2.0 |  |
| `type` | Type | Lucide | [type](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/type.svg) | ISC |  |
| `versions` | Versions | Lucide | [layers](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/layers.svg) | ISC |  |
| `vertical-type` | Vertical Type | VectorCraft | [dc-type-vertical](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-type-vertical.svg) | MIT OR Apache-2.0 |  |
| `wand-sparkles` | Magic Wand | Lucide | [wand-sparkles](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/wand-sparkles.svg) | ISC |  |
| `wb-picker` | White Balance Selector | Lucide | [pipette](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pipette.svg) | ISC |  |
| `zoom-in` | Zoom | Lucide | [zoom-in](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/zoom-in.svg) | ISC |  |
| `vector-anchor-add` | Add Anchor Point | Lucide (lucide-static 1.49.0, as vendored by VectorCraft) | [pen-tool-add](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/pen-tool-add.svg) | ISC (VectorCraft badge MIT OR Apache-2.0) |  |
| `vector-anchor-delete` | Delete Anchor Point | Lucide (lucide-static 1.49.0, as vendored by VectorCraft) | [pen-tool-delete](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/pen-tool-delete.svg) | ISC (VectorCraft badge MIT OR Apache-2.0) |  |
| `vector-anchor-point` | Anchor Point | VectorCraft | [dc-anchor](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-anchor.svg) | MIT OR Apache-2.0 |  |
| `vector-arc` | Arc | VectorCraft | [dc-arc](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-arc.svg) | MIT OR Apache-2.0 |  |
| `vector-area` | Area | VectorCraft | [dc-measure](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-measure.svg) | MIT OR Apache-2.0 |  |
| `vector-arrow` | Arrow | Lucide | [move-right](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/move-right.svg) | ISC |  |
| `vector-artboard` | Artboard | Lucide | [frame](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/frame.svg) | ISC |  |
| `vector-blend` | Blend | VectorCraft | [dc-blend](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-blend.svg) | MIT OR Apache-2.0 |  |
| `vector-blob-brush` | Blob Brush | Lucide | [brush](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/brush.svg) | ISC |  |
| `vector-brush` | Vector Brush | Lucide | [paintbrush](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/paintbrush.svg) | ISC |  |
| `vector-callout-ellipse` | Ellipse Callout | Lucide | [message-circle](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/message-circle.svg) | ISC |  |
| `vector-callout` | Callout | Lucide | [message-square](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/message-square.svg) | ISC |  |
| `vector-cloud` | Cloud | Lucide | [cloud](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/cloud.svg) | ISC |  |
| `vector-cog` | Cog | Lucide | [cog](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/cog.svg) | ISC |  |
| `vector-contour` | Contour | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-corner` | Corner | Local Image originals | drawn for Local Image | GPL-3.0-or-later | Must differ from Anchor Point (dc-anchor); no open set. |
| `vector-crescent` | Crescent | Lucide | [moon](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/moon.svg) | ISC |  |
| `vector-crop` | Vector Crop | Lucide | [crop](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/crop.svg) | ISC |  |
| `vector-crystallize` | Crystallize | VectorCraft | [dc-crystallize](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-crystallize.svg) | MIT OR Apache-2.0 |  |
| `vector-curvature` | Curvature | Lucide (lucide-static 1.49.0, as vendored by VectorCraft) | [spline-pointer](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/spline-pointer.svg) | ISC (VectorCraft badge MIT OR Apache-2.0) | `spline-pointer` is newer than Lucide 0.468; VectorCraft's lucide-static 1.49.0 copy. |
| `vector-diamond` | Diamond | Lucide | [diamond](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/diamond.svg) | ISC |  |
| `vector-donut` | Donut | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-double-star` | Double Star | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-draw-behind` | Draw Behind | VectorCraft | [dc-draw-behind](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-draw-behind.svg) | MIT OR Apache-2.0 |  |
| `vector-draw-inside` | Draw Inside | VectorCraft | [dc-draw-inside](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-draw-inside.svg) | MIT OR Apache-2.0 |  |
| `vector-draw-normal` | Draw Normal | VectorCraft | [dc-draw-normal](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-draw-normal.svg) | MIT OR Apache-2.0 |  |
| `vector-envelope` | Envelope | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-eraser` | Vector Eraser | Lucide | [eraser](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/eraser.svg) | ISC |  |
| `vector-fill-stroke` | Fill and Stroke | VectorCraft | [dc-swap](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-swap.svg) | MIT OR Apache-2.0 |  |
| `vector-flood-fill` | Vector Flood Fill | Lucide | [paint-bucket](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/paint-bucket.svg) | ISC |  |
| `vector-frame-text` | Frame Text | VectorCraft | [dc-type-area](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-type-area.svg) | MIT OR Apache-2.0 |  |
| `vector-free-transform` | Free Transform | VectorCraft | [dc-free-transform](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-free-transform.svg) | MIT OR Apache-2.0 |  |
| `vector-group-select` | Group Selection | VectorCraft | [dc-group-select](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-group-select.svg) | MIT OR Apache-2.0 |  |
| `vector-heart` | Heart | Lucide | [heart](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/heart.svg) | ISC |  |
| `vector-join` | Join | VectorCraft | [dc-join](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-join.svg) | MIT OR Apache-2.0 |  |
| `vector-knife` | Knife | VectorCraft | [dc-knife](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-knife.svg) | MIT OR Apache-2.0 |  |
| `vector-lasso` | Vector Lasso | Lucide | [lasso](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/lasso.svg) | ISC |  |
| `vector-line-cut` | Line Cut | VectorCraft | [dc-line-cut](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-line-cut.svg) | MIT OR Apache-2.0 |  |
| `vector-live-paint` | Live Paint Bucket | VectorCraft | [dc-live-bucket](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-live-bucket.svg) | MIT OR Apache-2.0 |  |
| `vector-live-select` | Live Paint Selection | VectorCraft | [dc-live-select](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-live-select.svg) | MIT OR Apache-2.0 |  |
| `vector-mesh` | Mesh | VectorCraft | [dc-mesh](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-mesh.svg) | MIT OR Apache-2.0 |  |
| `vector-path-eraser` | Path Eraser | VectorCraft | [dc-path-eraser](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-path-eraser.svg) | MIT OR Apache-2.0 |  |
| `vector-path-text` | Text on Path | VectorCraft | [dc-type-path](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-type-path.svg) | MIT OR Apache-2.0 |  |
| `vector-pencil` | Vector Pencil | Lucide | [pencil](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pencil.svg) | ISC |  |
| `vector-pie` | Pie | Lucide | [chart-pie](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/chart-pie.svg) | ISC |  |
| `vector-point-transform` | Point Transform | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-polar-grid` | Polar Grid | VectorCraft | [dc-polar-grid](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-polar-grid.svg) | MIT OR Apache-2.0 |  |
| `vector-puppet` | Puppet Warp | VectorCraft | [dc-puppet](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-puppet.svg) | MIT OR Apache-2.0 |  |
| `vector-rect-grid` | Rectangular Grid | VectorCraft | [dc-rect-grid](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-rect-grid.svg) | MIT OR Apache-2.0 |  |
| `vector-reflect` | Reflect | Lucide | [flip-horizontal-2](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/flip-horizontal-2.svg) | ISC |  |
| `vector-repeat` | Repeat | Lucide | [repeat](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/repeat.svg) | ISC |  |
| `vector-reshape` | Reshape | VectorCraft | [dc-reshape](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-reshape.svg) | MIT OR Apache-2.0 |  |
| `vector-rotate` | Rotate | Lucide | [rotate-ccw](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/rotate-ccw.svg) | ISC |  |
| `vector-rounded-rectangle` | Rounded Rectangle | VectorCraft | [dc-rounded-rect](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-rounded-rect.svg) | MIT OR Apache-2.0 |  |
| `vector-scale` | Scale | Lucide | [scaling](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/scaling.svg) | ISC |  |
| `vector-scallop` | Scallop | VectorCraft | [dc-scallop](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-scallop.svg) | MIT OR Apache-2.0 |  |
| `vector-scissors` | Path Scissors | Lucide | [scissors](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/scissors.svg) | ISC |  |
| `vector-segment` | Segment | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-shape-builder` | Shape Builder | VectorCraft | [dc-shape-builder](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-shape-builder.svg) | MIT OR Apache-2.0 |  |
| `vector-shear` | Shear | VectorCraft | [dc-shear](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-shear.svg) | MIT OR Apache-2.0 |  |
| `vector-smooth` | Smooth Path | VectorCraft | [dc-smooth](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-smooth.svg) | MIT OR Apache-2.0 |  |
| `vector-spiral` | Spiral | Tabler Icons (outline) | [spiral](https://github.com/tabler/tabler-icons/blob/v3.44.0/icons/outline/spiral.svg) | MIT | Tabler fallback: Lucide 0.468 has no spiral (its tornado reads as lines at 16 px). |
| `vector-square-star` | Square Star | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-star` | Star | Lucide | [star](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/star.svg) | ISC |  |
| `vector-style-picker` | Style Picker | Lucide | [pipette](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/pipette.svg) | ISC |  |
| `vector-tear` | Tear | Lucide | [droplet](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/droplet.svg) | ISC |  |
| `vector-transparency` | Transparency | Lucide | [blend](https://github.com/lucide-icons/lucide/blob/f12b0de177fbc2a6795e99be065887e72b237123/icons/blend.svg) | ISC |  |
| `vector-trapezoid` | Trapezoid | Local Image originals | drawn for Local Image | GPL-3.0-or-later | No open set. |
| `vector-width` | Width | VectorCraft | [dc-width](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-width.svg) | MIT OR Apache-2.0 |  |
| `vector-wrinkle` | Wrinkle | VectorCraft | [dc-wrinkle](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/assets/icons/dc-wrinkle.svg) | MIT OR Apache-2.0 |  |

## Interface icons (Library and Develop)

`crates/lc-ui-egui/src/icons.rs` draws its interface icons from line SVGs: Lucide 0.468.0 files in
`assets/icons/*.svg` (added 2026-10-13: `panel-left`, `arrow-left`, `arrow-right`, `filter`, `bell`,
`share`, `circle-help`, `layout-grid`, `layout-dashboard`, `columns-2`,
`square-split-horizontal`, `arrow-down-wide-narrow`, `flag`, `flag-off`, `film`, `book-image`, `images`,
`trash-2`, `chart-spline`, `flip-horizontal-2`, `video`, `message-circle`, `scan-face`;
unchanged, ISC), PhotoCraft's `star-filled.svg` (Lucide star with a solid fill) and the shared tool icons.
Collections use Lucide `images.svg`. Smart Collection keeps the legacy asset path `smart-album.svg`: Lucide `images` stacked outline with a Local Image funnel (ISC + GPL-3.0-or-later).
The editor's generic icons (`icon_data.rs`) are the Lucide files PhotoCraft shipped; see
`licenses/photocraft-ATTRIBUTION.md`.
