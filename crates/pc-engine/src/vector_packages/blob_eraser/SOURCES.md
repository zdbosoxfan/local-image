# Package 40 sources and fixtures

- Engine transaction, connected contour/hole grouping and merge semantics adapted from
  VectorCraft `crates/engine/src/cmd/draw2.rs`, pinned
  `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0,
  copyright 2026 ArtCraft Team and contributors. Existing notices in `licenses/vectorcraft-*`.
  Read from `~/.local/share/local-image-dev/engine-sources/vectorcraft`.
- Open-curve cutting follows the integrated package 10 arrangement/cut_out pattern,
  reusing V1 public intersection and exact cubic cut APIs instead of VC's 24-point sampling.
- Public 05 fitter/stabilizer/Sample and 17 pressure_profile/BrushProjection APIs are
  consumed without modifying their files. Package 11 supplies non-brush projections.
- Behavior verified against Adobe's official
  [Blob Brush](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/merge-paths-using-the-blob-brush-tool.html),
  [Blob Brush options](https://helpx.adobe.com/illustrator/desktop/paint-and-fill/learn-painting-basics/blob-brush-options-and-best-practices.html),
  and [Eraser](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/erase-paths-using-eraser-tool.html)
  documentation, accessed 2026-10-10. These are behavior references; no artwork copied.

All `tests.rs` and GPU fixtures are original synthetic GPL-3.0-or-later source records,
generated at test runtime. Rectangle 100×100, sweep diameter 10, capsule length 80,
compound 120×120 with a 40×40 hole, cubic controls explicitly recorded in test source,
0.02 px narrow-cut rectangle. No proprietary corpus or external goldens are present.
Independent oracles are analytic area, source/sweep raster coverage multiplication,
nearest distance to the original cubic, exact endpoint spans, and undo document equality.
Toolchain Rust 1.98.1; V1 linesweeper 0.4.0 / kurbo 0.13.1; egui 0.36.
