# Blend fixtures and independent oracles

All tracked fixtures are synthetic Rust geometry authored for Local Image
(GPL-3.0-or-later). No Illustrator/Affinity/VectorCraft brand assets, screenshots,
proprietary files or external downloaded corpus are embedded. Missing external and
proprietary corpus counts are separate from passing synthetic tests.

`suite.rs`: circle radii 5→17 and centers 20→100, five intermediates; analytic bounds,
neutral-axis Oklab L(t)=t ⇒ linear RGB=t³ and standard sRGB transfer; analytic circle area
with cubic-kappa approximation tolerance; rectangular shapes with opposite winding,
rotated/explicit starts and extra collinear node; 20×20 outer with 10×10 hole reordered;
independent raster coverage verifies the hole and total area. Missing hole area at
midpoint is 400−25=375. Two-segment straight spine (20,20)→(50,60)→(80,20), total length 100, distance 25,
first tangent atan2(4,3); symmetric cubic controls (0,100),(100,100) has length 200 and midpoint (50,75).
Additional paint opacity uses the independent expected alpha 0.4t on both sides of
the midpoint; strokes interpolate width 2→6, opacity .4→.8, dashes [2,2]→[6,2],
offset 0→8 and asymmetric profile widths [1,3]→[3,5].
Scene DTO export repeats reserve symbol IDs, preserve units/DPI/warnings, and remain
deterministic independently of temporary document identities.
Group structure/IDs/caches/lazy edits, serde defaults and bounded invalid inputs use
synthetic document records. Source-inspired expectations: pinned VectorCraft tests
listed in integration/18-SOURCES.md; test code is original, not copied.

Engine and egui_kittest fixtures use small U8 RGB documents, two cached circles,
actual click/double-click/text/pointer-down/move/up/focus/Escape/modifier events and
session undo. Straight-spine insertion follows V1
geom::SubPath::insert_anchor: a straight line receives a corner with collapsed handles.
The modifier oracle moves that outgoing handle from (100,60) to (120,100), and release
without modifiers moves the anchor itself to (120,100).
GPU sources add an ordinary clipped raster sibling and drop shadow to a live group;
compare premultiplied max absolute diff ≤1/255. Full-app source uses a 1500×1000
offscreen harness, 240×180 document, zoom 2, fixed canvas pointer coordinates, and crops
preview/release pixels away from active controls. No desktop window is opened.

Versions: Rust/cargo 1.98.1; egui/eframe/egui_kittest 0.36.2 (workspace lock); kurbo
0.13.1 through V1 pc-pathops; wgpu 30.0.1 per existing workspace lock. No new registry packages.
Spine lookup uses native V1 curve lengths and exact curve tangents with 32 inverse-length
bisections; unequal-node correspondence uses the same bounded inverse metric. Bounds/budgets are in the evaluator.
GPU execution and release pointer-frame benchmarking are deferred by owner speed rules;
no measured GPU or performance result is claimed.
