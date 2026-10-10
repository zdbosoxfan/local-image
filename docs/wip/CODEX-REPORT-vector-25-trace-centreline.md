# Package 25 trace centreline report

Status: package-owned algorithm and engine bridge implemented and validated;
**integrated acceptance is not complete**. Package 00 shared T1 registration,
11 variable-width projection, 28 actual dialog/output and the licensed external
corpus are not integrated in this branch. Do not advertise this package as done
or enable the mode by merging the T1 route alone. No commit was made.

Continued base `4e05417d`, preserving the existing T1 checked image/parameters,
fitter, metrics, command/job infrastructure and compiled engine slot. Initial
working tree contained only the untracked task brief; the owned centreline
algorithm files did not exist and the engine slot returned no commands.

## Delivered against the task

- Ported VectorCraft's topology-preserving Zhang–Suen thinning, staircase cleanup,
  junction grouping and deterministic graph extraction onto existing Path types.
  T/cross meeting pixels are retained; legitimate short arms are not pruned.
  Components, endpoints, forks, rings, skinny and 1px inputs are covered. Rings
  are open subpaths with identical first/last endpoints. Isolated pixels are
  coincident two-knot, round-capped point strokes supported by pc-vector.
- Replaced upstream chamfer/area-over-length widths with an exact separable
  Euclidean distance transform and nearest-background-site field, including
  exterior background. Original analytic normal refinement resolves half-pixel
  plateaus and diagonal lattice bias. Width is derived from nearest pixel sites;
  it does not claim exact reconstruction of continuous antialiased boundaries.
  Independent exhaustive site enumeration verifies distances and witnesses;
  analytic strips verify widths 1–10 in both axes, including even-width centres.
- Stroked open Path/ShapeStroke/optional WidthProfile results use the existing
  document model. Each graph run owns its profile; constant profiles are omitted.
  Width changes survive. The shared T1 kurbo fitter is reused via a supplied
  angle-threshold extension, with bounded 256-edge calls and exact restoration
  of shared endpoints. No duplicate T1 fitter, metrics or pc-pathops dependency.
- Cancellation checks all long passes, component walks, thinning, graph extraction,
  analytic ridge samples and fits. Input dimensions/buffer are checked before
  allocation. Work bound 128 million charged visits; 8192 strokes; 1,048,576
  total knots and width markers; 4096 profile points per run. Budget failures
  return an error rather than partial success. Worker-count tests compare bytes
  for 1/2/4 concurrent workers; 256 random small inputs check finite, open and
  deterministic results. No unsafe, shipped unwrap/expect/panic, new dependencies
  or C/C++ were added.
- Supplied exact package-00 Mode/Params/Trace/module/test registration and shared
  fitter fragments in integration/25.md and executable register.py. Old minimal
  Params and old Trace JSON load; empty stroke fields preserve fill serialization.
  LineArt/Sketch route explicitly to centreline; Sketch uses stronger fitting.
  Incompatible preset/mode pairs reject. Fill algorithms and defaults remain
  unchanged. These shared changes are tested in a disposable registration copy,
  not competing edits to shared working-tree files.
- Implemented engine capture/check/prepare/preview/publish bridge for 28. Captures
  doc ID/revision, source, active and complete selected IDs, origin and scale;
  checks missing targets, duplicates, inherited locks, staleness, numeric bounds
  and insertion depth. Validates the complete output before caches; charges
  bounds-clipped storage against 512 MiB before rendering. Preview is detached,
  with no revision/history/journal; publication inserts identical prepared layers
  in one Session::edit. Shapes have no fill, legacy stroke mirrors and authoritative
  StrokeItem/profile metadata. Position and absolute widths scale together.
  Native round-trip and byte-identical undo/redo are tested. Oversized per-run
  and aggregate width profiles reject before cloning/projection/cache work. Revalidation catches
  an invalid final target and a locked ancestor on release.
- Uniform output uses the actual appearance renderer. Varying output requires
  package 11's StrokeProjection and explicitly errors with NoProjection; no
  silent loss of widths. Package 28 remains sole owner of command journaling.
  Notes include optional-mode admission in T1's default-object parameter guard
  and the necessary stroke-result branch before its old filled-layer closure.
- Added strict offscreen RTX result-stroke compositor acceptance and a supplied
  actual PhotocraftApp dialog/canvas acceptance contract for 00/28. The latter
  requires the unimplemented public trace_dialog preview hook and actual menu /
  Centreline control / Trace publication. It has not been compiled or run and
  is not counted as a passing UI/GPU gate. It uses a real 5px scan requiring 11.
- Added a six-input normal synthetic benchmark using existing fidelity/MS-SSIM
  metrics and an independent blank-canvas control. Supplied a registered ignored
  full release benchmark with 18 synthetic inputs at 256/512/1024, all licensed
  coordinator scans when present, and explicit incomplete/missing-input records.
  Input-only scans are not labelled as vector ground truth. Missing files,
  fewer than five scans, missing width projection and unmeasured B.4 reference
  comparisons never count as passing gates.

## Verification

All Cargo commands use Rust 1.98.1, --offline, CARGO_BUILD_JOBS=3, one Cargo build
at a time. Working-tree ownership is preserved through the disposable copy
`target/centerline25-registration/worktree`; its owned sources/tests are symlinks
back here. `CARGO_TARGET_DIR` points to this worktree's target, or target/clippy
for lint. No whole-workspace suite was run. Only changed-code tests were run
during development; the final unfiltered changed-crate suites run once each.

Focused verification: pc-trace 12 passed, one ignored full scan benchmark;
engine seven passed (five in the first focused run, plus target/ancestor
preflight and aggregate profile admission cases). The profile test supplies a
projection probe and verifies zero calls and no document/history mutation. Logs:
centerline25-focused.log, centerline25-engine-focused.log,
centerline25-target-validation.log, centerline25-profile-admission.log under target. This includes the 256-case
property test, restored singleton regression seed, numerical width/distance
oracles and the adapted upstream diagonal/circle cases. The T1 fitter produces
five anchors for the diagonal at detail 0.8, vs VC's <=4 line-snapping result;
our explicit adaptation permits <=8 and independently samples every resulting
cubic against the analytic axis at <=1.5px. This is not VC node-count parity.

Final pc-trace suite: **27 passed, 2 failed, 4 ignored** (12 unit passed;
15 integration passed, 2 failed, 4 ignored). All new centreline tests passed.
The failures are existing fill self-intersections in
`benchmark::fast_six_image_benchmark` (logo-02-256-blur1) and
`engine::binary_outputs_are_finite_closed_and_simple` (3×6 minimal mask).
Both were rerun individually with the four private T1 root/fitter/test files
restored byte-for-byte from this branch's baseline. Both failed identically;
the binary case reports the same intersection parameters
(0.9703875060776606, 0.038633406839233844). The registration files were then
restored. These failures were neither weakened nor fixed in unowned T1 files.
Doc tests were not reached after the integration test executable failed.
Logs: `target/centerline25-trace-full.log`,
`target/centerline25-baseline-fill-benchmark.log`,
`target/centerline25-baseline-fill-property.log`.

Final engine suite: **1,660 passed, 0 failed, 19 ignored**: unit 1019/13
ignored, coverage 582/0, integration 47/5 ignored, vector integration 12/1
ignored; zero doc tests. All seven centreline engine tests passed, including
strict real-GPU parity. Log: `target/centerline25-engine-full.log`.

Both changed crates passed all-targets clippy with `-D warnings` and formatting
checks. Logs: `target/centerline25-trace-clippy.log`,
`target/centerline25-engine-clippy.log`. Exact verification commands are in
`docs/wip/vector-tasks/integration/25.md`. `git diff --check` passed; original
T1 roots/fitter, engine command route, manifests/lock and PDF shared files have
no diff. No unfiltered suite was repeated and no whole-workspace test was run.

Hardware GPU evidence: NVIDIA GeForce RTX 5090, discrete, Vulkan,
PCI 0000:01:00.0, vendor 4318/device 11141, NVIDIA driver 615.71.09.
Rgba32Float accumulation is required, as are finite premultiplied pixels.
`centerline_rtx5090_result_strokes_detached_preview_release_and_undo_parity`
passed without a skipped adapter. Preview/commit max absolute difference
0.00023496151, below 1/255; undo difference 0. This is production compositor
acceptance for the actual uniform traced strokes, not an interactive app speed
measurement or variable-width/dialog acceptance. No desktop window was opened;
no running app was killed. No frame-rate/performance claim is made.

## Existing integration issue found

The base's package-22 PDF engine references do not compile: pc-pdf root still
only exports export, and its production manifest omits the import dependencies.
The first focused engine build failed with seven missing PageBox/import_with/
PdfOptions errors before tests. The retained log is
`target/centerline25-baseline-pdf-registration-failure.log`.

The disposable copy applies the exact already supplied integration/22.md root
exports and approved/locked hayro-syntax/hayro-interpret/flate2/image production
registration. PDF implementation and original roots/manifests are untouched.
This restored engine verification; it is coordinator wiring, not a new centreline
feature/dependency. The real branch still needs that shared registration.

The pre-port xtask assets audit failed on 861 pre-existing unattributed SVG /
fixture assets from other packages. VectorCraft's existing curated attribution
and MIT/Apache/NOTICE files were inspected and retained. Log:
`target/centerline25-attributions-before.log`. No competing catalog fix was made.

## Sources, attribution and remaining acceptance

Read the required architecture, persona spec including Owner decisions, quality
plan and work-package README, the T1 report, existing tests and the pinned source.
VectorCraft read-only pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092: trace/centerline.rs
and relevant trace/tests.rs; T1 vtracer binary admission pin
928ed0a6f654408e28fb741b6133d4c456bd0160; existing kurbo 0.13.1 simplify API.
Exact EDT follows [Felzenszwalb–Huttenlocher](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf),
§2, with original Rust and an independent brute-force oracle. Nearest-site
normal refinement, bounded/cancellable adaptation and engine preparation are
original Local Image work. No brand material. Fixture licences, hashes, tool
versions and parameters are in crates/pc-trace/tests/centerline/SOURCES.md;
PORTS rows and i18n row for 00 are in integration/25.md and i18n/25.tsv.

Remaining mandatory integration:
1. The T1 owner resolves the two independently reproduced pre-existing fill
   self-intersection failures; the changed-crate all-pass requirement is not met.
2. 00 applies the T1 root/Mode/Trace/fitter/test fragments and the existing PDF
   wiring; regenerates central attribution/catalog records.
3. 11 supplies actual varying-width outline projection shared by renderer,
   exports and full benchmark. Most thick/junction/sketch results have profiles;
   these cannot currently publish through NoProjection.
4. 28 consumes stroked results in the existing cancellable jobs/command route,
   admits explicit mode overrides, implements/wires the actual dialog/canvas
   preview hook, and registers/runs the supplied full-app click/Trace/Undo GPU
   gate. Menu/UI/interaction/variable-width GPU gates remain unestablished.
5. The coordinator supplies licensed external scans and centreline reference
   sets, then runs the full release corpus and B.4 gates. There is no inputs.json
   or supplied scan/reference set in this base. The full scan benchmark remains
   ignored/not executed; no missing corpus or competitor quality is claimed as
   success. Manual merge QA remains coordinator-owned.

These dependencies cannot be implemented here without violating the task's
explicit shared ownership. All available owned work is delivered uncommitted;
this report deliberately leaves integrated acceptance incomplete.
