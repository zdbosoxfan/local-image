# 43 — source and fixture records

Local implementation date: 2026-10-10. No downloaded corpus, proprietary artwork or brand assets.

- VectorCraft pin `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, read from
  `~/.local/share/local-image-dev/engine-sources/vectorcraft`: `crates/doc/src/live.rs`
  (GradientMesh, edge handles, split insertion, deletion, highlight),
  `crates/tools/src/meshblend.rs`, `crates/tools/src/meshedit.rs`,
  `crates/engine/src/cmd/live.rs`, `crates/effects/src/live.rs`.
  MIT OR Apache-2.0; existing `licenses/vectorcraft-*` notices retained.
- [Adobe mesh overview](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/create-and-edit-meshes/mesh-objects-overview.html)
  and [tool reference](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html): interaction reference, no copied prose/artwork.
- Adobe PDF 1.7 reference link in the task was unavailable to the web reader. Used the official
  [ISO 32000-1 specification](https://developer.adobe.com/document-services/docs/assets/35e4369068f86065372c18787171a17e/PDF_ISO_32000-1.pdf),
  §8.7.4.5.7/8: bicubic Coons boundaries and tensor-product nets. No PDF writer copied.
- [krilla 0.8.2 paint API](https://docs.rs/krilla/0.8.2/krilla/paint/index.html)
  exposes linear/radial/sweep gradients and patterns, without a public type 6/7 mesh paint.
  Thus this pin requires raster fallback; no hand-written PDF object insertion or new dependency.
- Local `pc-color::convert::{srgb_to_linear,linear_to_srgb}` and `dither_noise` are reused.
  The colour oracle explicitly differs from upstream's sRGB flat-quad interpolation.
- Engine preparation reuses public `styles::Projection` (existing width/brush providers),
  `corners::project_shape` and the appearance legacy mirror; no outline algorithms copied.
- Verified the installed egui 0.36.2 `ui.rs` implementation and
  [float colour-picker API](https://docs.rs/egui/0.36.2/egui/struct.Ui.html#method.color_edit_button_rgba_unmultiplied):
  it expects linear RGBA. The mesh widget converts its sRGB wire colours through the shared
  pc-color functions before and after the picker, retaining straight alpha.
- Read 20's `pc-vecfx/src/envelope/mesh.rs` and `coons.rs`. Their evaluators are private;
  the mesh evaluator is Catmull-Rom and the outline Coons evaluator does not expose mesh
  handles. This package adapts VC's gradient-mesh-specific geometry, without modifying or
  duplicating 20's outline partition/grid algorithms.

All new fixtures are original GPL-3.0-or-later Local Image test code. Tools: Rust 1.98.1,
existing egui/egui_kittest 0.36.2 and workspace wgpu. Deterministic parameters:

- CPU: 100×100 rectangle/circle, 1×1/2×2/3×3 patches; four RGB/white corner colours;
  independent analytic bilinear linear-light oracle; alpha 0–1; insertion u=.37/v=.62;
  regular and moved handles; shared dither seed 0 and 738; scales 1/2/4/8.
- Engine: two 90×90 shapes inside 100×100 F32 RGB document, IDs allocated normally;
  old `pc-format/tests/fixtures/vector-v1.pcraft` reused read-only.
- UI replay: 120×120 F32 document, 100×100 mesh at (10,10), 2×2 patches, 600×800 harness;
  actual U key, press/move/release, Shift/Alt/Escape/focus, swatch, actual colour-picker channel drag and opacity slider.
  The synthetic read provider supplies editable input to the controller. These tests
  check emitted captured requests, **not** native publication or production router acceptance.
- GPU: ignored offscreen integration gates require RTX 5090/Vulkan/discrete/Rgba32Float;
  engine 1/2/4/8 scaling through the required 09 projection hook; app 1280×800 with preview/release.
  Benchmark fixture: 20×20 patches, 120×120 document, 60 real-app pointer frames and GPU
  readbacks, median/p95/p99 and release time. It requires release mode. Not executed here.

No external mesh goldens or licensed Illustrator/PDF/PSD mesh corpus were supplied. They
are missing coverage, not passing gates. No measurements or hardware claims are inferred
from compiling the tests.
