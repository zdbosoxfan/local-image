# Package 28 output fixtures

All fixtures in `tests.rs` are original test-time Local Image geometry/pixels,
GPL-3.0-or-later. No third-party image, brand asset, proprietary file or external
corpus was used. Two flat red/blue rectangles on transparent 32×24 RGBA8 test
ordinary output. A 10×10 ring with a 4×4 counter and separate 4×4 island tests
connectivity; expected filled area = 100−16+16 = 100 square pixels. Coverage uses
the independent production exact-area rasterizer; rectangles also use analytic
pixel membership as an independent oracle.

Smart input uses a synthetic cached black surface and intentionally nonexistent
linked path `/missing-trace28.png`, with a 5×7 selection at (7,9), preview scale
0.5, exact rounded X/Y scales (3/5,4/7). No file is read there. Full-res expected
path bounds (7,9,12,16), area 35. This exact coordinate oracle uses PixelArt.
A transparent-source 3×3 ring selection (one unselected centre) uses PixelArt
with `use_alpha=false`; expected selected area = 8, centre alpha = 0, and
before-view pixels retain original transparent alpha.

Tests use T1 PixelArt (`speckle=0`, two-colour max where stated), BlackWhite
(default preset threshold 128, speckle 2), and fixed green document palette.
No reference CLI or generator executable is needed. Tool versions: Rust 1.98.1,
egui/eframe/egui_kittest 0.36.2, kurbo 0.13.1 and linesweeper 0.4.0 (workspace
lock). Algorithm attribution remains in the existing pc-trace/pc-pathops notices.
