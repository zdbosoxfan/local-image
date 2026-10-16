# Package 41 sources and fixtures

Read-only upstream: `~/.local/share/local-image-dev/engine-sources/vectorcraft`, verified
HEAD `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. No brand assets were copied.
MIT OR Apache-2.0, copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
Existing `licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}` and curated attribution
are retained. `41-PORTS.tsv` is the exact fragment for 00's PORTS/attribution regeneration.

| Pinned upstream file | SHA-256 |
| --- | --- |
| [crates/tools/src/distort/liquify.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/liquify.rs) | `c2478ff4a32b77cbfde3bdf0ab08327cecf0f36269fbf11cac537f22217f4268` |
| [crates/tools/src/distort/arap.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/arap.rs) | `9c1136b2d9ef4efdcc1a7188b8109ab559ba83db28ddab9e520c6fa0b02b4ef2` |
| [crates/tools/src/distort/puppet.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/distort/puppet.rs) | `7787b11755f25a4e6e1374d0b3c0e7c55ca12ee524628d9e00f78f8f4bb3ea54` |
| [crates/engine/src/cmd/distortcmds.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/distortcmds.rs) | `90b035ba9fcd6fe62f3a6c939527fe27e20cb12f5a034d57352f57ace21bc98b` |

Additional behavior references read: VectorCraft `doc/src/puppet.rs`, engine
`tests_liquify.rs` / `tests_puppetwarp.rs`, `tools/distort/arap.rs` tests; V1 native adapter
and curve fitting; 06 corner projection; 08 CompiledAppearance; 11/17 projection; 36 shared
expansion and PSD-block clearing; existing stylus/pc-tablet bridge and 33 offscreen app tests.

Primary references opened 2026-10-10:

- [Adobe tool reference](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html).
- [Adobe Transform/Puppet Warp](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-objects.html).
- [Igarashi, Moscovich, Hughes, As-Rigid-As-Possible Shape Manipulation, SIGGRAPH 2005](https://www-ui.is.s.u-tokyo.ac.jp/~takeo/papers/rigid.pdf).

The two-stage banded solver and automatic limb pins are adapted from the pinned upstream,
which implements the paper. Local additions: checked boundaries; exact identity/common
translation; bounded local/global rigid-fit refinement with a reused Cholesky factor (the
upstream single rigid-fit step initially exceeded the new independent 2% edge oracle);
global subdivision/work budgets; intensity-times-pressure; V1 refit on changed
subpaths/mesh-mapped segment samples; current layer transaction and cache projection.
The refinement retains the upstream triangle energy and barycentric pin constraints.
Other additions follow this repository's established contracts.

All fixtures are small, synthetic Rust generators authored in the owned tests, licensed
GPL-3.0-or-later with Local Image. No proprietary corpus or external binary golden is
claimed available or passed. Build/test toolchain: Rust/cargo 1.98.1 offline, three jobs,
li-rustc-gate wrapper; egui/egui_kittest 0.36 from the existing lock. No new dependencies.

- Liquify: square +/-20; 200px line and untouched parallel line; ordered subpaths and fill
  flags; 256 eight-point radial polygons with a second open subpath, seeded integer generator
  `r=10+((case*17+i*23)%40)`, 64 total knot budget, detail 10, complexity 15, rotated 120x90
  brush, intensity .7, two pressure samples. Area oracle integrates densely evaluated cubics
  independently with shoelace, 128 samples/segment. Pressure/zero/determinism tests use native
  path equality only where exact identity/replay is the behavior under test.
- Puppet: 200x100 bar, rectangular hole, open line; 16-cell local 0.04-radian turn with two
  other fixed pins. Independent edge ratios assert max stretch <2%; pin positions/rotation
  checked separately. Exact common translation (24,-17) checks all native handles/flags.
  This is a feasible small-turn ARAP oracle, not a promise that incompatible large pin
  constraints can preserve every edge within 2%.
- Engine: two native colored rectangles, locks, absent/duplicate IDs, stale revision,
  corner+appearance, live conversion/swept live targets, locked group children, multi-target
  atomic preview/undo and metadata flags; a synthetic 16000px F32 vector-only canvas rejects
  over-budget caches before allocation/publication.
- UI: small egui_kittest host, real button clicks and pointer/keyboard events; package
  CommandSpecs dispatched directly because 00 owns runtime registration. History is real
  Session::edit history. This host is not evidence of Session::execute journaling/app routing.
  Numeric entry types Width=64; text conversion uses existing type.create with "oil" at 36px;
  no-selection Warp sweeps 5px to 500px across a live rectangle and checks its exact prompt.
- GPU: real offscreen PhotocraftApp with 64/2000-node seven-frequency perturbed ellipse,
  translucent fill on white, all eight tools; hidden overlays; normal app GPU/readback;
  strict adapter/formats, pixel release/undo/cancel and release pointer frame percentiles.
  Execution explicitly deferred by final task speed rules and runtime integration dependency.
