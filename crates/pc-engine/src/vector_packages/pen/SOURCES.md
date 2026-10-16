# Pen sources and fixture provenance

Upstream source: https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/pen.rs
Read from the owner's read-only `~/.local/share/local-image-dev/engine-sources/vectorcraft` checkout;
`git rev-parse HEAD` verified that exact pin. MIT OR Apache-2.0, copyright (c) 2026 ArtCraft Team
and VectorCraft contributors. Root notices already exist in `licenses/vectorcraft-*`.
Replays follow its Alt-during-drag, Alt-from-start, close-drag and continuation tests,
adapted to detached whole-curve transactions rather than upstream's per-anchor edits.

Behavior references: [Affinity Designer 2 Pen modes](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pen.html),
[Draw curves (Line mode click-drag)](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/draw_linesAndShapes.html),
[egui 0.36.2 Event ordering](https://docs.rs/egui/0.36.2/egui/enum.Event.html), and existing Local Image
`vector_ui.rs`, `direct_select.rs`, `direct_select_tests.rs`, and `vector_api.rs`.
Graphite is UX-only in the spec; no Graphite code or brand assets are copied.

The exact Smart tangent formula is not specified by the public mode documentation or
the pinned VectorCraft pen. The local implementation uses the standard uniform
Catmull–Rom-to-cubic formula: interior tangent `(next - previous)/6`, endpoint tangent
`(next - endpoint)/3`, applied only to auto-created knots. Explicit handles and continued
endpoints are preserved. Collinearity and numeric tangent examples are tested independently.
No claim of numerical Affinity equivalence is made.

Fixtures are authored in Rust in this package, GPL-3.0-or-later as Local Image. Canvas 128x128,
Pen replay points `(20,20)`, `(100,20)`, `(80,90)` with outgoing handle `(50,90)` and closure
at `(20,20)`; tolerance 4 px; black solid fill, RGBA8. Cubic midpoint oracle `(101.25,55)`
is computed from the Bézier basis, and raster inside/outside assertions are independent.
256 deterministic replay seeds, 16 edits each, use an explicit 64-bit LCG (no network).
GPU gate uses wgpu 30.0.1, Vulkan, discrete RTX 5090, Rgba32Float accumulation and
premultiplied max difference <=1/255. No downloaded/proprietary fixture is needed.
Toolchain: Rust 1.98.1; egui/egui_kittest 0.36.2, versions pinned by the unchanged Cargo.lock.
