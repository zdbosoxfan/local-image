# GPU capture sharpening — 2026-10-09

Changes are uncommitted in `wip/codex-gpu-capture`. The existing renderer/filter
infrastructure and the earlier builder's work were retained. No dependencies,
unsafe Rust, C/C++ build code, GUI windows or writes to owner files were added.
Cargo used Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, one build at a time.

## 1. Native device stage and cache

Removed the GPU renderer's CPU `presource` call. New `lc-gpu/src/capture.rs` and
`src/wgsl/capture.wgsl` implement Rec.2020 luminance, the black/clipped-neighbour
exclusion, 21-pixel coefficient-of-variation sigmoid mask, mask smoothing/mix,
per-pixel corner-boost/border-taper kernel indices, Richardson–Lucy division and
multiplication, and the final luminance-ratio RGB gain. Source-scale capture runs
before resampling and white balance. Parameters come directly from the existing
`lightcraft_pipeline::capture_params`, including automatic sensor radius and
threshold, sensor scale, section enable state and the 1–25 slider → 1–50 iteration
mapping. Small images and subpixel no-op radii keep the reference behavior.

The 256 quarter PSFs are generated once on the host using the CPU's exact f32
table construction (25 KiB). All image-sized work runs on the GPU. RL uses true
sampled Gaussian 5x5/9x9 disc kernels, not separable Gaussian approximations or
Detail's small-sigma variance-adjusted taps. Each 16x16 workgroup loads a 24x24
shared tile including the 4px halo; every lane participates before bounds returns.
Estimate and ratio buffers never alias their outputs. Submissions are bounded by
the existing invocation limit and flushed at iteration boundaries.

`GpuStages.source` holds the original upload and the last sharpened device source,
keyed by source Arc identity and the complete resolved capture parameters. Other
tools and output sizes reuse capture. Capture/metadata changes invalidate sampled,
linear and spatial stages as well as the capture result; the original upload is
retained. Both source buffers are counted once in `GpuStages.bytes` and released
by cache clearing. Results are submitted before publication to another thread.
The existing `capture_sharpening_changes_re_upload_the_source` retains its original
stale-source regression meaning, while re-uploading original pixels is unnecessary.

CPU-only lens geometry reads back and resamples the GPU-sharpened source. Automatic
Upright without a stored transform and pupil detection also inspect sharpened
pixels, preserving the reference's planning order. Ordinary renders have no
source readback. If the full capture source exceeds the device storage limit,
the render returns `None` with a limit reason for the normal CPU fallback; it does
not run CPU capture inside a successful GPU render.

## 2. CPU reference and numerical accuracy

`lc-pipeline` code, CPU behavior and output goldens are unchanged. The port uses
the reference's row-major RL tap additions and local-variance addition order.
Mask smoothing reuses `blur.wgsl` with whole-row horizontal sums and 32-row
vertical restart bands, matching the CPU's running-sum order. A race-free gather
replaces the CPU mask's scatter of zeros without changing its neighbourhood.

Known f32 differences: WGSL sqrt of squared offsets replaces Rust hypot for corner
distance; device exp/log/division and compiler multiply-add contraction may differ
by ulps. A residual division correction protects kernel-index truncation against
an approximate reciprocal and is also used for iterative ratios/final gain.
The GPU test allows at most one adjacent sigma index at floating boundaries, but
final RGB comparisons always use the complete CPU reference and unchanged
mean |delta| <0.5 LSB / maximum <=3 LSB bounds.

Adapter-independent unit tests transcribe the WGSL arithmetic in f32. They compare
12 masks and 48 output/estimate checkpoints (1/2/8/50 iterations) against the CPU,
including tiny/odd dimensions, black/clipped pixels and corner-centre parameters.
The intermediate CPU estimate is recovered from the reference's known blend and
RGB gain on strong-mask pixels. A second test compares 4096 estimates after eight
RL iterations against the existing independent upstream fixture: maximum absolute
difference **0.000001132**, below its 0.000003 bound. It checks all 256 kernels'
mass and the true small-sigma response. All assembled WGSL passes Naga parsing and
validation without an adapter.

Measured scalar maxima over the 12 masks / 48 checkpoints: mask **0**, RGB **0**,
recovered intermediate estimate **0.000000417** (the inverse introduces its own
RGB/luminance rounding). These are f32 CPU transcriptions, not GPU measurements.

The device intermediate test checks luminance, blend, indices and both ratio and
estimate after every iteration, with and without clipping. The device cache test
asserts buffer identity reuse, parameter/source invalidation, memory accounting
and clear behavior. Test-only device-init synchronization fixes an existing
crash-marker test race exposed by the new unit tests; production marker behavior
is unchanged.

## 3. GPU integration tests and benchmark

Six added `toolset.rs` tests provide **76** bounded CPU/GPU comparisons:

| Test | Comparisons / coverage |
| --- | --- |
| `capture_sharpening_parameter_sweeps_match` | 28: radius including 0.25 cutoff, 0.65/0.66 boundary and sigma-table cap; threshold, iterations and corner boost endpoints |
| `capture_sharpening_binned_previews_edges_and_borders_match` | 30: five tiny/odd sizes, three sensor scales, full/preview output; HDR/black/negative edges and borders |
| `capture_sharpening_source_info_and_geometry_changes_invalidate_cache` | 11: auto radius/threshold, sensor scale, exposure, source identity, raw/rendered, section off/on and CPU-only lens geometry |
| `capture_sharpening_reuses_device_source_for_other_tools_and_sizes` | 4: exposure/clarity/size edits, no CPU `StageCache.pre`, memory release |
| `capture_sharpening_over_limit_source_falls_back_without_device_failure` | 1 after the source-limit refusal; device remains usable |
| `capture_sharpening_before_source_analysis_matches` | 2: pupil detection and unresolved automatic Upright |

The existing `capture_sharpening_matches` and
`capture_sharpening_changes_re_upload_the_source` remain, including cached == fresh,
radius/threshold/iterations/corner/off/on changes, binned preview and rendered
source behavior. There are **87** bounded capture comparisons including those.

Added ignored `bench_capture_sharpening_24mp`: 6000x4000, radius 0.8, threshold 25,
8 iterations, corner boost 30. It prints fresh GPU vs CPU ms, a cached exposure
edit and a radius change retaining the upload. The existing ignored
`bench_toolset_24mp` capture row now identifies capture as GPU work. Device timing
and parity remain unmeasured here because there is no hardware adapter.

## 4. Documentation and licensing

`docs/GPU-VALIDATION.md` describes native capture and marks old CPU-hybrid timings
as historical. Capture was removed from the unbuilt-kernel candidates.
`docs/PORTS.md` and `licenses/darktable-NOTICE.md` credit the new host/WGSL files
to darktable capture.c at `733bd69f32cac7ff5e41025115942772add1f088`, GPL-3.0-or-later,
Copyright (C) 2025–2026 darktable developers; Ingo Weyrich / RawTherapee algorithm.

## 5. Sandbox validation

Final regression run: **234 passed, 5 ignored, zero failures**. GPU tests print
**“skipped: no GPU adapter”** and pass without device execution. No claim of RTX
parity or speedup is made from those skips.

| Suite | Passed | Ignored |
| --- | ---: | ---: |
| lightcraft-gpu unit | 15 | 1 |
| GPU equivalence | 13 | 0 |
| GPU fallback | 3 | 0 |
| GPU memory | 1 | 0 |
| GPU toolset | 22 | 2 |
| lightcraft-pipeline (unchanged reference / goldens) | 180 | 2 |

The new ignored benchmark ran separately: **1 passed**, CPU **9759.2 ms** for the
full 24 MP pipeline with capture. This is one optimized development/test-profile
measurement with shared machine load, not a release/RTX timing or isolated stage
time. GPU columns were unavailable. The release command below is the comparison
to run on hardware.

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline -p lightcraft-gpu -p lightcraft-pipeline -- --nocapture
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 fmt -p lightcraft-gpu
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-gpu --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline -p lightcraft-gpu --test toolset bench_capture_sharpening_24mp -- --ignored --exact --nocapture
```

Strict Clippy, formatting and `git diff --check` pass. The GPU package also passed
again after the final tiny-fixture guard correction (54 passed, 3 ignored); tiny
fixtures are constructed even on no-adapter runs. Final ignored logs:
`target/gpu-capture-tests.log`, `target/gpu-capture-tests-final.log`, `target/gpu-capture-clippy.log`,
`target/gpu-capture-bench.log`. No reference fixture or CPU golden was changed.

## Exact RTX 5090 validation commands

Run one Cargo invocation at a time. Confirm the adapter output names the RTX 5090
and that no device test says “skipped”. These commands cover both new device unit
tests, every capture integration test, existing GPU regression suites and timing:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3

cargo +1.98.1 test --offline -p lightcraft-gpu --lib gpu_capture_intermediate_planes_match_cpu -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu --lib capture_source_cache_reuses_upload_and_invalidates_parameters -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu --test toolset capture_sharpening_ -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu
cargo +1.98.1 test --offline --release -p lightcraft-gpu --test toolset bench_capture_sharpening_24mp -- --ignored --exact --nocapture
```

The `capture_sharpening_` filter runs exactly these eight non-ignored tests:

1. `capture_sharpening_matches`
2. `capture_sharpening_changes_re_upload_the_source`
3. `capture_sharpening_parameter_sweeps_match`
4. `capture_sharpening_binned_previews_edges_and_borders_match`
5. `capture_sharpening_source_info_and_geometry_changes_invalidate_cache`
6. `capture_sharpening_reuses_device_source_for_other_tools_and_sizes`
7. `capture_sharpening_over_limit_source_falls_back_without_device_failure`
8. `capture_sharpening_before_source_analysis_matches`

For the historical whole-toolset timing table as well:

```sh
cargo +1.98.1 test --offline --release -p lightcraft-gpu --test toolset bench_toolset_24mp -- --ignored --exact --nocapture
```

Remaining external validation: those RTX device comparisons and release timings.
All implementation, sandbox-verifiable work and reporting are complete.
