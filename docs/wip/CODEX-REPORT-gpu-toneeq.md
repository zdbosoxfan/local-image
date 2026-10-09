# Remaining Develop GPU stages — 2026-10-09

Continued the existing branch's native primary, lens-database and capture implementation.
No commits, new dependencies, unsafe Rust, C/C++ code or settings-format changes.

## Work completed

1. **Tone equalizer:** device-compensated Euclidean luminance, faithful quantized two-iteration
   EIGF with the final geometric blend, log mask and cached spatial preparation. Reuses the
   existing interpolation, Deriche Gaussian, moment bounds and covariance kernels. The CPU's
   fitted gain LUT is metadata; interpolation/gain runs at the reference finish-stage position.
   Neutral/non-neutral Show Mask uses native posterization and the existing overlay drawing.
   Removed the whole-render tone fallback.
2. **Layer tools:** native per-stage alpha blends for layer NR, scene dehaze/WB/exposure,
   radius-specific sharpening, contrast tone, Point Color, vignette, working/channel curves
   with saturation refinement, and encoded grain. Existing primary layer kernels continue to
   handle H/S, whites/blacks, clarity/detail and primary colour tools. Removed the renderer and
   engine's blanket layer rejection. Layer order, amount, opacity, inversion and visibility
   retain the reference's semantics.
3. **NR statistics:** 4×4 tiles and pooled finite 2×2 high-pass samples, exact radix median
   selection, lower tile quartile, device VST parameter generation and recursive per-band sum
   reductions/threshold generation. No NR image/statistics readback remains. GPU sum trees are
   f32; the CPU's f64 accumulation and all CPU image algorithms/goldens remain unchanged.
4. **Linear stage:** native nonlinear Bradford, gamut compression and clipping after WB;
   resolved reference matrices/constants are shared. Stored AI RGBA patches upload once per
   preparation, minify with premultiplied integer averaging when needed, bilinearly sample and
   composite before WB. Missing patches are skipped. Other linear-stage host tools retain their
   existing fallback.
5. **Depth masks:** decoded stored logits upload as model data; transformed cell-centred
   sampling, sigmoid and feathered band evaluation run on device, including missing/damaged
   grids and component/mask inversion/composition.

Also moved **dehaze airlight/depth statistics** to device, since the full toolset benchmark
includes dehaze. The reference's order-sensitive median-of-three selection and reversed
first-half brightness-sample order are preserved, followed by native reductions. Selection
uses serial device scratch; its full-size RTX latency needs measurement. Prepared haze
fields remain cached. This is independent of typical edits, which have dehaze off.

Added `last_cpu_stages()` to report the host pixel/statistical stages actually executed on
that render thread. The benchmark now uses it rather than obsolete settings predicates.
Geometry/linear/unsupported-mask/source-analysis/hidden-overlay/device-limit haze host paths
are still recorded. Limits, allocation/device failure, blank/incomplete work, deep output and
proof fallbacks remain intact. Source/mask metadata resolution and the output histogram are
ordinary host orchestration, not ported image stages.

Updated `docs/GPU-VALIDATION.md`, preserving historical RTX measurements as historical.

## Verification

All cargo builds ran offline with Rust 1.98.1 and `CARGO_BUILD_JOBS=3`, one build at a time.
The required command passed with `RUST_TEST_THREADS=1`:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-develop -p lightcraft-ui-egui
```

| Crate | Passed | Ignored | Failed |
| --- | ---: | ---: | ---: |
| lightcraft-gpu | 80 | 4 | 0 |
| lightcraft-pipeline | 201 | 3 | 0 |
| lightcraft-engine | 368 | 2 | 0 |
| lightcraft-develop | 29 | 0 | 0 |
| lightcraft-ui-egui | 178 | 1 | 0 |
| Total | 856 | 10 | 0 |

GPU counts comprise 32 unit and 48 integration tests. The six new numerical tests executed
on test-only software Vulkan, as did the existing primary numerical coverage. Production
adapter-dependent tests return with their skip note here; their pass counts do not imply
RTX execution. All five doc-test suites passed (zero doc tests).

Formatting for the three changed crates and `git diff --check` passed. Clippy passed for
all changed crates and all targets with warnings denied:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-gpu -p lightcraft-pipeline -p lightcraft-engine --all-targets -- -D warnings
```

After the lint cleanup, reran `RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu`
with the same PATH/jobs settings: 80 passed, 4 ignored, zero failures; all six new numerical
tests executed again. Final formatting and whitespace checks passed.

## Added and converted GPU coverage

- `src/remaining_tests.rs`: six numerical tests using the existing test-only software Vulkan
  device. Tone equalizer tiny/odd/extreme/masked/cached renders and overlay; robust NR model
  and reconstruction; calibration/patch minification/depth; mixed overlapping layer ordering,
  opacity, NR/sharpening radii, curve/grain/vignette/dehaze; exact dehaze airlight; negative,
  HDR and zero-luminance calibration across four adaptations, clipping and gamut strengths.
  Native render comparisons also assert that no host stages ran.
- `tests/toolset.rs`: converted the tone fallback test to native parity; added tiny/odd/extreme
  tone-mask cache sweeps, layered tool ordering/opacity/cache/geometry, and a 2053×2049
  tone-equalizer + depth + curve comparison crossing finish row bands.
- `tests/equivalence.rs`: converted EIGF tone-equalizer/preview fallback assertions to parity.
- `tests/fallback.rs`: converted the layer-curve rejection to native mean/max image bounds.
- `lc-engine/src/tests_gpu.rs`: tone equalizer + layer curve + depth and Show Mask through
  `media::develop`, verifying device stage cache use.
- Existing NR, calibration, depth, patch, layer and fallback suites retained; Naga validates
  every new WGSL module without an adapter. Final-image bounds remain mean <0.5 LSB, max ≤3.

## Hardware validation remaining

No production GPU adapter is available in this sandbox. Production device integration tests
skip with a note; software Vulkan numerical tests do run here. Software execution does not
establish RTX equivalence or performance. The coordinator should run:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-develop -p lightcraft-ui-egui
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --release --offline -p lightcraft-gpu --test toolset bench_toolset_24mp -- --ignored --exact --nocapture
```

The ordinary five requested cases have complete device paths. Every full toolset benchmark
row should report GPU on an RTX 5090 that fits its buffers; actual host fallbacks remain
visible on smaller devices. The typical-edit ≤~200 ms target and full-size serial dehaze
selection latency are unmeasured here. No hardware timing claim is made.
