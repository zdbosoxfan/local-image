# Package 43 — gradient mesh

2026-10-10, branch `wip/vec-43-gradient-mesh`. Changes remain uncommitted. Continued the
existing branch at `1accbad3` (merged work through package 38); preserved that implementation.
The task file was already untracked. No existing package-43 source was present at inspection.

**Status: owned implementation supplied; integrated acceptance remains pending.** The task
reserves native appearance schema/dispatch, production routing and exporters to other owners.
Those hooks are absent here. Native mesh publication reports the missing 00/08 schema explicitly,
and the integration gates remain ignored. This report does not call the integrated package done.
Exact code fragments, APIs and remaining owner work are in
[integration/43.md](vector-tasks/integration/43.md).

## Implementation by requirement

| Requirement | Supplied in this branch | Remaining integration |
|---|---|---|
| Creation | Bounded rows/columns, Flat/To Center/To Edge, highlight; samples existing solid/gradient fill in linear light; captured create command and actual egui controls | Object menu and command aggregate in 00; pattern sampling provider in 08 |
| Mesh tool | Captured point/handle drag, Shift-constrained drag, interior insertion, Shift colour preservation, Alt interior-line deletion, outside release, cancellation on Escape/focus/staleness/selection changes | Production U/tool/persona routing and original icon in 00/01/46 |
| Point colour/opacity | Actual egui colour picker, swatches and opacity slider; detached preview and one release command; common eyedropper `apply_color` entrypoint | Connect existing document swatch/eyedropper colour conversion and normal display preview |
| Geometry/paint | Bicubic tensor-product Coons control nets from shared edge handles, exact de Casteljau splits, local handle edits, affine transforms; independent bilinear premultiplied linear-light colour/alpha | Native paint enum and exhaustive appearance consumers in 00/08 |
| Rendering | Conservative exact subpatch bounds, bounded spatial lookup/inverse, shared seeded gradient dither, exact-area shape clipping and tile-band Surface generation | 08 appearance sampler must apply coverage once; 09 scaled provider/export required |
| Atomic commands | Complete target validation, duplicate/lock/path/numeric/stale checks, all spatial/cache budgets before rendering; reused public stroke and live-corner projection and legacy mirror; detached preview; one `apply_shape_edits` publication | Register public command specs in 00; native schema and mesh dispatch in 08 |
| Persistence | Serde-defaulted mesh wire and options, old native v1 fixture load test; compiled native save/load/preview/undo/redo gate | New-mesh native save/load gate awaits 00/08 and is not claimed passed |
| PDF/SVG/PSD | Bounded raster provider and one aggregated exchange warning per export; exact exporter integration instructions | Actual adapter scanner/writer changes belong to 14/21/23 |
| GPU/scaled/performance | Compiled strict engine and full-app offscreen GPU gates, required 09 provider hook, release-only 20×20 pointer-frame measurement | Integration and owner milestone execution; no measurements here |
| Registration/i18n/attribution | Minimal append-only module registrations, 14 English i18n keys, one PORTS fragment per adapted file and fixture source records | 00 applies command/catalog/PORTS fragments; manifests/lock untouched |

Owned code:

- `crates/pc-vector/src/mesh_paint.rs`, `crates/pc-vector/tests/mesh_paint/`.
- `crates/pc-engine/src/vector_packages/mesh.rs`, `mesh/{tests,gpu_tests}.rs`.
- `crates/pc-ui-egui/src/vector_packages/mesh.rs`, `mesh/{tests,gpu_tests}.rs`.
- Append-only registrations in the three existing crate module files; no aggregate command,
  schema, exporter, catalog, production state or router changes.

Requests use document ID/revision and target IDs, 1–64 shapes, 1–50 patches per dimension,
at most 2601 points, finite coordinates/handle endpoints bounded to 1e6, RGBA/opacity in
0–1, at most 1,048,576 spatial references and 256 MiB prepared pixel caches. Errors publish
no document/history/journal change. Mesh data is never stored in a process-global sidecar
or flattened into a substitute editable shape.

## Validation

Followed the final SPEED RULES, which override earlier full-suite/GPU/benchmark requirements.
Every cargo compilation uses offline Rust 1.98.1, `CARGO_BUILD_JOBS=3`, and the inherited
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`. Only one cargo
build runs at a time. No whole-crate/full-workspace tests, GPU execution, release build,
benchmark or desktop window was run.

| Final check | Result |
|---|---|
| `cargo +1.98.1 check --offline --workspace --all-targets` | PASS; `target/mesh43-check-pass.log` |
| `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-vector -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings` | PASS; `target/mesh43-clippy-pass.log` |
| Changed-crate `cargo +1.98.1 fmt ... -- --check` | PASS |
| `git diff --check` | PASS; Cargo manifests/lock unchanged |
| New vector `mesh43_` tests | 11 unique tests verified passing, 0 ignored; first run 9 passed/2 failed, then only the two corrected failures rerun |
| New engine `mesh43_` tests | 7 passed, 0 failed, 2 ignored; 1176 old unit tests and all old integration cases filtered out |
| New UI `--lib mesh43_` tests | 5 passed, 0 failed, 2 ignored; 1130 old unit tests filtered out |

**Total: 23 new CPU/UI tests verified passing; four integration gates compiled but unrun.**
Vector logs: `target/mesh43-tests-vector.log`, `mesh43-tests-vector-budget.log` and
`mesh43-tests-vector-clip.log`; engine/UI logs: `target/mesh43-tests-engine.log` and
`target/mesh43-tests-ui.log`. The engine and UI filters each ran once; the two failed vector
cases received focused corrective reruns. No successful test case was repeatedly executed.

The existing lightcraft-engine build script warns about missing optional CJK font assets.
The shared `li-test-runner` emitted post-test arithmetic/counting diagnostics (lines 13/18)
after the UI Rust harness reported 5 passed/0 failed/2 ignored; cargo returned exit 0.
Counts above come from the Rust harness output. Neither external shared runner nor font
assets were changed in this worktree.

The first vector run exposed unused outer creation handles extending beyond the coordinate
bound. Fixed them to zero at the boundary, as in the source model. The circle fixture's
zero-coverage corner had the existing exact-area rasterizer's approximately 3e-8 residual;
the assertion now allows 1e-7 while retaining full interior and out-of-shape checks. Only
these two failed tests were rerun after correction; successful tests were not rerun.

Workspace check attempts also exposed an absent shared `scaled_view` crate export and a
missing UI test trait import. Fixed package references with an explicit required provider
hook and imported the existing trait. These were corrective retries, not broad bug-hunting
test runs. Logs remain under the ignored `target/mesh43-*.log` paths.
Final check and lint both passed after all code changes. Also verified egui's installed
float picker expects linear RGBA and added the shared sRGB conversions before the UI run;
the real picker-channel drag test passes with the expected displayed sRGB channel value.

## Integration gates and coverage limits

- Engine native save/load, detached preview, one undo/journal item and redo test: compiled,
  ignored pending real native paint and aggregate commands.
- Engine `mesh_paint_shapes`: RTX 5090/Vulkan/discrete adapter and `Rgba32Float`, finite
  premultiplied max difference ≤1/255 at 1/2/4/8x, both preview and release through a required
  09 projection provider. Compiled, ignored, not executed.
- Full-app offscreen live preview/cancel/release GPU test: compiled, ignored, not executed.
  The package host injects input and detached display into the normal app canvas renderer;
  it must be replaced with production menu/U/state routing before integrated acceptance.
- Release-only full-app 20×20 mesh pointer-frame test: compiled, ignored, not executed;
  prints hardware identity and median/p95/p99/release timings when run at integration.
- Headless module/widget replay tests use a synthetic mesh read provider to assert actual
  click/drag-generated captured requests. They do not prove native publication or menu routing.
- No external Illustrator/PDF/PSD mesh corpus, licensed artwork, mesh round-trip golden or
  PDF shading fixture was supplied. Missing corpus is not a successful gate.
- Folded/self-overlapping or singular meshes use a bounded numerical inverse; no external
  stress-corpus claim is made. Degenerate inverse candidates can produce transparent pixels.

## References and attribution

Read the required architecture, persona (including owner decisions), quality plan, package
index and gap audit. Followed pinned VectorCraft gradient-mesh geometry/interaction/command
sources and official Adobe mesh/PDF documentation. The independent local Coons evaluator,
linear premultiplied colour interpolation and bounded inverse/spatial lookup adapt the
source behaviour to Local Image; they do not duplicate package 20's private Catmull-Rom
envelope or outline partition evaluator.

The pinned krilla 0.8.2 public paint API exposes linear/radial/sweep gradients and patterns,
without type 6/7 mesh shading. Its supported exporter path therefore needs raster fallback.
Sources, licenses, tool versions, fixture parameters, upstream pin and the three exact PORTS
rows are recorded in [43-SOURCES.md](vector-tasks/integration/43-SOURCES.md) and the handoff.
Existing VectorCraft license/notice files are retained, with source notices on all three
adapted modules. No dependencies, Cargo manifests/lock, unsafe code or brand assets added;
no owner files outside the worktree changed and no commit created.
