# Package 10 sources and reproducible fixtures

Repository base: `7c1b909c`; continuation retains the foundation command slot and V1
geometry, shared transactions, appearance renderer, and egui 0.36.2 app. No prior
constructive implementation or tests existed beyond that slot. All calculations call
`photocraft_pathops`; no V1 algorithms or dependencies were changed.

VectorCraft checkout read at
`~/.local/share/local-image-dev/engine-sources/vectorcraft`, verified HEAD
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0,
copyright 2026 ArtCraft Team and VectorCraft contributors. Existing
`licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}` retain the notices.
No brand assets were used. Files inspected at this pin:

| Upstream file under `crates/` | SHA-256 | Used for |
|---|---|---|
| tools/src/builder.rs | 80ca6fa2aee642711a0df395770bf2b7e049819854549a09773521768114a29b | Cached faces, scrub sampling, selected-face hover hatch, clicked style |
| tools/src/cut.rs | 88da893a03aecfa2034c822b549e6a59b81c690a2c7ffb4132fcd0c6608369b7 | Captured line drag, Shift 45° snapping, preview/release/cancel |
| tools/src/draw2.rs | 9032b26db9537e028acc495cc048f1b9ad8ab4da260da3a5b26d180feb85aebb | Helper tool family |
| tools/src/draw2/gesture.rs | 0f825d7ba3925022a1a03867d6471bdf152e498c47bf01fe843ac3b0445e3605 | Freehand/helper scrub state and local interaction radius |
| tools/src/draw2/anchor.rs | 8b99fc87c661fe72d624012764d64c6ec66b86909087629bb4efdaa7703a1fed | Scissors picking/splitting |
| tools/src/extra.rs | 12ebeda18df0d49bd29fe183f2de1bdd172ef7ad366bdc87dae2d7330f076a8f | Examined; no applicable Contour implementation, no adaptation |

Also inspected `crates/pathops/src/planar.rs` and engine `cmd/{draw2,cut}.rs`
at the same pin. They informed the choice to use existing V1 arrangement/cut_out:
upstream Knife's 0.05-wide slit violates this package's area-conservation contract;
upstream eraser's fixed 24 samples can miss narrow erased intervals. We instead
use V1 arrangement intersections and V1 cut_out's exact cubic pieces. We did not
copy their boolean, offset, smoothing, joining or erasure algorithms.

Official behavior references, retrieved 2026-10-10:

- [Inkscape 1.3 Shape Builder](https://wiki.inkscape.org/wiki/Release_notes/1.3#Shape_Builder_Tool)
- [Affinity Contour Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_contour.html)
- [Affinity Knife Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_knife.html)

The task's Alt removal and per-frame modifier contract take precedence over upstream
latched modifiers. Offset normal picking is our small geometric adapter: sampled
curve tangent, left normal, filled-interior test to orient outwards. No applicable
upstream Contour state machine was found.

All package fixtures are generated in the owned Rust tests; Local Image,
GPL-3.0-or-later. No external/proprietary files or golden exports are claimed:

- Two radius-40 circles, centers (56,64)/(96,64); analytical lens formula,
  independent per-component coverage at 4x and 0.01px flatten tolerance.
- 100x100 rectangle at (20,20); central/diagonal node cut, tangent edge and
  incomplete cut; independent exact polynomial Green integral, coverage oracle.
- Rectangle with a 50x50 subtractive hole; short infinite-axis line cut.
- Cubic open path plus untouched closed contour; scissors at t=0.5.
- Two coincident 100x100 rectangles; Scissors picks the top source in paint order
  and leaves the lower layer byte-for-byte unchanged through the transaction.
- 60x60 rectangle at (40,40); round offset ±8; exact Euclidean signed distance
  from the analytic rectangle, 8x raster samples excluding 0.05px boundary band.
  This is a distance-transform limit oracle, not a claim that V1-Q's full EDT
  corpus has run. Inner offset -40 must disappear.
- Local smoothing, 0.5px erasure at x=64.23 (between upstream sample positions),
  closed outline erasure, and endpoint joining with an untouched third object.
- F32 160x160 app fixture on white; actual canvas GPU pixel comparison on
  152x152 source pixel centers, premultiplied max difference <=1/255.
- Kittest real pointer/button/modifier/typing events; no direct setter substitutes
  for the tested interactions.

Toolchain: Rust 1.98.1, egui/egui_kittest 0.36.2 from existing lock. Parameters
and geometry are recorded in the Rust source. GPU tests require RTX 5090,
Vulkan, DiscreteGpu, Rgba32Float accumulation and f16 canvas support; they fail
if the required hardware is absent. GPU execution, timing, full V1-Q corpus,
Inkscape/Affinity goldens, PSD corpus and manual QA are deferred under the owner's
latest speed/ownership rules, and are not successful gates.
