# Symbols package source and fixture record

Date: 2026-10-10. Implementation and generated fixtures: original Local Image code,
GPL-3.0-or-later. No brand assets or proprietary document bytes were copied.

## Behaviour sources

- [Affinity Designer 2 Symbols](https://affinity.help/designer2/en-US.lproj/pages/SymbolsAssets/symbols.html), consulted 2026-10-10: create from selected objects, place instances, synchronized master changes, detach, and delete converting instances to ordinary objects.
- [SVG 1.1 symbol/use](https://www.w3.org/TR/SVG11/struct.html#SymbolElement), W3C Recommendation 2011-08-16: reusable template content and per-instance transformation/expansion. This package supplies expansion; packages 13/14 own SVG syntax and viewBox handling.
- Local Image vector architecture section 3 and V8 persona/quality specifications in this repository.
- VectorCraft read-only reference at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0, `crates/doc/src/lib.rs` and symbol tool/engine call sites: behavioural/type reference only. No source file was adapted or copied. Retain existing upstream notices; no additional PORTS row or attribution JSON entry is claimed.
- egui/egui_kittest 0.36.2 cached official crate source: DragValue accessibility roles, real pointer/text events, `Harness::build_eframe`, and wgpu offscreen rendering. Existing app Move and drawing GPU harnesses supplied the app callback/readback contract.

## Reproducible synthetic fixtures

All fixture art is generated directly in the checked-in test source. No image generator,
ComfyUI, reference binary, network font or external asset is needed.

| Fixture | Parameters | Source/license | Oracle |
|---|---|---|---|
| Three linked badges and detach | 128×96 RGB U8; 12×10 rectangle at (2,3); instances offset (0,0), (30,20), (60,20); original rgba=(.8,.2,.1,.75) | `symbols/tests.rs`, original GPL-3.0-or-later | Independently constructed ordinary rectangles, exact pixel arrays |
| Placement family | 256 integer translations on a 16×16 grid; same badge | original GPL-3.0-or-later | Ordinary rectangle coordinates derived without symbol expansion |
| Linked text | Three “BBB” labels, size12, at (2,20), (32,40), (62,40), from edited “AAA” master | original GPL-3.0-or-later | Separately created ordinary type layers, exact pixel arrays |
| Affine family | uniform 2× scale and 90° turn `[0,1,-1,0,30,10]` | original GPL-3.0-or-later | Independent rectangle positions/sizes |
| Hostile graph | self/nested cycles; duplicate namespaces; missing references; depth >64; 2-way fanout across 21 masters; NaN/Infinity/huge/singular numbers | original GPL-3.0-or-later | Error before document/revision/journal changes |
| Native bundle | 24×24 RGB U16; sparse raster master ID17, affine offset (2,3), cache; ZIP and directory | `pc-format/src/convert.rs`, original GPL-3.0-or-later | Exact document/cache equality and disjoint LayerId remap |
| Frozen v1 | Existing `pc-format/tests/fixtures/vector-v1.json` and the existing vector-foundations fixture record | Existing committed source/license record retained | v1 migration changes only version; frozen shape/mask/path data loads |
| Kittest and app | Panel 680×650 with HDR red=1.5 master metadata and no-op Apply; full app 1200×900; document 128×96; logical zoom 1/2/4/8 | original GPL-3.0-or-later | Real click/drag/type, transient history checks, actual GPU texture versus ordinary CPU compositor |
| Release pointer timing | Full app fixture above; zoom2; 20 warmup +128 measured pointer frames; GPU callbacks and screenshot readback included | original GPL-3.0-or-later | Hardware identity asserted; p50/p95/max, no extrapolation to large artboards |

Tools: Rust/Cargo/rustfmt/clippy +1.98.1, offline locked dependency graph; egui/egui_kittest 0.36.2 and pinned workspace wgpu. GPU gates require NVIDIA RTX 5090, Vulkan, discrete device, Rgba32Float accumulator, premultiplied max difference ≤1/255. Exact observed driver details and test counts are recorded in the package report after execution.

Missing corpus: proprietary Affinity symbol files and external SVG symbol corpus are not present in this package, and do not count as passed gates. Final scaled geometry, advanced appearance/path text/live projection and file adapter round trips remain named dependency integration gates (09, 08/06/12/18–20, 13/14/21/23).
