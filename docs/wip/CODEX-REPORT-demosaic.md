# Demosaic implementation report

Implementation complete; changes left uncommitted. Validation status below follows the owner's
latest instruction: **no further full suites, GPU acceptance runs or benchmarks in this worktree**.
The merged coordinator owns the remaining acceptance measurements.

## Implemented

- X-Trans: safe Rust port of darktable **Markesteijn three-pass**, eight directional candidates,
  green refinement, YPbPr homogeneity voting and upstream 122-pixel working tiles. All 36 CFA
  translations are supported. Small inputs and other CFA layouts retain generic interpolation.
- X-Trans dual: Markesteijn in detail, upstream three-colour VNG-linear with two median colour
  smoothing passes in flat areas, using the existing pinned darktable detail mask. Threshold zero
  returns the high-detail result exactly. The old edge-weighted algorithm remains an explicit
  **X-Trans fast** override. Existing reduced-sensor previews and thumbnails remain available.
- Bayer: **LMMSE** with three median passes and **DCB** with three correction iterations and
  standard final chroma interpolation, alongside the existing AMaZE/RCD/VNG/dual methods.
  LMMSE retains upstream's 136-pixel working tile; its mirrored halo is 16 pixels rather than
  upstream's 12, covering the complete filtering/colour/median stencil. HDR scaling retains
  unclipped scene-linear samples. EECI refinement and DCB's optional enhancement are not exposed.
- Automatic Bayer choice: measured normalized CFA noise selects **RCD below sigma 0.0035** and
  **dual RCD/VNG above**, with dual threshold 0.2. Noise is measured from same-colour second
  differences across all Bayer phases, using block MAD and a low quartile to reject texture.
  This responds to actual sensor noise instead of assuming one ISO threshold fits every camera.
  Explicit overrides remain authoritative. Automatic X-Trans uses Markesteijn three-pass.
- CPU processing uses disjoint bands and the existing global Rayon pool. No production private
  pools, unsafe code, new dependencies, C/-sys build dependencies, or Cargo.lock changes.
- Develop continues to demosaic **before** the CPU/GPU renderer split. WGSL receives RGB; there
  is no Develop WGSL CFA kernel to port in this branch. Both backends consume the new CPU result.
- Existing shared Raw Processing widgets expose all methods. A two-column method grid keeps
  names readable. The added headless test clicks the actual shared controls at the design-system
  panel width. No new colours, icons, spacing metrics or custom controls were introduced.
- Old method names and enum ordering are retained; old settings still deserialize. Automatic
  rendering intentionally changes. Legacy AHD colour/tone goldens remain unchanged and are now
  checked with explicit AHD; Automatic has a separate selection-equivalence regression.
- PORTS, upstream notices, original DCB BSD terms, generated app attributions and the pending
  translation list are updated. No Match Look files or owner photos were touched.

This extends the branch's existing AMaZE/RCD/VNG/dual kernels and scalar fixture workflow.
At entry, the only uncommitted file was the owner's CODEX-TASK.md edit; it is preserved.
The referenced RAW-DECODER-COMPARISON.md was absent; RAW-DECODERS.md and the checked decoder
contract supplied the decoder boundary/context instead.

## Measured quality — completed before the pause

All measurements use procedural data or SHA-256-verified CC0 raw.pixls.us data under target/.
The eight natural crops come from two CC0 camera files (Nikon D750 898 and Fujifilm X-T20 1178),
exported as linear Rec.2020 and re-mosaiced. They are eight crops, not eight independent scenes.
CPSNR uses peak 1 and excludes a 24-pixel boundary; aggregate values pool MSE, not average dB.

| Natural X-Trans, injected normalized noise sigma | Old edge-weighted | Markesteijn 3 | Dual Markesteijn |
| --- | ---: | ---: | ---: |
| 0 | 37.034 dB | **45.293 dB** | 44.920 dB |
| 0.005 | 36.556 | 42.764 | **42.822** |
| 0.02 | 32.499 | 34.638 | **34.837** |
| 0.08 | 22.188 | 23.726 | **23.762** |

Clean natural-image gain: **8.259 dB** over the old method. Unmodified rawler 0.8.0's Rust
three-pass reference scores **45.231 dB** on the same clean crops. It is used only in the
reference example. Its CFA arithmetic overflows in debug builds; the comparison used release.

Synthetic 256x256 regression charts: old/new CPSNR is **14.124/28.691 dB** on the zone plate
and **23.112/39.600 dB** on the Siemens star. The sharp colour-only chart is a real counterexample:
**30.109/25.489 dB**. Markesteijn is not universally superior; the fast override is retained.
The cached 200% crops visibly remove most false-colour rings/spokes; residual centre aliasing
and colour-edge zippering remain visible. Evidence:
`target/demosaic-quality/xtrans-moire-zipper-crops.png`.

| Natural Bayer sigma | AHD | RCD | Dual RCD/VNG | LMMSE | DCB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 44.159 | **44.766** | 44.257 | 44.016 | 42.790 |
| 0.002 | 43.736 | **44.346** | 44.015 | 43.545 | 42.513 |
| 0.005 | 41.971 | 42.743 | **42.799** | 41.778 | 41.364 |
| 0.01 | 38.580 | 39.707 | **40.031** | 38.591 | 38.903 |
| 0.02 | 33.599 | 35.169 | **35.489** | 34.027 | 34.764 |
| 0.08 | 21.988 | 24.259 | **24.323** | 23.065 | 24.035 |

The observed RCD/dual crossover bracket is [0.002, 0.005]; 0.0035 is its midpoint. LMMSE wins
strongly on the frequency charts (e.g. clean 384x384 zone plate **36.549 dB**, AMaZE 25.714),
but did not win these natural-image curves. DCB also did not win them. The automatic rule follows
those measurements rather than forcing a presumed high-ISO winner. Real camera-profile curves
are prepared but not yet run; the coordinator should revisit the threshold with those results.
Texture can bias the noise estimate. The clean references are themselves demosaiced exports,
which can favour their source interpolation method; these limited curves are not independent
ground truth or a universal camera calibration.

Controlled **darktable-cli 5.6.2** comparison uses identical quantized sensor input, neutral WB,
linear Rec.709 output, disabled highlights/OpenCL and isolated library/config directories.
It isolates demosaic from tone/profile/lens differences:

- Markesteijn: **123.201 dB CPSNR**, maximum absolute error **8.762e-6**. Old directional:
  **14.597 dB** against the same output.
- LMMSE: **61.867 dB**, maximum **0.083092**. Sparse differences occur near upstream tile seams;
  our complete mirrored halo differs from upstream's zero/PPG border and shorter overlap.
  On the same zone-plate truth, upstream is approximately **36.537 dB**, ours **36.549 dB**.
  This is a documented algorithm-boundary difference, not bit-parity claimed from an average metric.

Pinned scalar fixtures are stronger implementation checks: Markesteijn is **bit-exact** over
103,788 RGB values across 36 phases and tile boundaries; DCB is bit-exact over all four Bayer
phases; LMMSE maximum difference is **4.17e-7**. Earlier fixtures are unchanged. Sources, shims,
sampling coordinates, hashes and update procedure are in tests/fixtures/README-demosaic.md.

## Timing and memory — completed before the pause

Release CPU runs on the shared owner's machine, using the global pool; these are single-run
observations under contention, not guaranteed latency or hardware-specific tuning. Working-tile
constants follow the pinned upstream header. The same code was measured with two Rayon threads.

| Synthetic source/method | 24 MP ms/MP | 60.2 MP ms/MP | 24 MP, two threads ms/MP |
| --- | ---: | ---: | ---: |
| Bayer Automatic | 47.285 | 35.715 | 132.241 |
| Bayer LMMSE | 73.253 | 66.985 | 325.245 |
| Bayer DCB | 30.549 | 23.744 | 179.095 |
| X-Trans old directional | 11.420 | 8.126 | 65.247 |
| X-Trans Markesteijn 3 | 47.379 | 48.106 | 274.985 |
| X-Trans dual | 84.079 | 90.968 | 277.247 |

Process peak RSS across these method runs: **1.079 GiB at 24 MP**, **2.726 GiB at 60.2 MP**,
**1.077 GiB with two threads at 24 MP**. On the actual 24.3 MP CC0 sensors, Automatic measured
45.327 ms/MP for the RAF and 16.442 ms/MP for the NEF. Variability between repeated equivalent
methods in the retained logs demonstrates host contention. Interactive app-path timing remains
for the merged coordinator; no interactive-speed claim is made from these kernel numbers.

## Checks and tests

After the owner's latest testing rule:

- `cargo +1.98.1 check --offline` on raw/develop/engine/ui: pass.
- Strict Clippy, `--offline --all-targets -- -D warnings`, on each of those four crates: pass.
- Filtered new headless test `demosaic_new_overrides_click_shared_raw_controls`: **1 passed**;
  clicks LMMSE, DCB, Markesteijn, dual X-Trans and Automatic at the token panel width.
- Python reference scripts compile. No further full suite, GPU acceptance or benchmark was run.

Earlier results, retained for audit rather than rerun:

| Check | Result |
| --- | --- |
| Six new raw tests (`demosaic::tests_new`) | 6 passed |
| Raw full suite | 236 passed, 9 ignored |
| Develop unit suite, including new settings compatibility test | 39 passed |
| Develop coverage repair (`demosaic_` filter) | 5 passed; two outdated nine-method assertions corrected |
| Engine full suite | 898 passed, 6 ignored |
| New GPU parity test | 1 passed, six Bayer/X-Trans cases on **RTX 5090 Vulkan**, max 1 LSB, mean printed 0.0000 |
| UI full suite | interrupted by owner pause; no completion claim and no restart |

New raw tests cover 36 X-Trans phases, four Bayer phases, preservation of known samples,
constant colours, tiny inputs, single-pixel final bands, tile boundaries, dual threshold zero,
measured noise selection and exact one/four-thread determinism. Added engine tests cover
Automatic/manual selection equality and the shared CFA -> CPU/GPU render path. Old method
JSON keys and new overrides round-trip. Initial fixture/lint/coverage failure logs are retained
under target/refvec/demosaic/; fixes did not relax existing numerical fixtures or lint policy.

## Coordinator handoff / deferred acceptance

No implementation point is left as a stub. Per the latest owner rule, these measurements await
merged acceptance:

1. Real-profile shot/read noise curves, to confirm/revise Automatic's crossover. The whole
   upstream noiseprofiles.json is retained unmodified and pinned; `scripts/demosaic_profiles.py`
   prepares ISO 100–25600 D750 coefficients. The Rust example's `--profiles` mode implements
   signal-dependent Gaussian shot/read noise with variance `a[channel]*signal+b[channel]`.
2. Controlled CLI comparison on **ten SHA-256-pinned CC0 files**: GFX 50S compressed/uncompressed,
   X-H2 three modes, X-H2S two modes, X-T5 three modes. All ten were downloaded and verified
   before the pause. The `--reference-raw` mode preserves a centre crop's actual CFA phase;
   `scripts/demosaic_corpus_compare.py` compares Markesteijn/dual and Bayer RCD/LMMSE without
   confounding WB/profile/tone differences. It has not been executed.
3. Interactive speed through the existing `lightcraft-engine` render_bench app path, GPU acceptance
   and final merged suites. The latest owner expressly deferred them.

Commands for the authorized coordinator (not executed after the pause):

```sh
python3 scripts/demosaic_profiles.py
cargo +1.98.1 run --offline --release -p lightcraft-raw --example demosaic_quality -- --profiles target/refvec/demosaic/d750-noise.tsv target/demosaic-quality/natural-0.f32 target/demosaic-quality/natural-1.f32 target/demosaic-quality/natural-2.f32 target/demosaic-quality/natural-3.f32 target/demosaic-quality/natural-4.f32 target/demosaic-quality/natural-5.f32 target/demosaic-quality/natural-6.f32 target/demosaic-quality/natural-7.f32
python3 scripts/demosaic_samples.py
cargo +1.98.1 run --offline --release -p lightcraft-raw --example demosaic_quality -- --reference-raw target/demosaic-corpus/1435.RAF target/demosaic-corpus/1436.RAF target/demosaic-corpus/6001.RAF target/demosaic-corpus/6002.RAF target/demosaic-corpus/6003.RAF target/demosaic-corpus/6007.RAF target/demosaic-corpus/6008.RAF target/demosaic-corpus/6122.RAF target/demosaic-corpus/6123.RAF target/demosaic-corpus/6124.RAF
python3 scripts/demosaic_corpus_compare.py
N=3 ONLY=drag cargo +1.98.1 run --offline -p lightcraft-engine --example render_bench -- target/rawdecode/1178.RAF
```

Logs/CSVs/reference exports remain in target/refvec/demosaic/ and target/demosaic-quality/.
No cache sweeper, Cargo concurrency change, owner app shutdown, GUI window or Desktop access occurred.

## Provenance and outside changes

Followed existing ports/fixtures and these primary sources:

- [Pinned darktable xtrans.c](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/xtrans.c),
  [lmmse.c](https://github.com/darktable-org/darktable/blob/733bd69f32cac7ff5e41025115942772add1f088/src/iop/demosaicing/lmmse.c),
  dual.c/vng.c/detail mask, common/darktable.h; GPL-3.0-or-later.
- [Pinned RawTherapee DCB integration](https://github.com/RawTherapee/RawTherapee/blob/5f486d3678b34c74ba0c63571c17babe20935019/rtengine/demosaic_algos.cc),
  original Jacek Gozdz BSD-3-Clause terms reproduced in licenses/; RT integration GPL-3.0-or-later.
- Unmodified crates.io rawler 0.8.0 public Markesteijn API, LGPL-2.1, as a second reference only.
- [Official darktable demosaic manual](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/demosaic/)
  for sensor-specific fallback, dual behavior and LMMSE/refinement tradeoffs. The actual automatic
  selection is our measured rule, not a port of an upstream ISO policy.
- docs/DESIGN-SYSTEM.md and the existing shared widgets/headless harness; threading review rules
  for the global pool, disjoint output and deterministic tests.

Unavoidable changes outside the raw kernel directory are the Develop enum/compatibility tests,
engine dispatch/regressions, the existing UI method grid/headless test, and required attribution,
licence, pending-translation and reference-harness files. No new dependencies or broad UI refactor.
