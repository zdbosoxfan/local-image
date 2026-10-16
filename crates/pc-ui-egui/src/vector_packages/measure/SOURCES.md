# Package 44 UI fixtures

See `crates/pc-engine/src/vector_packages/measure/SOURCES.md` for pinned source,
notices, licenses, official behavior references, PORTS fragments and tool versions.

`tests.rs` is original GPL-3.0-or-later egui_kittest source. It generates a 400×300
300-DPI document with a 100×50 rectangle at (20,20), dispatches actual egui pointer
press/move/release and keyboard events, clicks units/perimeter mode/Assign and types
254 mm in the real-length DragValue. The host reserves a fixed options band so
pointer mapping is stable. Other tests deliberately change zoom during capture,
release outside the canvas, switch document/revision/tool mode and lose focus.

`app_gpu.rs` is original offscreen full-app test source, awaiting 00's central GPU
registration and production canvas hook. The fixture is a 400×300 F32 white document
and a black 100×50 rectangle. Real-app GPU/CPU comparison uses the whole float canvas,
premultiplied max diff <=1/255. An explicit empty Perf.last_refresh sentinel (existing
canvas test convention) detects any document repaint during the overlay gesture.
Overlay visibility is checked in the GPU screenshot. Warm pointer frames run 35 times,
drop five warmups, measure 30 real UI+GPU/readback frames, report median/p95/p99 and
release latency, and require p95 <16ms. All GPU runs require RTX 5090/Vulkan/discrete
and Rgba32Float; no skipped/software adapter passes. Owner speed rules defer execution.
No desktop window, external image, proprietary corpus, brand asset or network test.
