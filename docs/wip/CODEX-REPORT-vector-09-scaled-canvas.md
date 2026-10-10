# Package 09: scaled canvas

Status: owned legacy implementation is present; integrated acceptance is pending package
00 routing/registration and the named enhanced projection providers. This is not a claim
that package 09 is complete. Work continues from baseline `4a4af2ed` and preserves the
existing pen/node/V1 foundations. Changes are uncommitted.

## Implementation

- `pc-compose/src/scaled_view.rs` implements `ViewRequest`, quarter-octave physical scale
  buckets, `ViewTiles`, `flatten_scaled[_with]` and a transient per-layer projection cache.
  Scale includes logical zoom and physical pixels per point. A detached layer tree goes
  through the established compositor, preserving stack order, clipped bases, groups,
  adjustments, masks, patterns and effects. Global coordinates preserve paint phase across
  tiles. Raster magnification follows the existing nearest document sampler.
- `pc-vector/src/view.rs` transforms legacy paths/strokes once and renders exact-area
  coverage within a supplied clip. Relative dash lengths remain relative to stroke width.
  Geometry, stroke dimensions and scale are finite and bounded; persisted caches are intact.
- `pc-text/src/view.rs` retains document text layout/DPI, post-scales the transform and
  rasterizes visible 256px bands with the existing glyph drawer, warp and faux-style rules.
  The native module is tested independently. Native registry wiring belongs to 00; until
  that wiring lands, the app uses the existing public text renderer only for bounded whole
  ink buffers, then crops to the visible apron. Large offscreen type returns a named
  registration dependency instead of allocating an unbounded buffer.
- `pc-gpu/src/scaled_view.rs` renders the projected tree through the real existing GPU
  compositor. The app callback encodes its output directly into visible GPU textures. No
  GPU readback/flatten-upload is used by this path. Unsupported GPU plans use the same
  scaled CPU tree/reference.
- `gpu_canvas.rs` snapshots source documents, retains per-view scaled frames and projection
  caches, and samples them with the exact view transform. Device tiles have sampling
  gutters; nearby fractional zooms sample level zero across shared edges. CMS, HDR,
  checkerboard and grid remain in the established display shader. Scaled view textures
  use Rgba16Float when supported, including 8-bit documents, retaining composite coverage
  until screen quantization. Float interpolation avoids hardware subtexel-weight rounding
  at fractional zoom. Normal document textures keep their existing mip behavior. Move planes
  retain their existing callback and warm uniform path.
- Relevant supplied font/master/appearance generations are keyed per layer. Referenced
  pattern surface identities, text DPI, effect global light and document profile participate
  in cache identity. Unrelated pattern edits preserve projections. Conservative old/new
  damage updates affected view texture regions. Uncached detached shape bounds come from
  compiled geometry, so opacity/path edits do not disappear with an empty 1x cache. Supplied
  generations include both old bounds and newly projected coverage (including newly available
  glyphs). Source documents/history are not modified.
- Pixel/vector masks are baked once over the bounded visible apron, preventing the GPU
  mask planner from allocating a full scaled document. Effects/mask padding follows the
  longest ancestor chain. A 16 MP view/apron budget, 64x scale ceiling and the existing
  compositor's 512px effect-reach limit produce explicit errors. An unavailable feature
  projector or exhausted projection budget currently leaves the existing 1x app display;
  this limitation is not presented as successful scaled rendering.
  Large stroked paths also require a bounded-region shared shape splitter: the current
  splitter can otherwise allocate the whole magnified path despite a small viewport.
  The owned projection rejects that allocation and supplies the exact hook to 08/00.

No shared lib/canvas/plan/router, manifests, lockfile, model or translation catalogs were
edited. No dependencies, unsafe code, foreign implementation or reverse pc-vector→pathops
edge were added. There are no new UI strings or persisted settings. Exact native module,
provider, attribution and app call fragments are in
[`integration/09.md`](vector-tasks/integration/09.md); i18n fragment is header-only.

## Acceptance and verification

Final validation finished on 2026-10-10. Commands used Rust 1.98.1, `--offline`,
`CARGO_BUILD_JOBS=3`, one cargo build at a time. The machine's shared `li-rustc-gate` was
respected. No desktop windows were opened. Each changed crate's full suite ran once:

```text
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline \
  -p <crate> --no-fail-fast -- --nocapture --test-threads=3
```

| Crate | Reported passed | Failed | Ignored | Log under `target/` |
|---|---:|---:|---:|---|
| photocraft-vector | 90 | 0 | 1 | scaled-final-photocraft-vector.log |
| photocraft-text | 154 | 0 | 3 | scaled-final-photocraft-text.log |
| photocraft-compose | 207 | 0 | 0 | scaled-final-photocraft-compose.log |
| photocraft-gpu | 101 | 0 | 0 | scaled-final-photocraft-gpu.log |
| photocraft-ui-egui | 1095 | 1 | 13 | scaled-final-photocraft-ui-egui.log |

Counts include existing tests and repeated source-bridge registrations, not unique fixture
counts. The sole failure is `i18n::tests::every_tl_literal_is_translated`: the unchanged
shared node/pen sources have 14 Japanese catalogue omissions. Catalogue changes belong to
00, and 09 introduces no `tl!` strings. Exact missing keys for that lane:

```text
Add to selected curve
Align nodes horizontally
Align nodes vertically
Break Curve
Close Curve
Convert the live shape to curves to edit its nodes?
Convert to curves
Drag a corner to scale; Ctrl or Command rotates; Shift preserves proportions; Alt uses the center.
Join Curves
Polygon
Reverse Curve
Rubber band
Smart
Transform selected nodes
```

The 13 UI ignores include two owned dependency gates below and 11 existing integration,
benchmark or review gates. Vector's one ignore and text's three ignores are existing
benchmark/dump gates. Four additional nominally passing tests returned early because
optional craft-fonts are absent: text's `mincho_first_reorders_only_craft_families`,
`ui_order_prefers_biz_udpgothic_regular`, `craft_fonts_cover_japanese_without_system_fonts`,
and UI's `craft_fonts_render_japanese_without_system_fonts`. These are missing CJK corpus,
not successful font coverage. No owned RTX gate skipped or returned early.

All-target clippy passed for all five changed crates with `-D warnings`:

```text
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-vector -p photocraft-text \
  -p photocraft-compose -p photocraft-gpu -p photocraft-ui-egui --all-targets -- -D warnings
```

Log: `target/scaled-clippy.log`. Clippy found one test-module placement lint; moving that
module after its helpers resolved it. `cargo +1.98.1 fmt` passed for each changed package,
and `git diff --check` passed. No full workspace suite was run.

Development checks already establish the CPU analytic eight-column 800% hairline profile,
source preservation, post-transform text parity, visible clipping, mixed masks/effects and
tile equivalence. The strict real GPU mixed gate has run successfully at 2/4/8x on:
NVIDIA GeForce RTX 5090, Vulkan, DiscreteGpu, PCI 0000:01:00.0, NVIDIA 615.71.09,
Rgba32Float accumulation. Required adapter/format assertions cannot skip or accept llvmpipe.
Final mixed CPU/GPU maximum premultiplied differences were 0.0003659129 at 2x,
0.00036609173 at 4x and 0.00036612153 at 8x; tiled/untiled GPU differences were zero.
The independent eight-column GPU hairline profile passed. Full-app pixel, pan/zoom and
P3/HDR gates also passed in the final full UI suite. The opacity edit gate caught an
uncached-geometry damage bug, and fractional zoom caught intermediate rounding; both
were fixed and the same strict pixel oracle now passes. The existing Move GPU stack,
off-canvas raster and backdrop-dependent blend gates passed with the updated tile uniforms.

New gates supplied:

- Native vector/text/CPU view tests, including analytic pixels, tiled equivalence, bounded
  large-document masks, gradient pan stability, relevant dependency invalidation and HDR
  raster defaults/negative coordinates.
- `pc-gpu/tests/scaled_view.rs`: strict RTX/Vulkan CPU/GPU premultiplied parity ≤1/255,
  tiled GPU stitching and independent 800% GPU hairline column profile.
- Owned full-app offscreen GPU gates: 2/4/8x and fractional zoom, HiDPI, horizontal flip,
  warm cache reuse and one-layer damage; actual middle-button pan and pinch zoom events;
  P3 conversion and above-one 32-bit preview on float scaled textures.
- Full-app node drag preview/release/Escape gate and release 6000×4000/500-shape actual
  node pointer-frame benchmark are supplied but dependency-ignored. They require production
  detached node routing, not test-host injection. The benchmark records first/release costs,
  median/p95/p99, actual GPU/readback frames, hardware and uploads/rebuilds, and asserts
  warm median <16ms. No interactive performance result is claimed until it runs.

## Work still requiring integration

1. Package 00 must register native view modules and replace the temporary bounded app text
   provider with pc-text::view. The current source bridges compile the CPU/vector/GPU owned
   implementations without competing shared-file edits.
2. Package 00 must apply package 03's `frame`, `canvas_input` and `display_doc` production
   hooks from [`integration/03.md`](vector-tasks/integration/03.md). These are absent in this
   branch. Until then the new live node/cancel GPU gate and release pointer benchmark do
   not count as passed. Remove the live-node gate's dependency ignore after the hooks land.
   Keep the release benchmark ignored in ordinary debug suites, then run explicitly:
   `cargo +1.98.1 test --offline --release -p photocraft-ui-egui --lib scaled_release_6000x4000_500_shapes_node_pointer_frames_rtx5090 -- --ignored --nocapture --test-threads=1`
   Detached preview routing must also pass old/new preview damage to the normal refresh;
   otherwise its existing 1x full-document work can dominate before scaled preparation.
3. Package 08's enhanced appearance/corner/pattern-shape paint provider and 12/15/18–20's
   path-text/symbol/live feature projection must produce ordinary projected paths/layers
   with the documented generations before view projection. Missing enhanced geometry is
   refused, never treated as valid legacy output.
4. Crisp vector Move planes still require scaled geometry supplied to the existing plane
   path by feature integration. Preserving the existing cached planes alone does not pass
   this enhanced acceptance point.
5. Run the integrated node release benchmark and complete the final enhanced GPU/app
   gates before declaring this package done. Median/p95/p99 and <16ms are unmeasured here.

## Sources, attribution and corpus

Followed the existing canvas/Move implementation, Move reports, architecture §6 and §9.3,
persona owner decisions, quality-plan crisp zoom and package ownership contract.
Official API references:
[egui-wgpu 0.36.2 callback lifecycle](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/trait.CallbackTrait.html)
and [wgpu 30.0.1 encoder ordering](https://docs.rs/wgpu/30.0.1/wgpu/struct.CommandEncoder.html),
plus [WGSL texture loads](https://www.w3.org/TR/2026/CRD-WGSL-20260921/#textureload).
Scale bucketing, sparse tree projection and sampling gutters are original work using these
established APIs.

Synthetic fixtures and parameters/licenses are recorded in
[`09-SOURCES.md`](vector-tasks/integration/09-SOURCES.md). The text glyph drawer adapts the
local imported `pc-text/src/render.rs` snapshot at
`c18d18cf613422fe08d31001e1f803cf8ef7dd68`; its notice is retained and the exact PORTS row is
supplied to 00. The original PhotoCraft whole-app fork hash is not available in the
foundations and is not invented. No VectorCraft code/branding or proprietary goldens were
copied. Missing external/proprietary corpus and absent optional craft-fonts CJK files do
not count as successful gates.
