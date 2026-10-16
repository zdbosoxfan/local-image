# Package 35 source and fixture record

Implementation reference: read-only VectorCraft checkout
`~/.local/share/local-image-dev/engine-sources/vectorcraft`, project commit
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, version 0.7.0, MIT OR Apache-2.0.
Read `crates/engine/src/cmd/object.rs` (clippingMask and compoundPath commands;
last-change commit `7e82dbc05b972d8b32b68e7b31444a87ed8e6d48`), `crates/doc/src/clipnest.rs`
and clip/layerclip tests. The source object tree is not adopted. Original Local Image
transaction, native-mask and selection adapters follow the repository architecture.
Notice: Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors; existing
`licenses/vectorcraft-{NOTICE,LICENSE-MIT,LICENSE-APACHE}`.

Official behavior references, read 2026-10-10:

- [Adobe clipping masks](https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/about-clipping-masks.html)
- [Adobe compound paths](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/create-compound-paths.html)
- [Adobe drawing modes](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/learn-drawing-basics/drawing-modes-overview.html)
- [Affinity Layer clipping](https://affinity.help/designer2/en-US.lproj/pages/Layers/layerClip.html)
- [Affinity Fill modes](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/fillMode.html)

All fixtures are generated in Rust tests, original GPL-3.0-or-later, no downloaded artwork.
The 96x96 star fixture uses the ten explicit points in `tests.rs`, three independently
composited rectangle children, F32 RGB and pc-vector star coverage as the multiplication
oracle. No clipping-group construction is shared with that oracle. Concentric cubic
circles have center (48,48), radii 30 and 15; the V1 area and analytic pi*r^2 values are
independent of the compound command. Cubic-circle analytic tolerance is 0.1 percent.
Raster/CPU/GPU parity uses premultiplied maximum channel difference <=1/255.
Text fixture: bundled Inter, SIL OFL-1.1 (existing font notices), glyph O, 42pt,
transform translate(15,60), default document DPI. pc-text supplies all glyph outlines.
UI fixture: 160x160 U8 RGB, rect(10,10,130,130), ellipse(30,30,80,80);
egui/eframe/egui_kittest 0.36.2, Rust 1.98.1; workspace packages 2.0.0-alpha.1.
Actual keyboard and pointer events drive package fragments.
Production app GPU fixture: 160x160 F32 RGB, white raster background, white ellipse,
real Rectangle tool drag (10,10) to (130,130), fixed 1x view, offscreen app canvas.
Vector persona with remembered Rectangle tool; black foreground, fill enabled,
zero stroke/corner radius. Its CPU preview must have a visibly black center before
checking the actual app GPU float texture, preventing unchanged white pixels from passing.

Exports generated at test time through 14/21/23: SVG default precision 6, PDF krilla 0.8.2
uncompressed content for structural clipping-operator inspection, PSD default options,
native .pcraft default save/load. No external parser claimed as a visual oracle.
Missing proprietary Adobe/Affinity fixtures and external goldens are not passing corpus gates.
The task's final speed override defers GPU runs, production router acceptance and
performance measurements to integration; it forbids whole-crate suites and release builds.
