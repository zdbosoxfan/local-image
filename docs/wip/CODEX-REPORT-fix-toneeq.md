# GPU tone-equalizer precision fix — 2026-10-09

Continued the existing implementation and its integer guidance-bin thresholds.
Only `lightcraft-gpu` is changed. CPU image algorithms, parity bounds, dependencies,
settings and file formats are unchanged. No unsafe Rust, C/C++, commits, index
writes, desktop operations or owner-file changes were made.

## Cause and implementation

The previous fix removed the approximate `log2` from guidance-bin selection, but
the **input to that selection** still depended on driver arithmetic. In a controlled
software-Vulkan probe, replacing the two Gaussian recurrences with explicit FMAs
changed **17 second-iteration guidance bins** in the 641×427 extreme fixture.
The strict image test failed with mean **0.000516367259 LSB, max 13 LSB**. Changed
coordinates included (329, 2), (330, 100), (339, 132), (324, 68) and (353, 289),
overlapping the earlier report's problem coordinates. This independently reproduces
the remaining failure mechanism; it is not a capture of NVIDIA's exact contraction
order or a claim to have reproduced its exact 25-LSB maximum.

[WGSL permits reassociation and fusion](https://www.w3.org/TR/WGSL/#reassociation-and-fusion),
and ordinary `let` bindings do not preserve the CPU's intermediate rounding.
`wgsl/toneeq_round.wgsl` now implements separate binary32 round-to-nearest-even
addition and multiplication with integer significands, guard/sticky bits, and two
32-bit product words. Division and square root use native arithmetic only to seed
a candidate, then correct it with exact integer residual/midpoint comparisons.
Subnormal inputs/results, cancellation, signed zero and finite overflow are covered.
The helpers' input domain is finite operands (plus propagated overflow in addition),
nonzero division denominators and nonnegative square-root inputs, as used by this
filter. No float64 shader feature or adapter-specific setting is required.

The tone-equalizer path uses these operations for compensated Euclidean luminance,
corner-aligned interpolation (including coordinates), Gaussian endpoint/recurrence
arithmetic, moments, variance/covariance, coefficient evaluation and the geometric
blend. The existing CPU-resolved integer bin thresholds remain intact. Other
primary tools explicitly select their existing arithmetic through the kernel flags.
The self-guided moment caller now supplies an explicit zero for the added flag.
Preparation caching and stage order are retained; no ordinary image readback was
added.

Also removed the equal-dimension interpolation shortcut **for this path**. The CPU
still evaluates its divide/multiply coordinates and weights at equal sizes; those
coordinates can round beside an integer. Skipping that arithmetic produced small
float-word differences even on software Vulkan.

The same temporary fused-recurrence probe **passes after the fix**, with zero
differences in the traced guidance, moments, covariance and filtered planes across
all 12 iterations of the six fixture renders. Temporary shader edits were restored
before final verification.

## Added tests and diagnostics

- `primary::tests::native_toneeq_rounding_preserves_bin_boundaries`: a concrete
  multiply/subtract example changes from the −5 EV bin to −6 EV when contracted.
  Then checks **77,120 result words** (15,424 operand triples × five operations)
  against Rust binary32 arithmetic, including ties, cancellation, negative inputs,
  exponent extremes, subnormals and overflow.
- `primary::tests::native_toneeq_eigf_matches_cpu_float_words`: checks compensated
  luminance and the complete two-iteration geometric filter bit-for-bit on a
  71×47 fixture at three sigma/epsilon/compensation combinations, including equal
  dimensions, fractional downsampling and a large radius.
- `ctx::shader_validation::toneeq_wgsl_validates_without_an_adapter`: Naga validation
  of both changed production modules and the test-only arithmetic probe module.
- `render::remaining_tests::native_toneeq_5090_dump` (`--ignored`): shares the exact
  existing 641×427 depth-masked fixture and its six size/edit combinations. Prints
  adapter/driver, settings, final mean/max and the largest 32 differing RGBA pixels,
  then their linear RGB and both iterations' input, guidance, downsample, moments,
  Gaussian averages, covariance and output as decimal values **and float words**.
  Bin mismatches also print CPU/GPU inputs and CPU quantization of the GPU input,
  distinguishing input drift from a selection error. Lower-resolution snapshot
  coordinates are explicitly printed as the lower interpolation corner. The
  historical (333, 52) coordinate is included even when there are no differences.

The ignored dump continues through image mismatches to collect every case; the
ordinary tests retain **mean <0.5 LSB, max ≤3 LSB**, cache equality, native-stage
and device-error assertions. Dump snapshots are scoped to the test thread, require
no environment mutation and are cleared after each render. `LC_TONEEQ_TRACE=1`
continues to work, now with input/bin diagnostics and Gaussian-average tracing.
Its CPU stage trace starts from the same uploaded linear RGB to isolate this filter.

## Verification

All Cargo commands used `PATH="$HOME/.cargo/bin:$PATH"`, Rust **1.98.1**, offline
builds and `CARGO_BUILD_JOBS=3`, one Cargo build at a time. Tests were serial.
Followed the task-specific instruction to run **only tone-equalizer tests and the
new dump**, rather than the generic full-crate test rule.

| Final check | Passed | Ignored | Failed | Execution |
| --- | ---: | ---: | ---: | --- |
| `--lib toneeq` | 5 | 1 | 0 | Four numerical/render tests on llvmpipe; one Naga test |
| `--lib native_power_bin_boundaries_match_cpu_rounded_log` | 1 | 0 | 0 | 2,709 guidance-boundary inputs, llvmpipe |
| `--test it tone_equalizer` | 4 | 0 | 0 | All four explicitly skipped: no production GPU adapter |
| `--lib render::remaining_tests::native_toneeq_5090_dump -- --ignored --exact --nocapture` | 1 | 0 | 0 | Six renders, llvmpipe |

Software device: llvmpipe LLVM 22.1.8, Mesa 26.2.3, Vulkan. The final dump reported
**zero intermediate float-word differences throughout all 12 EIGF iterations**.
Final display output was at most **1 LSB**; the largest mean was
**0.000007307084 LSB**. The full-size large-radius case had mean
**0.000002435695 LSB**, max **1**; its half-size case was identical.

Formatting and `git diff --check` pass. All-targets clippy passes with warnings
denied:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-gpu --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 fmt -p lightcraft-gpu -- --check
git diff --check
```

## Coordinator commands and remaining hardware validation

There is no production GPU adapter in this sandbox. **Actual RTX 5090 parity and
latency remain to be measured**. Integer rounding adds device arithmetic to this
tool; no hardware performance claim is made. No implementation item remains open.
The consolidated integration-test target is `it`, with the `toolset::` module prefix:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu --test it toolset::tone_equalizer_extremes_masks_tiny_odd_and_cached_edits_match -- --exact --nocapture
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu --lib render::remaining_tests::native_toneeq_5090_extreme_fixture_matches -- --exact --nocapture
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu --lib render::remaining_tests::native_toneeq_5090_dump -- --ignored --exact --nocapture > toneeq-5090-dump.txt 2>&1
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 cargo +1.98.1 test --offline -p lightcraft-gpu --lib toneeq -- --nocapture
```

The dump command needs no `LC_TONEEQ_TRACE` setting. Send `toneeq-5090-dump.txt` if
hardware differences remain. Leave tracing unset for hardware timing.
