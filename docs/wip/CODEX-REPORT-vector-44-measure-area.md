# Package 44 Measure/Area report

Status: owned implementation and test source supplied; shared production integration
is pending package 00/01/46. Integrated acceptance is **not claimed complete**.
Continued branch `07b53336` with existing V1/07/16/06 foundations preserved. On entry
only CODEX-TASK.md was untracked; there were no Measure/Area implementation files or
prior package report to overwrite. No commits, new dependencies or lockfile edits.

## Implemented

- Detached distance/angle/ΔX/ΔY queries, exact node/edge snapping through 07+V1,
  guides/page/artboards, current modifiers on preview and release. Shift is a hard
  45° constraint preserving raw length (VC constrain_angle), even with snapping
  disabled or Ctrl bypass. Captures document,
  revision and target IDs; stale/missing target publication fails.
- Pixel/point/mm/in presentation through package 16's exact conversion definitions;
  geometry remains document pixels. Session-only overlay/readout persists after
  release until the next measurement; Escape/focus loss restores the last completed
  line. Tool/persona/module changes cancel captured previews. Frozen press transform,
  captured release outside canvas, no raster rendering or edits for a reading.
- V1 hole-aware area, area including holes, total boundary perimeter, Full edges and
  Cusped segments; selected objects sum (overlaps intentionally counted per task).
  Groups and selected descendants deduplicate. Hover honors paint order/visibility.
  Open fills use V1 straight closure for area, while length omits the implicit edge.
  Per-revision detached area cache, no layer cache rebuild on warm pointer frames.
- Native live circles use PI/TAU exact analytic metrics after validating their
  metadata reproduces the authoritative path; arbitrary curves retain actual kurbo
  arc lengths. This avoids misrepresenting four-cubic circle approximation as exact.
- Optional scale adapter, finite/bounded input, lengths ×factor and areas ×factor²,
  1:100 support, Assign from raw measurement and real dimension. Prepared ScaleEdit
  checks staleness/targets/artboard locks and runs one atomic metadata Session::edit
  through 00's setter. No fake persistent scale command is advertised.
- Real widgets for units/perimeter modes/typed real-length/Assign; clipped overlay
  painter and production canvas adapter. Pixel Ruler and its registries untouched.
- Append-only engine/UI module registration; exact command aggregate, persona/tool,
  canvas/options, scale persistence, i18n and attribution fragments in
  `docs/wip/vector-tasks/integration/44.md` and `i18n/44.tsv`.

## Verification

Completed checks:

- `cargo +1.98.1 check --offline --workspace --all-targets`: passed, once
  (`target/measure44-check.log`). Existing build-script notice: craft-fonts not
  embedded for Chinese/Japanese, unrelated to this package's synthetic fixtures.
- Strict clippy `--offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings`:
  passed (`target/measure44-clippy-final.log`). Initial attempt found one owned
  collapsible-if style lint; it was fixed and lint resumed. No remaining diagnostics.
- `cargo +1.98.1 test --offline -p photocraft-engine --lib measure44_`: **7 passed,
  0 failed, 0 ignored, 1206 unrelated tests filtered** (`target/measure44-engine-tests.log`).
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --lib measure44_`: **5 passed,
  0 failed, 0 ignored, 1143 unrelated tests filtered** (`target/measure44-ui-tests.log`).
  Real pointer events, clicks and typed input run in a headless host; no GPU/desktop
  window was created.
- Final magnitude-preserving Shift refinement: affected test only rerun, **1 passed,
  0 failed, 0 ignored, 1212 filtered** (`target/measure44-shift-test.log`). No broad
  suite repeated. The UI run exercised the final Shift implementation.
- `cargo +1.98.1 fmt -p photocraft-engine -p photocraft-ui-egui -- --check` and
  `git diff --check`: passed. Dormant GPU source also rustfmt-parsed/formatted.
- Strict all-target clippy on the **final** engine/UI source: passed
  (`target/measure44-clippy-completed.log`). This narrow resume followed the Shift
  change and fixed-height UI fixture; no full test suite was repeated.

Owner SPEED RULES supersede earlier full-crate/GPU/performance instructions: one workspace
all-target check, strict clippy of changed crates, only new package tests. Build env
uses Rust 1.98.1, offline, CARGO_BUILD_JOBS=3 and the preserved li-rustc-gate wrapper.
No whole-crate test suite, release build, GPU run or benchmark is authorized here.

New test source: seven engine tests and five egui_kittest tests, prefix measure44_.
Independent oracles and parameters in
`crates/pc-engine/src/vector_packages/measure/SOURCES.md`.

Two real-app offscreen GPU/release tests supplied in
`pc-ui-egui/src/vector_packages/measure/app_gpu.rs`, for 00's central registration.
They require RTX 5090/Vulkan/discrete/Rgba32Float (software/skip fails), exercise
actual production route live/release and area hover, assert document/revision/history/
journal stability, unchanged layer texels against CPU premultiplied diff <=1/255,
visible overlay GPU pixels and no document layer refresh. Release source records
hardware and median/p95/p99/release pointer+GPU/readback time with warm p95 <16ms.
The two GPU tests have ignored attributes in their source, are not yet registered
in the central test binary, and were **not compiled as GPU tests or run** per ownership/
speed rules; no hardware performance claim. They contribute zero successful gates.

## Remaining shared integration

00 owns command aggregate, canvas/menu/options hooks, scale data/native persistence,
central GPU registration and i18n/PORTS/attribution regeneration. 01/46 own the two
new distinct tool IDs and icons/flyout. These are explicit ownership dependencies,
not silent successes. Scale fields do not exist in the current shared data model;
absence remains 1:1. Native backward-compatibility for the optional persisted field
must be verified by 00 after it is added; package changes do not alter native schema.
Full-app CPU/GPU integrated acceptance must follow production hook installation.
Unsupported text/symbol/live-group area selection returns an explicit provider error;
inverted unbounded fills return V1 NeedsClip rather than zero success. Shared feature
projection should supply those vector paths in integration. No external/proprietary
fixtures supplied, and missing corpus contributes no successful gates.

Sources: read architecture/persona spec (including Owner decisions)/quality plan,
package README/gap audit; inspected pinned VC Measure implementation and existing
07 snapping/16 units/V1 geometry/06 corners/egui test and GPU harness patterns.
Official Affinity Measure/Area and Adobe Measure distance/area docs were fetched
2026-10-10. Detailed links, licenses, adapted-file PORTS fragments, interpretation of
cusped runs, synthetic fixtures and tool versions are in SOURCES.md. No brand assets.
