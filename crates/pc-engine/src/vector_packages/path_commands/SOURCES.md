# Package 36 sources and fixtures

Read the repository architecture, persona spec (including owner decisions), quality plan,
package README and gap audit. Baseline 571f0cd4 retained; no earlier package-36 files were present.

Behavior/transaction adaptation: VectorCraft at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, local read-only checkout
`~/.local/share/local-image-dev/engine-sources/vectorcraft`:
`crates/engine/src/cmd/path.rs` (nearest ends and global Average),
`crates/engine/src/cmd/pathops.rs` (cleanup/divide/grid),
`crates/engine/src/cmd/expand.rs` (one family and options),
`crates/engine/src/cmd/edit.rs` and tests_clipboard.rs (Front/Back relative insertion and
no-selection insertion at the current container's ends; here the document root).
MIT OR Apache-2.0, © 2026 ArtCraft Team and contributors;
existing `licenses/vectorcraft-*` notices retained. No brand assets copied.
Geometry algorithms are called through existing pc-pathops public APIs, including its
`kernel::{simplify_with,add_anchor_points,remove_anchor}` and document adapter; they are not
copied into this package. Layer insertion/native clipboard/transaction code is original
GPL-3.0-or-later Local Image code.

Official behavior docs read online 2026-10-10:
- https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/average-the-position-of-anchor-points.html
- https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/manually-simplify-paths.html
- https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/divide-or-split-objects.html
- https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/expand-objects.html

All geometry fixtures are original, generated in Rust tests (GPL-3.0-or-later). No downloaded
or proprietary corpus is represented as passing. The 100-node circle is a radius-40 regular
polygon centered at (70,70). Simplify precision 80 maps to tolerance 1.008 document pixels;
bidirectional polygon/curve Hausdorff is checked independently at flatten tolerance 0.002.
The ellipse anchor-removal oracle flattens at 0.001 and checks error <0.02 document pixels.
Grid fixture: bounds (10,20)-(120,100), rows=3, columns=2, both gutters=10, six exact cells.
Divide fixture: 100×80 rectangle cut by a 20-wide rectangle, total area 8000 and disjoint interiors.
Four-paint fixture: rectangle (20,20)-(70,70); bottom fill, 8px stroke, second fill, 2px stroke;
fill opacity 0.7, stroke opacity 0.6, outer opacity 0.8, float CPU composite. Max premultiplied
error <=1/255. Full-app Expand uses the radius-40 100-node circle with the same four
paints/outer opacity, in a 180×140 float document. Full-app Simplify uses that circle with
precision 90 initially, real slider pointer frames and Cancel/release/undo. Path-text fixture uses pc-text's bundled Inter (SIL OFL), "ABC", 20 pt,
straight baseline (20,100)-(220,100); no system or proprietary font required. Owner 12's
conversion oracle (`path_text_converted_curves_match_render_and_keep_counters`) is filled-glyph
IoU >0.97; its text coverage and pc-vector edge AA differ. Package 36 follows that owner
contract for path text, while retaining max premultiplied <=1/255 for the appearance and other
specified pixel gates.

Versions: Rust/Cargo 1.98.1, locked egui/egui_kittest 0.36.2, wgpu 30.0.1 family (Cargo.lock),
kurbo 0.13.1 through V1. Offline, CARGO_BUILD_JOBS=3 and the supplied li-rustc-gate wrapper.
GPU gates require a named RTX 5090, Vulkan, DiscreteGpu, Rgba32Float accumulation; full-app
checks additionally require float canvas support and reject CPU fallback. Execution belongs
at integration under the final task speed override. No performance claim is made.

PORTS attribution rows for coordinator 00 (do not change the shared catalog here):

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-engine/src/vector_packages/path_commands.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/path.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/path_commands.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/pathops.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/expand.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/expand.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/path_commands/clipboard.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/edit.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
