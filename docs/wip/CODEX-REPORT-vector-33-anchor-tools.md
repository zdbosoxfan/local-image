# Vector 33 — anchor tools and Curvature

Date: 2026-10-10. Baseline: `906858ae`. Changes are uncommitted in this worktree.
The initial tree contained only the untracked task brief; there was no existing 33-specific
implementation/report to recover. Existing node-editing, Pen, geometry, corner and rendering
work was inspected and retained. No shared document schema, dependencies or lock changes.

## Implemented scope

- **Add/Delete Anchor:** selected-curve hit helpers use V1 nearest; nodes suppress endpoint
  insertion. Alt swaps the tools. Press/release prepare detached previews and call the existing
  journaled `path.node.edit` once. Insertion, refitting deletion and topology/corner provenance
  reuse 03's public operations. Returned selections are remapped into the existing node model.
- **Convert:** click retracts handles, drag pulls symmetric smooth handles, displayed-handle
  drag breaks one handle independently, and segment drag bends through the pointer. Hidden
  handles are not picked. Shift constrains direction even with object snapping disabled.
- **Curvature:** bounded interpolating Catmull–Rom spline in pc-vector; smooth interior knots
  have matching derivatives, explicit corners retract handles, and all clicks remain exact
  anchors. Detached multi-click drawing, Alt/double-click toggle, existing-point drag, selected
  point Delete, open finish via Esc/Enter, first-point closure and rubber band are implemented.
  Esc during a captured drag cancels the unfinished sequence. Previously committed curves
  finish each drag/toggle/delete separately; the selected point survives release so Delete
  can follow the drag. Fresh gestures recapture the current complete selection. Shape/mask/
  work/saved subpaths are supported.
- **Transactions:** document/revision/target capture, complete selected-set preflight, inherited
  lock checks, finite/coordinate/node/cache bounds, one prepared publication, and atomic corner
  remapping. Curvature preserves knot identity explicitly through `origin`, including moves
  and removal of a subpath. Live shapes return a conversion request carrying the original
  press and target IDs; confirmation rejects stale context before replaying it.
- **Integration deliverables:** compiled engine/UI/spline modules, exact registry/router/display
  hooks, tool IDs/shortcuts/icon requests, Pen hover preference wiring, i18n fragment, attribution
  rows and fixture provenance. All of 03/04's owned files remain unchanged.

Files: `crates/pc-engine/src/vector_packages/anchor_tools.rs` and tests/SOURCES;
`crates/pc-ui-egui/src/vector_packages/anchor_tools.rs` and CPU/full-app GPU tests;
`crates/pc-vector/src/curvature_fit.rs` and tests. Shared edits add only public module declarations.
Handoff: [integration/33.md](vector-tasks/integration/33.md); strings: [i18n/33.tsv](vector-tasks/i18n/33.tsv).

## Validation

The task's final **SPEED RULES** override full-crate suites, GPU execution, release builds and
benchmarks. All cargo commands are offline, Rust 1.98.1, CARGO_BUILD_JOBS=3 and retain the
required `/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate` wrapper. One cargo
build runs at a time. No desktop windows were opened or owner files modified.

| Gate | Result |
|---|---|
| `cargo check --offline --workspace --all-targets` | Passed; run once (2m20s). Existing missing craft-fonts warning from lightcraft-engine. |
| Strict clippy, all targets, the three changed crates, `CARGO_TARGET_DIR=target/clippy` | Passed on final source (16.48 s); initial pass also passed. |
| pc-vector `--lib anchor33_` | 3 passed, 0 failed, 0 ignored, 61 filtered out. |
| pc-engine new `anchor33_` tests | 9 passed across the initial batch, two corrected-fixture tests and one additional selection-guard test. No ignored tests. See failure record below. |
| pc-ui-egui new `anchor33_` tests | 6 passed; 2 GPU tests ignored; 1,065 unrelated tests filtered in the initial batch. Only the failed double-click test was rerun; see below. |
| Format, package-file whitespace and `git diff --check` | Passed. |
| Cargo.lock / manifests | Unchanged. |

New tests check insertion at six t values with 1,001 samples each and 1e-6 px tolerance;
all eight possible deletions from an eight-node, r=100 cubic circle using a symmetric V1
nearest-distance Hausdorff oracle at <=0.5 px; 256 seeded topology edits with finite geometry
and retained corner identities; symmetric Convert handles, independent breaking and bend
displacement; interpolation/G1/true-corner/seam oracles; stale/locked/invalid/live conversion
rejection; and actual egui clicks/drags/keys for Add/Delete/Alt/Shift+C, Curvature, rubber band,
toggle/drag/Delete/close, cancellation, conversion confirmation and undo.

The initial engine batch passed six tests and failed two: both fixtures incorrectly attached
live-corner metadata to open endpoints. The corner evaluator correctly rejected those
ineligible targets. Only those two failed tests were rerun after replacing the fixtures with
radius-2 eligible interior corners; both passed (1,089 other tests filtered out). Review then
added full-selection validation to Curvature requests and a new regression test; it passed
once (1,091 others filtered out). Passing geometry tests were not rerun. No production corner
validation was weakened to accommodate invalid fixtures.

The initial UI batch passed five CPU tests, failed the double-click case, and ignored both
GPU tests. egui_kittest defaults to 0.25 s per frame, putting the test clicks outside egui's
0.3 s double-click window. At 60 Hz, the first correction still overlapped the preceding
Alt-click's 0.6 s triple-click history. The fixture now waits 60 frames between those
separate interactions and asserts egui reports a double-click. That single corrected test
passed (1,072 others filtered); the five passing tests were not repeated. These were
fixture timing failures, not false successful input gates. The final UI source also retains
existing-point selection between finished gestures; its registered-command/app behavior is
covered by the supplied deferred GPU gate, not claimed as executed CPU integration.

Unique new CPU tests passed: **18** (3 vector + 9 engine + 6 UI). GPU tests: **2 ignored,
0 executed**. No existing crate tests ran. The workspace check ran once before the final
review fixes; final all-target strict clippy validates their compilation.

Log records (ignored build artifacts): `target/anchor33-{workspace-check,clippy,vector-tests,
engine-tests,engine-curvature-retest,engine-selection-test,ui-tests}.log`.
Additional correction/final records: `target/anchor33-ui-double-click-{retest,final}.log`
and `target/anchor33-clippy-final.log`.

## GPU tests supplied and gates deferred

Two compiled, explicitly ignored tests in `anchor_tools/app_gpu.rs` require the full
PhotocraftApp canvas, actual input/preview/release and offscreen egui_kittest rendering.
They assert RTX 5090 / Vulkan / DiscreteGpu, Rgba32Float accumulation and float canvas support;
preview/release parity uses premultiplied bytes with max diff <=1/255. They assert one journal
entry/history step, cancellation and undo, including existing-point drag then Delete.
They fail without 00's real app hooks/command registration; no software
adapter or successful skip substitutes for the gate. No GPU hardware identity or performance
measurement is claimed from this job because execution is forbidden by the final speed rule.

Skipped by that rule: full-crate/workspace test suites, GPU acceptance execution, release
builds and performance benchmarks. No external/proprietary corpus or upstream golden artwork
was used; missing corpus is not counted as a successful gate. All fixtures are synthetic GPL
Rust definitions with parameters and regeneration records in SOURCES.md.

## Remaining coordinator work / acceptance status

**This report does not declare integrated acceptance complete.** The task retains 00 ownership
of central command registration, menu/tool/router wiring, aggregate GPU registration and
catalogs. Its REGISTRATION override allows the additive public module lines, not competing
edits to those shared registries. The complete fragments are in integration/33.md.

00 must register `out.extend(anchor_tools::specs())` for editing existing Curvature curves;
new drawing already uses the registered `path.penCommit`. Install the canvas input/display/frame
hooks, 03's displayed-handle list and conversion modal, 01/46 tool IDs/icons/shortcuts and
current paint proxies, Pen Disable Auto Add/Delete preference, and i18n/attribution fragments.
Combined decorated appearance rendering uses the named 06/08/11/17 provider. The normal
fallback supports ordinary paths/corners/appearance and returns explicit errors for unsupported
decorations rather than dropping them. Then execute the supplied serialized real-app GPU gates
at the integration milestone. These dependencies are enumerated, not counted as passed.

03's target API prioritizes a shape path over a vector mask on the same shape layer.
Mask editing on non-shape layers is supported; an explicit mask-on-shape target extension
belongs to 00/03 and is requested in the handoff rather than editing their owned API here.

Deletion uses 03's existing refitter; the circle tolerance gate is tested independently.
Arbitrary sharp-corner removal cannot preserve its exact outline with one replacement cubic;
no universal Hausdorff bound or performance claim is made for such inputs.

## Sources

Followed the pinned VectorCraft `draw2/anchor.rs`, `draw2/curvature.rs`, `draw2.rs::catmull_rom`,
and native command contracts; reused established Local Image APIs rather than designing a
second node editor. Retained VectorCraft copyright/license notices; one PORTS fragment row
per adapted file is supplied for 00's regeneration.

Primary behavior references: [Adobe Add/remove anchors](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/add-or-remove-anchor-points.html),
[Convert anchors](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/convert-anchor-points-on-a-path.html),
and [Curvature](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-shapes-with-the-curvature-tool.html).
Detailed upstream URLs, licenses, tool versions, fixture parameters and intentional batching
differences are in [SOURCES.md](../../crates/pc-engine/src/vector_packages/anchor_tools/SOURCES.md).
