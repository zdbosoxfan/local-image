# Layout 16 sources and fixtures

Implementation base: integration commit `9157733a` (2026-10-10). Existing engine
registration slots, vector_api, transform_cmds, layer_multi_cmds, artboard_cmds,
appearance renderer, corners evaluator, and package 07/09 APIs were inspected and
reused; no existing builder was replaced. No dependencies or lock entries added.

Behavior references read on 2026-10-10:

- [Affinity Designer 2 Constraints](https://affinity.help/designer2/English.lproj/pages/DesignAids/constraints.html): parent/child pins and optional size scaling.
- [Point Transform](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_pointTransform.html): transform origin and dragging a node to rotate/scale.
- [Vector Crop](https://affinity.help/designer2/English.lproj/pages/Tools/tools_crop.html): non-destructive crop handles, aspect ratio, and movement inside a crop.
- [Transform panel](https://affinity.help/designer2/en-US.lproj/pages/Panels/transformPanel.html): nine anchors, dimensions, rotation and shear.
- [Alignment](https://affinity.help/designer2/English.lproj/pages/ObjectControl/align.html): key-object selection and edge/center alignment.
- [Artboard Tool](https://affinity.help/designer2/English.lproj/pages/Tools/tools_artboard.html): create, move and resize artboards.

These proprietary help pages are behavior references, not copied code/assets or
external fixtures. The task architecture specifies fixed-size precedence for both
pins, but the public help does not define that conflict numerically. Original
arithmetic centers the fixed size between the original pinned margins, giving each
margin half the parent-size delta. This choice is explicit in arithmetic.rs and
independently asserted in the table tests. Rotated child paths are handled by
parent-axis affine scaling; rotated/sheared parent bases are rejected because the
current Group model does not persist a local basis.

Conversions use exact inch definitions (72 pt/in, 25.4 mm/in), f64 document pixels
and explicit document DPI. Cache preflight counts both geometric and cached appearance
extents; prepared outputs also enforce the aggregate budget. Negative pasteboard
artboards retain their coordinates without unsigned canvas growth overflow. Artboard
constraint arithmetic uses the published integer frame, with repeated fractional
requests independently checked for margin drift. Frameless group padding is rejected
because the existing native Group model cannot store it. Print presets choose mm; Web/Mobile/Film & Video/Art &
Illustration choose pt; Photo/legacy defaults choose px. The original file.new and
New Document UI need the additive unit hooks in 16.md.

Test/tool sources inspected locally: locked egui/egui_kittest 0.36.2 Response,
DragValue, AccessKit labels, pointer event and offscreen WgpuSetup implementation;
wgpu 30.0.1; existing package 08/12 offscreen app tests and package 09 scaled view
bridge. Rust toolchain 1.98.1, cargo offline, CARGO_BUILD_JOBS=3. GPU acceptance
asserts NVIDIA RTX 5090, Vulkan, discrete adapter, Rgba32Float; no adapter skip.

Release measurement parameters: offscreen full PhotocraftApp, 1500×1000 logical pixels,
PPP 1, zoom 1, 256×192 pixel synthetic document containing white background, red vector
rectangle and clipped half-alpha blue raster. Select Vector Crop through the actual
context panel; measure 30 pointer-move frames, each including UI, GPU render and readback,
then measure release separately. The median averages sorted samples 14/15, p95 is sample
28 and p99 is sample 29 (zero-based). Hardware: AMD Ryzen 7 9800X3D, 8 cores/16 threads,
RTX 5090 Vulkan driver 615.71.09. The owner's machine is shared; this is not an exclusive
hardware or large-document benchmark. No all-frames-under-16-ms claim is made. Exact
measurements and commands are recorded in the package report.

All new fixtures are original Local Image synthetic geometry, GPL-3.0-or-later:
rectangles, a diamond, differently sized alignment objects, a mixed clipped raster
layer, a parent artboard and existing vector masks. Tests generate them in memory
with explicit coordinates/colors. Arithmetic property corpus: 256 cases, LCG seed
19, binary-fraction margins. Native backward-load fixture is the existing
pc-engine/tests/fixtures/vector-v1.pcraft supplied by the foundations. No downloaded
artwork, fonts, brand assets or proprietary Affinity corpus used. Missing external
manual/corpus evidence is not counted as a passed test.

Attribution fragment for 00: **no new port rows**. All new layout/constraint/tool
logic is original GPL-3.0-or-later code; existing PhotoCraft API calls do not copy
upstream source into a new file. Retain existing PhotoCraft/VectorCraft notices and
PORTS rows for reused imported modules. No attributions regeneration required for
this package's original code.
