# Vector package 08 — Appearance V4

Status: implementation supplied in the owned lane; **integrated acceptance remains pending**.
The shared router/registration and PSD dependency gates below are not counted as passes.
Changes are uncommitted. Base inspected: `4a4af2ed`, including the previous builder's V0
#1321 gradient port, document appearance models, vector transaction boundary and engine slot.
Those foundations were retained; no shared model/registry/manifest/lock/catalog was edited.

## Supplied implementation

- `pc-vector/src/appearance.rs` compiles authoritative bottom-to-top paints through the
  existing exact-area rasterizer and `pc-color::blend::composite`. `None` preserves legacy
  paints; `Some(empty)` paints nothing. Each item has visibility, opacity and blend mode.
  Solid, all existing gradient styles/fields and document patterns are supported. Missing
  patterns are explicit errors. Dissolve uses the existing document-space dither.
- Pattern sampling supports phase, link-to-shape origin, rotation, uniform scale and wrapped
  bilinear interpolation in premultiplied color. Transparent tile neighbors do not introduce
  a colored fringe. SVG capability checks report skew, nonuniform scale, reflection,
  singular/nonfinite transforms and repeat/reflect gradient spread for the adapter's fallback
  collector. Native Fill cannot encode those SVG features; no import success is claimed.
- Native strokes retain existing caps, joins, alignment and relative dashes. Fitted dashes
  center on endpoints and real corners, including the closed starting corner; curve samples
  do not restart dashes. Fitting uses source Bezier tangents and starts a closed run at a real
  corner, avoiding a false restart at a redundant smooth starting knot. All eleven arrow styles are supplied, including hollow rings and
  distinct barbed/open arrows. The line and heads share coverage, applying opacity once.
- Stroke gradients use object coordinates or nearest flattened arc/normal coordinates for
  Along/Across. Along restarts on each subpath. Across spans the aligned visible band and
  interpolates native absolute left/right profile widths supplied with an outline provider.
  This sampler does not implement VC gradient meshes or its export slice generator.
- `scale_with_object` applies the pinned area-scale convention once when a transform is
  baked; fixed-width strokes remain fixed and relative dash values remain relative. Legacy
  PSD mirror chooses the topmost visible representable fill/stroke, folding fill opacity into
  alpha. A translucent pattern fill is skipped because legacy Fill cannot encode its item
  opacity. Edited shapes clear their obsolete raw PSD path payload.
- Engine set/info/preview commands and captured `Gesture` record document ID, revision and
  target IDs. Detached previews do not edit Session or journal. Every target's IDs, locks,
  numbers, paths, resources and projected geometry are validated before publication. Set
  prepares all caches and calls the existing transaction boundary once. Failure, staleness
  and invalid last targets leave the document/history unchanged.
- The compositor's owned `split_layer` and `uses_legacy_outline` helpers and both GPU plan
  routes keep explicit stacks on their merged cache. The required CPU lib.rs call sites are
  exact fragments in [integration/08.md](vector-tasks/integration/08.md); these prevent PSD
  mirror strokes from being painted again over clips/effects.
- The Appearance/Stroke module supplies add/reorder/hide/duplicate/delete, per-item opacity,
  blend and paint controls; gradient stops, midpoints, opacity stops and pattern transforms;
  stroke width/alignment/caps/joins/miter/dashes, fitted dashes, arrows/scales, scaling and
  gradient mode. Multi-target mutations retain each target's unrelated stack items.
- Fill/Transparency handles use actual pointer capture, endpoint editing or a new chord,
  stationary modifier changes, 45-degree Shift constraints, final release outside canvas,
  detached preview and one release command. Fill preserves an existing gradient style;
  Transparency edits the selected paint's opacity stops while retaining its color stops.
  Escape, pointer/focus loss, disabled tool and stale documents cancel. Numeric entry stays
  detached while focused and commits on Enter/focus loss. Old option JSON defaults load.

## Bounds and projection contract

Inputs reject nonfinite/out-of-range numbers. Limits include 128 stack items, 4096 stops or
profile points, 64 targets, 16384 path nodes/subpaths, 262144 flattened points, weighted
geometry and stroke tessellation/arrow/fitted-dash budgets, and coordinates within ±1,000,000. Rendering uses tile-sized
RGBA buffers; a standalone region cannot exceed 1,048,576 pixels. Prepared caches have a
512 MiB conservative **padded tile storage** budget across all targets, checked before render.

`StrokeProjection::outline(&Path, &StrokeItem) -> Result<Path, String>` returns an aligned,
dashed, arrow-complete filled outline. Native visible profile/brush strokes explicitly fail
without packages 11/17. Synthetic injected outlines test the contract and profile-band color
oracles, not those packages' implementations. No new dependency or unsafe code was added.

## Verification environment and scope

Linux, Rust `+1.98.1`, offline Cargo, `CARGO_BUILD_JOBS=3`, one build at a time.
Hardware: NVIDIA GeForce RTX 5090, Vulkan, DiscreteGpu, NVIDIA driver `615.71.09`, PCI
`0000:01:00.0`. The new GPU gates assert that identity and `Rgba32Float`; absent/software
adapters fail. All compositor comparisons use premultiplied RGBA max error <= 1/255.
GPU parity also explicitly rejects nonfinite pixels before taking the maximum error.
No desktop window was opened or owner process/files outside the worktree altered.

Because module registration and CPU lib.rs hooks belong to 00, validation uses
`target/appearance-integration`: copies of the five changed crates, unchanged workspace
crates/assets linked to this checkout, the existing workspace manifest/lock, and only these
registration fragments applied to the copies:

1. `pc-vector/src/lib.rs`: `pub mod appearance;`.
2. `pc-compose/src/lib.rs`: both routing fragments from integration/08.md.
3. UI manifest: the isolated `tests/appearance/main.rs` test target, with no dependency change.

`target/appearance-sync.py` refreshes these copies. The original shared files remain untouched.
The engine appearance slot was already registered at the base. The widget test includes its
module directly; the actual full app has **not** been registered by this validation overlay.
This distinction matters: a passing widget is not a passing app acceptance gate.

Commands use `--manifest-path target/appearance-integration/Cargo.toml` and the existing
absolute worktree `target` for tests; clippy uses `target/clippy`. Relevant logs are in `target/`.
The final suite ran once after the coordinator pause. The earlier process ended during
compilation, with zero test executions; its log is `appearance-full-tests-before-pause.log`.
The resumed build reused cached artifacts and included the import cleanup and finite-pixel
assertions. No full suite was repeated.

| Verification | Passed | Failed | Ignored | Evidence |
|---|---:|---:|---:|---|
| Final renderer focused run | 15 | 0 | 0 | `appearance-vector-focused.log` |
| Final engine appearance filter | 8 | 0 | 0 | `appearance-engine-focused.log`; 7 owned tests and 1 existing comp appearance test |
| Final UI widget/SVG/RTX target | 11 | 0 | 0 | `appearance-ui-focused.log` |
| Full `photocraft-compose` | 199 | 0 | 0 | `appearance-full-tests.log` |
| Full `photocraft-engine` | 1521 | 0 | 19 | Same log; lib + coverage + integration + V1 |
| Full `photocraft-gpu` | 91 | 0 | 0 | Same log; includes 46 GPU tests |
| Full `photocraft-ui-egui` | 1091 | 1 | 11 | Same log; catalog dependency below |
| Full `photocraft-vector` | 102 | 0 | 1 | Same log |
| **Full-suite total** | **3004** | **1** | **31** | Cargo exit 101; no acceptance pass claimed for the failing catalog gate |
| Clippy, five changed crates, all targets, `-D warnings` | Pass (5 crates) | 0 | — | `appearance-clippy.log`; Cargo exit 0 |

The full-suite engine GPU run measured max error `0.000000059604645` for stack/clipping
preview, commit and redo; `0.00021520257` for effects preview/release, and zero for an explicit
empty stack with shadow. These are strict real-GPU results, not skipped fallback gates.

The sole failing test is `i18n::tests::every_tl_literal_is_translated`. Its source scanner
includes unregistered UI files. Japanese currently lacks 42 scanned labels: 28 are covered
by this package's 109-row English handoff, and 14 belong to the existing node/tool sources.
No catalog was changed because catalogs belong to 00. Installing translations is required
before rerunning this focused gate; the failure is retained, not hidden by changing the
scanner or the labels.

The 31 ignored cases are existing cases: 22 performance measurements, 3 node/V1 integration
gates, 2 slow/child panic-hunt entries, 2 manual review/export entries, 1 explicit tool-icon
GPU gate, and 1 user-supplied Camera Raw PSD fixture. `PHOTOCRAFT_CAMERA_RAW_PSD` was not
supplied for this run, so that external fixture remains unvalidated. The unrelated craft-fonts build warning is recorded separately
below. The appearance full-app sources are unregistered and are not included in these counts.

Final suite command:

```sh
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" \
  cargo +1.98.1 test --offline --manifest-path target/appearance-integration/Cargo.toml \
  -p photocraft-vector -p photocraft-compose -p photocraft-gpu \
  -p photocraft-engine -p photocraft-ui-egui --no-fail-fast -- --nocapture --test-threads=1
```

Formatting checks cover all five changed crates and the unregistered renderer/UI/test roots;
both pass. `git diff --check` passes. Clippy uses the same package list with `--all-targets
-- -D warnings` and `CARGO_TARGET_DIR="$PWD/target/clippy"`.
The first three lint attempts found only owned mechanical issues: a nested profile-validation
condition, a redundant `Ok(...?)` result wrapper, and two constant-size chunk iterators in
renderer assertions. These were corrected without lint suppression or behavior changes;
the failed logs are retained as `appearance-clippy-attempt-{1,2,3}.log`. The final attempt
passes all targets. The full-suite results above precede only these mechanical cleanups;
the suites were not repeated.

## Acceptance fixtures and interaction gates

Owned renderer tests include the required **six exact sample pixels** for two fills and two
strokes, hidden/reordered/empty/legacy behavior, an independent general-alpha Multiply
formula, patterned transparency, dash endpoints/corners, analytic triangle/shaft union area
64 px², hollow head coverage and single opacity, along/across geometric ramps, per-subpath
restart, aligned/profile bands, atomic transform validation and explicit provider failure.

Engine tests cover native save/reopen, old native fixture loading, detached preview command,
one undo/redo/journal transaction, duplicate/stale/locked/missing-pattern targets, invalid
numbers and padded-tile budget rejection before allocation. Strict GPU tests are named
`appearance_stacks_on_shapes_rtx5090_preview_release_clipping` and
`layer_effects_on_appearance_shapes_rtx5090_merged_alpha`.

The isolated egui_kittest host exercises actual add/reorder/hide/duplicate/delete clicks,
Fill and Transparency drags, slider drag, click/type/Enter width entry, radial endpoint drag,
multi-selection preservation, stationary Shift and release outside canvas, focus/pointer/tool
cancellation and staleness. A strict RTX widget render compares visible preview/release frames
and cancellation. Its image texture is a test host, not a proposed app canvas route.

resvg/usvg 0.45.1 is already present through `egui_extras`; no new dependencies were needed.
Tests render intrinsic 128×128 SVG gradient and phased stripe-pattern references and compare
interior pixels within 1/255. Separate skew/repeat fixtures differ from native projection and
produce explicit capability warnings. Exact SVG and renderer parameters, native source,
synthetic fixture licensing and upstream pins are in the owned `appearance/SOURCES.md` files.
No missing proprietary corpus is counted as successful acceptance. The existing LightCraft
build script reports absent craft-fonts (CJK glyph coverage); this is not an appearance
fixture pass and the English widget tests do not validate that unrelated visual coverage.

## Remaining integrated acceptance (named dependencies)

- **00/01:** register module/test/UI state/options, tools and actual panel/router/cancellation;
  route detached Document plus generation through the normal CPU/GPU canvas. Apply CPU split
  and effect-outline hooks. Register and run `tests/appearance/app_acceptance.rs`, which uses
  the actual `PhotocraftApp` offscreen GPU harness and tests preview/release, undo, cancellation
  and panel clicks. This source deliberately depends on the documented future state/tool
  interfaces and is not counted as a compiled/passed test yet.
- **00 catalogs:** install the supplied translation rows in the complete language catalogs
  and resolve the full-suite `every_tl_literal_is_translated` failure. Its 14 other labels
  are from existing node/tool sources, outside this package's ownership.
- **00 plus geometry-edit packages:** call appearance cache refresh for path edits, transforms,
  native refresh and pattern edits, and scale stroke/profile width once. Existing consumers
  that reject appearance edits must retain their rejection until the refresh hook is installed.
- **11/17:** install the real outline provider into renderer/engine paths and supply final
  profile/brush acceptance. Default commands reject those paints until then.
- **23:** collect the documented lost-feature PSD warnings and perform PSD export/reopen
  pixel/warning acceptance. Native ZIP roundtrips are not a substitute for this gate.
- **Performance:** no speed claim. An ignored release-only real-app pointer-frame measurement
  source records hardware and median/p95 after router registration. It is unregistered and
  unrun, not an ignored successful GPU acceptance test.

The task explicitly reserves these shared files/dependencies and forbids competing edits.
Exact fragments, signatures and source attribution are supplied in integration/08.md;
109 English translation rows are in i18n/08.tsv for 00's catalog pass. The package must not
be marked done until the dependencies and full-app/PSD gates pass.

## Sources followed

Read the architecture, persona including Owner decisions, quality plan and integration
contract; existing pc-vector coverage/gradient/stroke, pc-color blend, pattern compositor,
vector_api, native format fixtures and egui_kittest/wgpu harnesses before implementation.
Read pinned VectorCraft source locally at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` for
stack, arrows, fitted dashes and stroke gradient semantics. Pattern placement follows the
PhotoCraft fork at `c18d18cf613422fe08d31001e1f803cf8ef7dd68`. Retained MIT/Apache copyright
notices and supplied one PORTS row per adapted file for 00 to regenerate attribution.
Original bounded compilation, engine transactions, UI and analytic fixtures use existing
Local Image interfaces; no duplicate VC renderer or new geometry dependency was introduced.

Primary references: [SVG 2 paint servers](https://www.w3.org/TR/SVG2/pservers.html) for
transform/spread/pattern coordinates, [W3C compositing and blending](https://www.w3.org/TR/compositing-1/#blending)
for the alpha oracle, and locally pinned egui 0.36.2 widget/accessibility and egui_kittest
source for actual pointer/keyboard/offscreen behavior. VC's mesh/export slicing is outside
this native paint sampler; the nearest flattened metric uses the existing exact-area coverage.
