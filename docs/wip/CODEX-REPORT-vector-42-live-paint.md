# Package 42: Live Paint and Vector Flood Fill

Date: 2026-10-10. Changes are uncommitted in `vec-42-live-paint`; no dependencies,
manifests, lockfile, shared data variants, app router or locale catalogs were changed.
Continued the existing branch and reused its V1 geometry, appearance renderer, command
transactions and preview conventions. **Integrated acceptance is incomplete**: the
shared `LiveGroup::LivePaint` variant and production display/export/menu/tool routing
are absent on this branch and belong to 00/01/09/46. The exact handoff is
[integration/42.md](vector-tasks/integration/42.md).

## Implemented within the owned package

- A bounded face/edge graph using V1 `shape_builder`, `merge_regions`, `area` and
  geometry helpers. Closed-only input uses V1's full native Live Paint map so blank
  enclosed holes remain paintable; empty sweep failures return an error. This package
  adds no replacement planar engine. Complete filled coverage-cell selections merge
  through V1's exact closed arrangement, preserving source curves across cutting lines.
- Stable signatures store source child slots and directed source segment ownership.
  Face and edge paints survive the requested 5-pixel source movement and ID remapping
  when topology is retained. Ambiguous face signatures fail explicitly. Gap bridges
  close dangling endpoints at Small/Medium/Large = 4/8/16 document pixels, without
  modifying sources or producing visible bridge strokes.
- Flood Fill creates ordinary shapes with Inside/In-between placement, Add on top,
  Smart refill, Knockout and matching-colour Visible Boundaries. It preserves outside
  source fills/strokes and handles transparent fill stacks. Adjacent faces merge per
  insertion target; different source wrappers keep their individual layer semantics.
- Captured Make/Paint/Options/Merge/Expand/Release commands and non-journaled preview,
  info, sample and selection queries. Merge transfers painted faces and split edges.
  Edits validate document/revision, the entire target set, inherited locks, geometry,
  numeric limits and bounds-limited caches before one `Session::edit`. Command/action
  mismatches fail. Failed preparation leaves document/history/journal unchanged.
  Existing `Session::execute` still records a successful no-op edit command in its
  journal; no-op preparation creates no history entry.
- Detached display and exchange-scene projection to plain painted faces/edges, using
  the established cache renderer. The serde bridge fails explicitly until the shared
  data variant is added; it never persists paint in names or unrelated variants.
- Compiled egui tool module with K Bucket, Shift edge painting, Alt sampling,
  Shift+L face/edge Selection, Affinity R Flood Fill, context controls and gap options.
  Pointer capture retains its press transform through outside release; Escape, focus
  loss, stale capture and tool/persona transitions cancel detached preview. Selection
  signatures are transient. The production router must call these hooks.
- Minimal module registration in the three affected crates and one engine aggregate
  command registration line. No shared menu/router/schema changes. Fixed strings and
  source attribution fragments are supplied to their shared owners.

## Validation

Validation results are recorded below. Every Cargo build uses
`cargo +1.98.1 --offline`, `CARGO_BUILD_JOBS=3`, the existing `li-rustc-gate` wrapper,
and one Cargo build at a time in this job. Formatting uses the requested toolchain.

| Gate | Result |
|---|---|
| `cargo check --offline --workspace --all-targets` | PASS after corrections (11.17s); `target/live-paint42-check-final.log` |
| Strict clippy, affected three crates, `--all-targets -- -D warnings`, `target/clippy` | PASS after corrections (20.33s); `target/live-paint42-clippy-postfix.log` |
| New `live_paint42` vecfx tests | FAIL: 7 passed, 2 failed, 0 ignored, 7 filtered; `target/live-paint42-vecfx-tests.log` |
| New `live_paint42` engine tests | FAIL: 4 passed, 1 failed, 2 ignored, 1162 filtered; `target/live-paint42-engine-tests.log` |
| New `live_paint42` egui tests | PASS: 4 passed, 0 failed, 3 ignored, 1122 filtered; `target/live-paint42-ui-tests.log` |
| Formatting / `git diff --check` | Formatting applied; diff check clean |

The successful initial check follows one failed workspace-check attempt (a new evaluator
Rect API mismatch). Strict clippy compile/lint retries corrected new-code document
clone/API mismatches, a needless borrow, collapsible conditionals and the intentionally
runtime release-build assertion. Check/lint are repeated only to validate the two
product corrections prompted by the once-only test failures. An existing
lightcraft-engine build-script warning reports missing optional craft-fonts; no new
Rust warning is suppressed outside the documented test-only lint allowances.

The once-only vecfx run exposed two failures: `sparse_pointer_frames_hit_subpixel_regions`
(3 faces generated, but only 2 hit across the 0.01-pixel-wide cell), and
`visible_boundaries_merge_only_connected_matching_paints` (merged area versus the V1
Boolean union differs beyond 1e-5). Source inspection identified the open-path merger's
refitting as a likely cause of the latter, and complete-cell merges now use V1's exact
closed arrangement. That pure regression was not rerun under the once-only limit;
the first engine/UI runs use the updated implementation. The narrow-cell failure remains
unresolved; its planar geometry and hit test need integration-lane investigation.
Both regression tests remain enabled with stronger failure diagnostics. No failed gate
is counted as a pass, and the precision failure is not hidden by widening the fixture
or its tolerance.

The once-only engine run passed visible-boundary fill, transparent Add/Smart/Knockout,
query/journal safety and invalid/stale/locked captures. Its flood/undo test failed on
the extra selection revision before reaching the undo assertions. Inspection found
that standalone `set_selection` increments revision and clears canvas damage; commit
now uses the established structural-edit `reselect` helper, retaining the one edit's
history target and damage. The engine filter was not rerun. The first UI run uses this
correction and independently exercises release and undo; all four normal UI tests pass.

Across the three once-only filters: **15 passed, 3 failed, 5 ignored**. Two failures
prompted product corrections; their original filters were not repeated, so neither
is reported as a newly passed regression. The narrow-cell hit-test failure remains.
Exact commands: `cargo +1.98.1 test --offline -p photocraft-vecfx --lib live_paint42`,
then the same filter for `photocraft-engine` and `photocraft-ui-egui`, sequentially.

CPU tests use analytic half-circle/lens areas with the established cubic approximation
tolerance, V1 exact area comparisons, and independent pc-vector PSD Intersect raster
coverage (premultiplied maximum difference <= 1/255). Other fixtures check a 3-pixel
Small gap, a 5-pixel Medium gap, a subpixel region crossed between sparse pointer frames,
blank ring holes, native ID remapping, transparent fill modes, stale/invalid/locked
targets, one undo and unjournaled queries. Egui kittest really clicks, drags, types and
sends keyboard/modifier/focus events.

Five supplied integration tests remain ignored and **do not count as passes**:

1. Engine native Make, face/edge paint, source movement, Merge, save/load, Expand and Release.
2. Engine real-GPU preview/commit/Expand parity.
3. Headless Bucket click/drag, Shift edge painting and actual Expand button.
4. Offscreen production `PhotocraftApp` shortcut/pointer dispatch, detached live preview,
   release, Escape cancellation, Expand and undo with canvas parity.
5. Release production app press-to-painted-frame latency (20 samples, median/p95/p99),
   with painted-output verification and GPU readback.

The GPU gates require RTX 5090 / Vulkan / discrete adapter and `Rgba32Float`; no fallback
adapter or missing-adapter skip passes. They compare premultiplied pixels <= 1/255.
No GPU acceptance runs, release builds, benchmarks or whole-crate/workspace test suites
were executed, per the owner's final SPEED RULES overriding the earlier gates. No
hardware parity, speed or full-app acceptance claim is made. No proprietary/external
corpus was supplied or counted as successful.

## Remaining integration work and limits

00 must install the additive shared data source/variant, coordinate the exhaustive
matches and native wire codec with 18–20, then register shared projection and export
dispatch. 01/46/00 must register tools, icons, menus, persona/keymap routes, colour-chip
binding, context bar and canvas preview/input hooks. The handoff gives exact fragments,
signatures, serde fields and call order. After that, enable the two native/headless
acceptance gates and run the real-GPU/production app gates at the integration milestone.
The package cannot be called integrated/done before those gates pass.
Resolve the still-enabled narrow-cell regression and verify both corrected failed
filters at that milestone as well. The current branch does not meet the complete-task
acceptance gate, despite passing compilation, strict lint and headless interaction.

Topology-changing node edits and arbitrary source reordering require signature
reconciliation; unmatched paints are retained for topology restoration without applying
to unrelated faces. Merge appends source slots. Other live geometry, corners, clipping
and masks must be projected before region construction; unsupported targets error.
Flood Visible Boundaries matches the visible fill representation of selected sources;
it does not claim flattened-compositor equivalence for arbitrary effects/blends.

## Sources, fixture provenance and attribution

Read the architecture, persona specification including Owner decisions, quality plan,
task index and gap audit. Followed pinned VectorCraft initialization/lifecycle logic at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, its builder interaction and build tests,
the existing local V1 adapters, and official Adobe/Affinity interaction documentation.
Read egui/egui_kittest 0.36.2 source for input modifiers and the existing project's
labelled numeric controls. Signature, gap, flood decomposition and lifecycle code are
original where the established sources supply no directly usable solution.

Exact source URLs, fixture geometry, tool versions, licenses and two per-file PORTS
rows for 00 are in [SOURCES.md](../../crates/pc-vecfx/tests/live_paint/SOURCES.md).
MIT OR Apache-2.0 upstream notices are retained, and existing VectorCraft license files
are referenced. No brand assets were copied. Locale strings are in
[i18n/42.tsv](vector-tasks/i18n/42.tsv).
