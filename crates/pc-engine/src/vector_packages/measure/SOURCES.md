# Package 44 sources and synthetic fixtures

Distance signs, Shift 45° behavior, persistent session-only readout, and overlay
interaction are adapted from VectorCraft `crates/tools/src/xform/measure.rs`, pin
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0), copyright
2026 ArtCraft Team and contributors. Read the pinned local source in
`~/.local/share/local-image-dev/engine-sources/vectorcraft`. Existing licenses
and NOTICE remain in `licenses/vectorcraft-*`. No artwork or brand assets copied.

Behavior references accessed 2026-10-10:
- [Affinity Measure](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_measure.html)
- [Affinity Area](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_area.html)
- [Illustrator distance](https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-distance-between-two-points.html)
- [Illustrator area](https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-area-of-objects.html)

Affinity distinguishes area excluding holes, area including holes, total perimeter
and Full edges/Cusped segments. The task explicitly requires selected-object sums;
Illustrator's overlap union is therefore not used. Group+child targets are deduplicated.
Local Image interpretation: a smooth knot continues a cusped run; corner knots end it.
Counterclockwise angles in a y-down document follow the pinned VC convention.
Hard Shift constraint preserves raw vector magnitude, as VC `geom::constrain_angle`
in `crates/geom/src/lib.rs` at the same pin; 07 remains the snapping query provider.

The existing V1 `finish_compound`, `area`, `nearest`, `bounds`, and `to_geometry`
handle fill topology and exact cubic geometry. `PathData::length` calls kurbo
`ParamCurveArclen` at 1e-6 absolute accuracy (source inspected in
`pc-pathops/src/geom/path.rs`). Unit conversions reuse package 16's exact 72 pt/in
and 25.4 mm/in definitions. Native live circles use analytic PI and TAU only when
metadata regenerates the authoritative path exactly; arbitrary cubic paths retain
cubic lengths. No circle recognition approximation is invented.

Tests generate original GPL-3.0-or-later fixtures at runtime: 100×50 px rectangles,
100×100 outer/50×50 square hole, 100 px diameter live circle, 30×40 straight line,
three-segment 50+50+20 px cusped run, linear cubic controls at 0/10/20/30, overlapping
rectangles, node/edge/guide/page hits, and 1:100 drawing scale at 300 DPI.
GPU source generates a 400×300 F32 white document with one black rectangle and
requires the real production canvas hook. No external or proprietary corpus supplied.
Independent oracles: shoelace square areas, Pythagorean distance, PI circle area/TAU
circumference, exact straight lengths, dimensional scale powers, native byte equality,
real pointer events, and premultiplied CPU/GPU pixel comparison.
Rust 1.98.1; egui/egui_kittest 0.36; kurbo 0.13.1; linesweeper 0.4.0.

## PORTS fragments for 00 (one per adapted implementation file)

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-engine/src/vector_packages/measure.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/xform/measure.rs`, `crates/geom/src/lib.rs` (constrain_angle) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/measure.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/tools/src/xform/measure.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |

Regenerate attribution with the existing VectorCraft license/NOTICE entries; no new
third-party dependency or attribution asset is required. Test files are original.
