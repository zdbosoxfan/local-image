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
initially used serial device scratch, superseded by the 5090 follow-up below. Prepared
haze fields remain cached. This is independent of typical edits, which have dehaze off.

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
visible on smaller devices. The coordinator measured typical edits and the initial
dehaze regression; see the 5090 follow-up below. No sandbox hardware timing claim is made.

## 5090 follow-up

Coordinator baseline at `cad2cc4`, NVIDIA 615.71.09/Vulkan with serial tests: 855 passed,
one failure; every benchmark row reported GPU. Typical edit was 257 ms under heavy load.
The 641×427 extreme tone fixture had mean 0.0007 LSB / max 25 LSB. Dehaze was 3354 ms,
versus the earlier 406–549 ms. These are coordinator measurements, not sandbox timings.

### Tone-equalizer root cause and fix

The discontinuity was `exp2(floor(log2(luminance)))` in quantized EIGF guidance. The
[WGSL accuracy rules](https://www.w3.org/TR/WGSL/#floating-point-accuracy) permit three ULP
error in `log2` outside [0.5, 2]. An error across an integer boundary changes a guidance
value by a factor of two. The covariance normalization then amplifies that isolated bin
change; it is not an accumulated 25-LSB arithmetic error in the finish LUT.

A controlled software-Vulkan probe of the old quantizer moved its returned negative
`log2` values down three ULP, within this accuracy allowance. It reproduced max **25 LSB**
on the exact 641×427 fixture (mean 0.00027036 LSB). The intermediate dump localized the
first guidance discrepancy to `(333, 52)` during the large-radius case (sigma 80.125,
epsilon 0.0875), and to downsampled coordinate `(83, 13)`:

| Intermediate | CPU | Old quantizer with precision probe | Fixed quantizer, software Vulkan |
| --- | ---: | ---: | ---: |
| First-iteration guidance at (333, 52) | 0.015625 | 0.0078125 | 0.015625 |
| Resampled guidance at (83, 13) | 0.01324863173 | 0.01068690419 | 0.01324863173 |
| Final filtered luminance at (333, 52) | 0.03152512386 | 0.01244867221 | 0.03152512386 |

Other large final-filter outliers in that controlled probe were `(324, 68)`
(CPU 0.01137301978, probe 0.02288573049), `(339, 132)` (0.03095987998,
0.04172601178), `(330, 100)` (0.03213339299, 0.04086847603) and `(329, 2)`
(0.03227299824, 0.04078136384). For `(333, 52)`, the final CPU/probe float words
were `3d01207d` / `3c4bf584`; the first guidance words were `3c800000` /
`3c000000`. These coordinates and values came from the intermediate trace,
before final display conversion/masking.

The probe is an independent reproduction of the driver-sensitive failure mechanism,
not an RTX capture. Its temporary shader change was removed. The production fix uses
integer comparisons against 17 scalar boundary constants resolved with the CPU's
rounded `log2` semantics. This also preserves the CPU's rounding for the few floats just
below exact powers of two; simply extracting the exponent would get those ties wrong.
Guidance selection no longer evaluates a driver transcendental. The mean/max bounds
remain unchanged, and CPU image algorithms remain unchanged.

Added `LC_TONEEQ_TRACE=1`: prints coordinates, channel, value and float bits for the largest
CPU/GPU differences in compensated luminance, downsample, quantized/resampled guidance,
moments, variance/covariance and each iteration's output. The CPU trace starts from the
same uploaded linear RGB, isolating this stage. The final-image tests now print all
outlier pixel coordinates (up to 32) and CPU/GPU RGBA. Diagnostic readbacks occur only
when tracing is explicitly requested; they are absent from ordinary rendering/timing.

### Dehaze performance fix

Full-image scalar quick selection was the regression. Large selections now perform
eight exact parallel Hoare partitions: stable left/right stop lists, hierarchical integer
prefix scans, disjoint pair swaps, and constant-size pivot/interval commits. Pair ordering
and the reference's unusual first right-scan inclusion of the pivot are preserved,
including repeated dark-channel minima. A final uncapped reference selector handles the
remaining interval; source buffers of at most 65536 elements use that selector directly.
The full-image brightness prefix is now hierarchical too. There is no statistics
readback, CPU percentile, approximate rank, or per-tile full-image selection.

RGB guide means and six covariances are now prepared once for both positive/negative
transmission fields. This reduces guide/input statistic channels from 26 to 17 per pixel
(about 35% less moment generation/box filtering), retaining the same Kahan filters and
covariance solve. Prepared haze fields remain cached.

Added production-device-first tests (software Vulkan fallback in this sandbox): 2709
power-boundary inputs checked bit-for-bit against CPU quantization; the exact 641×427
fixture at both sizes with extreme/cache edits; and 15 large selector cases covering
sorted/reversed inputs, plateaus, constants, negative values and random ties, checked
bit-for-bit against the order-sensitive reference. Seven additional brightness cases
cover empty, tiny, partial and full buffers, with unused storage poisoned by NaNs.
Existing strict airlight and both-sign final-image tests are retained. Naga validates
the new partition/scan module.

The required serial offline five-crate run passed: **859 passed, 10 ignored,
0 failed** (Develop 29; Engine 368 + 2 ignored; GPU 83 + 4 ignored; Pipeline
201 + 3 ignored; UI 178 + 1 ignored). Production-device integration tests skip
in this sandbox; the software-Vulkan numerical tests execute. GPU all-targets
clippy passed with `-D warnings` using `target/clippy`. The final GPU rerun also
passed: **83 passed, 4 ignored, 0 failed**, including the seven additional
brightness-buffer cases. GPU formatting and `git diff --check` pass.
The final opt-in trace run passed the six fixture renders, with zero guidance-bin
differences across all 12 EIGF iterations on software Vulkan.

Follow-up verification commands:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-develop -p lightcraft-ui-egui
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-gpu --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 LC_TONEEQ_TRACE=1 cargo +1.98.1 test --offline -p lightcraft-gpu --lib render::remaining_tests::native_toneeq_5090_extreme_fixture_matches -- --exact --nocapture
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 fmt -p lightcraft-gpu -- --check
git diff --check
```

Actual RTX follow-up latency/equivalence is still pending; ≤300 ms dehaze is a target,
not a measured claim in this sandbox. An exact-row benchmark filter is available:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 LC_TONEEQ_TRACE=1 cargo +1.98.1 test --offline -p lightcraft-gpu --test toolset tone_equalizer_extremes_masks_tiny_odd_and_cached_edits_match -- --exact --nocapture
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 LC_TOOLSET_BENCH_ROW='detail dehaze' cargo +1.98.1 test --release --offline -p lightcraft-gpu --test toolset bench_toolset_24mp -- --ignored --exact --nocapture
```

Run the original full equivalence suite and full benchmark after these focused checks.
Leave `LC_TONEEQ_TRACE` unset for performance measurements. Changes are uncommitted;
no index writes, dependencies, settings changes, unsafe or owner-file edits were made.
