# Package 10: constructive tools

Date: 2026-10-10. Base: `7c1b909c`. Changes are uncommitted.

The package-owned implementation is complete and supplied for package 00's serial
integration lane. Final native app acceptance remains pending the shared router/tool
wiring and the acceptance runs explicitly deferred by the owner's final speed rule.
This report does not count those pending gates as passes.

## Implementation

- **Shape Builder:** cached V1 arrangement, face hover with neutral/red hatch and
  outline, continuously sampled merge/remove drag, Alt changes during a stationary
  drag, first touched face style, top-source styles for untouched regions. Open
  dividers inside merged/removed faces disappear; external open pieces survive.
- **Knife / Scissors / Line Cut:** V1 planar splitting and exact cubic split pieces;
  rectangle area is conserved. Incomplete/tangent/missed cuts are no-ops. Scissors
  preserves other compound components and their operations. Line Cut extends the
  dragged axis through source bounds and supports Shift snapping to 45 degrees.
- **Contour:** V1 offset with round/miter/bevel joins, captured outward normal drag,
  detached preview, bounded numeric offset in document units/DPI, inset disappearance.
- **Smooth / Path Eraser / Join:** local scrub neighborhoods, zero-strength no-op,
  exact V1 erasure intervals from arrangement intersections rather than fixed sample
  estimates, endpoint joining, preservation of untouched paths and layers.
- **Transactions:** capture document ID/revision/targets, validate the entire target
  set and inherited locks, bound finite parameters/geometry/output/cache allocation,
  prepare every output and appearance cache before publishing, then one command and
  `Session::edit`. Escape, focus, tool, selection, document and revision changes
  cancel previews. Pointer capture continues outside the canvas and retains all raw
  pointer samples in a frame. Preview does not mutate or journal the source document.
- **Style / compatibility:** existing 08 appearance projection and legacy fills,
  source sibling slots and metadata retained, geometry-dependent corner/live/PSD
  projections cleared after baking. Ordered non-destructive compounds continue to
  use existing V1 commands. No new dependencies or persisted document/settings
  fields; no V1 algorithm, native registry, manifest, lock or catalog edits.

Engine commands are `path.builder.commit`, `path.cut.commit`, `path.erase.commit`,
`path.constructive.commit`, and non-journaled `path.constructive.preview`. The latter
two wrap captured multi-output transactions; existing V1 geometry command IDs remain
registered exactly once. The engine accepts an injected stroke projection for preview;
the default rejects unsupported future width/brush projections before editing.

## Verification

Build, lint and test commands ran offline with Rust 1.98.1, `CARGO_BUILD_JOBS=3`, the required
`li-rustc-gate` wrapper, and one cargo process at a time. No desktop windows were opened.

The single native `cargo check --offline --workspace --all-targets` attempt failed
with pre-existing E0063 at `crates/pc-trace/src/cutout/compose.rs:48`: its `Trace`
initializer omits the new `strokes` field. Package 10 does not own this source.
The exact empty-vector compatibility fix is in the integration handoff. It was
applied only to a disposable verification copy, together with the UI registration
and app hooks, so the otherwise unregistered package UI can be compiled and tested.
Native check log: `target/constructive-workspace-check.log`.

- Engine filtered new tests: **13 passed, 0 failed, 0 ignored**, 1052 unrelated
  tests filtered out. Filter: `constructive_cpu_`, `--lib`, executed once.
  Log: `target/constructive-engine-tests.log`.
- Strict clippy: **passed**, both `photocraft-engine` and `photocraft-ui-egui`,
  `--all-targets -- -D warnings`, including the supplied GPU module. Log:
  `target/constructive-clippy.log`. The existing missing craft-fonts build-script
  advisory remains; there were no Rust/clippy warnings from the changed code.
- UI filtered new tests: **9 passed, 0 failed, 0 ignored**, 1040 unrelated tests
  filtered out. Filter: `constructive_ui_`, `--lib`, executed once. Log:
  `target/constructive-ui-tests.log`.
- A final review corrected coincident Scissors/Contour hit ties to choose the top
  selected source. Its additional focused regression: **1 passed, 0 failed,
  0 ignored**, 1065 unrelated tests filtered out. Filter:
  `constructive_paint_order_`, `--lib`, executed once; the 13 passing engine cases
  were not repeated. Log: `target/constructive-paint-order-test.log`.

Total: **23 new CPU/UI tests passed (14 engine, 9 UI), 0 failed, 0 ignored**.
Formatting passed; all five owned Rust files match their formatted verification
copies, and `git diff --check` passed.

Supplied tests include independent exact polynomial area and analytical circle-lens
oracles, coverage samples, an analytical rectangle signed-distance offset oracle,
real egui pointer/button/modifier/typing interactions, and one full offscreen GPU app
test. The GPU gate drives live preview/release/undo/cancel and asserts RTX 5090,
Vulkan, discrete adapter, float accumulation/f16 support, and premultiplied max pixel
difference <=1/255. It is compiled by all-targets clippy and is **not executed**.

Under the last speed-rule override, whole-crate/workspace test suites, GPU acceptance
execution, release builds and benchmarks were not run. No hardware performance or
GPU pixel-parity result is claimed. V1-Q full EDT/corpus, external application goldens,
PSD corpus and manual QA are separately pending, not ignored/passed tests.

## Integration and sources

[`integration/10.md`](vector-tasks/integration/10.md) contains exact public signatures,
command schemas, app/canvas/overlay/context hooks, full-app GPU registration, eight
tool IDs for package 01, the pc-trace compatibility fragment, and one PORTS row per
adapted upstream file. [`10-register.py`](vector-tasks/integration/10-register.py)
recreates the disposable registration workspace and its exact shared-file patch;
the Pen sentinel is solely a compile/test rehearsal until the actual tool registry
is wired. [`i18n/10.tsv`](vector-tasks/i18n/10.tsv) contains the catalog handoff.

[`SOURCES.md`](vector-tasks/constructive/SOURCES.md) records fixture geometry,
licenses, parameters, versions, pinned upstream file hashes and official reference
links. The implementation follows VectorCraft's builder/cut/helper gesture patterns
at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, Inkscape 1.3 Shape Builder, and
Affinity Contour/Knife documentation. Existing VectorCraft notices were retained.
The task's area-conservation and narrow-erasure requirements motivated adapters
using existing V1 arrangements instead of upstream slits/fixed sampling; no geometry
algorithm was copied or independently replaced. No brand assets or external fixtures
were introduced.

Remaining shared work: package 00 applies native app/test/catalog/attribution hooks,
package 01 registers the tools, 00/27 fixes the baseline `Trace.strokes` omission,
11/17 supplies enhanced stroke projection routing, and the coordinator executes the
deferred integrated acceptance gates. No owner files outside this worktree were changed.
