# Package 42 sources and fixture records

Implementation/fixtures are GPL-3.0-or-later Local Image except for the explicitly adapted
VectorCraft initialization/lifecycle logic, whose MIT OR Apache-2.0 notices are retained.
VectorCraft root notices are already in `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}`.
No upstream renderer, object tree, brand assets, dependencies or proprietary fixtures were added.

Read-only upstream source: `~/.local/share/local-image-dev/engine-sources/vectorcraft`, pin
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. Read `crates/engine/src/cmd/buildcmds.rs`
(livePaint make/merge/release/expand/fill/strokeEdge/info), `crates/tools/src/builder.rs`,
`crates/pathops/src/planar.rs`, and `crates/engine/src/tests_build.rs`. V1's already adapted
fallible `shape_builder`, `merge_regions`, `area`, `nearest`, `contains` and PSD-normalizing
geometry adapters are reused, never copied or replaced.

Official interaction references read 2026-10-10:

- [Affinity Vector Flood Fill Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_vectorFloodFill.html):
  click/drag; Inside/In-between; Add/Smart/Knockout; visible boundaries; R.
- [Adobe Live Paint overview](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/about-live-paint.html):
  dynamic regions, independently painted faces and edges.
- [Adobe Live Paint Bucket](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/paint-with-the-live-paint-bucket-tool.html):
  bucket input, edge modifier and sampling interaction.
- [Adobe shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html),
  also recorded by the gap audit, for K, Shift+L and Make.

Topology signatures, deterministic source slots, gap bridges, snapshot validation,
transparent flood-fill decomposition and app lifecycle adapters are original implementations.
Adobe does not specify numeric gap sizes; our explicit Small/Medium/Large policy is 4/8/16
document pixels, validated against the requested 3-pixel Small fixture. Sources remain unchanged.

All fixtures are generated in Rust tests (no files to license externally): two cubic circles
at (16,24,80,80)/(56,24,80,80), horizontal line (0,64)→(150,64); open 60×60 square with
a 3-pixel/5-pixel end gap; and a 100×20 rectangle split at x=50 and x=50.01. Own geometry,
GPL-3.0-or-later. Ellipses use pc-vector's established cubic approximation. Analytic circle
half/lens area allows <1 square pixel cubic approximation error; exact areas use V1.
Coverage oracle independently uses pc-vector's PSD Intersect coverage at 160×128. Pointer
tests use egui/egui_kittest 0.36 and F32 RGB documents; no desktop windows. Toolchain 1.98.1,
offline, shared `li-rustc-gate`, CARGO_BUILD_JOBS=3. GPU format Rgba32Float, premultiplied
max difference <=1/255; hardware gate RTX 5090, Vulkan, discrete. GPU/release gates deferred
by the owner's final speed rules, not counted as successful tests. No proprietary corpus
was supplied or counted.

Exact attribution fragments for 00 (one row per adapted local file):

| Local file | Project | Upstream file | Commit | License | Date |
|---|---|---|---|---|---|
| `crates/pc-vecfx/src/live_paint/mod.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/buildcmds.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-engine/src/vector_packages/live_paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/cmd/buildcmds.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |

UI, data, signature, gap and test files are original; they do not need invented port rows.
Retain this distinction when regenerating attribution assets.
