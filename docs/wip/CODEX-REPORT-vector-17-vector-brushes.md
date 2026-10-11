# Package 17 — Vector brushes

Branch baseline: `8e069442`. Changes are uncommitted. The inherited engine brush slot was
empty; there was no pc-vecfx brush implementation or brush fixture directory to discard.
Existing V1 geometry, pencil fitting/tablet samples, native appearance data, transaction
helpers and merged-appearance compositor behavior were preserved and reused.

## Implemented scope

- Registered new local `photocraft-vecfx` crate, with no new registry dependencies or
  upgrades. The lock diff contains only its local package entry. All new production code
  is Rust, safe, formatted and uses the workspace lint policy.
- Adapted pinned VectorCraft elliptical calligraphic nib sweep, corner-aware arc-length
  tracks, art stretch/proportional/between-guide mapping, seeded scatter, pattern fitting,
  corner/end tiles, flips and orientations. Art uses native closed paths and stroke paint;
  no upstream tree, rasterizer, text engine, bristle/texture assets or brand assets copied.
- Actual V1 normalization cleans folded/self-overlapping output into ordinary filled paths.
  Tight art corners get explicit breakpoints. Zero-length, malformed, unknown-resource and
  excessive-geometry inputs return bounded errors instead of silent empty/truncated output.
- Pressure becomes absolute-pixel WidthProfile points on normalized stabilized arc length;
  all four brush kinds consume it. Calligraphic sweep includes pressure breakpoints even
  when fitting reduces the source curve to two knots. Scatter uses an explicit persisted
  SplitMix64 seed and is byte-identical across four OS threads.
- Versioned self-contained resources use the existing opaque BrushRef.id; the actual native
  pc-format ZIP writer/loader roundtrip retains refs/profile and resolves every preset.
  Unknown refs fail explicitly. IDs fit the existing renderer's 4096-byte bound; larger
  custom art needs 00's resource table. Shared document/persistence types were not expanded.
- Three registered journaled engine commands: `vector.brush.commit`, `.apply`, `.expand`.
  Captures contain document ID/revision and insertion or complete target IDs. Validation
  checks staleness, all inherited locks and IDs before projecting caches; one Session::edit
  publishes each operation. `vector.brush.preview` is a registered non-journaled query;
  prepared preview documents are detached. Target sets are bounded to 64 layers with
  a 512 MiB aggregate cache budget. Cache bounds
  intersect the canvas and have a 16,777,216-pixel-equivalent tile budget. The actual
  existing package 08 CompiledAppearance/StrokeProjection interface supplies cache paint;
  brush pieces are united before opacity/blend, shared with ordinary expansion.
- Registered UI module with Calligraphic/Round/Art/Scatter/Pattern preset controls, bounded
  width/tolerance, pressure, rope stabilization, nib angle/roundness and scatter seed.
  Captured drag/release follows existing tablet/pencil ToolEvent behavior. Escape, focus loss,
  tool/document/revision change, errors and context changes discard transient state.
  Old missing settings deserialize to defaults. Kittest tests actually click, type and drag.
- Exact 00/01/08/11 hooks, dependency-bridge cleanup, full-app GPU test registration,
  attribution append rows and twelve-language translation fragment are supplied in
  `docs/wip/vector-tasks/integration/17.md`, `i18n/17.tsv`, and the fixture source record.

## Validation

Owner speed override followed: cargo/rustc 1.98.1, offline, CARGO_BUILD_JOBS=3,
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate, one cargo
operation at a time. No whole-crate/workspace test suite, GPU run, release build or benchmark.

- `cargo +1.98.1 fmt -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui`: passed.
- Strict clippy all-targets for those three crates, `CARGO_TARGET_DIR=target/clippy`,
  `--offline -- -D warnings`: **passed** after all final changes, 34.29 s. A first attempt
  found a moved-layer assignment; it was fixed.
- `cargo +1.98.1 check --offline --workspace --all-targets`: **passed** after the final
  changes, 50.47 s. An initial attempt found wrong appearance import paths, corrected.
- `cargo +1.98.1 test --offline -p photocraft-vecfx brush17`: **9 passed**, 0 failed,
  0 ignored. Independent ellipse/flat-nib widths, analytic 1000-pixel² pressure lens,
  exact pattern spacing/fits/corner/end reserves, art corner occupancy and tight folds,
  deterministic scatter, bad-input/budget errors and reference serde.
- `cargo +1.98.1 test --offline -p photocraft-engine brush17`: **5 passed**, 0 failed,
  **1 ignored** (the supplied GPU gate), 1054 existing unit tests filtered out. Detached
  preview/query/commit, pressure and undo/redo, malformed/stale/locked gestures, all-target
  atomicity, all-preset native ZIP resource resolution and premultiplied expansion parity,
  translucent scatter coverage and the existing gradient paint stack.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui brush17`: **3 passed**, 0 failed,
  0 ignored, 1040 existing unit tests filtered out. Real kittest preset clicks, width
  typing and pressure drag; detached preview/release, Escape/focus/context/tool cancellation,
  stale revisions, missing-settings defaults and undo.
- New CPU tests total: **17 passed**, 0 failed. The one registered ignored GPU test and
  one unregistered full-app GPU source are counted separately, not as successful gates.
- Translation fragment structure: 180 distinct translations, 15 keys for each of 12
  languages, no duplicate language/key pair.
- `git diff --check`: passed.

Execution logs (worktree-local, ignored build artifacts): `target/brush17-check-final.log`,
`target/brush17-clippy-final.log`, and `target/brush17-tests-{vecfx,engine,ui}.log`.
The inherited lightcraft-engine build emits its existing missing craft-fonts warning;
no dependency/font asset or unrelated shared source was changed for this task.

## GPU gates supplied; deferred acceptance

Registered ignored engine test `brush17_rtx5090_vulkan_preview_release_expanded_geometry_parity`
checks all presets, CPU/GPU premultiplied max diff <=1/255, live detached preview, release,
ordinary expansion and undo. It requires **RTX 5090, discrete Vulkan, Rgba32Float** and fails
if unavailable; no llvmpipe success or skipped-adapter path.

Full PhotocraftApp offscreen GPU test source is in UI `brushes/gpu_tests.rs`. It verifies
actual canvas pointer-frame preview, release/expanded pixel equality, journal granularity,
cancel and undo. It is not registered before 00 supplies the app field/router and 01 the
tool variant. Its source targets the exact documented `vector_brush`/`Tool::VectorBrush` hooks.
No GPU test was executed, as the current task expressly prohibits those runs. No interactive
speed or GPU hardware measurement is claimed.

This is a compiled package implementation and integration handoff, **not completed integrated
app acceptance**. Shared persona/menu/router/prefs/catalog wiring belongs to 00/01. The actual package 08
renderer is used by brush refresh, including gradient/pattern and stroke-gradient paint.
The shared appearance/translate/zoom/export call sites still need 00 to compose
BrushProjection with 11's width provider. Brushed arrows/dashes/corners/unbrushed width,
and ordinary expansion of paints requiring a coordinate bake, remain explicit errors
rather than erased metadata or stale pixels. The shared
resource table is absent: 00 can intern the supplied versioned resources while retaining
this decoder for saved documents. Final tool registration, enhanced renderer hooks and
full-app GPU/manual QA are the named integration work that remains.

## Sources, fixtures, attribution

Read-only VectorCraft checkout HEAD verified at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`;
MIT OR Apache-2.0 copyright notices accompany every adapted source/test file. Existing root
license/NOTICE and curated VectorCraft attribution entries were inspected. 00 receives one
PORTS row per adapted file and the exact attribution text addition; shared generated files
were not edited.

[The pinned brush crate](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/brush/src),
[pinned appearance model](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/doc/src/appearance.rs),
and [official Affinity Vector Brush behavior](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_Brush.html)
were followed. Native geometry, safe budgets, versioned refs and pressure consumption were
adapted to Local Image's established model because upstream's document/tree/render stack is
out of scope. Detailed fixture parameters, versions, sources/licenses and PORTS fragment:
`crates/pc-vecfx/tests/brush/SOURCES.md`.

External/proprietary Affinity/Illustrator brush corpora and raster-texture fixtures are
missing, **not counted as successful gates**. All executed fixtures are generated synthetic
Rust geometry or native ZIP documents. Broad regression suites, real GPU gates, release
performance and manual QA are deferred to integration under the owner's explicit speed rule.
