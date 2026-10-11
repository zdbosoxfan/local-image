# Package 40 interaction records

Pointer capture/preview/release semantics adapted from VectorCraft
`crates/tools/src/draw2/gesture.rs` at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0,
copyright 2026 ArtCraft Team and contributors; existing `licenses/vectorcraft-*` notices.
Pressure input follows the current app ToolEvent/tablet route and 05 stabilizer API.
Official Adobe behavior references and attribution rows are in `integration/40.md`.

Kittest tests are original GPL-3.0-or-later fixtures. Real egui 0.36 mouse, modifier,
keyboard and text events drive a 200×200 document and pressure=0.5. They type size 18,
click the merge/selection options, drag across the canvas, release outside the canvas,
and cancel via Escape/focus/tool deselection. No external images or golden screenshots.

`app_gpu.rs` is a production-router integration gate awaiting 00/01/46 hooks. It
requests offscreen eframe/egui_kittest rendering, asserts RTX 5090/discrete/Vulkan and
Rgba32Float, reads the real GpuCanvas texture, and checks linear premultiplied pixels
against CPU composition (max difference <=1/255). Its release measurement samples
32 actual pointer/render frames plus release, printing median/p95/p99 and hardware.
No GPU or performance result is claimed before these integration gates run.
