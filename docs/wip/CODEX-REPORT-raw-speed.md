# Raw decode speed report — 2026-10-09

Continued the faithful kernels already in this branch (`88e3543`), without replacing
reference fixtures, changing decode goldens, adding dependencies, or changing public
options/serialization. Changes are uncommitted. All builds were offline with Cargo
1.98.1, `CARGO_BUILD_JOBS=3`, incremental compilation disabled, and one Cargo build
at a time. No GUI was opened or stopped and no photographs were accessed.

## Implementation

* **AMaZE:** parallel 128-row output bands retain the exact 160-pixel tile and
  32-pixel overlap layout, CFA phase, mirror borders, and original shared scratch
  plane lifetimes. Each Rayon job owns its scratch; horizontal tiles reuse it in
  their original order. Workers write disjoint RGB slices directly. Removing the
  four-channel staging image and final RGB copy removes a 384 MB temporary at
  24 MP; parallel scratch adds smaller job-local buffers.
  Crops narrower than 128 pixels retain scalar band order: the original kernel's
  truncated first tiles can carry scratch boundary values into the next band.
  The new baseline tests caught this at 127×129 GRBG; the fallback preserves those
  bits. Tiny-crop reflection remains unchanged.
* **VNG4 and both VNG dual branches:** precomputed phase-specific linear stencils,
  counts and directional neighbor metadata remove repeated CFA lookups. Gradient
  terms retain their order; only the set gradient bits are accumulated. RGB merge
  is parallel. The linear branch consumes its original four-colour image instead
  of cloning it, avoiding a 384 MB clone at 24 MP.
* **Median smoothing:** snapshot R−G and B−G together, reuse scratch for both
  passes, read neighboring row slices, and update both channels in one parallel
  row pass. Green remains untouched. A fixed 19-comparison median network uses
  integer keys with the same `total_cmp` ordering and branchless comparisons,
  retaining the original sort's signed-zero/NaN behavior.
* **Segmentation:** parallel WB conversion and integer clip counting in one pass;
  opposed masks, dilation and expensive reference averages by rows; CFA plane
  initialization by rows; independent colour morphology/segmentation and candidate
  evaluation; candidate rows keep original first-pixel ties through an ordered
  fold. Candidate application partitions three raw rows with the corresponding
  plane row, preserving the last write within every CFA block. Morphology,
  recovery initialization, attenuation and final WB undo use disjoint rows.
  EDT columns use column-major staging and reusable job-local envelope scratch;
  the row pass keeps the same arithmetic and square roots. Box smoothing preserves
  each running sum's order while parallelizing horizontal/vertical lines and
  reusing its buffers. The existing Gaussian smoothing was already parallel.
* Chroma's floating-point accumulation remains serial, with expensive deltas
  evaluated in parallel first (96 MB extra staging at 24 MP). Scanline segment
  numbering, gradient propagation between rings/segments, and per-segment random
  sequences retain their original order. Parallel reductions are limited to
  integers/booleans and finite nonnegative distance maxima.

## Bit fidelity and tests

`crates/lc-raw/tests/raw_speed.rs` stores complete-output fingerprints captured
from the original kernels, then compares every float's `to_bits()` under one and
eight Rayon workers. Archived fingerprints are asserted on their recorded
x86_64 Linux platform; other platforms still run every worker comparison, since
platform math libraries can round transcendental functions differently.
Normal tests cover 335 configurations: 128 demosaic cases,
175 segmentation cases and 32 extra AMaZE padding/narrow/tall cases. Coverage
includes all Bayer phases, shifted X-Trans, negative/HDR samples, signed zero,
7×9 crops, 127/128/129 and 255/257 tile borders, more than eight tile rows,
all seven recovery modes, non-neutral WB, noisy recovery and radius-8 morphology.

The explicit `amaze_24mp_original_bits` test covers four further 6000×4000 Bayer
cases. An additional temporary test directly compared the original AMaZE source
against the optimized kernel at every output float for 36 size/phase cases,
including these four 24 MP cases; it passed and its duplicate source/test modules
were removed. Original copies and the log remain ignored under `target/`.
The median network is checked against the original total-order sort for all
362,880 permutations of values including duplicates, signed zero, infinities
and positive/negative NaNs. Existing fixture assertions/tolerances are unchanged.

Required tests passed on the final code: **451 passed, 0 failed, 6 ignored**: raw unit 99 passed /
3 ignored; raw integration 32 passed / 1 ignored; engine 320 passed / 2 ignored.
The additional 24 MP test was explicitly run and passed (four Bayer phases).
All existing upstream-vector tests and
`tests_toolset::default_raw_loading_matches_colour_tone_goldens` passed unchanged.
Formatting, `git diff --check`, and all-target Clippy with `-D warnings` passed.
The thread/fingerprint integration tests were also rerun after making archived
fingerprint assertions conditional on their recorded platform: 3 passed, 1 ignored.

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 \
  RAYON_NUM_THREADS=8 cargo +1.98.1 test --offline -p lightcraft-raw -p lightcraft-engine
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 \
  cargo +1.98.1 test --offline -p lightcraft-raw --test raw_speed \
  amaze_24mp_original_bits -- --ignored
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
  cargo +1.98.1 fmt -p lightcraft-raw --check
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 \
  CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p lightcraft-raw --all-targets -- -D warnings
```

One preceding suite rerun failed the unchanged PNG smart-preview test
`tests_import::smart_previews_folder_is_chosen_per_library` during temporary-drive
`remove_dir_all`, with `DirectoryNotEmpty`. Two earlier full runs passed it, and
the final complete rerun with eight Rayon workers also passed. No test assertion
was weakened and no engine file was changed; the failure log is retained as
`target/raw-speed-required-cleanup-race.log`.

Logs: `target/raw-speed-required-tests.log`, `target/raw-speed-24mp-bits.log`,
`target/raw-speed-extra-bits.log`, `target/raw-speed-final-thread-tests.log`,
`target/raw-speed-clippy.log`.

## 24 MP timings

AMD Ryzen 7 9800X3D, x86_64 Linux, eight Rayon workers, Rust 1.98.1 release
(thin LTO), unchanged `crates/lc-raw/examples/quality.rs --timings`, 6000×4000
synthetic CFA. The original executable was built before editing the kernels and
saved as `target/raw-speed-quality-before`. After building the optimized example,
three original/optimized pairs ran sequentially, with no Cargo build overlapping
measurements. The table reports each side's median and min–max range in ms.
The initial build-run and the first two exploratory baseline runs are excluded.

| Method | Before median [range], ms | After median [range], ms | Speedup |
|---|---:|---:|---:|
| AMaZE | 2160.887 [1371.366–2218.375] | 468.825 [443.334–512.498] | 4.61× |
| Full VNG4 | 665.191 [617.788–671.957] | 517.952 [455.162–545.028] | 1.28× |
| RCD + VNG-linear + medians | 1260.292 [1227.905–1487.986] | 736.444 [627.142–754.735] | 1.71× |
| AMaZE + VNG-linear + medians | 2863.000 [2853.275–2934.429] | 1110.552 [971.676–1217.293] | 2.58× |
| Segmentation, full CFA | 1676.275 [1468.321–1693.632] | 494.936 [494.641–508.850] | 3.39× |
| Segmentation preview, k=2 | 1612.041 [1544.830–1899.591] | 670.424 [641.359–760.899] | 2.40× |
| Segmentation preview, k=4 | 500.914 [433.841–582.487] | 231.623 [200.520–267.421] | 2.16× |
| Segmentation preview, k=6 | 294.707 [265.674–319.628] | 112.956 [105.832–152.686] | 2.61× |
| Segmentation preview, k=8 | 166.692 [162.526–182.749] | 80.240 [79.313–137.414] | 2.08× |

Unmodified RCD control: **114.874 → 123.008 ms** median. The shared machine's load
varied during measurements; ranges expose that variation. All quality and preview
output lines were identical across all six runs. Demosaic times exclude parsing,
normalization and later rendering; full-CFA segmentation excludes normalization
and demosaic. Preview timings include normalization, CFA reconstruction and binning.

Build/reproduce the optimized example:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 \
  RAYON_NUM_THREADS=8 cargo +1.98.1 run --offline --release \
  -p lightcraft-raw --example quality -- --timings
```

Paired runs used the pre-edit executable and that newly built executable:

```sh
for raw_speed_run in 1 2 3; do
  RAYON_NUM_THREADS=8 target/raw-speed-quality-before --timings \
    > "target/raw-speed-before-paired-${raw_speed_run}.log" 2>&1
  RAYON_NUM_THREADS=8 target/release/examples/quality --timings \
    > "target/raw-speed-after-paired-${raw_speed_run}.log" 2>&1
done
```

Full logs are `target/raw-speed-{before,after}-paired-{1,2,3}.log`; parsed timings
are `target/raw-speed-timing-summary.json`. The example's source and fixtures
were not changed.

## GPU and remaining work

No GPU code or new GPU tests were added: these kernels are CPU CFA processing
before the existing CPU/GPU renderer boundary. The existing engine test
`tests_gpu::new_cfa_methods_and_segmentation_render_on_the_gpu` covers VNG4,
RCD/VNG, AMaZE and AMaZE/VNG × default/segmentation highlights × full/binned
preview (16 cases). It was rerun explicitly with `--nocapture` and passed with
`skipped: no GPU adapter`; llvmpipe was recognized as a software adapter and
skipped. None of the 16 numerical GPU cases ran here. The coordinator should run
that existing test on the RTX 5090 and check that it does not skip. Log:
`target/raw-speed-gpu.log`.

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 \
  RAYON_NUM_THREADS=8 cargo +1.98.1 test --offline -p lightcraft-engine \
  new_cfa_methods_and_segmentation_render_on_the_gpu -- --nocapture
```

No requested implementation work remains. The scalar narrow-crop fallback and
ordered accumulations/propagation/noise are intentional requirements for unchanged
output bits, rather than deferred parallel work.
