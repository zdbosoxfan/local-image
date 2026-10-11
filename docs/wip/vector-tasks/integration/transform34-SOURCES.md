# Transform 34 source and fixture records

Synthetic fixtures were authored for this package; license GPL-3.0-or-later with Local Image.
No proprietary/external corpus or application output is claimed. All tests are offline.

- Square: (20,20)..(40,40), 4px stroke; UI square (20,20)..(100,100); translated copies at
  (10,5). Group fixtures are ordinary shapes with independently fresh layer IDs.
- Trapezoid oracle: source [0,0,100,100], destination [(20,0),(80,0),(100,100),(0,100)].
  Independent bilinear formula X=20+.6x-.2y+.004xy,Y=y. Independent projective formula
  W=1-.004y,X=(.6x-.2y+20)/W,Y=.6y/W. Every refit interval samples 129 points against these
  formulas. Acceptance bound <=0.25 destination px. Circle uses existing live_path cubic
  ellipse, so this checks homography of the source cubic circle, not a new raster oracle.
- Reshape: anchors x=0,10,20,40 at y=10, selected x=0, radius20, deltaY16; expected y=26,19,10,10.
- Each: fixed seed123/456; scale [2,1.5], movement [30,20], rotation30; row-major center pivot.
- Painted text: embedded Inter, ordinary/path "AB", 20 pt, black underlined A/white B,
  affine-equivalent bilinear translation (10,5), Copy and undo. Font assets in
  assets/fonts/Inter-{Regular,Medium,SemiBold}.ttf; existing assets/fonts/OFL-Inter.txt,
  SIL OFL 1.1, copyright 2016 The Inter Project Authors. No font asset was added.
- Auxiliary type: Inter "A", 20 pt effects; 128 COW copies at 300 pt on 8192x8192,
  scale4 for aggregate-budget rejection. Fixtures use real type_cmds::refresh; those two
  corrected fixture cases were not rerun. UI keyboard focus regression uses actual
  Alt-click, Angle editor focus, S, Escape; one execution passed after the context fix.
- Rounded-corner pattern fixture: triangle [(20,20),(40,40),(0,40)], 2px round first
  corner, synthetic opaque-white 2x2 pattern, scale2 with Patterns disabled. Independent
  quarter-circle cubic control-bound minimum is 20+sqrt(2)*(1-k),
  k=(4/3)*tan(pi/8); its original phase adds 4px. The single test run passed; the final
  control-bound refinement was compiled by clippy but not rerun (see report).
- Native topology: Join holes, EvenOdd + inverted flags, closed/open/singleton/empty subpaths.
- egui_kittest 0.36.2/egui 0.36.2, actual pointer/key/TextEdit events, no desktop windows.
- Compiler cargo +1.98.1, offline, CARGO_BUILD_JOBS=3, retained li-rustc-gate wrapper.
  Numeric finite bounds: affine linear coefficients +/-1000, translations +/-1e7, actual
  path coordinates +/-1e6. Projective pure mapper +/-1e7, 131072 nodes, split depth24.

Read-only reference clone verified HEAD exactly d522c1d7be4035bd4f4a84cd6ebfca44f5155092:
~/.local/share/local-image-dev/engine-sources/vectorcraft. Used files are in transform34-PORTS.md.
Existing providers: vector_api, layout bounds, drawing::geometry, corners::project_shape,
appearance::CompiledAppearance, snapping::query::Cache, vecfx::{width,brush}, pc-text outlines,
symbols::detach_prepared, path_text::{project,on_path}, and its established
path_text/curves.rs native contour conversion adapted into the owned contours.rs
because the original module is private; no shared API was expanded. Lane 00 can expose
the original adapter and replace this local adapter with a use. No second text shaper. These were preserved, not reimplemented.

Official behavior sources read 2026-10-10:
[Rotate](https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/rotate-objects.html),
[Transform](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-objects.html),
[Distort](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/distort-objects.html).
The bounded Bernstein refit is original: compose cubics into homogeneous degree3 or bilinear
degree6 polynomials, construct endpoint Hermite cubic, elevate its product with denominator,
then bound the Bernstein residual convex hull / minimum signed denominator. Conservative
roundoff allowance is included. It avoids an unproven sampled-only error check.

GPU source: transform_tools/gpu_tests.rs uses actual photocraft_gpu::Compositor and compose
flatten, strict 5090/Vulkan/discrete/Rgba32Float identity, premultiplied max diff <=1/255.
Not executed under the final speed override. Full-app preview plane and 500-shape release
pointer-frame performance require 00 routing; no timing or hardware success is inferred.
