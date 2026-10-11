# Centreline package 25 source / fixture records

The read-only VectorCraft checkout is
`~/.local/share/local-image-dev/engine-sources/vectorcraft`, verified at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`.

- `crates/trace/src/centerline.rs`: SHA-256
  `1ba897b777e058e019cd368c2f858593eae09e4a05ab9e6ff4da833c2d5c1870`.
- `crates/trace/src/tests.rs`: SHA-256
  `5de3ba88202d46d0f7735fa51bbf0849c36096c212081b193a85f3d3b2d79693`.
- Both: Copyright (c) 2026 ArtCraft Team and VectorCraft contributors,
  MIT OR Apache-2.0; existing `licenses/vectorcraft-*` retained. No brand assets.
- T1 `frontend/binary.rs`, pinned vtracer
  `928ed0a6f654408e28fb741b6133d4c456bd0160`, SHA-256
  `18cce3369cbf39b7767b6c2fd555c2335573c4f64afde5fc9765a1d759df5d52`.
  Its Bradley–Roth rule is retained in cancellable mask admission.
- Exact Euclidean transform: original Rust implementation of
  [Felzenszwalb–Huttenlocher, Distance Transforms of Sampled Functions](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf),
  Theory of Computing 8 (2012), §§2.1–2.2. The independent test exhaustively
  enumerates background sites; it does not reuse the lower-envelope algorithm.

Original analytic strips, T/cross graphs, diamond rings, varying-width strips,
transparent/single-pixel fixtures and synthetic benchmark inputs are generated
in Rust, dedicated to CC0-1.0. Code remains GPL-3.0-or-later. The diagonal capsule
and circular-ring behavioural cases are adapted from the pinned VC tests above,
with our open-loop convention and existing T1 fitter. No binary artwork or
external/proprietary scan is fabricated.

Parameters: algorithm tests use LineArt, fixed threshold 128, speckle 0, detail
0.2; the upstream capsule/ring use detail 0.8. Routing defaults are adaptive,
speckle 0; Sketch detail 0.8. Synthetic benchmarks use fixed threshold 128;
external scans use the actual adaptive preset, with per-case Params recorded.
Rust 1.98.1, pc-vector/testkit/kurbo/resvg/wgpu at workspace lock pins.

`benchmark.rs` reads the existing coordinator `tests/corpus/inputs.json` local
filename/licence/URL schema; non-SVG inputs are input-only scan comparisons.
Absent files, fewer than five scans, unsupported width projection and unmeasured
competitor gates are listed in `target/trace-bench/centerline25-scores.json`,
which has `complete: false` in those circumstances. A test process success is
not a passed external quality gate. Current baseline supplies no such manifest,
licensed scans or centreline competitor sets.

`register.py` creates only a disposable git-archive registration copy beneath
this worktree's target directory, then symlinks owned implementation/tests. It
applies package 00's T1 mode/result/module/test fragments plus the small shared
fitter-angle interface extension. The already documented package-22 PDF root/
production-dependency registration is also applied in that copy because the
baseline engine fails to compile without it. No centreline dependency changes
or shared working-tree edits. All Cargo verification uses --offline and three
build jobs; the report records precise commands and outcomes.

The diagonal-case T1 fitter produces five anchors at detail 0.8, whereas VC's
line-snapping test expects at most four. Our adapted test bounds samples of the
actual cubics against the analytic capsule axis (1.5 px maximum deviation),
retains the endpoint/width checks, and permits up to eight anchors. This
explicit fitter difference is not represented as VC node-count parity.
