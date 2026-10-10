# Vector 19 live repeat report

Date: 2026-10-10. Baseline: `b546f0dc`, integration destination `wip/vector`.
Changes are uncommitted in this worktree. The starting tree had only the untracked task
brief; prior builders' committed foundations, native format, vector rasterizer and
symbol projection/preparation are preserved. The owned repeat engine slot was empty.

## Implementation

| Task point | Result |
|---|---|
| Radial/grid/mirror evaluator | New internal `photocraft-vecfx::repeat`, using the existing shared `RepeatSpec` and `Affine`. Full-turn radial symmetry with radian phase, row-major grid with exact geometric-bound gap spacing, source plus mirror reflection. Analytic cubic bounds come from the established pc-pathops adapter. |
| Source-preserving live groups | Ordinary group children and `VectorData::live` remain authoritative. Preview/projected output is detached. No second object tree or duplicate persisted source store. The native regression found a shared tag collision; a backward-compatible field codec and exact 00 registration fix are supplied. Native acceptance remains blocked until registration. |
| Deterministic generated layers | Stable transient IDs allocated locally without `LayerId::fresh`, unique within the projected tree, instance groups preserve clipping-stack boundaries, absolute affine map and original-source provenance. Nested repeats compose transforms once. |
| Render/export projection | Public `expand_layers`/`expand_scene` consume ordinary layers and the shared exchange DTO. Engine `project_document` prepares bounds-limited ordinary caches through 15's existing public renderer. File consumers need only pc-vecfx/doc; no UI/engine dependency. Exact consumer hooks are in the integration handoff. |
| Editable commands / expansion | `vector.repeat.create`, `.set`, `.expand` are compiled and registered through the existing engine slot. Each requires document/revision and a disjoint target list. Every target and descendant/inherited lock is validated before detached preparation. One command/Session::edit publishes each operation. Expand preserves the enclosing group and allocates fresh persistent IDs after successful preparation. Undo/redo are covered. |
| On-canvas controls | Grid row/column counts and X/Y gap handles; radial count, center and angle; mirror center and axis angle. Screen-sized hit tolerance, frozen view mapping during capture, current Shift modifiers, pointer motion/release outside the image. Original sources stay editable. |
| Numeric panel | Create/switch repeat kinds; bounded editable counts, center, angle and spacing; detached numeric previews; release/Enter/Apply commit once; Expand and Cancel. Old `{}` preferences load through serde defaults. Transient state is held in egui memory rather than settings/native documents. |
| Gesture safety / invalidation | Captures document ID, revision and IDs. Escape, lost focus/pointer, tool/persona/module/modal or document/revision/selection change cancel. Display cache includes original source Arc, document revision and preview/view key; source edits and undo rebuild instances. |
| Bounds / refusals | Instances 1–1000, grid product <=1000, checked arithmetic, minimum grid pitch 0.001 px, finite coordinates/parameters bounded to 1e6, 100,000 generated layers, 1,000,000 generated knots, normal depth limit, 16,000,000 aggregate content/mask cache pixels. Malformed geometry/paint is validated before raster compilation. Errors publish no revision, history or journal change. |
| Registration / dependencies | Minimal new-crate registration, existing engine dependency, UI module registration. Lock diff contains only the new local package and its engine edge; no external package, feature upgrade, unsafe code, C/-sys dependency or brand asset. Shared menu/router/catalog/PORTS/attribution edits remain in 00's owned lane. |

## Validation

All cargo builds preserve
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, use
`CARGO_BUILD_JOBS=3`, `PATH=~/.cargo/bin:$PATH`, Rust 1.98.1 and `--offline`.
Only one cargo process runs at a time. No whole-crate or whole-workspace test suite,
GPU acceptance execution, release build or benchmark was run.

- Workspace/all-targets check: **passed**. `target/repeat19-check-final.log`.
  This preceded the additive compatibility codec/old-v1 tests; final changed-crate
  all-targets clippy also checks that latest source. No broad check was repeated.
- Strict clippy, all targets of `photocraft-vecfx`, `photocraft-engine`,
  `photocraft-ui-egui`: **passed**. `target/repeat19-clippy-final.log`.
- Formatting: `cargo +1.98.1 fmt -p photocraft-vecfx -p photocraft-engine
  -p photocraft-ui-egui`; whitespace review `git diff --check`.
- New evaluator tests: **9 passed, 0 failed, 0 ignored**;
  `cargo +1.98.1 test --offline -p photocraft-vecfx --test repeat repeat19_`.
  Log: `target/repeat19-tests-vecfx.log`.
- New engine tests: **5 passed, 1 failed, 1 ignored, 1064 filtered out**;
  `cargo +1.98.1 test --offline -p photocraft-engine --lib repeat19_`.
  The actual native writer/reader regression failed on the shared serialization bug
  described below. Log: `target/repeat19-tests-engine.log`. The successful tests were
  not rerun.
- Additional new native codec/old-v1 tests: **3 passed, 0 failed, 0 ignored,
  1071 filtered out**; `cargo +1.98.1 test --offline -p photocraft-engine --lib
  repeat19_native_codec`. Log: `target/repeat19-tests-native-codec.log`.
- New UI tests: **6 passed, 0 failed, 1 ignored, 1054 filtered out**;
  `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib repeat19_`.
  Log: `target/repeat19-tests-ui.log`.

Totals across these new-test filters: **23 passed, 1 failed, 2 ignored**. Each new
CPU test was executed once. The failing native regression is a real unmet gate;
codec/old-v1 passes and ignored GPU tests do not replace it.

The initial compilation found a wrong Point reexport assumption; it was corrected to
pc-geom. Initial lint found three collapsible-if warnings and a wgpu 30 constructor
default change; corrected before the passing final checks. Corrective compilation was
limited to resolving these failures. Existing build-script warning: no craft-fonts
embedded for Chinese/Japanese; these shape-only fixtures require no fonts.

Independent oracles include explicit quarter-turn points, exact grid origins/gaps,
signed mirror-axis distance plus involution, the cubic polynomial height at t=1/2,
explicit rendered pixel/cache bounds, byte/model identity on refusal/undo, and the real
native writer/reader. Kittest actually clicks, drags and types; it does not directly
change parameters as a substitute for UI input.

## GPU tests supplied; owner speed override

Both are compiled, ignored and **not counted as passed GPU gates**:

1. Engine `repeat19_gpu_projection_source_edit_and_expand_parity_rtx5090`: real
   Vulkan device, source edit, detached previews, commit/expand, premultiplied max
   CPU/GPU difference <=1/255 through the existing compositor.
2. UI `repeat19_full_app_gpu_live_preview_cancel_release_expand_undo_source_edit_rtx5090`:
   offscreen `PhotocraftApp`, production panel clicks, actual pointer drags, repeated
   live GPU preview/release/cancel, expand/undo and source edit, and parity against the
   actual app GPU canvas texture. Missing production hooks fail the gate. CPU compositor
   fallback is refused.

Both assert RTX 5090, Vulkan, discrete adapter and Rgba32Float accumulation. Neither
accepts llvmpipe, adapter absence or format absence as a skip. No desktop window was
opened and no owner app/process or ComfyUI job was touched. No interactive speed claim
is made; release pointer-frame benchmarks are deferred by the explicit speed rules.

## Integration / remaining gates

The package-owned implementation and compiled command/widget APIs are delivered.
**Final integrated acceptance is pending**, not represented as complete by isolated
widgets or unexecuted GPU tests:

- Package 00 must apply the exact frame/input/overlay/display/menu hooks in
  `docs/wip/vector-tasks/integration/19.md`. The task explicitly reserves UI menu/router
  wiring to that lane. UI module registration alone does not activate the panel/canvas
  in the production app. CPU, GPU and Navigator must all consume the same projection.
- **Observed native blocker:** `LiveGroup` and `RepeatSpec`/`EnvelopeSpec` are both
  internally tagged with `kind`; the nested kind overwrites the outer one. The native
  reader rejects a grid with `unknown variant grid, expected blend/repeat/envelope`.
  `repeat/native.rs` supplies a pure data field codec that writes adjacent `kind/spec`
  tags and reads both that form and legacy flattened blend/repeat/envelope records.
  The handoff gives the exact `VectorData::live` serde annotation/module registration
  for 00. Shared types are read-only in this task, so the registration was not applied
  competitively. The real native regression remains unignored/failing and is not counted
  as a successful gate. The codec tests do not substitute for native acceptance.
- Package 09 must feed the shared expansion/transforms into scaled geometry and its
  damage/cache path; 13/14/21 consume the exchange projection for exports. Package 23
  handles the advanced-feature PSD editability warning/fallback. Their core files are
  not edited from this lane.
- Advanced appearance/corners/path text/symbol/other-live rendering needs the named
  shared providers. The legacy renderer explicitly refuses unsupported content,
  transformed non-solid paint and rotated artboard clips before publication. The pure
  evaluator retains their source metadata and instance affines for those consumers.
- Package 00 merges `i18n/19.tsv` and the per-file PORTS/attribution fragment, then runs
  the compiled ignored full-app RTX gates after wiring. GPU, release performance,
  full-crate suites and broad corpus runs were skipped under the final speed override.

Corpus: all fixtures are local synthetic GPL-3.0-or-later shapes, plus the existing
native v1 fixture. No proprietary/external repeat corpus was provided, required or
counted as a successful gate. No CJK font or competitor result is claimed as validated.

## Sources and records

Read the architecture, full persona spec including Owner decisions, quality plan and
package index. Inspected owned files/current tests before changes and reused the
shared live/exchange data, vector transaction contract and existing 15 preparation.

Pinned primary source: VectorCraft checkout
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0. Repeat is in
[doc/pattern.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/pattern.rs),
last file commit `5afc78a1002cc217f743f74cea9dcdf640318b0a`; `doc/live.rs` delegates
expansion to it. Grid gap spacing, paint order, reflection and per-instance grouping
follow that source. Local Image's shared model uses radians and a full radial turn,
with no separate upstream radius/sweep/grid-brick/flip fields. Bounds/resource refusal,
IDs, provenance, transaction and UI details are Local Image adaptations/original work.
Cached egui 0.36.2 source and existing project kittest/wgpu patterns guided interactions
and headless setup; no crate/API version guess was left in the final compilation.

Copyright/license notices are retained in the adapted source and the existing
`licenses/vectorcraft-*` files. Two per-file PORTS rows and the attribution regeneration
fragment are supplied to 00, not written competitively into core files.

Fixture licenses, exact geometry/oracles, parameters and tools:
`crates/pc-vecfx/tests/repeat/SOURCES.md`. Handoff and source/attribution fragment:
`docs/wip/vector-tasks/integration/19.md`. Strings: `docs/wip/vector-tasks/i18n/19.tsv`.
