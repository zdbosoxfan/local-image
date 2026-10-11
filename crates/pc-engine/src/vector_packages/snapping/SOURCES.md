# Snapping fixtures and provenance

Local Image code/tests: GPL-3.0-or-later. Generated at test time; no proprietary assets, external documents or network tests.

Grid rounding is adapted from VectorCraft `crates/geom/src/snap.rs` at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0, Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. Original notices remain in `licenses/vectorcraft-*`; the one PORTS row is supplied in `integration/07.md` for 00. The cache, total ordering, query composition and spacing projection are original integrations. VC `crates/engine/src/tests_smartguides.rs` at the same pin was read as a behavioral reference; its source was not copied. Existing Move `snap.rs`, `snap_ui.rs` and `CODEX-REPORT-move-tool.md` supply the cache/modifier contract.

Official behavior reference: https://affinity.help/designer2/en-US.lproj/pages/DesignAids/snapping.html (read 2026-10-10): screen tolerance, nodes/geometry/bounds, independent target toggles and equal-gap measurements. No documentation text was copied into code.

Toolchain: Rust/cargo 1.98.1, egui/egui_kittest 0.36.2, wgpu 30.0.1, existing pc-vector rasterizer and pc-compose/GPU. Offline builds, CARGO_BUILD_JOBS=3. No dependency/lock changes.

Parameters/oracles:
- Axis guide x=120; zoom 0.25/1/4/8 and PPP 1/2; independent screen-to-document tolerance relation with inside/outside probes.
- Grid step 20, origin (3,-2), integer pixel grid; node at (100,90). The 256-case independent lattice-neighbor oracle uses step7, origin(3,-2), seed0x9a22_1371 and an LCG with multiplier6364136223846793005/increment1; candidates are enumerated over lattice indices -100..=100.
- Cubic parabola `(0,0),(100/3,0),(200/3,100/3),(100,100)` has p(t)=(100t,100t²). Query (49,26) lies on the normal at (50,25), t=0.5. The injected analytic callback tests the V1 bridge/atomic selection only; **it is not a test of V1's absent nearest solver**.
- Equal gap: fixed boxes [10,30] and [90,110], moving union width20 => left edge50, two gaps20. Repeated existing gap60 => next left edge170. Repeat after swapping X/Y.
- Angle: 45-degree construction line through (100,100), orthogonal projection of (161,162) => (161.5,161.5).
- Reordered layer traversal retains stable IDs and results; guide ties use coordinate ordering.
- Invalid-input regression includes an artboard `(0,0,100000000,20)`, beyond the snap coordinate limit of +/-10000000; cache construction rejects it before a guide span can reach painting.

No competitor or proprietary corpus is required for these synthetic query fixtures. Final V1 curve-solver, analytic-bounds and native UI/real-app node/spacing routes are named missing gates until 00 integrates them; callback fixtures cannot count as those gates.
