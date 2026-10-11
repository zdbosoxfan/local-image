# Engine 45 acceptance fixtures

Original generated Local Image fixtures, GPL-3.0-or-later. Two translucent 50×60
rectangles at x10/90 in a 160×128 F32 RGB document. Tests exercise radius10, source
resize to 70×40, a locked target, duplicate targets, invalid entire-set parameters,
stale revision, a failing persistence adapter, Undo/Redo and unrelated-edit cache hits.
Three Transform copies move by (12,4); the independent oracle builds four ordinary
layer duplicates via the preexisting affine API and renderer, with exact RGBA8 output pixels and a 1e-7 floating-composite guard.
An Appearance fill with Dissolve checks exact merged/detached compositor parity so
effect projection never introduces a second layer-level dissolve pass.

`tF45` test PSD blocks are an explicit synthetic storage adapter only. Native test
requires the actual 00-installed VectorData field and the repository's v1 pcraft fixture.
GPU gate is ignored under speed rules; assertions require RTX 5090/Vulkan/discrete/
Rgba32Float, real compositor readback, all nine presets, Preview/release and 1/2/4/8x
projection with premultiplied diff<=1/255. A skipped or software GPU is failure.
The nine GPU cases use nonidentity parameters: Pucker60, Twist90, Transform120/80
percent with move(12,4)/angle15/copies3, and a displaced Free Distort quadrilateral;
the other cases retain the declared presets. Preview/release pixels are also compared
directly at every scale.
No GPU/performance measurements are claimed in this worktree.

Toolchain cargo/rustc1.98.1; wgpu30; existing 06/V1/08/09 providers. See vecfx fixture
SOURCES.md and integration/45.md for pinned upstream formula/attribution evidence.
