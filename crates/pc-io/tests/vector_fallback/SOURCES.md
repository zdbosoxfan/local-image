# Package 23 fixture and reference sources

All fixtures here are original Local Image GPL-3.0-or-later test code, generated
at test time. No brand/proprietary assets are bundled and tests never use network.

- `data.rs`: 32×24 RGBA8, manually specified translucent cache samples independent
  of the legacy mirror; four paint items plus hidden/profile/brush/arrow/stroke
  options; nested blend/repeat/envelope metadata. Expected feature keys are a
  hand-written specification table. Existing pc-io block parsers and merged PSD
  channels are independent oracles for block/cache output. These are **proxy/data
  tests**, not proof of an unmerged feature renderer.
- `roundtrip.rs`: 96×64, ellipse (12.25,10.5,40,36), two fills/two strokes of widths
  8 and 3, multiply item opacity .5 and stroke opacity .7. Package 08 generates
  native appearance caches. PSD and PSB at U8/U16/F32 round-trip caches, byte-level
  vector blocks, hidden/empty stacks; premultiplied max diff <=1/255. Synthetic
  text/symbol caches are explicitly injected format proxies.
  The separate shape-mask case uses a (12,10,20,36) rectangle, density .65 and
  feather .5, evaluated by the native combined-mask renderer. A 30-degree
  gradient overlay checks the source effect frame, including Move (7,5) and
  re-export. Pixel blending metadata survives proxy invalidation.
- `app_gpu.rs`: real 256×256 PhotocraftApp; package-08 multi-paint rectangle,
  package-12 `ABC` on ellipse (110,30,100,100), package-15 circle symbol instance
  translated (140,160), ordinary stroke effect. The existing bundled/system text
  engine resolves fonts; no new font fixture is copied. Actual Save As/dismissal
  clicks and reopened Move (12,8) preview/release. RTX 5090, Vulkan, discrete and
  Rgba32Float are mandatory. Canvas premultiplied max <=1/255; no timings claim.
- `bootstrap.py`: reproducible serial-00 activation of the exact integration
  fragments on private source copies/symlinks under `target/psd23-workspace`.
  Includes IO/importer and CPU/GPU rendering-kind hooks, preserving native type
  gamma and shape stroke-effect rules after raster fallback. The full-app fixture
  retains its original outside stroke effect and default text blending gamma.
  Python is out-of-build registration tooling; all production code remains Rust.
  Uses package 21's existing manifest fragment because its merged source has no
  Cargo.toml in this branch. No original shared file, dependency pin or owner file
  is mutated. All cargo commands are 1.98.1, --offline, CARGO_BUILD_JOBS=3.

Established implementation references: `pc-io/src/vector_map.rs` unchanged-model
raw block guards; `pc-io/tests/it/vector.rs` PSD round-trips and Photoshop corpus;
`pc-vector/src/appearance.rs::mirror_legacy` (package 08); package 12 path-text and
15 symbol common cache providers; `pc-ui-egui/tests/gpu/vector_integration.rs`
and existing notice/Save As flow. Keep their PhotoCraft/VectorCraft attribution.

Official format reference: Adobe Photoshop File Formats Specification,
November 2019, https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/.
Consulted 2026-10-10. Marker semantics are original Local Image architecture §7
policy, not an Adobe editable feature or a copied upstream format extension.

External Photoshop corpus expected at `corpus/psd`; missing in this worktree and
no licensed copy found in the available local reference paths. Both named corpus
gates must run when supplied. Missing corpus never counts as success. Native
11/17–20 renderer acceptance remains dependent on those packages' integration.

Before the renderer hooks, the real-app stroke-effect fixture failed native/reopen
pixel parity (max .599215686); all individual source/imported cache pixels matched.
A color-overlay diagnostic still failed (max .155005217), identifying the independent
path-text gamma loss over white. Those failures were corrected by retaining the
source rendering kind, rather than changing the tolerance, background, text gamma,
or native appearance algorithms. The final fixture again uses the outside stroke
effect. Diagnostic evidence is in `target/psd23-stroke-effect-failure.log`; the
final real-GPU evidence is in `target/psd23-gpu.log`.
