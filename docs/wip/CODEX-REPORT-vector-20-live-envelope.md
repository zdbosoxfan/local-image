# Package 20 — live envelope implementation report

Implementation is registered and compiles on this branch. Native envelope reload
is blocked by an existing shared serde tag collision; the new roundtrip test fails
and the exact schema repair is supplied in `integration/20.md` for package 00.
Production menu/canvas routing, exchange consumers and integrated RTX acceptance
also remain in package 00's serial integration lane. This is not a completed
native-persistence or integrated-acceptance claim. The final speed rule forbids executing GPU
acceptance, release builds and benchmarks in this job; none is claimed passed.
Changes are uncommitted.

## Implemented

- Continued the existing integration baseline (`abef8e8c`), preserving the shared
  `LiveGroup::Envelope` / `EnvelopeSpec` schema, group hierarchy, appearance,
  path/text providers and command registry. The existing envelope slot was empty;
  no prior envelope implementation or tests were discarded.
- Registered the new `photocraft-vecfx` crate, its public envelope module and the
  engine's minimal internal-crate consumer dependency under the brief's registration
  override. Registered `photocraft_ui_egui::envelope` with an append-only module
  declaration. Lock changes contain only this local crate and the engine's local
  dependency; no registry version, feature selection or external package was added.
- All 15 pinned VectorCraft warp styles and horizontal/vertical percentage
  distortion; normalized tensor Catmull-Rom mesh with linear edge extrapolation;
  Coons top-object mapping with exact four-corner cubic boundaries and bounded
  flattening/arc-length partition for general closed outlines. Largest closed
  outline wins deterministically; invalid/degenerate frames return an error.
- Exact identity and affine mesh paths retain native handles, explicit open/closed
  contours, fill rule and ordered PSD component operations. Flat axes use a stable
  centered 1px source frame. Inputs/options/coordinates/grids are finite and bounded.
- Nonlinear fitting uses pinned kurbo `fit_to_cubic`, adaptive subdivision and a
  second bidirectional 65-sample geometric guard. Default tolerance is 0.05px;
  options permit 1e-6..10px. The distance bound is numerical (as kurbo documents),
  not an interval proof. Budget exhaustion is an error rather than an inaccurate
  successful bake: 8192 source nodes, <=32x growth / 65536 output nodes, depth 16,
  one million mapped samples, 32x32 mesh cells, top boundary <=16384 samples
  with flattening accuracy min(0.005px, fitting tolerance/8).
- Engine `.set`, `.preview`, `.expand`: captures document ID/revision and every
  source/group target; rejects duplicates, overlap, changed sets, inherited locks,
  unsupported sources and malformed geometry before publication. Entire target
  geometry evaluates before bounds-limited cache preparation (256MiB per set).
  Previews are detached and non-journaled; commits make one journaled command and
  one Session edit. Source shape paths, metadata and editable text runs remain
  native. Expand publishes ordinary paths/groups and is undoable as one edit.
- Native and path text use existing pc-text outlines, run colours, synthetic bold
  and decoration geometry. Projection outline IDs are deterministic, disjoint from
  existing source IDs and never consume global IDs on pointer frames. Expand gives
  generated children native IDs. Layer opacity/clipping/effects are retained once
  on the text projection container.
- egui controls: warp style/bend/distortions, mesh rows/columns, mesh overlay/handle
  drag, Apply/Cancel/Expand. Press captures the view transform, including flip;
  Shift is sampled per frame. Escape, focus loss, target/tool/persona/document
  switch and stale revision cancel. Numeric changes preview until explicit Apply;
  mesh release commits once. Cancel abandons egui numeric buffers with a fresh ID.
  Focus-loss events cancel even if a host's raw focused field lags behind; starting
  a mesh drag retains a pending resize and the original document capture.
  Source edits/undo invalidate display projection via revision; native reload
  uses the same hook once 00 repairs the shared wire schema;
  an explicit hook invalidates font/paint generation changes.
- Integration signatures, production call sites, pure exchange dependency boundary,
  attribution rows and rendering limitations are in `integration/20.md`. UI keys
  are supplied in `i18n/20.tsv`; shared catalogs/PORTS/generated attribution assets
  remain with 00. Existing VectorCraft license/copyright notices were retained.

## Verification

All cargo commands used `--offline`, Rust/Cargo 1.98.1, `CARGO_BUILD_JOBS=3` and the
inherited shared `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`.
Only one cargo process ran at a time. No desktop window, ComfyUI job, external owner
file mutation, commit, release build or benchmark was performed.

- Workspace all-target check: **passed**. Log: `target/envelope-workspace-check.log`.
  Failed compiler attempts during bring-up were corrected before the successful
  final check; no workspace tests were run.
- Strict clippy, all targets of `photocraft-vecfx`, `photocraft-engine` and
  `photocraft-ui-egui`, `CARGO_TARGET_DIR=target/clippy`, `-D warnings`: **passed**.
  Final-source log: `target/envelope-clippy-final.log` (11.70s). Earlier bring-up
  log: `target/envelope-clippy.log`. Initial compiler/lint failures were corrected;
  no new source warning is suppressed. The successful workspace check was not
  repeated after focused corrections; final clippy checked all changed crate
  targets, including the two compiled ignored GPU gates.
- Formatting and `git diff --check`: passed.
- New tests only: **21 passed, 1 failed, 2 ignored** (22 distinct CPU/UI tests,
  two additional GPU gates). Geometry: 8/8 passed. Engine: 8 passed, 1 failed,
  1 GPU ignored. UI: 5/5 passed, 1 GPU ignored. Filter `envelope_`, with
  `PHOTOCRAFT_NO_SYSTEM_FONTS=1` for engine/UI to fix the text provider to the
  repository's OFL Inter/JetBrains Mono fonts. Logs:
  `target/envelope-tests.log`, `target/envelope-remaining-tests.log`,
  `target/envelope-ui-fixes-tests.log`, `target/envelope-geometry-tests.log`.

The first filtered run reached the engine: 8 passed, 1 failed, 1 ignored. The
failure is the native ZIP reload test, with `unknown variant warp, expected blend,
repeat, envelope`. Both shared enums are tagged `kind`; native JSON-value parsing
retains only the inner tag. No fix was applied to the task's read-only shared
schema file. The handoff includes an exact wire adapter and legacy compatibility
cases. The failing test remains active; it was not converted to an ignored/pass
gate. The initially unrun geometry/UI tests executed separately without repeating
the engine tests that already ran. A UI focus-event test failed initially; after
the fix, only that test and the newly added pending-mesh-resize regression were
executed, both passing. All other new tests ran once. Full-crate/workspace suites,
GPU runs, release builds and benchmarks were skipped under the latest speed rule.

The build emits its existing LightCraft notice that optional craft-fonts CJK fonts
are absent. This package's test strings use committed Latin fonts; no CJK/external
font corpus success is inferred from that build.

## GPU tests supplied, deferred gates and integration limits

Two compiled ignored GPU gates are supplied:

1. Engine projection/compositor parity for warped path and visibly red text,
   detached preview and committed projection, premultiplied max diff <=1/255.
2. Offscreen full-app renderer with a package-owned input/display host, real mesh
   press/move/cancel/release and warped text preview/release, CPU-app/GPU-app canvas
   comparison within one premultiplied byte. The test host restores original source
   Arc/revision after every app frame and does not journal a preview. It exercises
   the actual app renderer, while production menu/router registration awaits 00.

Both assert RTX 5090, Vulkan, discrete device and Rgba32Float; the app gate also
requires float canvas support. Missing/wrong adapters fail, never silently pass.
**Neither gate was executed**, as required by the final speed override. No RTX,
interactive latency, release performance or full integrated acceptance claim is made.

00 still owns production menu/persona/canvas/Navigator/scaled-view routing, common
SVG/PDF/PSD exchange projection and shared attribution/i18n catalogs. Exact hooks
are supplied. The final app test should be switched from test-host injection to
those production hooks at integration, then run on the real GPU milestone.

Existing appearance is rendered after geometry distortion. Width/brush projection
uses the appearance provider's explicit named dependency error until 11/17 supply
it; there is no misleading “distort gradients/strokes/patterns” toggle. Source
raster/vector masks and vector clips and nested other live groups require the unified projection
provider and are explicitly rejected. Parent group/artboard compositor behavior
is retained. No proprietary or external envelope corpus was provided or counted
as passed; production manual QA and those corpora remain integration work.

## Sources followed

- Read all required architecture/persona/owner-decision/quality/integration documents.
- Verified local VectorCraft checkout HEAD is exactly
  `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` and clean. Followed
  [doc/live.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/live.rs)
  and [effects/warp.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/warp.rs),
  MIT OR Apache-2.0. Four per-adapted-file PORTS rows are in the handoff.
- Read cached kurbo 0.13.1 source/docs for
  [ParamCurveFit](https://docs.rs/kurbo/0.13.1/kurbo/trait.ParamCurveFit.html) and
  [fit_to_cubic](https://docs.rs/kurbo/0.13.1/kurbo/fn.fit_to_cubic.html).
  The bounded driver, bidirectional guard, affine detection and transaction/UI
  adapter are original Local Image code where no existing bounded adapter existed.
- Followed existing native text/path-text conversion, live-shape/corner projection,
  `CompiledAppearance`, Session edit/journal APIs and egui 0.36 panel/kittest sources.
- Fixture licensing, generator coordinates, versions, parameters and independent
  analytic/symmetric geometry oracles are recorded in
  `crates/pc-vecfx/tests/envelope/SOURCES.md`.
