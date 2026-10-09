# Codex task: capture sharpening on the GPU

Worktree of "Local Image" (Develop engine: crates/lc-pipeline CPU reference, crates/lc-gpu wgpu/WGSL twin). Capture
sharpening (Richardson–Lucy deconvolution with a Gaussian PSF on luminance, per-pixel kernel table with corner boost,
variance blend mask; a faithful port of darktable `src/iop/demosaicing/capture.c` — see `crates/lc-pipeline/src/capture.rs`,
`lightcraft_pipeline::presource` / `presource_with`, `StageCache.pre`) currently runs on the CPU **for both renderers**:
`lightcraft_gpu::render` calls `presource` before uploading the source. On the owner's real 33 MP Sony raws it costs
1.4–3.5 s per full render and runs again whenever a capture value changes — the slowest stage in Develop.

## Work

1. Port capture sharpening to the GPU (WGSL compute + host in crates/lc-gpu), operating on the uploaded source: the RL
   iterations (Gaussian blur passes — reuse/extend the existing blur kernels in `src/wgsl/blur.wgsl` if their accuracy
   matches; small-σ kernels need true Gaussian taps), the per-pixel kernel table with corner boost, and the blend mask.
   Iteration count, radius (incl. `info.sensor_scale` for binned previews), threshold and corner boost exactly as the
   CPU. Cache the sharpened source on the device per (source, parameters) like `GpuStages.source`, so slider drags of
   other tools don't redo it, and a capture change re-runs it (the existing test
   `capture_sharpening_changes_re_upload_the_source` in `crates/lc-gpu/tests/toolset.rs` must keep its meaning).
2. The CPU path stays the reference and is unchanged. GPU-vs-CPU: extend `crates/lc-gpu/tests/toolset.rs`
   (`capture_sharpening_matches`, plus radius/threshold/iterations/corner-boost sweeps, binned preview, edges/borders):
   mean |Δ| < 0.5 LSB, max ≤ 3 LSB. Add an `#[ignore]` 24 MP bench row (GPU vs CPU ms).
3. Keep float accuracy: RL is iterative, so compare intermediate results against the CPU in a CPU-side unit test of
   your WGSL math where possible (e.g. run the same algorithm step on the CPU in f32 with the GPU's ordering) and
   document any ordering differences.
4. Update `docs/GPU-VALIDATION.md` (capture sharpening is no longer a CPU stage) and the darktable notice if needed.

Report file: `docs/wip/CODEX-REPORT-gpu-capture.md` — list exactly which GPU tests to run on the RTX 5090.

## Rules (all Codex jobs)

- "Local Image" is GPL-3.0-or-later and **all Rust**: workspace denies `unsafe`; no new dependencies, no C/C++ or
  `-sys` crates in the build (C only in out-of-build reference harnesses under `target/refvec/`).
- Offline builds (`--offline`). The PC has 60 GB RAM shared with other jobs: ONE cargo build at a time in your job,
  `CARGO_BUILD_JOBS=3`. Use `PATH=~/.cargo/bin:$PATH` and `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`;
  lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <crate> --all-targets -- -D warnings`.
- Run the tests of every crate you change (`cargo +1.98.1 test --offline -p <crate>`); all must pass. Your sandbox has
  **no GPU adapter**: GPU tests print "skipped: no GPU adapter" and pass without running — still write GPU code/tests
  and list them in your report; the coordinator runs them on the real RTX 5090.
- UI is egui 0.36; Compositing UI tests use egui_kittest, Library/Develop uses its own `headless.rs` harness. Add tests
  that actually click/drag the UI you change.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows. Don't touch the owner's files
  outside this worktree (read-only access to reference data is fine).
- Old settings/documents must still load (serde defaults). The owner does not need old edits to render identically.
- **Do not commit.** Leave changes in the working tree. When done, write the report file named in your task: what's
  done per point, tests with counts, GPU tests added, anything left and why.

