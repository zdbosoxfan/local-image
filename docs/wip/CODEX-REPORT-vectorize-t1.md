# Vectorizer T1 report

## Scope and implementation

Continued this branch's existing document, vector renderer, engine command and job infrastructure.
At task start, there were no pending trace implementation changes to recover; the supplied
upstream source copies in `target/vendor-src/` were used read-only. No commit was made.

- Added `pc-trace`, with checked RGBA8 input and serializable parameters. Colour clustering
  ports the pinned visioncortex builder/runner/container/cluster algorithms; Photo ports
  vtracer's volume-extinction watershed hierarchy. Binary tracing includes fixed and
  Bradley–Roth adaptive thresholds. Palette snapping, weighted median quantization with
  Oklab assignment, maximum colours, speckle removal, alpha keying and ignore-colour are present.
- Stacked output folds regions into one compound nonzero path per colour in paint order.
  Region masks are cropped. Directed lattice boundaries preserve counters and distinguish
  components meeting diagonally. A flattened-segment crossing guard falls back to exact
  collinear lattice boundaries when fitted curves cross.
- `potrace.rs` is a Rust translation of Potrace 1.16 `trace.c`: prefix sums, longest lines,
  penalty/optimal-polygon dynamic programming, pointslope/quadratic vertex adjustment,
  alphamax smoothing and area/tangency constrained opticurve optimization. It is not a
  substitute generic curve fitter. Three independent upstream C numeric fixtures check
  polygon indices and final controls with a 1e-8 tolerance.
- The alternate fitter ports visioncortex staircase simplification, polygon reduction,
  corner detection, four-point subdivision and tangent construction. Both fitters use
  kurbo's optimized Bézier refit through `simplify_bezpath`/`fit_to_bezpath_opt`.
  The local adapter emits existing `photocraft_doc::Path` knots, including quadratic
  conversion and a split for a closed single-cubic loop. No `pc-pathops` dependency.
- Presets cover Logo, B&W, Few Colors, Silhouette, Photo and Pixel Art. Pixel Art uses exact
  pixel boundaries without smoothing. Line Art and Sketch enum values are reserved and
  explicitly rejected until T2 implements centreline strokes. Default fitters are provisional
  pending the coordinator's comparison, as recorded in corpus `SOURCES.md`.
- `layer.vectorize` snapshots pixels from a raster layer or smart object, applies the active
  selection, traces in the existing cancellable background-job framework, and creates
  `Vectorized – <name>` with hex-named shape children in one undo step. An explicit layer
  target works even if the active layer is a group. `layer.vectorize.preview` returns paths
  in scaled preview pixels plus scale and crop bounds without editing or locking the document, so edits and newer previews remain available.
  Smart objects use their rendered source or cached pixels. RGB and other document colour
  modes pass through the existing colour conversion. The commands are registered for
  automation; T2 owns menu/dialog wiring.
- Pixels are validated before allocation (16,777,216 pixel limit; 16000² returns an error).
  A cumulative eight-million-edge limit is enforced while building boundary maps.
  Cancellation is checked during segmentation, edge extraction and between fits.
  The crate forbids unsafe code and denies unwrap/expect/panic in shipped code.

## Metrics, corpus and gates

`photocraft-testkit::vector` supplies an independent five-scale Gaussian-window Wang MS-SSIM,
the vtracer-bench fidelity composite (RGB error, MS-SSIM and coherent-error patch opening),
mean/p95 ΔE_ok (100 × Oklab distance), mask IoU, sampled boundary Hausdorff using kurbo
nearest points, node counts, finite-path checks, and an independent flattened intersection
oracle. No dssim-core code or dependency was used.

The test-time generator creates 30 original logos at 256/512/1024 pixels with circles,
rounded rectangles, stars, arrows, counters, rotations and overlaps. The OBO wordmark uses
committed OFL Inter outlines. Every case has clean AA, JPEG q60, Gaussian sigma 1,
deterministic normal noise sigma 4/255, half-resolution down/up sampling and a scan variant
(0.7° rotation, paper texture, uneven lighting). Each is scored against the clean render
from `pc-vector`. Reference node counting counts repeated closing anchors once, matching
the document knot convention. Source/licence records live in `tests/corpus/SOURCES.md`; the on-disk corpus
and numerical fixtures are well below 5 MB.

Normal tests run six images across all six fill presets and enforce finite, closed/simple
output and structural MS-SSIM above an independently scored blank-canvas control. The exact competitor gates are separate:
median fidelity within 0.005 of the strongest baseline, 80% per-image wins for Logo/B&W
and 70% otherwise, and median nodes within 1.2× the closest-fidelity baseline. Tests exercise
the gate's rejection of fidelity and node regressions. Reference SVGs are re-rendered with
resvg and scored with the same code; stored scores are provenance, not a scoring shortcut.

The ignored full test compares both fitters across the synthetic matrix and supplied
external cases. It writes per-image scores and per-preset median fidelity/node/time summaries
to `target/trace-bench/scores.json`, preserving completed measurements before a gate failure.
The ignored release timing test enforces Logo 1024² <1.5s and Photo 2048² <8s on the
coordinator's machine. Determinism and finite/closed/simple outputs are checked by 256 cases
each of two proptests, repeated calls and three worker threads.

### Reference regeneration

No vtracer/potrace/Inkscape CLI or committed competitor reference SVGs were supplied.
Tests print an explicit skip for each unavailable tool/preset. These skips are permitted
by `CODEX-TASK.md` and do not mean the competitor gates passed.

The coordinator harness is `crates/pc-trace/tests/regenerate.rs`, ignored in normal runs.
It checks vtracer 1.0.0-alpha.4, Potrace 1.16 and Inkscape on PATH, optionally uses
`VTRACER_065` (otherwise `vtracer-0.6.5`), applies matching presets, shares fixed thresholding
for the binary subset, normalizes SVGs through Inkscape batch mode, and writes SVGs plus
tool versions, parameters, case IDs and scores. Scratch inputs/raw outputs stay in
`target/refvec/trace/`; final references go under `tests/references/<tool>/<preset>/`.

```
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 TRACE_REF_FAST=1 cargo +1.98.1 test --offline -p pc-trace --test regenerate -- --ignored --nocapture
```

Omit `TRACE_REF_FAST` for the full corpus. Commit the generated references, then run:

```
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline --release -p pc-trace --test benchmark full_quality_benchmark -- --ignored --nocapture
PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline --release -p pc-trace --test benchmark release_performance_gates -- --ignored --nocapture
```

The coordinator has also not supplied the specified 15 CC0/PD external SVGs and five licensed
pixel/line-art scans. `external_corpus` explicitly skips until `tests/corpus/inputs.json`
exists. Each entry names a local SVG/PNG, licence and source URL. SVGs use resvg ground truth;
scans use input fidelity only. The full benchmark and reference harness include supplied
external entries; B&W/Silhouette use their monochrome subset. No synthetic fixture is represented as real external artwork.

### Known quality limits

The scan smoke case exposes a substantial coherent background-colour error (Logo fidelity
about 3.3e-13 with MS-SSIM 0.983; Photo MS-SSIM 0.946); no competitor-quality success is claimed. Anti-alias fringes can also
increase node counts, and topology fallback can retain many lattice nodes. The T2 denoising,
anti-alias-aware palette, upscaling and geometric cleanup are relevant to these issues.
Hausdorff uses sampled points against exact cubic nearest distances. Per-colour geometry
uses nearest-colour compound paths and includes occluded stacked boundaries. External
geometry extraction supports solid filled SVG paths; gradients/strokes still contribute
to the authoritative resvg raster fidelity. The complete benchmark and measured fitter
decision remain coordinator checks, not results established in this sandbox.

## Attribution and dependency audit

`docs/PORTS.md` has one row per ported upstream file, with full pinned source hashes.
Visioncortex/vtracer MIT and Apache notices and Potrace GPL-2.0-or-later COPYING/notice
are in `licenses/`. Potrace's tarball SHA-256 is recorded. Original copyright headers are
retained in ported files. The optional numeric-fixture C oracle is generated/compiled only
under `target/refvec/trace-stages/` by `scripts/generate-trace-stage-fixtures.py`; it is
outside the Cargo build. Application code is Rust, with no C, build script, unsafe code
or new external registry package added by this task. Existing locked kurbo/image/resvg/
proptest/ttf-parser/wgpu dependencies are reused.

## Verification

Clippy passed for all three changed Rust crates with all targets and `-D warnings` in
`target/clippy`. Formatting and `git diff --check` passed. Attributions regenerated
successfully, and all 10 xtask tests passed, including coverage and up-to-date checks.
Regeneration exposed an existing unreferenced `vkdt-NOTICE.md`; its missing curated entry
was restored from that existing notice without changing the algorithm.

The unfiltered engine command encountered one existing sandbox failure:
`ai_cmds::tests::ai_commands_against_the_mock_server` cannot bind its local HTTP mock server
(`PermissionDenied: Operation not permitted`). That run had 808 passed, one failed and
11 ignored tests before Cargo stopped. The complete rerun with just that test explicitly
excluded passed 851 tests, with 15 ignored and one filtered. The full failure is retained
in `target/t1-engine-test-full.log`; it is not claimed as an unfiltered suite pass.

Latest available-suite results:

| Suite | Passed | Ignored | Qualification |
|---|---:|---:|---|
| `pc-trace` | 17 | 3 | Full quality, release timing and CLI regeneration are coordinator-only. |
| `photocraft-testkit` | 7 | 0 | Includes three new vector metric/oracle tests. |
| `photocraft-engine` | 852 | 15 | One existing mock-server test explicitly excluded; six vectorization integration tests passed. |
| `xtask` | 10 | 0 | Attributions are complete and up to date. |
| `photocraft-io --test vector` | 3 | 0 | Shapes, vector masks and saved paths round-trip. |

The combined engine/trace/testkit/xtask rerun passed 885 tests with 18 ignored and one
filtered. The subsequent tracing rerun passed after the reference-node correction and
boundary-budget adjustment, including the additional node regression test. The eight-million
edge budget permits four edges per pixel in the full 1024² corpus, with headroom for stacking.
Two PSD corpus tests are feature-gated and were not run because `corpus/psd` is absent;
the coordinator must run them with `--features corpus` once those reference files exist.

All builds used `PATH=~/.cargo/bin:$PATH`, `CARGO_BUILD_JOBS=3`, `+1.98.1`, `--offline`,
and one Cargo build at a time. Verification commands:

```
cargo +1.98.1 test --offline -p pc-trace -p photocraft-testkit -- --nocapture
cargo +1.98.1 test --offline -p photocraft-engine -- --skip ai_cmds::tests::ai_commands_against_the_mock_server
cargo +1.98.1 test --offline -p xtask
cargo +1.98.1 test --offline -p photocraft-io --test vector -- --nocapture
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p pc-trace -p photocraft-testkit -p photocraft-engine --all-targets -- -D warnings
cargo +1.98.1 fmt -p pc-trace -p photocraft-testkit -p photocraft-engine --check
```

Logs are retained in `target/t1-test.log`, `target/t1-final-tests.log`,
`target/t1-psd-test.log`, `target/t1-xtask-test.log`, `target/t1-clippy.log` and
`target/t1-attributions.log`.

Added GPU test: `pc-trace/tests/gpu_parity.rs::traced_shape_group_matches_cpu` compares the
traced cached shape group through CPU flatten and GPU compositing, premultiplied max error
≤1/255. It prints `skipped: no GPU adapter` if adapter probing fails. The run executed
and passed on `llvmpipe (LLVM 22.1.8, 256 bits)`, a CPU software Vulkan adapter using Mesa
26.2.3. This is shader/compositor parity evidence, not an RTX 5090 hardware result; that
coordinator check remains pending. No GPU shader or UI code changed. No GUI was opened
or running app killed.

Engine integration tests cover apply/preview, undo/redo, invalid inputs, exact rectangle
bounds and area, selection coordinates, cached smart objects, job cancellation, editing
during a background preview, explicit layer targets and version 1 save/load with
shapes, vector masks and named paths. Output uses existing shape/path persistence without
changing `FORMAT_VERSION`.

T2-owned work deliberately remains outside this task: dialog, Library handoff, centreline,
shape recognition, snapping/symmetry, cutout layering, prefilter/auto-upscale/anti-alias-aware
palette, expansion and Make Work Path. Final merge acceptance needs the coordinator's real
corpus, reference SVGs, full release quality/performance run and hardware GPU parity run.
