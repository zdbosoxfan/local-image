# Vector 02 live shapes — implementation and gate report

Status: **not done at the integrated acceptance level**. Owned implementation is present, but the
package-00 module/app/router wiring and package-01/07 dependencies are absent in this checkout.
The task forbids competing edits to those shared files. Exact integration work, attribution rows,
and test registration are supplied in `docs/wip/vector-tasks/integration/02.md`.
No commit was made; all changes remain in this worktree. Initial baseline was 2e8c671f, with only
CODEX-TASK.md untracked. The previous builder's foundation/legacy vector implementation is preserved.

## Implementation

- Every ShapeParameters variant has a bounded finite validator and generator: mixed-kind rectangle,
  ellipse, polygon with curved edges/corners, star/double-star/square-star, triangle/trapezoid/diamond,
  donut/pie/segment, arrow (single/double), rectangular/elliptical callout, cog, heart, tear, crescent,
  cloud, spiral, arc, line, rectangular/polar grids. Generated document Path is authoritative;
  live metadata is the existing Layer.vector.live_shape. No shared model was expanded.
- Hole contours are reversed and Join-owned (donut/cog/full annular segment), not independent Combine.
  Lines/arcs/spirals/grids are stroke-only on creation. Rotation applies equally to all anchors/controls.
- Bounds: finite ±1e6 coordinates, positive dimensions, bounded counts (256), sweeps/ratios/radii and
  total turns. Whole payload preflight precedes cache preparation; caches use existing bounds-clipped
  render_shape. Invalid/degenerate/nonfinite inputs return errors.
- Four commands in the already-registered engine drawing slot: shape.design.create/set/convert/preview.
  Create and changes capture document ID/revision and target IDs, reject stale/locked/duplicate/invalid
  targets, use one Session edit and one journal entry. Detached previews never journal. Convert clears
  extended and legacy live metadata atomically, preserves the authoritative path/paint/cache, and undoes once.
- UI owned module: actual egui input and production pointer hook; rectangle drag, Alt center,
  current-frame Shift square/line angles, snap-provider injection, rotated live handles, numeric/boolean/
  corner-kind controls, unit conversions at document DPI, and cancel/focus/persona/tool hooks.
  Human labels identify each coordinate/corner and dimensionless radius ratios, and zero rotation
  remains editable even though document serialization omits it. Stale base-path metadata is refused.
  Saved state keeps parameters/angle with serde defaults; captures/previews/caches/errors are transient.
- Legacy six forms are exercised through unchanged shape.create and native save/load; frozen v1 fixture loads.
  The underlying four legacy LiveShape enum variants remain unchanged (rounded rect/star are legacy presets).

## Validation (commands are offline, Rust 1.98.1, CARGO_BUILD_JOBS=3)

- `cargo check --offline -p photocraft-engine`: passed.
- Focused `cargo test --offline -p photocraft-engine live_shapes --lib`: initial 13 passed, 1 failed,
  0 ignored, 861 filtered. This caught a callout straight-edge parameterization error, now corrected.
  RTX/Vulkan parity passed in that run for all 25 fixture variants (24 enum variants plus ellipse callout).
- Full `cargo test --offline -p photocraft-engine`: passed. Unit: 865 passed / 11 ignored;
  coverage: 582 passed / 0 ignored; integration: 47 passed / 5 ignored; doc tests: 0.
  Total 1,494 passed, 16 ignored, 0 failures. All 14 package tests present at that run passed.
  Subsequent rotation/angle-handle regressions and final edge-case fixes are checked with the
  `live_shapes` filter rather than repeating the full suite, following the owner's testing budget.
- Final `cargo test --offline -p photocraft-engine live_shapes --lib -- --nocapture`: 17 passed,
  0 ignored, 861 filtered, 0 failures (16 package tests plus one existing canvas guide test).
  This executed all 25 strict GPU fixture variants again on the final geometry source.
- Final coverage audit added `live_shapes_all_family_analytic_bounds`: 1 passed, 0 ignored,
  878 filtered, 0 failures. It checks all 25 fixture variants against independent coordinate-extrema
  oracles, including cubic heart/tear, polar cloud and exponential-spiral bounds.
- `cargo test --offline -p photocraft-vector`: passed. Unit: 39 passed / 1 ignored;
  coverage: 42 passed / 0 ignored; doc tests: 0. Total 81 passed, 1 existing performance probe ignored.
  The new generator is tested through the owned engine bootstrap until 00 registers the module.
- Final owned UI kittest host: 8 passed / 0 ignored / 0 failures, including the final
  zero-angle/stale-metadata regression.
  Actual pointer clicks/drags and typed numeric values execute in egui_kittest 0.36.2.
  An ignored `target/live-shapes-harness` includes the owned UI source by path, using only existing
  root-lock dependencies and no edits to the workspace manifests/lock/registration. Command:
  `CARGO_TARGET_DIR=target cargo test --offline --manifest-path target/live-shapes-harness/Cargo.toml live_shapes`.
- `cargo test --offline -p photocraft-ui-egui`: passed. Unit: 991 passed / 5 ignored;
  GPU integration: 59 passed / 4 ignored; integration: 14 passed / 0 ignored; doc tests: 0.
  Total 1,064 passed, 9 ignored, 0 failures. The owned UI source still requires the separate host
  until 00 registers it; this baseline app suite is not counted as live-shape app acceptance.
- `CARGO_TARGET_DIR=target/clippy cargo clippy --offline -p photocraft-vector -p photocraft-engine
  -p photocraft-ui-egui --all-targets -- -D warnings`: passed. An initial collapsible-if warning
  in the generator was corrected; no competing/shared source was changed.
  `CARGO_TARGET_DIR=target/clippy cargo clippy --offline --manifest-path
  target/live-shapes-harness/Cargo.toml --all-targets -- -D warnings`: also passed, checking the
  unregistered owned UI source and actual kittest tests with workspace-equivalent lint settings.
- Cargo fmt for the three changed crates, explicit rustfmt checks of unregistered owned sources,
  and `git diff --check`: passed.

The engine ignores are existing benchmarks/timing probes (12), slow registry fuzz/child entry (2),
a user-supplied Camera Raw PSD (1), and Photoshop manual output (1). The user-supplied PSD is missing,
not a passing corpus gate. The owner-provided test runner printed an arithmetic diagnostic between
binaries; all test binaries and cargo exited 0. No owner script was modified.
UI ignores are seven existing performance probes, one contact-sheet writer, and one manually invoked
tool-icon GPU test. The latter was not executed or counted as a GPU pass.

GPU test source: owned engine drawing/gpu_tests.rs. It asserts discrete NVIDIA RTX 5090, Vulkan,
Rgba32Float and premultiplied max pixel difference <=1/255. There is no software-adapter fallback/skip.
Observed GPU identity: NVIDIA GeForce RTX 5090, driver 615.71.09 (nvidia-smi); wgpu identity is recorded
by the executed strict test. The supplied full-app GPU source is drawing/app_gpu.rs and is **not yet
registered/executed**; it checks filled mid-drag preview pixels, release, journal/undo, and cancel using
actual offscreen PhotocraftApp pointer frames. An isolated widget pass is not counted as that gate.
No release real-app pointer-frame benchmark was measured and no performance claim is made.

## Remaining integration and limitations

00 must register pc-vector::design_shapes and the UI module/state/router/context/menu/i18n hooks,
use the detached prepared layer in the actual transient GPU canvas path, and clear stale parametric
metadata in incompatible shared Move/node/path edits. 01 supplies persona/tool registry additions;
07 supplies actual snapping. The current exact guide callback proves the consumer constraint
contract only, not acceptance of unmerged 07. Then register/run the provided full-app GPU test.
No desktop window was opened, owner process killed, or owner data outside this worktree changed.

Complex-family templates absent upstream were implemented from the shared parameter contract.
Generic polygon/star corners are quadratic fillets represented exactly as cubics; a V1 circular-corner
projection is not claimed. Segment follows the shared start/end/inner model (chord when inner=0,
annular sector otherwise), not Affinity's separate upper/lower-line controls. Appearance/corner
projection editing is explicitly rejected pending its named provider rather than committing stale pixels.

## Sources and attribution

Pinned local VectorCraft d522c1d7be4035bd4f4a84cd6ebfca44f5155092:
[geom/shapes.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/geom/src/shapes.rs),
[tools/shape.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/shape.rs).
Module copyright/license notices retained; existing vectorcraft license texts/NOTICE retained.
PORTS rows and attribution regeneration fragment are handed to 00 through the owned lane.
Official Affinity [Star](https://affinity.help/designer2/English.lproj/pages/Tools/tools_star.html),
[Cog](https://affinity.help/designer2/English.lproj/pages/Tools/tools_cog.html),
[Square Star](https://affinity.help/designer2/English.lproj/pages/Tools/tools_squareStar.html),
[Segment](https://affinity.help/designer2/English.lproj/pages/Tools/tools_segment.html) help informed behavior.
Local egui_kittest 0.36.2 and existing app/renderer tests informed test/input hooks.

Synthetic fixtures, analytic formulae, tool versions, parameters, ownership/license and source URLs:
`crates/pc-vector/tests/design_shapes/SOURCES.md`. Independent quadrature checks analytical areas,
actual coverage checks holes, grid tests check divider lengths/counts, and corruption tests replace
numeric parameters individually. No external/proprietary corpus was provided; none is reported passed.
Core manifests/Cargo.lock/registries/catalogs/PORTS/aggregate test registration were not edited.
