# Vector 18 live blend implementation report

Branch: `wip/vec-18-live-blend`; continued the existing V1/vector integration foundations
at `b546f0dc`. No previous builder work discarded. Changes are uncommitted.

Owned evaluator, engine commands, controls, spine interaction, synthetic acceptance and
GPU acceptance sources are implemented. The root UI module/new crate registrations are
included under the latest REGISTRATION override. Production menu/router/UiState/display
connections remain in package 00's explicitly owned lane with exact fragments in
[integration/18.md](vector-tasks/integration/18.md). Integrated full-app acceptance is
**pending**, not reported as passed. The latest SPEED RULES prohibit GPU runs, whole-crate
suites, release builds and benchmarks here.

## Scope delivered

- Pure pc-vecfx blend evaluator adapts pinned VectorCraft correspondence/interpolation.
  Source keys stay unchanged and editable; generated nodes are detached ordinary layers.
  Steps 0 returns exact keys; multiple keys retain paint order. Distance follows upstream
  closest spacing convention; SmoothColor uses bounded Oklab distance counts.
- Winding and start normalization avoid reversed contour twists. Explicit transient
  starts are available through PathPair::with_starts. Joined compound subpaths match by
  containment role and normalized geometry, preserving ordered native component ops.
  Missing contours collapse/grow continuously. Unequal node counts use common arclength
  knots, keeping curve pieces intact rather than pairing mismatched corners.
- Geometry/handles, opacity, appearance fill colors/gradients, gradient alpha, stroke
  widths/dashes/offset/profile widths interpolate. Paint colors use published Oklab with
  exact endpoint values. Extra appearance paints fade on both sides of the midpoint.
- Spine placement uses native V1 curve lengths, bounded 32-step inverse arc length and
  native exact tangent evaluation, with direct straight-line interpolation. Page/path
  orientation and default key-center spines are supported. Shared V9 live metadata is
  retained on the ordinary source group; projection and expansion clear it only on the
  detached/result group.
- Engine create/options/expand/release/info commands use captured document ID/revision/
  target IDs. Missing, duplicate, overlapping ancestor/descendant, inherited-locked and
  stale targets fail before publishing. Create requires adjacent sibling keys to preserve
  unrelated clipping/paint order. Commands prepare caches before one Session::edit and
  one journal entry; previews do neither. Expansion IDs are deterministic and above all
  document/layer/symbol-master IDs. Undo restores live groups and original key geometry.
- Lazy UI display projection keys its cache by immutable source Arc identity. Node/Move
  previews selected by the host are projected by the same evaluator, so key edits update
  intermediates without mutating keys or creating a second renderer.
- Real egui controls support Smooth colour, Steps (including 0), Distance, orientation,
  create/apply/expand/release and spine editing. Actual pointer gestures insert/move
  spine anchors, Shift constrains deltas and Alt adjusts the outgoing handle using current
  modifiers. Escape/focus/persona/tool/selection/revision/dialog changes cancel detached
  previews. Old settings deserialize with defaults and transient state is never saved.
- File adapters receive `project_scene(ImportedScene, PixelFormat, &[Pattern])` through
  vector_design::exchange; pc-vecfx depends on neither UI nor engine. No new external
  dependencies or unsafe/C/C++ code introduced. The engine source bridge compiles the
  identical provider until 00 adds its crate dependency.

## Bounds and compatibility

Limits: 2–32 keys, 0–1000 intermediates per span, 4096 projected layers, depth 16, 64
subpaths/4096 knots per path, 1,048,576 aggregate projected knots, ±1,000,000 coordinate
components, distance .01–1,000,000 and one non-inverted 2–128-anchor spine. Non-finite
inputs are errors. Before raster allocation, the existing bounded appearance compiler
checks geometry and an aggregate 256 MiB new-cache budget is enforced in a first pass.
Source caches/patterns and ordinary clipped/effect siblings remain ordinary compositor
inputs. Budgets reject oversized blends atomically.

No shared schema change. BlendSpec `{}` still loads. The current read-only BlendSpec
has no upstream persisted `starts`/`key_anchors`: automatic start matching works, explicit
pair starts are implemented, but a persisted clicked-key-start gesture awaits 00's
additive defaulted schema. Group keys must be curves/ordinary curve groups; artboards,
text/symbol/raster keys, nested live geometry and unprojected masks/corners/clips fail
explicitly. Named enhanced brush/profile geometry providers remain pc-vector's existing
responsibility; missing providers produce errors, not successful substitute rendering.

pc-color's documented Oklab API is absent on this branch. The owned adapter uses the
published MIT matrices and existing pc-color transfer functions; the integration handoff
includes promotion to pc-color and complete attribution. No competing shared color edit. Shared Cargo.lock is left to 00: the temporary
validation-generated local-crate entry is supplied verbatim in integration/18.md; no
registry packages or versions changed. Root workspace-dependency and UI module
registration additions are the minimal latest-rule exceptions.

## Verification

All Cargo commands use `PATH=~/.cargo/bin:$PATH`, Rust 1.98.1, `--offline`, three jobs,
the pre-existing li-rustc-gate RUSTC_WRAPPER and one cargo build at a time. Formatter run
on the three changed crates; the future app acceptance source is explicitly rustfmt'd.

| Gate | Result |
|---|---|
| Workspace check, all targets (once) | PASS, 2m58s; log `target/blend18-workspace-check.log` |
| Strict clippy, all targets, three changed crates | PASS on final source, -D warnings; log target/blend18-clippy-complete.log |
| pc-vecfx new evaluator tests | PASS: 14 unique tests, 0 outstanding failures; only affected/failed tests rechecked |
| pc-engine new command tests | PASS: 4 passed, 0 failed, 1 GPU ignored; 1064 unrelated tests filtered out |
| pc-ui-egui new egui_kittest tests | PASS: 5 unique tests, 0 outstanding failures; 1054 unrelated tests filtered out initially |
| Registered engine RTX acceptance | 1 ignored, not executed (owner speed rule) |
| Full-app offscreen RTX acceptance source | 1 supplied; registration awaits 00's app hooks, not executed |
| Full crate/workspace test suites | Not run (owner speed rule) |
| Release app pointer-frame benchmarks | Not run; no performance claim |
| Proprietary/external fixture corpus | Not present; zero external corpus gates claimed |

Total: **23 unique new CPU tests pass** (14 evaluator/DTO, 4 engine, 5 interaction).
The workspace check ran once; final changed-source type/lint checks ran through strict
all-targets clippy. The existing unrelated craft-fonts build-script notice remains.

The initial evaluator run passed 11/12 and exposed a curved-spine tangent accuracy issue.
The curve oracle retained its tolerance; the implementation changed from flattened chord
angles to native curve tangents. Only that failed test and the affected straight-spine
oracle were rechecked, both passed. The additional appearance/profile oracle was run
once and passed; the new exchange-DTO export oracle also passed once. Other passing
tests were not repeatedly rerun. The initial five UI tests passed four and exposed an
incorrect modifier-test coordinate expectation: V1 inserts a straight-segment corner
with collapsed handles, not an implicit smooth cubic handle. The expected coordinate
was corrected using the established V1 contract; only that failed UI test was rechecked
and passed (1058 tests filtered out). Initial clippy requested derived Default; fixed
and the strict gate passed on the next attempt. Final strict clippy passed after the
tangent/paint/malformed-spine refinements and new test additions. Logs/counts are recorded in the table and command log list.

Independent oracles include analytic radius/bounds, neutral-axis Oklab/sRGB, rectangle/
hole area and raster coverage, cubic length 200/midpoint (50,75), spine spacing/tangents,
paint alpha .4t, numeric stroke/profile midpoints, ID uniqueness and immutable model/
revision/journal snapshots. UI tests click, double-click, type, drag, cancel and undo.
They are isolated controls tests; they do not impersonate full-app acceptance.

Focused execution logs (all within this worktree):

- `target/blend18-evaluator-tests.log` (initial 12 new tests)
- `target/blend18-curved-spine-fix.log` (failed curve oracle only)
- `target/blend18-straight-spine-fix.log` (affected line oracle only)
- `target/blend18-appearance-tests.log` (new appearance/profile oracle only)
- `target/blend18-scene-tests.log` (new exchange DTO oracle only)
- `target/blend18-engine-tests.log` (`cargo test --offline -p photocraft-engine --lib blend18_`)
- `target/blend18-ui-tests.log` (`cargo test --offline -p photocraft-ui-egui --lib blend18_`)
- `target/blend18-ui-modifiers-fix.log` (failed UI modifier oracle only)
- `target/blend18-clippy-complete.log` (final strict check of all targets in changed crates)

No passing old crate/workspace suites were executed. All unexecuted GPU/corpus gates
remain distinct from the passing CPU counts.

GPU tests assert RTX 5090/Vulkan/discrete/Rgba32Float and compare premultiplied max diff
≤1/255 with clipped/effect layers. The full-app source uses actual PhotocraftApp,
context controls, real canvas pointer routing and GpuCanvas preview/release, expand,
cancel and undo. No software/absent adapter can count as a passing gate. No desktop
app/window was opened and no owner's files outside the worktree were changed.

## Sources and handoff

Read all required architecture, persona (including Owner decisions), quality plan and
vector task ownership instructions. Followed the pinned VectorCraft sources, installed
V1/appearance/session APIs and the official [Adobe blended objects documentation](https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/blended-objects-overview.html).
Oklab formulas follow [Björn Ottosson's published 2021 matrices](https://bottosson.github.io/posts/oklab/).
Full fingerprints, PORTS rows, retained MIT/Apache notices and the attribution-generator
fragment are in [18-SOURCES.md](vector-tasks/integration/18-SOURCES.md).
Synthetic fixture sources, parameters and tool versions are in
[blend/SOURCES.md](../../crates/pc-vecfx/tests/blend/SOURCES.md).

00 must merge [18.md](vector-tasks/integration/18.md), register the full-app test,
connect the shared display/export projector and merge [18.tsv](vector-tasks/i18n/18.tsv)
into supported locales. It owns the persisted clicked-start schema and optional pc-color
promotion. GPU and release app performance verification stay deferred to integration
under the task's latest rules; none of those missing gates are labelled complete.
