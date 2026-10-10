# Snapping UI/GPU fixtures

Own synthetic fixtures, GPL-3.0-or-later. No imported/brand assets. Existing crate-locked egui 0.36.2, egui_kittest 0.36.2 and wgpu 30.0.1; Rust/cargo1.98.1 offline, CARGO_BUILD_JOBS=3.

`tests.rs`: actual egui pointer presses/motion/release, Ctrl and mac_cmd changes while held, popover checkbox clicks, Escape/focus/revision cancellation and captured motion outside the response. Small host uses the production package-owned State/Manager/paint module. It is **not** the unmodified full application's native router; package 00 must wire that router before final feature acceptance.

`tests_gpu.rs`: Harness::build_eframe opens no desktop window and renders the real PhotocraftApp with its GPU canvas callback. 400x300 white document, opaque 20x20 blue shape at (40,40), vertical guide120. Pointer (50,50)->(131,75), query/Move correction -1X => bounds (120,65,140,85). Preview and release are read back; per-channel max <=1 byte. Independent opaque pixel oracle covers every pixel in [20,300)x[20,140), including edges. Guide and transient snap-bounds overlays are hidden after press (extras.guides/extras.snap=false), so the existing captured cache continues snapping while the paint oracle remains unobstructed. No pixel/edge exclusions or relaxed threshold are used. PPP1/2, Ctrl bypass at motion/release. GPU identity assertions require RTX5090, Vulkan, DiscreteGpu, preferred Rgba32Float; absent/software adapters fail. This is preservation of the existing guide/Move bridge, not a claim that new node/curve/spacing/manager routes are already wired.

No interactive performance claim. No release benchmark timing has been inferred from these correctness tests. Final native routes and release pointer-frame speed require 00/V1 integration.
