# Package 28 interaction and GPU fixtures

`tests.rs::session` generates original Local Image GPL-3.0-or-later red/blue
rectangles on transparent raster pixels. CPU kittest: 32×24; offscreen real-app
GPU test: 128×96. PixelArt uses exact corners and no speckle removal. The injected
document library contains green and magenta. No external/proprietary corpus or
brand image is used. Fixture geometry parameters are stated in the test source.

The CPU harness runs egui 0.36.2 with accesskit queries and actual pointer/key/text
input. The ignored GPU harness uses egui_kittest/eframe 0.36.2, the production
PhotocraftApp compositor/canvas, Vulkan-only RTX 5090/discrete selection, required
Rgba32Float accumulator and F16 canvas. render() yields premultiplied RGBA8;
maximum channel difference gate = one byte (1/255). Cancellation and apply/undo
are compared against the original app canvas; modal view changes must alter real
rendered pixels. No timings, release measurements or performance claims.

Sources followed: local T1 report, existing jobs.rs run_latest/cancel framework,
dialogs.rs modal/Resize usage, existing persona offscreen GPU identity/readback
patterns, and installed egui 0.36.2 modal/resize/slider/response sources. Pinned
VectorCraft `crates/ui-egui/src/panels/image_trace.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` was read as a behavior reference; none
of its code was copied. New implementation/fixture files therefore require no
additional upstream PORTS rows or license notices.
