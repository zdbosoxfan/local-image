# Package 26: trace logo cleanup

Status: owned implementation and integration handoff supplied; integrated acceptance remains incomplete pending packages 00/28, inherited T1 failures and the external corpus/reference sets. Changes are uncommitted. The inherited T1 implementation was retained; no T1 fitting, shared Params, centerline, cutout, manifest or lockfile was changed.

## Implemented

- Optional bounded bilateral prefilter, conservative low-contrast enclosed-defect filling, Auto/2×/4× preprocessing and source-coordinate output. Transparent and contrasting text counters are preserved; Pixel fitter retains the original lattice. Binary presets retain their threshold behavior when AA palette is selected.
- AA-aware palette estimation from low-gradient opaque cores, stable Oklab assignment, and fractional color-mixture contours. Fractional contours use the locked kurbo simplifier locally; T1 Potrace's unit-lattice assumptions and fitting modules remain unchanged. Default settings take the legacy path and deserialize with all flags off.
- Axes, collinear reduction/straight-edge fitting, right-angle snapping, circle/rotated ellipse/arc recognition and mirror symmetry. Geometry proposals have bounded bidirectional curve residual, winding and independent topology guards. Mirror proposals require source-mask reflection IoU strictly greater than .98; handles and disconnected components are paired as well as anchors.
- Finite/coordinate/pixel/boundary/sample/comparison limits and cancellation checks; output and counters are deterministic across tracing threads. No new dependencies or unsafe Rust.
- Engine evaluator and private captured/prepared state: document ID/revision and all target IDs are checked; previews and bounds-clipped caches remain detached; publication rechecks the complete target set and uses one Session::edit. Tests cover stale/cancel/invalid/locked second targets, unchanged history/journal and one undo/redo.
- Exact settings, worker and apply-call fragments, stats semantics, package-28 UI/full-app acceptance instructions and attribution row in [integration/26.md](vector-tasks/integration/26.md). English label handoff in [i18n/26.tsv](vector-tasks/i18n/26.tsv). Module registration is append-only, using the task's registration override.

## Quality measurements

The final six 256-pixel clean T1 corpus fixtures use identical Params::Logo/input for baseline and cleanup. Independent pc-testkit fidelity and intersection oracles check the rendered result. The 78 measurements (six fixtures × thirteen requests) are written to `target/cleanup-26/toggles.json`. Combined Logo reduces aggregate nodes **160 → 129 (19.375%)**, exceeding 15%; every measured fixture improves fidelity, with worst fidelity loss **−0.014429**, below .002. This is the measured subset, not a claim about the missing complete corpus.

| Combined Logo fixture | Baseline nodes | Cleaned nodes | Fidelity loss |
|---|---:|---:|---:|
| Circle | 14 | 8 | −.073837 |
| Rounded rectangle | 17 | 14 | −.062024 |
| Pentagon | 19 | 37 | −.043178 |
| Arrow | 19 | 11 | −.014429 |
| Donut | 25 | 12 | −.082700 |
| Outline wordmark | 66 | 47 | −.056886 |

The pentagon uses more nodes while improving fidelity; the acceptance reduction is aggregate. The individual toggles are measured separately and are not all quality-neutral: prefilter's worst loss is .002841, standalone 2× is .010054 and 4× is .020572. Their combined Logo behavior is measured independently. Geometry-only toggles can conservatively reject the coarse inherited T1 paths; dedicated analytic/Hausdorff fixtures exercise their successful behavior.

| Request | Aggregate nodes | Reduction vs 160 | Worst fidelity loss |
|---|---:|---:|---:|
| Axes | 160 | 0% | .000000 |
| Collinear | 131 | 18.125% | .000000 |
| Right angles | 160 | 0% | .000000 |
| Circles | 160 | 0% | .000000 |
| Ellipses | 160 | 0% | .000000 |
| Arcs | 160 | 0% | .000000 |
| Symmetry | 160 | 0% | .000000 |
| AA palette | 987 | −516.875% | −.038860 |
| Prefilter | 158 | 1.25% | .002841 |
| Small holes | 160 | 0% | .000000 |
| Upscale 2× | 1047 | −554.375% | .010054 |
| Upscale 4× | 1801 | −1025.625% | .020572 |
| Combined Logo | 129 | 19.375% | −.014429 |

## Validation

All cargo commands use Rust 1.98.1, offline mode and CARGO_BUILD_JOBS=3, one build at a time. Focused changed-code tests/checks were used during development. Final full suites are restricted to the two touched crates; no workspace suite was run. The first final pc-trace invocation caught redundant fractional anchors on exact PixelArt upscale (render parity already passed). The wrapper now removes only collinear working-lattice anchors before rescaling; the focused regression passed. Final lint and suite were rerun after this correction, as required for a failing gate. The shared test-runner emitted a harmless process-exit RSS arithmetic message during the failed invocation; it was not an adapter skip or test success.

- Strict all-target clippy, `-p pc-trace -p photocraft-engine -- -D warnings`: passed. Log: `target/cleanup-26-clippy-final.log`. Existing craft-fonts build-script warning remains unrelated.
- Final pc-trace suite: **failed in inherited T1 tests**. Unit tests: 18 passed, 0 failed, 1 ignored (all 16 new nonignored cleanup tests passed). Integration tests: 12 passed, 3 failed, 3 ignored. Aggregate: 30 passed, 3 failed, 4 ignored; doctests were not reached after the integration failure. Log: `target/cleanup-26-trace-final.log`.
  - `benchmark::fast_six_image_benchmark`: T1 `logo-02-256-blur1` self-intersection.
  - `engine::binary_outputs_are_finite_closed_and_simple`: T1 binary generated mask self-intersection, shrunk to 2×12.
  - `engine::deterministic_color_and_thread_counts`: the inherited test's self-intersection assertion fails on a generated 5×4 colored mask (serialization equality was not the failing assertion).
  These tests call unchanged `pc_trace::trace`, not cleanup. The only shared pc-trace source edit is append-only module/test-alias registration. No original tests were weakened or ignored. Proptest-generated reproducer seeds are retained in `crates/pc-trace/tests/proptest-regressions/engine.txt` for 00/T1; this automatically generated source record is the additional test artifact outside the cleanup directory. Existing `external_corpus` prints a missing-corpus skip while returning success; that result is not counted as external acceptance.
- Final photocraft-engine suite: the default parallel invocation exited abnormally with **SIGSEGV / runner exit 139** after 967 logged passes and 13 ignored tests; no failed Rust assertion or completed suite summary was emitted. Log: `target/cleanup-26-engine-final.log`. This is not a pass and its cause is not established. The two new cleanup transaction tests were then run serially and both passed (0 failures), log `target/cleanup-26-engine-focused.log`. The full serial retry **passed**: 1,655 passed, 0 failed, 19 ignored, 0 doctests. Breakdown: unit 1,014 passed/13 ignored; coverage 582 passed; integration 47 passed/5 ignored; vector_v1 12 passed/1 ignored. Both new cleanup transaction tests passed in that full run. Command: `cargo +1.98.1 test --offline -p photocraft-engine -- --test-threads=1`, log `target/cleanup-26-engine-serial-final.log`. This additional run is justified by the incomplete crashed gate, not routine repeated testing. Existing ignored checks comprise timing/benchmarks, two package-00 V1-adapter tests, registry panic fuzz/child entry point, a missing user Camera Raw PSD, and a manual Photoshop check; these remain separate from successful behavior tests. Serial success does not establish the cause of the parallel crash or validate concurrent GPU execution.
- Explicit complete competitor gate: **failed**, 0 passed/1 failed, with `missing corpus: supplied 0 of required 15 external SVGs + 5 licensed scans`. This is recorded as a missing acceptance dependency, not a successful gate. Log: `target/cleanup-26-competitor-final.log`.
- Formatting (`cargo fmt -- --check`) and `git diff --check`: passed.

New behavior tests use analytic area/winding, independent Hausdorff/intersection/fidelity metrics, exact disabled-output comparison, 256 generated masks, finite/budget/cancellation checks and 1/2/4-worker output comparison. Synthetic fixture sources and licenses are recorded in [SOURCES.md](../../crates/pc-trace/tests/cleanup/SOURCES.md).

New mandatory real GPU test: `cleanup::tests::gpu::cleanup_cleaned_shapes_rtx5090_vulkan_cpu_parity`. It requires NVIDIA RTX 5090, Vulkan, DiscreteGpu and Rgba32Float; it renders cleaned circle/donut/wordmark shape groups and checks CPU vs actual GPU premultiplied max difference ≤1/255. No adapter fallback or skip is accepted. Final adapter: NVIDIA GeForce RTX 5090, vendor 0x10de, device 11141, PCI 0000:01:00.0, NVIDIA driver 615.71.09, Vulkan/DiscreteGpu/Rgba32Float. All three cleaned shapes have measured premultiplied max difference 0.

## Remaining integrated acceptance

1. Package 00 must merge the additive settings/pipeline field and the supplied PORTS fragment; package 28 must consume controls, statistics and captured/detached publication helpers. The actual trace dialog and its request/apply wiring are absent in this branch. Shared UI/settings/registry ownership prevents inventing a competing dialog or command. The handoff specifies actual click/type/drag, preview cancellation/staleness/history and release tests through the app's offscreen harness.
2. Full-app RTX live-preview/release parity and release pointer-frame performance measurements require that integration. The package-level real GPU test does not substitute for them. No interactive performance claim is made and no desktop window was opened.
3. The inherited T1 self-intersection failures above require its owner to repair the fitting/topology code; this package is explicitly forbidden to edit T1 fitting modules. The engine's default parallel full run also crashed; the complete serial retry passed, but the crash remains an integration diagnostic to investigate.
4. The external Logo corpus has no supplied 15 CC0/PD SVGs or five licensed scans. Competitor references are partial (54 vtracer-065 and 55 alpha4 Logo SVGs, versus 540 synthetic cases per tool plus external cases); no complete Potrace/other-preset suite is supplied. The added full gate explicitly errors on missing data and requires release mode once data exist. Missing assets and existing T1 tests' limited/skipped comparisons are not successful full competitor acceptance.

This package must not be marked fully accepted until these integrated gates pass. No owner files outside the worktree were changed and no commits were made.

## Sources followed

Read the task, vector architecture, persona/Owner decisions, quality plan and vector-task integration contract. Followed the pinned local VectorCraft recognition source at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` with upstream notices retained; adapted-file PORTS row is supplied to 00. Reused T1's pinned vtracer Oklab implementation rather than copying it. Read locked kurbo 0.13.1 arc/simplify/path/extrema APIs. The original bounded bilateral kernel follows [Tomasi–Manduchi (1998)](https://doi.org/10.1109/ICCV.1998.710815). Original marching squares follows the [official scikit-image contour semantics](https://scikit-image.org/docs/0.26.x/api/skimage.measure.html#skimage.measure.find_contours). Residual/topology/symmetry, conservative defect policy, edge-aware resampling and transaction code are original Rust because no existing shared implementation satisfied these constraints. Full fixture/tool/license details are in the source record linked above.
