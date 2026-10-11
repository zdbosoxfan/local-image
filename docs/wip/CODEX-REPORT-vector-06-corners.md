# Package 06 — live corners

Status: package-owned implementation and integrated offscreen acceptance complete.
Final focused UI tests, strict GPU gates, release measurements, formatting and all-targets
lint passed. Ready for the documented serial 00/01 integration.
The shared 00/01 registration lane remains authoritative. No commit, shared registry/catalog,
manifest/lock change, desktop window, new dependency, or owner's document edit was made.

## Implemented

- Safe native `Path` + indexed `KnotCorner` projection for Round, Inverted, Chamfer and Straight.
  Straight retains the original sharp corner and its stored radius for switching kinds.
  Line cuts preserve VectorCraft's tangent-angle setback and half-edge clamp. Curved Round
  solves the two radius-offset curves, with a circular fillet tangent to both trimmed sides;
  Inverted intersects each side with a circle about the original knot. Original incident
  cubics are trimmed with de Casteljau; untouched edges, fill rule, component ops and inversion
  remain intact. Projected knot provenance points back to the base knots.
- Finite/coordinate/radius/knot/sample/cache budgets, duplicate/out-of-range/ineligible index
  rejection, deterministic static safety caps, and fail-closed crossing/offset-root checks.
  Static caps do not depend on the requested radius, so effective radius is monotone while
  dragging. Safety checking uses bounded adaptive flattening at 0.001px; the independent
  dense-sampling tests cover the generated curved fixtures. This is not a mathematical
  certification of every possible pathological cubic at arbitrary scale.
- `CornerGesture` captures document ID/revision and target IDs. Detached preview is neither
  journaled nor published. Preview updates only captured targets, reusing a lazy projected
  baseline. Commit revalidates the complete captured set, prepares bounds-clipped caches and
  calls one `vector_api::apply_shape_edits` / Session::edit through `path.corner.set`.
  `path.corner.preview` is non-journaled. Base geometry remains editable and is never baked.
- UI widget hit/drag, per-node/selected subset/all, kind menu and bounded typed radius.
  Pointer capture runs before canvas paint, retaining the original view transform through relayout and outside release; modifiers are read each frame. Kind-only changes
  preserve mixed per-node radii. Numeric gestures capture staleness and abandon the egui edit buffer on cancellation. Escape, focus
  loss, tool/persona/document/target/revision changes cancel. Committed render projections
  are revision-keyed, and held previews carry a separate key.
- Native save/load, old v1 loading, cancel, single undo/redo, atomic bad-target rejection,
  base-index remapping for 03, and bounds-limited cache allocation on a 6000x4000 document.

## Independent acceptance tests

- Exact cubic-polynomial Green's-theorem integral against square/circle formulas:
  Round `s²-(4-pi)r²`, Chamfer `s²-2r²`, Inverted `s²-pi*r²`, Straight `s²`.
  Cubic arc subdivision <=11.25 degrees; explicit Round allowance
  `2e-8*r² + 1e-8` square pixels. 256 seeded rotated/translated rectangles use analytic
  ground truth, plus oversized/clamped radii, monotonicity and determinism.
- Independent uniform sampling/intersection oracle for curved sides, and a separate
  endpoint-tangent/circle-distance oracle for true curved circular fillets. Degenerate,
  hostile numeric/index, compound-path and remapping cases are included.
- Kittest tests really press/move/release, click the kind/All controls and type a numeric
  radius. Full-app GPU test source uses actual egui pointer capture, normal app canvas
  paint callbacks, held preview/release/cancel/undo and CPU comparison.

## Verification

| Gate | Result |
|---|---|
| `cargo +1.98.1 check --offline -p photocraft-engine` | passed |
| focused package engine tests | 11 passed, 0 failed, 0 ignored; 862 other unit tests filtered |
| incidental legacy `corners_` coverage matches | 2 passed; not counted as new corner acceptance |
| strict RTX 5090 compositor parity, held/release | passed; Round max PM diff 0.00032365322; Inverted 0.0001989603; each <1/255 |
| kittest controls/gestures | final focused rerun: 7 passed, 0 failed, 0 ignored; 996 filtered; 0.58s; actual pointer/menu/type events |
| full-app GPU live preview/release/cancel/undo | strengthened release gate: 1 passed, 0 failed, 0 ignored; 64 filtered. All 12,544 interior pixels per comparison, including corner boundaries; no widget exclusions. Round held/release max PM diff 0.00256598, undo/cancel 0.0025289655; Inverted held/release 0.0021761656, undo/cancel 0.0015490055; all <1/255. |
| final full tests of changed crates, once each | engine 1491 passed / 16 ignored; vector 81 passed / 1 ignored; UI 1071 passed / 10 ignored / 1 existing mock-Comfy completion failure with the live data configuration; the unchanged test passed (1/1) with isolated LOCAL_IMAGE_DATA_DIR in 2.50s. 0 doctests in each crate. No full-suite repeat. |
| clippy all-targets `-D warnings` / fmt | passed for engine/vector/UI with registered modules; final UI lint refresh passed in 17.39s after coordinator resume. Existing craft-fonts build-script warning remains separate. |
| release real-app pointer-frame benchmark | passed: 1 passed, 0 failed, 0 ignored; 64 filtered; 40 real pointer frames on the RTX 5090. Measurements below. |

The one full run contained **2643 passed, 27 ignored and 1 failed**. Its breakdown is:

| Crate / test binary | Passed | Ignored | Failed |
|---|---:|---:|---:|
| engine unit / coverage / integration | 862 / 582 / 47 | 11 / 0 / 5 | 0 |
| UI unit / GPU / integration | 997 / 60 / 14 | 5 / 5 / 0 | 1 / 0 / 0 |
| vector unit / coverage | 39 / 42 | 1 / 0 | 0 |

The unchanged failing test was
`generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened`:
its mock completion assertion timed out with the live data configuration, including a focused
retry. The same focused test passed unchanged with
`LOCAL_IMAGE_DATA_DIR=$PWD/target/corners-test-data` (1 passed, 1002 filtered, 2.50s).
No Generate code or owner's configuration was edited. The complete suite was not repeated.
Logs are under ignored `target/corners-final-tests.log`, `corners-mock-comfy-retry.log` and
`corners-mock-comfy-isolated.log`. Final UI logs: `target/corners-ui-final-focused.log`
and `target/corners-ui-final-clippy.log`.

Ignored coverage is separate: existing benchmarks, slow registry fuzz/child entry points,
manual Photoshop/contact-sheet checks, an opt-in existing tool-icon GPU test, and a Camera Raw
PSD test requiring a user-supplied fixture. That external PSD and manual QA are not counted
as passed. Package 06 uses generated fixtures plus the committed v1 document; it requires no
proprietary corner corpus. The corner release benchmark is ignored in the ordinary suite and
run explicitly below; the corner compositor and full-app parity tests are not ignored.

Hardware assertion: NVIDIA GeForce RTX 5090 / Vulkan / DiscreteGpu, NVIDIA driver 615.71.09,
PCI 0000:01:00.0; compositor requires Rgba32Float. The app gate additionally requires float
canvas support. Software adapters and no-adapter skips fail these gates. CPU: AMD Ryzen 7 9800X3D, 8 cores / 16 threads. No speed claim is
made from debug timings or from the compositor-only parity test.

Release measurements use the actual offscreen `PhotocraftApp`, 1100x780 harness, 128x128
RGB/F32 white-background document, generated five-point star, 1x aligned view, all corners,
40 egui pointer frames and normal GPU canvas callbacks/readback. Stock release profile:
thin LTO, one codegen unit, no debug assertions; offline Rust 1.98.1, three build jobs.
The shared machine was also serving other build jobs; this is a small-scene diagnostic,
not a large-document or guaranteed frame-rate claim.

| Measured operation | Median ms | p95 ms | p99 ms |
|---|---:|---:|---:|
| pointer event + app UI step | 20.466284 | 32.316500 | 35.030202 |
| GPU canvas render + harness readback | 6.970347 | 18.237428 | 20.630712 |

Press plus render: 13.356759ms. Release/one commit plus render: 36.073670ms. One final GPU
upload. The benchmark asserts a held gesture and empty journal on every pointer frame,
and exactly one journal/history entry on release. For 40 samples the sorted indices are 20 / 37 / 39 (upper median / nearest-rank p95 / p99).
The phase percentiles are separate; they are not added to manufacture combined-frame percentiles. The ordinary suite's ignore
count still includes this opt-in benchmark; its explicit release run is the separate pass
above.

Reproduction after 00 applies the registration fragment in the integration branch:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" \
LOCAL_IMAGE_DATA_DIR="$PWD/target/corners-test-data" cargo +1.98.1 test --release --offline \
  -p photocraft-ui-egui --test gpu \
  corners_full_app_gpu_pointer_frame_measurements -- --ignored --nocapture
```

This worktree's runs additionally used
`--manifest-path target/corners-integration/Cargo.toml` to test the exact fragments in the
temporary integration copy. Use `corners_full_app_gpu_preview_release_cancel_and_undo`
without `--ignored` for the strict pixel gate. Raw logs: `target/corners-release-benchmark.log` and
`target/corners-app-gpu-all-pixels.log`. The test runner emitted a `/proc` exit-race arithmetic
warning after the benchmark; the Rust gate itself passed and cargo exited zero.

## Integration / remaining providers

[Exact hooks, command signatures, remapping and attribution](vector-tasks/integration/06.md),
[i18n fragment](vector-tasks/i18n/06.tsv), and
[fixture/source records](../../crates/pc-engine/src/vector_packages/corners/SOURCES.md).

00 owns module/catalog/aggregate GPU registration; 01 owns Tool::Corner/persona/toolbar routing.
A temporary copy under `target/corners-integration` applies the supplied shared fragments for
actual full-app offscreen testing while leaving those shared branch files untouched. Applying
those fragments in the integration branch remains 00/01's work; an isolated widget is not
being called a finished app feature. The compiled engine commands work now through their
existing owned slot. The pure evaluator is compiled there by path until 00 registers its
pc-vector module, then the documented re-export uses that module directly.

V1's pc-pathops registration is absent. No duplicate adapter/nearest/boolean implementation
or new dependency was created. Native widget centres require no curve-nearest query; 03
uses V1 nearest plus `Projection.sources` after integration. 08/09 compose corners into
appearance/scaled projection, and lower-level exchange adapters call the pure pc-vector
projection rather than engine/UI. Appearance-bearing corner edits fail with the named
provider requirement until the appearance renderer lands. No missing proprietary/external
fixture, manual coordinator QA, or unmerged provider is counted as a successful gate.

## Sources / attribution

Read all required architecture/persona/owner-decision/quality/package-index documents and
inspected the pinned VectorCraft sources before design. The checkout is exactly
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` (MIT OR Apache-2.0). Retained copyright notices;
existing VectorCraft license/NOTICE files already contain the upstream notices. Integration
notes supply one PORTS row for each adapted geometry/tool source and tell 00 to regenerate
attribution. No brand assets or external corpus was copied.

Followed [VectorCraft geom/corners.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/geom/src/corners.rs)
and [tools/corners.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/corners.rs),
[Affinity's Corner Tool documentation](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_corner.html),
existing native cubic/renderer/transaction APIs and the existing real-app `move_canvas` harness.
The upstream source excludes curved adjacency despite the older quality-plan description;
our bounded offset-root and static safety-cap extension is explicitly original work. Official
cached egui/egui_kittest 0.36.2 and wgpu 30.0.1 sources establish the harness/device APIs.
