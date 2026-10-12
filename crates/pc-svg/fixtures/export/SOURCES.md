# SVG export fixture and oracle records

All fixtures in `tests/export/` are authored for Local Image and licensed
GPL-3.0-or-later. No brand assets, proprietary documents or downloaded logos are
used. Inputs are generated at test time, so no image golden is confused with an
independent reference.

- Geometry: native rectangle and cubic ellipse, 256×256 pixels, RGB/U8, 96 DPI;
  rectangles (16,20,224,216), ellipses with the same frame, and a 3-knot closing
  cubic (last outgoing 50,250; first incoming 5,100). Ordinary paint defaults are
  written in each test. The fixtures use pc-vector's existing shape generators.
- Gradients: Linear/Reflected/Radial, 0/31/90 degrees, reverse, scale 0.8,
  offset (0.1,-0.05), unsorted stops, midpoint 0.3, independent 0.5→1 opacity.
  Reference: resvg **0.45.1**, usvg **0.45.1**, tiny-skia **0.11.4**, parsed
  with explicit 96 DPI, transparent canvas, identity render transform. Compare
  8× supersampled opaque coverage IoU>0.995 (integrated min/max fractional
  coverage, as required by the quality plan) and output-resolution mean
  DeltaE_ok<0.5 over white, using the existing
  pc-testkit Oklab metric. resvg output is premultiplied; pc-compose is straight.
- Additional generated cases: subtract/even-odd/inverted rectangle holes
  (24,24)–(232,232), hole (64,64)–(192,192); duplicate gradient stops at 0.5,
  colour alpha 0.6/0.9 and independent opacity 0.7→1 over a fractional rectangle,
  with canvas-aligned gradient geometry; translucent 48pt HH at 60% alpha with
  strikethrough, checking fully covered glyph/decoration overlap interiors.
  Roundtrip byte stability covers solid, linear/reflected/radial and reversed
  midpoint/independent-alpha gradients, with and without a hard clip.
- PNG fallback: native angle gradient, image **0.25.10** PNG encoder and decoder;
  inspect the embedded base64 signature and decode pixels against the independent
  pc-compose result. The pinned resvg feature graph has no raster-images: enabling
  that feature adds gif 0.13.3 / zune-core 0.4.12 / zune-jpeg 0.4.21, which this
  package is forbidden to add. This PNG decoder check is recorded separately
  from the resvg vector gate, never as a passed resvg PNG gate.
- Text: existing bundled `assets/fonts/Inter-Regular.ttf` (SIL OFL-1.1,
  existing repository font attribution), 48pt, red H/blue i, translation (20,100).
  Font SHA-256: `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82`.
  Uses pc-text public layout/outlines, no second shaper, system-font enumeration,
  font file addition or external-font resolution. Editable text warns that
  viewers need the named font; curves retain positioned glyph geometry.
- Property tests: proptest **1.11.0**, default **256 cases**, x/y -10..100,
  width/height .01..100, precision 0..9, XML and deterministic-byte oracles.
- UI: egui/egui_kittest **0.36.2**, 700×600 offscreen host; actual selection,
  artboard, curves/minify/export/cancel clicks and typed precision. Write service
  records bytes in memory. No desktop windows or owner file writes.
- GPU gates: strict RTX 5090, discrete Vulkan, Rgba32Float; 128² library
  roundtrip and 256² real PhotocraftApp canvas with actual Move pointer press,
  motion, release, export command and public import, max premultiplied
  channel difference <=1/255. Missing package-13 registration is a dependency,
  not a successful skip. No performance claims are made.

## Implementation references

Read-only source: VectorCraft at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`,
`crates/svg/src/export.rs` (path_d, gradient_def, stacked shape/text writer),
source SHA-256 `cc4d2b2f32bcc6f67ef77d108742ee8f3752498f6d735562759f51eae018268a`,
MIT OR Apache-2.0. Existing `licenses/vectorcraft-*` notices are retained.
The document adapter, scope validation, cache bounds, transaction guards and
projection-provider bridge are written for Local Image rather than transplanted
from its object tree.

Official specification references consulted 2026-10-10:
[paths/closepath](https://www.w3.org/TR/SVG11/paths.html#PathDataClosePathCommand),
[painting/dashes](https://www.w3.org/TR/SVG11/painting.html#StrokeDasharrayProperty),
[gradients/units](https://www.w3.org/TR/SVG11/pservers.html),
[text/textPath](https://www.w3.org/TR/SVG11/text.html#TextPathElement).
Native gradient layout/ramp uses pc-vector's pinned PhotoCraft #1321 projection
(`d438f842256435dba7aa41ca306656a216b98fbf`) directly, including pixel-corner
sampling, midpoint/alpha and offset behavior. Linear SVG endpoints receive a
(0.5,0.5) pixel-centre translation; its perpendicular component does not affect
paint and keeps native whole-pixel endpoint snapping stable on reimport. Only
colour-alpha times independent-opacity products require extra ramp subdivisions;
already normalized SVG stops retain their count. These serialization choices
are original Local Image code based on the native sampler and SVG interpolation.

## Reproducing disjoint integration checks

`bootstrap.py` writes only `target/svg-export-workspace/`: copied manifests,
symlinked source, temporary SVG lib/test registrations and UI module registration.
The real manifests, lock, UI router and catalogs stay owned by package 00.
Use `--manifest-path target/svg-export-workspace/Cargo.toml`, `--offline`,
`CARGO_BUILD_JOBS=3`, and Rust 1.98.1. It preserves the pinned lock and default
features. Optional `SVG_EXPORT_IMPORT_SOURCE=/absolute/path/to/13/src/import.rs`
copies that real public importer read-only into target/svg-export-workspace/
svg-import-source and records its source-set SHA256, so a concurrent sibling builder
cannot change a probe halfway through compilation. Optional importer-only wiring
includes the already locked simplecss 0.2.2 used by package 13. The snapshot is never
modified or replaced with an exporter-owned importer. Its public module is
re-exported from a temporary dependency crate, keeping isolated package-14
all-target linting separate from package-13 WIP. No lint allowance is introduced;
the combined shared-crate lint gate remains an integration requirement. Record the exact source
revision/working-tree status if used, and do not describe a sibling WIP probe as
merged acceptance.


## Cropped raster-fallback gate (FS007)

The cropped PNG fallback fixtures require installed **Inkscape 1.4+**. They use
`inkscape` on PATH by default; set `LOCAL_IMAGE_INKSCAPE` to an executable path
when needed (including Windows CI). Both version and rendering invocations use
that executable, with
`--app-id-tag`, `--export-type=png`, `--export-area-page`, `--export-dpi=96` and
transparent export background. The inspected validation host has **Inkscape 1.4.4
(dcaf3e7d9e, 2026-05-05)**. Missing Inkscape, unsupported flags, timeout or failed
output fails the test explicitly; there is no successful skip. No Cargo feature,
dependency or version was added.

Each invocation uses a unique alphanumeric application-ID tag, isolated
XDG_CONFIG_HOME/CACHE_HOME/DATA_HOME/STATE_HOME/RUNTIME_DIR and TMPDIR, and removes
DISPLAY, WAYLAND_DISPLAY and session-bus addresses. It never requests a GUI or an
active window. Only its own child is killed on the 30-second timeout. SVG, PNG,
version and process logs remain in `target/svg-crop-qa/<unique-tag>/` for review.

These are **native output-resolution** raster/effect fixtures. Inkscape PNG and
pc-compose RGBA are both decoded as straight alpha; compare integrated coverage
IoU>0.995 and mean DeltaE_ok<0.5 over white. The vector-only 8x opaque coverage gate
above remains unchanged: scaling its paths without scaling baked masks/shadows
would not be a valid raster-fallback reference. Each sparse-icon embedded PNG is
also decoded and checked byte-for-byte against its original isolated layer at the
SVG-declared x/y/width/height, with nonempty art and bounded summed image areas.

The rotated warped-text fixture uses bundled default type at 28 pt, a 0.2-radian
rotation and a 60% arc warp on a 512px canvas. It requires ink beyond unwarped
bounds, one cropped PNG matching the full warped ink rectangle, exact embedded
pixels against the original compositor crop, and the same native Inkscape gates.


## Native layer blend retention (FS008)

Synthetic GPL-3.0-or-later fixtures in `tests/export/blends.rs` retain twenty
small vector rectangles, their vector backdrop and a text layer beneath an
overlapping translucent blended shape. The 14 admitted non-Normal standard
modes are independently rendered with Inkscape at native 96 dpi (configured as above),
using the existing unchanged pixel/coverage thresholds. Structural assertions
require retained paths/type and native blend resources/styles, with no images
for vector-only cases. Additional fixtures distinguish isolated and pass-through
groups (including group fill opacity), and masked raster sources with Multiply
and Screen whose blend must apply exactly once against the original backdrop.

Negative controls retain warned whole-composition rasterization for Photoshop
Soft Light, Linear Dodge, Blend If, an adjustment, clipped stacking and a blended
layer with effects. Soft Light is deliberately excluded from native mappings:
the compositor uses Photoshop's square-root variant whereas CSS/PDF specifies
a different low-backdrop branch. Unsupported effects/artboard blend interactions
remain conservative; this change does not claim portable adjustment semantics.
