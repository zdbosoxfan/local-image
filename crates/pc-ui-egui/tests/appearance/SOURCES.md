# Appearance fixtures and sources

Synthetic paths, colors, patterns and hand-derived areas are original Local Image fixtures,
GPL-3.0-or-later. There are no brand assets, downloaded goldens or proprietary files.

Implementation references read before design:
- VectorCraft `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`,
  `crates/doc/src/appearance.rs`, `crates/effects/src/stroke/arrow.rs`, `dash.rs` and `gradient.rs`.
  MIT OR Apache-2.0, copyright ArtCraft Team and contributors. Local Image's stroker and
  exact-area coverage replace kurbo / VC renderer mechanics; fitted corner/end dash and
  arrow placement/shape semantics are adapted. These are not copied render modules.
- Existing PhotoCraft fork `c18d18cf613422fe08d31001e1f803cf8ef7dd68`,
  `crates/pc-compose/src/pattern.rs`: phase/link/rotation/uniform scale and premultiplied
  wrap interpolation. MIT OR Apache-2.0. Read-only local source, not an upstream app pin.
- Existing pc-vector V0 gradient layout/sampling at `4a4af2ed`, itself ported from official
  PhotoCraft #1321 `d438f842256435dba7aa41ca306656a216b98fbf`.
- W3C SVG 2 paint servers: https://www.w3.org/TR/SVG2/pservers.html
- W3C alpha/blend reference already implemented by pc-color:
  https://www.w3.org/TR/compositing-1/#blending

The six exact test pixels use an integer rectangle, two solid fills and two concentric
centered strokes, avoiding antialias ambiguity. Multiply alpha derives the general W3C
premultiplied equation independently. Arrow union area is shaft 40 + triangle 32 - overlap
8 = 64 px². A line gradient oracle uses normalized arc length and signed normal distance.
Aligned across bands probe the boolean fill; width profile distances follow the native absolute-width contract.
No performance claim follows from these small tests.

SVG oracles run through the existing egui_extras svg loader, which uses pinned resvg/usvg
0.45.1 (Cargo.lock), at intrinsic 128×128, identity transform, sRGB. The linear-gradient SVG
uses x=20.5..108.5 to account for Local Image's documented Photoshop top-left sampling and
integer gradient endpoints; compare interior pixels with tolerance 1/255. The phased stripe
pattern uses userSpaceOnUse, phase (3,2), 4×4 opaque tile, identity scale/angle. Skew and
repeat/reflect spread are explicitly rejected by capability warnings; they are not falsely
reported as equivalent native paints. Additional limitations/oracles are tracked in UI tests.

Plain-document native fixture is the existing licensed repository
`crates/pc-format/tests/fixtures/vector-basic.pcraft` (current format); new native roundtrips are generated in
memory. Width/brush projection fixtures inject a synthetic filled rectangle and explicitly
exercise the provider contract, not an implementation of packages 11/17.

GPU tests require Vulkan, discrete NVIDIA RTX 5090 and Rgba32Float. Missing/software
adapters fail. Engine preview/release tests compare pc-compose and the real GPU compositor
in premultiplied RGBA with max diff <=1/255. UI widget frames and eventual full-app frames
are separate acceptance gates; full-app source waits for 00/01's actual router hooks. PSD
reopen/export warnings require package 23, and are not supplied by native ZIP tests.
