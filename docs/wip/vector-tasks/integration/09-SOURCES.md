# Scaled canvas sources and synthetic fixtures

All fixture inputs are original synthetic Local Image geometry, GPL-3.0-or-later. No
proprietary corpus, brand imagery, external fonts or vendor goldens are represented as
passed. Bundled Inter resolves through the existing pc-text font registry and license.
The text fixture uses `assets/fonts/Inter-Regular.ttf`, SIL Open Font License 1.1
(`assets/fonts/OFL-Inter.txt`), SHA-256
`40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82`.

- Local baseline `4a4af2ed`: `pc-ui-egui/src/gpu_canvas.rs`, `move_fast.rs`,
  `pc-compose/{lib,proxy,masks,effects,shape_split}.rs`, pc-vector exact-area renderer,
  pc-text `render.rs`, pc-gpu compositor/planner and plan cache.
- `docs/wip/CODEX-REPORT-group-move-shape-rerasterise.md` and Move reports guide preservation
  of the existing cached-plane behavior. Their timings are not new measurements here.
- Official pinned callback lifecycle and command ordering:
  https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/trait.CallbackTrait.html
- Official pinned encoder/texture-copy lifecycle:
  https://docs.rs/wgpu/30.0.1/wgpu/struct.CommandEncoder.html
- WGSL textureLoad/textureDimensions and float mix semantics, 2026-09-21 draft:
  https://www.w3.org/TR/2026/CRD-WGSL-20260921/#textureload
- Architecture section 6, persona specification section 9.3 and quality-plan crisp zoom.

Fixtures: 1px vertical line centered at x=4.5, y=1..8 at 8x (analytic eight-column oracle);
fractional triangles across four 64px tiles; 80x60 rectangle with clipped red raster and
vector mask; default drop shadow and feathered pixel mask; 96x80 RGBA8 mixed GPU document
at 2/4/8x; 160x120 offscreen-app triangle at 2/4/8x plus 2px/point HiDPI/flip; Inter "ABC"
at 72 DPI/12pt and 4x; fractional app zoom 1.7 at 1/2px per point; Display P3
(0.8,0.5,0.3); F32 shape RGB(2,2,2) with -3-stop preview; 6000x4000/500
20px square shapes on a 32px grid for the dependency-pending node benchmark.
Tests use Rust 1.98.1 offline, existing locked egui 0.36.2/wgpu
30.0.1 and exact-area rasterizers. GPU gates require RTX 5090, Vulkan, DiscreteGpu and
Rgba32Float accumulation; no skipped adapter can pass.
