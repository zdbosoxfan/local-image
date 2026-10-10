# Width package sources and fixture records

All synthetic geometry and oracle fixtures in this package are original Local Image
code (GPL-3.0-or-later), generated in tests, without external assets or network access.

Implementation references read at the pinned source checkout:

- VectorCraft `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0,
  `crates/doc/src/appearance.rs`: linear interpolation, seven preset shapes, named profiles.
  Local Image stores **absolute** side distances; upstream stores weight factors.
- Same pin, `crates/effects/src/stroke/width.rs`: y-down left normals, marked corners,
  profile-point insertion, outer joins, inner overshoot detours, caps, closed rings.
  Local changes: finite and allocation budgets; V1 union cleanup; adaptive round caps;
  native constant-profile tessellation; original-distance dash spans; incoming t=1 versus
  outgoing t=0 widths at a closed seam.
- Same pin, `crates/tools/src/distort/width.rs`, `crates/engine/src/tests_widthtool.rs`:
  add at path, move centre, symmetric drag, Alt one-side drag, remove, single commit.
  Duplicate positions/step profiles are not supported by Local Image's strictly ordered
  shared appearance contract; moving onto another point replaces it atomically.
- [kurbo 0.13.1 ParamCurveArclen](https://docs.rs/kurbo/0.13.1/kurbo/trait.ParamCurveArclen.html):
  arc length and inverse arc length for handle placement, tolerance 1e-7 document pixels.
- [Inkscape 0.91 PowerStroke behavior](https://wiki.inkscape.org/wiki/Release_notes/0.91#PowerStroke):
  editable stroke knots, sorting along path, add/delete, caps and joins; behavior only.
- `docs/specs/vector-quality-plan.md` width oracle: independent triangle area and native coverage.

Geometry parameters: 100px straight lens, maximum 10px per side, analytic area 1000px²;
constant 10px total strokes, all 3 caps × 3 joins on line/V/closed rectangle;
unequal 10px+90px segments; collinear cubic x(u)=100u³; tight 70px zigzag with
5px spacing and wave side distances up to 15px; 20px dash/gap with taper, area 600px².
Closed-seam oracle: a 400px square perimeter with 1/1px start and 10/2px end widths;
independent containment samples on both sides of the seam for all three joins.
Projection tolerance 0.01px, boolean precision 1e-8px. No competitor goldens are bundled.
GPU fixtures: synthetic 160×120 strokes, real RTX 5090/Vulkan/discrete, Rgba32Float
compositor capability. GPU tests are integration gates and not executed under the
2026-10-10 task speed override. No GPU or interactive-speed claim is made.

Tool versions: cargo/rustc 1.98.1; locked kurbo 0.13.1, linesweeper 0.4.0, egui/kittest 0.36.2.
Retain `licenses/vectorcraft-LICENSE-MIT`, `vectorcraft-LICENSE-APACHE`, `vectorcraft-NOTICE`.
No proprietary fixtures were supplied or counted as passing gates.
