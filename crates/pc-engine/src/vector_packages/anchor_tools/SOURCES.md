# Anchor tools / Curvature sources and fixtures

Implementation inspected locally in `~/.local/share/local-image-dev/engine-sources/vectorcraft`
at pinned `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`:

- [tools/draw2/anchor.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/anchor.rs): Add/Delete Alt swap, click/drag Convert, displayed-handle precedence, bend gestures.
- [tools/draw2/curvature.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/curvature.rs): point toggle/drag/delete/close and rubber band.
- [tools/draw2.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2.rs): uniform Catmull–Rom handles `(next-prev)/6`, open-end/corner retraction.
- [engine/cmd/path.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/path.rs) and `engine/cmd/draw2.rs`: native command boundaries and Curvature request behavior. Existing Local Image `node`, `pen`, `edit` implementations are reused, not replaced.

VectorCraft is MIT OR Apache-2.0, copyright (c) 2026 ArtCraft Team and contributors.
Notices remain in `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}`. PORTS rows
are supplied in integration/33.md; 00 owns central attribution regeneration.

Official interaction docs inspected 2026-10-10:

- [Adobe Add/remove anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/add-or-remove-anchor-points.html).
- [Adobe Convert anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/convert-anchor-points-on-a-path.html).
- [Adobe Curvature tool](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-shapes-with-the-curvature-tool.html).

All test fixtures are original synthetic Rust definitions, GPL-3.0-or-later. No proprietary
fixtures, copied artwork, Inkscape goldens or external corpus are available/count as passed.
No external reference tool was executed. Toolchain: Rust/cargo/rustfmt/clippy 1.98.1;
egui/egui_kittest 0.36.2 from the existing lock; native V1/kurbo 0.13.1 nearest oracle.

Parameters and regeneration: run the `anchor33_` tests (GPU excluded). Cubic insertion uses
t=0.001, 0.12, 0.37, 0.5, 0.92, 0.999 and 1,001 uniform samples per case; max distance
1e-6 px. Circle fixture has eight 45-degree cubic arcs, radius 100 px, center (128,128),
handle length `100 * 4/3 * tan(pi/16)`. Each deleted node is checked both directions using
257 points per surviving/source segment and independent V1 nearest distance; maximum
0.5 px. Topology sequence: seed 0x331234, LCG multiplier 6364136223846793005, increment 1,
256 insert/delete steps, insertion t in [0.2,0.799], minimum five surviving nodes. Corner
identity is checked against retained anchors and distinct original radius labels.

Curvature fixtures: four unequal clicks, explicit internal corner, closed four-point square,
coincident clicks, short paths, nonfinite/oversize/overflow input. Independent endpoint,
derivative, incident-ray and polynomial-midpoint assertions check interpolation/G1/corners.
Metadata-remap fixtures use radius-2 Round corners at eligible internal polyline knots;
the replacement Curvature point stays a true corner. Open endpoints are deliberately not
used as live-corner targets. The captured-selection rejection fixture places a missing
work path last, then supplies it and checks both edited and untouched selection identities.
Kittest input uses a 240x180 document and actual PointerButton/PointerMoved/Key events at
60 Hz. The double-click sequence waits 60 frames after Alt-click to clear egui's 0.6 s
triple-click history, then verifies egui recognized a double-click before the curve oracle.
GPU tests use a 240x180 F32 white document, translucent blue stroke, full PhotocraftApp,
egui_kittest offscreen eframe renderer, and required RTX 5090 Vulkan/discrete identity,
Rgba32Float accumulation/float canvas. Readback is already premultiplied; compare bytes
directly with max difference <=1. Hardware execution is deferred by the owner's speed rules.
