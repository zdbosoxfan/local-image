# Persona fixtures and references

`pixel-toolbar.json` freezes the Pixel All Tools (53 ids, canonical order) and Photographer
(28 ids, canonical order) lists from Local Image commit
`2e8c671fb3733c7b8b5c2cf85b136b2f5578183c` (`crates/pc-engine/src/toolsets.rs`). Input and fixture license:
GPL-3.0-or-later, the repository license. Extracted with Python 3.14.8's standard library (git 2.56.0) from
`git show HEAD:crates/pc-engine/src/toolsets.rs` before persona changes, 2026-10-10.
It is a baseline compatibility oracle; tests do not regenerate it from the new registry.

UI/GPU inputs are original synthetic fixtures generated at test time: 256×192 px white
document; two integer-coordinate red rectangles `[20,20,80,60]` and `[60,20,80,60]`.
Their intersection is 40×60 and union area is 7200 px. GPU switch test also uses an ellipse
`[30,30,90,70]`, color `#c02030`. No external/proprietary corpus is required for persona.
The source tests and generated fixtures are GPL-3.0-or-later.

Versions: Rust 1.98.1, egui/egui_kittest/eframe 0.36.2, locked wgpu 30.0.1. All builds/tests
are offline; no new dependencies. GPU harness chooses RTX 5090, Vulkan, discrete, requires
Rgba32Float accumulation and Rgba16Float canvas, 1 physical pixel/point, 1280×800 UI.
Canvas comparison is premultiplied maximum absolute difference ≤1/255.

Behavior references (no source code/brand assets copied):

- [Affinity Personas](https://affinity.help/designer2/en-US.lproj/pages/Introduction/about_Personas.html),
  official documentation read 2026-10-10. Owner decision 2 keeps the panel workspace intact.
- Local Image's existing `toolsets_ui_tests.rs`, rasterize prompt, `vector_ui.rs` and GPU
  canvas flip harness at the baseline commit above.
- VectorCraft `crates/tools/src/lib.rs` at
  `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0), inspected read-only
  under `~/.local/share/local-image-dev/engine-sources/vectorcraft/`: action/capture model
  reference, no code port. V1's documented command signatures were also checked read-only
  in the existing `codex-vector-v1` worktree.
- Locked crate source for `egui_kittest::HarnessBuilder::wgpu_setup`,
  `WgpuTestRenderer::from_setup`, and egui input key consumption; used the existing APIs.

PORTS fragment for 00: no new upstream adaptations; retain existing toolset, toolbar and
icon attribution/license records. This package introduces original persona orchestration.

Fixture SHA-256: `4dbfaef8b0af1659f12825a53304b3a62cf3d97696ed1c320cd9c1be130980e7`.
