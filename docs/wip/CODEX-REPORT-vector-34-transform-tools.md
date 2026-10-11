# Package 34 — reference-point transform tools

Date: 2026-10-10. Branch: `wip/vec-34-transform-tools`. Changes are uncommitted.

## Status and inherited work

Continued the branch at `ab753378`, preserving the merged drawing/live-shape, snapping,
width/corner, typography, symbol, brush and V1 geometry work. At initial inspection there
were no package-34 implementation files or tracked local edits; the task file was untracked.
The new package builds on those established providers rather than replacing them.

Owned implementation and integration fragments are supplied. **Integrated acceptance is
pending lane 00 wiring and milestone gates**, as required by the ownership boundary and
final speed override. This report does not call the isolated package full-app accepted.

## Delivered scope

- Rotate, Reflect, Scale and Shear reference-point tools: snapped visible pivot, real pointer
  drags, Shift constraints, current release Alt copy flag, Alt-click/Enter numeric dialogs,
  detached Preview, Copy/OK/Cancel, stroke/effect/corner/object/pattern options.
- Reshape node selection and Shift toggle, anchor/handle translation with pinned VC quartic
  distance falloff; node IDs and complete target set validated before editing. Repeat after
  copying remaps node targets into the corresponding new object/group.
- Free Transform movement, rotation, edge/corner scaling, constrained proportional scaling,
  side shear, three modes, perspective paired corners and Free Distort individual corners.
  Homogeneous rational cubic and bilinear degree-six composition use adaptive endpoint
  Hermite refits and Bernstein whole-interval error certificates, maximum 0.25 destination
  pixels. Node/depth/coordinate budgets, convexity, singularity and horizon checks fail atomically.
- Distorted symbols/text prompt for curves or a typed non-destructive envelope handoff.
  Symbols use the existing detach provider; ordinary warped text uses shared layout,
  warp adapter and glyph outlines. Unwarped styled/path text uses package 12 painted
  outlines and an owned adapter following its native contour conversion, preserving colours and decorations. Conversion remains part of the same edit. Envelope routing
  is supplied via `EnvelopeRouter`; package 20 is currently an empty compiled command slot.
- Transform Again resolves the last successful vector apply for the active document, keeping
  operation/options/copy. Existing Pixel transform state and Ctrl+D routing are untouched.
- Transform Each uses each selected object's own nine-point bounds reference, movement,
  scale, rotate, reflect and seeded xorshift64* Random. Groups share their root's transform.
- Move numeric entry supports horizontal/vertical, absolute position, distance/angle and Copy.
- Similarities retain representable live geometry and scale corner parameters per option;
  other transforms bake to curves. Unrepresentable reflected cogs/spirals bake per persona §5.
  Width/profile/effect dimensions scale using similarity magnitude or area-equivalent scale.
  Group clips and linked vector masks transform with descendants; unlinked masks stay fixed.
- Every gesture captures document ID, revision and roots. Inherited locks and descendants
  preflight, geometry/cache preparation is detached, aggregate caches are capped at 512 MiB,
  and publication is one command/one Session::edit. Preview, cancellation and failed requests
  do not journal. Copy recursively allocates new layer IDs and retains original hierarchy.
- Defaults load from empty old settings. Dialogs use bounded numeric parsing, accesskit
  TextEdit labels and existing i18n::t. English lookup fragments are in `i18n/34.tsv`.

## Files and shared-lane handoff

New files are confined to the engine/UI transform_tools modules and tests, vector projective
module and tests, and package documentation. Shared registration is only nine appended lines
across the engine package slot/specs and vector/UI module exports, authorized by the task's
registration override. No dependency, manifest, Cargo.lock, old transform_tool/move source,
edit.transform command, persisted shared type, tool registry or translation catalog changes.

[Integration instructions](vector-tasks/integration/34.md) provide exact public signatures,
command payloads, tool IDs, icon aliases, scoped shortcuts, lifecycle/input/paint/context-bar
hooks, detached display_doc/Navigator plane selection, dialogs, and envelope provider hooks.
00/01/46 own app field/router/menu/keymap integration and catalog/attribution regeneration.
Exporters use the pure projective module with vector_design::exchange, without UI/engine deps.

## Validation

All cargo commands: toolchain `+1.98.1`, `--offline`, `CARGO_BUILD_JOBS=3`, retained
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`; one cargo
build at a time. No app window was opened and no running app was killed.

Workspace `cargo check --offline --workspace --all-targets` passed after the initial
compiler correction. Final strict clippy for all three changed crates, all-targets,
`-D warnings`, passed. Formatting and `git diff --check` passed. The workspace check
predated the final private adapters/regressions; those were subsequently compiled by
changed-crate all-target clippy and the focused test builds. No public dependency graph
or shared registration changed after the workspace check.

Every new CPU test was attempted once. Counts below are **initial execution results**;
filtered totals are per invocation and should not be added as unique test counts.

| Crate / filter | Passed | Failed | Ignored | Library filtered | Other targets filtered |
|---|---:|---:|---:|---:|---|
| vector / `transform34_` | 5 | 0 | 0 | 61 | coverage 42; view 3 |
| engine / `transform34_` (initial 16 cases) | 16 | 0 | 0 | 1090 | coverage 582; integration 52; V1 13 |
| engine / `transform34_conversion_` | 1 | 0 | 0 | 1106 | coverage 582; integration 52; V1 13 |
| engine / `transform34_auxiliary_` | 1 | 2 | 0 | 1107 | not reached after library failure |
| engine / `transform34_corner_pattern_frame_` | 1 | 0 | 0 | 1110 | coverage 582; integration 52; V1 13 |
| UI / `transform34_` (initial 8 cases) | 0 | 8 | 0 | 1068 | not reached after library failure |
| UI / `transform34_keyboard_focus_regression_` | 1 | 0 | 0 | 1076 | appearance 11; drawing 66; brush 14; layout 8; path text 6; pen 8; snapping 10 |

Total: **35 new CPU tests attempted once; 25 passed, 10 initial failures, 0 ignored**.
The ten failed cases were corrected but **not rerun**, following the owner's test-once
limit; no success is claimed for their final interaction/effect/budget assertions.

- The original eight UI cases failed during harness startup, before interaction, with
  egui's recursive context-lock timeout. The keyboard-focus query was inside `ctx.input`
  and was moved outside that read lock. The distinct keyboard-focus/cancellation regression
  passed on the corrected implementation, using real pointer/key events.
- Two auxiliary type cases lacked appearance-cache initialization required by package 16's
  cached bounds provider (`layout target has no geometry`). Their fixtures now call the real
  `type_cmds::refresh`, using sparse COW copies for the 128-text budget case. The nonlinear
  pattern-phase case passed. The corrected two type fixtures compile but remain unexecuted.
- The added rounded-corner/pattern-frame case passed its single execution. A final audit
  aligned old/new pattern origins and the fillet oracle with the renderer's projected
  path control bounds rather than analytic curve bounds. That refinement was made while
  this test build was in progress; its final source is compiled by strict clippy, but the
  test was not rerun and final-source runtime success is not asserted.

Compiler/lint corrections included a text-warp adapter mismatch, an egui API rename,
test ownership, private/duplicate contour module registration, and collapsible-if findings.
Logs are under `target/transform34-*`; these are local build artifacts, not fixture evidence.
The coordinator must complete interaction and corrected type assertions at integration.

New CPU coverage includes independent known-coordinate/area/falloff formulas, analytic
homography/bilinear maps sampled at 129 points per refit interval, closure/fill/operation
preservation, deterministic random, exact copy offsets, stale/locked/invalid atomicity,
undo, converted symbol Copy, painted text conversion, group auxiliary geometry, pattern
phase during nonlinear/rounded-corner geometry, editable text effects, aggregate pre-raster type budgets
and provider failures.
Eight egui_kittest scenarios are written to click, drag, type into TextEdits and send shortcuts, including
R/snapped pivot/preview/release, Alt-click/45/Copy/Ctrl+D, E corner/Escape, modifier Copy,
Move entry, Each seed/cancel, Reshape and stale cancellation. Their host is offscreen and
uses production State/Session; it is not represented as the coordinator's full app router.

GPU test supplied: `transform_tools_real_gpu_detached_preview_release_and_cancel_parity`.
It uses the production compositor, requires RTX 5090/Vulkan/discrete/Rgba32Float, checks
premultiplied CPU/GPU max diff <=1/255 and exact GPU preview/release/cancel/undo document
parity. It neither ignores nor successfully skips an unsuitable adapter.

**Not run by the owner's final speed override:** whole-crate/workspace suites, real-GPU
acceptance, release builds, benchmarks and 500-shape warm release pointer-frame measurements.
No GPU identity/parity/performance success or <16 ms claim is made. After lane 00 wiring,
the full-app offscreen preview-plane/release/input/journal and hardware performance gates
in integration/34.md remain necessary. No proprietary/external application fixture was
available or counted as a passing comparison. Existing craft-fonts build warning records
missing CJK font corpus; no CJK conversion success is inferred.

## Remaining provider limits

Affine type/symbol bounds inherit package 16's appearance-cache precondition. Native loads
and edits must use the existing type/path-text/symbol refresh hooks before these gestures.
Empty or uninitialized type/symbol appearance currently returns a no-geometry error; this
is not claimed as an uncached-model transform API.


Existing pattern schema expresses similarity and phase, not arbitrary affine/reflected or
non-affine paint frames. Those operations return an explicit error; ordinary similarity,
Patterns-only transforms and compensation with Patterns disabled are implemented. Shape/group linked pixel masks still need a bridge to the existing mask warp provider;
affine type/symbol masks already use the shared layout provider. Styled warped text needs a painted warp outline provider. Live Blend/Repeat/Envelope roots require their owner to expand
or supply transform support. Combined brush/profile/arrows remain limited by existing
projectors. These failures preserve the document and history rather than approximating silently.

## Sources and attribution

Read the architecture, full persona including owner decisions, quality plan, package README
and gap audit. Used the read-only VectorCraft clone pinned and verified at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, its transform/free/Reshape and engine object/menu/path
commands, plus official Adobe Rotate/Transform/Distort behavior documentation. The bounded
projective refitter is original; no obvious existing certified mapper was exposed by the
established providers. Existing V1 bounds, corners, appearance, width/brush, snapping, symbol
and text providers were reused. Details, synthetic fixture licensing, tool versions and
parameters are in [source records](vector-tasks/integration/transform34-SOURCES.md).
[PORTS fragments](vector-tasks/integration/transform34-PORTS.md) give one row per adapted
file; copyright/license headers and existing VectorCraft license/NOTICE files are retained.
No proprietary code or brand assets were copied.
