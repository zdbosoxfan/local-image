# Package 17 source and fixture record

Upstream source was read locally, read-only, at
`~/.local/share/local-image-dev/engine-sources/vectorcraft`, git HEAD verified as
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. Source URLs below are pinned, not latest.
Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors; MIT OR Apache-2.0.
Root notices are already retained in `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}`.

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-vecfx/src/brush/mod.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/brush/src/lib.rs`, `crates/doc/src/appearance.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/brush/track.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/brush/src/track.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/brush/calli.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/brush/src/calli.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/brush/warp.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/brush/src/warp.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/tests/brush/package17.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/brush/src/tests.rs` (adapted analytic cases plus original independent oracles) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |

The above is the exact PORTS append fragment for 00; the shared PORTS/catalog generator
is intentionally not edited in this package. Engine/UI code is original Local Image code
using existing pencil, tablet, vector_api, compose and egui_kittest interfaces.

Primary references:

- [Pinned brush source](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/brush/src).
- [Pinned width model](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/appearance.rs).
- [Affinity Vector Brush tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_Brush.html),
  read 2026-10-10: editable freehand variable-width strokes, pressure, width controls and
  rope/window stabilization. This is behavior reference; no proprietary brushes/assets copied.
- Local contracts `vector-architecture.md`, `vector-persona-spec.md` (Owner decisions),
  `vector-quality-plan.md`; existing package 05 sample/fitter/capture APIs and package 00's
  merged-appearance shape_split implementation.

Fixtures are generated in Rust at test time, under Local Image's GPL-3.0-or-later license;
no external/proprietary corpus is bundled. Parameters: horizontal lines 100/104/109 px,
ellipse nib diameter 10 px, angles 0/30/45/90 degrees, minor/major ratios 0/0.2/1;
20-px peak pressure profile with peak at 20% of a 100-px line; pattern 10x4-px side tile,
6x6-px corner tile, 5x2-px ends; art 10x2/10x4-px rectangle over right-angle and near-U-turn
paths; scatter 4x4-px art, seeds 7 and 8, four independent OS threads. Native roundtrip uses
the actual pc-format ZIP writer/loader, generated document only. CPU parity tolerance is
premultiplied max diff <=1/255. Tests use toolchain cargo/rustc 1.98.1 and existing locked
pc-vector, pc-pathops/kurbo 0.13.1/linesweeper 0.4.0, egui/egui_kittest 0.36, wgpu 30.

No external Affinity/Illustrator brush corpus, proprietary texture fixture or upstream CLI
raster golden is available here; none is counted as a passed gate. No performance claim is
made. GPU sources require RTX 5090 Vulkan discrete hardware and Rgba32Float; the owner
explicitly prohibited executing GPU/release/benchmark gates in this package run.
