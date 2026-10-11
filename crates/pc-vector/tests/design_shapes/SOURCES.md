# Live-shape fixtures and oracles (package 02)

All fixtures are synthetic Rust data in `geometry.rs`; no owner, Adobe, or Affinity files are used.
Original fixture code is GPL-3.0-or-later, like this repository. There is no external corpus to count as passed.
Generated on demand with Rust 1.98.1, f64 geometry in document pixels, default rectangle 0,0,100,100,
rotation 0 and pi/2. Tests list each family's exact parameters. Raster hole checks use pc-vector's
existing exact-area renderer at 0.01 px tolerance, a 2x2 region at (49,49).
The 256 deterministic rotation cases cycle the 25 fixtures with LCG seed 0x0202 and sample angles
within +/-50 turns. Angle-handle regressions cover clockwise, wrapped and full-turn sweeps.

Reference code: VectorCraft at d522c1d7be4035bd4f4a84cd6ebfca44f5155092,
`crates/geom/src/shapes.rs` and `crates/tools/src/shape.rs`, read from the pinned local checkout
`~/.local/share/local-image-dev/engine-sources/vectorcraft`. Copyright (c) 2026 ArtCraft Team
and the VectorCraft contributors; MIT OR Apache-2.0. Retained license texts/notices already exist in
`licenses/vectorcraft-*`. PORTS rows are handed to 00 in `integration/02.md`.

Behavior references (official help, read 2026-10-10):
- https://affinity.help/designer2/English.lproj/pages/Tools/tools_star.html (inner radius, on-canvas handles, convert)
- https://affinity.help/designer2/English.lproj/pages/Tools/tools_cog.html (teeth, inner/hole radii, curvature)
- https://affinity.help/designer2/English.lproj/pages/Tools/tools_squareStar.html (square-ended protrusions)
- https://affinity.help/designer2/English.lproj/pages/Tools/tools_segment.html (chord-cut ellipse behavior)

Analytic oracles use Green's theorem, shoelace/radial polygon areas, circular sector/chord areas,
equal-circle lens area, and polar area integration. Independent dense cubic quadrature uses 1024
samples per segment rather than reproducing the generator. Rectangle Kappa arcs have the conventional
~0.028% circle area error; ellipse/sector arcs subdivide at 45 degrees for tighter small-segment areas.
The general area tolerance is 0.04% plus 0.1 px². Grid tests count cell dividers
and measure exact Euclidean segment lengths. Holes test actual raster coverage, not only enums.
Every family's geometric bounds also compare dense cubic boundary samples to independent mathematical
extrema (0.025 px tolerance for cubic approximation). Heart/tear use polynomial derivative roots;
cloud uses the polar-coordinate derivative root; spiral uses exponential-spiral coordinate extrema.

Original extended templates not present in pinned VectorCraft:
- Cloud radius is 0.85+0.15*cos(bubbles*theta), area pi*(0.85²+0.15²/2), scaled by rx*ry.
- Heart consists of four cubics in [-1,1] nominal coordinates; exact area is
  (1567-350*depth)/500 * rx*ry. Tear consists of two cubics, area 219/100 * rx*ry,
  invariant under its shear-like bend. Exact rational Green integrals were independently computed
  with Python 3's fractions.Fraction; tests contain numerical moment oracles, not copied paths.
- Rectangle callout with zero radius and bottom-center tail at y=130 adds a triangle of area 300
  to the 100x100 body. Ellipse callout boundary segments are split at 0.4/0.6 by de Casteljau.
- Segment model has start/end angles plus inner ratio: inner=0 is an ellipse segment closed by
  its chord; positive inner gives an annular sector. This is the existing shared model's chosen
  interpretation, not Affinity's independent upper/lower-line model. No shared data type was altered.
- Generic polygon/star `corner` is a quadratic fillet with bounded edge cuts, represented exactly
  as cubics; it is not a claim of V1 circular Corner-tool geometry.

No brand assets, proprietary fixtures, screenshot goldens, or downloaded third-party artwork.
RTX parity and full-app acceptance are reported separately; a missing app route is never a pass.

Native compatibility testing reads the existing frozen `pc-format/tests/fixtures/vector-v1.pcraft`,
an original GPL-3.0-or-later Local Image fixture generated on 2026-10-10 by
`pc-format/examples/vector_arch_fixture.rs`. Its adjacent `SOURCES.md` records the version-1 schema
and exact contents. The checked-in fixture was read without regeneration or modification.
